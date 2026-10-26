// Unit tests for ui/qml/Trowel/ListMerge.js (a QML `.pragma library` file) under node:
//   node scripts/listmerge-test.mjs
import { readFileSync } from "node:fs";
import vm from "node:vm";
import assert from "node:assert/strict";

const src = readFileSync(new URL("../ui/qml/Trowel/ListMerge.js", import.meta.url), "utf8")
  .replace(/^\.pragma library\s*$/m, "");
const M = {};
vm.runInNewContext(src + "\nthis.M = { strongApp, merge, clipRows, selectable, defaultSelection, ago, filterActions, matchCommands };", M);
const L = M.M;

const files = [
  { rowType: "header", rowId: "hdr:documents" },
  { rowType: "row", rowId: "/d/firewall.pdf" },
  { rowType: "row", rowId: "/d/fire.txt" },
  { rowType: "fold", rowId: "\u0000weaker" },
];
const firefox = { id: "firefox", name: "Firefox", actions: [{ id: "p", name: "Private" }] };
const chromium = { id: "chromium", name: "Chromium", generic: "Web Browser" };
// Arrays from the vm context have another realm's prototype: copy into this one.
const ids = (rows) => Array.from(rows, (r) => r.rowId);
let n = 0;
const test = (name, f) => { f(); n++; };

test("strong app goes first", () => {
  const rows = L.merge(files, [firefox, chromium], "fire");
  assert.deepEqual(ids(rows), ["hdr:apps", "app:firefox", "app:chromium", ...ids(files)]);
  assert.equal(rows[1].actionCount, 1);
  assert.equal(L.defaultSelection(rows, false), "app:firefox");
});

test("weak apps follow the files, before the weaker fold", () => {
  const rows = L.merge(files, [chromium], "browser");
  assert.deepEqual(ids(rows), ["hdr:documents", "/d/firewall.pdf", "/d/fire.txt", "hdr:apps", "app:chromium", "\u0000weaker"]);
  assert.equal(L.defaultSelection(rows, false), "/d/firewall.pdf");
});

test("no fold: apps at the end; no apps: files untouched", () => {
  const noFold = files.slice(0, 3);
  assert.deepEqual(ids(L.merge(noFold, [chromium], "web")).slice(-2), ["hdr:apps", "app:chromium"]);
  assert.deepEqual(ids(L.merge(files, [], "x")), ids(files));
  assert.notEqual(L.merge(files, [], "x"), files, "a copy, never the input");
});

test("prefix is case-insensitive and trimmed; name only", () => {
  assert.ok(L.strongApp(firefox, "  FIR "));
  assert.ok(!L.strongApp(firefox, "fox"));
  assert.ok(!L.strongApp({ id: "firefox", name: "Web" }, "fire"), "id prefix is a lower band");
  assert.ok(!L.strongApp(firefox, ""));
});

test("commands: a strong match goes first, before apps", () => {
  const rows = L.merge(files, [firefox], "clip");
  assert.deepEqual(ids(rows).slice(0, 2), ["hdr:commands", "cmd:clipboard"]);
  assert.equal(L.defaultSelection(rows, false), "cmd:clipboard");
  assert.equal(rows[1].glyph, "clipboard");
  // Both strong: commands, then apps, then files.
  const both = L.merge(files, [{ id: "clipgrab", name: "ClipGrab" }], "clip");
  assert.deepEqual(ids(both).slice(0, 4), ["hdr:commands", "cmd:clipboard", "hdr:apps", "app:clipgrab"]);
});

test("commands: name, word and keyword prefixes; weak ones go below the files", () => {
  const ok = (q) => Array.from(L.matchCommands(q), (m) => m.command.id + (m.strong ? "!" : ""));
  assert.deepEqual(ok("Clipboard"), ["clipboard!"]);
  assert.deepEqual(ok("paste"), ["clipboard!"], "keyword");
  assert.deepEqual(ok("hist"), ["clipboard!"], "keyword 'history'");
  assert.deepEqual(ok("board"), [], "not mid-word");
  assert.deepEqual(ok(""), []);
  const custom = [{ id: "x", name: "Toggle Night Light", keywords: [] }];
  assert.deepEqual(Array.from(L.matchCommands("night", custom), (m) => m.strong), [false], "a later name word is weak");
  const rows = L.merge(files, [], "night", { commands: custom });
  assert.deepEqual(ids(rows).slice(-3), ["hdr:commands", "cmd:x", "\u0000weaker"]);
  assert.deepEqual(ids(L.merge(files, [], "fire", { commands: [] })), ids(files));
});

test("app cap", () => {
  const many = Array.from({ length: 9 }, (_, i) => ({ id: "a" + i, name: "App " + i }));
  assert.equal(L.merge([], many, "app").filter((r) => r.rowType === "app").length, 5);
  assert.equal(L.merge([], many, "app", { maxApps: 2 }).filter((r) => r.rowType === "app").length, 2);
});

test("calculator card is selected first and ↑/↓ starts from it", () => {
  const rows = L.merge(files, [firefox], "fire");
  assert.equal(L.defaultSelection(rows, true), "calc:");
  assert.deepEqual(Array.from(L.selectable(rows, true)).slice(0, 2), ["calc:", "app:firefox"]);
  assert.ok(!L.selectable(rows, false).includes("hdr:apps"), "headers are not selectable");
  assert.equal(L.defaultSelection([{ rowType: "fold", rowId: "w" }], false), "");
});

