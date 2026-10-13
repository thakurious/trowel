import QtQuick

// Components.swift PlaceBadge: 10 semibold tnum, capsule, 6×2.
Rectangle {
  id: root
  property string text: ""
  property bool onImage: false
  visible: text.length > 0
  implicitWidth: label.implicitWidth + 12
  implicitHeight: label.implicitHeight + 4
  radius: height / 2
  color: onImage ? Qt.rgba(0, 0, 0, 0.6) : Theme.keycapFill

  Text {
    id: label
    anchors.centerIn: parent
    text: root.text
    color: root.onImage ? "white" : Theme.foreground
    font.family: Theme.fontFamily
    font.pixelSize: Theme.groupSize
    font.weight: Font.DemiBold
    font.features: {
      "tnum": 1
    }
  }
}
