// Unit tests for ui/qml/Trowel/ListMerge.js (a QML `.pragma library` file) under node:
//   node scripts/listmerge-test.mjs
import { readFileSync } from "node:fs";
import vm from "node:vm";
import assert from "node:assert/strict";

const src = readFileSync(new URL("../ui/qml/Trowel/ListMerge.js", import.meta.url), "utf8")
  .replace(/^\.pragma library\s*$/m, "");
const M = {};
vm.runInNewContext(src + "\nthis.M = { strongApp, merge, clipRows, selectable, defaultSelection, ago };", M);
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
});

test("ago", () => {
  assert.equal(L.ago(100, 130), "now");
  assert.equal(L.ago(100, 100 + 150), "2 m ago");
  assert.equal(L.ago(0, 7200), "2 h ago");
  assert.equal(L.ago(0, 86400 * 3), "3 d ago");
});

console.log(`listmerge: ${n} tests passed`);
