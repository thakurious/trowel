// Unit tests for ui/qml/Trowel/SettingsModel.js (a QML `.pragma library` file) under node:
//   node scripts/settings-model-test.mjs
import { readFileSync } from "node:fs";
import vm from "node:vm";
import assert from "node:assert/strict";

const src = readFileSync(new URL("../ui/qml/Trowel/SettingsModel.js", import.meta.url), "utf8")
  .replace(/^\.pragma library\s*$/m, "");
const M = {};
vm.runInNewContext(src + "\nthis.M = { CATALOG, SECTIONS, byId, rows, value, patchFor, optionsFor, stepChoice, accessory, infoLines, actionsFor, primary };", M);
const S = M.M;

// Arrays from the vm context have another realm's prototype: copy into this one.
const plain = (x) => JSON.parse(JSON.stringify(x));
let n = 0;
const test = (name, f) => {
  try {
    f();
    n++;
  } catch (e) {
    console.error("FAIL", name);
    throw e;
  }
};

const cfg = {
  hotkey: "SUPER + D",
  index_on_battery: true,
  roots: ["/home/u/Documents", "/home/u/Pictures"],
  excluded_types: ["log"],
  code_roots: [],
  code_editor: "",
  code_quiet_seconds: 2,
  clipboard: { capture: true, max_entries: 1000, max_age_days: 30, max_blob_mb: 512, excluded_apps: ["keepassxc"] },
  ui: { pop_to_root_seconds: 90 },
};

test("ids are unique and every section has a setting", () => {
  const ids = S.CATALOG.map((e) => e.id);
  assert.equal(new Set(ids).size, ids.length);
  for (const s of S.SECTIONS) assert.ok(S.CATALOG.some((e) => e.section === s), s);
});

test("empty query lists every setting under its section header", () => {
  const rows = plain(S.rows(""));
  assert.equal(rows.filter((r) => r.rowType === "setting").length, S.CATALOG.length);
  assert.equal(rows[0].rowId, "hdr:General");
  assert.equal(rows[1].rowId, "set:hotkey");
});

test("query matches title, section and keyword prefixes; empty sections drop out", () => {
  const ids = (q) => plain(S.rows(q)).map((r) => r.rowId);
  assert.deepEqual(ids("batt"), ["hdr:General", "set:battery"]);
  assert.deepEqual(ids("nvim"), ["hdr:Code", "set:code_editor"]);
  assert.ok(ids("clipboard").includes("set:max_entries"));
  assert.ok(ids("clipboard").every((id) => id === "hdr:Clipboard" || id.startsWith("set:")));
  // Every word must match.
  assert.deepEqual(ids("skip code"), ["hdr:Code", "set:code_excluded"]);
  assert.deepEqual(ids("zzz"), []);
});

test("patchFor nests dotted keys for config_set", () => {
  assert.deepEqual(plain(S.patchFor("clipboard.max_entries", 500)), { clipboard: { max_entries: 500 } });
  assert.deepEqual(plain(S.patchFor("roots", ["/a"])), { roots: ["/a"] });
});

test("values come from the config, ctx, or the defaults", () => {
  const ctx = { cfg, autostart: true, paused: false, hidden: ["steam"] };
  assert.equal(S.value(S.byId("battery"), ctx), true);
  assert.equal(S.value(S.byId("autostart"), ctx), true);
  assert.equal(S.value(S.byId("max_entries"), ctx), 1000);
  assert.deepEqual(plain(S.value(S.byId("hidden"), ctx)), ["steam"]);
  // ui.reduce_motion is missing from cfg: the default.
  assert.equal(S.value(S.byId("motion"), ctx), false);
  assert.deepEqual(plain(S.value(S.byId("excluded_folders"), ctx)), []);
});

