#!/usr/bin/env bash
# L2c false-positive gate on this machine's own text (LAUNCHER-PLAN.md L2c). Local only: nothing
# here is committed, and the clipboard corpus is checked with --quiet so its text is never printed.
#   scripts/calc-corpus.sh [trowel-binary]
set -euo pipefail
TROWEL=${1:-target/release/trowel}
TB=${TROWEL_EVAL:-/tmp/trowel-eval}
out=$(mktemp -d)
trap 'rm -rf "$out"' EXIT

# Every installed app name (Name=, GenericName=, Keywords= split on ;).
IFS=: read -ra dirs <<<"${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"
for d in "$HOME/.local/share" "${dirs[@]}" "$HOME/.nix-profile/share"; do
  [ -d "$d/applications" ] || continue
  find -L "$d/applications" -name '*.desktop' -print0 2>/dev/null |
    xargs -0 -r grep -h -E '^(Name|GenericName|Keywords)=' 2>/dev/null || true
done | sed -E 's/^[^=]+=//' | tr ';' '\n' | sed '/^\s*$/d' | sort -u >"$out/apps.txt"

# 500 file names from the eval testbed.
if [ -d "$TB" ]; then
  find "$TB" -type f -printf '%f\n' | sort -u | sed -n '1,500p' >"$out/files.txt"
else
  echo "no testbed at $TB (scripts/make-testbed.sh); skipping file names" >&2
  : >"$out/files.txt"
fi

# Omarchy clipboard text entries (private: --quiet).
CLIP="$HOME/.local/state/omarchy/clipboard-history.json"
if [ -f "$CLIP" ]; then
  python3 -I -c 'import json,sys
d=json.load(open(sys.argv[1]))
items=d if isinstance(d,list) else d.get("entries") or d.get("items") or []
for e in items:
    t=e.get("text") if isinstance(e,dict) and e.get("type","text")=="text" else (e if isinstance(e,str) else None)
    if isinstance(t,str) and t.strip() and "\n" not in t.strip():
        print(t.strip())' "$CLIP" >"$out/clip.txt"
else
  : >"$out/clip.txt"
fi

echo "apps $(wc -l <"$out/apps.txt")  files $(wc -l <"$out/files.txt")  clipboard $(wc -l <"$out/clip.txt") (single-line text)"
status=0
"$TROWEL" calc --corpus "$out/apps.txt" --corpus "$out/files.txt" || status=1
"$TROWEL" calc --corpus "$out/clip.txt" --quiet || status=1
exit $status