test("clip rows: pinned section, then recent", () => {
  const clips = [
    { id: 9, kind: "image", preview: "Image 4×3", pinned: true, thumb: "/c/9@132.png" },
    { id: 8, kind: "link", preview: "https://x.org", domain: "x.org", source_app: "google-chrome", last_used: 100 },
  ];
  const rows = L.clipRows(clips);
  assert.deepEqual(ids(rows), ["hdr:pinned", "clip:9", "hdr:recent", "clip:8"]);
  assert.equal(rows[3].sourceApp, "google-chrome");
  assert.deepEqual(ids(L.clipRows([clips[1]])), ["clip:8"], "no headers without pins");
  assert.deepEqual(ids(L.clipRows(clips, false)), ["clip:9", "clip:8"], "grid: no headers");
});

test("ago", () => {
  assert.equal(L.ago(100, 130), "now");
  assert.equal(L.ago(100, 100 + 150), "2 m ago");
  assert.equal(L.ago(0, 7200), "2 h ago");
  assert.equal(L.ago(0, 86400 * 3), "3 d ago");
});

test("action filter: prefix, then word start, then subsequence", () => {
  const acts = [
    { id: "copy", label: "Copy" },
    { id: "paste", label: "Paste" },
    { id: "plain", label: "Paste as plain text" },
    { id: "pin", label: "Pin" },
    { id: "open", label: "Open in browser" },
  ];
  const f = (q) => Array.from(L.filterActions(acts, q), (a) => a.id);
  assert.deepEqual(f(""), ["copy", "paste", "plain", "pin", "open"]);
  assert.deepEqual(f("pa"), ["paste", "plain"]);
  assert.deepEqual(f("pl"), ["plain"], "pLain is a subsequence of 'paste as plain' only");
  assert.deepEqual(f("brow"), ["open"], "word start");
  assert.deepEqual(f("pn"), ["plain", "pin", "open"], "all subsequences: given order");
  assert.deepEqual(f("zz"), []);
  assert.deepEqual(f("P+"), [], "regex characters are literal");
});

test("learned: an app opened with this query leads the commands", () => {
  const chrome = { id: "google-chrome", name: "Google Chrome", learned: { frecency: 101, term: "exact" } };
  // "c" is a Clipboard History keyword prefix and not Chrome's name, but "c" opened Chrome. Only
  // the clipboard command, so the test is about order and not about which commands exist.
  const clip = [{ id: "clipboard", name: "Clipboard History", keywords: ["clipboard", "copy"] }];
  let rows = L.merge([], [chrome], "c", { commands: clip });
  assert.deepEqual(ids(rows), ["hdr:apps", "app:google-chrome", "hdr:commands", "cmd:clipboard"]);
  assert.equal(L.defaultSelection(rows, false), "app:google-chrome");
  // Nothing learned: commands keep leading.
  rows = L.merge([], [chromium], "c", { commands: clip });
  assert.deepEqual(ids(rows), ["hdr:commands", "cmd:clipboard", "hdr:apps", "app:chromium"]);
  // Both strong, only frecency: the more frecent section first; a learned command wins back.
  const used = { id: "chromium", name: "Chromium", learned: { frecency: 150 } };
  rows = L.merge([], [used], "c", { commands: clip });
  assert.equal(rows[0].rowId, "hdr:apps");
  rows = L.merge([], [used], "c", { commands: clip, signals: { clipboard: { frecency: 101, term: "prefix" } } });
  assert.equal(rows[0].rowId, "hdr:commands");
});

test("learned: commands order by learned term, then text, then frecency", () => {
  const cmds = [
    { id: "a", name: "Alpha Notes", keywords: [] },
    { id: "b", name: "Notes", keywords: [] },
    { id: "c", name: "Note Search", keywords: [] },
  ];
  let m = L.matchCommands("not", cmds);
  assert.deepEqual(Array.from(m, (x) => x.command.id), ["b", "c", "a"]);
  m = L.matchCommands("not", cmds, { a: { frecency: 101, term: "exact" }, c: { frecency: 300 } });
  assert.deepEqual(Array.from(m, (x) => x.command.id), ["a", "c", "b"]);
  assert.equal(m[0].strong, true, "a learned query makes a weak word match strong");
  // A learned weak command lifts the section above the files.
  const rows = L.merge(files, [], "not", { commands: cmds.slice(0, 1), signals: { a: { frecency: 101, term: "prefix" } } });
  assert.equal(rows[0].rowId, "hdr:commands");
});

test("settings rows are selectable, headers are not", () => {
  const rows = [
    { rowType: "header", rowId: "hdr:General" },
    { rowType: "setting", rowId: "set:hotkey" },
    { rowType: "setting", rowId: "set:battery" },
  ];
  assert.deepEqual(Array.from(L.selectable(rows, false)), ["set:hotkey", "set:battery"]);
  assert.equal(L.defaultSelection(rows, false), "set:hotkey");
});

console.log(`listmerge: ${n} tests passed`);
