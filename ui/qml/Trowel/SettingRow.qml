import QtQuick

// A setting in the panel's Settings list (DECISIONS D26): the same 46 px frame as AppRow, a glyph
// tile, the title, and the current value on the right (Raycast's list accessory).
Rectangle {
  id: root
  property var entry: null
  property string accessory: ""
  property bool selected: false
  signal clicked
  signal opened
  implicitHeight: 46
  implicitWidth: 314
  radius: Theme.innerRadius(Theme.panelRadius()) || 8
  color: selected ? Theme.selectedFill : (mouse.containsMouse ? Theme.keycapFill : "transparent")

  Rectangle {
    id: tile
    anchors.left: parent.left
    anchors.leftMargin: 10
    anchors.verticalCenter: parent.verticalCenter
    width: 28
    height: 28
    radius: Theme.innerRadius(Theme.panelRadius()) + 1
    color: root.selected ? Theme.alpha(Theme.accent, 0.22) : Theme.keycapFill
    Text {
      anchors.centerIn: parent
      text: root.entry ? root.entry.glyph : ""
      color: root.selected ? Theme.accent : Theme.secondary
      font.family: Theme.monoFamily
      font.pixelSize: 13
    }
  }

  Text {
    id: title
    anchors.left: tile.right
    anchors.leftMargin: 10
    anchors.right: value.left
    anchors.rightMargin: 8
    anchors.verticalCenter: parent.verticalCenter
    text: root.entry ? root.entry.title : ""
    elide: Text.ElideRight
    color: Theme.foreground
    font.family: Theme.fontFamily
    font.pixelSize: Theme.rowTitleSize
  }

  Text {
    id: value
    anchors.right: parent.right
    anchors.rightMargin: 12
    anchors.verticalCenter: parent.verticalCenter
    width: Math.min(implicitWidth, root.width * 0.42)
    horizontalAlignment: Text.AlignRight
    text: root.accessory
    elide: Text.ElideLeft
    color: root.accessory === "On" ? Theme.accent : Theme.secondary
    font.family: Theme.fontFamily
    font.pixelSize: Theme.captionSize
  }

  MouseArea {
    id: mouse
    anchors.fill: parent
    hoverEnabled: true
    onClicked: root.clicked()
    onDoubleClicked: root.opened()
  }
}
