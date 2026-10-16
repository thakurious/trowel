import QtQuick

// A short confirmation at the bottom of the panel ("Copied", "Pinned", "Deleted").
Rectangle {
  id: root
  property int holdMs: 1400
  visible: opacity > 0
  opacity: 0
  implicitWidth: label.implicitWidth + 24
  implicitHeight: label.implicitHeight + 10
  radius: implicitHeight / 2
  color: Theme.alpha(Theme.background, 0.92)
  border.width: Theme.hairlineWidth()
  border.color: Theme.hairline

  function show(text) {
    label.text = text
    root.opacity = 1
    hold.restart()
  }

  Behavior on opacity {
    NumberAnimation {
      duration: Motion.swapMs
    }
  }

  Text {
    id: label
    anchors.centerIn: parent
    color: Theme.foreground
    font.family: Theme.fontFamily
    font.pixelSize: Theme.captionSize
    font.weight: Font.Medium
  }

  Timer {
    id: hold
    interval: root.holdMs
    onTriggered: root.opacity = 0
  }
}
