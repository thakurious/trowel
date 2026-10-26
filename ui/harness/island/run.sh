#!/bin/sh
# Offscreen Island harness (no compositor): ui/harness/island/run.sh [out-dir]
# Renders IslandView through its states and prints one HARNESS line per state.
set -e
H=$(dirname "$(readlink -f "$0")")
OUT=${1:-$H/../out}
mkdir -p "$OUT"
HARNESS_OUT=$OUT WAYLAND_DISPLAY= QT_QPA_PLATFORM="offscreen:configfile=$H/../screens.json" \
  timeout 60 quickshell -p "$H" 2>&1 | grep -E "HARNESS|ERROR|WARN|TypeError|ReferenceError" | grep -v "window masks" || true
