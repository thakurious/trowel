import QtQuick
import "DeskModel.js" as DeskModel

// The Timer view's right pane: what ↵ would start, or the selected timer counting down with its
// ring. The countdown ticks here (1 Hz, only while shown); the daemon fires the timer.
Item {
  id: root
  // A row of DeskModel.timerRows, and the daemon's timer for a running one.
  property var row: null
  property var timer: null
  property real now: Date.now() / 1000

  readonly property bool fresh: root.row !== null && root.row.rowType === "timer-new"
  readonly property real remaining: root.timer ? DeskModel.timerLeft(root.timer, root.now) : (root.row ? root.row.seconds : 0)
  readonly property real total: root.timer ? root.timer.total : (root.row ? root.row.seconds : 0)

  Timer {
    interval: 1000
    repeat: true
    running: root.visible && root.timer !== null && root.timer.state === "running"
    onTriggered: root.now = Date.now() / 1000
  }
  onTimerChanged: root.now = Date.now() / 1000

  Column {
    anchors.centerIn: parent
    width: parent.width - 48
    spacing: 14
    visible: root.row !== null

    Item {
      width: 150
      height: 150
      anchors.horizontalCenter: parent.horizontalCenter
      Canvas {
        id: ring
        anchors.fill: parent
        property real fraction: root.total > 0 ? root.remaining / root.total : 0
        onFractionChanged: requestPaint()
        onPaint: {
          var ctx = getContext("2d")
          ctx.reset()
          var r = width / 2 - 6
          ctx.lineWidth = 6
          ctx.lineCap = "round"
          ctx.strokeStyle = Theme.hairline
          ctx.beginPath()
          ctx.arc(width / 2, height / 2, r, 0, Math.PI * 2)
          ctx.stroke()
          ctx.strokeStyle = Theme.accent
          ctx.beginPath()
          ctx.arc(width / 2, height / 2, r, -Math.PI / 2, -Math.PI / 2 + Math.PI * 2 * Math.max(0.005, fraction))
          ctx.stroke()
        }
      }
      Text {
        anchors.centerIn: parent
        text: root.timer && root.timer.state === "done" ? "Done" : DeskModel.clock(root.remaining)
        color: Theme.foreground
        font.family: Theme.monoFamily
        font.pixelSize: 30
        font.weight: Font.DemiBold
        font.features: {
          "tnum": 1
        }
      }
    }

    Text {
      width: parent.width
      horizontalAlignment: Text.AlignHCenter
      text: root.row ? (root.row.label || (root.fresh ? "New timer" : "Timer")) : ""
      color: Theme.foreground
      font.family: Theme.fontFamily
      font.pixelSize: Theme.rowTitleSize + 2
      font.weight: Font.DemiBold
      elide: Text.ElideRight
    }

    Text {
      width: parent.width
      horizontalAlignment: Text.AlignHCenter
      wrapMode: Text.WordWrap
      text: {
        if (root.fresh)
          return "↵ starts it and closes the panel. It counts down on the Island under the bar and rings when it's done."
        if (!root.timer)
          return ""
        if (root.timer.state === "paused")
          return "Paused. ↵ resumes it."
        if (root.timer.state === "done")
          return root.timer.late ? "Finished while the computer was asleep." : "Finished."
        return "Ends at " + Qt.formatTime(new Date((root.timer.ends_at || 0) * 1000), "hh:mm") + ". ↵ pauses it."
      }
      color: Theme.secondary
      font.family: Theme.fontFamily
      font.pixelSize: Theme.captionSize
    }
  }

  EmptyState {
    visible: root.row === null
    anchors.centerIn: parent
    width: parent.width - 48
    title: "Type how long"
    detail: "25m, 1h 30m, 90s, at 15:30, or 10 tea."
  }
}
