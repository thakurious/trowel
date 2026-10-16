pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import "ListMerge.js" as ListMerge

// LAUNCHER-PLAN.md L4 action panel (Ctrl+K): everything the selected row can do, with its keys,
// filtered as you type. `actions` is [{id, label, keys}]; Enter runs one, Escape closes.
Rectangle {
  id: root
  property var actions: []
  property int current: 0
  signal triggered(string id)
  signal closed
  visible: false
  width: 300
  height: Math.min(list.contentHeight, 280) + field.height + 12
  radius: Theme.innerRadius(Theme.panelRadius()) + 4 || 10
  color: Theme.alpha(Theme.background, 0.97)
  border.width: Theme.hairlineWidth()
  border.color: Theme.hairline

  readonly property var shown: ListMerge.filterActions(root.actions, field.text)

  function open() {
    field.text = ""
    root.current = 0
    root.visible = true
    field.forceActiveFocus()
  }

  function close() {
    root.visible = false
    root.closed()
  }

  function run(i) {
    var a = root.shown[i]
    if (!a)
      return
    root.visible = false
    root.triggered(a.id)
  }

  TextField {
    id: field
    x: 6
    y: 6
    width: parent.width - 12
    placeholderText: "Search actions"
    color: Theme.foreground
    placeholderTextColor: Theme.secondary
    font.family: Theme.fontFamily
    font.pixelSize: Theme.rowTitleSize
    background: Rectangle {
      color: Theme.keycapFill
      radius: 6
    }
    onTextChanged: root.current = 0
    Keys.onPressed: function (event) {
      if (event.key === Qt.Key_Escape || ((event.modifiers & Qt.ControlModifier) && event.key === Qt.Key_K)) {
        root.close()
        event.accepted = true
      } else if (event.key === Qt.Key_Down || ((event.modifiers & Qt.ControlModifier) && event.key === Qt.Key_N)) {
        root.current = Math.min(root.shown.length - 1, root.current + 1)
        event.accepted = true
      } else if (event.key === Qt.Key_Up || ((event.modifiers & Qt.ControlModifier) && event.key === Qt.Key_P)) {
        root.current = Math.max(0, root.current - 1)
        event.accepted = true
      } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
        root.run(root.current)
        event.accepted = true
      }
    }
  }

  ListView {
    id: list
    anchors.top: field.bottom
    anchors.topMargin: 4
    anchors.left: parent.left
    anchors.right: parent.right
    anchors.bottom: parent.bottom
    anchors.margins: 6
    clip: true
    model: root.shown
    currentIndex: root.current
    delegate: Rectangle {
      id: item
      required property var modelData
      required property int index
      width: list.width
      height: 30
      radius: 6
      color: item.index === root.current ? Theme.selectedFill : "transparent"
      Text {
        anchors.left: parent.left
        anchors.leftMargin: 8
        anchors.right: keys.left
        anchors.rightMargin: 8
        anchors.verticalCenter: parent.verticalCenter
        text: item.modelData.label
        color: Theme.foreground
        font.family: Theme.fontFamily
        font.pixelSize: Theme.captionSize
        elide: Text.ElideRight
      }
      Keycap {
        id: keys
        visible: !!item.modelData.keys
        anchors.right: parent.right
        anchors.rightMargin: 6
        anchors.verticalCenter: parent.verticalCenter
        label: item.modelData.keys || ""
        size: 10
      }
      MouseArea {
        anchors.fill: parent
        onClicked: root.run(item.index)
      }
    }
  }

  Text {
    anchors.centerIn: list
    visible: root.shown.length === 0
    text: "No matching actions"
    color: Theme.secondary
    font.family: Theme.fontFamily
    font.pixelSize: Theme.captionSize
  }
}
