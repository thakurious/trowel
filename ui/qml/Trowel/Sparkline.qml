import QtQuick

// The last 60 samples of one number (Process Monitor pane): a line with a soft fill, scaled to
// `max` (or the largest sample), the latest value as the caption.
Item {
  id: root
  property var values: []
  property real max: 0
  property string label: ""
  property string caption: ""
  implicitHeight: 52

  Text {
    id: head
    anchors.left: parent.left
    anchors.top: parent.top
    text: root.label
    color: Theme.secondary
    font.family: Theme.fontFamily
    font.pixelSize: Theme.captionSize
  }
  Text {
    anchors.right: parent.right
    anchors.top: parent.top
    text: root.caption
    color: Theme.foreground
    font.family: Theme.monoFamily
    font.pixelSize: Theme.captionSize
    font.features: {
      "tnum": 1
    }
  }

  Canvas {
    id: canvas
    anchors.left: parent.left
    anchors.right: parent.right
    anchors.top: head.bottom
    anchors.topMargin: 4
    anchors.bottom: parent.bottom
    onPaint: {
      var ctx = getContext("2d")
      ctx.reset()
      var v = root.values || []
      ctx.strokeStyle = Theme.hairline
      ctx.lineWidth = 1
      ctx.beginPath()
      ctx.moveTo(0, height - 0.5)
      ctx.lineTo(width, height - 0.5)
      ctx.stroke()
      if (v.length < 2)
        return
      var top = root.max > 0 ? root.max : Math.max.apply(null, v.concat([1e-9]))
      var step = width / 59
      var x0 = width - (v.length - 1) * step
      ctx.beginPath()
      for (var i = 0; i < v.length; i++) {
        var x = x0 + i * step
        var y = height - 1 - (height - 2) * Math.min(1, v[i] / top)
        if (i === 0)
          ctx.moveTo(x, y)
        else
          ctx.lineTo(x, y)
      }
      ctx.strokeStyle = Theme.accent
      ctx.lineWidth = 1.5
      ctx.stroke()
      ctx.lineTo(width, height)
      ctx.lineTo(x0, height)
      ctx.closePath()
      ctx.fillStyle = Theme.alpha(Theme.accent, 0.12)
      ctx.fill()
    }
  }

  onValuesChanged: canvas.requestPaint()
  onWidthChanged: canvas.requestPaint()
}
