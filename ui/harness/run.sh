#!/bin/sh
# Offscreen panel harness (no compositor, never the live session):
#   ui/harness/run.sh '<json steps>' [out-dir]
# Starts `troweld --mock`, runs the plugin's Panel in Quickshell with QT_QPA_PLATFORM=offscreen,
# replays the steps ({"do": ready|open|clipboard|type|down|up|tab|log|shot, …}; see shell.qml), and
# saves each "shot" as <out-dir>/<name>.png. "log" prints a HARNESS line with the panel state.
set -e
H=$(dirname "$(readlink -f "$0")")
REPO=$(dirname "$(dirname "$H")")
OUT=${2:-$H/out}
R=${XDG_RUNTIME_DIR:-/tmp}/trowel-harness-$$
rm -rf "$R" "$OUT"
mkdir -p "$R" "$OUT"
python3 -I "$H/prep.py"
"$REPO/target/debug/troweld" --mock --latency-ms "${HARNESS_LATENCY_MS:-0}" ${HARNESS_FIXTURE:+--fixture "$HARNESS_FIXTURE"} --socket "$R/troweld.sock" >"$R/mock.log" 2>&1 &
MP=$!
trap 'kill $MP 2>/dev/null; rm -rf "$R"' EXIT
sleep 0.4
HARNESS_OUT=$OUT HARNESS_SCRIPT="$1" TROWEL_SOCKET=$R/troweld.sock WAYLAND_DISPLAY= \
  QT_QPA_PLATFORM="offscreen:configfile=$H/screens.json" timeout 60 quickshell -p "$H" 2>&1 |
  grep -E "HARNESS|ERROR|WARN|TypeError|ReferenceError" |
  grep -v "window masks" || true
