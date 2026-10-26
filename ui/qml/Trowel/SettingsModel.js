.pragma library

// The panel's Settings view (the "Trowel Settings" command, DECISIONS D26): every setting as a row
// in the result list, filtered by the query, with its current value as the row's accessory. The
// right pane (SettingDetail.qml) edits the selected one. Pure (no QML types), so
// `scripts/settings-model-test.mjs` runs it under node.
//
// Types: "toggle" (↵ flips it from the list), "choice" (←/→ from the list, or pick in the pane),
// "list" (folders, file types or apps, edited in the pane), "hotkey", "action" (↵ runs it; a
// `danger` one asks for a second ↵) and "info" (read-only lines, with actions in the pane).
// `key` is the config path ("clipboard.max_entries"); entries without one read from `ctx`.

var SECTIONS = ["General", "Folders", "Code", "Launcher", "Clipboard", "Index & Storage"]

function secs(n) {
  return n === 0 ? "Never" : (n < 60 ? n + " s" : (n % 60 === 0 ? n / 60 + " min" : n + " s"))
}

var CATALOG = [{
    id: "hotkey",
    section: "General",
    type: "hotkey",
    key: "hotkey",
    title: "Search Shortcut",
    glyph: "",
    detail: "Opens Trowel from any app. Press ↵, then a key: Super is added to it (Hyprland keeps Super combinations to itself while you record). Add Ctrl, Alt or Shift if you like.",
    keywords: ["hotkey", "keybinding", "keys", "bind", "open"]
  }, {
    id: "autostart",
    section: "General",
    type: "toggle",
    title: "Start at Login",
    glyph: "",
    detail: "Runs the Trowel service with your session, so the shortcut always works.",
    keywords: ["login", "boot", "startup", "autostart", "systemd"]
  }, {
    id: "battery",
    section: "General",
    type: "toggle",
    key: "index_on_battery",
    title: "Index on Battery Power",
    glyph: "",
    detail: "When it's off, a folder's first pass waits for a charger. New files always go in right away.",
    keywords: ["battery", "power", "charger", "laptop"]
  }, {
    id: "motion",
    section: "General",
    type: "toggle",
    key: "ui.reduce_motion",
    title: "Reduce Motion",
    glyph: "",
    detail: "Turns off the panel's animations.",
    keywords: ["animation", "motion", "accessibility"]
  }, {
    id: "roots",
    section: "Folders",
    type: "list",
    mode: "folder",
    key: "roots",
    title: "Search Folders",
    glyph: "",
    detail: "Trowel searches the files in these folders. New and changed files are picked up within seconds; a removed folder's files leave the results.",
    empty: "No folders yet. Add the ones Trowel should search.",
    keywords: ["folders", "directories", "roots", "index", "add", "location"]
  }, {
    id: "excluded_folders",
    section: "Folders",
    type: "list",
    mode: "folder",
    key: "excluded_folders",
    title: "Skipped Folders",
    glyph: "",
    detail: "Folders inside the search folders that Trowel never reads.",
    empty: "Nothing is skipped.",
    keywords: ["exclude", "ignore", "skip", "private"]
  }, {
    id: "excluded_types",
    section: "Folders",
    type: "list",
    mode: "ext",
    key: "excluded_types",
    title: "Skipped File Types",
    glyph: "",
    detail: "Trowel reads pictures, PDFs, documents (txt, md, rtf, docx, odt, html), audio and video. Files with these extensions are left out. Keys, certificates, password files and hidden folders like ~/.ssh are never read.",
    empty: "No file types are skipped.",
    keywords: ["extensions", "exclude", "ignore", "types", "files"]
  }, {
    id: "code_roots",
    section: "Code",
    type: "list",
    mode: "folder",
    key: "code_roots",
    title: "Code Folders",
    glyph: "",
    detail: "Type code: in the panel to search code by what it does, as in “code: retry with backoff”. Code has its own index and never shows up in other searches. Git-ignored files are skipped.",
    empty: "Code search is off. Add a folder to turn it on.",
    keywords: ["code", "projects", "repositories", "git", "source"]
  }, {
    id: "code_excluded",
    section: "Code",
    type: "list",
    mode: "folder",
    key: "code_excluded",
    title: "Skipped Code Folders",
    glyph: "",
    detail: "Folders inside the code folders that Trowel never reads.",
    empty: "Nothing is skipped.",
    keywords: ["code", "exclude", "ignore", "skip", "vendor"]
  }, {
    id: "code_editor",
    section: "Code",
    type: "choice",
    key: "code_editor",
    title: "Open Code In",
    glyph: "",
    detail: "A code result opens there at the line that matched. Space shows the whole file in Quick Look.",
    keywords: ["editor", "ide", "vscode", "cursor", "zed", "neovim", "nvim"],
    // Filled from the editors installed (SettingsState.editors).
    options: [{
        label: "Default app",
        value: ""
      }]
  }, {
    id: "code_quiet",
    section: "Code",
    type: "choice",
    key: "code_quiet_seconds",
    title: "Re-read Code After",
    glyph: "",
    detail: "A changed file is read again once it has been quiet for this long after a save.",
    keywords: ["save", "delay", "quiet", "debounce"],
    options: [1, 2, 3, 5, 10, 30].map(function (n) {
      return {
        label: n + " s",
        value: n
      }
    })
  }, {
    id: "pop",
    section: "Launcher",
    type: "choice",
    key: "ui.pop_to_root_seconds",
    title: "Remember Last Search",
    glyph: "",
    detail: "Reopen the panel within this time and it brings back the view, the search and the selection. After it, the panel opens blank.",
    keywords: ["pop to root", "reopen", "history", "keep", "timeout"],
    // Raycast's choices.
    options: [0, 5, 15, 30, 60, 90, 120, 180].map(function (n) {
      return {
        label: secs(n),
        value: n
      }
    })
  }, {
    id: "hidden",
    section: "Launcher",
    type: "list",
    mode: "app",
    noAdd: true,
    title: "Hidden Apps",
    glyph: "",
    detail: "Apps the launcher leaves out. Hide one with Ctrl+K on it in the panel; remove it here to show it again.",
    empty: "No apps are hidden.",
    keywords: ["hide", "apps", "applications", "launcher", "show"]
  }, {
    id: "capture",
    section: "Clipboard",
    type: "toggle",
    key: "clipboard.capture",
    title: "Clipboard History",
    glyph: "",
    detail: "Keeps the text, links, colors, images and files you copy on this computer, searchable from the Clipboard History command. Copies an app marks as a password are never kept.",
    keywords: ["clipboard", "history", "copy", "capture", "paste"]
  }, {
    id: "max_entries",
    section: "Clipboard",
    type: "choice",
    key: "clipboard.max_entries",
    title: "Keep Up To",
    glyph: "",
    detail: "The oldest items go first once the history is this long. Pinned items are always kept.",
    keywords: ["clipboard", "limit", "items", "size", "entries"],
    options: [100, 250, 500, 1000, 2500, 5000, 10000].map(function (n) {
      return {
        label: n.toLocaleString("en-US") + " items",
        value: n
      }
    })
  }, {
    id: "max_age",
    section: "Clipboard",
    type: "choice",
    key: "clipboard.max_age_days",
    title: "Keep History For",
    glyph: "",
    detail: "Items older than this are removed. Pinned items are always kept.",
    keywords: ["clipboard", "days", "age", "retention", "expire"],
    options: [1, 7, 14, 30, 90, 180, 365].map(function (n) {
      return {
        label: n === 1 ? "1 day" : (n === 7 ? "1 week" : (n === 14 ? "2 weeks" : (n === 30 ? "1 month" : (n === 90 ? "3 months" : (n === 180 ? "6 months" : "1 year"))))),
        value: n
      }
    })
  }, {
    id: "max_blob",
    section: "Clipboard",
    type: "choice",
    key: "clipboard.max_blob_mb",
    title: "Images and Files Use At Most",
    glyph: "",
    detail: "Copied images and files are kept up to this much disk space.",
    keywords: ["clipboard", "disk", "storage", "images", "megabytes"],
    options: [128, 256, 512, 1024, 2048, 4096].map(function (n) {
      return {
        label: n >= 1024 ? n / 1024 + " GB" : n + " MB",
        value: n
      }
    })
  }, {
    id: "clip_excluded",
    section: "Clipboard",
    type: "list",
    mode: "app",
    key: "clipboard.excluded_apps",
    title: "Never Keep Copies From",
    glyph: "",
    detail: "Copies made in these apps are never kept. Use the app's window class.",
    empty: "Every app's copies are kept.",
    keywords: ["clipboard", "exclude", "password", "private", "apps"]
  }, {
    id: "clip_import",
    section: "Clipboard",
    type: "action",
    title: "Import Omarchy's Clipboard History",
    glyph: "",
    detail: "Copies what Omarchy's clipboard (cliphist) kept into Trowel's history.",
    keywords: ["clipboard", "import", "cliphist", "omarchy"],
    actions: [{
        id: "clip_import",
        label: "Import"
      }]
  }, {
    id: "clip_clear",
    section: "Clipboard",
    type: "action",
    title: "Clear Clipboard History",
    glyph: "",
    detail: "Removes everything that isn't pinned.",
    keywords: ["clipboard", "clear", "delete", "remove", "wipe"],
    actions: [{
        id: "clip_clear",
        label: "Clear History",
        danger: true
      }]
  }, {
    id: "index",
    section: "Index & Storage",
    type: "info",
    title: "Index",
    glyph: "",
    detail: "What Trowel has read so far, on this computer.",
    keywords: ["index", "status", "files", "count", "progress", "storage"],
    actions: [{
        id: "open_index",
        label: "Open Index Folder"
      }]
  }, {
    id: "index_speed",
    section: "Index & Storage",
    type: "choice",
    key: "index_speed",
    title: "Indexing Speed",
    glyph: "\uf0e4",
    detail: "How hard indexing may work while you use the computer. Gentle: about a third of the graphics chip and one efficiency core, so a big import never slows anything else down; it just takes longer. Balanced: about half, two efficiency cores. Full speed: everything the machine has, fans and all. A single new file is always picked up right away.",
    keywords: ["speed", "cpu", "gpu", "performance", "battery", "resources", "throttle", "slow", "fast", "fan"],
    options: [{
        label: "Gentle",
        value: "gentle"
      }, {
        label: "Balanced",
        value: "balanced"
      }, {
        label: "Full speed",
        value: "full"
      }]
  }, {
    id: "pause",
    section: "Index & Storage",
    type: "toggle",
    title: "Pause Indexing",
    glyph: "",
    detail: "Search keeps working with what is already indexed.",
    keywords: ["pause", "resume", "stop", "index"]
  }, {
    id: "model",
    section: "Index & Storage",
    type: "info",
    title: "Model",
    glyph: "",
    detail: "EmbeddingGemma 2 by Google (Apache 2.0, 865 MB). It runs on this computer; Trowel goes online only to download it, once.",
    keywords: ["model", "embedding", "gemma", "download", "ai"],
    actions: [{
        id: "open_models",
        label: "Open Model Folder"
      }]
  }, {
    id: "health",
    section: "Index & Storage",
    type: "info",
    title: "Health Checks",
    glyph: "",
    detail: "Why isn't it working? The same checks as trowel doctor.",
    keywords: ["doctor", "health", "diagnose", "problem", "broken", "check"],
    actions: [{
        id: "doctor",
        label: "Run Checks Again"
      }]
  }]

