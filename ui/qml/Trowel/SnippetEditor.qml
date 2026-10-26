import QtQuick
import QtQuick.Controls

// Create Snippet / Edit Snippet: name, keyword and the text (several lines), keyboard only.
// Tab moves between the fields, ctrl ↵ (or ctrl S) saves, esc goes back. The daemon checks the
// fields and says what's wrong (`error`).
Item {
  id: root
  property int snippetId: 0
  property string error: ""
  readonly property bool busy: nameField.activeFocus || keywordField.activeFocus || bodyArea.activeFocus
  signal save(var snippet)
  signal cancel

  function load(s) {
    root.snippetId = s && s.id ? s.id : 0
    nameField.text = s && s.name ? s.name : ""
    keywordField.text = s && s.keyword ? s.keyword : ""
    bodyArea.text = s && s.body ? s.body : ""
    root.error = ""
    nameField.forceActiveFocus()
  }

  function submit() {
    root.save({
      "id": root.snippetId,
      "name": nameField.text,
      "keyword": keywordField.text,
      "body": bodyArea.text
    })
  }

  function keys(event) {
    var ctrl = event.modifiers & Qt.ControlModifier
    if (ctrl && (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_S)) {
      root.submit()
      event.accepted = true
    } else if (event.key === Qt.Key_Escape) {
      root.cancel()
      event.accepted = true
    }
  }

  Column {
    anchors.fill: parent
    anchors.margins: 18
    spacing: 10

    Text {
      text: root.snippetId ? "Edit Snippet" : "Create Snippet"
      color: Theme.foreground
      font.family: Theme.fontFamily
      font.pixelSize: Theme.rowTitleSize + 3
      font.weight: Font.DemiBold
    }

    Row {
      width: parent.width
      spacing: 12
      Column {
        width: (parent.width - 12) * 0.62
        spacing: 3
        Text {
          text: "Name"
          color: Theme.secondary
          font.pixelSize: Theme.captionSize
        }
        TextField {
          id: nameField
          width: parent.width
          placeholderText: "Email signature"
          color: Theme.foreground
          font.family: Theme.fontFamily
          background: Rectangle {
            radius: 6
            color: Theme.keycapFill
            border.width: nameField.activeFocus ? 2 : 1
            border.color: nameField.activeFocus ? Theme.accent : Theme.hairline
          }
          KeyNavigation.tab: keywordField
          Keys.onPressed: function (event) {
            root.keys(event)
          }
        }
      }
      Column {
        width: (parent.width - 12) * 0.38
        spacing: 3
        Text {
          text: "Keyword (optional)"
          color: Theme.secondary
          font.pixelSize: Theme.captionSize
        }
        TextField {
          id: keywordField
          width: parent.width
          placeholderText: ";sig"
          color: Theme.foreground
          font.family: Theme.monoFamily
          background: Rectangle {
            radius: 6
            color: Theme.keycapFill
            border.width: keywordField.activeFocus ? 2 : 1
            border.color: keywordField.activeFocus ? Theme.accent : Theme.hairline
          }
          KeyNavigation.tab: bodyArea
          KeyNavigation.backtab: nameField
          Keys.onPressed: function (event) {
            root.keys(event)
          }
        }
      }
    }

    Text {
      text: "Text"
      color: Theme.secondary
      font.pixelSize: Theme.captionSize
    }
    ScrollView {
      width: parent.width
      height: parent.height - y - hint.height - 20
      TextArea {
        id: bodyArea
        wrapMode: TextEdit.Wrap
        placeholderText: "Best regards"
        color: Theme.foreground
        font.family: Theme.monoFamily
        background: Rectangle {
          radius: 6
          color: Theme.keycapFill
          border.width: bodyArea.activeFocus ? 2 : 1
          border.color: bodyArea.activeFocus ? Theme.accent : Theme.hairline
        }
        KeyNavigation.backtab: keywordField
        KeyNavigation.tab: nameField
        Keys.onPressed: function (event) {
          root.keys(event)
        }
      }
    }

    Text {
      id: hint
      width: parent.width
      wrapMode: Text.WordWrap
      text: root.error.length ? root.error : "{clipboard}, {date}, {time}, {date:%d %b} and {argument:name} are filled in when it pastes. ctrl ↵ saves."
      color: root.error.length ? Theme.urgent : Theme.secondary
      font.family: Theme.fontFamily
      font.pixelSize: Theme.captionSize
    }
  }
}
