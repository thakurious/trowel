// Unit tests for ui/qml/Trowel/DeskModel.js (Phase 9 views) under node:
//   node scripts/deskmodel-test.mjs
import { readFileSync } from "node:fs";
import vm from "node:vm";
import assert from "node:assert/strict";

const src = readFileSync(new URL("../ui/qml/Trowel/DeskModel.js", import.meta.url), "utf8")
  .replace(/^\.pragma library\s*$/m, "");
const M = {};
vm.runInNewContext(src + "\nthis.M = { clock, duration, bytes, rate, pct, systemCommands, stateCommands, timerCommand, keywordHits, timerRows, metric, procRows, scriptRows, themeRows, snippetRows, parts, SORTS, menuSearch, menuRows, menuCommands, menuTitle, menuScore };", M);
const D = M.M;
const plain = (v) => JSON.parse(JSON.stringify(v));
let n = 0;
const test = (name, f) => { f(); n++; };

test("formatting", () => {
  assert.equal(D.clock(0.2), "0:01");
  assert.equal(D.clock(65), "1:05");
  assert.equal(D.clock(3725), "1:02:05");
  assert.equal(D.duration(5400), "1 h 30 min");
  assert.equal(D.duration(90), "1 min 30 s");
  assert.equal(D.bytes(512 * 1024 * 1024), "512 MB");
  assert.equal(D.bytes(1536), "1.5 KB");
  assert.equal(D.rate(null, 1), "—", "unreadable is a dash, never 0");
  assert.equal(D.rate(0, 10), "0 B/s");
  assert.equal(D.pct(null), "—");
  assert.equal(D.pct(0.01), "<0.1%");
  assert.equal(D.pct(4.25), "4.3%");
  assert.equal(D.pct(42.4), "42%");
});

test("system commands become prefixed commands with On/Off", () => {
  const cs = plain(D.systemCommands([
    { id: "night-light", name: "Toggle Night Light", group: "Toggles", glyph: "moon", keywords: ["night"], state: true },
    { id: "shut-down", name: "Shut Down", group: "Session", glyph: "power", confirm: true },
  ]));
  assert.equal(cs[0].id, "sys:night-light");
  assert.equal(cs[0].accessory, "On");
  assert.ok(cs[0].keywords.includes("toggles"));
  assert.equal(cs[1].accessory, "");
  assert.equal(cs[1].confirm, true);
  assert.equal(cs[1].system, "shut-down");
  const st = plain(D.stateCommands({ indexingPaused: true }));
  assert.equal(st[0].id, "resume-indexing");
  assert.equal(st[1].id, "pause-clipboard");
});

test("a typed timer and snippet keywords", () => {
  assert.equal(D.timerCommand(null), null);
  const c = plain(D.timerCommand({ seconds: 1500, label: "tea" }));
  assert.equal(c.name, "Start Timer · 25:00 tea");
  const hits = plain(D.keywordHits([{ id: 1, keyword: ";sig" }, { id: 2, keyword: ";meet" }, { id: 3, keyword: "" }], ";SIG"));
  assert.deepEqual(hits.map((h) => h.id), [1]);
  assert.deepEqual(plain(D.keywordHits([{ id: 3, keyword: "" }], "")), []);
});

test("timer rows: presets or the typed one, then the timers", () => {
  const timers = [
    { id: 1, label: "Tea", total: 1500, state: "running", ends_at: 1100, left: 1500 },
    { id: 2, label: "", total: 600, state: "paused", left: 300 },
  ];
  const empty = plain(D.timerRows("", null, timers, 1000));
  assert.equal(empty[0].rowId, "tnew:1");
  assert.ok(empty.some((r) => r.rowId === "hdr:timers"));
  const run = empty.find((r) => r.rowId === "timer:1");
  assert.equal(run.accessory, "1:40");
  assert.equal(empty.find((r) => r.rowId === "timer:2").title, "Timer");
  const typed = plain(D.timerRows("10 tea", { seconds: 600, label: "tea" }, timers, 1000));
  assert.equal(typed[0].rowId, "tnew:typed");
  assert.equal(typed[0].title, "Start 10:00 · tea");
  assert.equal(plain(D.timerRows("zz", null, [], 0)).length, 0);
});

test("process rows filter by name or pid; metrics follow the sort", () => {
  const rows = [
    { key: "app:a", name: "firefox", pids: [10, 11], cpu: 12.5, gpu: null, ram: 1048576, disk_read: 0, disk_write: 2048 },
    { key: "pid:20", name: "yes", pids: [20], cpu: 99, ram: 1024, disk_read: null, disk_write: null },
  ];
  assert.equal(D.procRows(rows, "fire").length, 1);
  assert.equal(D.procRows(rows, "20")[0].rowId, "pid:20");
  assert.equal(D.metric(rows[0], "cpu"), "13%");
  assert.equal(D.metric(rows[0], "gpu"), "—");
  assert.equal(D.metric(rows[0], "ram"), "1.0 MB");
  assert.equal(D.metric(rows[0], "disk"), "2.0 KB/s");
  assert.equal(D.metric(rows[1], "disk"), "—");
  assert.deepEqual(plain(D.SORTS), ["cpu", "gpu", "ram", "disk"]);
});

