import QtQuick
import Quickshell
import "../qml/Trowel"

Item {
  id: root
  required property var client
  property int step: 0
  readonly property var steps: ["welcome", "folders", "code", "shortcut", "ready"]
  signal finished
  width: 560
  height: 640

  function next() {
    if (step === 0)
      client.modelDownload("start")
    if (step === 1) {
      var roots = xdgFolders()
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

  Column {
    anchors.fill: parent
    anchors.margins: 24
    spacing: 16

    Text {
      text: "Step " + (root.step + 1) + " of " + root.steps.length
      color: Theme.secondary
      font.pixelSize: Theme.captionSize
    }

    Column {
      visible: root.step === 0
      width: parent.width
      spacing: 10
      Text {
        text: "DigUp"
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
        text: "Everything is indexed and searched on this Mac. DigUp downloads its AI model (865 MB) once; after that it works offline, and nothing you index ever leaves your Mac."
        color: Theme.secondary
      }
    }

    Column {
      visible: root.step === 1
      width: parent.width
      spacing: 8
      Text {
        text: "Choose folders to search"
        color: Theme.foreground
        font.weight: Font.DemiBold
      }
      Text {
        text: "DigUp only looks where you tell it to."
        color: Theme.secondary
      }
      Repeater {
        model: root.xdgFolders()
        delegate: FolderRow {
          width: parent.width
          path: modelData
          checked: true
        }
      }
    }

    Column {
      visible: root.step === 2
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
        text: "DigUp can find code by what it does, in the folders you pick. Code gets an index of its own and never shows up in other searches."
        color: Theme.secondary
      }
    }

    Column {
      visible: root.step === 3
      width: parent.width
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
    }

    Column {
      visible: root.step === 4
      width: parent.width
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
        text: "DigUp lives in the menu bar, which shows how indexing is going."
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
      spacing: 12
      Text {
        visible: root.step > 0
        text: "Back"
        color: Theme.accent
        MouseArea {
          anchors.fill: parent
          onClicked: root.back()
        }
      }
      Text {
        text: root.step === root.steps.length - 1 ? "You're all set" : "Continue"
        color: Theme.accent
        font.weight: Font.DemiBold
        MouseArea {
          anchors.fill: parent
          onClicked: root.next()
        }
      }
    }
  }
}
