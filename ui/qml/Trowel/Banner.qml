import QtQuick

Rectangle {
  id: root
  property string text: ""
  property string action: ""
  signal actionClicked
  implicitHeight: Math.max(32, label.implicitHeight + 12)
  implicitWidth: 330
  radius: Theme.innerRadius(Theme.panelRadius())
  color: Theme.alpha(Theme.urgent, 0.16)

  Row {
    anchors.fill: parent
    anchors.margins: 8
    spacing: 8

    Text {
      id: label
      width: parent.width - (root.action.length ? actionLabel.implicitWidth + 8 : 0)
      text: root.text
      color: Theme.foreground
      font.family: Theme.fontFamily
      font.pixelSize: Theme.captionSize
      wrapMode: Text.WordWrap
    }

    Text {
      id: actionLabel
      visible: root.action.length > 0
      text: root.action
      color: Theme.accent
      font.family: Theme.fontFamily
      font.pixelSize: Theme.captionSize
      font.weight: Font.DemiBold
      MouseArea {
        anchors.fill: parent
        cursorShape: Qt.PointingHandCursor
        onClicked: root.actionClicked()
      }
    }
  }
}
