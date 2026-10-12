import QtQuick
import QtQuick.Controls
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import qs.Commons
import qs.Ui

// Phase 0 spike: Overlay panel with Exclusive focus, rounded 0.75 fill, and a
// real TextField so fcitx5 text-input-v3 pre-edit works on layer-shell.
Item {
  id: root

  property var shell: null
  property var manifest: null
  property bool opened: false

  readonly property string pluginId: (manifest && manifest.id) ? String(manifest.id) : "io.github.thakurious.trowel-dev"
  readonly property int panelWidth: 780
  readonly property real topFraction: 0.16

  function open(payloadJson) {
    root.opened = true
    queryField.text = ""
    Qt.callLater(function() { queryField.forceActiveFocus() })
  }

  function close() {
    root.opened = false
  }

  function dismiss() {
    root.opened = false
    if (root.shell && typeof root.shell.hide === "function")
      root.shell.hide(root.pluginId)
  }

  function toggle() {
    if (root.opened)
      root.dismiss()
    else
      root.open("{}")
  }

  IpcHandler {
    target: "trowel"
    function open(): void { root.open("{}") }
    function close(): void { root.dismiss() }
    function toggle(): void { root.toggle() }
  }

  PanelWindow {
    id: panel
    visible: root.opened
    color: "transparent"
    exclusionMode: ExclusionMode.Ignore
    WlrLayershell.namespace: "trowel-panel"
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.keyboardFocus: visible ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None
    anchors { top: true; bottom: true; left: true; right: true }

    MouseArea {
      anchors.fill: parent
      onClicked: root.dismiss()
    }

    Item {
      id: card
      width: Math.min(root.panelWidth, panel.width - 24)
      height: fieldColumn.implicitHeight + 32
      x: Math.round((panel.width - width) / 2)
      y: Math.round(panel.height * root.topFraction)
      layer.enabled: false

      Rectangle {
        id: fill
        anchors.fill: parent
        radius: Math.max(Style.cornerRadius, 0)
        color: Qt.rgba(Color.background.r, Color.background.g, Color.background.b, 0.75)
        clip: true
        border.width: 1
        border.color: Qt.rgba(Color.foreground.r, Color.foreground.g, Color.foreground.b, 0.14)

        MouseArea {
          anchors.fill: parent
          onClicked: queryField.forceActiveFocus()
        }

        Column {
          id: fieldColumn
          anchors.left: parent.left
          anchors.right: parent.right
          anchors.top: parent.top
          anchors.margins: 16
          spacing: 8

          Text {
            text: "Trowel"
            color: Color.foreground
            font.family: Style.font.family
            font.pixelSize: Math.round(Style.font.body * 1.67)
            font.weight: Font.DemiBold
          }

          TextField {
            id: queryField
            width: parent.width
            placeholderText: "Type — fcitx5 pre-edit should appear here"
            Keys.onPressed: function(event) {
              if (event.key === Qt.Key_Escape) {
                if (queryField.text.length > 0)
                  queryField.text = ""
                else
                  root.dismiss()
                event.accepted = true
              }
            }
          }

          Text {
            width: parent.width
            wrapMode: Text.Wrap
            color: Qt.rgba(Color.foreground.r, Color.foreground.g, Color.foreground.b, 0.62)
            font.family: Style.font.family
            font.pixelSize: Math.round(Style.font.body * 0.92)
            text: {
              var pre = ""
              try { pre = queryField.preeditText } catch (e) { pre = "" }
              return "committed: “" + queryField.text + "”\npreedit: “" + pre + "”\nEsc closes · click outside dismisses"
            }
          }
        }
      }
    }
  }
}
