// Omarchy's side of the apps parity gate: its own AppSearch.js ranks every query.
//   node scripts/apps-parity.mjs <AppSearch.js> <entries.json> <queries.txt>
import { createRequire } from "node:module";
import { readFileSync } from "node:fs";

const [appSearchPath, entriesPath, queriesPath] = process.argv.slice(2);
const AppSearch = createRequire(import.meta.url)(appSearchPath);
const entries = JSON.parse(readFileSync(entriesPath, "utf8"));
const out = {};
for (const q of readFileSync(queriesPath, "utf8").split("\n")) {
  if (q === "" && Object.hasOwn(out, "")) continue;
  out[q] = AppSearch.sortedEntries(entries, q, null).map((r) => r.entry.id);
}
process.stdout.write(JSON.stringify(out));