test("accessories: toggles, choices, lists, info", () => {
  const ctx = { cfg, autostart: false, status: { phase: "idle", main: { done: 0, total: 0 } }, model: { present: true }, checks: [{ name: "gpu", status: "OK" }, { name: "disk", status: "WARN" }] };
  assert.equal(S.accessory(S.byId("battery"), ctx), "On");
  assert.equal(S.accessory(S.byId("autostart"), ctx), "Off");
  assert.equal(S.accessory(S.byId("pop"), ctx), "90 s");
  assert.equal(S.accessory(S.byId("max_entries"), ctx), "1,000 items");
  assert.equal(S.accessory(S.byId("roots"), ctx), "2 folders");
  assert.equal(S.accessory(S.byId("excluded_types"), ctx), ".log");
  assert.equal(S.accessory(S.byId("clip_excluded"), ctx), "1 app");
  assert.equal(S.accessory(S.byId("index"), ctx), "Up to date");
  assert.equal(S.accessory(S.byId("model"), ctx), "Installed");
  assert.equal(S.accessory(S.byId("health"), ctx), "1 problem");
  assert.equal(S.accessory(S.byId("index"), { status: { phase: "indexing", main: { done: 25, total: 100 } } }), "Indexing 25%");
});

test("a value that isn't a choice is kept as a custom one, in order", () => {
  const ctx = { cfg: { ...cfg, clipboard: { ...cfg.clipboard, max_entries: 750 } } };
  const opts = plain(S.optionsFor(S.byId("max_entries"), ctx)).map((o) => o.value);
  assert.deepEqual(opts, [100, 250, 500, 750, 1000, 2500, 5000, 10000]);
  assert.equal(S.accessory(S.byId("max_entries"), ctx), "750 (custom)");
});

test("editors found on the machine become choices", () => {
  const ctx = { cfg: { ...cfg, code_editor: "zed" }, editors: ["code", "zed"] };
  const opts = plain(S.optionsFor(S.byId("code_editor"), ctx));
  assert.deepEqual(opts.map((o) => o.value), ["", "code", "zed"]);
  assert.equal(S.accessory(S.byId("code_editor"), ctx), "zed");
});

test("←/→ step through the choices and stop at the ends", () => {
  const ctx = { cfg };
  const pop = S.byId("pop");
  assert.equal(S.stepChoice(pop, ctx, 1), 120);
  assert.equal(S.stepChoice(pop, ctx, -1), 60);
  assert.equal(S.stepChoice(pop, { cfg: { ui: { pop_to_root_seconds: 180 } } }, 1), 180);
  assert.equal(S.stepChoice(pop, { cfg: { ui: { pop_to_root_seconds: 0 } } }, -1), 0);
});

test("↵ from the list: toggles flip, single actions run, the rest edit", () => {
  assert.equal(S.primary(S.byId("battery"), {}), "toggle");
  assert.equal(S.primary(S.byId("clip_clear"), {}), "action");
  assert.equal(S.primary(S.byId("roots"), {}), "edit");
  assert.equal(S.primary(S.byId("pop"), {}), "edit");
  assert.equal(S.primary(S.byId("health"), {}), "action");
});

test("a missing model offers Download first", () => {
  const acts = plain(S.actionsFor(S.byId("model"), { model: { present: false } }));
  assert.deepEqual(acts.map((a) => a.id), ["model_download", "open_models"]);
  assert.equal(S.primary(S.byId("model"), { model: { present: false } }), "edit");
});

test("info lines: index counts and health checks", () => {
  const lines = plain(S.infoLines(S.byId("index"), { status: { phase: "idle", counts: { images: 1234, code: 5 } } }));
  assert.deepEqual(lines.find((l) => l.label === "Images"), { label: "Images", value: "1,234" });
  const health = plain(S.infoLines(S.byId("health"), { checks: [{ name: "gpu", detail: "GPU0", status: "OK" }] }));
  assert.deepEqual(health, [{ label: "Gpu", value: "GPU0", level: "OK" }]);
});

console.log(`settings-model: ${n} tests passed`);
