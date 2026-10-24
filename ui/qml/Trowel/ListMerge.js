.pragma library

// LAUNCHER-PLAN.md L4 "Merged list model": the panel's rows from the file groups and the
// launcher sources. Pure (no QML types), so `scripts/listmerge-test.mjs` runs it under node.
// Apps come ranked by the daemon. Commands are matched here, and the daemon's learned signals
// (`apps_query` `commands` / `AppRow.learned`, D19 as amended 2026-10-22) order them and decide
// which launcher section leads.

// Built-in commands (Raycast-style): searchable by name and keywords from the main list, and Enter
// runs them inside the panel. Clipboard History is the first; more go here.
var COMMANDS = [{
    id: "clipboard",
    name: "Clipboard History",
    subtitle: "Search, paste, copy and remove what you've copied",
    keywords: ["clipboard", "clip", "history", "paste", "copied", "copy"],
    glyph: "clipboard"
  }]

function words(text) {
  return String(text || "").toLowerCase().split(/[^a-z0-9]+/).filter(function (w) {
    return w.length > 0
  })
}

// Raycast's search-term classes (troweld/src/learn.rs): a query typed before to open the item.
function termClass(signal) {
  var t = signal ? signal.term : ""
  return t === "exact" ? 3 : (t === "prefix" ? 2 : 0)
}

function frecency(signal) {
  return signal && signal.frecency > 1 ? signal.frecency : 1
}

// A command matches when the query is a prefix of its name, of one of its name's words, or of a
// keyword. `strong` is the name or a keyword starting with it, like an app's name prefix, or a
// query the command was run with before (exact or as a prefix). `signals` maps id → learned
// signal; learned commands come first, then strong ones, then by frecency.
function matchCommands(query, commands, signals) {
  var q = String(query || "").replace(/^\s+|\s+$/g, "").toLowerCase()
  var out = []
  if (!q.length)
    return out
  var list = commands || COMMANDS
  var sigs = signals || {}
  for (var i = 0; i < list.length; i++) {
    var c = list[i]
    var name = String(c.name).toLowerCase()
    var keys = (c.keywords || []).map(function (k) {
      return String(k).toLowerCase()
    })
    var text = name.indexOf(q) === 0 || keys.some(function (k) {
      return k.indexOf(q) === 0
    })
    var weak = words(c.name).some(function (w) {
      return w.indexOf(q) === 0
    })
    var signal = sigs[c.id] || null
    if (text || weak)
      out.push({
        command: c,
        strong: text || termClass(signal) > 0,
        signal: signal,
        order: i
      })
  }
  out.sort(function (a, b) {
    return termClass(b.signal) - termClass(a.signal) || (b.strong ? 1 : 0) - (a.strong ? 1 : 0) || frecency(b.signal) - frecency(a.signal) || a.order - b.order
  })
  return out
}

function commandItem(c) {
  return {
    rowType: "command",
    rowId: "cmd:" + c.id,
    commandId: c.id,
    name: c.name,
    title: c.name,
    generic: c.subtitle || "",
    glyph: c.glyph || "app"
  }
}

// Omarchy's `AppSearch.js` name-prefix band: the query is a prefix of the app's name. A query
// the app was opened with before counts too (the daemon put it first for that reason).
function strongApp(app, query) {
  var q = String(query || "").replace(/^\s+|\s+$/g, "").toLowerCase()
  if (!q.length || !app)
    return false
  return String(app.name || "").toLowerCase().indexOf(q) === 0 || termClass(app.learned) > 0
}

function appItem(a) {
  return {
    rowType: "app",
    rowId: "app:" + a.id,
    appId: a.id,
    name: a.name || a.id,
    title: a.name || a.id,
    generic: a.generic || "",
    comment: a.comment || "",
    icon: a.icon || "",
    iconName: a.icon_name || "",
    actionCount: (a.actions || []).length,
    terminal: !!a.terminal,
    launches: a.launches || 0,
    raw: a
  }
}

function clipItem(c) {
  return {
    rowType: "clip",
    rowId: "clip:" + c.id,
    clipId: c.id,
    kind: c.kind || "text",
    title: c.preview || "",
    name: c.preview || "",
    thumb: c.thumb || "",
    color: c.color || "",
    domain: c.domain || "",
    sourceApp: c.source_app || "",
    files: c.files || 0,
    lastUsed: c.last_used || 0,
    pinned: !!c.pinned,
    raw: c
  }
}

