import QtQuick

Row {
  id: root
  property bool checked: true
  property string path: ""
  property string estimate: ""
  property string skip: ""
  signal toggled
  spacing: 8
  width: parent ? parent.width : 400

  Rectangle {
    width: 16
    height: 16
    radius: 3
    anchors.verticalCenter: parent.verticalCenter
    color: root.checked ? Theme.accent : "transparent"
    border.color: Theme.hairline
    border.width: 1
    MouseArea {
      anchors.fill: parent
      onClicked: root.toggled()
    }
  }

  Column {
    width: parent.width - 24
    spacing: 2
    Text {
      width: parent.width
      text: root.path
      color: Theme.foreground
      font.family: Theme.fontFamily
      font.pixelSize: Theme.rowTitleSize
      elide: Text.ElideMiddle
    }
    Text {
      visible: root.estimate.length > 0 || root.skip.length > 0
      width: parent.width
      text: root.skip.length ? root.skip : root.estimate
      color: Theme.secondary
      font.family: Theme.fontFamily
      font.pixelSize: Theme.captionSize
      elide: Text.ElideRight
    }
  }
}
