.pragma library

// Phase 9 views (PHASE9-PLAN.md §1): Timer, Process Monitor, Snippets, Omarchy commands, themes,
// and the system commands in the main list. Pure (no QML types), so
// `scripts/deskmodel-test.mjs` runs it under node. The daemon parses, samples and ranks; this
// only shapes rows and formats numbers.

// m:ss or h:mm:ss, rounded up so a running timer never reads 0:00 (troweld `clock::countdown`).
function clock(seconds) {
  var s = Math.max(0, Math.ceil(Number(seconds) || 0))
  var h = Math.floor(s / 3600)
  var m = Math.floor(s / 60) % 60
  var r = s % 60
  var two = function (n) {
    return (n < 10 ? "0" : "") + n
  }
  return h > 0 ? h + ":" + two(m) + ":" + two(r) : m + ":" + two(r)
}

// "25 min", "1 h 30 min", "45 s": a duration in words for subtitles.
function duration(seconds) {
  var s = Math.round(Number(seconds) || 0)
  if (s < 60)
    return s + " s"
  var h = Math.floor(s / 3600)
  var m = Math.floor(s / 60) % 60
  var r = s % 60
  var out = []
  if (h)
    out.push(h + " h")
  if (m)
    out.push(m + " min")
  if (r && !h)
    out.push(r + " s")
  return out.join(" ")
}

function bytes(n) {
  if (n === null || n === undefined)
    return "—"
  var v = Number(n)
  var units = ["B", "KB", "MB", "GB", "TB"]
  var i = 0
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024
    i++
  }
  return (i === 0 || v >= 100 ? Math.round(v) : v.toFixed(1)) + " " + units[i]
}

// Disk read+write per second; "—" when the daemon can't read it (another user's process).
function rate(read, write) {
  if (read === null || read === undefined || write === null || write === undefined)
    return "—"
  var t = Number(read) + Number(write)
  if (t < 1024)
    return "0 B/s"
  return bytes(t) + "/s"
}

function pct(v) {
  if (v === null || v === undefined)
    return "—"
  var n = Number(v)
  if (n > 0 && n < 0.05)
    return "<0.1%"
  return (n < 10 ? n.toFixed(1) : Math.round(n)) + "%"
}

// ---- main-list commands

// What the daemon's `system_commands` lists, as ListMerge command objects. Ids get a `sys:`
// prefix so they never clash with the panel's own commands; toggles carry On/Off.
function systemCommands(list) {
  var out = []
  var cs = list || []
  for (var i = 0; i < cs.length; i++) {
    var c = cs[i]
    out.push({
      id: "sys:" + c.id,
      name: c.name,
      subtitle: c.subtitle || "",
      keywords: (c.keywords || []).concat([String(c.group || "").toLowerCase()]),
      glyph: c.glyph || "power",
      accessory: c.state === true ? "On" : (c.state === false ? "Off" : ""),
      confirm: !!c.confirm,
      system: c.id
    })
  }
  return out
}

// Commands whose name follows state: indexing and clipboard capture paused or not.
function stateCommands(ctx) {
  var c = ctx || {}
  return [{
      id: c.indexingPaused ? "resume-indexing" : "pause-indexing",
      name: c.indexingPaused ? "Resume Indexing" : "Pause Indexing",
      subtitle: c.indexingPaused ? "Start reading new and changed files again" : "Stop reading files until you resume",
      keywords: ["pause", "resume", "indexing", "index", "stop", "trowel"],
      glyph: "refresh"
    }, {
      id: c.clipPaused ? "resume-clipboard" : "pause-clipboard",
      name: c.clipPaused ? "Resume Clipboard History" : "Pause Clipboard History",
      subtitle: c.clipPaused ? "Record what you copy again" : "Don't record copies for 30 minutes",
      keywords: ["pause", "resume", "clipboard", "history", "private", "incognito"],
      glyph: "clipboard"
    }]
}

// A typed timer offered first in the main list ("25m tea" → Start Timer).
function timerCommand(spec) {
  if (!spec || !(spec.seconds > 0))
    return null
  return {
    id: "timer-start",
    name: "Start Timer · " + clock(spec.seconds) + (spec.label ? " " + spec.label : ""),
    subtitle: spec.at ? "Ends at the time you typed" : duration(spec.seconds) + " countdown on the Island",
    keywords: [],
    glyph: "timer",
    lead: true,
    spec: spec
  }
}

// Snippets whose keyword is exactly the query (`;sig`), for the main list.
function keywordHits(snippets, query) {
  var q = String(query || "").replace(/^\s+|\s+$/g, "").toLowerCase()
  var out = []
  if (!q.length)
    return out
  var ss = snippets || []
  for (var i = 0; i < ss.length; i++)
    if (ss[i].keyword && String(ss[i].keyword).toLowerCase() === q)
      out.push(ss[i])
  return out
}

// ---- Timer view

var PRESETS = [1, 3, 5, 10, 15, 25, 30, 45, 60]

function timerLeft(t, now) {
  if (t.state === "running" && t.ends_at)
    return Math.max(0, t.ends_at - now)
  return t.left || 0
}

