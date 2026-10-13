import QtQuick
import Quickshell

// First-run wizard. Product name is Trowel (DECISIONS D17), not upstream DigUp.
Item {
  id: root
  required property var client
  property int step: 0
  property var selectedFolders: ["~/Desktop", "~/Documents", "~/Downloads", "~/Pictures"]
  readonly property var steps: ["welcome", "folders", "code", "shortcut", "ready"]
  signal finished
  implicitWidth: 560
  implicitHeight: col.implicitHeight + 40
  focus: true

  Keys.onPressed: function (event) {
    if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
      root.next()
      event.accepted = true
    } else if (event.key === Qt.Key_Backspace || event.key === Qt.Key_Left) {
      if (root.step > 0) {
        root.back()
        event.accepted = true
      }
    }
  }

  Component.onCompleted: {
    selectedFolders = xdgFolders()
    root.forceActiveFocus()
  }

  function next() {
    if (step === 0)
      client.modelDownload("start")
    if (step === 1) {
      var roots = selectedFolders.length ? selectedFolders : xdgFolders()
      if (roots.length)
        client.configSet({
            "roots": roots
          })
    }
    if (step === steps.length - 1) {
      client.configSet({
          "onboarded": true
        })
      root.finished()
      return
    }
    step += 1
  }

  function back() {
    if (step > 0)
      step -= 1
  }

  function xdgFolders() {
    var override = Quickshell.env("TROWEL_ONBOARDING_LOCATIONS")
    if (override && override.length)
      return override.split(":")
    return ["~/Desktop", "~/Documents", "~/Downloads", "~/Pictures"]
  }

  function folderSelected(path) {
    for (var i = 0; i < selectedFolders.length; i++) {
      if (selectedFolders[i] === path)
        return true
    }
    return false
  }

  function toggleFolder(path) {
    var next = []
    var found = false
    for (var i = 0; i < selectedFolders.length; i++) {
      if (selectedFolders[i] === path)
        found = true
      else
        next.push(selectedFolders[i])
    }
    if (!found)
      next.push(path)
    selectedFolders = next
  }

  Column {
    id: col
    width: parent.width - 40
    x: 20
    y: 20
    spacing: 12

    Text {
      text: "Step " + (root.step + 1) + " of " + root.steps.length
      color: Theme.secondary
      font.pixelSize: Theme.captionSize
    }

    Column {
      visible: root.step === 0
      width: parent.width
      height: visible ? implicitHeight : 0
      spacing: 8
      Text {
        text: "Trowel"
        color: Theme.brandAmber
        font.pixelSize: Theme.querySize
        font.weight: Font.DemiBold
      }
      Text {
        text: "Describe it. Dig it up."
        color: Theme.foreground
        font.pixelSize: Theme.emptyTitleSize
        font.weight: Font.DemiBold
      }
      Text {
        width: parent.width
        wrapMode: Text.Wrap
        text: "Everything is indexed and searched on this computer. Trowel downloads its AI model (865 MB) once; after that it works offline, and nothing you index ever leaves this machine."
        color: Theme.secondary
      }
    }

    Column {
      visible: root.step === 1
      width: parent.width
      height: visible ? implicitHeight : 0
      spacing: 8
      Text {
        text: "Choose folders to search"
        color: Theme.foreground
        font.weight: Font.DemiBold
      }
      Text {
        text: "Trowel only looks where you tell it to."
        color: Theme.secondary
      }
      Repeater {
        model: root.xdgFolders()
        delegate: FolderRow {
          width: parent.width
          path: modelData
          checked: root.folderSelected(modelData)
          onToggled: root.toggleFolder(modelData)
        }
      }
    }

    Column {
      visible: root.step === 2
      width: parent.width
      height: visible ? implicitHeight : 0
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
        text: "Trowel can find code by what it does, in the folders you pick. Code gets an index of its own and never shows up in other searches."
        color: Theme.secondary
      }
    }

    Column {
      visible: root.step === 3
      width: parent.width
      height: visible ? implicitHeight : 0
      spacing: 8
      Text {
        text: "That's it. It works from any app."
        color: Theme.foreground
        font.weight: Font.DemiBold
      }
      HotkeyRecorder {
        chord: "SUPER + D"
      }
      Text {
        text: "IN THE SEARCH PANEL"
        color: Theme.secondary
        font.pixelSize: Theme.groupSize
        font.weight: Font.DemiBold
      }
      Text {
        width: parent.width
        wrapMode: Text.Wrap
        text: "Press Super+D from anywhere. Esc closes the panel."
        color: Theme.secondary
      }
    }

    Column {
      visible: root.step === 4
      width: parent.width
      height: visible ? implicitHeight : 0
      spacing: 8
      Text {
        text: "You're all set"
        color: Theme.foreground
        font.pixelSize: Theme.emptyTitleSize
        font.weight: Font.DemiBold
      }
      Text {
        width: parent.width
        wrapMode: Text.Wrap
        text: "Trowel lives in the status bar, which shows how indexing is going. Super+D opens search."
        color: Theme.secondary
      }
      Text {
        width: parent.width
        wrapMode: Text.Wrap
        text: "Index on battery power"
        color: Theme.foreground
        font.weight: Font.DemiBold
      }
      Text {
        width: parent.width
        wrapMode: Text.Wrap
        text: "When it's off, a folder's first pass waits for a charger. New files always go in right away."
        color: Theme.secondary
      }
    }

    Row {
      spacing: 16
      Text {
        visible: root.step > 0
        text: "Back"
        color: Theme.accent
        font.pixelSize: Theme.rowTitleSize
        MouseArea {
          anchors.fill: parent
          anchors.margins: -10
          onClicked: root.back()
        }
      }
      Text {
        text: root.step === root.steps.length - 1 ? "You're all set" : "Continue"
        color: Theme.accent
        font.pixelSize: Theme.rowTitleSize
        font.weight: Font.DemiBold
        MouseArea {
          anchors.fill: parent
          anchors.margins: -10
          onClicked: root.next()
        }
      }
    }
  }
}
