import QtQuick
import QtQuick.Controls

// PROTOCOL preview types: image, pdf_page, code, text, audio, video.
// DigUp PreviewPicture / CodeLines / Passage — not an embedded editor.
// `upstream/Sources/DigUpApp/Previews.swift`
Item {
  id: root
  property var preview: ({})
  property var row: ({})
  // The query as typed: its words are marked in passages and code lines.
  property string query: ""
  clip: true

  // `upstream/Sources/DigUpApp/Previews.swift:401` Passage.marked: the row's found words (the «»
  // in its excerpt, which carry prefix matches like "Grok4" for "grok") and the query's words,
  // two letters or more, marked as whole words. Case-insensitive; no diacritic folding here.
  readonly property var markWords: {
    var words = []
    var seen = ({})
    function add(w) {
      var k = String(w).toLowerCase()
      if (k.length >= 2 && !seen[k]) {
        seen[k] = true
        words.push(k)
      }
    }
    var excerpt = (row && row.excerpt) ? String(row.excerpt) : ""
    var re = /«([^»]+)»/g
    var m
    while ((m = re.exec(excerpt)) !== null)
      add(m[1])
    var q = String(root.query || "").replace(/^\s*code:\s*/i, "")
    var parts = q.split(/[\s.,;:!?"'()\[\]{}<>\/\\|@#$%^&*+=~`]+/)
    for (var i = 0; i < parts.length; i++)
      add(parts[i])
    words.sort(function (a, b) {
      return b.length - a.length
    })
    return words
  }

  function isWordChar(ch) {
    return ch.length > 0 && (/[0-9_]/.test(ch) || ch.toLowerCase() !== ch.toUpperCase())
  }

  function escapeHtml(t) {
    return t.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;")
  }

  // Rich text with the words marked, or "" when nothing is marked (the plain path is cheaper).
  function marked(text, keepSpaces) {
    var words = root.markWords
    if (!words.length || !text.length)
      return ""
    var lower = text.toLowerCase()
    var spans = []
    for (var w = 0; w < words.length && spans.length < 500; w++) {
      var word = words[w]
      var from = 0
      while (spans.length < 500) {
        var at = lower.indexOf(word, from)
        if (at < 0)
          break
        from = at + word.length
        var end = at + word.length
        if ((at > 0 && isWordChar(text.charAt(at - 1))) || (end < text.length && isWordChar(text.charAt(end))))
          continue
        var overlaps = false
        for (var k = 0; k < spans.length; k++) {
          if (at < spans[k][1] && end > spans[k][0]) {
            overlaps = true
            break
          }
        }
        if (!overlaps)
          spans.push([at, end])
      }
    }
    if (!spans.length)
      return ""
    spans.sort(function (a, b) {
      return a[0] - b[0]
    })
    var out = ""
    var pos = 0
    for (var j = 0; j < spans.length; j++) {
      out += escapeHtml(text.substring(pos, spans[j][0]))
      out += "<span style=\"background-color:#ffdb33;color:#000000\">" + escapeHtml(text.substring(spans[j][0], spans[j][1])) + "</span>"
      pos = spans[j][1]
    }
    out += escapeHtml(text.substring(pos))
    if (keepSpaces)
      return "<span style=\"white-space:pre\">" + out + "</span>"
    return out.replace(/\n/g, "<br>")
  }

  readonly property string ptype: (preview && preview.type) ? preview.type : ""
  readonly property bool showPicture: ptype === "image" || ptype === "pdf_page" || ptype === "video"
  readonly property bool showCode: ptype === "code"
  readonly property bool showText: ptype === "text"
  readonly property bool showAudio: ptype === "audio"
  readonly property var codeLines: (preview && preview.lines) ? preview.lines : []
  readonly property string passageBody: {
    var source = (preview && preview.text) ? String(preview.text) : ""
    if (source.length <= 900)
      return source
    var start = 0
    var excerpt = (row && row.excerpt) ? String(row.excerpt) : ""
    var marked = excerpt.match(/«([^»]+)»/)
    var needle = marked && marked[1] ? marked[1] : ""
    if (needle.length >= 2) {
      var idx = source.toLowerCase().indexOf(needle.toLowerCase())
      if (idx > 200)
        start = Math.max(0, idx - 160)
    }
    var window = source.substring(start, start + 900)
    return (start > 0 ? "…" : "") + window + (start + 900 < source.length ? "…" : "")
  }

  readonly property int focusLine: {
    if (preview && preview.focus_line)
      return Math.round(Number(preview.focus_line))
    if (row && row.focus)
      return Math.round(Number(row.focus))
    if (row && row.loc)
      return Math.round(Number(row.loc))
    return 0
  }

  function fileUrl(path) {
    var f = path ? String(path) : ""
    if (!f.length)
      return ""
    if (f.indexOf("file:") === 0)
      return f
    var parts = f.split("/")
    for (var i = 0; i < parts.length; i++)
      parts[i] = encodeURIComponent(parts[i])
    return "file://" + parts.join("/")
  }

  function picturePath() {
    if (!root.preview)
      return ""
    var f = root.preview.file || root.preview.frame || ""
    if (f && String(f).length)
      return String(f)
    if (root.showPicture && root.row) {
      var kind = root.row.kind || ""
      if (kind === "image" || kind === "screenshot" || kind === "pdf" || kind === "video")
        return root.row.rowId || ""
    }
    return ""
  }

  Rectangle {
    id: card
    anchors.fill: parent
    anchors.margins: 10
    visible: root.showPicture || root.showCode || root.showText || root.showAudio
    radius: 8
    color: Theme.alpha(Theme.foreground, 0.045)
    border.width: Theme.hairlineWidth()
    border.color: Theme.alpha(Theme.foreground, 0.1)
    clip: true

    Image {
      id: pic
      anchors.fill: parent
      anchors.margins: 1
      visible: root.showPicture
      source: root.showPicture ? root.fileUrl(root.picturePath()) : ""
      fillMode: Image.PreserveAspectFit
      asynchronous: true
      cache: true
      sourceSize.width: Math.max(Math.round(width), 132)
      sourceSize.height: Math.max(Math.round(height), 132)
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

    ListView {
      id: codeView
      anchors.fill: parent
      visible: root.showCode
      clip: true
      boundsBehavior: Flickable.StopAtBounds
      model: root.codeLines
      spacing: 1
      ScrollBar.vertical: ScrollBar {
        policy: ScrollBar.AsNeeded
      }

      header: Item {
        width: 1
        height: 8
      }
      footer: Item {
        width: 1
        height: 8
      }

      delegate: Item {
        width: codeView.width
        height: Math.max(lineText.implicitHeight, numText.implicitHeight) + 1
        readonly property var lineObj: (typeof modelData !== "undefined") ? modelData : ({})
        readonly property int lineNo: Number(lineObj && lineObj.n ? lineObj.n : index + 1)
        readonly property bool isFocus: root.focusLine > 0 && lineNo === root.focusLine

        Rectangle {
          anchors.fill: parent
          visible: parent.isFocus
          color: Theme.alpha(Theme.brandAmber, 0.16)
        }

        Text {
          id: numText
          anchors.left: parent.left
          anchors.leftMargin: 8
          width: 36
          text: String(lineNo)
          color: Theme.muted
          font.family: Theme.monoFamily
          font.pixelSize: Theme.captionSize
          horizontalAlignment: Text.AlignRight
        }

        Text {
          id: lineText
          anchors.left: numText.right
          anchors.leftMargin: 10
          anchors.right: parent.right
          anchors.rightMargin: 8
          readonly property string plain: (lineObj && lineObj.text) ? String(lineObj.text) : ""
          readonly property string rich: root.marked(plain, true)
          text: rich.length ? rich : plain
          textFormat: rich.length ? Text.RichText : Text.PlainText
          color: Theme.foreground
          font.family: Theme.monoFamily
          font.pixelSize: Theme.captionSize
          wrapMode: Text.NoWrap
          elide: Text.ElideRight
        }
      }

      onModelChanged: Qt.callLater(root.scrollCodeToFocus)
    }

    Flickable {
      id: passageFlick
      anchors.fill: parent
      visible: root.showText
      clip: true
      boundsBehavior: Flickable.StopAtBounds
      contentWidth: width
      contentHeight: passageText.height + 28
      ScrollBar.vertical: ScrollBar {
        policy: ScrollBar.AsNeeded
      }
      ScrollBar.horizontal: ScrollBar {
        policy: ScrollBar.AlwaysOff
      }

      Text {
        id: passageText
        x: 14
        y: 14
        width: passageFlick.width - 28
        readonly property string rich: root.marked(root.passageBody, false)
        text: rich.length ? rich : root.passageBody
        textFormat: rich.length ? Text.RichText : Text.PlainText
        color: Theme.foreground
        font.family: Theme.fontFamily
        font.pixelSize: Theme.captionSize
        lineHeight: 1.35
        wrapMode: Text.Wrap
      }
    }

    MomentBar {
      visible: root.showAudio || root.ptype === "video"
      anchors.left: parent.left
      anchors.right: parent.right
      anchors.bottom: parent.bottom
      anchors.margins: 8
      start: root.preview && root.preview.start ? root.preview.start : 0
      end: root.preview && root.preview.end ? root.preview.end : 0
      duration: root.preview && root.preview.duration ? root.preview.duration : 1
    }
  }

  PlaceBadge {
    anchors.right: card.right
    anchors.bottom: card.bottom
    anchors.margins: 8
    visible: card.visible && root.row && root.row.place
    text: (root.row && root.row.place) ? root.row.place : ""
    onImage: root.showPicture
  }

  EmptyState {
    visible: !root.ptype || root.ptype.length === 0 || (root.showPicture && pic.status === Image.Error && !root.picturePath())
    anchors.centerIn: parent
    width: parent.width - 24
    title: (root.row && root.row.name) ? root.row.name : ""
    detail: (root.row && root.row.reason) ? root.row.reason : ""
  }

  function scrollCodeToFocus() {
    if (!root.showCode || !codeView.count)
      return
    var want = root.focusLine
    if (want <= 0) {
      codeView.positionViewAtBeginning()
      return
    }
    for (var i = 0; i < codeView.count; i++) {
      var it = root.codeLines[i]
      var n = it && it.n ? Number(it.n) : i + 1
      if (n === want) {
        codeView.positionViewAtIndex(Math.max(0, i - 2), ListView.Beginning)
        return
      }
    }
  }

  onPtypeChanged: Qt.callLater(scrollCodeToFocus)
  onFocusLineChanged: Qt.callLater(scrollCodeToFocus)
}
