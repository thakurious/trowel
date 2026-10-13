#!/bin/bash
# Linux port of upstream/scripts/make-testbed.sh.
# Never samples the live home directory (CLAUDE.md). Default is /tmp/trowel-eval so
# GitRepos.enter_root does not walk into this repo's .git (code eval 41/41).
#
# Needs: ffmpeg, vips, pango-view, chromium, python3 + Pillow, network
# (Wikimedia + Google TTS). No macOS tools (sips, say, Swift, /Library/User Pictures).
#   scripts/make-testbed.sh [dir]
set -euo pipefail
REPO="$(cd "$(dirname "$0")/.." && pwd)"
TB="${1:-/tmp/trowel-eval}"
command -v ffmpeg >/dev/null || { echo "needs ffmpeg"; exit 1; }
command -v vips >/dev/null || { echo "needs vips"; exit 1; }
command -v pango-view >/dev/null || { echo "needs pango-view (pango)"; exit 1; }
command -v chromium >/dev/null || { echo "needs chromium (print-to-pdf)"; exit 1; }
python3 -c "from PIL import Image" || { echo "needs python-pillow"; exit 1; }

if [[ -e "$TB" ]]; then
  echo "$TB already exists; delete it first (rm -rf \"$TB\")"
  exit 1
fi
mkdir -p "$TB"

echo "== synthetic/ (Linux: Wikimedia + pango + TTS, not macOS samples) =="
python3 -I "$REPO/scripts/testbed/make_eval_corpus.py" "$TB"

S="$TB/synthetic"
T="$TB/traps"
mkdir -p "$T"

echo "== traps/ =="
mkdir -p "$T/ts-code" "$T/real-mpeg-ts"
printf 'export const greet = (name: string): string => `hello ${name}`;\n' > "$T/ts-code/app.ts"
printf 'import { greet } from "./app";\nconsole.log(greet("zebra"));\n' > "$T/ts-code/main.mts"
ffmpeg -loglevel error -y -i "$S/video/penguin_zebra_parrot_18s.mp4" -c copy -f mpegts "$T/real-mpeg-ts/zoo clip.ts"

P="$T/MyProject"
mkdir -p "$P/.git" "$P/assets" "$P/node_modules/lib" "$P/screenshots"
echo '{"name": "my-project"}' > "$P/package.json"
cp "$S/img/Animals-Owl.jpg" "$P/assets/hero.jpg"
cp "$S/img/Animals-Eagle.jpg" "$P/node_modules/lib/logo.jpg"
cp -p "$S/screenshots/shot_weather.png" "$P/screenshots/Screenshot 2026-10-02 at 8.15.40 AM.png"

mkdir -p "$T/node_modules/pkg" "$T/.hidden" "$T/Fake.app/Contents/Resources"
cp "$S/img/Nature-Leaf.jpg" "$T/node_modules/pkg/leaf.jpg"
cp "$S/img/Nature-Zen.jpg" "$T/.hidden/zen.jpg"
cp "$S/img/Fun-Medal.jpg" "$T/Fake.app/Contents/Resources/medal.jpg"

mkdir -p "$T/icons"
for px in 32 64 128; do
  vips thumbnail "$S/img/Sports-Target.jpg" "$T/icons/target-$px.png" "$px"
done

mkdir -p "$T/dataset_clips"
ffmpeg -loglevel error -y -f lavfi -i "sine=frequency=440:duration=1" -ac 1 -ar 16000 "$T/dataset_clips/clip_000000.wav"
for i in $(seq -f "%06g" 1 599); do
  ln "$T/dataset_clips/clip_000000.wav" "$T/dataset_clips/clip_$i.wav"
done

N="$T/Names with spaces & ünïcödé"
mkdir -p "$N"
cp -p "$S/screenshots/shot_chat_meeting.png" "$N/Screenshot 2026-10-05 at 9.41.07"$'\u202f'"PM.png"
cp "$S/img/Flowers-Sunflower.jpg" "$N/Café menü — 日本語.jpg"

mkdir -p "$T/duplicates"
cp "$S/img/Animals-Penguin.jpg" "$T/duplicates/penguin.jpg"
cp "$S/img/Animals-Penguin.jpg" "$T/duplicates/penguin copy.jpg"

# Linux crawler traps (Phase 2).
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
mkdir -p "$T/linux/unreadable"
echo x > "$T/linux/unreadable/x.txt"
mkdir -p "$T/linux/loop/a" "$T/linux/loop/b"
echo x > "$T/linux/loop/a/note.txt"
ln -sfn "$T/linux/loop/a" "$T/linux/loop/b/to-a"
ln -sfn "$T/linux/loop/b" "$T/linux/loop/a/to-b"

echo "== code/ =="
python3 -I "$REPO/upstream/scripts/testbed/make_code.py" "$TB/code"

cat > "$TB/README.md" <<'EOF'
# Trowel eval testbed (Linux)

Built by `scripts/make-testbed.sh`. Delete any time. Default path is `/tmp/trowel-eval`
so a code crawl does not inherit this git repo.

- `synthetic/`: Wikimedia photos, rendered UI screenshots, TTS, penguin/zebra/parrot video,
  docs/PDFs, Bengali/Arabic (`lang/`), long files (`long/`).
- `traps/`: TypeScript vs MPEG-TS, project screenshots-only, dataset, odd names, duplicates,
  Linux name traps.
- `code/`: four small git repos for `evals/code.json`.
- No `real/` sample of $HOME.
EOF

echo "== done: $TB =="
du -sh "$TB" 2>/dev/null
echo "index with:"
echo "  trowel index $TB/synthetic $TB/traps --index $TB/index --exclude $TB/traps/linux"
echo "  trowel index $TB/code --code --index $TB/index"
echo "  trowel eval evals/synthetic.json --index $TB/index --base $TB"
