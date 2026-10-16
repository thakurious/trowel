import QtQuick
import "ListMerge.js" as ListMerge

// LAUNCHER-PLAN.md L4: a clipboard entry. ResultRow's frame; the kind glyph, a thumbnail
// (images and a single copied picture) or a color swatch; the preview; `source · 2 m ago`; a pin.
Rectangle {
  id: root
  property var row: ({})
  property bool selected: false
  // Seconds; the panel ticks it so "2 m ago" stays true while it is open.
  property real now: Date.now() / 1000
  signal clicked
  signal opened
  implicitHeight: 46
  implicitWidth: 314
  radius: Theme.innerRadius(Theme.panelRadius()) || 8
  color: selected ? Theme.selectedFill : "transparent"

  readonly property string kind: (row && row.kind) ? row.kind : "text"
  readonly property string thumb: (row && row.thumb) ? String(row.thumb) : ""

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
        kind: root.kind === "code" ? "code" : root.kind
        side: 22
        visible: root.kind !== "color" && thumbImg.status !== Image.Ready
      }

      Rectangle {
        anchors.centerIn: parent
        visible: root.kind === "color"
        width: 26
        height: 26
        radius: 6
        color: (root.row && root.row.color) ? root.row.color : "transparent"
        border.width: Theme.hairlineWidth()
        border.color: Theme.hairline
      }

      Image {
        id: thumbImg
        anchors.fill: parent
        source: root.thumb.length ? "file://" + root.thumb : ""
        sourceSize.width: 36
        sourceSize.height: 36
        asynchronous: true
        cache: true
        fillMode: Image.PreserveAspectCrop
        visible: status === Image.Ready
      }
    }

    Column {
      width: parent.width - 46 - (pin.visible ? pin.width + 6 : 0)
      anchors.verticalCenter: parent.verticalCenter
      spacing: 2

      Text {
        width: parent.width
        text: (root.row && root.row.title) ? root.row.title : ""
        color: Theme.foreground
        font.family: root.kind === "code" ? Theme.monoFamily : Theme.fontFamily
        font.pixelSize: Theme.rowTitleSize
        font.weight: Font.DemiBold
        elide: Text.ElideRight
        maximumLineCount: 1
        textFormat: Text.PlainText
      }

      Text {
        width: parent.width
        text: {
          var r = root.row || {}
          var parts = []
          if (r.kind === "link" && r.domain)
            parts.push(r.domain)
          else if (r.kind === "files" && r.files > 1)
            parts.push(r.files + " files")
          if (r.sourceApp)
            parts.push(r.sourceApp)
          if (r.lastUsed)
            parts.push(ListMerge.ago(r.lastUsed, root.now))
          return parts.join(" · ")
        }
        color: Theme.secondary
        font.family: Theme.fontFamily
        font.pixelSize: Theme.captionSize
        elide: Text.ElideRight
      }
    }

    KindIcon {
      id: pin
      visible: (root.row && root.row.pinned) === true
      anchors.verticalCenter: parent.verticalCenter
      kind: "pin"
      side: 14
      color: Theme.secondary
    }
  }

  MouseArea {
    anchors.fill: parent
    onClicked: root.clicked()
    onDoubleClicked: root.opened()
  }
}
