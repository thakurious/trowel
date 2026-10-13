pragma Singleton
import QtQuick
import Quickshell.Io

// UI-DESIGN §4. Every duration becomes 0 when reduce_motion is on.
QtObject {
  id: root

  property bool reduceMotion: false
  readonly property int openMs: ms(140)
  readonly property int closeMs: ms(90)
  readonly property int heightMs: ms(160)
  readonly property int highlightMs: ms(90)
  readonly property int swapMs: ms(80)
  readonly property int previewMs: ms(120)
  readonly property int thumbMs: ms(100)
  readonly property int foldMs: ms(160)
  readonly property int hairlineLoopMs: ms(900)
  readonly property int progressMs: ms(250)
  readonly property int hoverMs: ms(80)

  readonly property int openEasing: Easing.OutCubic
  readonly property int closeEasing: Easing.InCubic
  readonly property int heightEasing: Easing.OutQuint
  readonly property int highlightEasing: Easing.OutCubic
  readonly property int foldEasing: Easing.OutQuint

  function ms(n) {
    return root.reduceMotion ? 0 : n
  }

  property Process gsettings: Process {
    command: ["gsettings", "get", "org.gnome.desktop.interface", "enable-animations"]
    running: true
    stdout: SplitParser {
      onRead: function (line) {
        var s = String(line).replace(/^\s+|\s+$/g, "")
        if (s === "false" || s === "'false'")
          root.reduceMotion = true
      }
    }
  }
}
