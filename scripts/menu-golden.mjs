// Goldens for the Omarchy menu port (D35) from Omarchy's own MenuModel.js:
//   node scripts/menu-golden.mjs [MenuModel.js]   (default $OMARCHY_PATH/shell/plugins/menu/MenuModel.js)
// Reads tests/menu/{omarchy-menu,extension}.jsonc and guards.txt; writes tests/menu/golden.json.
// guards.txt holds `<id>:<w|c>:<0|1>` lines, the shape the guard script prints.
import { createRequire } from "node:module";
import { readFileSync, writeFileSync } from "node:fs";

const omarchy = process.env.OMARCHY_PATH || "/usr/share/omarchy";
const modelPath = process.argv[2] || `${omarchy}/shell/plugins/menu/MenuModel.js`;
const M = createRequire(import.meta.url)(modelPath);
const dir = new URL("../tests/menu/", import.meta.url);
const read = (f) => readFileSync(new URL(f, dir), "utf8");

const merged = M.mergeMenuSources(M.parseMenuJsonc(read("omarchy-menu.jsonc")), M.parseMenuJsonc(read("extension.jsonc")));
const { items, itemOrder } = merged;

// Same parse as Menu.qml's guardProc.onExited.
const when = {};
const checked = {};
for (const raw of read("guards.txt").split("\n")) {
  const line = raw.trim();
  const colon = line.lastIndexOf(":");
  if (colon < 0) continue;
  const rest = line.substring(0, colon);
  const tagAt = rest.lastIndexOf(":");
  if (tagAt < 0) continue;
  const id = rest.substring(0, tagAt);
  const tag = rest.substring(tagAt + 1);
  const value = line.substring(colon + 1) === "1";
  if (tag === "w") when[id] = value;
  else if (tag === "c") checked[id] = value;
}

const tree = itemOrder.map((id) => {
  const e = items[id];
  return {
    id, parent: e.parent, kind: e.kind, label: M.labelFor(e, checked), path: M.pathFor(items, id),
    target: e.target, provider: e.provider, aliases: e.aliases, description: e.description,
    action: e.action, order: e.order, visible: M.isVisible(items, itemOrder, when, e),
  };
});

// Search as Menu.qml's filtered display does: every visible match, best score first.
const queries = ["aur", "update", "install", "theme", "lock", "font", "remove chrome", "wifi", "sys", "notes", "mine", "dns", "z"];
const search = {};
for (const q of queries) {
  search[q] = itemOrder
    .map((id) => items[id])
    .filter((e) => M.matchesQuery(e, q, M.isVisible(items, itemOrder, when, e)))
    .map((e) => ({ id: e.id, score: M.searchScore(items, e, q) }))
    .sort((a, b) => a.score - b.score)
    .map((r) => r.id);
}

writeFileSync(new URL("golden.json", dir), JSON.stringify({ tree, search, slugs: ["Fira Code", "Fira-Code", "  JetBrains Mono NF ", "!!!"].map(M.slugify) }, null, 1) + "\n");
writeFileSync(new URL("guard.sh", dir), M.guardScript(items));
console.log(`${itemOrder.length} items, ${queries.length} queries`);