function byId(id) {
  for (var i = 0; i < CATALOG.length; i++)
    if (CATALOG[i].id === id)
      return CATALOG[i]
  return null
}

function words(text) {
  return String(text || "").toLowerCase().split(/[^a-z0-9]+/).filter(function (w) {
    return w.length > 0
  })
}

// Every query word must start a word of the title, the section or a keyword.
function matches(entry, query) {
  var q = words(query)
  if (!q.length)
    return true
  var hay = words(entry.title + " " + entry.section + " " + (entry.keywords || []).join(" "))
  return q.every(function (w) {
    return hay.some(function (h) {
      return h.indexOf(w) === 0
    })
  })
}

// List rows for the query: a header per section, then its settings, in catalog order.
function rows(query) {
  var out = []
  for (var s = 0; s < SECTIONS.length; s++) {
    var hits = CATALOG.filter(function (e) {
      return e.section === SECTIONS[s] && matches(e, query)
    })
    if (!hits.length)
      continue
    out.push({
      rowType: "header",
      rowId: "hdr:" + SECTIONS[s],
      title: SECTIONS[s]
    })
    for (var i = 0; i < hits.length; i++)
      out.push({
        rowType: "setting",
        rowId: "set:" + hits[i].id,
        title: hits[i].title
      })
  }
  return out
}

