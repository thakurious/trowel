import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import "KeyLockModel.js" as KeyLockModel

// While `trowel keyboard lock` runs: a small pill at the top center, over the bar, with the
// countdown and an Unlock button for the mouse (the keyboard is off). It reads
// `$XDG_RUNTIME_DIR/trowel/keyboard-lock.json`, which `trowel keyboard unlock` (the systemd timer,
// or this button) removes. Not a bar widget (DECISIONS D25): a layer surface of the panel plugin.
Item {
  id: root
  readonly property string statePath: (Quickshell.env("XDG_RUNTIME_DIR") || "/tmp") + "/trowel/keyboard-lock.json"
  property real until: 0
  property real total: 0
  property real now: Date.now() / 1000
  property bool unlocking: false
  readonly property real remaining: root.until - root.now
  readonly property bool locked: root.until > 0 && root.remaining > -2 && !root.unlocking

  function read() {
    var t = String(lockFile.text() || "")
    if (!t.length) {
      root.until = 0
      root.unlocking = false
      return
    }
    try {
      var v = JSON.parse(t)
      root.until = Number(v.until) || 0
      root.total = Number(v.seconds) || 0
    } catch (e) {
      root.until = 0
    }
  }

  FileView {
    id: lockFile
    path: root.statePath
    watchChanges: true
    printErrors: false
    onLoaded: root.read()
    onLoadFailed: root.read()
    onFileChanged: reload()
  }

  // inotify can't watch a file that doesn't exist yet: look again every 2 s while unlocked.
  Timer {
    interval: root.locked ? 250 : 2000
    running: true
    repeat: true
    onTriggered: {
      root.now = Date.now() / 1000
      if (!root.locked || root.remaining < 0)
        lockFile.reload()
    }
  }

  Process {
    id: unlock
    command: ["trowel", "keyboard", "unlock"]
    onExited: lockFile.reload()
  }

  PanelWindow {
    id: win
    visible: root.locked
    color: "transparent"
    exclusionMode: ExclusionMode.Ignore
    WlrLayershell.namespace: "trowel-popup"
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
    anchors.top: true
    margins.top: 2
    implicitWidth: pill.width
    implicitHeight: 22

    Rectangle {
      id: pill
      width: row.implicitWidth + 8
      height: 22
      radius: 11
      color: Theme.alpha(Theme.background, 0.94)
      border.width: 1
      border.color: Theme.alpha(Theme.accent, 0.7)
      clip: true

      // Time left, as a fill behind the text.
      Rectangle {
        anchors.left: parent.left
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        width: root.total > 0 ? parent.width * Math.max(0, Math.min(1, root.remaining / root.total)) : 0
        color: Theme.alpha(Theme.accent, 0.16)
        radius: 11
      }

      Row {
        id: row
        anchors.verticalCenter: parent.verticalCenter
        x: 10
        spacing: 8
        Text {
          anchors.verticalCenter: parent.verticalCenter
          text: ""
          color: Theme.accent
          font.family: Theme.monoFamily
          font.pixelSize: 11
        }
        Text {
          anchors.verticalCenter: parent.verticalCenter
          text: "Keyboard locked"
          color: Theme.foreground
          font.family: Theme.fontFamily
          font.pixelSize: 12
          font.weight: Font.Medium
        }
        Text {
          anchors.verticalCenter: parent.verticalCenter
          text: KeyLockModel.clock(root.remaining)
          color: Theme.accent
          font.family: Theme.monoFamily
          font.pixelSize: 12
          font.weight: Font.DemiBold
        }
        Rectangle {
          anchors.verticalCenter: parent.verticalCenter
          width: unlockText.implicitWidth + 14
          height: 16
          radius: 8
          color: unlockMouse.containsMouse ? Theme.accent : Theme.alpha(Theme.accent, 0.22)
          Text {
            id: unlockText
            anchors.centerIn: parent
            text: "Unlock"
            color: unlockMouse.containsMouse ? Theme.background : Theme.foreground
            font.family: Theme.fontFamily
            font.pixelSize: 11
            font.weight: Font.Medium
          }
          MouseArea {
            id: unlockMouse
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: {
              root.unlocking = true
              unlock.running = true
            }
          }
        }
      }
    }
  }
}
