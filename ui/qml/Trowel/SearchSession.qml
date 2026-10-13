import QtQuick

// Two-phase search (UI-DESIGN §3.3): keyword immediately, hybrid after 120 ms.
QtObject {
  id: root
  property var client
  property string query: ""
  property bool codeMode: false
  property string phase: "empty"
  property bool navigated: false
  property string selection: ""
  property var selectedRow: null
  property var preview: ({})
  property int typingPauseMs: 120

  function setQuery(raw) {
    root.query = raw
    root.navigated = false
    var trimmed = String(raw).replace(/^\s+|\s+$/g, "")
    root.codeMode = trimmed.indexOf("code:") === 0
    var text = root.codeMode ? trimmed.replace(/^code:\s*/, "") : trimmed
    if (trimmed.length === 0) {
      root.phase = "empty"
      root.selection = ""
      root.selectedRow = null
      if (client) {
        client.groups = []
        client.weaker = []
        client.syncModel()
      }
      hybrid.stop()
      return
    }
    if (root.codeMode && text.length === 0) {
      root.phase = "codeHint"
      if (client) {
        client.groups = []
        client.weaker = []
        client.syncModel()
      }
      return
    }
    root.phase = "words"
    if (client) {
      client.typing(text, root.codeMode)
      client.search(text, "keyword", root.codeMode, true)
    }
    hybrid.restart()
  }

  function toggleCode() {
    var t = root.query
    if (t.indexOf("code:") === 0)
      setQuery(t.replace(/^code:\s*/, ""))
    else
      setQuery("code: " + t)
  }

  function runHybrid() {
    var trimmed = String(root.query).replace(/^\s+|\s+$/g, "")
    var text = root.codeMode ? trimmed.replace(/^code:\s*/, "") : trimmed
    if (text.length && client)
      client.search(text, "hybrid", root.codeMode, false)
  }

  function onSearchArrived(meaning) {
    if (meaning === "used")
      root.phase = "meaning"
    else if (meaning === "unavailable")
      root.phase = "wordsOnly"
    if (!client)
      return
    if (!root.navigated || !selectionPresent()) {
      for (var i = 0; i < client.rows.count; i++) {
        if (client.rows.get(i).rowType === "row") {
          root.selection = client.rows.get(i).rowId
          break
        }
      }
    }
    pickSelected()
  }

  function selectionPresent() {
    if (!client)
      return false
    for (var i = 0; i < client.rows.count; i++) {
      if (client.rows.get(i).rowId === root.selection)
        return true
    }
    return false
  }

  function pickSelected() {
    if (!client)
      return
    for (var i = 0; i < client.rows.count; i++) {
      var it = client.rows.get(i)
      if (it.rowId === root.selection && it.rowType === "row") {
        root.selectedRow = it
        if (it.segment_id)
          client.preview(it.segment_id, 450)
        return
      }
    }
    root.selectedRow = null
  }

  function moveSelection(step) {
    if (!client)
      return
    var ids = []
    for (var i = 0; i < client.rows.count; i++) {
      var it = client.rows.get(i)
      if (it.rowType === "row" || it.rowType === "fold")
        ids.push(it.rowId)
    }
    if (ids.length === 0)
      return
    var idx = ids.indexOf(root.selection)
    if (idx < 0)
      idx = -1
    idx = Math.max(0, Math.min(ids.length - 1, idx + step))
    root.selection = ids[idx]
    root.navigated = true
    pickSelected()
  }

  property Timer hybrid: Timer {
    interval: root.typingPauseMs
    repeat: false
    onTriggered: root.runHybrid()
  }
}