function get(obj, path) {
  var parts = String(path).split(".")
  var v = obj
  for (var i = 0; i < parts.length; i++) {
    if (v === null || v === undefined)
      return undefined
    v = v[parts[i]]
  }
  return v
}

// {"clipboard.max_entries": 500} → {clipboard: {max_entries: 500}} for config_set.
function patchFor(path, value) {
  var parts = String(path).split(".")
  var out = {}
  var at = out
  for (var i = 0; i < parts.length - 1; i++) {
    at[parts[i]] = {}
    at = at[parts[i]]
  }
  at[parts[parts.length - 1]] = value
  return out
}

var DEFAULTS = {
  "clipboard.capture": true,
  "clipboard.max_entries": 1000,
  "clipboard.max_age_days": 30,
  "clipboard.max_blob_mb": 512,
  "ui.pop_to_root_seconds": 90,
  "ui.reduce_motion": false,
  "code_quiet_seconds": 2,
  "index_speed": "gentle",
  "code_editor": ""
}

// The current value. ctx: {cfg, autostart, paused, hidden}.
function value(entry, ctx) {
  if (!entry)
    return undefined
  var c = ctx || {}
  if (entry.id === "autostart")
    return c.autostart === true
  if (entry.id === "pause")
    return c.paused === true
  if (entry.id === "hidden")
    return c.hidden || []
  if (!entry.key)
    return undefined
  var v = get(c.cfg || {}, entry.key)
  if (v === undefined || v === null)
    v = DEFAULTS[entry.key]
  if (entry.type === "list")
    return v || []
  return v
}

