#!/bin/bash
# Linux port of upstream/scripts/make-testbed.sh.
# Never samples the live home directory (Phase 5 / CLAUDE.md). Default: repo testbed/.
set -euo pipefail
REPO="$(cd "$(dirname "$0")/.." && pwd)"
TB="${1:-$REPO/testbed}"
command -v ffmpeg >/dev/null || { echo "needs ffmpeg"; exit 1; }
command -v vips >/dev/null || { echo "needs vips"; exit 1; }

mkdir -p "$TB/traps" "$TB/synthetic"
T="$TB/traps"

# TypeScript vs real MPEG-TS (RulesTests).
mkdir -p "$T/ts-code" "$T/real-mpeg-ts"
printf 'export const greet = (name: string): string => `hello ${name}`;\n' > "$T/ts-code/app.ts"
# 4 × 188-byte packets starting 0x47.
python3 - <<'PY' "$T/real-mpeg-ts/clip.ts"
import pathlib, sys
p = pathlib.Path(sys.argv[1])
buf = bytearray(b"\xff") * (188 * 4)
for i in range(4):
    buf[i * 188] = 0x47
p.write_bytes(buf)
PY

# Code project: only its Omarchy-named screenshot should be indexed.
P="$T/MyProject"
mkdir -p "$P/.git" "$P/assets" "$P/node_modules/lib" "$P/screenshots"
echo '{"name": "my-project"}' > "$P/package.json"
# 320×320 PNG via vips so it is above minImageSide.
vips black "$P/assets/hero.png" 320 320
vips black "$P/node_modules/lib/logo.png" 320 320
vips black "$P/screenshots/screenshot-2026-10-12_08-15-40.png" 320 320

# Linux traps from EXECUTION-PLAN Phase 2.
mkdir -p "$T/linux"
python3 - "$T/linux" <<'PY'
import os, sys
root = os.fsencode(sys.argv[1])
os.makedirs(root, exist_ok=True)
open(os.path.join(root, b"-notes.txt"), "wb").write(b"hello")
open(os.path.join(root, b"bad\xff.txt"), "wb").write(b"x")
open(os.path.join(root, ("n" * 251 + ".txt").encode()), "wb").write(b"x")
open(os.path.join(root, b"trunc.pdf"), "wb").write(b"%PDF-1.4\ntruncated")
open(os.path.join(root, b"bomb.docx"), "wb").write(b"PK\x03\x04not-a-zip")
open(os.path.join(root, b"empty.mp4"), "wb").write(b"")
PY

# Unreadable directory (restored after chmod in tests; here we only create it).
mkdir -p "$T/linux/unreadable"
echo x > "$T/linux/unreadable/x.txt"

echo "testbed traps at $T"
echo "classify with: cargo run -p trowel-cli -- estimate --json $T"
