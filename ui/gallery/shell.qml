import QtQuick
import Quickshell
import "../qml/Trowel"

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
          text: "Settings tabs: General · Folders · Code · Storage"
          color: Theme.secondary
          font.pixelSize: Theme.captionSize
        }
      }
    }
  }
}
