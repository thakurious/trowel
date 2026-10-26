import QtQuick
import QtQuick.Controls
import "DeskModel.js" as DeskModel

// Process Monitor's right pane: the process or app, its command line and 60 s sparklines of CPU,
// GPU, memory and disk (from `proc_detail`, refreshed each sample), the GPU split by engine, and
// the confirm field for a protected process (P-D30: type its name).
Item {
  id: root
  property var row: null
  property var detail: ({})
  property string armed: ""
  property string notice: ""
  readonly property bool needsName: root.row !== null && root.row.protected === true && root.row.kill_refused !== true
  readonly property alias typed: confirmField.text
  readonly property alias confirmFocus: confirmField.activeFocus
  signal confirmAccepted
  signal escaped

  function focusConfirm() {
    confirmField.text = ""
    confirmField.forceActiveFocus()
  }

  function series(key) {
    var h = root.detail && (root.row && root.row.pids && root.row.pids.length > 1 ? root.detail.app_history : root.detail.history) || []
    return h.map(function (p) {
      return key === "disk" ? p.disk : (key === "ram" ? p.ram : p[key])
    })
  }

  Flickable {
    anchors.fill: parent
    anchors.margins: 18
    contentHeight: col.implicitHeight
    clip: true
    visible: root.row !== null

    Column {
      id: col
      width: parent.width
      spacing: 12

      Row {
        spacing: 10
        width: parent.width
        KindIcon {
          kind: root.row && (root.row.protected || root.row.kill_refused) ? "lock" : "process"
          side: 26
          color: Theme.accent
          anchors.verticalCenter: parent.verticalCenter
        }
        Column {
          width: parent.width - 36
          Text {
            width: parent.width
            text: root.row ? root.row.name : ""
            color: Theme.foreground
            font.family: Theme.fontFamily
            font.pixelSize: Theme.rowTitleSize + 3
            font.weight: Font.DemiBold
            elide: Text.ElideRight
          }
          Text {
            width: parent.width
            text: DeskModel.procSubtitle(root.row) + (root.row && root.row.app_id ? " · " + root.row.app_id : "")
            color: Theme.secondary
            font.family: Theme.fontFamily
            font.pixelSize: Theme.captionSize
            elide: Text.ElideMiddle
          }
        }
      }

      Text {
        width: parent.width
        visible: text.length > 0
        text: root.detail && root.detail.cmdline ? root.detail.cmdline : ""
        color: Theme.secondary
        font.family: Theme.monoFamily
        font.pixelSize: Math.round(Theme.fontBase * 0.85)
        wrapMode: Text.WrapAnywhere
        maximumLineCount: 3
        elide: Text.ElideRight
      }

      Grid {
        columns: 2
        columnSpacing: 16
        rowSpacing: 10
        width: parent.width
        Sparkline {
          width: (col.width - 16) / 2
          label: "CPU"
          caption: DeskModel.pct(root.row ? root.row.cpu : null)
          values: root.series("cpu")
          max: 100
        }
        Sparkline {
          width: (col.width - 16) / 2
          label: "GPU"
          caption: DeskModel.pct(root.row ? root.row.gpu : null)
          values: root.series("gpu")
          max: 100
        }
        Sparkline {
          width: (col.width - 16) / 2
          label: "Memory"
          caption: DeskModel.bytes(root.row ? root.row.ram : null)
          values: root.series("ram")
        }
        Sparkline {
          width: (col.width - 16) / 2
          label: "Disk"
          caption: root.row ? DeskModel.rate(root.row.disk_read, root.row.disk_write) : "—"
          values: root.series("disk")
        }
      }

      Text {
        width: parent.width
        visible: text.length > 0
        text: {
          var e = (root.detail && root.detail.engines) || []
          var parts = e.filter(function (x) {
            return x.busy > 0.05
          }).map(function (x) {
            var names = {
              "rcs": "render",
              "ccs": "compute",
              "vcs": "video",
              "vecs": "video enhance",
              "bcs": "copy"
            }
            return (names[x.engine] || x.engine) + " " + DeskModel.pct(x.busy)
          })
          var vram = root.row && root.row.vram ? "graphics memory " + DeskModel.bytes(root.row.vram) : ""
          return parts.concat(vram ? [vram] : []).join(" · ")
        }
        color: Theme.secondary
        font.family: Theme.fontFamily
        font.pixelSize: Theme.captionSize
        wrapMode: Text.WordWrap
      }

      Text {
        width: parent.width
        visible: !!(root.detail && (root.detail.threads || root.detail.pss))
        text: {
          var d = root.detail || {}
          var out = []
          if (d.pss)
            out.push("PSS " + DeskModel.bytes(d.pss))
          if (d.threads)
            out.push(d.threads + " threads")
          if (d.started)
            out.push("started " + Qt.formatDateTime(new Date(d.started * 1000), "MMM d, hh:mm"))
          return out.join(" · ")
        }
        color: Theme.secondary
        font.family: Theme.fontFamily
        font.pixelSize: Theme.captionSize
      }

      Rectangle {
        width: parent.width
        height: notice.implicitHeight + 16
        radius: 8
        visible: root.notice.length > 0 || !!(root.row && root.row.kill_refused)
        color: Theme.alpha(Theme.urgent, 0.14)
        Text {
          id: notice
          anchors.centerIn: parent
          width: parent.width - 20
          wrapMode: Text.WordWrap
          text: root.notice.length ? root.notice : "Trowel won't end this one: it's part of your session or another user's."
          color: Theme.foreground
          font.family: Theme.fontFamily
          font.pixelSize: Theme.captionSize
        }
      }

      Column {
        width: parent.width
        spacing: 6
        visible: root.needsName
        Text {
          width: parent.width
          wrapMode: Text.WordWrap
          text: "Your session needs " + (root.row ? root.row.name : "") + ". To end it anyway, type its name and press ↵."
          color: Theme.secondary
          font.family: Theme.fontFamily
          font.pixelSize: Theme.captionSize
        }
        TextField {
          id: confirmField
          width: parent.width
          placeholderText: root.row ? root.row.name : ""
          font.family: Theme.monoFamily
          color: Theme.foreground
          background: Rectangle {
            radius: 6
            color: Theme.keycapFill
            border.width: confirmField.activeFocus ? 2 : 1
            border.color: confirmField.activeFocus ? Theme.accent : Theme.hairline
          }
          Keys.onReturnPressed: root.confirmAccepted()
          Keys.onEnterPressed: root.confirmAccepted()
          Keys.onEscapePressed: root.escaped()
        }
      }
    }
  }

  EmptyState {
    visible: root.row === null
    anchors.centerIn: parent
    width: parent.width - 48
    title: "Nothing selected"
    detail: "Tab changes the sort: CPU, graphics, memory, disk."
  }
}
