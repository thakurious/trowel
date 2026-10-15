pragma ComponentBehavior: Bound
import QtQuick

Row {
  id: root
  property string current: "all"
  property var counts: ({})
  // Code is a tab of its own (the code index) when code folders are chosen.
  property bool showCode: true
  // Kinds with no strong result for the query are dimmed and skipped by Tab.
  property bool dimEmpty: false
  signal picked(string kind)
  spacing: 6

  readonly property var ids: showCode ? ["all", "screenshot", "image", "doc", "video", "audio", "code"] : ["all", "screenshot", "image", "doc", "video", "audio"]
  readonly property var countKeys: ({
      "screenshot": "screenshots",
      "image": "images",
      "doc": "documents",
      "video": "video",
      "audio": "audio",
      "code": "code"
    })

  // The file kinds a tab sends as `search.kinds`. Documents are PDFs and documents
  // (`upstream/Sources/DigUpApp/Results.swift:163`); All and Code send none.
  function kindsFor(id) {
    if (id === "doc")
      return ["doc", "pdf"]
    if (id === "all" || id === "code")
      return []
    return [id]
  }

  function countOf(id) {
    var key = countKeys[id]
    return key && root.counts[key] ? root.counts[key] : 0
  }

  function isEmpty(id) {
    return root.dimEmpty && id !== "all" && id !== "code" && id !== root.current && countOf(id) === 0
  }

  // Tab / Shift+Tab: the next tab that has results (All and Code always count).
  function cycle(step) {
    var i = ids.indexOf(root.current)
    for (var n = 0; n < ids.length; n++) {
      i = (i + step + ids.length) % ids.length
      if (!isEmpty(ids[i])) {
        pick(ids[i])
        return
      }
    }
  }

  function pick(id) {
    if (ids.indexOf(id) < 0 || id === root.current)
      return
    root.current = id
    root.picked(id)
  }

  readonly property var tabs: {
    var all = [{
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
    return root.showCode ? all : all.slice(0, all.length - 1)
  }

  Repeater {
    model: root.tabs
    delegate: Rectangle {
      id: pill
      required property var modelData
      property bool on: root.current === modelData.id
      implicitHeight: 24
      implicitWidth: lab.implicitWidth + 16
      radius: 12
      color: on ? Theme.selectedFill : Theme.keycapFill
      opacity: root.isEmpty(modelData.id) ? 0.45 : 1
      Text {
        id: lab
        anchors.centerIn: parent
        text: {
          var n = root.countOf(pill.modelData.id)
          return pill.modelData.label + (n ? " " + n : "")
        }
        color: Theme.foreground
        font.pixelSize: Theme.captionSize
        font.features: {
          "tnum": 1
        }
      }
      MouseArea {
        anchors.fill: parent
        onClicked: root.pick(pill.modelData.id)
      }
    }
  }
}
