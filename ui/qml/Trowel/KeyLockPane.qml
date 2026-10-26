import QtQuick
import "KeyLockModel.js" as KeyLockModel

// The Lock Keyboard view's right pane: what ↵ will do with the duration picked in the list.
Item {
  id: root
  property var row: null

  Column {
    visible: !!root.row
    x: 20
    y: 18
    width: parent.width - 40
    spacing: 14

    Row {
      spacing: 12
      Rectangle {
        width: 38
        height: 38
        radius: Theme.innerRadius(Theme.panelRadius()) + 3
        color: Theme.alpha(Theme.accent, 0.16)
        KindIcon {
          anchors.centerIn: parent
          kind: "keyboard"
          side: 20
        }
      }
      Column {
        anchors.verticalCenter: parent.verticalCenter
        spacing: 2
        Text {
          text: root.row ? "Lock for " + KeyLockModel.label(root.row.seconds) : ""
          color: Theme.foreground
          font.family: Theme.fontFamily
          font.pixelSize: Theme.rowTitleSize + 3
          font.weight: Font.DemiBold
        }
        Text {
          text: "Lock Keyboard"
          color: Theme.secondary
          font.family: Theme.fontFamily
          font.pixelSize: Theme.captionSize
        }
      }
    }

    Text {
      width: parent.width
      wrapMode: Text.Wrap
      lineHeight: 1.25
      text: "Every key is ignored until the countdown ends, so you can wipe the keyboard. Your shortcuts don't work either, Trowel's included. The countdown shows at the top of the screen; click Unlock there to end it early. The mouse, touchpad and power button keep working."
      color: Theme.secondary
      font.family: Theme.fontFamily
      font.pixelSize: Theme.captionSize + 1
    }

    Text {
      width: parent.width
      wrapMode: Text.Wrap
      text: "Type a duration for one that isn't listed, like 45 or 3m (up to 30 minutes)."
      color: Theme.alpha(Theme.foreground, 0.45)
      font.family: Theme.fontFamily
      font.pixelSize: Theme.captionSize
    }
  }
}
