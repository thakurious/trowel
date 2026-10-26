pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import "DeskModel.js" as DeskModel

// Snippets' right pane: the text with its placeholders marked, and a field for each
// `{argument:…}` (↵ in the pane moves to the next one, then pastes). The daemon fills the
// placeholders when it pastes (P-D33).
Item {
  id: root
  property var snippet: null
  property var values: ({})
  readonly property var args: (root.snippet && root.snippet.arguments) ? root.snippet.arguments : []
  readonly property bool editing: fields.activeIndex >= 0
  signal submit(var args)
  signal escaped

  // Focus the first empty field; false when every argument has a value.
  function focusArguments() {
    for (var i = 0; i < root.args.length; i++)
      if (!String(root.values[root.args[i]] || "").length) {
        fields.focusAt(i)
        return true
      }
    return false
  }

  function complete() {
    for (var i = 0; i < root.args.length; i++)
      if (!String(root.values[root.args[i]] || "").length)
        return false
    return true
  }

  onSnippetChanged: root.values = ({})

  Flickable {
    anchors.fill: parent
    anchors.margins: 18
    contentHeight: col.implicitHeight
    clip: true
    visible: root.snippet !== null

    Column {
      id: col
      width: parent.width
      spacing: 12

      Row {
        spacing: 10
        width: parent.width
        KindIcon {
          kind: "snippet"
          side: 24
          color: Theme.accent
          anchors.verticalCenter: parent.verticalCenter
        }
        Column {
          width: parent.width - 34
          Text {
            width: parent.width
            text: root.snippet ? root.snippet.name : ""
            color: Theme.foreground
            font.family: Theme.fontFamily
            font.pixelSize: Theme.rowTitleSize + 2
            font.weight: Font.DemiBold
            elide: Text.ElideRight
          }
          Text {
            width: parent.width
            text: {
              var s = root.snippet || {}
              var out = []
              if (s.keyword)
                out.push("Keyword " + s.keyword)
              out.push(s.uses ? "used " + s.uses + (s.uses === 1 ? " time" : " times") : "never used")
              return out.join(" · ")
            }
            color: Theme.secondary
            font.family: Theme.fontFamily
            font.pixelSize: Theme.captionSize
          }
        }
      }

      Rectangle {
        width: parent.width
        height: body.implicitHeight + 20
        radius: 8
        color: Theme.keycapFill
        border.width: Theme.hairlineWidth()
        border.color: Theme.hairline
        Text {
          id: body
          x: 10
          y: 10
          width: parent.width - 20
          textFormat: Text.StyledText
          wrapMode: Text.Wrap
          // Placeholders in the accent color; the rest escaped as plain text.
          text: DeskModel.parts(root.snippet ? root.snippet.body : "").map(function (p) {
            var t = p.text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/\n/g, "<br>")
            return p.place ? "<font color=\"" + Theme.accent + "\">" + t + "</font>" : t
          }).join("")
          color: Theme.foreground
          font.family: Theme.monoFamily
          font.pixelSize: Math.round(Theme.fontBase * 0.95)
        }
      }

      Column {
        id: fields
        width: parent.width
        spacing: 8
        visible: root.args.length > 0
        property int activeIndex: -1
        // The argument fields by index (each registers itself).
        property var inputs: []

        function focusAt(i) {
          if (fields.inputs[i])
            fields.inputs[i].forceActiveFocus()
        }

        Repeater {
          id: rep
          model: root.args
          delegate: Column {
            id: argCell
            required property string modelData
            required property int index
            width: fields.width
            spacing: 3
            Text {
              text: argCell.modelData
              color: Theme.secondary
              font.family: Theme.fontFamily
              font.pixelSize: Theme.captionSize
            }
            TextField {
              id: f
              width: parent.width
              color: Theme.foreground
              font.family: Theme.fontFamily
              Component.onCompleted: {
                var list = fields.inputs.slice()
                list[argCell.index] = f
                fields.inputs = list
              }
              onActiveFocusChanged: fields.activeIndex = activeFocus ? argCell.index : (fields.activeIndex === argCell.index ? -1 : fields.activeIndex)
              onTextEdited: {
                var v = root.values
                v[argCell.modelData] = text
                root.values = v
              }
              background: Rectangle {
                radius: 6
                color: Theme.keycapFill
                border.width: f.activeFocus ? 2 : 1
                border.color: f.activeFocus ? Theme.accent : Theme.hairline
              }
              Keys.onReturnPressed: root.next(argCell.index)
              Keys.onEnterPressed: root.next(argCell.index)
              Keys.onTabPressed: root.next(argCell.index)
              Keys.onEscapePressed: root.escaped()
            }
          }
        }
      }

      Text {
        width: parent.width
        wrapMode: Text.WordWrap
        text: root.args.length ? "Fill in the blanks, then ↵ pastes it into the app you were in." : "↵ pastes it into the app you were in; ctrl ↵ copies it."
        color: Theme.secondary
        font.family: Theme.fontFamily
        font.pixelSize: Theme.captionSize
      }
    }
  }

  function next(i) {
    if (i + 1 < root.args.length)
      fields.focusAt(i + 1)
    else if (root.complete())
      root.submit(root.values)
  }

  EmptyState {
    visible: root.snippet === null
    anchors.centerIn: parent
    width: parent.width - 48
    title: "No snippets yet"
    detail: "Create Snippet saves text you type often. {date}, {clipboard} and {argument:name} are filled in when it pastes."
  }
}
