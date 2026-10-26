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
    "pin": "M9 4h6l-1 5 3 3H7l3-3z M12 12v8",
    "settings": "M4 7h9 M17 7h3 M15 5v4 M4 17h3 M11 17h9 M9 15v4",
    "keyboard": "M3.5 7h17v10h-17z M7 10h1 M10.5 10h1 M14 10h1 M17 10h0.5 M7.5 14h9",
    // Phase 9: the Island, system commands, timers, processes, snippets.
    "lock": "M7 11h10v8.5H7z M9 11V8a3 3 0 0 1 6 0v3",
    "timer": "M5 13a7 7 0 1 0 14 0a7 7 0 1 0 -14 0z M12 13V9.5 M10 3.5h4 M17.5 6.5l1.5 -1.5",
    "process": "M3.5 12h4l2.5 -6 4 12 2.5 -6h4",
    "snippet": "M9 4.5c-2 0 -2.5 1 -2.5 2.5v2.5c0 1.2 -0.8 2.5 -2 2.5c1.2 0 2 1.3 2 2.5v2.5c0 1.5 0.5 2.5 2.5 2.5 M15 4.5c2 0 2.5 1 2.5 2.5v2.5c0 1.2 0.8 2.5 2 2.5c-1.2 0 -2 1.3 -2 2.5v2.5c0 1.5 -0.5 2.5 -2.5 2.5",
    "terminal": "M4 5h16v14H4z M7.5 9.5l3 2.5 -3 2.5 M12.5 15h4",
    "palette": "M12 4a8 8 0 1 0 0 16c1.2 0 1.8 -0.8 1.8 -1.7c0 -1.2 -1 -1.5 -1 -2.5c0 -0.9 0.7 -1.6 1.7 -1.6H17a3 3 0 0 0 3 -3C20 7 16.4 4 12 4z M7.6 11.5h0.8 M10.6 7.8h0.8 M14.8 8.4h0.8",
    "power": "M12 3.5v8 M7.5 6.5a7 7 0 1 0 9 0",
    "sleep": "M18.5 14.5A7 7 0 0 1 9.5 5.5a7 7 0 1 0 9 9z M15 4h4l-4 4h4",
    "moon": "M18.5 14.5A7 7 0 0 1 9.5 5.5a7 7 0 1 0 9 9z",
    "coffee": "M5 9h11v5a4 4 0 0 1 -4 4H9a4 4 0 0 1 -4 -4z M16 10.5h1.5a2 2 0 0 1 0 4H16 M8.5 3.5v3 M11.5 3.5v3",
    "bell": "M6.5 16.5V11a5.5 5.5 0 0 1 11 0v5.5l1.5 1.5H5z M10 20h4",
    "bar": "M4 5h16v14H4z M4 8.5h16",
    "touchpad": "M5 5h14v14H5z M5 15h14 M12 15v4",
    "wifi": "M3.5 9.5a12 12 0 0 1 17 0 M6.5 12.5a7.8 7.8 0 0 1 11 0 M9.5 15.5a3.6 3.6 0 0 1 5 0 M11.5 18.5h1",
    "bluetooth": "M7 7.5l10 9 -5 4v-17l5 4 -10 9",
    "sound": "M4 9.5h3.5L12 5.5v13l-4.5 -4H4z M15.5 9a4 4 0 0 1 0 6 M18 6.5a7.5 7.5 0 0 1 0 11",
    "refresh": "M19 12a7 7 0 1 1 -2 -4.9 M19 4.5v4h-4",
    "download": "M12 4v11 M7.5 10.5L12 15l4.5 -4.5 M5 19.5h14",
    "record": "M7 12a5 5 0 1 0 10 0a5 5 0 1 0 -10 0z",
    "progress": "M4.5 9.5h15v5h-15z M4.5 12h8"
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
    case "settings":
    case "keyboard":
      return kind
    default:
      return root.paths[kind] !== undefined ? kind : (kind === "music" ? "audio" : "doc")
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
