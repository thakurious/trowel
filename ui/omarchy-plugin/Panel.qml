import QtQuick
import QtQuick.Controls
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import qs.Commons
import qs.Ui
import "Trowel"

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

  // Footer, left: `upstream/Sources/DigUpApp/SearchPanelView.swift:131` statusText.
  readonly property string statusText: {
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
    var st = client.status || {}
    if (st.paused_reason)
      return "Indexing paused"
    var main = st.main || {}
    if ((st.phase === "indexing" || st.phase === "syncing") && main.total > 0)
      return "Indexing " + Math.floor((main.done || 0) / main.total * 100) + "%"
    return ""
  }

  readonly property bool needsOnboarding: root.wizardOpen || (root.configKnown && client.config && client.config.onboarded === false)
  readonly property string pluginId: (manifest && manifest.id) ? String(manifest.id) : "io.github.thakurious.trowel"
  readonly property int panelWidth: 780
  readonly property int listWidth: 330
  readonly property real topFraction: 0.16
  readonly property int maxPanelHeight: Math.min(560, Math.round(panel.height * 0.6))

  function open(payloadJson) {
    unmapTimer.stop()
    root.mapped = true
    root.opened = true
    root.navigated = false
    if (root.needsOnboarding)
      return
    client.panelOpened()
    Qt.callLater(function () {
      query.forceActiveFocus()
      query.selectAll()
    })
  }

  function close() {
    root.opened = false
    unmapTimer.restart()
  }

  function dismiss() {
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
    var raw = query.text
    var trimmed = raw.replace(/^\s+|\s+$/g, "")
    root.codeMode = trimmed.indexOf("code:") === 0
    var text = root.codeMode ? trimmed.replace(/^code:\s*/, "") : trimmed
    if (trimmed.length === 0) {
      root.phase = "empty"
      client.groups = []
      client.weaker = []
      client.syncModel()
      hybridTimer.stop()
      return
    }
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

  function runHybrid() {
    var trimmed = query.text.replace(/^\s+|\s+$/g, "")
    var text = root.codeMode ? trimmed.replace(/^code:\s*/, "") : trimmed
    if (text.length === 0)
      return
    client.search(text, "hybrid", root.codeMode, false)
  }

  function toggleCodeMode() {
    var t = query.text
    if (t.indexOf("code:") === 0)
      query.text = t.replace(/^code:\s*/, "")
    else
      query.text = "code: " + t
    onQueryEdited()
  }

  function moveSelection(step) {
    var ids = []
    for (var i = 0; i < client.rows.count; i++) {
      var it = client.rows.get(i)
      if (it.rowType === "row" || it.rowType === "fold")
        ids.push(it.rowId)
    }
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

  function openSelected() {
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
    if (root.selectedRow)
      client.revealRow(root.selectedRow)
    dismiss()
  }

  IpcHandler {
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
      if (meaning === "used")
        root.phase = "meaning"
      else if (meaning === "unavailable")
        root.phase = "wordsOnly"
      if (!root.navigated || !selectionStillPresent()) {
        for (var i = 0; i < rows.count; i++) {
          if (rows.get(i).rowType === "row") {
            root.selection = rows.get(i).rowId
            break
          }
        }
      }
      pickSelected()
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
    for (var i = 0; i < client.rows.count; i++) {
      if (client.rows.get(i).rowId === root.selection)
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
            hybridPending: root.phase === "words"
            countText: root.activityBadge
            placeholder: root.codeMode ? "Search your code" : "Describe it: “payment error screenshot”, “zebra in a video”…"
            onTextEdited: root.onQueryEdited()
            onKeyPressed: function (event) {
              var ctrl = event.modifiers & Qt.ControlModifier
              if (query.composing() && (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Up || event.key === Qt.Key_Down || event.key === Qt.Key_Escape))
                return
              if (event.key === Qt.Key_Escape) {
                if (query.text.length > 0)
                  query.text = ""
                else
                  root.dismiss()
                event.accepted = true
              } else if (event.key === Qt.Key_Down || (ctrl && event.key === Qt.Key_N)) {
                root.moveSelection(1)
                event.accepted = true
              } else if (event.key === Qt.Key_Up || (ctrl && event.key === Qt.Key_P)) {
                root.moveSelection(-1)
                event.accepted = true
              } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                if (ctrl)
                  root.revealSelected()
                else
                  root.openSelected()
                event.accepted = true
              } else if (event.key === Qt.Key_Space && (root.navigated) && root.selection.length) {
                if (root.selection === client.weakerId) {
                  client.showWeaker = !client.showWeaker
                  root.showWeaker = client.showWeaker
                } else
                  root.quickLookSelected()
                event.accepted = true
              } else if (ctrl && event.key === Qt.Key_Y) {
                root.quickLookSelected()
                event.accepted = true
              } else if (ctrl && event.key === Qt.Key_C && root.navigated) {
                client.copyPath(root.selectedRow)
                event.accepted = true
              } else if (ctrl && event.key === Qt.Key_O) {
                appProc.running = true
                event.accepted = true
              } else if (event.key === Qt.Key_Tab) {
                root.toggleCodeMode()
                event.accepted = true
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

          EmptyState {
            visible: client.rows.count === 0
            anchors.centerIn: parent
            width: parent.width - 48
            title: {
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
              if (root.phase === "words")
                return ""
              if (root.codeMode)
                return "Try saying what the code does, or a name that's in it."
              return "Try what you'd see or hear in it, or words you remember from it."
            }
          }

          Row {
            visible: client.rows.count > 0
            anchors.fill: parent

            ListView {
              id: list
              width: root.listWidth
              height: parent.height
              model: client.rows
              clip: true
              reuseItems: true
              cacheBuffer: 240
              highlightMoveDuration: Motion.highlightMs
              highlightResizeDuration: 0
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
                  title: title
                  width: parent.width
                }
                WeakerFold {
                  visible: rowType === "fold"
                  width: parent.width
                  count: count
                  expanded: client.showWeaker
                  selected: root.selection === rowId
                  onToggled: {
                    client.showWeaker = !client.showWeaker
                    root.showWeaker = client.showWeaker
                    root.selection = client.weakerId
                    root.navigated = true
                  }
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

            Rectangle {
              width: 1
              height: parent.height
              color: Qt.rgba(Color.foreground.r, Color.foreground.g, Color.foreground.b, 0.14)
            }

            PreviewPane {
              width: parent.width - root.listWidth - 1
              height: parent.height
              preview: root.preview
              row: root.selectedRow
              query: query.text
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
              model: [["↵", "Open"], ["ctrl ↵", "Reveal"], ["space", "Quick Look"], ["ctrl C", "Copy"]]
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
