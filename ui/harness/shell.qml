import QtQuick
import Quickshell

// Offscreen harness: drives Panel through a scripted session against `troweld --mock` and
// saves each state to out/<name>.png. Never touches the live session.
ShellRoot {
  id: shell
  property int step: 0
  property var steps: []
  readonly property string out: Quickshell.env("HARNESS_OUT")

  Panel {
    id: panel
  }

  Component.onCompleted: {
    var script = JSON.parse(Quickshell.env("HARNESS_SCRIPT") || "[]")
    shell.steps = script
    timer.start()
  }

  Timer {
    id: timer
    interval: 400
    onTriggered: {
      if (shell.step >= shell.steps.length) {
        Qt.quit()
        return
      }
      var s = shell.steps[shell.step++]
      var wait = s.wait || 400
      if (s.do === "ready")
        panel.harnessSkipOnboarding()
      else if (s.do === "open")
        panel.open(s.payload ? JSON.stringify(s.payload) : "{}")
      else if (s.do === "clipboard")
        panel.openClipboard()
      else if (s.do === "type")
        panel.harnessType(s.text)
      else if (s.do === "down")
        panel.moveSelection(1)
      else if (s.do === "up")
        panel.moveSelection(-1)
      else if (s.do === "tab")
        panel.harnessTab(s.id)
      else if (s.do === "close")
        panel.dismiss()
      else if (s.do === "age")
        panel.closedAt -= s.seconds

      else if (s.do === "command")
        panel.runCommand(s.id)
      else if (s.do === "enter")
        panel.openSelected()
      else if (s.do === "back")
        panel.goBack()
      else if (s.do === "actions")
        panel.harnessActions()
      else if (s.do === "log")
        console.log("HARNESS " + s.what + " " + panel.harnessState())
      else if (s.do === "shot")
        panel.harnessShot(shell.out + "/" + s.name + ".png")
      timer.interval = wait
      timer.start()
    }
  }
}
