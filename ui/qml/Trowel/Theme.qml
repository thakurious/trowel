pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

// Omarchy token bridge. trowel-app and the gallery cannot import qs.Commons,
// so this singleton re-reads colors.toml + shell.toml with FileView watch.
// UI-DESIGN §2. Inside omarchy-shell, Color/Style are the live source too;
// these files are the same ones the shell loads, so a theme switch recolors both.
QtObject {
  id: root

  readonly property string home: Quickshell.env("HOME")
  readonly property string stateHome: home + "/.local/state"
  readonly property string currentThemePath: stateHome + "/omarchy/current/theme"

  property color foreground: "#cacccc"
  property color background: "#101315"
  property color accent: "#cacccc"
  property color urgent: "#a55555"
  property color muted: "#707880"

  property int cornerRadius: 12
  property int fontBase: 12
  property string fontFamily: "sans-serif"
  property string monoFamily: "monospace"
  property real selectedFillAlpha: 0.18
  property var shellValues: ({})

  readonly property color panelFill: Qt.rgba(background.r, background.g, background.b, 0.75)
  readonly property color secondary: Qt.rgba(foreground.r, foreground.g, foreground.b, 0.62)
  readonly property color hairline: Qt.rgba(foreground.r, foreground.g, foreground.b, 0.14)
  readonly property color keycapFill: Qt.rgba(foreground.r, foreground.g, foreground.b, 0.07)
  readonly property color selectedFill: Qt.rgba(accent.r, accent.g, accent.b, selectedFillAlpha)
  readonly property color brandAmber: "#F2A22A"
  readonly property color brandEmber: "#D97B1A"
  readonly property color brandMoss: "#356652"
  readonly property color brandLeaf: "#4F8058"
  readonly property color brandSprout: "#A2C78E"

  readonly property int querySize: Math.round(fontBase * 1.67)
  readonly property int queryCodeSize: Math.round(fontBase * 1.42)
  readonly property int rowTitleSize: Math.round(fontBase * 1.08)
  readonly property int captionSize: Math.round(fontBase * 0.92)
  readonly property int groupSize: Math.round(fontBase * 0.83)
  readonly property int emptyTitleSize: Math.round(fontBase * 1.25)
  readonly property int emptyIconSize: Math.round(fontBase * 2.3)

  function alpha(c, a) {
    return Qt.rgba(c.r, c.g, c.b, a)
  }

  function innerRadius(outer) {
    var r = Math.max(Number(outer), 0)
    return Math.round(r * 0.45)
  }

  function panelRadius() {
    return Math.max(cornerRadius, 0)
  }

  function hairlineWidth() {
    var dpr = 1
    if (typeof Screen !== "undefined" && Screen.devicePixelRatio)
      dpr = Screen.devicePixelRatio
    return 1 / dpr
  }

  function loadColors(raw) {
    var lines = String(raw || "").split("\n")
    var foundAccent = false
    var foundMuted = false
    var loadedForeground = false
    var loadedBackground = false
    var color0Value = ""
    var color4Value = ""
    var color7Value = ""
    var color8Value = ""
    for (var i = 0; i < lines.length; i++) {
      var match = lines[i].match(/^\s*([A-Za-z0-9_-]+)\s*=\s*["']?(#[0-9A-Fa-f]{6})/)
      if (!match)
        continue
      if (match[1] === "foreground") {
        foreground = match[2]
        loadedForeground = true
      } else if (match[1] === "background") {
        background = match[2]
        loadedBackground = true
      } else if (match[1] === "accent") {
        accent = match[2]
        foundAccent = true
      } else if (match[1] === "muted") {
        muted = match[2]
        foundMuted = true
      } else if (match[1] === "color0")
        color0Value = match[2]
      else if (match[1] === "color4")
        color4Value = match[2]
      else if (match[1] === "color7")
        color7Value = match[2]
      else if (match[1] === "color8")
        color8Value = match[2]
      else if (match[1] === "red" || match[1] === "color1")
        urgent = match[2]
    }
    if (!loadedBackground && color0Value.length > 0)
      background = color0Value
    if (!loadedForeground && color7Value.length > 0)
      foreground = color7Value
    if (!foundAccent && color4Value.length > 0)
      accent = color4Value
    if (!foundMuted)
      muted = color8Value.length > 0 ? color8Value : foreground
  }

  function parseShell(raw) {
    var parsed = {}
    var text = String(raw || "")
    if (!text)
      return parsed
    var lines = text.split("\n")
    var section = ""
    for (var i = 0; i < lines.length; i++) {
      var line = lines[i].replace(/^\s+|\s+$/g, "")
      if (!line || line.charAt(0) === "#")
        continue
      var sectionMatch = line.match(/^\[([A-Za-z0-9_-]+)\]\s*(#.*)?$/)
      if (sectionMatch) {
        section = sectionMatch[1]
        continue
      }
      var stringKv = line.match(/^([A-Za-z0-9_-]+)\s*=\s*["']([^"']+)["']\s*(#.*)?$/)
      var numKv = line.match(/^([A-Za-z0-9_-]+)\s*=\s*(-?\d+(?:\.\d+)?)\s*(#.*)?$/)
      var kv = stringKv || numKv
      if (!kv || !section)
        continue
      parsed[section + "." + kv[1]] = kv[2]
    }
    return parsed
  }

  function applyShell(raw) {
    var v = parseShell(raw)
    shellValues = v
    var base = Number(v["font.base-size"])
    if (isFinite(base) && base > 0)
      fontBase = Math.round(base)
    if (typeof v["font.family"] === "string" && v["font.family"].length)
      fontFamily = v["font.family"]
    if (typeof v["font.mono"] === "string" && v["font.mono"].length)
      monoFamily = v["font.mono"]
    var alpha = Number(v["style.selected-fill-alpha"])
    if (isFinite(alpha) && alpha >= 0)
      selectedFillAlpha = alpha
  }

  property FileView colorsFile: FileView {
    path: root.currentThemePath + "/colors.toml"
    watchChanges: true
    printErrors: false
    onLoaded: root.loadColors(text())
    onFileChanged: reload()
  }

  property FileView shellFile: FileView {
    path: root.currentThemePath + "/shell.toml"
    watchChanges: true
    printErrors: false
    onLoaded: root.applyShell(text())
    onFileChanged: reload()
    onLoadFailed: root.applyShell("")
  }

  property FileView userShellFile: FileView {
    path: root.home + "/.config/omarchy/shell.toml"
    watchChanges: true
    printErrors: false
    onLoaded: root.applyShell((root.shellFile.text() || "") + "\n" + text())
    onFileChanged: reload()
    onLoadFailed: {}
  }
}
