import QtQuick

// On/off switch, drawn the way omarchy-shell's `Ui/ToggleSwitch.qml` draws it (track, sliding
// knob, pill only when the theme rounds corners). The row that holds it owns the click and keys.
Item {
  id: root
  property bool checked: false
  readonly property bool rounded: Theme.cornerRadius > 0
  implicitWidth: 40
  implicitHeight: 22

  Rectangle {
    anchors.fill: parent
    radius: root.rounded ? height / 2 : 3
    color: root.checked ? Theme.accent : Theme.alpha(Theme.foreground, 0.16)
    Behavior on color {
      ColorAnimation {
        duration: Motion.highlightMs
      }
    }
  }

  Rectangle {
    width: 16
    height: 16
    radius: root.rounded ? 8 : 2
    y: 3
    x: root.checked ? root.width - width - 3 : 3
    color: root.checked ? Theme.background : Theme.foreground
    Behavior on x {
      NumberAnimation {
        duration: Motion.highlightMs
        easing.type: Motion.highlightEasing
      }
    }
  }
}