test("scripts rank name prefix, then words, then summary; themes mark current", () => {
  const scripts = [
    { id: "omarchy:omarchy-update", name: "Update", script: "omarchy-update", subtitle: "Update Omarchy", terminal: true },
    { id: "omarchy:omarchy-theme-bg-next", name: "Theme Bg Next", script: "omarchy-theme-bg-next", subtitle: "Cycle to the next background" },
    { id: "omarchy:omarchy-toggle-idle", name: "Toggle Idle", script: "omarchy-toggle-idle", subtitle: "Toggle idle behavior" },
  ];
  assert.deepEqual(plain(D.scriptRows(scripts, "idle")).map((r) => r.rowId), ["omarchy:omarchy-toggle-idle"]);
  assert.deepEqual(plain(D.scriptRows(scripts, "background")).map((r) => r.rowId), ["omarchy:omarchy-theme-bg-next"]);
  assert.equal(plain(D.scriptRows(scripts, ""))[0].accessory, "Terminal");
  const th = plain(D.themeRows(["Catppuccin", "Tokyo Night"], "Tokyo Night", ""));
  assert.equal(th[1].accessory, "Current");
  assert.equal(plain(D.themeRows(["Catppuccin", "Tokyo Night"], "", "tok")).length, 1);
});

test("snippet rows and placeholder parts", () => {
  const rows = plain(D.snippetRows([{ id: 3, name: "Greeting", keyword: ";hi", body: "Hi {argument:name},\n thanks" }]));
  assert.equal(rows[0].rowId, "snip:3");
  assert.equal(rows[0].subtitle, "Hi {argument:name}, thanks");
  assert.deepEqual(plain(D.parts("a {date} b {")), [
    { text: "a ", place: false }, { text: "{date}", place: true }, { text: " b {", place: false }]);
});


// Omarchy's menu (D35): the same matches in the same order as its own MenuModel.js, over the rows
// the daemon would send (visible, no root, no apps).
test("menu search matches MenuModel.js", () => {
  const golden = JSON.parse(readFileSync(new URL("../tests/menu/golden.json", import.meta.url), "utf8"));
  const byId = Object.fromEntries(golden.tree.map((t) => [t.id, t]));
  const depth = (t) => { let d = 0; let c = t; while (c && c.parent && c.parent !== "root") { d++; c = byId[c.parent]; } return d; };
  const rows = golden.tree
    .filter((t) => t.visible && t.id !== "root" && t.provider !== "apps")
    .map((t) => ({ id: t.id, parent: t.parent, kind: t.kind, label: t.label.replace(/ \u2713$/, ""), checked: / \u2713$/.test(t.label),
      path: t.path, depth: depth(t), order: t.order, aliases: t.aliases, description: t.description, target: t.target }));
  for (const [q, ids] of Object.entries(golden.search))
    assert.deepEqual(plain(D.menuSearch(rows, q).map((r) => r.id)), ids, q);

  const top = D.menuRows(rows, "root", "").map((r) => r.menuId);
  assert.deepEqual(plain(top.slice(0, 4)), ["learn", "trigger", "style", "setup"]);
  const install = D.menuRows(rows, "install", "");
  assert.equal(install[0].menuId, "install.package");
  assert.ok(install.every((r) => r.rowId.startsWith("menu:install.")));
  const link = D.menuRows(rows, "personal", "").find((r) => r.menuId === "personal.go-system");
  assert.equal(link.opens, "system");
  assert.equal(link.accessory, "\u203a");
  const hit = D.menuRows(rows, "root", "aur")[0];
  assert.deepEqual(plain([hit.title, hit.subtitle, hit.opens]), ["AUR", "Install", ""]);
  const stable = D.menuRows(rows, "update.channel", "").find((r) => r.menuId === "update.channel.stable");
  assert.equal(stable.title, "Stable \u2713");
  assert.equal(D.menuTitle(rows, "install.style"), "Install \u203a Style");

  // Main list: capped, no 1-letter queries, duplicates of curated commands dropped, and only an
  // exactly named submenu is strong.
  assert.deepEqual(plain(D.menuCommands(rows, "z")), []);
  assert.equal(D.menuCommands(rows, "theme").length, 4);
  const upd = D.menuCommands(rows, "update")[0];
  assert.deepEqual(plain([upd.id, upd.strong, upd.opens]), ["menu:update", true, "update"]);
  assert.equal(D.menuCommands(rows, "aur")[0].strong, false, "actions never lead");
  const dup = rows.map((r) => r.id === "system.lock" ? { ...r, dup_of: "lock-screen" } : r);
  assert.ok(!D.menuCommands(dup, "lock").some((c) => c.menu === "system.lock"));
});

console.log(`deskmodel: ${n} tests passed`);
