.pragma library

// LAUNCHER-PLAN.md L4 "Merged list model": the panel's rows from the file groups and the
// launcher sources. Pure (no QML types), so `scripts/listmerge-test.mjs` runs it under node.
// It concatenates and never re-ranks: ranking stays in the daemon.

// Omarchy's `AppSearch.js` name-prefix band: the query is a prefix of the app's name.
function strongApp(app, query) {
  var q = String(query || "").replace(/^\s+|\s+$/g, "").toLowerCase()
  if (!q.length || !app)
    return false
  return String(app.name || "").toLowerCase().indexOf(q) === 0
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
// apps: AppRow[] for the same query. opts.maxApps caps the section (default 5).
// A strong app match puts Applications first, so `fire⏎` launches Firefox; otherwise the
// section follows the file groups (before the weaker fold), so row 0 stays the best file.
function merge(files, apps, query, opts) {
  opts = opts || {}
  var max = opts.maxApps === undefined ? 5 : opts.maxApps
  var list = (apps || []).slice(0, max)
  var out = []
  if (!list.length)
    return (files || []).slice()
  var section = [{
    rowType: "header",
    rowId: "hdr:apps",
    title: "Applications",
    kind: "apps",
    count: list.length
  }]
  for (var i = 0; i < list.length; i++)
    section.push(appItem(list[i]))
  if (strongApp(list[0], query))
    return section.concat(files || [])
  var rest = files || []
  var fold = -1
  for (var j = 0; j < rest.length; j++) {
    if (rest[j].rowType === "fold") {
      fold = j
      break
    }
  }
  if (fold < 0)
    return rest.concat(section)
  return rest.slice(0, fold).concat(section, rest.slice(fold))
}

// Clipboard view rows: pinned first, as the daemon returns them; a "Pinned" header when any.
function clipRows(clips) {
  var out = []
  var cs = clips || []
  var pinned = 0
  for (var i = 0; i < cs.length; i++)
    if (cs[i].pinned)
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
    if (t === "row" || t === "fold" || t === "app" || t === "clip")
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
    if (t === "row" || t === "app" || t === "clip")
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
