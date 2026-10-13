import QtQuick
import Quickshell
import Quickshell.Io

// PROTOCOL v1 client. UI-DESIGN §5: gen-based stale dropping; ListModel
// insert/remove/move, never clear()+append() of the whole set.
QtObject {
  id: root

  property string clientName: "panel"
  property string socketPath: {
    var override = Quickshell.env("TROWEL_SOCKET")
    if (override && override.length)
      return override
    var runtime = Quickshell.env("XDG_RUNTIME_DIR")
    if (!runtime || runtime.length === 0)
      runtime = "/tmp"
    return runtime + "/trowel/troweld.sock"
  }
  property bool wantConnected: true
  readonly property bool connected: sock.connected
  property bool helloOk: false
  property string state: "connecting"
  property var status: ({})
  property var config: ({})
  property int protocol: 1
  property var capabilities: []
  property string lastError: ""
  property int nextId: 1
  property int gen: 0
  property int appliedGen: 0
  property var pending: ({})
  property var groups: []
  property var weaker: []
  property string meaning: ""
  property int tookMs: 0
  property bool showWeaker: false
  property bool stalled: false
  property int stallMs: 2000
  property var kinds: []

  readonly property string weakerId: "\u0000weaker"
  readonly property string bannerText: {
    if (stalled)
      return "Trowel isn't responding"
    if (!connected)
      return "Trowel isn't running"
    return ""
  }

  signal searchArrived(int gen, string meaning)
  signal lineEvent(var msg)
  signal previewArrived(var msg)
  signal configArrived(var cfg)

  property ListModel rows: ListModel {}

  function send(obj) {
    if (!sock.connected)
      return 0
    var id = root.nextId++
    obj.id = id
    sock.write(JSON.stringify(obj) + "\n")
    sock.flush()
    stallTimer.restart()
    return id
  }

  function hello() {
    root.helloOk = false
    send({
      op: "hello",
      protocol: 1,
      client: root.clientName,
      pid: 0
    })
  }

  function panelOpened() {
    send({
      op: "panel_opened"
    })
  }

  function typing(text, code) {
    send({
      op: "typing",
      text: text,
      code: !!code
    })
  }

  function search(text, mode, code, prefixLast) {
    root.gen += 1
    var g = root.gen
    var req = {
      op: "search",
      text: text,
      mode: mode || "keyword",
      code: !!code,
      prefix_last: prefixLast !== false,
      gen: g,
      limit: 60
    }
    if (root.kinds && root.kinds.length)
      req.kinds = root.kinds
    send(req)
    return g
  }

  function estimate(paths, code) {
    send({
      op: "estimate",
      paths: paths || [],
      code: !!code
    })
  }

  function modelStatus() {
    send({
      op: "model_status"
    })
  }

  function modelDownload(action) {
    send({
      op: "model_download",
      action: action || "start"
    })
  }

  function quickLook(segmentId) {
    send({
      op: "quick_look",
      segment_id: segmentId
    })
  }

  function subscribe() {
    send({
      op: "subscribe",
      topics: ["status"]
    })
  }

  function configGet() {
    send({
      op: "config_get"
    })
  }

  function configSet(patch) {
    send({
      op: "config_set",
      patch: patch
    })
  }

  function pause() {
    send({
      op: "pause"
    })
  }

  function resume() {
    send({
      op: "resume"
    })
  }

  function openRow(row) {
    if (!row)
      return
    send({
      op: "open",
      path: row.rowId || row.id || row.path,
      segment_id: row.segment_id || null
    })
  }

  function revealRow(row) {
    if (!row)
      return
    send({
      op: "reveal",
      path: row.rowId || row.id || row.path
    })
  }

  function copyPath(row) {
    if (!row)
      return
    send({
      op: "copy_path",
      path: row.rowId || row.id || row.path
    })
  }

  function preview(segmentId, width) {
    send({
      op: "preview",
      segment_id: segmentId,
      width: width || 450
    })
  }

  function thumbnail(segmentId, size) {
    send({
      op: "thumbnail",
      segment_id: segmentId,
      size: size || 132
    })
  }

  function setHotkey(chord) {
    send({
      op: "set_hotkey",
      chord: chord
    })
  }

  function startDaemon() {
    starter.running = true
  }

  function flatten() {
    var out = []
    var gs = root.groups || []
    for (var i = 0; i < gs.length; i++) {
      var g = gs[i]
      out.push({
        rowType: "header",
        rowId: "hdr:" + (g.kind || i),
        title: g.title || g.kind || "",
        kind: g.kind || "",
        total: g.total || 0,
        count: (g.rows || []).length
      })
      var rs = g.rows || []
      for (var j = 0; j < rs.length; j++)
        out.push(rowItem(rs[j], false))
    }
    var w = root.weaker || []
    if (w.length > 0) {
      out.push({
        rowType: "fold",
        rowId: root.weakerId,
        title: root.showWeaker ? "Weaker matches" : (w.length + " weaker " + (w.length === 1 ? "match" : "matches")),
        count: w.length,
        expanded: root.showWeaker
      })
      if (root.showWeaker) {
        for (var k = 0; k < w.length; k++)
          out.push(rowItem(w[k], true))
      }
    }
    return out
  }

  function rowItem(r, weak) {
    return {
      rowType: "row",
      rowId: r.id,
      name: r.name || "",
      title: r.name || "",
      folder: r.folder || "",
      kind: r.kind || "",
      excerpt: r.excerpt || "",
      place: r.place || "",
      reason: r.reason || "",
      thumb: r.thumb || "",
      matched_words: !!r.matched_words,
      matched_meaning: !!r.matched_meaning,
      segment_id: r.segment_id || 0,
      loc: r.loc,
      weak: !!weak,
      raw: r
    }
  }

  function syncModel() {
    var wanted = flatten()
    var model = root.rows
    var i = 0
    while (i < wanted.length || i < model.count) {
      if (i >= wanted.length) {
        model.remove(i, model.count - i)
        break
      }
      if (i >= model.count) {
        model.append(wanted[i])
        i++
        continue
      }
      var have = model.get(i)
      if (have.rowId === wanted[i].rowId && have.rowType === wanted[i].rowType) {
        model.set(i, wanted[i])
        i++
        continue
      }
      var found = -1
      for (var j = i + 1; j < model.count; j++) {
        if (model.get(j).rowId === wanted[i].rowId) {
          found = j
          break
        }
      }
      if (found >= 0)
        model.move(found, i, 1)
      else
        model.insert(i, wanted[i])
      model.set(i, wanted[i])
      i++
    }
  }

  function onLine(line) {
    root.stalled = false
    stallTimer.stop()
    var msg
    try {
      msg = JSON.parse(line)
    } catch (e) {
      return
    }
    if (msg.event) {
      if (msg.event === "status")
        root.status = msg
      root.lineEvent(msg)
      return
    }
    if (msg.ok === false && msg.error) {
      root.lastError = (msg.error.code || "") + ": " + (msg.error.message || "")
      return
    }
    if (msg.protocol !== undefined) {
      root.helloOk = msg.ok === true
      root.protocol = msg.protocol || 1
      root.state = msg.state || "ready"
      root.capabilities = msg.capabilities || []
      if (root.helloOk) {
        subscribe()
        configGet()
      }
      return
    }
    if (msg.groups !== undefined && msg.gen !== undefined) {
      if (msg.gen < root.gen)
        return
      var incoming = msg.groups || []
      var incomingRows = 0
      for (var gi = 0; gi < incoming.length; gi++)
        incomingRows += (incoming[gi].rows || []).length
      // Hybrid (prefix_last false) can return nothing for a half-typed last
      // word before document vectors exist. Keep the keyword hits.
      if (incomingRows === 0 && root.rows.count > 0) {
        root.appliedGen = msg.gen
        root.meaning = msg.meaning || root.meaning
        root.tookMs = msg.took_ms || root.tookMs
        root.searchArrived(msg.gen, root.meaning)
        return
      }
      root.appliedGen = msg.gen
      root.groups = incoming
      root.weaker = msg.weaker || []
      root.meaning = msg.meaning || ""
      root.tookMs = msg.took_ms || 0
      syncModel()
      root.searchArrived(msg.gen, root.meaning)
      return
    }
    if (msg.roots !== undefined) {
      root.config = msg
      root.configArrived(msg)
    } else if (msg.config !== undefined) {
      root.config = msg.config
      root.configArrived(msg.config)
    }
    if (msg.type && (msg.file || msg.lines || msg.text || msg.peaks || msg.frame || msg.band))
      root.previewArrived(msg)
  }

  property Timer stallTimer: Timer {
    interval: root.stallMs
    repeat: false
    onTriggered: {
      if (root.connected)
        root.stalled = true
    }
  }

  onShowWeakerChanged: syncModel()

  property Socket sock: Socket {
    path: root.socketPath
    connected: root.wantConnected && root.socketPath.length > 0
    parser: SplitParser {
      splitMarker: "\n"
      onRead: function (data) {
        root.onLine(data)
      }
    }
    onConnectionStateChanged: {
      if (connected) {
        root.lastError = ""
        root.hello()
      } else {
        root.helloOk = false
        root.state = "offline"
      }
    }
    onError: function () {
      root.state = "offline"
    }
  }

  property Process starter: Process {
    command: ["systemctl", "--user", "start", "troweld"]
    running: false
  }
}
