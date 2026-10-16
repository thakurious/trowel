import QtQuick

// LAUNCHER-PLAN.md L4 calculator card: the full panel width above the two columns, the
// expression on the left and the result on the right at the query-field size, tabular figures.
Rectangle {
  id: root
  property var result: null
  property bool selected: true
  signal clicked
  signal opened
  implicitHeight: 72
  radius: Theme.innerRadius(Theme.panelRadius()) || 8
  color: selected ? Theme.selectedFill : Theme.alpha(Theme.foreground, 0.045)
  border.width: Theme.hairlineWidth()
  border.color: Theme.alpha(Theme.foreground, 0.1)

  readonly property string expr: (result && result.pretty_expr) ? result.pretty_expr : ((result && result.expr) ? result.expr : "")
  readonly property string display: (result && result.display) ? result.display : ""
  readonly property string caption: {
    if (!result)
      return ""
    switch (result.kind) {
    case "unit":
      return "Conversion"
    case "base":
      return "Number base"
    case "date":
      return "Date"
    case "duration":
      return "Duration"
    default:
      return "Calculation"
    }
  }

  Item {
    anchors.fill: parent
    anchors.leftMargin: 18
    anchors.rightMargin: 18

    Text {
      id: exprText
      anchors.left: parent.left
      anchors.right: arrow.left
      anchors.rightMargin: 12
      anchors.verticalCenter: parent.verticalCenter
      text: root.expr
      color: Theme.secondary
      font.family: Theme.fontFamily
      font.pixelSize: Theme.querySize
      font.features: {
        "tnum": 1
      }
      elide: Text.ElideRight
      textFormat: Text.PlainText
    }

    Text {
      id: arrow
      anchors.centerIn: parent
      text: (root.result && root.result.approx) ? "≈" : "="
      color: Theme.muted
      font.family: Theme.fontFamily
      font.pixelSize: Theme.querySize
    }

    Column {
      anchors.left: arrow.right
      anchors.leftMargin: 12
      anchors.right: parent.right
      anchors.verticalCenter: parent.verticalCenter
      spacing: 1

      Text {
        width: parent.width
        horizontalAlignment: Text.AlignRight
        text: root.display
        color: Theme.foreground
        font.family: Theme.fontFamily
        font.pixelSize: Theme.querySize
        font.weight: Font.DemiBold
        font.features: {
          "tnum": 1
        }
        elide: Text.ElideLeft
        textFormat: Text.PlainText
      }

      Text {
        width: parent.width
        horizontalAlignment: Text.AlignRight
        text: root.caption
        color: Theme.secondary
        font.family: Theme.fontFamily
        font.pixelSize: Theme.captionSize
      }
    }
  }

  MouseArea {
    anchors.fill: parent
    onClicked: root.clicked()
    onDoubleClicked: root.opened()
  }
}
