import QtQuick

// SearchPanelView.swift WeakerRow. Height 26, radius 8.
Rectangle {
  id: root
  property int count: 0
  property bool expanded: false
  property bool selected: false
  signal toggled
  implicitHeight: 26
  implicitWidth: 300
  radius: Theme.innerRadius(Theme.panelRadius()) || 8
  color: selected ? Theme.selectedFill : "transparent"

  Row {
    anchors.fill: parent
    anchors.leftMargin: 10
    anchors.rightMargin: 10
    spacing: 7

    Text {
      anchors.verticalCenter: parent.verticalCenter
      text: "▸"
      rotation: root.expanded ? 90 : 0
      color: Theme.secondary
      font.pixelSize: 10
      font.weight: Font.Bold
      Behavior on rotation {
        NumberAnimation {
          duration: Motion.foldMs
          easing.type: Motion.foldEasing
        }
      }
    }

    Text {
      anchors.verticalCenter: parent.verticalCenter
      text: root.expanded ? "Weaker matches" : (root.count + " weaker " + (root.count === 1 ? "match" : "matches"))
      color: Theme.secondary
      font.family: Theme.fontFamily
      font.pixelSize: Math.round(Theme.fontBase * 0.96)
      font.weight: Font.Medium
    }
  }

  MouseArea {
    anchors.fill: parent
    onClicked: root.toggled()
  }
}
