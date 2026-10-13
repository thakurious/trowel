import QtQuick

Column {
  id: root
  property string title: ""
  property string detail: ""
  property string action: ""
  signal actionClicked
  spacing: 8
  width: parent ? parent.width : 330

  Text {
    visible: title.length > 0
    width: parent.width
    horizontalAlignment: Text.AlignHCenter
    text: root.title
    color: Theme.foreground
    font.family: Theme.fontFamily
    font.pixelSize: Theme.emptyTitleSize
    font.weight: Font.DemiBold
    wrapMode: Text.WordWrap
  }

  Text {
    visible: detail.length > 0
    width: parent.width
    horizontalAlignment: Text.AlignHCenter
    text: root.detail
    color: Theme.secondary
    font.family: Theme.fontFamily
    font.pixelSize: Theme.captionSize
    wrapMode: Text.WordWrap
  }

  Text {
    visible: root.action.length > 0
    width: parent.width
    horizontalAlignment: Text.AlignHCenter
    text: root.action
    color: Theme.accent
    font.family: Theme.fontFamily
    font.pixelSize: Theme.captionSize
    font.weight: Font.Medium
    MouseArea {
      anchors.fill: parent
      cursorShape: Qt.PointingHandCursor
      onClicked: root.actionClicked()
    }
  }
}
