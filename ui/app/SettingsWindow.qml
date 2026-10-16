import QtQuick
import QtQuick.Controls
import Quickshell
import Quickshell.Io
import "Trowel"

Item {
  id: root
  required property var client
  property string tab: "general"
  property bool launchAtLogin: false
  property string importResult: ""
  // Window classes of the running apps, for "exclude from clipboard history".
  property var runningApps: []
  readonly property var clipCfg: (client.config && client.config.clipboard) ? client.config.clipboard : ({})
  readonly property var excluded: root.clipCfg.excluded_apps || []
  readonly property string hidesPath: {
    var base = Quickshell.env("XDG_CONFIG_HOME")
    if (!base || !base.length)
      base = Quickshell.env("HOME") + "/.config"
    return base + "/trowel/launcher.hides"
  }
  readonly property var hiddenApps: String(hides.text() || "").split("\n").map(function (l) {
    return l.replace(/^\s+|\s+$/g, "").replace(/\.desktop$/, "")
  }).filter(function (l) {
    return l.length > 0
  })

  function setClip(patch) {
    client.configSet({
      "clipboard": patch
    })
  }

  Connections {
    target: root.client
    function onOpDone(op, msg) {
      if (op === "clip_import")
        root.importResult = "Imported " + msg.imported + (msg.skipped ? ", skipped " + msg.skipped : "")
      else if (op === "clip_clear")
        root.importResult = "Removed " + msg.removed
      else if (op === "app_hide")
        hides.reload()
    }
    function onOpFailed(op, error) {
      if (op === "clip_import")
        root.importResult = error.message || "Import failed"
    }
  }

  FileView {
    id: hides
    path: root.hidesPath
    watchChanges: true
    printErrors: false
    onFileChanged: reload()
  }

  Process {
    id: clients
    command: ["hyprctl", "-j", "clients"]
    running: false
    stdout: StdioCollector {
      onStreamFinished: {
        try {
          var seen = ({})
          var out = []
          var list = JSON.parse(text)
          for (var i = 0; i < list.length; i++) {
            var c = String(list[i]["class"] || "")
            if (c.length && !seen[c.toLowerCase()]) {
              seen[c.toLowerCase()] = true
              out.push(c)
            }
          }
          root.runningApps = out.sort()
        } catch (e) {
          root.runningApps = []
        }
      }
    }
  }
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
        model: ["general", "folders", "code", "launcher", "storage"]
        delegate: Rectangle {
          implicitHeight: 28
          implicitWidth: t.implicitWidth + 16
          radius: 6
          color: root.tab === modelData ? Theme.selectedFill : Theme.keycapFill
          Text {
            id: t
            anchors.centerIn: parent
            text: modelData === "launcher" ? "Launcher & Clipboard" : modelData.charAt(0).toUpperCase() + modelData.slice(1)
            color: Theme.foreground
            font.pixelSize: Theme.rowTitleSize
          }
          MouseArea {
            anchors.fill: parent
            onClicked: {
              root.tab = modelData
              if (modelData === "launcher")
                clients.running = true
            }
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
          estimate: modelData
          onToggled: {
            var roots = (client.config && client.config.roots) ? client.config.roots.slice() : []
            var next = []
            for (var i = 0; i < roots.length; i++) {
              if (roots[i] !== modelData)
                next.push(roots[i])
            }
            client.configSet({
                "roots": next
              })
          }
        }
      }
      Text {
        visible: !(client.config && client.config.roots && client.config.roots.length)
        text: "No folders yet. Add the ones Trowel should search."
        color: Theme.secondary
      }
      Rectangle {
        width: parent.width
        height: 42
        radius: 10
        color: "transparent"
        border.color: Theme.hairline
        border.width: 1
        Text {
          anchors.centerIn: parent
          text: "+  Add Folder…"
          color: Theme.secondary
          font.pixelSize: 13
          font.weight: Font.Medium
        }
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

    // Launcher & Clipboard (LAUNCHER-PLAN.md L4)
    Flickable {
      visible: root.tab === "launcher"
      width: parent.width
      height: parent.height - 40
      contentHeight: launcherCol.implicitHeight
      clip: true
      boundsBehavior: Flickable.StopAtBounds

      Column {
        id: launcherCol
        width: parent.width
        spacing: 8

        Text {
          text: "Clipboard history"
          color: Theme.foreground
          font.pixelSize: Theme.rowTitleSize
          font.weight: Font.DemiBold
        }
        Text {
          width: parent.width
          wrapMode: Text.Wrap
          text: "Keeps what you copy (text, links, colors, images and files) on this computer, searchable from the clipboard view. Copies from the apps listed below, and copies an app marks as a password, are never kept."
          color: Theme.secondary
          font.pixelSize: Theme.captionSize
        }
        Rectangle {
          width: 44
          height: 24
          radius: 12
          color: root.clipCfg.capture !== false ? Theme.accent : Theme.keycapFill
          MouseArea {
            anchors.fill: parent
            onClicked: root.setClip({
                "capture": root.clipCfg.capture === false
              })
          }
        }

        Text {
          text: "Keep"
          color: Theme.foreground
          font.weight: Font.DemiBold
          topPadding: 6
        }
        Repeater {
          model: [{
              "key": "max_entries",
              "label": "items, at most",
              "from": 50,
              "to": 10000,
              "step": 50
            }, {
              "key": "max_age_days",
              "label": "days",
              "from": 1,
              "to": 365,
              "step": 1
            }, {
              "key": "max_blob_mb",
              "label": "MB of images and files",
              "from": 16,
              "to": 8192,
              "step": 16
            }]
          delegate: Row {
            id: limitRow
            required property var modelData
            spacing: 8
            SpinBox {
              from: limitRow.modelData.from
              to: limitRow.modelData.to
              stepSize: limitRow.modelData.step
              editable: true
              value: root.clipCfg[limitRow.modelData.key] || limitRow.modelData.from
              onValueModified: {
                var patch = {}
                patch[limitRow.modelData.key] = value
                root.setClip(patch)
              }
            }
            Text {
              anchors.verticalCenter: parent.verticalCenter
              text: limitRow.modelData.label
              color: Theme.secondary
              font.pixelSize: Theme.captionSize
            }
          }
        }
        Text {
          width: parent.width
          wrapMode: Text.Wrap
          text: "Pinned items are always kept."
          color: Theme.secondary
          font.pixelSize: Theme.captionSize
        }

        Text {
          text: "Never keep copies from"
          color: Theme.foreground
          font.weight: Font.DemiBold
          topPadding: 6
        }
        Flow {
          width: parent.width
          spacing: 6
          Repeater {
            model: root.excluded
            delegate: Chip {
              required property var modelData
              label: modelData + "  ×"
              onActivated: root.setClip({
                  "excluded_apps": root.excluded.filter(function (a) {
                    return a !== modelData
                  })
                })
            }
          }
        }
        Text {
          visible: root.runningApps.length > 0
          text: "Add an app that's open now:"
          color: Theme.secondary
          font.pixelSize: Theme.captionSize
        }
        Flow {
          width: parent.width
          spacing: 6
          Repeater {
            model: root.runningApps.filter(function (a) {
              return !root.excluded.some(function (x) {
                return x.toLowerCase() === a.toLowerCase()
              })
            })
            delegate: Chip {
              required property var modelData
              label: "+ " + modelData
              onActivated: root.setClip({
                  "excluded_apps": root.excluded.concat([modelData])
                })
            }
          }
        }

        Text {
          text: "History"
          color: Theme.foreground
          font.weight: Font.DemiBold
          topPadding: 6
        }
        Row {
          spacing: 8
          Chip {
            label: "Import Omarchy's clipboard history"
            onActivated: root.client.clipImport()
          }
          Chip {
            label: "Clear history (pinned stay)"
            onActivated: root.client.clipClear(true)
          }
        }
        Text {
          visible: root.importResult.length > 0
          text: root.importResult
          color: Theme.secondary
          font.pixelSize: Theme.captionSize
        }
        Text {
          width: parent.width
          wrapMode: Text.Wrap
          text: "To open it with Super+Ctrl+V instead of Omarchy's clipboard, run trowel setup."
          color: Theme.secondary
          font.pixelSize: Theme.captionSize
        }

        Text {
          text: "Hidden apps"
          color: Theme.foreground
          font.weight: Font.DemiBold
          topPadding: 6
        }
        Text {
          visible: root.hiddenApps.length === 0
          width: parent.width
          wrapMode: Text.Wrap
          text: "None. Hide an app from the launcher with Ctrl+K on it in the search panel."
          color: Theme.secondary
          font.pixelSize: Theme.captionSize
        }
        Flow {
          width: parent.width
          spacing: 6
          Repeater {
            model: root.hiddenApps
            delegate: Chip {
              required property var modelData
              label: modelData + "  · Show"
              onActivated: root.client.appHide(modelData, false)
            }
          }
        }
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