// Choices, with the editors found and the current value when it isn't one of them.
function optionsFor(entry, ctx) {
  var opts = (entry.options || []).slice()
  if (entry.id === "code_editor")
    opts = opts.concat(((ctx || {}).editors || []).map(function (e) {
      return {
        label: e,
        value: e
      }
    }))
  var cur = value(entry, ctx)
  var known = opts.some(function (o) {
    return o.value === cur
  })
  if (!known && cur !== undefined && cur !== null && cur !== "") {
    opts.push({
      label: String(cur) + (entry.id === "code_editor" ? "" : " (custom)"),
      value: cur
    })
    if (typeof cur === "number")
      opts.sort(function (a, b) {
        return a.value - b.value
      })
  }
  return opts
}

// The next or previous choice (←/→), clamped at the ends.
function stepChoice(entry, ctx, step) {
  var opts = optionsFor(entry, ctx)
  if (!opts.length)
    return undefined
  var cur = value(entry, ctx)
  var i = -1
  for (var k = 0; k < opts.length; k++)
    if (opts[k].value === cur)
      i = k
  var n = Math.max(0, Math.min(opts.length - 1, (i < 0 ? 0 : i) + step))
  return opts[n].value
}

function num(n) {
  return Number(n || 0).toLocaleString("en-US")
}

