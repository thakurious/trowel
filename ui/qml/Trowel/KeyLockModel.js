.pragma library

// The Lock Keyboard command's list (Raycast's "Lock Keyboard" picks a duration the same way): the
// preset durations, and a custom one when the query reads as a duration ("45", "90s", "3m",
// "1.5 min"). `trowel keyboard lock` caps a lock at 30 minutes. Pure, so
// `scripts/keylock-model-test.mjs` runs it under node.

var MAX_SECONDS = 30 * 60
var PRESETS = [15, 30, 60, 120, 300, 600]

function label(secs) {
  if (secs < 60)
    return secs + (secs === 1 ? " second" : " seconds")
  var m = Math.floor(secs / 60)
  var s = secs % 60
  var mins = m + (m === 1 ? " minute" : " minutes")
  return s ? mins + " " + s + " s" : mins
}

// "0:42" for the countdown.
function clock(secs) {
  var s = Math.max(0, Math.ceil(secs))
  var m = Math.floor(s / 60)
  var r = s % 60
  return m + ":" + (r < 10 ? "0" : "") + r
}

// Seconds for a typed duration, or 0. A bare number is seconds.
function parse(text) {
  var t = String(text || "").toLowerCase().replace(/\s+/g, "")
  var m = t.match(/^(\d+(?:\.\d+)?)(s|sec|secs|second|seconds|m|min|mins|minute|minutes)?$/)
  if (!m)
    return 0
  var n = parseFloat(m[1])
  var unit = m[2] || "s"
  var secs = Math.round(unit.charAt(0) === "m" ? n * 60 : n)
  return secs >= 1 && secs <= MAX_SECONDS ? secs : 0
}

function row(secs, custom) {
  return {
    rowType: "duration",
    rowId: "lock:" + secs,
    title: label(secs),
    seconds: secs,
    custom: !!custom
  }
}

// Presets, filtered by the query's words; a typed duration that isn't a preset leads.
function rows(query) {
  var q = String(query || "").replace(/^\s+|\s+$/g, "").toLowerCase()
  var out = []
  var typed = parse(q)
  if (typed && PRESETS.indexOf(typed) < 0)
    out.push(row(typed, true))
  for (var i = 0; i < PRESETS.length; i++) {
    var r = row(PRESETS[i], false)
    if (!q.length || typed === PRESETS[i] || r.title.toLowerCase().indexOf(q) === 0)
      out.push(r)
  }
  return out
}
