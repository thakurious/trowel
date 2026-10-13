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
  property bool navigated: false
  property bool showWeaker: false
  property string selection: ""
  property string phase: "empty"
  property bool codeMode: false
  property var preview: ({})
  property var selectedRow: null
  property bool configKnown: false

  readonly property bool needsOnboarding: root.configKnown && client.config && client.config.onboarded === false
  readonly property string pluginId: (manifest && manifest.id) ? String(manifest.id) : "io.github.thakurious.trowel"
  readonly property int panelWidth: 780
  readonly property int listWidth: 330
  readonly property real topFraction: 0.16
  readonly property int maxPanelHeight: Math.min(560, Math.round(panel.height * 0.6))

  function open(payloadJson) {
    root.opened = true
    root.navigated = false
    if (root.needsOnboarding)
      return
    query.queryFieldForce()
    client.panelOpened()
    Qt.callLater(function () {
      query.forceActiveFocus()
    })
  }

  function close() {
    root.opened = false
  }

  function dismiss() {
    root.opened = false
    if (root.shell && typeof root.shell.hide === "function")
      root.shell.hide(root.pluginId)
  }

  function toggle() {
    if (root.opened)
      root.dismiss()
    else
      root.open("{}")
  }

  function queryFieldForce() {
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
    onLineEvent: function (msg) {
      if (msg.type && (msg.file || msg.lines || msg.text || msg.peaks || msg.frame))
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

  PanelWindow {
    id: panel
    visible: root.opened
    color: "transparent"
    exclusionMode: ExclusionMode.Ignore
    WlrLayershell.namespace: "trowel-panel"
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.keyboardFocus: visible ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None
    anchors {
      top: true
      bottom: true
      left: true
      right: true
    }

    MouseArea {
      anchors.fill: parent
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
      height: root.needsOnboarding ? Math.min(root.maxPanelHeight, Math.max(360, onboard.implicitHeight)) : (root.phase === "empty" ? Math.min(root.maxPanelHeight, fieldCol.implicitHeight + 8) : root.maxPanelHeight)
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
          visible: root.needsOnboarding
          anchors.fill: parent
          client: client
          onFinished: {
            root.configKnown = true
            client.configGet()
            query.forceActiveFocus()
            client.panelOpened()
          }
        }

        Column {
          id: searchCol
          visible: !root.needsOnboarding
          width: parent.width

        Column {
          id: fieldCol
          width: parent.width

          QueryField {
            id: query
            width: parent.width
            codeMode: root.codeMode
            hybridPending: root.phase === "words"
            countText: client.appliedGen > 0 && root.phase !== "empty" ? String(Math.max(0, client.rows.count)) : ""
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
            text: root.phase === "words" ? "Matching words…" : (client.weaker.length ? (client.groups.length + " groups") : "")
            color: Qt.rgba(Color.foreground.r, Color.foreground.g, Color.foreground.b, 0.62)
            font.pixelSize: Math.round(Style.font.body * 0.92)
          }

          Row {
            anchors.right: parent.right
            anchors.rightMargin: 16
            anchors.verticalCenter: parent.verticalCenter
            spacing: 6
            Keycap {
              label: "↵"
              size: 11
            }
            Text {
              text: "Open"
              color: Qt.rgba(Color.foreground.r, Color.foreground.g, Color.foreground.b, 0.62)
              font.pixelSize: 11
              anchors.verticalCenter: parent.verticalCenter
            }
            Keycap {
              label: "esc"
              size: 11
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
