import QtQuick

// PROTOCOL preview types: image, pdf_page, code, text, audio, video.
Item {
  id: root
  property var preview: ({})
  property var row: ({})
  clip: true

  readonly property string ptype: (preview && preview.type) ? preview.type : ""

  Image {
    anchors.fill: parent
    visible: root.ptype === "image" || root.ptype === "pdf_page" || root.ptype === "video"
    source: {
      if (!root.preview)
        return ""
      var f = root.preview.file || root.preview.frame || ""
      return f.length ? "file://" + f : ""
    }
    fillMode: Image.PreserveAspectFit
    asynchronous: true
    cache: true
    sourceSize.width: parent.width
    sourceSize.height: parent.height
  }

  Rectangle {
    visible: root.ptype === "pdf_page" && root.preview && root.preview.band
    width: parent.width
    y: {
      var b = root.preview && root.preview.band
      if (!b || b.length < 2)
        return 0
      return parent.height * b[0]
    }
    height: {
      var b = root.preview && root.preview.band
      if (!b || b.length < 2)
        return 0
      return parent.height * Math.max(0, b[1] - b[0])
    }
    color: Theme.alpha(Theme.accent, 0.18)
  }

  MomentBar {
    visible: root.ptype === "audio" || root.ptype === "video"
    anchors.left: parent.left
    anchors.right: parent.right
    anchors.bottom: parent.bottom
    anchors.margins: 8
    start: root.preview && root.preview.start ? root.preview.start : 0
    end: root.preview && root.preview.end ? root.preview.end : 0
    duration: root.preview && root.preview.duration ? root.preview.duration : 1
  }

  Text {
    visible: root.ptype === "text" || root.ptype === "code"
    anchors.fill: parent
    anchors.margins: 8
    text: {
      if (root.ptype === "code" && root.preview && root.preview.lines) {
        var out = []
        var lines = root.preview.lines
        for (var i = 0; i < lines.length; i++)
          out.push((lines[i].n || "") + "  " + (lines[i].text || ""))
        return out.join("\n")
      }
      return (root.preview && root.preview.text) ? root.preview.text : ""
    }
    color: Theme.foreground
    font.family: root.ptype === "code" ? Theme.monoFamily : Theme.fontFamily
    font.pixelSize: Theme.captionSize
    wrapMode: Text.Wrap
    elide: Text.ElideRight
  }

  EmptyState {
    visible: !root.ptype || root.ptype.length === 0
    anchors.centerIn: parent
    width: parent.width - 24
    title: (root.row && root.row.name) ? root.row.name : ""
    detail: (root.row && root.row.reason) ? root.row.reason : ""
  }
}
