import QtQuick

// A row in the Phase 9 views (Timer, Snippets, Omarchy Commands, themes): AppRow's 46 px frame,
// a glyph tile, title over an optional subtitle, and an accessory on the right (Raycast's list
// accessory: a countdown, a keyword, "Current").
Rectangle {
  id: root
  property string glyph: "doc"
  property string title: ""
  property string subtitle: ""
  property string accessory: ""
  property bool accent: false
  property bool mono: false
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
    KindIcon {
      anchors.centerIn: parent
      kind: root.glyph
      side: 16
      color: root.selected ? Theme.accent : Theme.secondary
    }
  }

  Column {
    anchors.left: tile.right
    anchors.leftMargin: 10
    anchors.right: value.left
    anchors.rightMargin: 8
    anchors.verticalCenter: parent.verticalCenter
    spacing: 1
    Text {
      width: parent.width
      text: root.title
      elide: Text.ElideRight
      color: Theme.foreground
      font.family: Theme.fontFamily
      font.pixelSize: Theme.rowTitleSize
    }
    Text {
      width: parent.width
      visible: root.subtitle.length > 0
      text: root.subtitle
      elide: Text.ElideRight
      color: Theme.secondary
      font.family: Theme.fontFamily
      font.pixelSize: Theme.captionSize
    }
  }

  Text {
    id: value
    anchors.right: parent.right
    anchors.rightMargin: 12
    anchors.verticalCenter: parent.verticalCenter
    width: Math.min(implicitWidth, root.width * 0.4)
    horizontalAlignment: Text.AlignRight
    text: root.accessory
    elide: Text.ElideLeft
    color: root.accent ? Theme.accent : Theme.secondary
    font.family: root.mono ? Theme.monoFamily : Theme.fontFamily
    font.pixelSize: Theme.captionSize
    font.features: {
      "tnum": 1
    }
  }

  MouseArea {
    id: mouse
    anchors.fill: parent
    hoverEnabled: true
    onClicked: root.clicked()
    onDoubleClicked: root.opened()
  }
}
