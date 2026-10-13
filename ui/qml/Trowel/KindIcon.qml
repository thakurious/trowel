import QtQuick
import QtQuick.Effects

Item {
  id: root
  property string kind: "doc"
  property int side: 24
  width: side
  height: side

  Image {
    id: img
    anchors.fill: parent
    source: Qt.resolvedUrl("icons/" + root.iconName() + ".svg")
    sourceSize.width: root.side
    sourceSize.height: root.side
    asynchronous: true
    cache: true
    visible: false
  }

  MultiEffect {
    anchors.fill: img
    source: img
    colorization: 1
    colorizationColor: Theme.foreground
  }

  function iconName() {
    switch (kind) {
    case "screenshot":
      return "screenshot"
    case "image":
      return "image"
    case "pdf":
      return "pdf"
    case "audio":
      return "audio"
    case "video":
      return "video"
    case "code":
      return "code"
    case "folder":
      return "folder"
    default:
      return "doc"
    }
  }
}
