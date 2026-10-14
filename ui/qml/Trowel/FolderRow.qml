import QtQuick

// FolderChoices.swift FolderRow: checkbox, folder icon, name, estimate.
Item {
  id: root
  property bool checked: true
  property string path: ""
  property string estimate: ""
  property string skip: ""
  property bool busy: false
  signal toggled

  width: parent ? parent.width : 400
  implicitHeight: 54

  readonly property string displayName: {
    var p = String(root.path || "").replace(/\/+$/, "")
    var i = p.lastIndexOf("/")
    var name = i >= 0 ? p.slice(i + 1) : p
    if (name === "" || name === "~")
      return "Home"
    return name
  }

  Rectangle {
    anchors.fill: parent
    radius: 10
    color: Theme.alpha(Theme.foreground, root.checked ? 0.055 : 0.025)
  }

  MouseArea {
    anchors.fill: parent
    onClicked: root.toggled()
  }

  Row {
    anchors.fill: parent
    anchors.leftMargin: 12
    anchors.rightMargin: 14
    spacing: 10

    Rectangle {
      width: 18
      height: 18
      radius: 4
      anchors.verticalCenter: parent.verticalCenter
      color: root.checked ? Theme.accent : "transparent"
      border.color: root.checked ? Theme.accent : Theme.hairline
      border.width: 1

      Text {
        visible: root.checked
        anchors.centerIn: parent
        text: "✓"
        color: Theme.foreground
        font.pixelSize: 12
        font.weight: Font.Bold
      }
    }

    KindIcon {
      kind: "folder"
      side: 28
      anchors.verticalCenter: parent.verticalCenter
    }

    Column {
      width: parent.width - 76
      anchors.verticalCenter: parent.verticalCenter
      spacing: 2

      Text {
        width: parent.width
        text: root.displayName
        color: Theme.foreground
        font.family: Theme.fontFamily
        font.pixelSize: Theme.rowTitleSize
        font.weight: Font.DemiBold
        elide: Text.ElideMiddle
      }
      Text {
        width: parent.width
        text: root.busy ? "Looking inside…" : (root.skip.length ? root.skip : (root.estimate.length ? root.estimate : root.path))
        color: root.skip.length ? Theme.brandAmber : Theme.secondary
        font.family: Theme.fontFamily
        font.pixelSize: Theme.captionSize
        elide: Text.ElideMiddle
      }
    }
  }
}
