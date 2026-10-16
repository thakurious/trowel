import QtQuick
import "Trowel"

Item {
  id: root
  required property var client
  property alias queryText: query.text
  signal quickLookRequested
  width: 1040
  height: 700

  SearchSession {
    id: session
    client: root.client
  }

  Connections {
    target: root.client
    function onSearchArrived(g, meaning) {
      session.onSearchArrived(meaning)
    }
    function onPreviewArrived(msg) {
      session.preview = msg
    }
  }

  Column {
    anchors.fill: parent
    spacing: 0

    QueryField {
      id: query
      width: parent.width
      codeMode: session.codeMode
      hybridPending: session.phase === "words"
      onTextEdited: session.setQuery(text)
      onKeyPressed: function (event) {
        var ctrl = event.modifiers & Qt.ControlModifier
        if (event.key === Qt.Key_Down) {
          session.moveSelection(1)
          event.accepted = true
        } else if (event.key === Qt.Key_Up) {
          session.moveSelection(-1)
          event.accepted = true
        } else if (event.key === Qt.Key_Tab) {
          session.toggleCode()
          event.accepted = true
        } else if ((event.key === Qt.Key_Return || event.key === Qt.Key_Enter) && ctrl) {
          client.revealRow(session.selectedRow)
          event.accepted = true
        } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
          client.openRow(session.selectedRow)
          event.accepted = true
        } else if (event.key === Qt.Key_Space && session.navigated) {
          if (session.selectedRow && session.selectedRow.segment_id)
            client.quickLook(session.selectedRow.segment_id)
          root.quickLookRequested()
          event.accepted = true
        } else if (ctrl && event.key === Qt.Key_Y) {
          if (session.selectedRow && session.selectedRow.segment_id)
            client.quickLook(session.selectedRow.segment_id)
          root.quickLookRequested()
          event.accepted = true
        } else if (ctrl && event.key === Qt.Key_C && session.navigated) {
          client.copyPath(session.selectedRow)
          event.accepted = true
        }
      }
    }

    FilterBar {
      id: filters
      width: parent.width
      // The query's strong results per kind, as upstream's kind buttons
      // (`SearchWindow.swift:208`), not the index totals.
      counts: client.counts
      // Dim kinds with no result once the query's file counts are known (also on the Code
      // tab, which keeps them); a query typed on the Code tab only knows the Code count.
      dimEmpty: Object.keys(client.counts || {}).some(function (k) {
        return k !== "code"
      })
      showCode: !!(client.config && client.config.code_roots && client.config.code_roots.length)
      onPicked: function (kind) {
        client.kinds = filters.kindsFor(kind)
        session.codeTab = kind === "code"
        if (session.query.length)
          session.setQuery(session.query)
      }
    }

    Rectangle {
      width: parent.width
      height: 1
      color: Theme.hairline
    }

    Item {
      width: parent.width
      height: parent.height - query.height - filters.height - 1 - statusBar.height

      Column {
        visible: session.phase === "empty"
        anchors.centerIn: parent
        width: Math.min(560, parent.width - 40)
        spacing: 8
        Text {
          width: parent.width
          horizontalAlignment: Text.AlignHCenter
          text: "Describe it. Dig it up."
          color: Theme.foreground
          font.pixelSize: 24
          font.weight: Font.Bold
        }
        Text {
          width: parent.width
          horizontalAlignment: Text.AlignHCenter
          wrapMode: Text.WordWrap
          text: "Type what you remember in the search field above: what's in it, what it looks or sounds like, or words from it."
          color: Theme.secondary
          font.pixelSize: Theme.rowTitleSize
        }
        Text {
          width: parent.width
          horizontalAlignment: Text.AlignHCenter
          text: "TRY"
          color: Theme.secondary
          font.pixelSize: Theme.groupSize
          font.weight: Font.DemiBold
          font.letterSpacing: 0.8
          topPadding: 18
        }
        Flow {
          width: parent.width
          spacing: 8
          Repeater {
            model: ["a screenshot of an error message", "receipt", "whiteboard notes", "someone laughing", "a dog on the beach", "slides about the budget"]
            delegate: Rectangle {
              implicitHeight: 26
              implicitWidth: chip.implicitWidth + 16
              radius: 13
              color: Theme.keycapFill
              Text {
                id: chip
                anchors.centerIn: parent
                text: modelData
                color: Theme.foreground
                font.pixelSize: Theme.captionSize
              }
              MouseArea {
                anchors.fill: parent
                onClicked: {
                  query.text = modelData
                  session.setQuery(modelData)
                }
              }
            }
          }
        }
      }

      Row {
        visible: session.phase !== "empty"
        anchors.fill: parent

        ListView {
          id: results
          width: parent.width - 325
          height: parent.height
          model: client.rows
          clip: true
          reuseItems: true
          cacheBuffer: 240

          // Keep the selected row in view as the arrows move it (upstream ScrollViewReader
          // scrollTo): the smallest scroll, with a group's header when its first row is picked.
          function reveal(rowId) {
            for (var i = 0; i < count; i++) {
              if (model.get(i).rowId !== rowId)
                continue
              if (i > 0 && model.get(i - 1).rowType === "header")
                positionViewAtIndex(i - 1, ListView.Contain)
              positionViewAtIndex(i, ListView.Contain)
              return
            }
          }

          Connections {
            target: session
            function onSelectionChanged() {
              var id = session.selection
              Qt.callLater(function () {
                results.reveal(id)
              })
            }
          }
          delegate: Item {
            width: ListView.view.width
            height: model.rowType === "row" ? (session.codeMode ? 46 : 176) : (model.rowType === "header" ? 28 : 26)
            GroupHeader {
              visible: model.rowType === "header"
              title: model.title
              width: parent.width
            }
            WeakerFold {
              visible: model.rowType === "fold"
              width: parent.width
              count: model.count
              expanded: client.showWeaker
              selected: session.selection === model.rowId
              onToggled: {
                client.showWeaker = !client.showWeaker
                session.selection = client.weakerId
                session.navigated = true
              }
            }
            ResultRow {
              visible: model.rowType === "row"
              width: parent.width
              row: model
              selected: session.selection === model.rowId
              onClicked: {
                session.selection = model.rowId
                session.navigated = true
                session.pickSelected()
              }
              onOpened: client.openRow(model)
            }
          }
        }

        Rectangle {
          width: 1
          height: parent.height
          color: Theme.hairline
        }

        PreviewPane {
          width: 324
          height: parent.height
          preview: session.preview
          row: session.selectedRow
          query: session.query
        }
      }
    }

    Rectangle {
      id: statusBar
      width: parent.width
      height: 28
      color: Theme.alpha(Theme.foreground, 0.04)
      Text {
        anchors.verticalCenter: parent.verticalCenter
        anchors.left: parent.left
        anchors.leftMargin: 12
        text: client.bannerText.length ? client.bannerText : (client.connected ? (client.state || "ready") : "Trowel isn't running")
        color: Theme.secondary
        font.pixelSize: Theme.captionSize
      }
    }
  }
}
