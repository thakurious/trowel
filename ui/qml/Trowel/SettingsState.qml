import QtQuick
import Quickshell
import Quickshell.Io
import "SettingsModel.js" as SettingsModel

// What the Settings view shows and how it writes (DECISIONS D26): the daemon's config, status,
// model and health checks, plus what only the desktop knows (autostart, installed editors, apps
// open now, hidden apps). `set`/`toggle`/`run` apply a change and say so through `toast`.
Item {
  id: root
  required property var client
  property bool autostart: false
  property var model: ({})
  property var checks: []
  property var editors: []
  property var runningApps: []
  property var statusInfo: ({})
  readonly property string home: Quickshell.env("HOME") || ""

  readonly property var status: (client.status && client.status.phase) ? client.status : root.statusInfo
  readonly property string hiddenPath: {
    var base = Quickshell.env("XDG_CONFIG_HOME")
    if (!base || !base.length)
      base = root.home + "/.config"
    return base + "/trowel/launcher.hides"
  }
  readonly property var hidden: String(hides.text() || "").split("\n").map(function (l) {
    return l.replace(/^\s+|\s+$/g, "").replace(/\.desktop$/, "")
  }).filter(function (l) {
    return l.length > 0
  })
  // Everything SettingsModel reads.
  readonly property var ctx: ({
      "cfg": root.client.config || ({}),
      "autostart": root.autostart,
      "paused": root.status.phase === "paused",
      "hidden": root.hidden,
      "status": root.status,
      "model": root.model,
      "checks": root.checks,
      "editors": root.editors
    })

  signal toast(string text)

  function refresh() {
    client.configGet()
    client.sendRouted("status", {})
    client.sendRouted("model_status", {})
    client.sendRouted("doctor", {})
    autostartProbe.running = true
    editorProbe.running = true
    clients.running = true
    hides.reload()
  }

  function value(entry) {
    return SettingsModel.value(entry, root.ctx)
  }

  function label(entry, v) {
    var opts = SettingsModel.optionsFor(entry, root.ctx)
    for (var i = 0; i < opts.length; i++)
      if (opts[i].value === v)
        return opts[i].label
    return String(v)
  }

  function set(entry, v) {
    if (entry.id === "autostart") {
      root.autostart = v
      autostart.command = ["trowel", "autostart", v ? "on" : "off"]
      autostart.running = true
    } else if (entry.id === "pause") {
      if (v)
        client.pause()
      else
        client.resume()
      client.sendRouted("status", {})
    } else if (entry.id === "hidden") {
      for (var i = 0; i < root.hidden.length; i++)
        if (v.indexOf(root.hidden[i]) < 0)
          client.appHide(root.hidden[i], false)
      return
    } else if (entry.key) {
      client.configSet(SettingsModel.patchFor(entry.key, v))
    }
    if (entry.type === "toggle")
      root.toast(entry.title + (v ? " on" : " off"))
    else if (entry.type === "choice")
      root.toast(entry.title + ": " + root.label(entry, v))
  }

  function toggle(entry) {
    root.set(entry, !root.value(entry))
  }

  function setHotkey(chord) {
    client.sendRouted("set_hotkey", {
      "chord": chord
    })
  }

  function run(id) {
    if (id === "clip_import")
      client.clipImport()
    else if (id === "clip_clear")
      client.clipClear(true)
    else if (id === "open_index")
      Quickshell.execDetached(["xdg-open", root.expand((client.config || {}).index_dir)])
    else if (id === "open_models")
      Quickshell.execDetached(["xdg-open", root.expand((client.config || {}).models_dir)])
    else if (id === "model_download") {
      client.modelDownload("start")
      root.toast("Downloading the model")
    } else if (id === "doctor") {
      root.checks = []
      client.sendRouted("doctor", {})
    }
  }

  function expand(p) {
    var s = String(p || "")
    if (s === "~")
      return root.home
    return s.indexOf("~/") === 0 ? root.home + s.slice(1) : s
  }

  function pretty(p) {
    var s = String(p || "")
    if (root.home.length && s.indexOf(root.home) === 0)
      return "~" + s.slice(root.home.length)
    return s
  }

  Connections {
    target: root.client
    function onOpDone(op, msg) {
      if (op === "status")
        root.statusInfo = msg
      else if (op === "model_status")
        root.model = msg
      else if (op === "doctor") {
        root.checks = msg.checks || []
      } else if (op === "set_hotkey") {
        root.client.configGet()
        root.toast(msg.conflict ? "Saved, but the keys are " + msg.conflict : "Shortcut saved")
      } else if (op === "clip_import")
        root.toast("Imported " + msg.imported + (msg.skipped ? ", skipped " + msg.skipped : ""))
      else if (op === "clip_clear")
        root.toast("Removed " + msg.removed + " items")
      else if (op === "app_hide")
        hides.reload()
    }
    function onOpFailed(op, error) {
      if (op === "set_hotkey" || op === "doctor" || op === "status" || op === "model_status")
        root.toast((error && error.message) ? error.message : "Couldn't " + op.replace("_", " "))
    }
  }

  FileView {
    id: hides
    path: root.hiddenPath
    watchChanges: true
    printErrors: false
    onFileChanged: reload()
  }

  Process {
    id: autostartProbe
    command: ["trowel", "autostart", "status"]
    stdout: StdioCollector {
      onStreamFinished: root.autostart = String(text).indexOf("enabled") === 0
    }
  }
  Process {
    id: autostart
    onExited: autostartProbe.running = true
  }
  Process {
    id: editorProbe
    command: ["bash", "-c", "for e in cursor code codium zed nvim hx kate; do command -v \"$e\" >/dev/null && echo \"$e\"; done"]
    stdout: StdioCollector {
      onStreamFinished: root.editors = String(text).split("\n").filter(function (l) {
        return l.length > 0
      })
    }
  }
  Process {
    id: clients
    command: ["hyprctl", "-j", "clients"]
    stdout: StdioCollector {
      onStreamFinished: {
        try {
          var seen = ({})
          var out = []
          var list = JSON.parse(text)
          for (var i = 0; i < list.length; i++) {
            var c = String(list[i]["class"] || "")
            if (c.length && !seen[c.toLowerCase()]) {
              seen[c.toLowerCase()] = true
              out.push(c)
            }
          }
          root.runningApps = out.sort()
        } catch (e) {
          root.runningApps = []
        }
      }
    }
  }
}
