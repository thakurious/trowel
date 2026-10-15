#!/usr/bin/env bash
# L2 ranking parity gate (LAUNCHER-PLAN.md L2): Omarchy's own AppSearch.js under node and
# Trowel's Rust port rank this machine's installed entries for every query; the orders must be
# identical (frecency off). Zero diffs or exit 1.
#   scripts/apps-parity.sh [trowel-binary]
set -euo pipefail
TROWEL=${1:-target/release/trowel}
APPSEARCH=${APPSEARCH:-${OMARCHY_PATH:-/usr/share/omarchy}/shell/services/AppSearch.js}
here=$(cd "$(dirname "$0")" && pwd)
out=$(mktemp -d)
trap 'rm -rf "$out"' EXIT

"$TROWEL" apps --dump-entries >"$out/entries.json"

# Queries: every name prefix of length 1–4, acronyms, two-word and multi-term queries, id
# fragments, generic names, and a few that match nothing.
python3 -I - "$out/entries.json" >"$out/queries.txt" <<'PY'
import json, re, sys
entries = json.load(open(sys.argv[1]))
qs = {"", "zzzz", "qqqx"}
for e in entries:
    name = e["name"].lower()
    for n in range(1, 5):
        if len(name) >= n:
            qs.add(name[:n])
    words = re.findall(r"[a-z0-9]+", re.sub(r"([a-z0-9])([A-Z])", r"\1 \2", e["name"]).lower())
    if len(words) >= 2:
        qs.add("".join(w[0] for w in words[:3]))
        qs.add(f"{words[0]} {words[1][:2]}")
        qs.add(f"{words[1]} {words[0][:3]}")
    for part in re.split(r"[.\-_ ]+", e["id"].lower()):
        if len(part) >= 3:
            qs.add(part[:5])
    if e.get("genericName"):
        qs.add(e["genericName"].lower().split()[0])
    for k in e.get("keywords", [])[:2]:
        qs.add(k.lower()[:6])
qs = sorted(q for q in qs if "\n" not in q)
print("\n".join(qs))
PY

node "$here/apps-parity.mjs" "$APPSEARCH" "$out/entries.json" "$out/queries.txt" >"$out/omarchy.json"
"$TROWEL" apps --entries "$out/entries.json" --rank-queries "$out/queries.txt" >"$out/trowel.json"

python3 -I - "$out/omarchy.json" "$out/trowel.json" "$out/entries.json" <<'PY'
import json, sys
omarchy, trowel = json.load(open(sys.argv[1])), json.load(open(sys.argv[2]))
entries = json.load(open(sys.argv[3]))
diffs = [q for q in omarchy if omarchy[q] != trowel.get(q)]
missing = [q for q in trowel if q not in omarchy]
for q in diffs[:20]:
    o, t = omarchy[q], trowel.get(q, [])
    first = next((i for i, (a, b) in enumerate(zip(o, t)) if a != b), min(len(o), len(t)))
    print(f"DIFF {q!r} at {first}: omarchy {o[first:first+3]} trowel {t[first:first+3]} (lens {len(o)}/{len(t)})")
rows = sum(len(v) for v in omarchy.values())
print(f"{len(entries)} entries, {len(omarchy)} queries, {rows} ranked rows, {len(diffs)} diffs")
sys.exit(1 if diffs or missing else 0)
PY
