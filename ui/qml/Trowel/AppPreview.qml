pragma ComponentBehavior: Bound
import QtQuick
import Quickshell
import "ListMerge.js" as ListMerge

// LAUNCHER-PLAN.md L4 preview column for an app: big icon, name, comment, the .desktop path,
// launch stats and its desktop actions.
Item {
  id: root
  property var row: ({})
  property real now: Date.now() / 1000
  clip: true

  readonly property var app: (row && row.raw) ? row.raw : ({})
  readonly property string iconSource: {
    if (row && row.icon && String(row.icon).length)
      return "file://" + row.icon
    if (row && row.iconName && String(row.iconName).length)
      return Quickshell.iconPath(row.iconName, true)
    return ""
  }

  Column {
    anchors.fill: parent
    anchors.margins: 22
    spacing: 10

    Item {
      width: 64
      height: 64
      Image {
        id: big
        anchors.fill: parent
        source: root.iconSource
        sourceSize.width: 64
        sourceSize.height: 64
        asynchronous: true
        fillMode: Image.PreserveAspectFit
        visible: status === Image.Ready
      }
      KindIcon {
        anchors.centerIn: parent
        kind: (root.row && root.row.glyph) ? root.row.glyph : "app"
        side: 40
        visible: big.status !== Image.Ready
      }
    }

    Text {
      width: parent.width
      text: (root.row && root.row.name) ? root.row.name : ""
      color: Theme.foreground
      font.family: Theme.fontFamily
      font.pixelSize: Theme.emptyTitleSize
      font.weight: Font.DemiBold
      wrapMode: Text.Wrap
    }

    Text {
      width: parent.width
      visible: text.length > 0
      text: {
        var r = root.row || {}
        var parts = []
        if (r.generic)
          parts.push(r.generic)
        if (r.comment && r.comment !== r.generic)
          parts.push(r.comment)
        return parts.join(" — ")
      }
      color: Theme.secondary
      font.family: Theme.fontFamily
      font.pixelSize: Theme.captionSize
      wrapMode: Text.Wrap
    }

    Text {
      width: parent.width
      text: {
        if (root.row && root.row.rowType === "command")
          return "Press Enter to open"
        var n = (root.row && root.row.launches) ? root.row.launches : 0
        if (!n)
          return "Not launched from Trowel yet"
        var last = root.app.last_launched ? " · last " + ListMerge.ago(root.app.last_launched, root.now) : ""
        return (n === 1 ? "Launched once" : "Launched " + n + " times") + last
      }
      color: Theme.secondary
      font.family: Theme.fontFamily
      font.pixelSize: Theme.captionSize
    }

    Text {
      width: parent.width
      text: {
        var s = root.app.source
        return s ? (typeof s === "string" ? s : (s.display || "")) : ""
      }
      color: Theme.muted
      font.family: Theme.monoFamily
      font.pixelSize: Math.round(Theme.fontBase * 0.83)
      elide: Text.ElideMiddle
    }

    Column {
      width: parent.width
      spacing: 4
      visible: (root.app.actions || []).length > 0

      GroupHeader {
        title: "Actions"
        leftPadding: 0
      }

      Repeater {
        model: root.app.actions || []
        delegate: Text {
          required property var modelData
          width: parent ? parent.width : 0
          text: modelData.name || modelData.id
          color: Theme.foreground
          font.family: Theme.fontFamily
          font.pixelSize: Theme.captionSize
          elide: Text.ElideRight
        }
      }
    }
  }
}
