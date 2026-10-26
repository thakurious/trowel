import QtQuick
import Quickshell
import Quickshell.Io
import "SettingsModel.js" as SettingsModel

// The Settings view's right pane (DECISIONS D26): the selected setting and a live control for it.
// It works with the mouse, and from the keyboard once the list hands it the focus (Tab or ↵):
// ↑/↓ move the cursor, ↵/Space act, ←/→ change a choice or a switch, ⌫ twice removes an entry,
// Esc goes back to the list (`done`). Adding to a list opens a field: Tab completes a folder,
// ↓ walks the suggestions, ↵ adds, Ctrl+O opens the system folder picker, Esc cancels.
FocusScope {
  id: root
  property var entry: null
  required property var store
  // The panel shows these in its footer while the pane has the focus.
  readonly property var hints: root.hintsFor()

  property int cursor: 0
  property bool adding: false
  property bool recording: false
  property string armed: ""
  property string error: ""
  property var completions: []

  readonly property bool focused: root.activeFocus
  readonly property var ctx: root.store.ctx
  readonly property var current: root.entry ? SettingsModel.value(root.entry, root.ctx) : undefined
  readonly property var items: root.itemsFor()
  readonly property int fieldIndex: {
    for (var i = 0; i < root.items.length; i++)
      if (root.items[i].kind === "field")
        return i
    return -1
  }

  signal done
  signal toast(string text)

  onEntryChanged: root.reset()

  function reset() {
    root.adding = false
    root.recording = false
    root.armed = ""
    root.error = ""
    root.completions = []
    root.cursor = 0
    root.cursorToValue()
    flick.contentY = 0
  }

  // On a choice, the cursor starts at the value in use.
  function cursorToValue() {
    if (root.entry && root.entry.type === "choice") {
      var opts = SettingsModel.optionsFor(root.entry, root.ctx)
      for (var i = 0; i < opts.length; i++)
        if (opts[i].value === root.current)
          root.cursor = i
    }
  }

  // Called by the panel (Tab / ↵ on the row): take the keyboard.
  function enter() {
    root.cursorToValue()
    root.forceActiveFocus()
    if (root.entry && root.entry.type === "list" && root.items.length === 1 && root.items[0].kind === "add")
      root.startAdd()
  }

  function itemsFor() {
    var e = root.entry
    if (!e)
      return []
    var out = []
    if (e.type === "toggle")
      return [{
          "kind": "toggle"
        }]
    if (e.type === "hotkey")
      return [{
          "kind": "hotkey"
        }]
    if (e.type === "choice") {
      var opts = SettingsModel.optionsFor(e, root.ctx)
      for (var i = 0; i < opts.length; i++)
        out.push({
          "kind": "option",
          "label": opts[i].label,
          "value": opts[i].value
        })
      return out
    }
    if (e.type === "list") {
      var list = root.current || []
      for (var j = 0; j < list.length; j++)
        out.push({
          "kind": "entry",
          "value": list[j],
          "label": root.prettyItem(list[j]),
          "problem": root.problemFor(list[j])
        })
      if (e.noAdd)
        return out
      out.push({
        "kind": root.adding ? "field" : "add"
      })
      if (root.adding)
        for (var k = 0; k < root.suggestions().length; k++)
          out.push({
            "kind": "suggest",
            "value": root.suggestions()[k],
            "label": root.prettyItem(root.suggestions()[k])
          })
      return out
    }
    var acts = SettingsModel.actionsFor(e, root.ctx)
    for (var a = 0; a < acts.length; a++)
      out.push({
        "kind": "button",
        "id": acts[a].id,
        "label": acts[a].label,
        "danger": !!acts[a].danger
      })
    return out
  }

  function hintsFor() {
    var e = root.entry
    if (!e)
      return []
    if (root.recording)
      return [["keys", "New shortcut"], ["esc", "Cancel"]]
    if (root.adding && root.items[root.cursor] && root.items[root.cursor].kind === "field") {
      if (e.mode === "folder")
        return [["↵", "Add"], ["tab", "Complete"], ["ctrl O", "Browse"], ["esc", "Cancel"]]
      return [["↵", "Add"], ["↓", "Suggestions"], ["esc", "Cancel"]]
    }
    var it = root.items[root.cursor] || {}
    var h = []
    if (it.kind === "option")
      h = [["↑↓", "Move"], ["↵", "Choose"]]
    else if (it.kind === "toggle")
      h = [["↵", "Toggle"], ["←→", "Off / On"]]
    else if (it.kind === "hotkey")
      h = [["↵", "Record"]]
    else if (it.kind === "entry")
      h = (e.mode === "folder" ? [["↵", "Open"]] : []).concat([["⌫", "Remove"]])
    else if (it.kind === "add")
      h = [["↵", e.mode === "folder" ? "Add Folder" : "Add"]].concat(e.mode === "folder" ? [["ctrl O", "Browse"]] : [])
    else if (it.kind === "suggest")
      h = [["↵", "Add"]].concat(e.mode === "folder" ? [["tab", "Open"]] : [])
    else if (it.kind === "button")
      h = [["↵", root.armed === it.id ? "Confirm" : it.label]]
    return h.concat([["esc", "Back"]])
  }

  function prettyItem(v) {
    if (!root.entry)
      return String(v)
    if (root.entry.mode === "folder")
      return root.store.pretty(v)
    if (root.entry.mode === "ext")
      return "." + v
    return String(v)
  }

  function problemFor(v) {
    if (!root.entry || root.entry.id !== "roots")
      return ""
    var st = root.store.status || {}
    if ((st.missing_roots || []).indexOf(v) >= 0)
      return "Missing. Trowel picks it up again when it's back."
    if ((st.unreadable_roots || []).indexOf(v) >= 0)
      return "Can't be read. Check its permissions."
    return ""
  }

  // Under the field: folder completions, or apps open right now that match what's typed.
  function suggestions() {
    var e = root.entry
    if (!e || !root.adding)
      return []
    var q = String(field.text || "").toLowerCase()
    var pool = e.mode === "folder" ? root.completions : (e.mode === "app" ? root.store.runningApps : [])
    return pool.filter(function (s) {
      if (root.has(s))
        return false
      return e.mode === "folder" || !q.length || String(s).toLowerCase().indexOf(q) >= 0
    }).slice(0, 8)
  }

  function moveCursor(step) {
    var n = root.items.length
    if (!n)
      return
    root.armed = ""
    root.cursor = Math.max(0, Math.min(n - 1, root.cursor + step))
    var it = root.items[root.cursor]
    if (it && it.kind === "field")
      field.forceActiveFocus()
    else
      root.forceActiveFocus()
    root.revealCursor()
  }

  function cellAt(i) {
    if (root.fieldIndex < 0 || i < root.fieldIndex)
      return repA.itemAt(i)
    if (i === root.fieldIndex)
      return fieldCell
    return repB.itemAt(i - root.fieldIndex - 1)
  }

  function revealCursor() {
    Qt.callLater(function () {
      var d = root.cellAt(root.cursor)
      if (!d)
        return
      var y = d.mapToItem(flick.contentItem, 0, 0).y
      if (y < flick.contentY + 8)
        flick.contentY = Math.max(0, y - 8)
      else if (y + d.height > flick.contentY + flick.height - 8)
        flick.contentY = Math.min(flick.contentHeight - flick.height, y + d.height - flick.height + 8)
    })
  }

  function activate(it) {
    var e = root.entry
    if (!it || !e)
      return
    if (it.kind === "toggle")
      root.store.toggle(e)
    else if (it.kind === "hotkey")
      root.recording = true
    else if (it.kind === "option")
      root.store.set(e, it.value)
    else if (it.kind === "entry") {
      if (e.mode === "folder")
        Quickshell.execDetached(["xdg-open", String(it.value)])
    } else if (it.kind === "add")
      root.startAdd()
    else if (it.kind === "field")
      root.submit(field.text)
    else if (it.kind === "suggest")
      root.submit(it.value)
    else if (it.kind === "button") {
      if (it.danger && root.armed !== it.id) {
        root.armed = it.id
        disarm.restart()
        return
      }
      root.armed = ""
      root.store.run(it.id)
    }
  }

  function removeAt(i) {
    var it = root.items[i]
    if (!it || it.kind !== "entry")
      return
    var key = "rm:" + it.value
    if (root.armed !== key) {
      root.armed = key
      disarm.restart()
      return
    }
    root.armed = ""
    var next = (root.current || []).filter(function (x) {
      return x !== it.value
    })
    root.store.set(root.entry, next)
    root.toast("Removed " + it.label)
    root.cursor = Math.max(0, Math.min(i, next.length))
  }

  function startAdd() {
    root.error = ""
    root.completions = []
    field.text = root.entry.mode === "folder" ? "~/" : ""
    root.adding = true
    Qt.callLater(function () {
      for (var i = 0; i < root.items.length; i++)
        if (root.items[i].kind === "field")
          root.cursor = i
      field.forceActiveFocus()
      if (root.entry.mode === "folder")
        root.complete(false)
    })
  }

  function stopAdd() {
    root.adding = false
    root.error = ""
    root.completions = []
    for (var i = 0; i < root.items.length; i++)
      if (root.items[i].kind === "add")
        root.cursor = i
    root.forceActiveFocus()
  }

  // A folder written as ~/x and as /home/me/x is the same folder.
  function has(v) {
    var norm = function (x) {
      return (root.entry && root.entry.mode === "folder" ? root.store.expand(String(x)).replace(/\/+$/, "") : String(x)).toLowerCase()
    }
    return (root.current || []).some(function (x) {
      return norm(x) === norm(v)
    })
  }

  function submit(text) {
    var e = root.entry
    var v = String(text || "").replace(/^\s+|\s+$/g, "")
    if (e.mode === "folder") {
      v = root.store.expand(v)
      if (v.length > 1)
        v = v.replace(/\/+$/, "")
    } else if (e.mode === "ext")
      v = v.replace(/^\*?\./, "").toLowerCase()
    if (!v.length)
      return
    if (root.has(v)) {
      root.error = "Already in the list"
      return
    }
    if (e.mode === "ext" && !/^[a-z0-9][a-z0-9_+-]{0,15}$/.test(v)) {
      root.error = "Type an extension, like log"
      return
    }
    if (e.mode === "folder") {
      check.candidate = v
      check.command = ["test", "-d", v]
      check.running = true
      return
    }
    root.commit(v)
  }

  function commit(v) {
    var n = (root.current || []).length + 1
    root.store.set(root.entry, (root.current || []).concat([v]))
    root.toast("Added " + root.prettyItem(v))
    root.stopAdd()
    // The config reply lands after this: keep the cursor on "Add…", one past the new entry.
    root.cursor = n
  }

  // Tab: complete the folder path as far as it is unambiguous; the matches become suggestions.
  function complete(fill) {
    completer.fill = fill
    // Hidden folders only when the name typed starts with a dot, as in a shell.
    completer.command = ["bash", "-c", "compgen -d -- \"$1\" | { case \"${1##*/}\" in .*) cat ;; *) grep -v '/\\.[^/]*$' ;; esac; } | LC_ALL=C sort | head -n 40", "trowel", root.store.expand(field.text)]
    completer.running = true
  }

  // SUPER is added when no modifier was held (UI-DESIGN §8: a bare key is never a global bind).
  function chordOf(event) {
    var k = event.key
    if (k === Qt.Key_Control || k === Qt.Key_Shift || k === Qt.Key_Alt || k === Qt.Key_Meta || k === Qt.Key_Super_L || k === Qt.Key_Super_R)
      return ""
    var name = ""
    if (k >= Qt.Key_A && k <= Qt.Key_Z)
      name = String.fromCharCode(k)
    else if (k >= Qt.Key_0 && k <= Qt.Key_9)
      name = String.fromCharCode(k)
    else if (k >= Qt.Key_F1 && k <= Qt.Key_F12)
      name = "F" + (k - Qt.Key_F1 + 1)
    else if (k === Qt.Key_Space)
      name = "SPACE"
    else if (k === Qt.Key_Return || k === Qt.Key_Enter)
      name = "RETURN"
    else if (k === Qt.Key_Slash)
      name = "SLASH"
    else if (k === Qt.Key_Period)
      name = "PERIOD"
    else if (k === Qt.Key_Comma)
      name = "COMMA"
    if (!name.length)
      return ""
    var parts = []
    if (event.modifiers & Qt.MetaModifier)
      parts.push("SUPER")
    if (event.modifiers & Qt.ControlModifier)
      parts.push("CTRL")
    if (event.modifiers & Qt.AltModifier)
      parts.push("ALT")
    if (event.modifiers & Qt.ShiftModifier)
      parts.push("SHIFT")
    if (!parts.length)
      parts.push("SUPER")
    parts.push(name)
    return parts.join(" + ")
  }

  Keys.onPressed: function (event) {
    var e = root.entry
    if (!e)
      return
    var ctrl = event.modifiers & Qt.ControlModifier
    var it = root.items[root.cursor] || {}
    if (root.recording) {
      if (event.key === Qt.Key_Escape) {
        root.recording = false
      } else {
        var chord = root.chordOf(event)
        if (!chord.length)
          return
        root.recording = false
        root.store.setHotkey(chord)
      }
      event.accepted = true
      return
    }
    if (event.key === Qt.Key_Escape || event.key === Qt.Key_Backtab) {
      root.done()
    } else if (event.key === Qt.Key_Down || (ctrl && event.key === Qt.Key_N)) {
      root.moveCursor(1)
    } else if (event.key === Qt.Key_Up || (ctrl && event.key === Qt.Key_P)) {
      root.moveCursor(-1)
    } else if (event.key === Qt.Key_Home || event.key === Qt.Key_End) {
      root.moveCursor(event.key === Qt.Key_Home ? -1000 : 1000)
    } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space) {
      root.activate(it)
    } else if (event.key === Qt.Key_Left || event.key === Qt.Key_Right) {
      var step = event.key === Qt.Key_Left ? -1 : 1
      if (e.type === "choice") {
        var v = SettingsModel.stepChoice(e, root.ctx, step)
        if (v !== root.current) {
          root.store.set(e, v)
          root.cursor = Math.max(0, Math.min(root.items.length - 1, root.cursor + step))
        }
      } else if (e.type === "toggle") {
        if (root.current !== (step > 0))
          root.store.set(e, step > 0)
      } else if (step < 0) {
        root.done()
      }
    } else if ((event.key === Qt.Key_Delete || event.key === Qt.Key_Backspace) && it.kind === "entry") {
      root.removeAt(root.cursor)
    } else if (event.key === Qt.Key_Tab && it.kind === "suggest" && e.mode === "folder") {
      field.text = root.store.pretty(it.value) + "/"
      root.moveCursor(-1000)
      root.startAdd()
      field.text = root.store.pretty(it.value) + "/"
    } else if (ctrl && event.key === Qt.Key_O && e.mode === "folder") {
      picker.running = true
    } else {
      return
    }
    event.accepted = true
  }

  Timer {
    id: disarm
    interval: 3000
    onTriggered: root.armed = ""
  }

  Rectangle {
    anchors.fill: parent
    color: root.focused ? Theme.alpha(Theme.accent, 0.035) : "transparent"
  }

  Flickable {
    id: flick
    anchors.fill: parent
    contentHeight: col.implicitHeight + 32
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    visible: !!root.entry

    Column {
      id: col
      x: 20
      y: 18
      width: flick.width - 40
      spacing: 14

      // Header: glyph tile, title, section.
      Row {
        spacing: 12
        Rectangle {
          width: 38
          height: 38
          radius: Theme.innerRadius(Theme.panelRadius()) + 3
          color: Theme.alpha(Theme.accent, 0.16)
          Text {
            anchors.centerIn: parent
            text: root.entry ? root.entry.glyph : ""
            color: Theme.accent
            font.family: Theme.monoFamily
            font.pixelSize: 17
          }
        }
        Column {
          anchors.verticalCenter: parent.verticalCenter
          spacing: 2
          Text {
            text: root.entry ? root.entry.title : ""
            color: Theme.foreground
            font.family: Theme.fontFamily
            font.pixelSize: Theme.rowTitleSize + 3
            font.weight: Font.DemiBold
          }
          Text {
            text: root.entry ? root.entry.section : ""
            color: Theme.secondary
            font.family: Theme.fontFamily
            font.pixelSize: Theme.captionSize
          }
        }
      }

      Text {
        width: col.width
        wrapMode: Text.Wrap
        text: root.entry ? root.entry.detail : ""
        color: Theme.secondary
        font.family: Theme.fontFamily
        font.pixelSize: Theme.captionSize + 1
        lineHeight: 1.2
      }

      // Read-only lines of an info entry.
      Column {
        visible: lineRep.count > 0
        width: col.width
        spacing: 0
        Repeater {
          id: lineRep
          model: root.entry && root.entry.type === "info" ? SettingsModel.infoLines(root.entry, root.ctx) : []
          delegate: Item {
            id: line
            required property var modelData
            width: col.width
            height: Math.max(28, lineValue.implicitHeight + 10)
            Rectangle {
              width: parent.width
              height: Theme.hairlineWidth()
              anchors.bottom: parent.bottom
              color: Theme.hairline
            }
            Row {
              anchors.left: parent.left
              anchors.verticalCenter: parent.verticalCenter
              spacing: 8
              Rectangle {
                visible: !!line.modelData.level
                anchors.verticalCenter: parent.verticalCenter
                width: 7
                height: 7
                radius: 3.5
                color: line.modelData.level === "OK" ? Theme.brandLeaf : (line.modelData.level === "WARN" ? Theme.brandAmber : Theme.urgent)
              }
              Text {
                text: line.modelData.label
                color: Theme.foreground
                font.family: Theme.fontFamily
                font.pixelSize: Theme.captionSize + 1
              }
            }
            Text {
              id: lineValue
              anchors.right: parent.right
              anchors.verticalCenter: parent.verticalCenter
              width: Math.min(implicitWidth, parent.width * 0.62)
              horizontalAlignment: Text.AlignRight
              elide: Text.ElideMiddle
              text: line.modelData.value
              color: Theme.secondary
              font.family: Theme.fontFamily
              font.pixelSize: Theme.captionSize + 1
            }
          }
        }
      }

      Text {
        visible: !!root.entry && root.entry.id === "health" && lineRep.count === 0
        text: "Checking…"
        color: Theme.secondary
        font.family: Theme.fontFamily
        font.pixelSize: Theme.captionSize + 1
      }

      Text {
        visible: root.entry && root.entry.type === "list" && (root.current || []).length === 0 && !root.adding
        width: col.width
        wrapMode: Text.Wrap
        text: root.entry ? (root.entry.empty || "") : ""
        color: Theme.secondary
        font.family: Theme.fontFamily
        font.pixelSize: Theme.captionSize + 1
        font.italic: true
      }

      // The control: every item the cursor can reach. The add field is not a delegate: the
      // items change as you type (suggestions), and a rebuilt delegate would drop its focus.
      Column {
        width: col.width
        spacing: 2
        Repeater {
          id: repA
          model: root.items.slice(0, root.fieldIndex < 0 ? root.items.length : root.fieldIndex)
          delegate: Cell {
            offset: 0
          }
        }
        Rectangle {
          id: fieldCell
          visible: root.adding
          width: col.width
          height: visible ? fieldBox.implicitHeight + 18 : 0
          radius: Theme.innerRadius(Theme.panelRadius()) + 2
          color: Theme.keycapFill
          border.width: 1
          border.color: Theme.alpha(Theme.accent, field.activeFocus ? 0.7 : 0.35)
          MouseArea {
            anchors.fill: parent
            cursorShape: Qt.IBeamCursor
            onClicked: {
              root.cursor = root.fieldIndex
              field.forceActiveFocus()
            }
          }
          Column {
            id: fieldBox
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.leftMargin: 12
            anchors.rightMargin: 12
            anchors.verticalCenter: parent.verticalCenter
            spacing: 6
            Row {
              width: parent.width
              spacing: 10
              Text {
                anchors.verticalCenter: parent.verticalCenter
                width: 16
                horizontalAlignment: Text.AlignHCenter
                text: "\uf067"
                color: Theme.accent
                font.family: Theme.monoFamily
                font.pixelSize: Theme.captionSize + 1
              }
              TextInput {
                id: field
                width: parent.width - 26
                anchors.verticalCenter: parent.verticalCenter
                color: Theme.foreground
                selectionColor: Theme.alpha(Theme.accent, 0.4)
                font.family: Theme.monoFamily
                font.pixelSize: Theme.rowTitleSize
                clip: true
                onTextEdited: {
                  root.error = ""
                  if (root.entry && root.entry.mode === "folder")
                    completeTimer.restart()
                }
                onActiveFocusChanged: {
                  if (activeFocus && root.fieldIndex >= 0)
                    root.cursor = root.fieldIndex
                }
                Text {
                  visible: !field.text.length
                  text: root.entry && root.entry.mode === "ext" ? "log" : (root.entry && root.entry.mode === "app" ? "org.keepassxc.KeePassXC" : "~/")
                  color: Theme.alpha(Theme.foreground, 0.32)
                  font: field.font
                }
                Keys.onPressed: function (event) {
                  if (event.key === Qt.Key_Escape) {
                    root.stopAdd()
                  } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                    root.submit(field.text)
                  } else if (event.key === Qt.Key_Tab) {
                    if (root.entry && root.entry.mode === "folder")
                      root.complete(true)
                  } else if (event.key === Qt.Key_Down) {
                    root.moveCursor(1)
                  } else if (event.key === Qt.Key_Up) {
                    root.moveCursor(-1)
                  } else if (event.key === Qt.Key_O && (event.modifiers & Qt.ControlModifier) && root.entry && root.entry.mode === "folder") {
                    picker.running = true
                  } else {
                    return
                  }
                  event.accepted = true
                }
              }
            }
            Text {
              width: parent.width
              leftPadding: 26
              wrapMode: Text.Wrap
              text: {
                if (root.error.length)
                  return root.error
                if (!root.entry)
                  return ""
                if (root.entry.mode === "folder")
                  return "Tab completes · ↓ suggestions · ↵ adds · Ctrl+O browses · Esc cancels"
                if (root.entry.mode === "app")
                  return "A window class, or ↓ for an app that's open · Esc cancels"
                return "An extension, like log · Esc cancels"
              }
              color: root.error.length ? Theme.urgent : Theme.secondary
              font.family: Theme.fontFamily
              font.pixelSize: Theme.captionSize
            }
          }
        }
        Repeater {
          id: repB
          model: root.fieldIndex < 0 ? [] : root.items.slice(root.fieldIndex + 1)
          delegate: Cell {
            offset: root.fieldIndex + 1
          }
        }
      }
    }
  }

  component Cell: Rectangle {
    id: cell
    required property var modelData
    required property int index
    property int offset: 0
    readonly property int at: cell.index + cell.offset
    readonly property bool here: root.focused && root.cursor === cell.at
    readonly property bool chosen: cell.modelData.kind === "option" && cell.modelData.value === root.current
    readonly property bool arming: root.armed === "rm:" + cell.modelData.value
    width: col.width
    height: (cell.modelData.kind === "toggle" || cell.modelData.kind === "hotkey" || cell.arming || !!cell.modelData.problem) ? 46 : 34
    radius: Theme.innerRadius(Theme.panelRadius()) + 2
    color: {
      if (cell.here)
        return Theme.selectedFill
      if (cell.modelData.kind === "toggle" || cell.modelData.kind === "hotkey")
        return Theme.keycapFill
      return mouse.containsMouse ? Theme.keycapFill : "transparent"
    }
    border.width: cell.here ? 1 : 0
    border.color: Theme.alpha(Theme.accent, 0.5)

    MouseArea {
      id: mouse
      anchors.fill: parent
      hoverEnabled: true
      cursorShape: Qt.PointingHandCursor
      onClicked: {
        root.cursor = cell.at
        root.forceActiveFocus()
        root.activate(cell.modelData)
      }
    }

    // option: radio + label
    Row {
      visible: cell.modelData.kind === "option"
      anchors.left: parent.left
      anchors.leftMargin: 12
      anchors.verticalCenter: parent.verticalCenter
      spacing: 10
      Rectangle {
        anchors.verticalCenter: parent.verticalCenter
        width: 14
        height: 14
        radius: 7
        color: "transparent"
        border.width: cell.chosen ? 4 : 1.5
        border.color: cell.chosen ? Theme.accent : Theme.alpha(Theme.foreground, 0.4)
      }
      Text {
        text: cell.modelData.label || ""
        color: cell.chosen ? Theme.foreground : Theme.secondary
        font.family: Theme.fontFamily
        font.pixelSize: Theme.rowTitleSize
        font.weight: cell.chosen ? Font.Medium : Font.Normal
      }
    }

    // toggle: state + switch
    Item {
      visible: cell.modelData.kind === "toggle"
      anchors.fill: parent
      anchors.leftMargin: 14
      anchors.rightMargin: 14
      Text {
        anchors.left: parent.left
        anchors.verticalCenter: parent.verticalCenter
        text: root.current ? "On" : "Off"
        color: Theme.foreground
        font.family: Theme.fontFamily
        font.pixelSize: Theme.rowTitleSize + 1
        font.weight: Font.Medium
      }
      ToggleSwitch {
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        checked: !!root.current
      }
    }

    // hotkey: keycaps
    Item {
      visible: cell.modelData.kind === "hotkey"
      anchors.fill: parent
      anchors.leftMargin: 14
      anchors.rightMargin: 14
      Text {
        anchors.left: parent.left
        anchors.verticalCenter: parent.verticalCenter
        text: root.recording ? "Press a key (Super is added)…" : "Shortcut"
        color: root.recording ? Theme.accent : Theme.secondary
        font.family: Theme.fontFamily
        font.pixelSize: Theme.rowTitleSize
      }
      Row {
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        spacing: 4
        opacity: root.recording ? 0.35 : 1
        Repeater {
          model: String(root.current || "").split("+").map(function (s) {
            return s.replace(/^\s+|\s+$/g, "")
          }).filter(function (s) {
            return s.length
          })
          delegate: Keycap {
            required property var modelData
            label: modelData
            size: Theme.captionSize + 1
          }
        }
      }
    }

    // entry / suggest / add / button
    Row {
      visible: cell.modelData.kind === "entry" || cell.modelData.kind === "suggest" || cell.modelData.kind === "add" || cell.modelData.kind === "button"
      anchors.left: parent.left
      anchors.leftMargin: 12
      anchors.right: parent.right
      anchors.rightMargin: 12
      anchors.verticalCenter: parent.verticalCenter
      spacing: 10
      Text {
        anchors.verticalCenter: parent.verticalCenter
        width: 16
        horizontalAlignment: Text.AlignHCenter
        text: {
          var k = cell.modelData.kind
          if (k === "add")
            return "\uf067"
          if (k === "button")
            return cell.modelData.danger ? "\uf1f8" : "\uf054"
          if (root.entry && root.entry.mode === "folder")
            return "\uf07b"
          if (root.entry && root.entry.mode === "ext")
            return "\uf15b"
          return "\uf2d0"
        }
        color: cell.modelData.kind === "add" || (cell.modelData.kind === "button" && !cell.modelData.danger) ? Theme.accent : (cell.modelData.danger ? Theme.urgent : Theme.secondary)
        font.family: Theme.monoFamily
        font.pixelSize: Theme.captionSize + 1
      }
      Column {
        anchors.verticalCenter: parent.verticalCenter
        width: parent.width - 26
        spacing: 1
        Text {
          width: parent.width
          elide: Text.ElideMiddle
          text: {
            if (cell.modelData.kind === "add")
              return root.entry && root.entry.mode === "folder" ? "Add Folder…" : (root.entry && root.entry.mode === "ext" ? "Add File Type…" : "Add App…")
            if (cell.modelData.kind === "button" && root.armed === cell.modelData.id)
              return "Press ↵ again to " + cell.modelData.label.toLowerCase()
            return cell.modelData.label || ""
          }
          color: cell.modelData.kind === "button" && root.armed === cell.modelData.id ? Theme.urgent : (cell.modelData.kind === "suggest" ? Theme.secondary : Theme.foreground)
          font.family: cell.modelData.kind === "entry" && root.entry && root.entry.mode !== "folder" ? Theme.monoFamily : Theme.fontFamily
          font.pixelSize: Theme.rowTitleSize
          font.weight: cell.modelData.kind === "button" ? Font.Medium : Font.Normal
        }
        Text {
          visible: cell.modelData.kind === "entry" && (cell.arming || !!cell.modelData.problem)
          width: parent.width
          elide: Text.ElideRight
          text: cell.arming ? "Press ⌫ again to remove" : (cell.modelData.problem || "")
          color: Theme.urgent
          font.family: Theme.fontFamily
          font.pixelSize: Theme.captionSize
        }
      }
    }
  }

  Timer {
    id: completeTimer
    interval: 120
    onTriggered: root.complete(false)
  }

  Process {
    id: check
    property string candidate: ""
    onExited: function (code) {
      if (code === 0)
        root.commit(check.candidate)
      else
        root.error = "No folder at " + root.store.pretty(check.candidate)
    }
  }

  Process {
    id: completer
    property bool fill: false
    stdout: StdioCollector {
      onStreamFinished: {
        var list = String(text).split("\n").filter(function (l) {
          return l.length > 0
        })
        root.completions = list
        if (!completer.fill)
          return
        if (!list.length) {
          root.error = "No folder starts with that"
          return
        }
        var common = list[0]
        for (var i = 1; i < list.length; i++) {
          var j = 0
          while (j < common.length && j < list[i].length && common[j] === list[i][j])
            j++
          common = common.slice(0, j)
        }
        field.text = root.store.pretty(common) + (list.length === 1 ? "/" : "")
        if (list.length === 1)
          root.complete(false)
      }
    }
  }

  Process {
    id: picker
    command: ["trowel", "portal-pick-folder"]
    stdout: SplitParser {
      onRead: function (line) {
        var p = String(line).replace(/^\s+|\s+$/g, "")
        if (!p.length)
          return
        if (root.has(p)) {
          root.toast(root.store.pretty(p) + " is already in the list")
          return
        }
        root.adding = true
        root.commit(p)
      }
    }
  }

  // Nothing selected: a hint.
  Text {
    visible: !root.entry
    anchors.centerIn: parent
    text: "Pick a setting"
    color: Theme.secondary
    font.pixelSize: Theme.rowTitleSize
  }
}
