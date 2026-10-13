pragma Singleton
import QtQuick
import Quickshell.Io

// Copy from ui/strings.json (Phase 1 extract). Lookup is exact upstream text.
QtObject {
  id: root

  property var byText: ({})
  property int count: 0
  readonly property string stringsPath: {
    var u = String(Qt.resolvedUrl("../../strings.json"))
    return u.replace(/^file:\/\//, "")
  }

  function t(text) {
    return text
  }

  function has(text) {
    return root.byText[text] === true
  }

  property FileView file: FileView {
    path: root.stringsPath
    watchChanges: false
    printErrors: false
    onLoaded: {
      try {
        var arr = JSON.parse(text())
        var map = {}
        var n = 0
        for (var i = 0; i < arr.length; i++) {
          if (arr[i] && arr[i].text) {
            map[arr[i].text] = true
            n++
          }
        }
        root.byText = map
        root.count = n
      } catch (e) {
        root.count = 0
      }
    }
  }
}
