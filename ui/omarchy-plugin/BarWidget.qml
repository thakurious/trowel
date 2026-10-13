import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui
import "../qml/Trowel"

// StatusMenu.swift → bar-widget. Popup: search, open, pause, folders, settings, quit.
BarWidget {
  id: root
  moduleName: (manifest && manifest.id) ? String(manifest.id) : "io.github.thakurious.trowel-dev"

  property var manifest: null
  property var shell: null
  readonly property bool opened: popup.open

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
    progress: {
      var st = client.status || {}
      var idx = st.index || st.indexing || {}
      if (idx && idx.total > 0)
        return idx.done / idx.total
      return -1
    }
    paused: !!(client.status && (client.status.paused || client.status.phase === "paused"))

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
            "label": client.status && client.status.paused ? "Resume indexing" : "Pause indexing",
            "act": "pause"
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
              if (modelData.act === "search" && root.shell)
                root.shell.summon(root.moduleName, "{}")
              else if (modelData.act === "pause") {
                if (client.status && client.status.paused)
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
