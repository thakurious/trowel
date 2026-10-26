pragma ComponentBehavior: Bound
import QtQuick
import "DeskModel.js" as DeskModel

// The Island's drawing (P-D29, PHASE9-PLAN.md §1.1): a near-black pill that grows out of the
// bar's bottom edge. Pure Item: Island.qml puts it on a layer surface, the harness in a window.
//   compact  — icon · title · trailing value (a countdown counts itself from `ends_at`)
//   split    — compact + a round bubble for the second activity (+N for more)
//   expanded — hover 300 ms, a click, or an alert: subtitle, progress bar, up to 3 buttons, the
//              other activities as lines
//   alert    — expanded with one bounce, back to compact after 4 s
// Every size change is one spring (UI-DESIGN island section); reduce motion makes it instant.
Item {
  id: root
  property var activities: []
  property bool reduceMotion: Motion.reduceMotion
  // Seconds (Unix) the countdowns read; ticks once a second only while something counts.
  property real now: Date.now() / 1000
  property bool expandedByUser: false
  property bool hoverExpanded: false
  property string alertId: ""
  property var alerted: ({})
  signal action(string activityId, string actionId)

  readonly property var primary: activities.length ? activities[0] : null
  readonly property var second: activities.length > 1 ? activities[1] : null
  readonly property int more: Math.max(0, activities.length - 2)
  readonly property bool expanded: root.primary !== null && (root.expandedByUser || root.hoverExpanded || root.alertId.length > 0)
  readonly property string mode: !root.primary ? "hidden" : (root.expanded ? "expanded" : (root.second ? "split" : "compact"))
  readonly property bool counting: {
    for (var i = 0; i < root.activities.length; i++)
      if (root.activities[i].ends_at)
        return true
    return false
  }
  // The island is still drawn while it shrinks away.
  readonly property bool mapped: root.primary !== null || shrinkTimer.running
  readonly property alias pillItem: pill
  readonly property alias bubbleItem: bubble

  readonly property color fill: Qt.rgba(0.035, 0.035, 0.045, 0.97)
  readonly property color ink: "#f4f4f5"
  readonly property color dim: "#a1a1aa"

  readonly property int compactHeight: 30
  readonly property int expandedWidth: 380
  // The expanded card is as tall as its content (title, bar, buttons, other lines) + margins.
  readonly property int expandedHeight: Math.max(60, Math.round(expandedCol.implicitHeight) + 26)
  readonly property int compactWidth: Math.max(168, Math.min(340, compactRow.implicitWidth + 28))

  implicitWidth: root.expandedWidth + 40
  implicitHeight: 240

  function valueOf(a) {
    if (!a)
      return ""
    if (a.ends_at)
      return DeskModel.clock(a.ends_at - root.now)
    return a.value || ""
  }

  // 0…1 left of a countdown, else the activity's own progress; -1 for none.
  function fractionOf(a) {
    if (!a)
      return -1
    if (a.ends_at && a.total > 0)
      return Math.max(0, Math.min(1, (a.ends_at - root.now) / a.total))
    if (a.progress !== undefined && a.progress !== null)
      return Math.max(0, Math.min(1, a.progress))
    return -1
  }

  // The pill and the bubble report hover here; moving from one to the other doesn't collapse.
  function hoverChanged(on) {
    if (on) {
      leaveTimer.stop()
      hoverTimer.restart()
    } else {
      hoverTimer.stop()
      leaveTimer.restart()
    }
  }

  function iconKind(a) {
    return a && a.icon && a.icon.length > 1 ? a.icon : "bell"
  }

  onActivitiesChanged: {
    root.now = Date.now() / 1000
    if (!root.primary) {
      root.expandedByUser = false
      root.hoverExpanded = false
      root.alertId = ""
      shrinkTimer.restart()
      return
    }
    // A new alert (a timer finished) expands the island once and bounces.
    for (var i = 0; i < root.activities.length; i++) {
      var a = root.activities[i]
      var key = a.id + "@" + (a.ends_at || "") + (a.subtitle || "")
      if (a.alert && !root.alerted[key]) {
        var seen = root.alerted
        seen[key] = true
        root.alerted = seen
        root.alertId = a.id
        alertTimer.restart()
        bounce.restart()
        break
      }
    }
    if (root.alertId.length && !root.activities.some(function (x) {
        return x.id === root.alertId
      }))
      root.alertId = ""
  }

  Timer {
    id: tick
    interval: 1000
    repeat: true
    running: root.counting && root.primary !== null
    onTriggered: root.now = Date.now() / 1000
  }

  Timer {
    id: alertTimer
    interval: 4000
    onTriggered: root.alertId = ""
  }

  Timer {
    id: hoverTimer
    interval: 300
    onTriggered: root.hoverExpanded = true
  }

  Timer {
    id: leaveTimer
    interval: 200
    onTriggered: {
      root.hoverExpanded = false
      root.expandedByUser = false
    }
  }

  // Unmap only after the shrink has played.
  Timer {
    id: shrinkTimer
    interval: root.reduceMotion ? 0 : 320
  }

  Rectangle {
    id: pill
    readonly property real targetWidth: root.mode === "hidden" ? 96 : (root.expanded ? root.expandedWidth : root.compactWidth)
    readonly property real targetHeight: root.mode === "hidden" ? 0 : (root.expanded ? root.expandedHeight : root.compactHeight)
    anchors.top: parent.top
    x: (root.width - width - (bubble.shown ? bubble.width + 6 : 0)) / 2
    width: targetWidth
    height: targetHeight
    radius: Math.min(height / 2, root.expanded ? 22 : 15)
    color: root.fill
    clip: true
    opacity: root.mode === "hidden" ? 0 : 1
    transformOrigin: Item.Top

    Behavior on width {
      enabled: !root.reduceMotion
      SpringAnimation {
        spring: 4
        damping: 0.32
        epsilon: 0.25
      }
    }
    Behavior on height {
      enabled: !root.reduceMotion
      SpringAnimation {
        spring: 4
        damping: 0.32
        epsilon: 0.25
      }
    }
    Behavior on opacity {
      enabled: !root.reduceMotion
      NumberAnimation {
        duration: 160
      }
    }

    SequentialAnimation {
      id: bounce
      NumberAnimation {
        target: pill
        property: "scale"
        to: root.reduceMotion ? 1 : 1.06
        duration: root.reduceMotion ? 0 : 140
        easing.type: Easing.OutQuad
      }
      SpringAnimation {
        target: pill
        property: "scale"
        to: 1
        spring: 5
        damping: 0.25
      }
    }

    // A HoverHandler stays hovered while the pointer is over the buttons inside; a MouseArea
    // here lost its hover to the buttons' own MouseAreas and the island collapsed under them.
    HoverHandler {
      onHoveredChanged: root.hoverChanged(hovered)
    }

    MouseArea {
      anchors.fill: parent
      onClicked: {
        if (root.alertId.length) {
          root.alertId = ""
          root.expandedByUser = false
          root.hoverExpanded = false
        } else
          root.expandedByUser = !root.expanded
      }
    }

    // ---- compact: icon · title · value
    Row {
      id: compactRow
      visible: !root.expanded && root.primary !== null
      anchors.verticalCenter: parent.verticalCenter
      anchors.horizontalCenter: parent.horizontalCenter
      spacing: 8

      Item {
        width: 18
        height: 18
        anchors.verticalCenter: parent.verticalCenter
        Ring {
          anchors.fill: parent
          fraction: root.fractionOf(root.primary)
        }
        KindIcon {
          anchors.centerIn: parent
          kind: root.iconKind(root.primary)
          side: root.fractionOf(root.primary) >= 0 ? 11 : 16
          color: Theme.accent
        }
      }
      Text {
        anchors.verticalCenter: parent.verticalCenter
        text: root.primary ? root.primary.title : ""
        color: root.ink
        font.family: Theme.fontFamily
        font.pixelSize: 12
        font.weight: Font.Medium
        elide: Text.ElideRight
        width: Math.min(implicitWidth, 190)
      }
      Text {
        anchors.verticalCenter: parent.verticalCenter
        visible: text.length > 0
        text: root.valueOf(root.primary)
        color: Theme.accent
        font.family: Theme.monoFamily
        font.pixelSize: 12
        font.weight: Font.DemiBold
        font.features: {
          "tnum": 1
        }
      }
      Text {
        anchors.verticalCenter: parent.verticalCenter
        visible: root.more > 0 && !root.second
        text: "+" + root.more
        color: root.dim
        font.pixelSize: 11
      }
    }

    // ---- expanded: the first activity in full, the others as lines
    Column {
      id: expandedCol
      visible: root.expanded && root.primary !== null
      anchors.fill: parent
      anchors.margins: 14
      anchors.topMargin: 12
      spacing: 8

      Row {
        width: parent.width
        spacing: 10
        Item {
          width: 34
          height: 34
          Ring {
            anchors.fill: parent
            fraction: root.fractionOf(root.primary)
            thickness: 3
          }
          KindIcon {
            anchors.centerIn: parent
            kind: root.iconKind(root.primary)
            side: 18
            color: Theme.accent
          }
        }
        Column {
          width: parent.width - 44 - bigValue.width - 10
          anchors.verticalCenter: parent.verticalCenter
          spacing: 1
          Text {
            width: parent.width
            text: root.primary ? root.primary.title : ""
            color: root.ink
            font.family: Theme.fontFamily
            font.pixelSize: 14
            font.weight: Font.DemiBold
            elide: Text.ElideRight
          }
          Text {
            width: parent.width
            visible: text.length > 0
            text: (root.primary && root.primary.subtitle) ? root.primary.subtitle : ""
            color: root.dim
            font.family: Theme.fontFamily
            font.pixelSize: 11
            elide: Text.ElideRight
          }
        }
        Text {
          id: bigValue
          anchors.verticalCenter: parent.verticalCenter
          text: root.valueOf(root.primary)
          color: Theme.accent
          font.family: Theme.monoFamily
          font.pixelSize: 22
          font.weight: Font.DemiBold
          font.features: {
            "tnum": 1
          }
        }
      }

      Rectangle {
        width: parent.width
        height: 4
        radius: 2
        visible: root.fractionOf(root.primary) >= 0
        color: Qt.rgba(1, 1, 1, 0.12)
        Rectangle {
          height: parent.height
          radius: 2
          // No animation: it moves once a second, and on first show it must not grow from 0.
          width: parent.width * Math.max(0, root.fractionOf(root.primary))
          color: Theme.accent
        }
      }

      Row {
        spacing: 8
        visible: !!(root.primary && root.primary.actions && root.primary.actions.length > 0)
        Repeater {
          model: (root.primary && root.primary.actions) ? root.primary.actions : []
          delegate: Rectangle {
            id: chip
            required property var modelData
            width: label.implicitWidth + 24
            height: 26
            radius: 13
            color: btn.containsMouse ? (chip.modelData.danger ? Theme.urgent : Theme.accent) : Qt.rgba(1, 1, 1, 0.12)
            Text {
              id: label
              anchors.centerIn: parent
              text: chip.modelData.label
              color: btn.containsMouse ? root.fill : (chip.modelData.danger ? Qt.lighter(Theme.urgent, 1.4) : root.ink)
              font.family: Theme.fontFamily
              font.pixelSize: 12
              font.weight: Font.Medium
            }
            MouseArea {
              id: btn
              anchors.fill: parent
              hoverEnabled: true
              cursorShape: Qt.PointingHandCursor
              onClicked: root.action(root.primary.id, chip.modelData.id)
            }
          }
        }
      }

      Repeater {
        model: root.activities.slice(1, 3)
        delegate: Item {
          id: line
          required property var modelData
          width: expandedCol.width
          height: 22
          Rectangle {
            width: parent.width
            height: 1
            color: Qt.rgba(1, 1, 1, 0.08)
          }
          KindIcon {
            anchors.left: parent.left
            anchors.bottom: parent.bottom
            kind: root.iconKind(line.modelData)
            side: 14
            color: root.dim
          }
          Text {
            anchors.left: parent.left
            anchors.leftMargin: 22
            anchors.right: lineValue.left
            anchors.rightMargin: 8
            anchors.bottom: parent.bottom
            text: line.modelData.title + (line.modelData.subtitle ? " · " + line.modelData.subtitle : "")
            color: root.dim
            font.family: Theme.fontFamily
            font.pixelSize: 11
            elide: Text.ElideRight
          }
          Text {
            id: lineValue
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            text: root.valueOf(line.modelData)
            color: root.ink
            font.family: Theme.monoFamily
            font.pixelSize: 11
            font.features: {
              "tnum": 1
            }
          }
        }
      }
    }
  }

  // ---- split: the second activity's bubble
  Rectangle {
    id: bubble
    readonly property bool shown: root.mode === "split"
    anchors.top: parent.top
    x: pill.x + pill.width + 6
    width: shown ? root.compactHeight : 0
    height: root.compactHeight
    radius: height / 2
    color: root.fill
    opacity: shown ? 1 : 0
    visible: opacity > 0.01
    Behavior on width {
      enabled: !root.reduceMotion
      SpringAnimation {
        spring: 4
        damping: 0.32
        epsilon: 0.25
      }
    }
    Behavior on opacity {
      enabled: !root.reduceMotion
      NumberAnimation {
        duration: 140
      }
    }
    Ring {
      anchors.fill: parent
      anchors.margins: 4
      fraction: root.fractionOf(root.second)
    }
    KindIcon {
      anchors.centerIn: parent
      kind: root.iconKind(root.second)
      side: 12
      color: root.ink
    }
    Text {
      anchors.right: parent.right
      anchors.bottom: parent.bottom
      visible: root.more > 0
      text: "+" + root.more
      color: root.dim
      font.pixelSize: 8
    }
    MouseArea {
      anchors.fill: parent
      onClicked: root.expandedByUser = true
    }
    HoverHandler {
      onHoveredChanged: root.hoverChanged(hovered)
    }
  }

  // A thin ring for time left (or progress); nothing when -1.
  component Ring: Canvas {
    property real fraction: -1
    property real thickness: 2
    visible: fraction >= 0
    onFractionChanged: requestPaint()
    onVisibleChanged: requestPaint()
    onPaint: {
      var ctx = getContext("2d")
      ctx.reset()
      if (fraction < 0)
        return
      var r = Math.min(width, height) / 2 - thickness
      if (r <= 0)
        return
      ctx.lineWidth = thickness
      ctx.lineCap = "round"
      ctx.strokeStyle = Qt.rgba(1, 1, 1, 0.16)
      ctx.beginPath()
      ctx.arc(width / 2, height / 2, r, 0, Math.PI * 2)
      ctx.stroke()
      ctx.strokeStyle = Theme.accent
      ctx.beginPath()
      ctx.arc(width / 2, height / 2, r, -Math.PI / 2, -Math.PI / 2 + Math.PI * 2 * Math.max(0.02, fraction))
      ctx.stroke()
    }
  }
}
