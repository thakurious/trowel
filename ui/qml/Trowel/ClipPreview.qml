pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import "ListMerge.js" as ListMerge

// LAUNCHER-PLAN.md L4 preview column per clipboard kind: text and code (monospace, wrapped),
// link (domain, URL, open), color (swatch and copy chips), image (fit, size), files (list),
// html (its text, a "has formatting" pill). `content` is the `clip_get` reply.
Item {
  id: root
  property var row: ({})
  property var content: ({})
  property real now: Date.now() / 1000
  signal openLink(string url)
  signal copyText(string text)
  clip: true

  readonly property string kind: (row && row.kind) ? row.kind : ""
  readonly property bool loaded: !!(content && content.row && row && content.row.id === row.clipId)
  readonly property string text: loaded && content.text ? String(content.text) : ""
  readonly property var image: loaded && content.image ? content.image : null
  readonly property var files: loaded && content.files ? content.files : []

  function pathText(p) {
    return typeof p === "string" ? p : (p && p.display ? p.display : "")
  }

  function fileUrl(path) {
    var parts = String(path).split("/")
    for (var i = 0; i < parts.length; i++)
      parts[i] = encodeURIComponent(parts[i])
    return "file://" + parts.join("/")
  }

  // #RRGGBB[AA] → the rgb() and hsl() forms for the copy chips.
  function colorForms(hex) {
    var h = String(hex || "").replace("#", "")
    if (h.length < 6)
      return []
    var r = parseInt(h.substr(0, 2), 16), g = parseInt(h.substr(2, 2), 16), b = parseInt(h.substr(4, 2), 16)
    var R = r / 255, G = g / 255, B = b / 255
    var max = Math.max(R, G, B), min = Math.min(R, G, B), l = (max + min) / 2
    var s = 0, hue = 0
    if (max !== min) {
      var d = max - min
      s = l > 0.5 ? d / (2 - max - min) : d / (max + min)
      if (max === R)
        hue = (G - B) / d + (G < B ? 6 : 0)
      else if (max === G)
        hue = (B - R) / d + 2
      else
        hue = (R - G) / d + 4
      hue *= 60
    }
    return ["#" + h.toUpperCase(), "rgb(" + r + ", " + g + ", " + b + ")", "hsl(" + Math.round(hue) + ", " + Math.round(s * 100) + "%, " + Math.round(l * 100) + "%)"]
  }

  Rectangle {
    id: card
    anchors.fill: parent
    anchors.margins: 10
    anchors.bottomMargin: meta.height + 16
    radius: 8
    color: Theme.alpha(Theme.foreground, 0.045)
    border.width: Theme.hairlineWidth()
    border.color: Theme.alpha(Theme.foreground, 0.1)
    clip: true

    // Text, code and HTML: the content, scrollable.
    Flickable {
      id: textFlick
      anchors.fill: parent
      visible: root.kind === "text" || root.kind === "code" || root.kind === "html"
      clip: true
      boundsBehavior: Flickable.StopAtBounds
      contentWidth: width
      contentHeight: body.height + 28
      ScrollBar.vertical: ScrollBar {
        policy: ScrollBar.AsNeeded
      }

      Text {
        id: body
        x: 14
        y: 14
        width: textFlick.width - 28
        text: root.text.length > 20000 ? root.text.substring(0, 20000) + "…" : root.text
        textFormat: Text.PlainText
        color: Theme.foreground
        font.family: root.kind === "code" ? Theme.monoFamily : Theme.fontFamily
        font.pixelSize: Theme.captionSize
        lineHeight: root.kind === "code" ? 1.15 : 1.35
        wrapMode: root.kind === "code" ? Text.WrapAnywhere : Text.Wrap
      }
    }

    // Link.
    Column {
      anchors.centerIn: parent
      width: parent.width - 40
      spacing: 10
      visible: root.kind === "link"

      Text {
        width: parent.width
        horizontalAlignment: Text.AlignHCenter
        text: (root.row && root.row.domain) ? root.row.domain : ""
        color: Theme.foreground
        font.family: Theme.fontFamily
        font.pixelSize: Theme.emptyTitleSize
        font.weight: Font.DemiBold
        elide: Text.ElideRight
      }
      Text {
        width: parent.width
        horizontalAlignment: Text.AlignHCenter
        text: root.text.trim()
        color: Theme.secondary
        font.family: Theme.monoFamily
        font.pixelSize: Theme.captionSize
        wrapMode: Text.WrapAnywhere
        maximumLineCount: 4
        elide: Text.ElideRight
      }
      Chip {
        anchors.horizontalCenter: parent.horizontalCenter
        label: "Open in browser"
        onActivated: root.openLink(root.text.trim())
      }
    }

    // Color.
    Column {
      anchors.centerIn: parent
      spacing: 12
      visible: root.kind === "color"

      // Three chips and the gaps take ~140 px; the swatch gets what's left, 48–120 px.
      Rectangle {
        anchors.horizontalCenter: parent.horizontalCenter
        width: Math.max(48, Math.min(120, card.height - 150))
        height: width
        radius: 12
        color: (root.row && root.row.color) ? root.row.color : "transparent"
        border.width: Theme.hairlineWidth()
        border.color: Theme.hairline
      }
      Repeater {
        model: root.colorForms(root.row ? root.row.color : "")
        delegate: Chip {
          required property var modelData
          anchors.horizontalCenter: parent ? parent.horizontalCenter : undefined
          label: modelData
          mono: true
          onActivated: root.copyText(modelData)
        }
      }
    }

    // Image.
    Image {
      anchors.fill: parent
      anchors.margins: 1
      visible: root.kind === "image"
      source: root.image ? root.fileUrl(root.pathText(root.image.file)) : ""
      fillMode: Image.PreserveAspectFit
      asynchronous: true
      cache: false
      sourceSize.width: Math.max(Math.round(width), 132)
      sourceSize.height: Math.max(Math.round(height), 132)
    }

    // Files.
    ListView {
      anchors.fill: parent
      anchors.margins: 8
      visible: root.kind === "files"
      clip: true
      model: root.files
      spacing: 2
      delegate: Row {
        id: fileRow
        required property var modelData
        readonly property string path: root.pathText(modelData)
        spacing: 8
        height: 30
        KindIcon {
          anchors.verticalCenter: parent.verticalCenter
          kind: /\.(png|jpe?g|webp|gif|bmp|tiff?|heic|avif)$/i.test(fileRow.path) ? "image" : (/\.pdf$/i.test(fileRow.path) ? "pdf" : "doc")
          side: 18
        }
        Column {
          anchors.verticalCenter: parent.verticalCenter
          Text {
            text: fileRow.path.split("/").pop()
            color: Theme.foreground
            font.family: Theme.fontFamily
            font.pixelSize: Theme.captionSize
            font.weight: Font.Medium
          }
          Text {
            text: fileRow.path.replace(/\/[^\/]*$/, "") || "/"
            color: Theme.secondary
            font.family: Theme.fontFamily
            font.pixelSize: Math.round(Theme.fontBase * 0.83)
          }
        }
      }
    }

    Rectangle {
      visible: root.kind === "html"
      anchors.right: parent.right
      anchors.bottom: parent.bottom
      anchors.margins: 8
      width: pill.implicitWidth + 12
      height: pill.implicitHeight + 4
      radius: height / 2
      color: Theme.alpha(Theme.accent, 0.18)
      Text {
        id: pill
        anchors.centerIn: parent
        text: "Has formatting"
        color: Theme.foreground
        font.family: Theme.fontFamily
        font.pixelSize: Math.round(Theme.fontBase * 0.83)
        font.weight: Font.Medium
      }
    }
  }

  // Source app, when, how often, size.
  Text {
    id: meta
    anchors.left: parent.left
    anchors.right: parent.right
    anchors.bottom: parent.bottom
    anchors.margins: 12
    text: {
      var r = (root.row && root.row.raw) ? root.row.raw : null
      if (!r)
        return ""
      var parts = []
      if (r.source_app)
        parts.push(r.source_app)
      if (r.last_used)
        parts.push("copied " + ListMerge.ago(r.last_used, root.now))
      if (r.uses > 1)
        parts.push(r.uses + " uses")
      if (root.image && root.image.w)
        parts.push(root.image.w + " × " + root.image.h)
      if (r.bytes)
        parts.push(r.bytes < 1024 ? r.bytes + " B" : r.bytes < 1048576 ? Math.round(r.bytes / 1024) + " KB" : (r.bytes / 1048576).toFixed(1) + " MB")
      return parts.join(" · ")
    }
    color: Theme.secondary
    font.family: Theme.fontFamily
    font.pixelSize: Math.round(Theme.fontBase * 0.83)
    elide: Text.ElideRight
  }
}
