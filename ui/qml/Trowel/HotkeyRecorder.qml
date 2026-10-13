import QtQuick

// Components.swift hotkey recorder: 13 pt, padding 12, radius 9, accent stroke while recording.
Rectangle {
  id: root
  property string chord: "SUPER + D"
  property bool recording: false
  property string conflict: ""
  signal captureRequested
  signal captured(string chord)
  implicitHeight: 36
  implicitWidth: 280
  radius: 9
  color: Theme.keycapFill
  border.width: recording ? 2 : Theme.hairlineWidth()
  border.color: recording ? Theme.accent : Theme.hairline
  focus: recording

  Text {
    anchors.centerIn: parent
    text: root.recording ? "Press the new shortcut…" : (root.chord.length ? root.chord : "Click to set a shortcut")
    color: Theme.foreground
    font.family: Theme.fontFamily
    font.pixelSize: Theme.rowTitleSize
  }

  MouseArea {
    anchors.fill: parent
    onClicked: {
      root.recording = !root.recording
      if (root.recording)
        root.forceActiveFocus()
      root.captureRequested()
    }
  }

  Keys.onPressed: function (event) {
    if (!root.recording)
      return
    if (event.key === Qt.Key_Escape) {
      root.recording = false
      event.accepted = true
      return
    }
    if (event.key === Qt.Key_Control || event.key === Qt.Key_Shift || event.key === Qt.Key_Alt || event.key === Qt.Key_Meta || event.key === Qt.Key_Super_L || event.key === Qt.Key_Super_R)
      return
    var parts = []
    if (event.modifiers & Qt.MetaModifier)
      parts.push("SUPER")
    if (event.modifiers & Qt.ControlModifier)
      parts.push("CTRL")
    if (event.modifiers & Qt.AltModifier)
      parts.push("ALT")
    if (event.modifiers & Qt.ShiftModifier)
      parts.push("SHIFT")
    var name = keyName(event.key)
    if (!name.length)
      return
    if (parts.length === 0)
      parts.push("SUPER")
    parts.push(name)
    root.chord = parts.join(" + ")
    root.recording = false
    root.captured(root.chord)
    event.accepted = true
  }

  function keyName(key) {
    if (key >= Qt.Key_A && key <= Qt.Key_Z)
      return String.fromCharCode(key)
    if (key >= Qt.Key_0 && key <= Qt.Key_9)
      return String.fromCharCode(key)
    if (key === Qt.Key_Space)
      return "SPACE"
    if (key === Qt.Key_Tab)
      return "TAB"
    if (key === Qt.Key_Return || key === Qt.Key_Enter)
      return "RETURN"
    return ""
  }
}
