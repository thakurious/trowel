import QtQuick
import QtQuick.Controls

// SearchPanelView header: 20 semibold (17 in code mode), 24 px icon column, count capsule.
Item {
  id: root
  property alias text: field.text
  property bool codeMode: false
  property string countText: ""
  property bool hybridPending: false
  property string placeholder: "Describe it: “payment error screenshot”, “zebra in a video”…"
  signal accepted
  signal textEdited
  signal keyPressed(var event)
  implicitHeight: 56
  implicitWidth: 780

  function forceActiveFocus() {
    field.forceActiveFocus()
  }

  // Like Spotlight: the panel opens on the last query, selected, so typing replaces it.
  // `upstream/Sources/DigUpApp/SearchPanel.swift:106`.
  function selectAll() {
    field.selectAll()
  }

  // Ctrl+C copies the selected text, not the row, when the field has a selection.
  function hasSelectedText() {
    return String(field.selectedText || "").length > 0
  }

  function composing() {
    try {
      return String(field.preeditText || "").length > 0
    } catch (e) {
      return false
    }
  }

  Row {
    anchors.fill: parent
    anchors.leftMargin: 18
    anchors.rightMargin: 18
    spacing: 11

    Item {
      width: 24
      height: parent.height
      Text {
        anchors.centerIn: parent
        text: root.codeMode ? "</>" : "⌕"
        color: Theme.brandAmber
        font.family: Theme.fontFamily
        font.pixelSize: root.codeMode ? Theme.queryCodeSize : Theme.querySize
        font.weight: Font.DemiBold
      }
    }

    TextField {
      id: field
      width: parent.width - 24 - 11 - (capsule.visible ? capsule.width + 8 : 0)
      anchors.verticalCenter: parent.verticalCenter
      placeholderText: root.placeholder
      color: Theme.foreground
      placeholderTextColor: Theme.secondary
      background: Item {}
      font.family: Theme.fontFamily
      font.pixelSize: root.codeMode ? Theme.queryCodeSize : Theme.querySize
      font.weight: Font.DemiBold
      onTextChanged: {
        if (!root.composing())
          root.textEdited()
      }
      onAccepted: root.accepted()
      Keys.onPressed: function (event) {
        root.keyPressed(event)
      }
    }

    Rectangle {
      id: capsule
      visible: root.countText.length > 0
      anchors.verticalCenter: parent.verticalCenter
      implicitWidth: capLabel.implicitWidth + 16
      implicitHeight: capLabel.implicitHeight + 6
      radius: height / 2
      color: Theme.keycapFill
      Text {
        id: capLabel
        anchors.centerIn: parent
        text: root.countText
        color: Theme.secondary
        font.family: Theme.fontFamily
        font.pixelSize: Theme.captionSize
        font.weight: Font.Medium
        font.features: {
          "tnum": 1
        }
      }
    }
  }

  Rectangle {
    visible: root.hybridPending
    anchors.left: parent.left
    anchors.right: parent.right
    anchors.bottom: parent.bottom
    height: 2
    color: Theme.alpha(Theme.accent, 0.55)

    SequentialAnimation on opacity {
      running: root.hybridPending
      loops: Animation.Infinite
      NumberAnimation {
        from: 0.25
        to: 1
        duration: Motion.hairlineLoopMs / 2
      }
      NumberAnimation {
        from: 1
        to: 0.25
        duration: Motion.hairlineLoopMs / 2
      }
    }
  }
}
