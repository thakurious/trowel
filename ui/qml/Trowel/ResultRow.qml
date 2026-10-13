import QtQuick

// SearchPanelView.swift PanelRow: padding 8×5, radius 8, thumb 36.
Rectangle {
  id: root
  property var row: ({})
  property bool selected: false
  property string thumbFile: ""
  signal clicked
  signal opened
  implicitHeight: 46
  implicitWidth: 314
  radius: Theme.innerRadius(Theme.panelRadius()) || 8
  color: selected ? Theme.selectedFill : "transparent"
  opacity: row && row.weak ? 0.85 : 1

  Row {
    anchors.fill: parent
    anchors.leftMargin: 8
    anchors.rightMargin: 8
    anchors.topMargin: 5
    anchors.bottomMargin: 5
    spacing: 10

    Item {
      width: 36
      height: 36
      anchors.verticalCenter: parent.verticalCenter

      KindIcon {
        anchors.centerIn: parent
        kind: (root.row && root.row.kind) ? root.row.kind : "doc"
        side: 22
        visible: thumbImg.status !== Image.Ready
      }

      Image {
        id: thumbImg
        anchors.fill: parent
        source: root.thumbFile.length ? "file://" + root.thumbFile : ""
        sourceSize.width: 36
        sourceSize.height: 36
        asynchronous: true
        cache: true
        fillMode: Image.PreserveAspectCrop
        visible: status === Image.Ready
      }

      Rectangle {
        anchors.fill: parent
        radius: 36 > 60 ? 8 : 6
        color: "transparent"
        border.width: 0
      }
    }

    Column {
      width: parent.width - 46
      anchors.verticalCenter: parent.verticalCenter
      spacing: 2

      Row {
        spacing: 6
        width: parent.width

        Text {
          width: Math.min(implicitWidth, parent.width - (place.visible ? place.implicitWidth + 6 : 0))
          text: (root.row && (root.row.title || root.row.name)) ? (root.row.title || root.row.name) : ""
          color: Theme.foreground
          font.family: Theme.fontFamily
          font.pixelSize: Theme.rowTitleSize
          font.weight: Font.DemiBold
          elide: Text.ElideMiddle
        }

        PlaceBadge {
          id: place
          text: (root.row && root.row.place) ? root.row.place : ""
          anchors.verticalCenter: parent.verticalCenter
        }
      }

      Text {
        width: parent.width
        text: {
          var r = root.row || {}
          if (r.excerpt && r.matched_words && r.kind !== "code")
            return String(r.excerpt).replace(/«/g, "").replace(/»/g, "")
          return r.folder || r.reason || ""
        }
        color: Theme.secondary
        font.family: (root.row && root.row.kind === "code") ? Theme.monoFamily : Theme.fontFamily
        font.pixelSize: Theme.captionSize
        elide: Text.ElideRight
      }
    }
  }

  MouseArea {
    anchors.fill: parent
    onClicked: root.clicked()
    onDoubleClicked: root.opened()
  }
}
