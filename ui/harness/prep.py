#!/usr/bin/env python3
"""Copies ui/omarchy-plugin/Panel.qml into this harness as Panel.qml (git-ignored) with test hooks
appended, and its layer-shell PanelWindow swapped for a FloatingWindow of the same size: Qt's
offscreen platform has no layer-shell backend. The panel's content is unchanged."""
import pathlib

here = pathlib.Path(__file__).resolve().parent
src = (here.parent / "omarchy-plugin" / "Panel.qml").read_text()
start = src.index("  PanelWindow {")
end = src.index("    MouseArea {", start)
src = src[:start] + """  FloatingWindow {
    id: panel
    visible: root.mapped
    color: "#202428"
    implicitWidth: 1536
    implicitHeight: 960

""" + src[end:]
# qs.Commons does not resolve offscreen; Theme reads the same Omarchy theme files, so the
# screenshots show the live theme's colors.
for a, b in (("Color.foreground", "Theme.foreground"), ("Color.background", "Theme.background"),
             ("Style.cornerRadius", "Theme.cornerRadius"), ("Style.font.body", "Theme.fontBase")):
    src = src.replace(a, b)
# The Island is a layer surface too: offscreen has no backend for it (ui/harness/island/ draws
# IslandView in a plain window instead).
i_start = src.index("  Island {\n")
i_end = src.index("\n  }\n", i_start) + len("\n  }\n")
src = src[:i_start] + src[i_end:]
hooks = """
  function harnessSkipOnboarding() { root.wizardOpen = false; root.configKnown = false; client.configSet({"onboarded": true}) }
  function harnessType(t) { query.text = t; root.onQueryEdited() }
  function harnessTab(id) { if (root.view === "clipboard") chips.pick(id); else tabs.pick(id) }
  function harnessActions() { actions.actions = root.actionsFor(); actions.open() }
  function harnessCall(target, fn, args) { var o = ({"detail": settingDetail, "editor": snippetEditor, "snippet": snippetPane, "proc": procPane})[target] || root; o[fn].apply(o, args) }
  function harnessShot(path) { card.grabToImage(function (r) { r.saveToFile(path) }) }
  function harnessState() {
    return JSON.stringify({view: root.view, phase: root.phase, selection: root.selection, type: root.selectedType,
      rows: root.shownRows().map(function (r) { return r.rowId }), calc: client.calcResult ? client.calcResult.display : null,
      caps: client.capabilities, err: client.lastError, grid: root.gridMode, fromSearch: root.clipFromSearch, query: query.text, tab: tabs.current, codeMode: root.codeMode, pop: root.popToRootSeconds, closedAgo: root.closedAt ? Date.now() / 1000 - root.closedAt : -1,
      drawn: JSON.parse(ipc.debugState()).delegates.map(function (d) { return d.replace(/^\\d+:/, "") }),
      actions: actions.visible ? actions.shown.map(function (a) { return a.id }) : [],
      setting: root.selectedSetting ? root.selectedSetting.id : null, detailFocus: settingDetail.activeFocus, cursor: settingDetail.cursor,
      completions: settingDetail.completions, items: settingDetail.items.map(function (i) { return i.kind + (i.label ? ":" + i.label : "") }), cfg: client.config,
      opened: root.opened, armed: root.armedCommand, procSort: root.procSort, procArmed: root.procArmed, procNotice: root.procNotice,
      accessory: root.selectedRow && root.selectedRow.accessory !== undefined ? root.selectedRow.accessory : null,
      title: root.selectedRow && root.selectedRow.title !== undefined ? root.selectedRow.title : null,
      snippetEditing: snippetPane.editing, island: client.island.map(function (a) { return a.id }),
      desk: root.inDesk ? root.shownRows().map(function (r) { var d = root.deskIndex[r.rowId] || {}; return r.rowId + "|" + (d.title || "") + "|" + (d.accessory || "") }) : []})
  }
"""
i = src.rstrip().rfind("}")
(here / "Panel.qml").write_text(src[:i] + hooks + "}\n")
