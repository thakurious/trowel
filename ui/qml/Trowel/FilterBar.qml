pragma ComponentBehavior: Bound
import QtQuick

Row {
  id: root
  property string current: "all"
  property var counts: ({})
  signal picked(string kind)
  spacing: 6

  Repeater {
    model: [{
        "id": "all",
        "label": "All",
        "countKey": ""
      }, {
        "id": "screenshot",
        "label": "Screenshots",
        "countKey": "screenshots"
      }, {
        "id": "image",
        "label": "Images",
        "countKey": "images"
      }, {
        "id": "doc",
        "label": "Documents",
        "countKey": "documents"
      }, {
        "id": "video",
        "label": "Video",
        "countKey": "video"
      }, {
        "id": "audio",
        "label": "Audio",
        "countKey": "audio"
      }, {
        "id": "code",
        "label": "Code",
        "countKey": "code"
      }]
    delegate: Rectangle {
      required property var modelData
      property bool on: root.current === modelData.id
      implicitHeight: 24
      implicitWidth: lab.implicitWidth + 16
      radius: 12
      color: on ? Theme.selectedFill : Theme.keycapFill
      Text {
        id: lab
        anchors.centerIn: parent
        text: {
          var key = modelData.countKey
          var n = key ? root.counts[key] : 0
          modelData.label + (n ? " " + n : "")
        }
        color: Theme.foreground
        font.pixelSize: Theme.captionSize
        font.features: {
          "tnum": 1
        }
      }
      MouseArea {
        anchors.fill: parent
        onClicked: {
          root.current = modelData.id
          root.picked(modelData.id)
        }
      }
    }
  }
}