// The view's rows: what the query would start (or the presets), then the timers there are.
function timerRows(query, spec, timers, now) {
  var out = []
  var q = String(query || "").replace(/^\s+|\s+$/g, "")
  if (q.length) {
    if (spec && spec.seconds > 0)
      out.push({
        rowType: "timer-new",
        rowId: "tnew:typed",
        title: "Start " + clock(spec.seconds) + (spec.label ? " · " + spec.label : ""),
        subtitle: spec.at ? "Ends at the time you typed" : duration(spec.seconds),
        seconds: spec.seconds,
        label: spec.label || "",
        accessory: "Typed"
      })
  } else {
    for (var i = 0; i < PRESETS.length; i++)
      out.push({
        rowType: "timer-new",
        rowId: "tnew:" + PRESETS[i],
        title: PRESETS[i] + (PRESETS[i] === 1 ? " minute" : " minutes"),
        subtitle: "",
        seconds: PRESETS[i] * 60,
        label: "",
        accessory: ""
      })
  }
  var ts = (timers || []).filter(function (t) {
    return !q.length || String(t.label || "").toLowerCase().indexOf(q.toLowerCase()) >= 0 || (spec && spec.seconds > 0)
  })
  if (ts.length) {
    out.push({
      rowType: "header",
      rowId: "hdr:timers",
      title: "Timers",
      kind: "timers",
      count: ts.length
    })
    for (var j = 0; j < ts.length; j++) {
      var t = ts[j]
      var left = timerLeft(t, now)
      out.push({
        rowType: "timer",
        rowId: "timer:" + t.id,
        timerId: t.id,
        title: t.label || "Timer",
        subtitle: t.state === "done" ? "Done" : (t.state === "paused" ? "Paused · " + duration(t.total) : duration(t.total)),
        seconds: t.total,
        label: t.label || "",
        state: t.state,
        accessory: t.state === "done" ? "Done" : clock(left)
      })
    }
  }
  return out
}

// ---- Process Monitor

var SORTS = ["cpu", "gpu", "ram", "disk"]
var SORT_LABELS = {
  "cpu": "CPU",
  "gpu": "GPU",
  "ram": "Memory",
  "disk": "Disk"
}

function metric(row, sort) {
  if (!row)
    return ""
  if (sort === "gpu")
    return pct(row.gpu)
  if (sort === "ram")
    return bytes(row.ram)
  if (sort === "disk")
    return rate(row.disk_read, row.disk_write)
  return pct(row.cpu)
}

// The daemon's rows (already sorted) narrowed by the query, as list items.
function procRows(rows, query) {
  var q = String(query || "").replace(/^\s+|\s+$/g, "").toLowerCase()
  var out = []
  var rs = rows || []
  for (var i = 0; i < rs.length; i++) {
    var r = rs[i]
    if (q.length && String(r.name).toLowerCase().indexOf(q) < 0 && String(r.pids[0]) !== q)
      continue
    out.push({
      rowType: "proc",
      rowId: r.key,
      title: r.name,
      raw: r
    })
  }
  return out
}

function procSubtitle(r) {
  if (!r)
    return ""
  var n = (r.pids || []).length
  return (n === 1 ? "PID " + r.pids[0] : n + " processes") + (r.user && r.user !== "me" ? " · " + r.user : "")
}

// ---- Omarchy commands and themes

// Name prefix first, then a word of the name, then the summary; ties keep the daemon's order.
function scriptRows(scripts, query) {
  var q = String(query || "").replace(/^\s+|\s+$/g, "").toLowerCase()
  var scored = []
  var ss = scripts || []
  for (var i = 0; i < ss.length; i++) {
    var s = ss[i]
    var name = String(s.name).toLowerCase()
    var rank = 3
    if (!q.length)
      rank = 0
    else if (name.indexOf(q) === 0)
      rank = 0
    else if (name.split(/\s+/).some(function (w) {
        return w.indexOf(q) === 0
      }) || String(s.script).indexOf(q) >= 0)
      rank = 1
    else if (String(s.subtitle || "").toLowerCase().indexOf(q) >= 0)
      rank = 2
    if (rank < 3)
      scored.push({
        rank: rank,
        i: i
      })
  }
  scored.sort(function (a, b) {
    return a.rank - b.rank || a.i - b.i
  })
  return scored.map(function (x) {
    var s = ss[x.i]
    return {
      rowType: "script",
      rowId: s.id,
      title: s.name,
      subtitle: s.subtitle || "",
      accessory: s.terminal ? "Terminal" : "",
      glyph: "terminal"
    }
  })
}

function themeRows(themes, current, query) {
  var q = String(query || "").replace(/^\s+|\s+$/g, "").toLowerCase()
  var out = []
  var ts = themes || []
  for (var i = 0; i < ts.length; i++) {
    if (q.length && String(ts[i]).toLowerCase().indexOf(q) < 0)
      continue
    out.push({
      rowType: "theme",
      rowId: "theme:" + ts[i],
      title: ts[i],
      subtitle: "",
      accessory: ts[i] === current ? "Current" : "",
      glyph: "palette"
    })
  }
  return out
}

// ---- Snippets

function snippetRows(snippets) {
  var out = []
  var ss = snippets || []
  for (var i = 0; i < ss.length; i++)
    out.push({
      rowType: "snippet",
      rowId: "snip:" + ss[i].id,
      snippetId: ss[i].id,
      title: ss[i].name,
      subtitle: String(ss[i].body || "").replace(/\s+/g, " ").slice(0, 80),
      accessory: ss[i].keyword || "",
      raw: ss[i]
    })
  return out
}

// The body split into text and `{placeholder}` parts, for the pane's preview.
function parts(body) {
  var out = []
  var s = String(body || "")
  var re = /\{[^{}]*\}/g
  var at = 0
  var m
  while ((m = re.exec(s)) !== null) {
    if (m.index > at)
      out.push({
        text: s.slice(at, m.index),
        place: false
      })
    out.push({
      text: m[0],
      place: true
    })
    at = m.index + m[0].length
  }
  if (at < s.length)
    out.push({
      text: s.slice(at),
      place: false
    })
  return out
}
