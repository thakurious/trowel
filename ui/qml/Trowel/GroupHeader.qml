import QtQuick

// SearchPanelView.swift GroupHeader: 10 semibold uppercase tracking 0.6.
Text {
  id: root
  property string title: ""
  text: title.toUpperCase()
  color: Theme.secondary
  font.family: Theme.fontFamily
  font.pixelSize: Theme.groupSize
  font.weight: Font.DemiBold
  font.letterSpacing: 0.6
  leftPadding: 10
  rightPadding: 10
  topPadding: 10
  bottomPadding: 3
}
