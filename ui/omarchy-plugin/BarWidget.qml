import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui
import "Trowel"

// StatusMenu.swift → bar-widget. Popup: search, open, pause, folders, settings, quit.
BarWidget {
  id: root
  moduleName: (manifest && manifest.id) ? String(manifest.id) : "io.github.thakurious.trowel"

  property var manifest: null
  property var shell: null
  readonly property bool opened: popup.open
  property real now: Date.now() / 1000

  Timer {
    interval: 30000
    repeat: true
    running: client.clipPausedUntil > 0
    onTriggered: root.now = Date.now() / 1000
  }

  Client {
    id: client
    clientName: "bar"
  }

  function open() {
    popup.open = true
  }
  function close() {
    popup.open = false
  }
  function toggle() {
    popup.open = !popup.open
  }

  implicitWidth: button.implicitWidth
  implicitHeight: button.implicitHeight

  ProgressRing {
    id: button
    anchors.centerIn: parent
    width: Style.bar.iconCanvas
    height: Style.bar.iconCanvas
    // Status has `main` / `code` progress; `index` / `indexing` never existed, so the ring
    // always showed the idle mark.
    progress: client.activePass ? client.activePass.done / client.activePass.total : -1
    paused: client.paused

    // Clipboard incognito: a small mark on the icon while capture is paused.
    Rectangle {
      visible: client.clipPausedAt(root.now)
      width: Math.max(5, Math.round(parent.width * 0.28))
      height: width
      radius: width / 2
      anchors.right: parent.right
      anchors.bottom: parent.bottom
      color: Color.foreground
      border.width: 1
      border.color: Color.background
    }

    MouseArea {
      anchors.fill: parent
      acceptedButtons: Qt.LeftButton | Qt.RightButton
      onClicked: function (mouse) {
        if (mouse.button === Qt.RightButton)
          popup.open = !popup.open
        else if (root.shell && typeof root.shell.summon === "function")
          root.shell.summon(root.moduleName, "{}")
        else
          popup.open = !popup.open
      }
    }
  }

  PopupCard {
    id: popup
    anchorItem: button
    bar: root.bar
    owner: root
    contentWidth: 220
    contentHeight: col.implicitHeight + padding * 2

    Column {
      id: col
      width: parent.width
      spacing: 4

      Repeater {
        model: [{
            "label": "Search…",
            "act": "search"
          }, {
            "label": "Open window",
            "act": "window"
          }, {
            "label": "Clipboard history…",
            "act": "clipboard"
          }, {
            "label": client.paused ? "Resume indexing" : "Pause indexing",
            "act": "pause"
          }, {
            "label": client.clipPausedAt(root.now) ? "Resume clipboard history" : "Pause clipboard history for an hour",
            "act": "clippause"
          }, {
            "label": "Choose folders…",
            "act": "folders"
          }, {
            "label": "Settings…",
            "act": "settings"
          }, {
            "label": "Quit Trowel",
            "act": "quit"
          }]
        delegate: Text {
          width: col.width
          text: modelData.label
          color: Color.foreground
          font.pixelSize: Style.font.body
          padding: 6
          MouseArea {
            anchors.fill: parent
            onClicked: {
              popup.open = false
              root.now = Date.now() / 1000
              if (modelData.act === "search" && root.shell)
                root.shell.summon(root.moduleName, "{}")
              else if (modelData.act === "clipboard" && root.shell)
                root.shell.summon(root.moduleName, JSON.stringify({
                  "view": "clipboard"
                }))
              else if (modelData.act === "clippause")
                client.clipPause(client.clipPausedAt(root.now) ? 0 : 60)
              else if (modelData.act === "pause") {
                if (client.paused)
                  client.resume()
                else
                  client.pause()
              } else if (modelData.act === "window" || modelData.act === "settings" || modelData.act === "folders")
                appProc.running = true
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
}