// files: the file rows already flattened (headers, rows, the weaker fold), in daemon order.
// apps: AppRow[] for the same query. opts.maxApps caps the section (default 5); opts.commands is the
// command list (default COMMANDS, [] for none); opts.signals maps command id → learned signal.
// A section whose top match is strong (a command or app name starting with the query, or a query
// it was opened with before) goes first, so `fire⏎` launches Firefox and `clip⏎` opens Clipboard
// History; otherwise it follows the file groups (before the weaker fold), so row 0 stays the best
// file. When both lead, the one learned for this query wins, then the more frecent, then commands.
function merge(files, apps, query, opts) {
  opts = opts || {}
  var max = opts.maxApps === undefined ? 5 : opts.maxApps
  var top = []
  var bottom = []
  var cmds = matchCommands(query, opts.commands, opts.signals)
  var cs = []
  if (cmds.length) {
    cs = [{
      rowType: "header",
      rowId: "hdr:commands",
      title: "Commands",
      kind: "commands",
      count: cmds.length
    }]
    for (var c = 0; c < cmds.length; c++)
      cs.push(commandItem(cmds[c].command))
  }
  var list = (apps || []).slice(0, max)
  var section = []
  if (list.length) {
    section = [{
      rowType: "header",
      rowId: "hdr:apps",
      title: "Applications",
      kind: "apps",
      count: list.length
    }]
    for (var i = 0; i < list.length; i++)
      section.push(appItem(list[i]))
  }
  var cmdTop = cmds.length > 0 && cmds[0].strong
  var appTop = list.length > 0 && strongApp(list[0], query)
  var appsFirst = appTop && !cmdTop
  if (appTop && cmdTop)
    appsFirst = (termClass(list[0].learned) - termClass(cmds[0].signal) || frecency(list[0].learned) - frecency(cmds[0].signal)) > 0
  var launch = appsFirst ? [section, cs] : [cs, section]
  for (var s = 0; s < launch.length; s++) {
    var part = launch[s]
    if (!part.length)
      continue
    var lead = part === cs ? cmdTop : appTop
    if (lead)
      top = top.concat(part)
    else
      bottom = bottom.concat(part)
  }
  var rest = files || []
  var fold = rest.length
  for (var j = 0; j < rest.length; j++) {
    if (rest[j].rowType === "fold") {
      fold = j
      break
    }
  }
  return top.concat(rest.slice(0, fold), bottom, rest.slice(fold))
}

// Clipboard view rows: pinned first, as the daemon returns them; a "Pinned" header when any.
// `headers: false` for the Images grid, whose cells are all thumbnails.
function clipRows(clips, headers) {
  var out = []
  var cs = clips || []
  var pinned = 0
  for (var i = 0; i < cs.length; i++)
    if (cs[i].pinned && headers !== false)
      pinned++
  if (pinned > 0)
    out.push({
      rowType: "header",
      rowId: "hdr:pinned",
      title: "Pinned",
      kind: "pinned",
      count: pinned
    })
  for (var j = 0; j < cs.length; j++) {
    if (pinned > 0 && j === pinned)
      out.push({
        rowType: "header",
        rowId: "hdr:recent",
        title: "Recent",
        kind: "recent",
        count: cs.length - pinned
      })
    out.push(clipItem(cs[j]))
  }
  return out
}

// The rows ↑/↓ visit, in order. The calculator card is not a list row but comes first.
function selectable(rows, hasCalc) {
  var ids = hasCalc ? ["calc:"] : []
  for (var i = 0; i < rows.length; i++) {
    var t = rows[i].rowType
    if (t === "row" || t === "fold" || t === "app" || t === "clip" || t === "command")
      ids.push(rows[i].rowId)
  }
  return ids
}

// Default selection when the user hasn't moved it: the calculator card, else the first row of
// the list (never the weaker fold).
function defaultSelection(rows, hasCalc) {
  if (hasCalc)
    return "calc:"
  for (var i = 0; i < rows.length; i++) {
    var t = rows[i].rowType
    if (t === "row" || t === "app" || t === "clip" || t === "command")
      return rows[i].rowId
  }
  return ""
}

// "2 m ago" for the clip rows: seconds, minutes, hours, days.
function ago(at, now) {
  var s = Math.max(0, Math.floor(now - at))
  if (s < 60)
    return "now"
  if (s < 3600)
    return Math.floor(s / 60) + " m ago"
  if (s < 86400)
    return Math.floor(s / 3600) + " h ago"
  return Math.floor(s / 86400) + " d ago"
}

// Action panel filter: every query character in order (case-insensitive). Labels starting with
// the query come first, then word-start matches, then the rest; ties keep the given order.
function filterActions(actions, query) {
  var q = String(query || "").toLowerCase().replace(/\s+/g, "")
  var list = actions || []
  if (!q.length)
    return list.slice()
  var scored = []
  for (var i = 0; i < list.length; i++) {
    var label = String(list[i].label || "").toLowerCase()
    var at = 0
    var ok = true
    for (var c = 0; c < q.length; c++) {
      at = label.indexOf(q.charAt(c), at)
      if (at < 0) {
        ok = false
        break
      }
      at++
    }
    if (!ok)
      continue
    var rank = label.indexOf(q) === 0 ? 0 : (new RegExp("(^|[^a-z0-9])" + q.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")).test(label) ? 1 : 2)
    scored.push({ rank: rank, i: i })
  }
  scored.sort(function (a, b) {
    return a.rank - b.rank || a.i - b.i
  })
  return scored.map(function (s) {
    return list[s.i]
  })
}
