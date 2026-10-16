import QtQuick

// A small clickable capsule (copy chips, "Open in browser"). Keycap tokens: fill 0.07, stroke 0.14.
Rectangle {
  id: root
  property string label: ""
  property bool mono: false
  signal activated
  implicitWidth: text.implicitWidth + 20
  implicitHeight: text.implicitHeight + 10
  radius: implicitHeight / 2
  color: hover.hovered ? Theme.alpha(Theme.foreground, 0.12) : Theme.keycapFill
  border.width: Theme.hairlineWidth()
  border.color: Theme.hairline

  Behavior on color {
    ColorAnimation {
      duration: Motion.hoverMs
    }
  }

  Text {
    id: text
    anchors.centerIn: parent
    text: root.label
    color: Theme.foreground
    font.family: root.mono ? Theme.monoFamily : Theme.fontFamily
    font.pixelSize: Theme.captionSize
    font.weight: Font.Medium
  }

  HoverHandler {
    id: hover
  }

  MouseArea {
    anchors.fill: parent
    cursorShape: Qt.PointingHandCursor
    onClicked: root.activated()
  }
}
