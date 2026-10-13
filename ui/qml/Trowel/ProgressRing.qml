import QtQuick

// StatusMenu.swift progress ring / paused bars / idle D-mark.
Item {
  id: root
  property real progress: -1
  property bool paused: false
  property bool updateDot: false
  implicitWidth: 18
  implicitHeight: 18

  Canvas {
    id: canvas
    anchors.fill: parent
    onPaint: {
      var ctx = getContext("2d")
      ctx.reset()
      var s = width / 64
      ctx.scale(s, s)
      ctx.strokeStyle = Theme.foreground
      ctx.fillStyle = Theme.foreground
      ctx.lineWidth = 4.5
      ctx.lineCap = "round"
      if (root.progress >= 0) {
        ctx.strokeStyle = Theme.alpha(Theme.foreground, 0.25)
        ctx.beginPath()
        ctx.arc(32, 32, 27, 0, Math.PI * 2)
        ctx.stroke()
        ctx.strokeStyle = Theme.foreground
        ctx.beginPath()
        var p = Math.max(0.03, Math.min(1, root.progress))
        ctx.arc(32, 32, 27, -Math.PI / 2, -Math.PI / 2 + Math.PI * 2 * p, false)
        ctx.stroke()
      } else {
        roundedRect(ctx, 4.5, 4.5, 55, 55, 12)
        ctx.stroke()
        var ticks = [[32, 5, 32, 16], [32, 48, 32, 59], [5, 32, 17, 32], [47, 32, 59, 32]]
        for (var i = 0; i < ticks.length; i++) {
          ctx.beginPath()
          ctx.moveTo(ticks[i][0], ticks[i][1])
          ctx.lineTo(ticks[i][2], ticks[i][3])
          ctx.stroke()
        }
      }
      ctx.fillStyle = Theme.foreground
      if (root.paused && root.progress < 0) {
        roundedRect(ctx, 23.5, 21, 6, 22, 1.8)
        ctx.fill()
        roundedRect(ctx, 34.5, 21, 6, 22, 1.8)
        ctx.fill()
      } else if (root.progress < 0) {
        ctx.beginPath()
        ctx.moveTo(25, 21)
        ctx.lineTo(30, 21)
        ctx.arc(30, 32, 11, -Math.PI / 2, Math.PI / 2, false)
        ctx.lineTo(25, 43)
        ctx.arc(25, 41, 2, Math.PI / 2, Math.PI, false)
        ctx.lineTo(23, 23)
        ctx.arc(25, 23, 2, Math.PI, Math.PI * 1.5, false)
        ctx.closePath()
        ctx.fill()
      }
      if (root.updateDot) {
        ctx.beginPath()
        ctx.arc(53, 11, 7, 0, Math.PI * 2)
        ctx.fill()
      }
    }
    function roundedRect(ctx, x, y, w, h, r) {
      ctx.beginPath()
      ctx.moveTo(x + r, y)
      ctx.lineTo(x + w - r, y)
      ctx.quadraticCurveTo(x + w, y, x + w, y + r)
      ctx.lineTo(x + w, y + h - r)
      ctx.quadraticCurveTo(x + w, y + h, x + w - r, y + h)
      ctx.lineTo(x + r, y + h)
      ctx.quadraticCurveTo(x, y + h, x, y + h - r)
      ctx.lineTo(x, y + r)
      ctx.quadraticCurveTo(x, y, x + r, y)
      ctx.closePath()
    }
  }

  onProgressChanged: canvas.requestPaint()
  onPausedChanged: canvas.requestPaint()
  onUpdateDotChanged: canvas.requestPaint()
  Connections {
    target: Theme
    function onForegroundChanged() {
      canvas.requestPaint()
    }
  }
}
