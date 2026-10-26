import QtQuick
import QtQuick.Controls
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import qs.Commons
import qs.Ui
import "Trowel"
import "Trowel/ListMerge.js" as ListMerge
import "Trowel/SettingsModel.js" as SettingsModel
import "Trowel/KeyLockModel.js" as KeyLockModel

// UI-DESIGN §3 quick panel. Production plugin id (DECISIONS D12/D17).
Item {
  id: root

  property var shell: null
  property var manifest: null
  property bool opened: false
  // The layer surface stays mapped until the close fade ends (UI-DESIGN §4: 90 ms). Unmapping on
  // `opened` cut the fade off, and the next open could start from a half-faded card.
  property bool mapped: false
  property bool navigated: false
  property bool showWeaker: false
  property string selection: ""
  property string phase: "empty"
  property bool codeMode: false
  property var preview: ({})
  property var selectedRow: null
  property bool configKnown: false
  // Stays true until Done even after folders Continue sets onboarded
  // (indexing starts then; the wizard must not flip to search).
  property bool wizardOpen: false
  // "search" (files + apps + calculator), "clipboard" (history; LAUNCHER-PLAN.md L4),
  // "settings" (the Trowel Settings command, DECISIONS D26) or "keylock" (Lock Keyboard).
  property string view: "search"
  property var clipContent: ({})
  // Clipboard History opened as a command from the search list: Esc (or Backspace on an empty
  // field) goes back to that search, as in Raycast.
  property bool clipFromSearch: false
  property bool settingsFromSearch: false
  property bool lockFromSearch: false
  property string savedQuery: ""
  // Pop to root (Raycast, oma-cast `LauncherWindow.qml`): reopening within this many seconds of
  // closing keeps the view, query and selection; after it, the panel opens blank. 0 = always blank.
  readonly property int popToRootSeconds: (client.config && client.config.ui && client.config.ui.pop_to_root_seconds !== undefined) ? client.config.ui.pop_to_root_seconds : 90
  property real closedAt: 0
  property real now: Date.now() / 1000
  // Launcher sources join the All tab only; a kind tab or code search is files alone.
  readonly property bool launcherOn: root.view === "search" && tabs.current === "all" && !root.codeMode
  readonly property bool showCalc: root.launcherOn && root.phase !== "empty" && client.calcResult !== null
  readonly property string selectedType: {
    if (root.selection === "calc:")
      return "calc"
    var r = root.selectedRow
    return (r && r.rowType) ? r.rowType : ""
  }
  readonly property var selectedSetting: root.selectedType === "setting" ? root.selectedRow.entry : null

  // Footer, left: `upstream/Sources/DigUpApp/SearchPanelView.swift:131` statusText.
  readonly property string statusText: {
    if (root.view === "keylock")
      return "Locks every keyboard; the mouse keeps working"
    if (root.view === "settings") {
      var n = 0
      for (var k = 0; k < settingsRows.count; k++)
        if (settingsRows.get(k).rowType === "setting")
          n++
      return n === 1 ? "1 setting" : n + " settings"
    }
    if (root.view === "clipboard") {
      var n = client.clipTotal
      return (n === 1 ? "1 item" : n + " items") + (root.clipPaused ? " · capture paused" : "")
    }
    if (root.phase === "words") {
      var st = client.status || {}
      return st.worker === "starting" ? "Matching words; meaning search is warming up…" : "Matching words…"
    }
    if (root.phase === "empty")
      return ""
    var code = root.codeMode || client.lastSearchCode
    var prog = (client.status && client.status.code) ? client.status.code : null
    if (code && prog && prog.total > 0 && prog.done < prog.total)
      return "Still reading code (" + Math.floor(prog.done / prog.total * 100) + "%): some isn't in yet"
    var count = 0
    var groups = client.groups || []
    for (var i = 0; i < groups.length; i++)
      count += (groups[i].rows || []).length
    var weaker = (client.weaker || []).length
    return (count === 1 ? "1 result" : count + " results") + (code ? " in your code" : "") + (weaker > 0 ? " · " + weaker + " weaker" : "")
  }

  // Field badge: what indexing is doing, not a result count.
  // `upstream/Sources/DigUpApp/AppModel.swift:99` activityBadge.
  readonly property string activityBadge: {
    if (client.paused)
      return "Indexing paused"
    var pass = client.activePass
    if (pass)
      return (pass.code ? "Indexing code " : "Indexing ") + Math.floor(pass.done / pass.total * 100) + "%"
    return ""
  }

  readonly property bool needsOnboarding: root.wizardOpen || (root.configKnown && client.config && client.config.onboarded === false)
  readonly property string pluginId: (manifest && manifest.id) ? String(manifest.id) : "io.github.thakurious.trowel"
  readonly property int panelWidth: 780
  readonly property int listWidth: 330
  readonly property real topFraction: 0.16
  readonly property int maxPanelHeight: Math.min(560, Math.round(panel.height * 0.6))

  readonly property bool clipPaused: client.clipPausedAt(root.now)
  // The Images chip shows the clipboard as a 3-column thumbnail grid (Raycast does this).
  readonly property bool gridMode: root.view === "clipboard" && chips.current === "image"

  // Ctrl+K: everything the selection can do (ActionPanel), with the keys that do it directly.
  function actionsFor() {
    var t = root.selectedType
    var r = root.selectedRow
    if (t === "calc") {
      var c = client.calcResult || {}
      var out = [{
          "id": "copy",
          "label": "Copy " + (c.copy || ""),
          "keys": "↵"
        }, {
          "id": "paste",
          "label": "Paste " + (c.copy || ""),
          "keys": "ctrl ↵"
        }]
      var alts = c.alts || []
      for (var i = 0; i < alts.length; i++)
        out.push({
          "id": "alt:" + i,
          "label": "Copy " + alts[i].label + ": " + alts[i].copy
        })
      return out
    }
    if (t === "command")
      return [{
          "id": "open",
          "label": "Open " + r.name,
          "keys": "↵"
        }]
    if (t === "app") {
      var acts = [{
          "id": "open",
          "label": "Open " + r.name,
          "keys": "↵"
        }]
      var desk = (r.raw && r.raw.actions) ? r.raw.actions : []
      for (var j = 0; j < desk.length; j++)
        acts.push({
          "id": "action:" + desk[j].id,
          "label": desk[j].name || desk[j].id
        })
      acts.push({
        "id": "hide",
        "label": "Hide from the launcher"
      })
      return acts
    }
    if (t === "clip") {
      var cl = [{
          "id": "paste",
          "label": "Paste",
          "keys": "↵"
        }, {
          "id": "copy",
          "label": "Copy",
          "keys": "ctrl ↵"
        }]
      if (r.kind === "html" || r.kind === "files")
        cl.push({
          "id": "plain",
          "label": r.kind === "files" ? "Paste the paths as text" : "Paste as plain text",
          "keys": "shift ↵"
        })
      if (r.kind === "link")
        cl.push({
          "id": "browser",
          "label": "Open in browser"
        })
      cl.push({
        "id": "pin",
        "label": r.pinned ? "Unpin" : "Pin",
        "keys": "ctrl shift P"
      }, {
        "id": "delete",
        "label": "Remove from history",
        "keys": "ctrl del"
      }, {
        "id": "clear",
        "label": "Clear history (pinned stay)"
      }, {
        "id": "pause",
        "label": root.clipPaused ? "Resume capturing" : "Pause capturing for an hour"
      })
      return cl
    }
    if (t === "row")
      return [{
          "id": "open",
          "label": "Open",
          "keys": "↵"
        }, {
          "id": "reveal",
          "label": "Show in folder",
          "keys": "ctrl ↵"
        }, {
          "id": "look",
          "label": "Quick Look",
          "keys": "space"
        }, {
          "id": "copypath",
          "label": "Copy",
          "keys": "ctrl C"
        }]
    return []
  }

  function runAction(id) {
    var t = root.selectedType
    var r = root.selectedRow
    if (t === "calc") {
      if (id === "copy" || id === "paste")
        copyCalc(id)
      else if (id.indexOf("alt:") === 0) {
        var alt = (client.calcResult.alts || [])[Number(id.slice(4))]
        if (alt) {
          client.calcCopy(alt.copy, client.calcResult.expr, "copy")
          root.closeWithToast("Copied " + alt.copy)
        }
      }
    } else if (t === "command") {
      if (id === "open")
        openSelected()
    } else if (t === "app") {
      if (id === "open")
        openSelected()
      else if (id.indexOf("action:") === 0) {
        client.appLaunch(r.appId, id.slice(7), client.launcherQuery)
        dismiss()
      } else if (id === "hide") {
        client.appHide(r.appId, true)
        toast.show(r.name + " is hidden from the launcher")
        root.askLauncher(query.text.replace(/^\s+|\s+$/g, ""))
      }
    } else if (t === "clip") {
      if (id === "paste")
        openSelected()
      else if (id === "copy")
        revealSelected()
      else if (id === "plain")
        pasteSelectedPlain()
      else if (id === "browser") {
        Quickshell.execDetached(["omarchy-launch-browser", r.title])
        dismiss()
      } else if (id === "pin")
        togglePin()
      else if (id === "delete")
        deleteSelected()
      else if (id === "clear") {
        client.clipClear(true)
        toast.show("History cleared")
      } else if (id === "pause") {
        client.clipPause(root.clipPaused ? 0 : 60)
        toast.show(root.clipPaused ? "Capturing again" : "Paused for an hour")
      }
    } else if (t === "row") {
      if (id === "open")
        openSelected()
      else if (id === "reveal")
        revealSelected()
      else if (id === "look")
        quickLookSelected()
      else if (id === "copypath")
        client.copyPath(r)
    }
    if (actions.visible === false && root.opened)
      query.forceActiveFocus()
  }

  // Whether the last session is still remembered (`popToRootSeconds`).
  function remembered() {
    return root.closedAt > 0 && root.popToRootSeconds > 0 && Date.now() / 1000 - root.closedAt < root.popToRootSeconds
  }

  function open(payloadJson) {
    var keep = root.remembered()
    // The bar's "Clipboard history" summons with {"view": "clipboard"}.
    try {
      if (JSON.parse(payloadJson || "{}").view === "settings") {
        if (keep && root.view === "settings")
          reopen()
        else
          openSettings(false)
        return
      }
      if (JSON.parse(payloadJson || "{}").view === "clipboard") {
        if (keep && root.view === "clipboard")
          reopen()
        else
          openClipboard()
        return
      }
    } catch (e) {}
    if (keep) {
      reopen()
      return
    }
    unmapTimer.stop()
    actions.visible = false
    root.view = "search"
    root.clipFromSearch = false
    root.settingsFromSearch = false
    root.lockFromSearch = false
    root.mapped = true
    root.opened = true
    root.navigated = false
    root.now = Date.now() / 1000
    if (root.needsOnboarding)
      return
    client.panelOpened()
    // Popped to root: a blank search on the All tab.
    tabs.current = "all"
    query.text = ""
    root.onQueryEdited()
    Qt.callLater(function () {
      query.forceActiveFocus()
    })
  }

  // Back where the user left it: same view, query and selection, with fresh results.
  function reopen() {
    unmapTimer.stop()
    actions.visible = false
    root.mapped = true
    root.opened = true
    root.now = Date.now() / 1000
    if (root.needsOnboarding)
      return
    client.panelOpened()
    if (root.view === "clipboard")
      client.clipRefresh()
    if (root.view === "settings")
      settingsStore.refresh()
    Qt.callLater(function () {
      query.forceActiveFocus()
      query.selectAll()
    })
  }

  // Clipboard history: the "Clipboard History" command (fromSearch) or straight in from
  // `omarchy-shell trowel clipboard` (LAUNCHER-PLAN.md L4).
  function openClipboard(fromSearch) {
    root.clipFromSearch = fromSearch === true
    root.savedQuery = root.clipFromSearch ? query.text : ""
    hybridTimer.stop()
    unmapTimer.stop()
    actions.visible = false
    root.view = "clipboard"
    root.mapped = true
    root.opened = true
    root.navigated = false
    root.now = Date.now() / 1000
    root.selection = ""
    root.selectedRow = null
    root.clipContent = ({})
    if (root.needsOnboarding)
      return
    query.text = ""
    chips.current = "all"
    client.clipHeaders = true
    root.phase = "clipboard"
    client.clearApps()
    client.clearCalc()
    client.clipQuery("", [])
    Qt.callLater(function () {
      query.forceActiveFocus()
    })
  }

  // Settings (DECISIONS D26): the "Trowel Settings" command (fromSearch) or straight in from
  // `omarchy-shell trowel settings`. The list holds the settings; the right pane edits one.
  function openSettings(fromSearch) {
    root.settingsFromSearch = fromSearch === true
    root.savedQuery = root.settingsFromSearch ? query.text : ""
    hybridTimer.stop()
    unmapTimer.stop()
    actions.visible = false
    root.view = "settings"
    root.mapped = true
    root.opened = true
    root.navigated = false
    root.now = Date.now() / 1000
    root.selection = ""
    root.selectedRow = null
    if (root.needsOnboarding)
      return
    query.text = ""
    root.phase = "settings"
    client.clearApps()
    client.clearCalc()
    settingsStore.refresh()
    root.syncSettings()
    Qt.callLater(function () {
      query.forceActiveFocus()
    })
  }

  // The list for the query; the selection stays on its setting while it still matches.
  function syncSettings() {
    var rows = SettingsModel.rows(query.text)
    settingsRows.clear()
    for (var i = 0; i < rows.length; i++)
      settingsRows.append(rows[i])
    root.settleSelection()
  }

  // Lock Keyboard: pick how long in the list (or type it), ↵ locks and the panel closes.
  function openKeyLock(fromSearch) {
    root.lockFromSearch = fromSearch === true
    root.savedQuery = root.lockFromSearch ? query.text : ""
    hybridTimer.stop()
    actions.visible = false
    root.view = "keylock"
    root.navigated = false
    root.selection = ""
    root.selectedRow = null
    query.text = ""
    root.phase = "keylock"
    client.clearApps()
    client.clearCalc()
    root.syncKeyLock()
    Qt.callLater(function () {
      query.forceActiveFocus()
    })
  }

  function syncKeyLock() {
    var rows = KeyLockModel.rows(query.text)
    keylockRows.clear()
    for (var i = 0; i < rows.length; i++)
      keylockRows.append(rows[i])
    // A typed duration is what the user meant: select it even after moving.
    if (rows.length && rows[0].custom)
      root.navigated = false
    root.settleSelection()
  }

  function lockKeyboard(seconds) {
    root.dismiss()
    lockProc.command = ["trowel", "keyboard", "lock", String(seconds)]
    lockProc.running = true
  }

  // ↵ on a setting row: flip a switch, run a single action, or hand the keys to the pane.
  function settingPrimary() {
    var e = root.selectedSetting
    if (!e)
      return
    var what = SettingsModel.primary(e, settingsStore.ctx)
    if (what === "toggle")
      settingsStore.toggle(e)
    else if (what === "action") {
      var act = SettingsModel.actionsFor(e, settingsStore.ctx)[0]
      if (act.danger && settingDetail.armed !== act.id) {
        settingDetail.armed = act.id
        toast.show("Press ↵ again to " + act.label.toLowerCase())
        return
      }
      settingDetail.armed = ""
      settingsStore.run(act.id)
      if (act.id === "doctor")
        toast.show("Checking…")
    } else
      settingDetail.enter()
  }

  // Keys the query field passes on in the Settings view. Tab hands the keys to the pane; ←/→ change
  // a choice or switch in place while the field is empty (the cursor has nowhere to go then).
  function settingsKey(event) {
    if (query.composing())
      return false
    var mods = event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier)
    if (event.key === Qt.Key_Tab && !mods) {
      if (root.selectedSetting)
        settingDetail.enter()
      return true
    }
    if ((event.key === Qt.Key_Left || event.key === Qt.Key_Right) && !mods && !query.text.length)
      return root.settingStep(event.key === Qt.Key_Left ? -1 : 1)
    return false
  }

  // ←/→ on a choice or switch row, from the list, while the field is empty.
  function settingStep(step) {
    var e = root.selectedSetting
    if (!e)
      return false
    if (e.type === "choice") {
      var v = SettingsModel.stepChoice(e, settingsStore.ctx, step)
      if (v !== settingsStore.value(e))
        settingsStore.set(e, v)
      return true
    }
    if (e.type === "toggle") {
      if (settingsStore.value(e) !== (step > 0))
        settingsStore.set(e, step > 0)
      return true
    }
    return false
  }

  // Stamps the close for pop to root and re-reads the config, so a timeout changed in Settings
  // (another process) holds from the next open.
  function stampClose() {
    if (!root.opened)
      return
    root.closedAt = Date.now() / 1000
    client.configGet()
  }

  function close() {
    root.stampClose()
    root.opened = false
    unmapTimer.restart()
  }

  function dismiss() {
    actions.visible = false
    root.stampClose()
    root.opened = false
    unmapTimer.restart()
    if (root.shell && typeof root.shell.hide === "function")
      root.shell.hide(root.pluginId)
  }

  function toggle() {
    if (root.opened)
      root.dismiss()
    else
      root.open("{}")
  }

  function onQueryEdited() {
    root.navigated = false
    if (root.view === "keylock") {
      root.syncKeyLock()
      return
    }
    if (root.view === "settings") {
      root.syncSettings()
      return
    }
    if (root.view === "clipboard") {
      root.phase = "clipboard"
      client.clipQuery(query.text.replace(/^\s+|\s+$/g, ""), chips.kindsFor(chips.current))
      return
    }
    var raw = query.text
    var trimmed = raw.replace(/^\s+|\s+$/g, "")
    // Typing "code:" picks the Code tab; the tab alone is enough too.
    var prefixed = trimmed.indexOf("code:") === 0
    if (prefixed && tabs.current !== "code")
      tabs.current = "code"
    root.codeMode = tabs.current === "code"
    var text = prefixed ? trimmed.replace(/^code:\s*/, "") : trimmed
    client.kinds = tabs.kindsFor(tabs.current)
    if (trimmed.length === 0) {
      root.phase = "empty"
      client.counts = ({})
      client.countsText = ""
      client.groups = []
      client.weaker = []
      client.clearApps()
      client.clearCalc()
      client.syncModel()
      hybridTimer.stop()
      return
    }
    root.askLauncher(text)
    if (root.codeMode && text.length === 0) {
      root.phase = "codeHint"
      client.groups = []
      client.weaker = []
      client.syncModel()
      return
    }
    root.phase = "words"
    client.typing(text, root.codeMode)
    client.search(text, "keyword", root.codeMode, true)
    hybridTimer.restart()
  }

  // Built-in commands (ListMerge.COMMANDS) run inside the panel.
  function runCommand(id) {
    if (id === "clipboard")
      openClipboard(true)
    else if (id === "settings")
      openSettings(true)
    else if (id === "lock-keyboard")
      openKeyLock(true)
  }

  // Esc / Backspace on an empty field inside a command: back to the search it came from.
  function goBack() {
    var fromSearch = (root.view === "clipboard" && root.clipFromSearch) || (root.view === "settings" && root.settingsFromSearch) || (root.view === "keylock" && root.lockFromSearch)
    if (!fromSearch)
      return false
    actions.visible = false
    root.view = "search"
    root.clipFromSearch = false
    root.settingsFromSearch = false
    root.lockFromSearch = false
    root.selection = ""
    root.selectedRow = null
    root.navigated = false
    query.text = root.savedQuery
    root.onQueryEdited()
    query.forceActiveFocus()
    return true
  }

  // Back to the All tab from a kind tab or code search (which "code:" makes sticky).
  function leaveTab() {
    if (root.view !== "search" || tabs.current === "all")
      return false
    tabs.current = "all"
    root.onTabPicked("all")
    return true
  }

  // Escape steps out one level at a time: the query, then the tab, then the clipboard view
  // opened from search, then the panel. Closing never leaves a tab to come back to.
  function escapeStep() {
    if (query.text.length > 0)
      query.text = ""
    else if (!root.leaveTab() && !root.goBack())
      root.dismiss()
  }

  // Apps and the calculator for the same text, each with its own gen (Client.qml).
  function askLauncher(text) {
    if (!root.launcherOn || !text.length) {
      client.launcherQuery = ""
      client.clearApps()
      client.syncModel()
      client.clearCalc()
      return
    }
    client.launcherQuery = text
    client.syncModel()
    root.settleSelection()
    if (client.hasApps)
      client.appsQuery(text)
    if (client.hasCalc)
      client.calc(text, false)
  }

  function runHybrid() {
    var trimmed = query.text.replace(/^\s+|\s+$/g, "")
    var text = trimmed.replace(/^code:\s*/, "")
    if (text.length === 0)
      return
    client.kinds = tabs.kindsFor(tabs.current)
    client.search(text, "hybrid", root.codeMode, false)
  }

  // A kind tab was picked (Tab, Ctrl+1…7 or a click): the same query again, filtered.
  function onTabPicked(id) {
    if (id !== "code" && query.text.replace(/^\s+/, "").indexOf("code:") === 0)
      query.text = query.text.replace(/^\s*code:\s*/, "")
    root.selection = ""
    root.selectedRow = null
    root.preview = ({})
    root.navigated = false
    root.codeMode = id === "code"
    client.kinds = tabs.kindsFor(id)
    var text = query.text.replace(/^\s+|\s+$/g, "").replace(/^code:\s*/, "")
    if (!text.length) {
      onQueryEdited()
      return
    }
    // `launcherOn` follows `tabs.current`, which FilterBar set before emitting `picked`.
    root.askLauncher(text)
    // Same text: its vector is cached, so go straight to the full search. A keyword pass
    // first showed other rows and counts for a moment.
    hybridTimer.stop()
    client.search(text, "hybrid", root.codeMode, false)
  }

  readonly property var listModel: root.view === "clipboard" ? client.clipModel : (root.view === "settings" ? settingsRows : (root.view === "keylock" ? keylockRows : client.rows))

  property ListModel settingsRows: ListModel {}
  property ListModel keylockRows: ListModel {}

  // {rowType, rowId} of the list on screen, for ListMerge's selection rules.
  function shownRows() {
    var out = []
    var m = root.listModel
    for (var i = 0; i < m.count; i++) {
      var it = m.get(i)
      out.push({
        rowType: it.rowType,
        rowId: it.rowId
      })
    }
    return out
  }

  // A reply landed: keep the user's pick if it is still there, else the default (the calculator
  // card, a strong app, or the first row).
  function settleSelection() {
    if (!root.navigated || !selectionStillPresent())
      root.selection = ListMerge.defaultSelection(shownRows(), root.showCalc)
    pickSelected()
  }

  function moveSelection(step) {
    var ids = ListMerge.selectable(shownRows(), root.showCalc)
    if (ids.length === 0)
      return
    var idx = ids.indexOf(root.selection)
    if (idx < 0)
      idx = -1
    idx = Math.max(0, Math.min(ids.length - 1, idx + step))
    root.selection = ids[idx]
    root.navigated = true
    pickSelected()
  }

  function pickSelected() {
    if (root.view === "keylock") {
      root.selectedRow = null
      for (var k = 0; k < keylockRows.count; k++)
        if (keylockRows.get(k).rowId === root.selection)
          root.selectedRow = {
            "rowType": "duration",
            "rowId": root.selection,
            "seconds": keylockRows.get(k).seconds
          }
      return
    }
    if (root.view === "settings") {
      var e = root.selection.indexOf("set:") === 0 ? SettingsModel.byId(root.selection.slice(4)) : null
      root.selectedRow = e ? {
        "rowType": "setting",
        "rowId": root.selection,
        "entry": e
      } : null
      return
    }
    if (root.selection === "calc:") {
      root.selectedRow = null
      return
    }
    var app = client.rowIndex[root.selection]
    if (root.view === "search" && app && (app.rowType === "app" || app.rowType === "command")) {
      root.selectedRow = app
      return
    }
    var clip = client.clipIndex[root.selection]
    if (root.view === "clipboard" && clip && clip.rowType === "clip") {
      if (!root.selectedRow || root.selectedRow.rowId !== clip.rowId)
        client.clipGet(clip.clipId)
      root.selectedRow = clip
      return
    }
    for (var i = 0; i < client.rows.count; i++) {
      var it = client.rows.get(i)
      if (it.rowId === root.selection && it.rowType === "row") {
        root.selectedRow = it
        root.preview = ({})
        if (it.segment_id)
          client.preview(it.segment_id, 450)
        return
      }
    }
    root.selectedRow = null
  }

  // Enter: the calculator copies, an app launches, a clip pastes into the app below (D21: the
  // panel hides first, the daemon sends Shift+Insert after 150 ms), a file opens.
  function openSelected() {
    var t = root.selectedType
    if (t === "calc") {
      copyCalc("copy")
      return
    }
    if (t === "app") {
      client.appLaunch(root.selectedRow.appId, "", client.launcherQuery)
      dismiss()
      return
    }
    if (t === "command") {
      client.commandRun(root.selectedRow.commandId, client.launcherQuery)
      runCommand(root.selectedRow.commandId)
      return
    }
    if (t === "setting") {
      root.settingPrimary()
      return
    }
    if (t === "duration") {
      root.lockKeyboard(root.selectedRow.seconds)
      return
    }
    if (t === "clip") {
      var id = root.selectedRow.clipId
      dismiss()
      client.clipPaste(id, "paste")
      return
    }
    if (root.selection === client.weakerId) {
      client.showWeaker = !client.showWeaker
      root.showWeaker = client.showWeaker
      return
    }
    if (root.selectedRow)
      client.openRow(root.selectedRow)
    dismiss()
  }

  function quickLookSelected() {
    if (!root.selectedRow)
      return
    if (root.selectedRow.segment_id)
      client.quickLook(root.selectedRow.segment_id)
    lookProc.running = true
  }

  function revealSelected() {
    var t = root.selectedType
    if (t === "calc") {
      copyCalc("paste")
      return
    }
    if (t === "clip") {
      // Ctrl+Enter on a clip: back on the clipboard only, nothing typed.
      client.clipPaste(root.selectedRow.clipId, "copy")
      root.closeWithToast("Copied")
      return
    }
    if (t === "app")
      return
    if (root.selectedRow)
      client.revealRow(root.selectedRow)
    dismiss()
  }

  function copyCalc(mode) {
    var r = client.calcResult
    if (!r)
      return
    if (mode === "paste") {
      dismiss()
      client.calcCopy(r.copy, r.expr, "paste")
      return
    }
    client.calcCopy(r.copy, r.expr, "copy")
    root.closeWithToast("Copied " + r.copy)
  }

  // A confirmation the user can read, then the panel closes.
  function closeWithToast(text) {
    toast.show(text)
    toastClose.restart()
  }

  function pasteSelectedPlain() {
    if (root.selectedType !== "clip")
      return
    var id = root.selectedRow.clipId
    dismiss()
    client.clipPaste(id, "plain")
  }

  function togglePin() {
    if (root.selectedType !== "clip")
      return
    var pinned = !(root.selectedRow.pinned === true)
    client.clipPin(root.selectedRow.clipId, pinned)
    toast.show(pinned ? "Pinned" : "Unpinned")
  }

  function deleteSelected() {
    if (root.selectedType !== "clip")
      return
    // The next row takes the selection.
    var ids = ListMerge.selectable(shownRows(), false)
    var at = ids.indexOf(root.selection)
    var next = ids[at + 1] || ids[at - 1] || ""
    client.clipDelete(root.selectedRow.clipId)
    root.selection = next
    toast.show("Removed")
  }

  IpcHandler {
    id: ipc
    target: "trowel"
    function open(): void {
      root.open("{}")
    }
    function close(): void {
      root.dismiss()
    }
    function toggle(): void {
      root.toggle()
    }
    function clipboard(): void {
      if (root.opened && root.view === "clipboard")
        root.dismiss()
      else
        root.open('{"view": "clipboard"}')
    }
    function apps(): void {
      root.open("{}")
    }
    function settings(): void {
      if (root.opened && root.view === "settings")
        root.dismiss()
      else
        root.open('{"view": "settings"}')
    }
    // Debugging without keystrokes: `omarchy-shell trowel debugState` etc.
    function debugState(): string {
      var lv = root.view === "clipboard" ? clipList : (root.view === "settings" ? settingsList : (root.view === "keylock" ? lockList : list))
      var m = lv.model
      var shown = []
      for (var i = 0; i < Math.min(m ? m.count : 0, 12); i++)
        shown.push(m.get(i).rowType + ":" + (m.get(i).title || m.get(i).rowId))
      var delegates = []
      // What each visible delegate is drawing: the row title it shows.
      for (var j = 0; j < lv.contentItem.children.length && delegates.length < 12; j++) {
        var d = lv.contentItem.children[j]
        if (!d || !d.visible || d.height < 30)
          continue
        for (var k = 0; k < d.children.length; k++)
          if (d.children[k].visible && d.children[k].row)
            delegates.push(Math.round(d.y) + ":" + (d.children[k].row.title || d.children[k].row.name || "?"))
      }
      return JSON.stringify({
        view: root.view,
        phase: root.phase,
        selection: root.selection,
        type: root.selectedType,
        list: root.view === "clipboard" ? "clipList" : (root.view === "settings" ? "settingsList" : "list"),
        modelRows: shown,
        clipCount: client.clipModel.count,
        rowsCount: client.rows.count,
        delegates: delegates,
        query: query.text
      })
    }
    function debugType(text: string): void {
      query.text = text
      root.onQueryEdited()
    }
    function debugCommand(id: string): void {
      root.runCommand(id)
    }
  }

  KeyLockPill {}

  Process {
    id: lockProc
    stderr: StdioCollector {
      id: lockErr
    }
    onExited: function (code) {
      if (code !== 0)
        Quickshell.execDetached(["notify-send", "-a", "Trowel", "Couldn't lock the keyboard", String(lockErr.text || "").trim()])
    }
  }

  SettingsState {
    id: settingsStore
    client: client
    onToast: function (text) {
      toast.show(text)
    }
  }

  Timer {
    id: toastClose
    interval: 450
    onTriggered: root.dismiss()
  }

  Timer {
    interval: 30000
    repeat: true
    running: root.opened
    onTriggered: root.now = Date.now() / 1000
  }

  Client {
    id: client
    clientName: "panel"
    showWeaker: root.showWeaker
    onConfigArrived: function (cfg) {
      root.configKnown = true
      if (cfg && cfg.onboarded === false)
        root.wizardOpen = true
      if (root.opened && root.needsOnboarding)
        Qt.callLater(function () {
          onboard.forceActiveFocus()
        })
    }
    onHelloOkChanged: function () {
      if (client.helloOk && root.opened && !root.needsOnboarding && query.text.length)
        Qt.callLater(function () {
          root.onQueryEdited()
        })
    }
    onSearchArrived: function (g, meaning) {
      if (root.view !== "search")
        return
      if (meaning === "used")
        root.phase = "meaning"
      else if (meaning === "unavailable")
        root.phase = "wordsOnly"
      root.settleSelection()
    }
    onAppsArrived: {
      if (root.view === "search")
        root.settleSelection()
    }
    onCalcArrived: {
      if (root.view === "search")
        root.settleSelection()
    }
    onClipsArrived: {
      if (root.view === "clipboard")
        root.settleSelection()
    }
    onClipContentArrived: function (msg) {
      root.clipContent = msg
    }
    onClipboardChanged: function (msg) {
      if (root.opened && root.view === "clipboard")
        client.clipRefresh()
    }
    onOpFailed: function (op, error) {
      var code = error.code || ""
      if (op === "clip_paste")
        toast.show(code === "not_found" ? "That item is no longer available" : "Couldn't use the clipboard")
      else if (op === "calc_copy")
        toast.show("Couldn't copy")
      else if (op === "app_launch")
        toast.show("Couldn't open the app")
      else if (op === "clip_delete" || op === "clip_pin")
        client.clipRefresh()
    }
    onOpDone: function (op, msg) {
      if ((op === "clip_pin" || op === "clip_delete") && root.view === "clipboard")
        client.clipRefresh()
    }
    onPreviewArrived: function (msg) {
      root.preview = msg
    }
    onLineEvent: function (msg) {
      if (msg.type)
        root.preview = msg
    }
  }

  function selectionStillPresent() {
    if (root.selection === "calc:")
      return root.showCalc
    var m = root.listModel
    for (var i = 0; i < m.count; i++) {
      if (m.get(i).rowId === root.selection)
        return true
    }
    return false
  }

  Timer {
    id: hybridTimer
    interval: 120
    repeat: false
    onTriggered: root.runHybrid()
  }

  Timer {
    id: unmapTimer
    // One frame past the close animation.
    interval: Motion.closeMs + 17
    repeat: false
    onTriggered: {
      if (!root.opened)
        root.mapped = false
    }
  }

  PanelWindow {
    id: panel
    visible: root.mapped
    color: "transparent"
    exclusionMode: ExclusionMode.Ignore
    WlrLayershell.namespace: "trowel-panel"
    WlrLayershell.layer: WlrLayer.Overlay
    // Keyboard goes back to the app at the *start* of the close (UI-DESIGN §4).
    WlrLayershell.keyboardFocus: root.opened ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None
    anchors {
      top: true
      bottom: true
      left: true
      right: true
    }

    MouseArea {
      anchors.fill: parent
      enabled: root.opened
      onClicked: root.dismiss()
    }

    Shortcut {
      sequence: "Escape"
      enabled: root.opened && root.needsOnboarding
      onActivated: root.dismiss()
    }
    Shortcut {
      sequences: ["Return", "Enter"]
      enabled: root.opened && root.needsOnboarding
      onActivated: onboard.next()
    }

    Item {
      id: card
      width: Math.min(root.panelWidth, panel.width - 24)
      // Hidden search chrome still reports implicitHeight, so onboarding
      // must size the card itself (UI-DESIGN §3 max 560). Empty search is
      // the query field only; results expand to maxPanelHeight.
      height: root.needsOnboarding ? root.maxPanelHeight : (root.phase === "empty" ? Math.min(root.maxPanelHeight, fieldCol.implicitHeight + 8) : root.maxPanelHeight)
      x: Math.round((panel.width - width) / 2)
      readonly property int restY: Math.round(panel.height * root.topFraction)
      y: root.opened ? restY : restY - 6
      layer.enabled: false

      Behavior on y {
        NumberAnimation {
          duration: root.opened ? Motion.openMs : Motion.closeMs
          easing.type: root.opened ? Motion.openEasing : Motion.closeEasing
        }
      }

      Behavior on height {
        enabled: !root.needsOnboarding
        NumberAnimation {
          duration: Motion.heightMs
          easing.type: Motion.heightEasing
        }
      }

      scale: root.opened ? 1 : 0.985
      opacity: root.opened ? 1 : 0
      transformOrigin: Item.Top
      Behavior on opacity {
        NumberAnimation {
          duration: root.opened ? Motion.openMs : Motion.closeMs
          easing.type: root.opened ? Motion.openEasing : Motion.closeEasing
        }
      }
      Behavior on scale {
        NumberAnimation {
          duration: root.opened ? Motion.openMs : Motion.closeMs
          easing.type: root.opened ? Motion.openEasing : Motion.closeEasing
        }
      }

      Rectangle {
        id: fill
        anchors.fill: parent
        radius: Math.max(Style.cornerRadius, 0)
        color: Qt.rgba(Color.background.r, Color.background.g, Color.background.b, 0.75)
        clip: true
        border.width: 1
        border.color: Qt.rgba(Color.foreground.r, Color.foreground.g, Color.foreground.b, 0.14)

        MouseArea {
          anchors.fill: parent
          onClicked: {
            if (!root.needsOnboarding)
              query.forceActiveFocus()
          }
        }

        ActionPanel {
          id: actions
          z: 4
          anchors.right: parent.right
          anchors.bottom: parent.bottom
          anchors.rightMargin: 10
          anchors.bottomMargin: 34
          onTriggered: function (id) {
            root.runAction(id)
          }
          onClosed: query.forceActiveFocus()
        }

        Toast {
          id: toast
          z: 3
          anchors.horizontalCenter: parent.horizontalCenter
          anchors.bottom: parent.bottom
          anchors.bottomMargin: 40
        }

        Onboarding {
          id: onboard
          z: 2
          visible: root.needsOnboarding
          anchors.fill: parent
          client: client
          onFinished: {
            root.wizardOpen = false
            root.configKnown = true
            client.configGet()
            query.forceActiveFocus()
            client.panelOpened()
          }
        }

        Column {
          id: searchCol
          visible: !root.needsOnboarding
          enabled: !root.needsOnboarding
          width: parent.width

        Column {
          id: fieldCol
          width: parent.width

          QueryField {
            id: query
            width: parent.width
            codeMode: root.codeMode
            crumb: root.view === "clipboard" ? "Clipboard History" : (root.view === "settings" ? "Settings" : (root.view === "keylock" ? "Lock Keyboard" : ""))
            hybridPending: root.phase === "words"
            countText: root.activityBadge
            placeholder: root.view === "keylock" ? "How long? Pick one, or type 45 or 3m" : root.view === "settings" ? "Search settings" : root.view === "clipboard" ? "Search clipboard history" : (root.codeMode ? "Search your code" : "Describe it: “payment error screenshot”, “zebra in a video”…")
            onTextEdited: root.onQueryEdited()
            onKeyPressed: function (event) {
              var ctrl = event.modifiers & Qt.ControlModifier
              var shift = event.modifiers & Qt.ShiftModifier
              if (query.composing() && (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Up || event.key === Qt.Key_Down || event.key === Qt.Key_Escape))
                return
              if (root.view === "settings" && root.settingsKey(event)) {
                event.accepted = true
              } else if (event.key === Qt.Key_Escape) {
                root.escapeStep()
                event.accepted = true
              } else if (event.key === Qt.Key_Backspace && query.text.length === 0 && (root.goBack() || (root.codeMode && root.leaveTab()))) {
                event.accepted = true
              } else if (ctrl && event.key === Qt.Key_Comma) {
                // Ctrl+, opens Settings from anywhere in the panel, as in Raycast.
                if (root.view !== "settings")
                  root.openSettings(root.view === "search")
                event.accepted = true
              } else if (ctrl && event.key === Qt.Key_K) {
                actions.actions = root.actionsFor()
                if (actions.actions.length)
                  actions.open()
                event.accepted = true
              } else if (ctrl && shift && event.key === Qt.Key_P) {
                root.togglePin()
                event.accepted = true
              } else if (ctrl && event.key === Qt.Key_Delete) {
                root.deleteSelected()
                event.accepted = true
              } else if (event.key === Qt.Key_Down || (ctrl && event.key === Qt.Key_N)) {
                root.moveSelection(root.gridMode ? 3 : 1)
                event.accepted = true
              } else if (event.key === Qt.Key_Up || (ctrl && event.key === Qt.Key_P)) {
                root.moveSelection(root.gridMode ? -3 : -1)
                event.accepted = true
              } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                if (ctrl)
                  root.revealSelected()
                else if (shift && root.selectedType === "clip")
                  root.pasteSelectedPlain()
                else
                  root.openSelected()
                event.accepted = true
              } else if (event.key === Qt.Key_Space && root.navigated && root.selection.length && (root.selectedType === "row" || root.selection === client.weakerId)) {
                if (root.selection === client.weakerId) {
                  client.showWeaker = !client.showWeaker
                  root.showWeaker = client.showWeaker
                } else
                  root.quickLookSelected()
                event.accepted = true
              } else if (ctrl && event.key === Qt.Key_Y) {
                root.quickLookSelected()
                event.accepted = true
              } else if (ctrl && event.key === Qt.Key_C && (root.navigated || root.selectedType === "calc") && !query.hasSelectedText()) {
                if (root.selectedType === "row")
                  client.copyPath(root.selectedRow)
                else
                  root.revealSelected()
                event.accepted = true
              } else if (ctrl && event.key === Qt.Key_O) {
                appProc.running = true
                event.accepted = true
              } else if (event.key === Qt.Key_Tab && !ctrl) {
                if (root.view === "clipboard")
                  chips.cycle(1)
                else
                  tabs.cycle(1)
                event.accepted = true
              } else if (event.key === Qt.Key_Backtab) {
                if (root.view === "clipboard")
                  chips.cycle(-1)
                else
                  tabs.cycle(-1)
                event.accepted = true
              } else if (ctrl && root.view === "search" && event.key >= Qt.Key_1 && event.key <= Qt.Key_7) {
                // Ctrl+1 All, Ctrl+2…6 one kind, Ctrl+7 Code (`SearchWindow.swift:133`, ⌘ there).
                tabs.pick(tabs.ids[event.key - Qt.Key_1] || tabs.current)
                event.accepted = true
              }
            }
          }

          Item {
            width: parent.width
            height: visible ? 34 : 0
            visible: root.phase !== "empty" && root.view !== "settings" && root.view !== "keylock"
            KindChips {
              id: chips
              visible: root.view === "clipboard"
              anchors.left: parent.left
              anchors.leftMargin: 14
              anchors.verticalCenter: parent.verticalCenter
              onPicked: function (id) {
                actions.visible = false
                root.selection = ""
                root.navigated = false
                client.clipHeaders = id !== "image"
                root.onQueryEdited()
              }
            }
            FilterBar {
              id: tabs
              visible: root.view === "search"
              anchors.left: parent.left
              anchors.leftMargin: 14
              anchors.verticalCenter: parent.verticalCenter
              counts: client.counts
              // Dim kinds with no result once the query's file counts are known (also on the Code
              // tab, which keeps them); a query typed on the Code tab only knows the Code count.
              dimEmpty: Object.keys(client.counts || {}).some(function (k) {
                return k !== "code"
              })
              showCode: !!(client.config && client.config.code_roots && client.config.code_roots.length)
              onPicked: function (id) {
                root.onTabPicked(id)
              }
            }
            Row {
              anchors.right: parent.right
              anchors.rightMargin: 16
              anchors.verticalCenter: parent.verticalCenter
              spacing: 4
              Keycap {
                label: "tab"
                size: 11
              }
              Text {
                text: "Type"
                color: Qt.rgba(Color.foreground.r, Color.foreground.g, Color.foreground.b, 0.62)
                font.pixelSize: 11
                anchors.verticalCenter: parent.verticalCenter
              }
            }
          }

          Banner {
            visible: client.bannerText.length > 0
            width: parent.width - 24
            anchors.horizontalCenter: parent.horizontalCenter
            text: client.bannerText
            action: client.connected ? "" : "Start"
            onActionClicked: client.startDaemon()
          }
        }

        Item {
          id: bodyCol
          width: parent.width
          height: root.phase === "empty" ? 0 : Math.max(180, card.height - fieldCol.implicitHeight - footer.implicitHeight)
          visible: root.phase !== "empty"

          CalcCard {
            id: calcCard
            visible: root.showCalc
            anchors.top: parent.top
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.margins: 10
            anchors.topMargin: 2
            height: visible ? implicitHeight : 0
            result: client.calcResult
            selected: root.selection === "calc:"
            onClicked: {
              root.selection = "calc:"
              root.navigated = true
              root.pickSelected()
            }
            onOpened: root.copyCalc("copy")
          }

          EmptyState {
            visible: root.listModel.count === 0 && !root.showCalc
            anchors.centerIn: parent
            width: parent.width - 48
            title: {
              if (root.view === "keylock")
                return "Type a duration, like 45 or 3m"
              if (root.view === "settings")
                return "No settings match"
              if (root.view === "clipboard")
                return query.text.length ? "No matches in your clipboard history" : "Nothing copied yet"
              if (root.phase === "codeHint")
                return "Search your code"
              if (root.phase === "codeOff")
                return "Code search is off"
              if (root.phase === "words")
                return "Searching…"
              if (root.codeMode)
                return "No good matches in your code"
              return "No good matches"
            }
            detail: {
              if (root.view === "keylock")
                return "Up to 30 minutes."
              if (root.view === "settings")
                return "Try a word like folders, clipboard or shortcut."
              if (root.view === "clipboard")
                return query.text.length ? "" : "Text, images, links and files you copy show up here."
              if (root.phase === "words")
                return ""
              if (root.codeMode)
                return "Try saying what the code does, or a name that's in it."
              return "Try what you'd see or hear in it, or words you remember from it."
            }
          }

          Row {
            visible: root.listModel.count > 0
            anchors.top: calcCard.visible ? calcCard.bottom : parent.top
            anchors.topMargin: calcCard.visible ? 8 : 0
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom

            GridView {
              id: grid
              visible: root.gridMode
              width: root.gridMode ? root.listWidth : 0
              height: parent.height
              model: root.gridMode ? client.clipModel : null
              clip: true
              cellWidth: Math.floor((root.listWidth - 12) / 3)
              cellHeight: cellWidth
              leftMargin: 6
              topMargin: 6
              cacheBuffer: 240
              reuseItems: true

              Connections {
                target: root
                function onSelectionChanged() {
                  if (!root.gridMode)
                    return
                  var m = client.clipModel
                  for (var i = 0; i < m.count; i++)
                    if (m.get(i).rowId === root.selection) {
                      grid.positionViewAtIndex(i, GridView.Contain)
                      return
                    }
                }
              }

              delegate: Item {
                id: cell
                required property string rowId
                required property string thumb
                required property string kind
                width: grid.cellWidth
                height: grid.cellHeight
                Rectangle {
                  anchors.fill: parent
                  anchors.margins: 3
                  radius: 8
                  color: Theme.alpha(Theme.foreground, 0.045)
                  border.width: root.selection === cell.rowId ? 2 : Theme.hairlineWidth()
                  border.color: root.selection === cell.rowId ? Theme.accent : Theme.hairline
                  clip: true
                  Image {
                    id: cellImg
                    anchors.fill: parent
                    anchors.margins: 2
                    source: cell.thumb.length ? "file://" + cell.thumb : ""
                    sourceSize.width: width
                    sourceSize.height: height
                    asynchronous: true
                    fillMode: Image.PreserveAspectCrop
                    visible: status === Image.Ready
                  }
                  KindIcon {
                    anchors.centerIn: parent
                    kind: "image"
                    side: 26
                    visible: cellImg.status !== Image.Ready
                  }
                }
                MouseArea {
                  anchors.fill: parent
                  onClicked: {
                    root.selection = cell.rowId
                    root.navigated = true
                    root.pickSelected()
                  }
                  onDoubleClicked: root.openSelected()
                }
              }
            }

            // Two lists, never one with a swapped model: on the live shell the swap from file
            // results to the clipboard left recycled delegates drawing file rows (2026-10-16).
            ListView {
              id: list
              visible: root.view === "search"
              width: visible ? root.listWidth : 0
              height: parent.height
              model: client.rows
              clip: true
              reuseItems: true
              cacheBuffer: 240
              highlightMoveDuration: Motion.highlightMs
              highlightResizeDuration: 0

              // Keep the selected row in view as the arrows move it (upstream ScrollViewReader
              // scrollTo): the smallest scroll, with a group's header when its first row is picked.
              function reveal(rowId) {
                for (var i = 0; i < count; i++) {
                  if (model.get(i).rowId !== rowId)
                    continue
                  if (i > 0 && model.get(i - 1).rowType === "header")
                    positionViewAtIndex(i - 1, ListView.Contain)
                  positionViewAtIndex(i, ListView.Contain)
                  return
                }
              }

              Connections {
                target: root
                function onSelectionChanged() {
                  var id = root.selection
                  Qt.callLater(function () {
                    list.reveal(id)
                  })
                }
              }
              delegate: Item {
                width: list.width
                height: {
                  if (rowType === "header")
                    return 28
                  if (rowType === "fold")
                    return 26
                  return 46
                }
                GroupHeader {
                  visible: rowType === "header"
                  title: model.title
                  width: parent.width
                }
                WeakerFold {
                  visible: rowType === "fold"
                  width: parent.width
                  count: model.count
                  expanded: client.showWeaker
                  selected: root.selection === rowId
                  onToggled: {
                    client.showWeaker = !client.showWeaker
                    root.showWeaker = client.showWeaker
                    root.selection = client.weakerId
                    root.navigated = true
                  }
                }
                AppRow {
                  visible: rowType === "app" || rowType === "command"
                  width: parent.width
                  row: (rowType === "app" || rowType === "command") ? (client.rowIndex[rowId] || ({})) : ({})
                  selected: root.selection === rowId
                  onClicked: {
                    root.selection = rowId
                    root.navigated = true
                    root.pickSelected()
                  }
                  onOpened: root.openSelected()
                }
                ResultRow {
                  visible: rowType === "row"
                  width: parent.width
                  row: model
                  selected: root.selection === rowId
                  onClicked: {
                    root.selection = rowId
                    root.navigated = true
                    root.pickSelected()
                  }
                  onOpened: root.openSelected()
                }
              }
            }

            ListView {
              id: clipList
              visible: root.view === "clipboard" && !root.gridMode
              width: visible ? root.listWidth : 0
              height: parent.height
              model: client.clipModel
              clip: true
              cacheBuffer: 240

              Connections {
                target: root
                function onSelectionChanged() {
                  if (!clipList.visible)
                    return
                  var id = root.selection
                  Qt.callLater(function () {
                    var m = client.clipModel
                    for (var i = 0; i < m.count; i++)
                      if (m.get(i).rowId === id) {
                        if (i > 0 && m.get(i - 1).rowType === "header")
                          clipList.positionViewAtIndex(i - 1, ListView.Contain)
                        clipList.positionViewAtIndex(i, ListView.Contain)
                        return
                      }
                  })
                }
              }

              delegate: Item {
                id: clipCell
                required property string rowType
                required property string rowId
                required property string title
                width: clipList.width
                height: rowType === "header" ? 28 : 46
                GroupHeader {
                  visible: clipCell.rowType === "header"
                  title: clipCell.title
                  width: parent.width
                }
                ClipRow {
                  visible: clipCell.rowType === "clip"
                  width: parent.width
                  row: clipCell.rowType === "clip" ? (client.clipIndex[clipCell.rowId] || ({})) : ({})
                  now: root.now
                  selected: root.selection === clipCell.rowId
                  onClicked: {
                    root.selection = clipCell.rowId
                    root.navigated = true
                    root.pickSelected()
                  }
                  onOpened: root.openSelected()
                }
              }
            }

            ListView {
              id: lockList
              visible: root.view === "keylock"
              width: visible ? root.listWidth : 0
              height: parent.height
              model: keylockRows
              clip: true
              delegate: SettingRow {
                id: lockCell
                required property string rowId
                required property string title
                required property bool custom
                width: lockList.width
                entry: ({
                    "glyph": "\uf017",
                    "title": lockCell.title
                  })
                accessory: lockCell.custom ? "Typed" : ""
                selected: root.selection === lockCell.rowId
                onClicked: {
                  root.selection = lockCell.rowId
                  root.navigated = true
                  root.pickSelected()
                  query.forceActiveFocus()
                }
                onOpened: root.openSelected()
              }
            }

            ListView {
              id: settingsList
              visible: root.view === "settings"
              width: visible ? root.listWidth : 0
              height: parent.height
              model: settingsRows
              clip: true
              cacheBuffer: 240

              Connections {
                target: root
                function onSelectionChanged() {
                  if (!settingsList.visible)
                    return
                  var id = root.selection
                  Qt.callLater(function () {
                    for (var i = 0; i < settingsRows.count; i++)
                      if (settingsRows.get(i).rowId === id) {
                        if (i > 0 && settingsRows.get(i - 1).rowType === "header")
                          settingsList.positionViewAtIndex(i - 1, ListView.Contain)
                        settingsList.positionViewAtIndex(i, ListView.Contain)
                        return
                      }
                  })
                }
              }

              delegate: Item {
                id: setCell
                required property string rowType
                required property string rowId
                required property string title
                width: settingsList.width
                height: rowType === "header" ? 28 : 46
                GroupHeader {
                  visible: setCell.rowType === "header"
                  title: setCell.title
                  width: parent.width
                }
                SettingRow {
                  readonly property var setting: setCell.rowType === "setting" ? SettingsModel.byId(setCell.rowId.slice(4)) : null
                  visible: setCell.rowType === "setting"
                  width: parent.width
                  entry: setting
                  accessory: setting ? SettingsModel.accessory(setting, settingsStore.ctx) : ""
                  selected: root.selection === setCell.rowId
                  onClicked: {
                    root.selection = setCell.rowId
                    root.navigated = true
                    root.pickSelected()
                    query.forceActiveFocus()
                  }
                  onOpened: root.settingPrimary()
                }
              }
            }

            Rectangle {
              width: 1
              height: parent.height
              color: Qt.rgba(Color.foreground.r, Color.foreground.g, Color.foreground.b, 0.14)
            }

            Item {
              width: parent.width - root.listWidth - 1
              height: parent.height

              KeyLockPane {
                anchors.fill: parent
                visible: root.view === "keylock"
                row: root.selectedType === "duration" ? root.selectedRow : null
              }
              SettingDetail {
                id: settingDetail
                anchors.fill: parent
                visible: root.view === "settings"
                entry: root.selectedSetting
                store: settingsStore
                onDone: query.forceActiveFocus()
                onToast: function (text) {
                  toast.show(text)
                }
              }
              PreviewPane {
                anchors.fill: parent
                visible: root.view !== "settings" && root.view !== "keylock" && (root.selectedType === "row" || root.selectedType === "")
                preview: root.preview
                row: root.selectedType === "row" ? root.selectedRow : ({})
                query: query.text
              }
              AppPreview {
                anchors.fill: parent
                visible: root.selectedType === "app" || root.selectedType === "command"
                row: (root.selectedType === "app" || root.selectedType === "command") ? root.selectedRow : ({})
                now: root.now
              }
              ClipPreview {
                anchors.fill: parent
                visible: root.selectedType === "clip"
                row: root.selectedType === "clip" ? root.selectedRow : ({})
                content: root.clipContent
                now: root.now
                onOpenLink: function (url) {
                  Quickshell.execDetached(["omarchy-launch-browser", url])
                  root.dismiss()
                }
                onCopyText: function (text) {
                  client.calcCopy(text, "", "copy")
                  toast.show("Copied " + text)
                }
              }
            }
          }
        }

        Item {
          id: footer
          width: parent.width
          implicitHeight: root.phase === "empty" ? 0 : 30
          visible: root.phase !== "empty"

          Text {
            anchors.left: parent.left
            anchors.leftMargin: 16
            anchors.verticalCenter: parent.verticalCenter
            text: root.statusText
            color: Qt.rgba(Color.foreground.r, Color.foreground.g, Color.foreground.b, 0.62)
            font.pixelSize: Math.round(Style.font.body * 0.92)
          }

          Row {
            anchors.right: parent.right
            anchors.rightMargin: 16
            anchors.verticalCenter: parent.verticalCenter
            spacing: 6
            // `SearchPanelView.swift` keysText, with Ctrl for ⌘.
            Repeater {
              model: {
                if (root.view === "keylock")
                  return [["↵", "Lock"], ["esc", root.lockFromSearch ? "Back" : "Close"]]
                if (root.view === "settings") {
                  if (settingDetail.activeFocus)
                    return settingDetail.hints
                  var e = root.selectedSetting
                  var what = e ? SettingsModel.primary(e, settingsStore.ctx) : ""
                  var h = []
                  if (what === "toggle")
                    h = [["↵", "Toggle"]]
                  else if (what === "action") {
                    var act = SettingsModel.actionsFor(e, settingsStore.ctx)[0]
                    h = [["↵", settingDetail.armed === act.id ? "Confirm" : act.label]]
                  }
                  else if (e)
                    h = [["↵", "Edit"]]
                  if (e && e.type === "choice" && !query.text.length)
                    h = [["← →", "Change"]].concat(h)
                  return h.concat([["tab", "Edit"], ["esc", root.settingsFromSearch ? "Back" : "Close"]])
                }
                switch (root.selectedType) {
                case "calc":
                  return [["↵", "Copy"], ["ctrl ↵", "Paste"], ["ctrl K", "Actions"]]
                case "app":
                  return [["↵", "Open"], ["ctrl K", "Actions"]]
                case "command":
                  return [["↵", "Open"]]
                case "clip":
                  return [["↵", "Paste"], ["ctrl ↵", "Copy"], ["ctrl del", "Remove"], ["ctrl K", "Actions"]].concat(root.clipFromSearch ? [["esc", "Back"]] : [])
                case "row":
                  return [["↵", "Open"], ["ctrl ↵", "Reveal"], ["space", "Quick Look"], ["ctrl K", "Actions"]]
                default:
                  return [["↵", "Open"], ["ctrl ↵", "Reveal"], ["space", "Quick Look"], ["ctrl C", "Copy"]]
                }
              }
              delegate: Row {
                spacing: 4
                anchors.verticalCenter: parent ? parent.verticalCenter : undefined
                Keycap {
                  label: modelData[0]
                  size: 11
                }
                Text {
                  text: modelData[1]
                  color: Qt.rgba(Color.foreground.r, Color.foreground.g, Color.foreground.b, 0.62)
                  font.pixelSize: 11
                  anchors.verticalCenter: parent.verticalCenter
                  rightPadding: 6
                }
              }
            }
          }
        }
        }
      }
    }
  }

  Process {
    id: appProc
    command: ["quickshell", "-p", Qt.resolvedUrl("../app").toString().replace(/^file:\/\//, "")]
    running: false
  }

  Process {
    id: lookProc
    command: ["env", "TROWEL_SURFACE=quicklook", "quickshell", "-p", Qt.resolvedUrl("../app").toString().replace(/^file:\/\//, "")]
    running: false
  }
}
