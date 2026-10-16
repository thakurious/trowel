import QtQuick
import Quickshell
import Quickshell.Io
import "ListMerge.js" as ListMerge

// PROTOCOL v2 client. UI-DESIGN §5: gen-based stale dropping; ListModel
// insert/remove/move, never clear()+append() of the whole set. Launcher sources (apps, calc,
// clipboard; LAUNCHER-PLAN.md L4) keep their own gen, so a slow file search never drops a fresh
// apps reply, and their replies are routed by request id.
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
  property int protocol: 2
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
  // A slow reply (a cold hybrid search, a video frame) is not a dead daemon: when nothing has come
  // back for stallMs, ask for `status`, and only call it stalled if that goes unanswered too (F18).
  property bool probing: false
  // Query text per search gen, and the text whose rows are on screen.
  property var genText: ({})
  property string rowsText: ""
  property var kinds: []
  // The indexing pass that is running: the main index's, else the code index's (the helper
  // reads code only once the main index has nothing waiting). {done, total, code} or null.
  readonly property var activePass: {
    var st = root.status || {}
    if (st.phase !== "indexing" && st.phase !== "syncing")
      return null
    var main = st.main || {}
    if (main.total > 0 && main.done < main.total)
      return {
        "done": main.done,
        "total": main.total,
        "code": false
      }
    var code = st.code || {}
    if (code.total > 0)
      return {
        "done": code.done,
        "total": code.total,
        "code": true
      }
    return null
  }
  readonly property bool paused: !!(root.status && (root.status.paused_reason || root.status.phase === "paused"))
  // Clipboard incognito from `status.clip_paused_until` (-1 = until resumed, else a Unix time).
  readonly property real clipPausedUntil: (root.status && root.status.clip_paused_until) ? root.status.clip_paused_until : 0
  function clipPausedAt(nowSec) {
    return root.clipPausedUntil === -1 || root.clipPausedUntil > nowSec
  }

  // Strong results per kind for the query without the kind filter (the tab counts),
  // and the query text they belong to.
  property var counts: ({})
  property string countsText: ""
  // Per search gen: {text, hybrid, code}, to place each reply's counts.
  property var genMeta: ({})
  property bool lastSearchCode: false

  // Launcher (v2). `apps` belong to `appsText`; the merged list shows them with the file groups.
  property var apps: []
  property string appsText: ""
  property int appsGen: 0
  property var calcResult: null
  property string calcError: ""
  property int calcGen: 0
  property var clips: []
  property int clipTotal: 0
  property int clipGen: 0
  property string clipText: ""
  property var clipKinds: []
  // Off for the Images grid (ListMerge.clipRows).
  property bool clipHeaders: true
  // rowId → the plain JS item behind each row. ListModel turns nested objects and arrays into
  // models, so previews read app and clip details from here instead.
  property var rowIndex: ({})
  property var clipIndex: ({})
  // Request id → op, for replies that only make sense to the op that asked.
  property var routes: ({})
  readonly property bool hasApps: (root.capabilities || []).indexOf("apps") >= 0
  readonly property bool hasClipboard: (root.capabilities || []).indexOf("clipboard") >= 0
  readonly property bool hasCalc: (root.capabilities || []).indexOf("calc") >= 0

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
  signal appsArrived
  signal calcArrived
  signal clipsArrived
  signal clipContentArrived(var msg)
  signal clipboardChanged(var msg)
  // A routed op failed (not_found, io, …) or finished: the panel shows a toast.
  signal opFailed(string op, var error)
  signal opDone(string op, var msg)

  property ListModel rows: ListModel {}
  property ListModel clipModel: ListModel {}

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

  function sendRouted(op, obj) {
    obj.op = op
    var id = send(obj)
    if (id) {
      var r = root.routes
      r[id] = op
    }
    return id
  }

  // Apps for the query (an empty text is frecency order; the panel only asks with text).
  function appsQuery(text) {
    root.appsGen += 1
    root.appsText = String(text)
    return sendRouted("apps_query", {
      text: String(text),
      limit: 8,
      gen: root.appsGen
    })
  }

  function clearApps() {
    root.appsGen += 1
    root.apps = []
    root.appsText = ""
  }

  function calc(text, force) {
    root.calcGen += 1
    return sendRouted("calc", {
      text: String(text),
      force: !!force,
      gen: root.calcGen
    })
  }

  function clearCalc() {
    root.calcGen += 1
    root.calcResult = null
    root.calcError = ""
  }

  // mode: "copy" | "paste". `expr` goes to the calculator history.
  function calcCopy(text, expr, mode) {
    return sendRouted("calc_copy", {
      text: String(text),
      expr: expr || undefined,
      mode: mode || "copy"
    })
  }

  function appLaunch(appId, action) {
    var req = {
      app_id: appId
    }
    if (action)
      req.action = action
    return sendRouted("app_launch", req)
  }

  function appHide(appId, hidden) {
    return sendRouted("app_hide", {
      app_id: appId,
      hidden: hidden !== false
    })
  }

  function clipQuery(text, kinds) {
    root.clipGen += 1
    root.clipText = String(text || "")
    root.clipKinds = kinds || []
    var req = {
      text: root.clipText,
      limit: 100,
      gen: root.clipGen
    }
    if (root.clipKinds.length)
      req.kinds = root.clipKinds
    return sendRouted("clip_query", req)
  }

  function clipRefresh() {
    clipQuery(root.clipText, root.clipKinds)
  }

  function clipGet(clipId) {
    return sendRouted("clip_get", {
      clip_id: clipId
    })
  }

  // The panel must already be hidden for "paste"/"plain" (D21): the daemon sends Shift+Insert.
  function clipPaste(clipId, mode) {
    return sendRouted("clip_paste", {
      clip_id: clipId,
      mode: mode || "paste"
    })
  }

  function clipPin(clipId, pinned) {
    return sendRouted("clip_pin", {
      clip_id: clipId,
      pinned: !!pinned
    })
  }

  function clipDelete(clipId) {
    return sendRouted("clip_delete", {
      clip_id: clipId
    })
  }

  function clipClear(keepPinned) {
    return sendRouted("clip_clear", {
      keep_pinned: keepPinned !== false
    })
  }

  // minutes: undefined pauses until resumed, 0 resumes.
  function clipPause(minutes) {
    var req = {}
    if (minutes !== undefined && minutes !== null)
      req.minutes = minutes
    return sendRouted("clip_pause", req)
  }

  function clipImport() {
    return sendRouted("clip_import", {
      source: "omarchy"
    })
  }

  function indexOf(items) {
    var map = ({})
    for (var i = 0; i < items.length; i++)
      map[items[i].rowId] = items[i]
    return map
  }

  function syncClips() {
    var wanted = ListMerge.clipRows(root.clips, root.clipHeaders)
    root.clipIndex = indexOf(wanted)
    syncList(root.clipModel, wanted)
  }

  // A routed reply: true when handled.
  function onRouted(op, msg) {
    if (msg.ok === false) {
      root.opFailed(op, msg.error || {})
      return true
    }
    if (op === "apps_query") {
      if (msg.gen < root.appsGen)
        return true
      root.apps = msg.rows || []
      syncModel()
      root.appsArrived()
    } else if (op === "calc") {
      if (msg.gen < root.calcGen)
        return true
      root.calcResult = msg.result || null
      root.calcError = msg.error || ""
      root.calcArrived()
    } else if (op === "clip_query") {
      if (msg.gen < root.clipGen)
        return true
      root.clips = msg.rows || []
      root.clipTotal = msg.total || 0
      syncClips()
      root.clipsArrived()
    } else if (op === "clip_get") {
      root.clipContentArrived(msg)
    } else {
      root.opDone(op, msg)
    }
    return true
  }

  function hello() {
    root.helloOk = false
    send({
      op: "hello",
      protocol: 2,
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
    var texts = ({})
    texts[g] = String(text) + (mode === "hybrid" ? "\u0000hybrid" : "")
    if (root.genText[g - 1] !== undefined)
      texts[g - 1] = root.genText[g - 1]
    root.genText = texts
    var metas = ({})
    metas[g] = {
      "text": String(text),
      "hybrid": mode === "hybrid",
      "code": !!code
    }
    root.genMeta = metas
    root.lastSearchCode = !!code
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
      topics: ["status", "clipboard"]
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
    var loc = null
    if (row.focus)
      loc = row.focus
    else if (row.raw && row.raw.focus)
      loc = row.raw.focus
    else if (row.loc !== undefined && row.loc !== null)
      loc = row.loc
    send({
      op: "open",
      path: pathOf(row),
      segment_id: row.segment_id || null,
      code: root.lastSearchCode,
      loc: loc
    })
  }

  function revealRow(row) {
    if (!row)
      return
    send({
      op: "reveal",
      path: pathOf(row)
    })
  }

  function copyPath(row) {
    if (!row)
      return
    send({
      op: "copy_path",
      path: pathOf(row)
    })
  }

  function preview(segmentId, width) {
    send({
      op: "preview",
      segment_id: segmentId,
      width: width || 450,
      code: root.lastSearchCode
    })
  }

  function thumbnail(segmentId, size) {
    send({
      op: "thumbnail",
      segment_id: segmentId,
      size: size || 132,
      code: root.lastSearchCode
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
    reconnectSoon()
  }

  function reconnectSoon() {
    reconnectTimer.restart()
  }

  // File groups, then the launcher sources merged in (ListMerge.js).
  function flatten() {
    return ListMerge.merge(flattenFiles(), root.apps, root.appsText)
  }

  function flattenFiles() {
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

  // A path on the wire is a string, or {b64, display} when it isn't UTF-8 (PROTOCOL §6). Row ids
  // must be one ListModel type, so those become "b64:<…>"; `pathOf` sends the original back.
  function rowKey(id) {
    if (typeof id === "string")
      return id
    return id && id.b64 ? "b64:" + id.b64 : String(id)
  }

  function pathOf(row) {
    var key = row.rowId || row.id || row.path
    var item = root.rowIndex[key]
    if (item && item.raw && item.raw.id !== undefined)
      return item.raw.id
    return key
  }

  function rowItem(r, weak) {
    return {
      rowType: "row",
      rowId: rowKey(r.id),
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
      focus: r.focus,
      weak: !!weak,
      raw: r
    }
  }

  function syncModel() {
    var wanted = flatten()
    root.rowIndex = indexOf(wanted)
    syncList(root.rows, wanted)
  }

  function syncList(model, wanted) {
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

  // Tab counts: a reply without `counts` means none. A keyword pass never replaces the hybrid
  // counts of the same text (they differ; switching tabs flickered); a code search only sets
  // the Code count and keeps the file kinds' counts for the same text.
  function applyCounts(msg) {
    var meta = root.genMeta[msg.gen]
    if (!meta)
      return
    var c = msg.counts || ({})
    var same = meta.text === root.countsText
    var next = ({})
    var k
    if (meta.code) {
      if (same)
        for (k in root.counts)
          next[k] = root.counts[k]
      next.code = c.code || 0
    } else {
      if (same && !meta.hybrid && Object.keys(root.counts).length)
        return
      for (k in c)
        next[k] = c[k]
      if (same && root.counts.code !== undefined)
        next.code = root.counts.code
    }
    root.counts = next
    root.countsText = meta.text
  }

  function onLine(line) {
    root.stalled = false
    root.probing = false
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
      else if (msg.event === "clipboard")
        root.clipboardChanged(msg)
      root.lineEvent(msg)
      return
    }
    if (msg.id !== undefined && root.routes[msg.id] !== undefined) {
      var op = root.routes[msg.id]
      delete root.routes[msg.id]
      root.onRouted(op, msg)
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
      // word before document vectors exist. Keep the keyword hits of that
      // same text; a query that matches nothing must still clear the list.
      var asked = root.genText[msg.gen] || ""
      var hybridOfShown = asked.indexOf("\u0000hybrid") > 0
          && asked.split("\u0000")[0] === root.rowsText
      applyCounts(msg)
      if (incomingRows === 0 && root.rows.count > 0 && hybridOfShown) {
        root.appliedGen = msg.gen
        root.meaning = msg.meaning || root.meaning
        root.tookMs = msg.took_ms || root.tookMs
        root.searchArrived(msg.gen, root.meaning)
        return
      }
      root.appliedGen = msg.gen
      root.rowsText = asked.split("\u0000")[0]
      root.groups = incoming
      root.weaker = msg.weaker || []
      root.meaning = msg.meaning || ""
      root.tookMs = msg.took_ms || 0
      syncModel()
      root.searchArrived(msg.gen, root.meaning)
      return
    }
    if (msg.present !== undefined && msg.total !== undefined) {
      // model_status / model_download replies are not events.
      root.lineEvent(msg)
      return
    }
    if (msg.roots !== undefined) {
      root.config = msg
      root.configArrived(msg)
    } else if (msg.config !== undefined) {
      root.config = msg.config
      root.configArrived(msg.config)
    }
    // type is enough: an image thumb may have file=null until the pane
    // falls back to the original path.
    if (msg.type)
      root.previewArrived(msg)
  }

  property Timer stallTimer: Timer {
    interval: root.stallMs
    repeat: false
    onTriggered: {
      if (!root.connected)
        return
      if (root.probing) {
        root.stalled = true
        return
      }
      root.probing = true
      root.send({
        op: "status"
      })
    }
  }

  // Quickshell Socket drops `connected` when troweld restarts and does not
  // retry on its own. Must be a property: Client is a QtObject.
  property Timer reconnectTimer: Timer {
    interval: 400
    repeat: false
    onTriggered: {
      if (root.wantConnected && !root.sock.connected)
        root.sock.connected = true
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
        root.reconnectTimer.stop()
        root.hello()
      } else {
        root.helloOk = false
        root.routes = ({})
        root.state = "offline"
        if (root.wantConnected)
          root.reconnectTimer.restart()
      }
    }
    onError: function () {
      root.state = "offline"
      if (root.wantConnected)
        root.reconnectTimer.restart()
    }
  }

  property Process starter: Process {
    command: ["systemctl", "--user", "start", "troweld"]
    running: false
    onExited: root.reconnectSoon()
  }
}
