import QtQuick
import Quickshell
import Quickshell.Hyprland
import Quickshell.Wayland

// The Island's surface (P-D29): a layer of the panel plugin just below the bar, centered on the
// monitor Hyprland had focused when it appeared. Not a bar widget (D25). The input mask is the
// pill and its bubble only, so the rest of the strip stays clickable, and it never takes the
// keyboard. Replaces the keyboard-lock pill, which sat on top of the bar.
Item {
  id: root
  required property var client
  // `Style.bar.sizeHorizontal` from the plugin host (P0: 26 on this machine).
  property int barHeight: 26
  property var targetScreen: null

  function focusedScreen() {
    var m = Hyprland.focusedMonitor
    var name = m ? String(m.name || "") : ""
    var all = Quickshell.screens
    for (var i = 0; i < all.length; i++)
      if (all[i].name === name)
        return all[i]
    return all.length ? all[0] : null
  }

  Connections {
    target: view
    function onMappedChanged() {
      if (view.mapped)
        root.targetScreen = root.focusedScreen()
    }
  }

  PanelWindow {
    id: win
    visible: view.mapped
    screen: root.targetScreen
    color: "transparent"
    exclusionMode: ExclusionMode.Ignore
    WlrLayershell.namespace: "trowel-popup"
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
    anchors.top: true
    margins.top: root.barHeight + 6
    // A fixed surface the size of the largest state: resizing a layer surface every spring frame
    // costs a configure round trip each time.
    implicitWidth: view.implicitWidth
    implicitHeight: view.implicitHeight
    mask: Region {
      item: view.pillItem
      Region {
        item: view.bubbleItem
      }
    }

    IslandView {
      id: view
      anchors.fill: parent
      activities: root.client.island || []
      onAction: function (activityId, actionId) {
        root.client.islandAction(activityId, actionId)
      }
    }
  }
}
