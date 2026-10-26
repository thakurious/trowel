import QtQuick
import "DeskModel.js" as DeskModel

// A process (or app) in the Process Monitor: name and PIDs, then the sorted-by number large and
// the other three small, so a re-sort never hides what the row was showing.
Rectangle {
  id: root
  property var row: ({})
  property string sort: "cpu"
  property bool selected: false
  signal clicked
  signal opened
  implicitHeight: 46
  implicitWidth: 314
  radius: Theme.innerRadius(Theme.panelRadius()) || 8
  color: selected ? Theme.selectedFill : (mouse.containsMouse ? Theme.keycapFill : "transparent")

  readonly property var others: DeskModel.SORTS.filter(function (s) {
    return s !== root.sort
  })

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
      kind: (root.row && (root.row.protected || root.row.kill_refused)) ? "lock" : "process"
      side: 15
      color: root.selected ? Theme.accent : Theme.secondary
    }
  }

  Column {
    anchors.left: tile.right
    anchors.leftMargin: 10
    anchors.right: big.left
    anchors.rightMargin: 8
    anchors.verticalCenter: parent.verticalCenter
    spacing: 1
    Text {
      width: parent.width
      text: root.row ? root.row.name || "" : ""
      elide: Text.ElideRight
      color: Theme.foreground
      font.family: Theme.fontFamily
      font.pixelSize: Theme.rowTitleSize
    }
    Text {
      width: parent.width
      text: root.others.map(function (s) {
        return DeskModel.SORT_LABELS[s].charAt(0) + " " + DeskModel.metric(root.row, s)
      }).join("  ")
      elide: Text.ElideRight
      color: Theme.secondary
      font.family: Theme.monoFamily
      font.pixelSize: Math.round(Theme.fontBase * 0.8)
    }
  }

  Text {
    id: big
    anchors.right: parent.right
    anchors.rightMargin: 12
    anchors.verticalCenter: parent.verticalCenter
    text: DeskModel.metric(root.row, root.sort)
    color: Theme.accent
    font.family: Theme.monoFamily
    font.pixelSize: Theme.rowTitleSize
    font.weight: Font.DemiBold
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
