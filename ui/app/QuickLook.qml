import QtQuick
import "../qml/Trowel"

// UI-DESIGN §7: borderless, 80 % of the monitor, opens at the matched place.
Item {
  id: root
  required property var client
  property var row: ({})
  property var preview: ({})
  signal closed

  Keys.onPressed: function (event) {
    if (event.key === Qt.Key_Escape || event.key === Qt.Key_Space) {
      root.closed()
      event.accepted = true
    }
  }

  Rectangle {
    anchors.fill: parent
    color: Theme.background

    Column {
      anchors.fill: parent
      anchors.margins: 16
      spacing: 8

      Row {
        width: parent.width
        Text {
          text: (root.row && root.row.name) ? root.row.name : "Quick Look"
          color: Theme.foreground
          font.pixelSize: Theme.queryCodeSize
          font.weight: Font.DemiBold
        }
        Item {
          width: 12
          height: 1
        }
        PlaceBadge {
          text: (root.row && root.row.place) ? root.row.place : ""
        }
      }

      PreviewPane {
        width: parent.width
        height: parent.height - 48
        preview: root.preview
        row: root.row
      }
    }
  }

  MouseArea {
    anchors.fill: parent
    enabled: false
  }
}
