import QtQuick
import Quickshell
import "Trowel"

ShellRoot {
  FloatingWindow {
    title: "Trowel gallery"
    implicitWidth: 1040
    implicitHeight: 700
    color: Theme.background

    Flickable {
      anchors.fill: parent
      contentHeight: col.implicitHeight + 40
      clip: true

      Column {
        id: col
        x: 20
        y: 20
        width: parent.width - 40
        spacing: 18

        Text {
          text: "Trowel components"
          color: Theme.foreground
          font.pixelSize: Theme.querySize
          font.weight: Font.DemiBold
        }

        QueryField {
          width: parent.width
          countText: "12"
        }
        QueryField {
          width: parent.width
          codeMode: true
          hybridPending: true
          placeholder: "Search your code"
        }

        Banner {
          width: parent.width
          text: "Trowel isn't running"
          action: "Start"
        }

        EmptyState {
          width: parent.width
          title: "No good matches"
          detail: "Try what you'd see or hear in it, or words you remember from it."
        }

        GroupHeader {
          title: "Screenshots"
        }
        ResultRow {
          width: 330
          selected: true
          row: {
            "name": "harbour.png",
            "kind": "screenshot",
            "place": "p. 3",
            "folder": "~/Pictures",
            "excerpt": "the «harbour» dock",
            "matched_words": true
          }
        }
        ResultRow {
          width: 330
          row: {
            "name": "notes.txt",
            "kind": "doc",
            "folder": "~/Documents",
            "reason": "Similar meaning"
          }
        }
        WeakerFold {
          width: 330
          count: 14
        }
        WeakerFold {
          width: 330
          count: 14
          expanded: true
          selected: true
        }

        Row {
          spacing: 12
          Keycap {
            label: "↵"
          }
          Keycap {
            label: "esc"
          }
          PlaceBadge {
            text: "p. 3"
          }
          PlaceBadge {
            text: "0:06"
          }
          PlaceBadge {
            text: "L12–40"
          }
        }

        MomentBar {
          width: parent.width
          start: 6
          end: 12
          duration: 40
          playhead: 8
        }

        Row {
          spacing: 16
          ProgressRing {
            progress: -1
          }
          ProgressRing {
            progress: 0.4
          }
          ProgressRing {
            paused: true
          }
          KindIcon {
            kind: "pdf"
            side: 24
          }
          KindIcon {
            kind: "code"
            side: 24
          }
          KindIcon {
            kind: "audio"
            side: 24
          }
        }

        HotkeyRecorder {
          chord: "SUPER + D"
        }
        HotkeyRecorder {
          recording: true
        }

        FolderRow {
          width: parent.width
          path: "~/Documents"
          estimate: "128 files · ~2 min"
          checked: true
        }
        FolderRow {
          width: parent.width
          path: "~/Downloads"
          skip: "No files of a searchable type"
          checked: false
        }

        PreviewPane {
          width: parent.width
          height: 160
          row: {
            "name": "notes.txt",
            "reason": "Similar meaning"
          }
          preview: {
            "type": "text",
            "text": "the harbour dock at dusk"
          }
        }

        FilterBar {
          width: parent.width
          counts: {
            "screenshot": 3,
            "image": 12
          }
        }
        Text {
          text: "Onboarding steps: welcome · folders · code · shortcut · ready"
          color: Theme.secondary
          font.pixelSize: Theme.captionSize
        }
        Text {
          text: "Settings tabs: General · Folders · Code · Launcher & Clipboard · Storage"
          color: Theme.secondary
          font.pixelSize: Theme.captionSize
        }

        // Phase 8 L4: launcher and clipboard components.
        Text {
          text: "Launcher & clipboard"
          color: Theme.foreground
          font.pixelSize: Theme.emptyTitleSize
          font.weight: Font.DemiBold
          topPadding: 12
        }
        CalcCard {
          width: parent.width
          result: {
            "expr": "1920*1.25 + 15%",
            "pretty_expr": "1920 × 1.25 + 15%",
            "display": "2,760",
            "copy": "2760",
            "kind": "number"
          }
        }
        CalcCard {
          width: parent.width
          selected: false
          result: {
            "expr": "pi",
            "pretty_expr": "π",
            "display": "3.1415926536",
            "copy": "3.1415926536",
            "kind": "number",
            "approx": true
          }
        }
        GroupHeader {
          title: "Applications"
        }
        AppRow {
          width: 330
          selected: true
          row: {
            "name": "Firefox",
            "generic": "Web Browser",
            "iconName": "firefox",
            "actionCount": 2
          }
        }
        AppRow {
          width: 330
          row: {
            "name": "Omacalc",
            "generic": "Calculator",
            "iconName": "accessories-calculator",
            "actionCount": 0
          }
        }
        GroupHeader {
          title: "Pinned"
        }
        Repeater {
          model: [{
              "kind": "text",
              "title": "firewall rules for the staging cluster",
              "sourceApp": "Slack",
              "lastUsed": Date.now() / 1000 - 130,
              "pinned": true
            }, {
              "kind": "link",
              "title": "https://github.com/thakurious/trowel/pull/7",
              "domain": "github.com",
              "sourceApp": "google-chrome",
              "lastUsed": Date.now() / 1000 - 4000
            }, {
              "kind": "color",
              "title": "#F2A22A",
              "color": "#F2A22A",
              "sourceApp": "gimp",
              "lastUsed": Date.now() / 1000 - 90000
            }, {
              "kind": "code",
              "title": "fn main() { println!(\"hello\"); }",
              "sourceApp": "com.mitchellh.ghostty",
              "lastUsed": Date.now() / 1000 - 30
            }, {
              "kind": "files",
              "title": "invoice-2026-09.pdf, receipt.png",
              "files": 2,
              "sourceApp": "io.github.lgse.Strata",
              "lastUsed": Date.now() / 1000 - 600
            }, {
              "kind": "html",
              "title": "Refund policy: items may be returned within 30 days",
              "sourceApp": "google-chrome",
              "lastUsed": Date.now() / 1000 - 7200
            }]
          delegate: ClipRow {
            required property var modelData
            required property int index
            width: 330
            selected: index === 0
            row: modelData
          }
        }
        KindChips {
          current: "text"
        }
        Row {
          spacing: 8
          Chip {
            label: "Open in browser"
          }
          Chip {
            label: "#F2A22A"
            mono: true
          }
          Toast {
            Component.onCompleted: {
              holdMs = 1e9
              show("Copied 152.4")
            }
          }
        }
        Row {
          spacing: 12
          ClipPreview {
            width: 450
            height: 260
            row: {
              "clipId": 1,
              "kind": "color",
              "color": "#F2A22A",
              "raw": {
                "source_app": "gimp",
                "bytes": 7
              }
            }
            content: {
              "row": {
                "id": 1
              },
              "text": "#F2A22A"
            }
          }
          ActionPanel {
            Component.onCompleted: {
              actions = [{
                  "id": "paste",
                  "label": "Paste",
                  "keys": "↵"
                }, {
                  "id": "copy",
                  "label": "Copy",
                  "keys": "ctrl ↵"
                }, {
                  "id": "pin",
                  "label": "Pin",
                  "keys": "ctrl shift P"
                }]
              visible = true
            }
          }
        }
        AppPreview {
          width: 450
          height: 300
          row: {
            "name": "Firefox",
            "generic": "Web Browser",
            "comment": "Browse the World Wide Web",
            "iconName": "firefox",
            "launches": 14,
            "raw": {
              "source": "/usr/share/applications/firefox.desktop",
              "last_launched": Date.now() / 1000 - 3 * 86400,
              "actions": [{
                  "id": "new-window",
                  "name": "New Window"
                }]
            }
          }
        }
      }
    }
  }
}
