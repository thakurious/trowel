import QtQuick

// Components.swift MomentBar: track primary@0.12, span ≥4 px, 9 px playhead.
Item {
  id: root
  property real start: 0
  property real end: 0
  property real duration: 1
  property real playhead: -1
  property bool onImage: false
  implicitHeight: 9
  implicitWidth: 120

  readonly property real from: duration > 0 ? Math.max(0, Math.min(1, start / duration)) : 0
  readonly property real to: duration > 0 ? Math.max(from, Math.min(1, end / duration)) : from

  Rectangle {
    anchors.fill: parent
    radius: height / 2
    color: root.onImage ? Qt.rgba(1, 1, 1, 0.3) : Theme.alpha(Theme.foreground, 0.12)
  }

  Rectangle {
    x: parent.width * root.from
    width: Math.max(4, parent.width * (root.to - root.from))
    height: parent.height
    radius: height / 2
    color: root.onImage ? "white" : Theme.accent
  }

  Rectangle {
    visible: root.playhead >= 0 && root.duration > 0
    x: parent.width * Math.max(0, Math.min(1, root.playhead / root.duration)) - 1
    width: 2
    height: parent.height
    color: root.onImage ? "white" : Theme.foreground
  }
}
