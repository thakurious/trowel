#!/usr/bin/env bash
# L3 live acceptance (LAUNCHER-PLAN.md L3): a scratch troweld against the real compositor copies
# text, a PNG, a file and HTML with wl-copy, checks each shows up in `trowel clip list` within
# 300 ms, and that `clip paste --mode copy` round-trips byte-identical (`wl-paste | sha256sum`).
#
# Isolated from the live daemon: its own runtime dir, socket, state, cache and config, deleted at
# the end. It overwrites the clipboard (cleared at the end) and never prints clipboard contents it
# did not put there. Other clipboard watchers (Omarchy's) will see the four test copies.
#   scripts/clip-live.sh [target-dir]   (default target/release; build first)
set -euo pipefail
BIN=$(realpath "${1:-target/release}")
REAL_RUNTIME=${XDG_RUNTIME_DIR:?needs a desktop session}
: "${WAYLAND_DISPLAY:?needs a Wayland session}"
case "$WAYLAND_DISPLAY" in
  /*) WL=$WAYLAND_DISPLAY ;;
  *) WL=$REAL_RUNTIME/$WAYLAND_DISPLAY ;;
esac
SCR=$(mktemp -d "$REAL_RUNTIME/trowel-clip-live.XXXXXX")
DPID=
cleanup() {
  [ -n "$DPID" ] && kill "$DPID" 2>/dev/null && wait "$DPID" 2>/dev/null
  wl-copy --clear 2>/dev/null || true
  rm -rf "$SCR"
}
trap cleanup EXIT
# hyprctl finds Hyprland through $XDG_RUNTIME_DIR/hypr.
[ -d "$REAL_RUNTIME/hypr" ] && ln -s "$REAL_RUNTIME/hypr" "$SCR/hypr"
mkdir -p "$SCR/trowel" "$SCR/files"

export XDG_RUNTIME_DIR=$SCR WAYLAND_DISPLAY=$WL XDG_CACHE_HOME=$SCR/cache TROWEL=$BIN/trowel
"$BIN/troweld" --fake --socket "$SCR/trowel/troweld.sock" --state-dir "$SCR/state" \
  --config "$SCR/config.toml" --index "$SCR/index" 2>"$SCR/troweld.log" &
DPID=$!
trowel() { "$BIN/trowel" "$@"; }

ms() { date +%s%3N; }
fail() { echo "FAIL: $*" >&2; sed 's/^/  troweld: /' "$SCR/troweld.log" >&2; exit 1; }

# Wait for the watcher. Its start-up capture of whatever was on the clipboard stays private.
for _ in $(seq 100); do
  [ -S "$SCR/trowel/troweld.sock" ] && trowel clip list --json >/dev/null 2>&1 && break
  sleep 0.05
done
sleep 0.5

# appears <jq-like python predicate> → prints "<id> <ms>" once a row matches, polling 10 ms.
appears() {
  local pred=$1 start=$2 out
  for _ in $(seq 300); do
    out=$(trowel clip list -n 50 --json 2>/dev/null | python3 -I -c '
import json, sys
pred = sys.argv[1]
for r in json.load(sys.stdin)["rows"]:
    if eval(pred, {}, {"r": r}):
        print(r["id"]); break' "$pred") || true
    if [ -n "$out" ]; then
      echo "$out $(($(ms) - start))"
      return 0
    fi
    sleep 0.01
  done
  return 1
}

TOKEN="trowel-live-$$-$RANDOM"
results=()
ID=
check() { # name pred start budget → sets $ID
  local got took
  got=$(appears "$2" "$3") || fail "$1 never appeared"
  ID=${got% *}; took=${got#* }
  [ "$took" -le "$4" ] || fail "$1 took ${took} ms (budget $4 ms)"
  results+=("$1 id=$ID visible after ${took} ms")
}

# An explicit type: bare `wl-copy` first sniffs the MIME type with xdg-mime (~150 ms here),
# which would count against Trowel's budget.
t=$(ms); printf '%s text' "$TOKEN" | wl-copy --type text/plain
check text "r['kind']=='text' and '$TOKEN text' in r['preview']" "$t" 300
TEXT_ID=$ID

magick -size 37x23 xc:'#F2A22A' "$SCR/files/live.png"
t=$(ms); wl-copy --type image/png <"$SCR/files/live.png"
check png "r['kind']=='image' and r['preview']=='Image 37×23'" "$t" 300
PNG_ID=$ID

printf 'x' >"$SCR/files/$TOKEN file.txt"
t=$(ms); printf 'file://%s\r\n' "$SCR/files/${TOKEN}%20file.txt" | wl-copy --type text/uri-list
check file "r['kind']=='files' and '$TOKEN file.txt' in r['preview']" "$t" 300
FILE_ID=$ID

t=$(ms); printf '<p><b>%s</b> html</p>' "$TOKEN" | wl-copy --type text/html
check html "'$TOKEN' in r['preview'] and 'html' in r['preview'] and r['kind'] in ('html','text')" "$t" 300
HTML_ID=$ID

# The PNG's thumbnail is rendered off-thread (libvips) and shows up in its row.
thumb=
for _ in $(seq 100); do
  thumb=$(trowel clip list -n 50 --json | python3 -I -c '
import json, sys
print(next((r.get("thumb", "") for r in json.load(sys.stdin)["rows"] if r["id"] == int(sys.argv[1])), ""))' "$PNG_ID")
  [ -n "$thumb" ] && break
  sleep 0.05
done
[ -n "$thumb" ] && [ -s "$thumb" ] || fail "no thumbnail for the PNG"
results+=("png thumbnail $(vipsheader "$thumb" 2>/dev/null | sed 's/.*: //')")

# Round trips through the daemon's wl-copy.
sleep 1.1 # past the 1 s self-copy window of the previous paste
roundtrip() { # id type expected-sha
  trowel clip paste "$1" --mode copy || fail "paste $1"
  sleep 0.2
  local got
  got=$(wl-paste --no-newline --type "$2" | sha256sum | cut -d' ' -f1)
  [ "$got" = "$3" ] || fail "round trip of $1 as $2: $got != $3"
  results+=("roundtrip $2 ok")
}
roundtrip "$PNG_ID" image/png "$(sha256sum <"$SCR/files/live.png" | cut -d' ' -f1)"
roundtrip "$TEXT_ID" text/plain "$(printf '%s text' "$TOKEN" | sha256sum | cut -d' ' -f1)"
roundtrip "$FILE_ID" text/uri-list \
  "$(printf 'file://%s\r\n' "$SCR/files/${TOKEN}%20file.txt" | sha256sum | cut -d' ' -f1)"

kind=$(trowel clip list -n 50 --json | python3 -I -c '
import json, sys
print([r["kind"] for r in json.load(sys.stdin)["rows"] if r["id"] == int(sys.argv[1])][0])' "$HTML_ID")
results+=("html entry kind=$kind")
printf '%s\n' "${results[@]}"
echo "PASS"
