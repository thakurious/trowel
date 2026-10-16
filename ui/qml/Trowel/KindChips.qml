pragma ComponentBehavior: Bound
import QtQuick

// LAUNCHER-PLAN.md L4 clipboard filter: All · Text · Images · Files · Links · Colors, styled as
// FilterBar's pills. Tab / Shift+Tab cycle; each sends `clip_query.kinds`.
Row {
  id: root
  property string current: "all"
  signal picked(string id)
  spacing: 6

  readonly property var chips: [{
      "id": "all",
      "label": "All"
    }, {
      "id": "text",
      "label": "Text"
    }, {
      "id": "image",
      "label": "Images"
    }, {
      "id": "files",
      "label": "Files"
    }, {
      "id": "link",
      "label": "Links"
    }, {
      "id": "color",
      "label": "Colors"
    }]
  readonly property var ids: chips.map(function (c) {
    return c.id
  })

  // "Text" covers everything that pastes as text: plain, code and formatted.
  function kindsFor(id) {
    if (id === "all")
      return []
    if (id === "text")
      return ["text", "code", "html"]
    return [id]
  }

  function cycle(step) {
    var i = ids.indexOf(root.current)
    pick(ids[(i + step + ids.length) % ids.length])
  }

  function pick(id) {
    if (ids.indexOf(id) < 0 || id === root.current)
      return
    root.current = id
    root.picked(id)
  }

  Repeater {
    model: root.chips
    delegate: Rectangle {
      id: pill
      required property var modelData
      implicitHeight: 24
      implicitWidth: lab.implicitWidth + 16
      radius: 12
      color: root.current === modelData.id ? Theme.selectedFill : Theme.keycapFill
      Text {
        id: lab
        anchors.centerIn: parent
        text: pill.modelData.label
        color: Theme.foreground
        font.pixelSize: Theme.captionSize
      }
      MouseArea {
        anchors.fill: parent
        onClicked: root.pick(pill.modelData.id)
      }
    }
  }
}
