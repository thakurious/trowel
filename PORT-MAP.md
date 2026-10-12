# Port map — every DigUp component → Linux

Legend: **[P]** port logic 1:1 · **[R]** replace with a native equivalent · **[K]** keep (portable) ·
**[D]** drop · **[N]** new on Linux.

---

## A. `Sources/DigUpKit` (the engine)

| DigUp file | Linux | Notes |
|---|---|---|
| `Database.swift` | **[P]** Rust `rusqlite` | Thin wrapper; `busy_timeout`, `WAL`, `synchronous=NORMAL`, `foreign_keys=ON`, `BEGIN IMMEDIATE` transactions. |
| `IndexStore.swift` | **[P]** Rust | **Schema, indexes, `fts5(...)` tokenizer string, migrations v1–v5 reproduced verbatim.** `scope` SQL (prefix range on `path`), `pending` ordering, `addSegment`, `keywordText`, `nameText`, `folderText`, `counts`. |
| `Model.swift` | **[K]** | `FileKind`, `SegmentKind`, `Modality`, `IndexOptions` + `fingerprint`, `Prompts`, `CodeQuery`. Pure constants/enums — copy to Rust enums with the same string values (they're stored in the DB). |
| `Vectors.swift` | **[P/N]** | fp16 storage; blocked fp16→fp32 + matmul. Rust: `half` + `matrixmultiply::sgemm` (or `cblas_sgemm`; `cblas` present). Keep `blockRows = 1024`. `mmap` via `memmap2` so `munmap` truly frees. |
| `VectorStorage` (in `Vectors.swift`) | **[P]** | mmap'd `rows×768` fp16 + one 1024×768 fp32 block. |
| `SearchText.swift` | **[P]** | NFKC (`unicode-normalization`), zwj/direction-mark stripping, Arabic alef/tatweel/harakat rules, `inflects`, `hasWords`, `matches`, `snippet`, `Landmark`, `PageTextProblem` (glyph-order / lost-letters detection). Must be byte-faithful — it drives FTS input. |
| `Rules.swift` | **[P/N]** | Classification, project markers, source/code extensions, secret detection, dataset detector, machine-named detector, MPEG-TS sniffer. **Linux changes:** screenshot detection has no `kMDItemIsScreenCapture` xattr. Use filename prefixes, including **Omarchy's `screenshot-YYYY-MM-DD_HH-MM-SS.png`** (`omarchy-capture-screenshot`) and the grim/hyprshot/flameshot/GNOME/KDE patterns, plus a monitor-resolution heuristic; `~/Library` → skip `~/.cache`, `~/.local/share/Trash`, `~/.var`, flatpak/snap dirs; add `~/.local/share/Steam`, `node_modules`, `.venv`, `target`, `.git`. |
| `FolderSelection.swift` | **[K]** | Pure path logic; port to Rust (`toRealPath` via `canonicalize`, keeping the "walk reports real path" caveat). |
| `Crawler.swift` | **[P/N]** | Walk with `walkdir` (skip hidden + no symlinks), lstat, classify. Git-ignore honoring: **[R]** shell out to `git ls-files -z --cached --others --exclude-standard` (same command; `git` present) instead of the hardcoded macOS git discovery. |
| `Extractors.swift` → `ImageExtractor` | **[R]** `libvips` (+`kamadak-exif`) | vips `thumbnail` shrink-on-load (like ImageIO) → screenshots 1536 PNG, photos 1024 JPEG, frames 1024 JPEG, PDF pages 1400 PNG. vips handles HEIC/RAW/WebP/AVIF (RAW via dcraw present; add `libheif` for HEIC). EXIF `DateTimeOriginal` via `kamadak-exif`. |
| `Extractors.swift` → `OCR` | **[R]** `tesseract` | Full-res screenshot OCR; `-l eng+…` from installed packs. Accurate-vs-fast: default to accurate (`--oem 1`). Note: OCR affects **only** exact-word-in-screenshot, not semantics. |
| `Extractors.swift` → `PDFExtractor` | **[R]** `poppler` (`pdftotext -layout`, `pdftoppm -png -r …`, `pdfinfo`) | Per-page text; render low-text/garbled pages; `PageTextProblem` checks preserved; title from `pdfinfo`. Password (`isLocked`) detection via `pdfinfo` "Encrypted". |
| `Extractors.swift` → `DocExtractor` | **[R]** native + optional `pandoc` | txt/md direct; HTML strip **[P]**. `docx`/`odt` via Rust (`docx-rs`, `odt` XML) — no Cocoa; `rtf` via `rtf-parser`/`unrtf`; `doc` legacy via `pandoc` or `antiword` if present, else skip with a reason. Chunker **[P]** (1800/200/limit). |
| `MediaExtractors.swift` → `AudioExtractor` | **[R]** `ffmpeg` | Decode to 16 kHz mono s16le (`ffmpeg -i in -f s16le -ac 1 -ar 16000 -`), stream over stdout; implement the 30 s window / 25 s hop, RMS silence gate, 2 s minimum, temp WAV write **[P]**. Works for any length, streaming. |
| `MediaExtractors.swift` → `VideoExtractor` | **[R]** `ffmpeg` | Duration/streams via `ffprobe -show_streams -show_format -of json`. Keyframes: `ffmpeg -vf fps=1/3[,scale=…]` (or `-skip_frame nokey` + scene detect) → PNG/JPEG; **8×8 average hash + Hamming ≤ 4 dedupe [P]**; frame "end = next kept frame". Audio track → AudioExtractor path. `MediaInfo.duration` via ffprobe. |
| `CodeExtractor.swift` | **[K/P]** | Stretch algorithm, notebook cells, `place` (repo detection), language table, `CodeLookup` (identifier detection, definition-strength, first-line). Pure string logic — port to Rust. `git`/`.git` checks same. |
| `Worker.swift` | **[K]** | The `Embedder` trait + `WorkerInfo`/`vectorSource` + JSON-lines client. Port the client to Rust (`serde_json`); keep the exact wire format and the zero-vector+error-per-item behavior. |
| `Indexer.swift` | **[P]** | Sync reconcile (path/inode/size/mtime/kind; move detection), `run` with prepare/commit overlap, batching, per-file state machine, `finishLongFiles`, `checkModel`/`adopt` fingerprint reset, `stopReading`. Concurrency: Rust threads + channels (a `crossbeam`/`std::sync::mpsc` double-buffer replacing `DispatchQueue` + `PendingBatch`). |
| `Searcher.swift` | **[P]** | **The math is the product** — reproduce verbatim: `score` blocked matmul, `meaning` per-modality z-score, `modalityMeans`, `reload`/`releaseVectors`, keyword FTS query + bm25, coverage, `isLookup`, fusion weights, `kindHints`, `strongCount`, `codePlace`. |
| `Estimator.swift` + `Costs`/`CodeIndexSize` | **[P/N]** | Same cost model. `pageCount`: `pdfinfo`; `duration`: `ffprobe`; the macOS Spotlight lookup (`MDItemCreate`) → `[R]` direct file-header probes (poppler/ffprobe), no Spotlight. |
| `ModelFiles.swift` | **[P/N]** | Same repo/revision/names/sizes/SHA-256. Path → `$XDG_DATA_HOME/trowel/models` (default `~/.local/share/trowel/models`); optional `$TROWEL_MODELS` override kept. HF-cache discovery kept (`~/.cache/huggingface/hub`). |
| `CLlama/module.modulemap` | **[D]** | Replaced by the C++ worker's own build. |

## B. `Sources/LlamaRuntime` (the runtime)

| DigUp file | Linux | Notes |
|---|---|---|
| `LlamaEmbedder.swift` | **[P]** → C++ `trowel-worker` | Same framing, `mtmd` usage, mean-pool + L2-normalize, `maxSequences=16`, `n_ctx` 2048/512, `image_min_tokens=1`, audio 30 s cut, batch APIs. Vulkan instead of Metal. |
| `LlamaModels.swift` | **[P]** | Locate the two GGUFs in the same folders. |
| `ImageSizing.swift` | **[P]** C++ | Pillow-exact bicubic + target-size math, 1:1 (it's already a port of mtmd's code). |
| `scripts/build-llama.sh` | **[R]** | Arch build of llama.cpp at the **same pinned tag** with the **same `llama.cpp-no-logits.patch`**, `-DGGML_VULKAN=ON -DLLAMA_BUILD_MTMD=ON`, static libs. Pin in `PKGBUILD`/`vendor/`. |
| `WorkerService.swift` | **[P]** C++ | The `trowel worker` JSON-lines loop (base64 f32 vectors, per-item errors). |

## C. `Sources/digup` (CLI)

| DigUp file | Linux | Notes |
|---|---|---|
| `main.swift` | **[P]** Rust `clap` | Same verbs & flags (`--index`, `--code`, `--kind`, `--exclude`, `--exclude-ext`, `--reindex`, `--zgate`, `--word-weight`, `--shot-tokens`, `--code-stretch`, `--limit`). Default index dir `~/.local/share/trowel/index`. |
| `IndexService.swift` | **[P]** Rust | Same stdin/stdout event protocol, same loop (sync → new files → backfill → code → finishLongFiles → linger → exit), same `missing`/`unreadable` root handling, scratch-folder cleanup, `SharedEmbedder`. |
| `WorkerService.swift` | **[P]** C++ | See B. |

## D. `Sources/DigUpApp` (UI) — reimplemented, not ported

| DigUp file | Linux | Notes |
|---|---|---|
| `App.swift`, `AppController.swift`, `AppModel.swift` | **[R]** daemon + UI clients | Lifecycle splits: `troweld` owns the engine; UI clients attach. Status model → IPC `status` events. |
| `AppSettings.swift` | **[R]** `config.toml` | Field-for-field (PROTOCOL §5); keep `batteryTitle`/`batteryDetail` copy; default hotkey becomes `SUPER + D`. |
| `Engine.swift` | **[R]** Rust `troweld` | Watcher, helper restart, debounce, searcher refresh (≤ every 2 s), encoder idle exit, embed-ahead + `QueryVectorCache(256)`, `prewarm`, code-searcher. |
| `StatusMenu.swift` | **[R]** omarchy-shell `bar-widget` (in the `trowel` plugin) | Icon states (idle/progress ring/paused/update-dot) + popup menu (search, open, pause, choose folders, settings, quit). |
| `SearchPanel.swift` + `SearchPanelView.swift` | **[R]** omarchy-shell `panel` plugin (QML) | UI-DESIGN §3: 780/330 px, top at 16 %, two-phase search (120 ms), grouping/folding, preview column, the `navigated` Space rule, focus-loss hide. |
| `SearchWindow.swift` | **[R]** `trowel-app` `FloatingWindow` | Grid + filters + preview pane + home/examples + status bar. Same ⌘(=Ctrl) shortcuts. |
| `Results.swift`, `Moments.swift` | **[R]** shared result model | `ResultRow` fields, `reason`, `place`, `focus`, page/moment jump semantics. |
| `Previews.swift`, `Thumbnails.swift` | **[R]** daemon-generated previews/thumbnails (PROTOCOL §4) + QML `PreviewPane`; Sushi fallback | Image, PDF page (with matched band), code lines (focus line highlighted), document passage, audio waveform, video frame; **Sushi** for generic Quick Look; QtMultimedia `MediaPlayer` in `trowel-app` for playback at the moment (`mpv --start=` for external open). |
| `Onboarding.swift` | **[R]** `trowel-app` QML wizard | Same 5 steps (welcome/folders/code/shortcut/ready), same copy/estimates. |
| `SettingsWindow.swift`, `FolderChoices.swift`, `AddFolders.swift`, `CodeFolders.swift`, `Estimates.swift` | **[R]** `trowel-app` QML settings window + sheets; folder picking via the portal FileChooser | Tabs General/Folders/Code/Storage; folder checkboxes with per-folder estimates via `trowel estimate --json`. |
| `Hotkey.swift` | **[R]** Hyprland Lua bind + omarchy-shell IPC | The recorder writes `~/.config/hypr/trowel.lua` after a conflict check against `omarchy menu keybindings --print` (UI-DESIGN §8). |
| `FolderWatcher.swift` | **[R]** `notify` crate | inotify; same hidden-file filtering and debounce. |
| `PowerPolicy.swift` | **[R]** sysfs/UPower/ppd | AC vs battery `/sys/class/power_supply`, thermal via ppd profile / `thermal_zone`, low-power via ppd. Event-driven via UPower/ppd DBus signals. |
| `CodeEditors.swift` | **[R]** PATH detection | `cursor`, `code`/`codium`, `zed`, `subl`, `nvim --server … --remote`/`nvr`, `hx`, `kate --line`, `idea/clion/… --line`; store choice. Detect: `cursor`, `nvim`, `hx` present here. |
| `ModelDownload.swift` | **[R]** Rust `reqwest` | Same files/revision/hashes, curl-style Range resume, `.partial` + rename after SHA-256, retry/backoff, free-space check, progress events. |
| `Updates.swift`, `appcast.xml` | **[D]** | pacman/AUR. |
| `DebugHooks.swift`, `Snapshot.swift` | **[R] (partial)** | Headless test harness: a `trowel debug-*` set + a UI snapshot tool for regression. |
| `Components.swift`, `AppLog.swift` | **[R]** | Keycap/PlaceBadge/MomentBar/etc. → `ui/qml/Trowel` (UI-DESIGN §6); brand colors only for icon/onboarding; logging to journald + `<index>/trowel.log`. |
| `Info.plist`, `*.entitlements`, `AppIcon.*` | **[R]** | `.desktop` file, AppStream metainfo, icon; no entitlements (works without Accessibility — use the portal/Hyprland bind). |

## E. Supporting assets

| Asset | Linux |
|---|---|
| `evals/*.json` | **[K]** Reuse verbatim (synthetic, code, multilingual) — they are the acceptance bar. |
| `Tests/LlamaRuntimeTests/Reference/{photo.jpg,screen.png,speech.wav,vectors.json}` | **[K]** Reuse **verbatim** as the vector-parity oracle. |
| `scripts/make-testbed.sh` + `scripts/testbed/*` | **[P]** Port to bash/python: ffmpeg media, PDFs/docs, Bengali/Arabic, 600-clip dataset, code repos. Apple-only bits (`sips`, `xattr`, APFS clone `cp -c`) → `vips`/`ffmpeg`/plain copy. |
| `worker/digup_worker.py` (mlx-vlm reference) | **[D]** (Apple/MLX). Keep only as documentation of the protocol. |

---

## Fidelity invariants (must not drift)

1. **Vectors.** Same GGUF (ggml-org Q8_0, pinned rev `bfcd298…`), same framing (bos/eos, mtmd marker,
   mean-pool, L2), same image sizing. Gate: `ReferenceVectorTests` cosine ≥ **0.999** against
   `Tests/.../Reference/vectors.json`, on this Linux build. Keep a `vectorVersion` integer.
2. **FTS tokenizer string** verbatim: `porter unicode61 remove_diacritics 2 categories 'L* N* Co M*'`.
3. **Normalization** (`SearchText.normalized`) and the stopword/kind-word lists verbatim.
4. **Search math**: `zGate 2.0`, `wordWeight 2.0`, `codeWordWeight 1.0`, `coverage^1.5`, `bm25(3,1)`,
   modality-mean +/- overall-std z-score, `setsScale` exclusion, `strongCount 0.5`, `isLookup` rule.
5. **Prompts**: `task: search result | query: `, `task: code retrieval | query: `,
   `title: T | text: B` (`none` when no title).
6. **Index fingerprint**: `<vectorSource> | <options fingerprint>`; a change resets the affected index.
7. **Chunk/stretch/window parameters**: doc chunks 1800/200; code stretch 1800, maxCodeBytes 1 MiB,
   maxNotebookBytes 20 MiB; PDF first step 30 / big 50 / max 5000, minPageText 200; audio 30/25;
   frame interval 3 s, hash threshold 4; image budgets photo 140, screenshot 280, page 280, frame 70.

## Linux deltas (new on Linux, kept out of upstream's schema; see D16)

| Item | Where | Why |
|---|---|---|
| `linux_raw_paths(file_id PK → files, raw BLOB)` | index DB | A non-UTF-8 path is stored lossily in `files.path` (with U+FFFD) for display and FTS. The raw bytes are kept here and used for every filesystem operation (ROBUSTNESS F15, PROTOCOL §6). |
| `linux_attempts(file_id PK, n, last_at)` | index DB | Poison-file journal (ROBUSTNESS F5c). |
| `meta.linux_poll_roots` | index DB | Roots on polling fallback (F3/F4). |
| `meta.worker_backend`, `meta.parity_selftest` | index DB | Which backend embedded this index, and the last self-test result (F6/F14). |
| Hard-skip list additions | `Rules` | `~/.ssh ~/.gnupg ~/.password-store ~/.local/share/keyrings ~/.local/share/Trash ~/.cache ~/.var ~/.steam ~/.local/share/Steam`, flatpak/snap dirs, `target/`, `.venv/` (F17). |
| Thumbnail/preview cache | `~/.cache/trowel/` | Generated by the daemon. |
| Path normalization | everywhere | macOS paths come back NFD from APFS while Linux paths are verbatim bytes. **Don't** normalize paths (only search text). Upstream's `scope` prefix-range SQL works unchanged on raw UTF-8. |
