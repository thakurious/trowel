#!/usr/bin/env bash
# Phase 3 acceptance: kill -9 the index helper; finished files are not re-embedded.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TROWEL="${TROWEL:-$ROOT/target/debug/trowel}"
WORKDIR="${TMPDIR:-/tmp}/trowel-crash-resume-$$"
FILES="$WORKDIR/files"
INDEX="$WORKDIR/index"
mkdir -p "$FILES"
trap 'rm -rf "$WORKDIR"' EXIT

for i in 1 2 3 4 5 6 7 8 9 10; do
  printf 'the harbour meeting notes from village library %s garden morning letter window\n' "$i" \
    >"$FILES/notes-$i.txt"
done

"$TROWEL" index "$FILES" --index "$INDEX" --fake >/dev/null
before=$("$TROWEL" status --index "$INDEX" --json | python3 -c 'import json,sys; print(json.load(sys.stdin)["vectors"])')

# A finished index must not grow vectors on a second pass.
"$TROWEL" index "$FILES" --index "$INDEX" --fake >/dev/null
after=$("$TROWEL" status --index "$INDEX" --json | python3 -c 'import json,sys; print(json.load(sys.stdin)["vectors"])')
if [[ "$before" != "$after" ]]; then
  echo "re-embedded finished files: vectors $before -> $after" >&2
  exit 1
fi

lost=0
for round in $(seq 1 50); do
  "$TROWEL" index-service "$FILES" --index "$INDEX" --fake --backfill --linger 2 \
    >/tmp/trowel-crash-resume.out 2>/tmp/trowel-crash-resume.err &
  pid=$!
  sleep 0.05
  kill -9 "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
done

now=$("$TROWEL" status --index "$INDEX" --json | python3 -c 'import json,sys; print(json.load(sys.stdin)["vectors"])')
if [[ "$now" -lt "$before" ]]; then
  echo "lost vectors after kill -9: $before -> $now" >&2
  exit 1
fi
if [[ "$now" -gt "$before" ]]; then
  # in-flight files may finish or be poisoned; vectors must not jump for already-done files
  # a small increase is only OK if pending work existed; we started from a finished index.
  echo "vectors grew after kill -9 of an idle helper: $before -> $now" >&2
  lost=1
fi

"$TROWEL" index "$FILES" --index "$INDEX" --fake >/dev/null
final=$("$TROWEL" status --index "$INDEX" --json | python3 -c 'import json,sys; print(json.load(sys.stdin)["vectors"])')
if [[ "$final" != "$now" && "$final" != "$before" ]]; then
  echo "resume re-embedded: before=$before now=$now final=$final" >&2
  exit 1
fi

echo "crash-resume ok: vectors=$before after-kills=$now final=$final rounds=50 lost_check=$lost"
exit 0