// The short value at the right of a row. ctx adds {status, model, checks}.
function accessory(entry, ctx) {
  var c = ctx || {}
  var v = value(entry, c)
  switch (entry.type) {
  case "toggle":
    return v ? "On" : "Off"
  case "hotkey":
    return String(v || "")
  case "choice":
    var opts = optionsFor(entry, c)
    for (var i = 0; i < opts.length; i++)
      if (opts[i].value === v)
        return opts[i].label
    return String(v)
  case "list":
    var n = (v || []).length
    if (entry.mode === "folder")
      return n === 0 ? "None" : (n === 1 ? "1 folder" : n + " folders")
    if (entry.mode === "ext")
      return n === 0 ? "None" : (n <= 3 ? v.map(function (x) {
        return "." + x
      }).join(" ") : n + " types")
    return n === 0 ? "None" : (n === 1 ? "1 app" : n + " apps")
  case "info":
    if (entry.id === "index")
      return indexState(c.status)
    if (entry.id === "model") {
      var m = c.model || {}
      if (m.downloading && m.total)
        return Math.round(100 * m.bytes / m.total) + "%"
      return m.present ? "Installed" : (m.present === false ? "Not downloaded" : "")
    }
    if (entry.id === "health") {
      var bad = (c.checks || []).filter(function (x) {
        return x.status !== "OK"
      }).length
      return !(c.checks || []).length ? "" : (bad === 0 ? "All OK" : (bad === 1 ? "1 problem" : bad + " problems"))
    }
    return ""
  default:
    return ""
  }
}

function indexState(status) {
  var s = status || {}
  if (!s.phase)
    return ""
  if (s.phase === "paused")
    return "Paused"
  var m = s.main || {}
  if (m.total > 0 && m.done < m.total)
    return "Indexing " + Math.floor(100 * m.done / m.total) + "%"
  return "Up to date"
}

// The pane's read-only lines for an info entry: [{label, value, level}].
function infoLines(entry, ctx) {
  var c = ctx || {}
  if (entry.id === "index") {
    var counts = (c.status || {}).counts || {}
    var names = [["screenshots", "Screenshots"], ["images", "Images"], ["documents", "Documents"], ["video", "Videos"], ["audio", "Audio"], ["code", "Code files"]]
    var out = [{
        label: "Status",
        value: indexState(c.status) || "…"
      }]
    for (var i = 0; i < names.length; i++)
      out.push({
        label: names[i][1],
        value: num(counts[names[i][0]])
      })
    return out
  }
  if (entry.id === "model") {
    var m = c.model || {}
    return [{
        label: "Status",
        value: m.downloading ? "Downloading" : (m.present ? (m.verified ? "Installed and verified" : "Installed") : "Not downloaded"),
        level: m.present ? "OK" : "WARN"
      }, {
        label: "Size",
        value: m.total ? Math.round(m.total / 1e6) + " MB" : "865 MB"
      }]
  }
  if (entry.id === "health")
    return (c.checks || []).map(function (x) {
      return {
        label: x.name.charAt(0).toUpperCase() + x.name.slice(1),
        value: x.detail || "",
        level: x.status
      }
    })
  return []
}

// The pane's buttons: an info entry's own, plus "Download" for a missing model.
function actionsFor(entry, ctx) {
  var acts = (entry.actions || []).slice()
  if (entry.id === "model" && ((ctx || {}).model || {}).present === false)
    acts.unshift({
      id: "model_download",
      label: "Download Model"
    })
  return acts
}

// What ↵ does on the row, from the list: "toggle", "action" (an action or info entry with a
// single button runs it), or "edit" (focus the pane).
function primary(entry, ctx) {
  if (entry.type === "toggle")
    return "toggle"
  if ((entry.type === "action" || entry.type === "info") && actionsFor(entry, ctx).length === 1)
    return "action"
  return "edit"
}
