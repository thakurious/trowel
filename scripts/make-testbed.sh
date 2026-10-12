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
S="$TB/synthetic"

# TypeScript vs real MPEG-TS (RulesTests).
mkdir -p "$T/ts-code" "$T/real-mpeg-ts"
printf 'export const greet = (name: string): string => `hello ${name}`;\n' > "$T/ts-code/app.ts"
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
open(os.path.join(root, b"line\nbreak.txt"), "wb").write(b"x")
open(os.path.join(root, ("n" * 251 + ".txt").encode()), "wb").write(b"x")
open(os.path.join(root, b"trunc.pdf"), "wb").write(b"%PDF-1.4\ntruncated")
open(os.path.join(root, b"bomb.docx"), "wb").write(b"PK\x03\x04not-a-zip")
open(os.path.join(root, b"empty.mp4"), "wb").write(b"")
PY

# Unreadable directory (tests chmod 000; the builder only creates it).
mkdir -p "$T/linux/unreadable"
echo x > "$T/linux/unreadable/x.txt"

# Symlink loop: crawler must not follow (walkdir follow_links=false).
mkdir -p "$T/linux/loop/a" "$T/linux/loop/b"
echo x > "$T/linux/loop/a/note.txt"
ln -sfn "$T/linux/loop/a" "$T/linux/loop/b/to-a"
ln -sfn "$T/linux/loop/b" "$T/linux/loop/a/to-b"

# Synthetic media/docs via ffmpeg/vips/python — never sample $HOME.
mkdir -p "$S/img" "$S/shots" "$S/audio" "$S/video" "$S/docs" "$S/icons" "$S/dataset_clips"
vips black "$S/img/holiday.png" 320 320
vips black "$S/img/trip.jpg" 400 300
vips black "$S/shots/screenshot-2026-10-12_09-00-00.png" 320 320
vips black "$S/shots/grim-2026-10-12.png" 320 320
vips black "$S/icons/favicon.png" 32 32
printf 'Trip notes\nTokyo itinerary\n' > "$S/docs/notes.txt"
printf '# Readme\n\nA day in Kyoto.\n' > "$S/docs/readme.md"
printf '<html><body><p>Hotel booking</p></body></html>\n' > "$S/docs/page.html"
python3 - "$S/docs/one.pdf" <<'PY'
import sys
# Minimal one-page PDF so pdfinfo reports Pages: 1.
pdf = b"""%PDF-1.1
1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj
2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj
3 0 obj<</Type/Page/MediaBox[0 0 612 792]/Parent 2 0 R>>endobj
xref
0 4
0000000000 65535 f 
0000000009 00000 n 
0000000058 00000 n 
0000000115 00000 n 
trailer<</Size 4/Root 1 0 R>>
startxref
190
%%EOF
"""
open(sys.argv[1], "wb").write(pdf)
PY
ffmpeg -loglevel error -y -f lavfi -i sine=frequency=440:duration=2 -ac 1 -ar 16000 "$S/audio/tone.wav"
ffmpeg -loglevel error -y -f lavfi -i color=c=blue:s=320x240:d=2 -pix_fmt yuv420p "$S/video/color.mp4"
python3 - "$S/dataset_clips" <<'PY'
import os, sys
root = sys.argv[1]
os.makedirs(root, exist_ok=True)
for i in range(400):
    open(os.path.join(root, f"clip_{i:06}.wav"), "wb").close()
PY

# Small code repo (portable subset of upstream make_code.py, no live sampling).
CODE="$TB/code"
mkdir -p "$CODE/app"
printf 'def retry_with_backoff(attempts=5):\n    pass\n' > "$CODE/app/retry.py"
printf '# App\n' > "$CODE/app/README.md"
git -C "$CODE/app" init -q >/dev/null 2>&1 || true

echo "testbed traps at $T"
echo "synthetic at $S"
echo "classify with: cargo run -p trowel-cli -- estimate --json $TB"
