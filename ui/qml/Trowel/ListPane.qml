import QtQuick

// The right pane for Omarchy Commands and Pick Theme: what ↵ will do with the selected row.
Item {
  id: root
  property var row: null

  Column {
    anchors.centerIn: parent
    width: parent.width - 48
    spacing: 10
    visible: root.row !== null

    KindIcon {
      anchors.horizontalCenter: parent.horizontalCenter
      kind: root.row ? (root.row.glyph || "terminal") : "terminal"
      side: 40
      color: Theme.accent
    }
    Text {
      width: parent.width
      horizontalAlignment: Text.AlignHCenter
      text: root.row ? root.row.title : ""
      color: Theme.foreground
      font.family: Theme.fontFamily
      font.pixelSize: Theme.rowTitleSize + 3
      font.weight: Font.DemiBold
      wrapMode: Text.WordWrap
    }
    Text {
      width: parent.width
      horizontalAlignment: Text.AlignHCenter
      visible: text.length > 0
      text: root.row ? root.row.subtitle : ""
      color: Theme.secondary
      font.family: Theme.fontFamily
      font.pixelSize: Theme.captionSize
      wrapMode: Text.WordWrap
    }
    Text {
      width: parent.width
      horizontalAlignment: Text.AlignHCenter
      text: {
        if (!root.row)
          return ""
        if (root.row.rowType === "theme")
          return root.row.accessory === "Current" ? "This is the theme you have now." : "↵ switches every app, the bar and the wallpaper to it."
        return root.row.accessory === "Terminal" ? "It asks questions, so ↵ opens it in a terminal." : "↵ runs it."
      }
      color: Theme.secondary
      font.family: Theme.fontFamily
      font.pixelSize: Theme.captionSize
      wrapMode: Text.WordWrap
    }
  }
}
