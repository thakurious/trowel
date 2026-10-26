import QtQuick
import Quickshell
import "Trowel"

// Offscreen Island harness: IslandView in a plain window (no layer shell offscreen), driven
// through its states; each step logs the shape's geometry and saves a picture.
ShellRoot {
  id: shell
  property int step: 0
  readonly property string out: Quickshell.env("HARNESS_OUT")
  readonly property real t0: Date.now() / 1000
  property var steps: [
    {"name": "hidden", "acts": []},
    {"name": "compact", "acts": [shell.keylock()]},
    {"name": "split", "acts": [shell.keylock(), shell.timer(false)]},
    {"name": "expanded", "acts": [shell.keylock(), shell.timer(false)], "expand": true},
    {"name": "alert", "acts": [shell.timer(true)]},
    {"name": "alert-settled", "acts": [shell.timer(true)], "wait": 4600},
    {"name": "progress", "acts": [{"id": "backup", "icon": "progress", "title": "Backing up", "progress": 0.4, "value": "40%", "priority": 10, "actions": []}]},
    {"name": "more", "acts": [shell.keylock(), shell.timer(false), {"id": "a", "title": "A", "priority": 5, "actions": []}, {"id": "b", "title": "B", "priority": 4, "actions": []}]},
    {"name": "gone", "acts": []},
    {"name": "reduced", "acts": [shell.keylock()], "reduce": true, "wait": 60}
  ]

  function keylock() {
    return {"id": "keylock", "icon": "lock", "title": "Keyboard locked", "subtitle": "The mouse still works", "ends_at": shell.t0 + 42, "total": 60, "priority": 100, "actions": [{"id": "unlock", "label": "Unlock"}]}
  }
  function timer(done) {
    return done ? {"id": "timer:7", "icon": "timer", "title": "Tea", "subtitle": "Time's up", "value": "Done", "priority": 90, "alert": true, "actions": [{"id": "add300", "label": "+5 min"}, {"id": "dismiss", "label": "Dismiss"}]}
      : {"id": "timer:7", "icon": "timer", "title": "Tea", "subtitle": "25:00 timer", "ends_at": shell.t0 + 1500, "total": 1500, "priority": 50, "actions": [{"id": "pause", "label": "Pause"}, {"id": "add60", "label": "+1 min"}, {"id": "cancel", "label": "Cancel", "danger": true}]}
  }

  FloatingWindow {
    id: win
    visible: true
    color: "#c9ccd1"
    implicitWidth: 460
    implicitHeight: 300
    Item {
      id: stage
      anchors.fill: parent
    // The bar the island hangs from.
    Rectangle {
      width: parent.width
      height: 26
      color: "#1e2127"
    }
    IslandView {
      id: view
      y: 32
      anchors.horizontalCenter: parent.horizontalCenter
      onAction: function (a, b) {
        console.log("HARNESS action " + JSON.stringify({"activity": a, "action": b}))
      }
    }
    }
  }

  Timer {
    id: timer
    interval: 300
    running: true
    property bool settle: false
    onTriggered: {
      if (settle) {
        var s = shell.steps[shell.step - 1]
        var pill = view.pillItem
        var bubble = view.bubbleItem
        console.log("HARNESS " + s.name + " " + JSON.stringify({
          "mode": view.mode,
          "mapped": view.mapped,
          "w": Math.round(pill.width),
          "h": Math.round(pill.height),
          "x": Math.round(pill.x),
          "y": Math.round(pill.y),
          "bubble": bubble.shown ? Math.round(bubble.width) : 0,
          "viewWidth": Math.round(view.width),
          "alert": view.alertId,
          "counting": view.counting
        }))
        stage.grabToImage(function (r) {
          r.saveToFile(shell.out + "/island-" + s.name + ".png")
        })
        settle = false
        interval = 300
        start()
        return
      }
      if (shell.step >= shell.steps.length) {
        Qt.quit()
        return
      }
      var st = shell.steps[shell.step++]
      view.reduceMotion = st.reduce === true
      view.expandedByUser = false
      view.hoverExpanded = false
      view.activities = st.acts
      if (st.expand)
        view.expandedByUser = true
      settle = true
      interval = st.wait || 1400
      start()
    }
  }
}
