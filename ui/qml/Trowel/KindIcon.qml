import QtQuick
import QtQuick.Shapes

// Kind glyphs (24×24 line icons, stroke 1.6) drawn as vector paths in the theme colour.
// Tinting the black-stroked SVGs with MultiEffect colorization multiplied the tint by the
// source luminance, so black stayed black: the icons were near-invisible on the dark panel.
Item {
  id: root
  property string kind: "doc"
  property int side: 24
  property color color: Theme.foreground
  width: side
  height: side

  readonly property var paths: ({
    "audio": "M10 16V6l9-2v10 M5.8 16.5a2.2 2.2 0 1 0 4.4 0a2.2 2.2 0 1 0 -4.4 0z M14.8 14.5a2.2 2.2 0 1 0 4.4 0a2.2 2.2 0 1 0 -4.4 0z",
    "code": "M9 7L4.5 12 9 17 M15 7l4.5 5L15 17 M13 5.5l-2 13",
    "doc": "M7 3.5h7l4 4V20.5H7z M14 3.5V8h4 M9 12h6M9 15.5h6",
    "folder": "M3.5 7.5h6l2 2.5H20.5v9.5H3.5z M3.5 7.5V6h5l1.5 1.5",
    "image": "M6 5h12a2 2 0 0 1 2 2v10a2 2 0 0 1 -2 2h-12a2 2 0 0 1 -2 -2v-10a2 2 0 0 1 2 -2z M7.6 10a1.4 1.4 0 1 0 2.8 0a1.4 1.4 0 1 0 -2.8 0z M4 16l4.5-4 4 3.5 3-2.5 4.5 4",
    "pdf": "M7 3.5h7l4 4V20.5H7z M14 3.5V8h4 M9 13h2.2a1.6 1.6 0 0 1 0 3.2H9V11",
    "screenshot": "M5 5.5h14a1.5 1.5 0 0 1 1.5 1.5v9a1.5 1.5 0 0 1 -1.5 1.5h-14a1.5 1.5 0 0 1 -1.5 -1.5v-9a1.5 1.5 0 0 1 1.5 -1.5z M8 20h8 M9.8 11.5a2.2 2.2 0 1 0 4.4 0a2.2 2.2 0 1 0 -4.4 0z",
    "video": "M5 6h9a1.5 1.5 0 0 1 1.5 1.5v9a1.5 1.5 0 0 1 -1.5 1.5h-9a1.5 1.5 0 0 1 -1.5 -1.5v-9a1.5 1.5 0 0 1 1.5 -1.5z M15.5 10l5-2.5v9L15.5 14z",
    "link": "M10.5 13.5a3.5 3.5 0 0 0 5 0l3-3a3.5 3.5 0 0 0 -5 -5l-1 1 M13.5 10.5a3.5 3.5 0 0 0 -5 0l-3 3a3.5 3.5 0 0 0 5 5l1 -1",
    "text": "M5 6.5h14 M5 10.5h14 M5 14.5h14 M5 18.5h9",
    "html": "M5 6.5h14 M5 10.5h9 M5 14.5h14 M5 18.5h9 M17 9l2.5 1.5L17 12",
    "files": "M8 3.5h6l4 4V17.5H8z M14 3.5V8h4 M5.5 7v13.5h10",
    "clipboard": "M8 5h-1.5a1.5 1.5 0 0 0 -1.5 1.5v13a1.5 1.5 0 0 0 1.5 1.5h11a1.5 1.5 0 0 0 1.5 -1.5v-13a1.5 1.5 0 0 0 -1.5 -1.5H16 M9 3.5h6v3h-6z",
    "app": "M5 5h5.5v5.5H5z M13.5 5H19v5.5h-5.5z M5 13.5h5.5V19H5z M13.5 13.5H19V19h-5.5z",
    "pin": "M9 4h6l-1 5 3 3H7l3-3z M12 12v8"
    })

  function iconName() {
    switch (kind) {
    case "screenshot":
      return "screenshot"
    case "image":
      return "image"
    case "pdf":
      return "pdf"
    case "audio":
      return "audio"
    case "video":
      return "video"
    case "code":
      return "code"
    case "folder":
      return "folder"
    case "link":
    case "text":
    case "html":
    case "files":
    case "clipboard":
    case "app":
    case "pin":
      return kind
    default:
      return "doc"
    }
  }

  Shape {
    width: 24
    height: 24
    scale: root.side / 24
    transformOrigin: Item.TopLeft
    preferredRendererType: Shape.CurveRenderer
    antialiasing: true

    ShapePath {
      strokeColor: root.color
      strokeWidth: 1.6
      fillColor: "transparent"
      capStyle: ShapePath.FlatCap
      joinStyle: ShapePath.MiterJoin
      PathSvg {
        path: root.paths[root.iconName()] || root.paths["doc"]
      }
    }
  }
}
