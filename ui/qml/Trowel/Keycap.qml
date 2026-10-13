import QtQuick

// Components.swift Keycaps: padding 0.45/0.28, radius 0.42·size, fill 0.07, stroke 0.14.
Rectangle {
  id: root
  property string label: ""
  property int size: Theme.captionSize
  implicitWidth: Math.max(labelText.implicitWidth + size * 0.9, size * 1.6)
  implicitHeight: labelText.implicitHeight + size * 0.56
  radius: size * 0.42
  color: Theme.keycapFill
  border.width: Theme.hairlineWidth()
  border.color: Theme.hairline

  Text {
    id: labelText
    anchors.centerIn: parent
    text: root.label
    color: Theme.foreground
    font.family: Theme.fontFamily
    font.pixelSize: root.size
    font.weight: Font.Medium
  }
}
