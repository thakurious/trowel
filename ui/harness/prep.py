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
hooks = """
  function harnessSkipOnboarding() { root.wizardOpen = false; root.configKnown = false }
  function harnessType(t) { query.text = t; root.onQueryEdited() }
  function harnessTab(id) { if (root.view === "clipboard") chips.pick(id); else tabs.pick(id) }
  function harnessActions() { actions.actions = root.actionsFor(); actions.open() }
  function harnessShot(path) { card.grabToImage(function (r) { r.saveToFile(path) }) }
  function harnessState() {
    return JSON.stringify({view: root.view, phase: root.phase, selection: root.selection, type: root.selectedType,
      rows: root.shownRows().map(function (r) { return r.rowId }), calc: client.calcResult ? client.calcResult.display : null,
      caps: client.capabilities, err: client.lastError, grid: root.gridMode,
      actions: actions.visible ? actions.shown.map(function (a) { return a.id }) : []})
  }
"""
i = src.rstrip().rfind("}")
(here / "Panel.qml").write_text(src[:i] + hooks + "}\n")
