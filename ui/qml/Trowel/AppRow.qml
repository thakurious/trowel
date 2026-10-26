import QtQuick
import Quickshell

// LAUNCHER-PLAN.md L4: an application in the merged list. Same 46 px frame as ResultRow
// (padding 8×5, radius 8), a 32 px icon, name, generic name, and an action-count badge.
Rectangle {
  id: root
  property var row: ({})
  property bool selected: false
  signal clicked
  signal opened
  implicitHeight: 46
  implicitWidth: 314
  radius: Theme.innerRadius(Theme.panelRadius()) || 8
  color: selected ? Theme.selectedFill : "transparent"

  // The daemon resolves the icon file; the theme lookup is the fallback for an Icon= it missed.
  readonly property string iconSource: {
    var r = root.row || {}
    if (r.icon && String(r.icon).length)
      return "file://" + r.icon
    if (r.iconName && String(r.iconName).length)
      return Quickshell.iconPath(r.iconName, true)
    return ""
  }

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

      Image {
        id: icon
        anchors.centerIn: parent
        width: 32
        height: 32
        source: root.iconSource
        sourceSize.width: 32
        sourceSize.height: 32
        asynchronous: true
        cache: true
        fillMode: Image.PreserveAspectFit
        visible: status === Image.Ready
      }

      KindIcon {
        anchors.centerIn: parent
        kind: (root.row && root.row.glyph) ? root.row.glyph : "app"
        side: 22
        visible: icon.status !== Image.Ready
      }
    }

    Column {
      width: parent.width - 46 - (badge.visible ? badge.width + 10 : 0)
      anchors.verticalCenter: parent.verticalCenter
      spacing: 2

      Text {
        width: parent.width
        text: (root.row && root.row.name) ? root.row.name : ""
        color: Theme.foreground
        font.family: Theme.fontFamily
        font.pixelSize: Theme.rowTitleSize
        font.weight: Font.DemiBold
        elide: Text.ElideRight
      }

      Text {
        width: parent.width
        text: {
          var r = root.row || {}
          return r.generic || r.comment || (r.rowType === "command" ? "Command" : (r.rowType === "snippet" ? "Snippet" : "Application"))
        }
        color: Theme.secondary
        font.family: Theme.fontFamily
        font.pixelSize: Theme.captionSize
        elide: Text.ElideRight
      }
    }

    // "3 actions" (desktop actions, run from the action panel); "Command" for a built-in command.
    Rectangle {
      id: badge
      visible: (root.row && (root.row.actionCount > 0 || root.row.rowType === "command" || root.row.rowType === "snippet")) === true
      anchors.verticalCenter: parent.verticalCenter
      width: badgeText.implicitWidth + 12
      height: badgeText.implicitHeight + 4
      radius: height / 2
      color: (root.row && root.row.accessory === "On") ? Theme.alpha(Theme.accent, 0.2) : Theme.keycapFill
      border.width: Theme.hairlineWidth()
      border.color: Theme.hairline

      Text {
        id: badgeText
        anchors.centerIn: parent
        text: {
          // A toggle's On/Off, a snippet's keyword; "Command" for the rest.
          if (root.row && root.row.accessory)
            return root.row.accessory
          if (root.row && root.row.rowType === "command")
            return "Command"
          var n = (root.row && root.row.actionCount) ? root.row.actionCount : 0
          return n === 1 ? "1 action" : n + " actions"
        }
        color: Theme.secondary
        font.family: Theme.fontFamily
        font.pixelSize: Math.round(Theme.fontBase * 0.83)
        font.weight: Font.Medium
        font.features: {
          "tnum": 1
        }
      }
    }
  }

  MouseArea {
    anchors.fill: parent
    onClicked: root.clicked()
    onDoubleClicked: root.opened()
  }
}
