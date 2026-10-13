import QtQuick
import Quickshell.Io
import "Trowel"

Item {
  id: root
  required property var client
  property string tab: "general"
  property bool launchAtLogin: false
  width: 640
  height: 520

  function refresh() {
    client.configGet()
    client.modelStatus()
  }

  Column {
    anchors.fill: parent
    anchors.margins: 16
    spacing: 12

    Row {
      spacing: 8
      Repeater {
        model: ["general", "folders", "code", "storage"]
        delegate: Rectangle {
          implicitHeight: 28
          implicitWidth: t.implicitWidth + 16
          radius: 6
          color: root.tab === modelData ? Theme.selectedFill : Theme.keycapFill
          Text {
            id: t
            anchors.centerIn: parent
            text: modelData.charAt(0).toUpperCase() + modelData.slice(1)
            color: Theme.foreground
            font.pixelSize: Theme.rowTitleSize
          }
          MouseArea {
            anchors.fill: parent
            onClicked: root.tab = modelData
          }
        }
      }
    }

    // General
    Column {
      visible: root.tab === "general"
      width: parent.width
      spacing: 10
      Text {
        text: "Search shortcut"
        color: Theme.foreground
        font.pixelSize: Theme.rowTitleSize
        font.weight: Font.DemiBold
      }
      Text {
        text: "Opens the search panel from any app."
        color: Theme.secondary
        font.pixelSize: Theme.captionSize
      }
      HotkeyRecorder {
        chord: (client.config && client.config.hotkey) ? client.config.hotkey : "SUPER + D"
        onCaptured: function (chord) {
          client.setHotkey(chord)
        }
      }
      Text {
        text: "Index on battery power"
        color: Theme.foreground
        font.pixelSize: Theme.rowTitleSize
        font.weight: Font.DemiBold
      }
      Text {
        width: parent.width
        wrapMode: Text.Wrap
        text: "When it's off, a folder's first pass waits for a charger. New files always go in right away."
        color: Theme.secondary
        font.pixelSize: Theme.captionSize
      }
      Rectangle {
        width: 44
        height: 24
        radius: 12
        color: (client.config && client.config.index_on_battery) ? Theme.accent : Theme.keycapFill
        MouseArea {
          anchors.fill: parent
          onClicked: client.configSet({
              "index_on_battery": !(client.config && client.config.index_on_battery)
            })
        }
      }
      Text {
        text: "Open Trowel when you log in"
        color: Theme.foreground
        font.pixelSize: Theme.rowTitleSize
        font.weight: Font.DemiBold
      }
      Text {
        text: "So the shortcut always works."
        color: Theme.secondary
        font.pixelSize: Theme.captionSize
      }
      Rectangle {
        width: 44
        height: 24
        radius: 12
        color: root.launchAtLogin ? Theme.accent : Theme.keycapFill
        MouseArea {
          anchors.fill: parent
          onClicked: {
            root.launchAtLogin = !root.launchAtLogin
            autostart.command = ["trowel", "autostart", root.launchAtLogin ? "on" : "off"]
            autostart.running = true
          }
        }
      }
    }

    // Folders
    Column {
      visible: root.tab === "folders"
      width: parent.width
      spacing: 8
      Text {
        text: "Folders to search"
        color: Theme.foreground
        font.weight: Font.DemiBold
      }
      Repeater {
        model: (client.config && client.config.roots) ? client.config.roots : []
        delegate: FolderRow {
          width: parent.width
          path: modelData
          checked: true
        }
      }
      Text {
        visible: !(client.config && client.config.roots && client.config.roots.length)
        text: "No folders yet. Add the ones Trowel should search."
        color: Theme.secondary
      }
      Text {
        text: "Add Folders…"
        color: Theme.accent
        MouseArea {
          anchors.fill: parent
          onClicked: picker.running = true
        }
      }
      Text {
        width: parent.width
        wrapMode: Text.Wrap
        text: "Skip these file types"
        color: Theme.foreground
        font.weight: Font.DemiBold
        topPadding: 8
      }
      Text {
        width: parent.width
        wrapMode: Text.Wrap
        text: "Trowel only reads pictures, PDFs, documents (txt, md, rtf, docx, odt, html), audio and video here; code is read only in the code folders (the Code tab). Keys, certificates and password files are never read, nor are hidden folders like ~/.ssh."
        color: Theme.secondary
        font.pixelSize: Theme.captionSize
      }
    }

    // Code
    Column {
      visible: root.tab === "code"
      width: parent.width
      spacing: 8
      Text {
        width: parent.width
        wrapMode: Text.Wrap
        text: "Search your code"
        color: Theme.foreground
        font.weight: Font.DemiBold
      }
      Text {
        width: parent.width
        wrapMode: Text.Wrap
        text: "Find code by what it does, or by a name in it: type code: in the search panel, as in “code: retry with backoff”. Code has an index of its own, and it never shows up in other searches."
        color: Theme.secondary
        font.pixelSize: Theme.captionSize
      }
      Repeater {
        model: (client.config && client.config.code_roots) ? client.config.code_roots : []
        delegate: FolderRow {
          width: parent.width
          path: modelData
          checked: true
        }
      }
      Text {
        text: "Open code in"
        color: Theme.foreground
        font.weight: Font.DemiBold
      }
      Text {
        width: parent.width
        wrapMode: Text.Wrap
        text: "A result opens there at the line that matched. Space shows the whole file in Quick Look."
        color: Theme.secondary
        font.pixelSize: Theme.captionSize
      }
      Text {
        text: (client.config && client.config.code_editor) ? client.config.code_editor : "The file's default app"
        color: Theme.foreground
      }
    }

    // Storage
    Column {
      visible: root.tab === "storage"
      width: parent.width
      spacing: 8
      Text {
        text: "Model"
        color: Theme.foreground
        font.weight: Font.DemiBold
      }
      Text {
        text: "Google · Apache 2.0 · 865 MB"
        color: Theme.secondary
      }
      Text {
        text: "Download"
        color: Theme.accent
        MouseArea {
          anchors.fill: parent
          onClicked: client.modelDownload("start")
        }
      }
      Text {
        text: "Index"
        color: Theme.foreground
        font.weight: Font.DemiBold
        topPadding: 8
      }
      Text {
        width: parent.width
        wrapMode: Text.Wrap
        text: "Privacy"
        color: Theme.foreground
        font.weight: Font.DemiBold
      }
      Text {
        width: parent.width
        wrapMode: Text.Wrap
        text: "Trowel reads the folders you choose and keeps what it learns in its index, on this computer. It goes online only to download its model, once, and nothing about your files is sent."
        color: Theme.secondary
        font.pixelSize: Theme.captionSize
      }
    }
  }

  Process {
    id: picker
    command: ["trowel", "portal-pick-folder"]
    running: false
    stdout: SplitParser {
      onRead: function (line) {
        var p = String(line).replace(/^\s+|\s+$/g, "")
        if (!p.length)
          return
        var roots = (client.config && client.config.roots) ? client.config.roots.slice() : []
        roots.push(p)
        client.configSet({
            "roots": roots
          })
      }
    }
  }

  Process {
    id: autostart
    running: false
  }
}
