// Unit tests for ui/qml/Trowel/KeyLockModel.js (a QML `.pragma library` file) under node:
//   node scripts/keylock-model-test.mjs
import { readFileSync } from "node:fs";
import vm from "node:vm";
import assert from "node:assert/strict";

const src = readFileSync(new URL("../ui/qml/Trowel/KeyLockModel.js", import.meta.url), "utf8")
  .replace(/^\.pragma library\s*$/m, "");
const M = {};
vm.runInNewContext(src + "\nthis.M = { label, clock, parse, rows, MAX_SECONDS };", M);
const K = M.M;
const ids = (q) => Array.from(K.rows(q), (r) => r.rowId);
let n = 0;
const test = (name, f) => {
  f();
  n++;
};

test("labels", () => {
  assert.equal(K.label(15), "15 seconds");
  assert.equal(K.label(60), "1 minute");
  assert.equal(K.label(90), "1 minute 30 s");
  assert.equal(K.label(600), "10 minutes");
});

test("countdown clock", () => {
  assert.equal(K.clock(42), "0:42");
  assert.equal(K.clock(61.2), "1:02");
  assert.equal(K.clock(-3), "0:00");
});

test("typed durations", () => {
  assert.equal(K.parse("45"), 45);
  assert.equal(K.parse("90s"), 90);
  assert.equal(K.parse("3m"), 180);
  assert.equal(K.parse("1.5 min"), 90);
  assert.equal(K.parse("2 minutes"), 120);
  assert.equal(K.parse("0"), 0);
  assert.equal(K.parse("31m"), 0, "over the 30 minute cap");
  assert.equal(K.parse("soon"), 0);
});

test("rows: presets, filtered, a custom duration first", () => {
  assert.deepEqual(ids(""), ["lock:15", "lock:30", "lock:60", "lock:120", "lock:300", "lock:600"]);
  assert.deepEqual(ids("45"), ["lock:45"]);
  assert.deepEqual(ids("3m"), ["lock:180"]);
  assert.deepEqual(ids("1m"), ["lock:60"], "a preset typed is not repeated");
  // "1": a one-second lock, then the presets whose label starts with it.
  assert.deepEqual(ids("1 "), ["lock:1", "lock:15", "lock:60", "lock:600"]);
  assert.deepEqual(ids("10"), ["lock:10", "lock:600"]);
  assert.deepEqual(ids("zz"), []);
});

console.log(`keylock-model: ${n} tests passed`);
