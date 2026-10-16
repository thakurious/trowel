# Architecture

Two sections: **A. how DigUp works today** (verified by reading every source file), and **B. the
target Linux architecture**. Read A first — the Linux design is a deliberate mirror of it, because
its decisions are battle-tested and its comments explain *why* against measured evals.

---

## A. How DigUp works today (macOS)

### A.1 Process model — three executables, two helper processes

`Package.swift` builds:

- **`DigUpApp`** — the SwiftUI/AppKit menu-bar app (`Sources/DigUpApp`). Owns the `Searcher`, the
  query encoder, and the index-service helper. Stays small while idle.
- **`digup`** — developer CLI (`Sources/digup`): `estimate | index | search | status | eval |
  index-service | worker`. Doubles as the app's two helpers.
- **`DigUpKit`** — the engine, no UI (`Sources/DigUpKit`).
- **`LlamaRuntime`** — EmbeddingGemma 2 on llama.cpp, linked in-process (`Sources/LlamaRuntime`).

Two deliberate helper-process boundaries exist so heavy memory returns to the OS:

1. **Indexing helper** — `digup index-service`, spawned by the app (`Engine.startHelper`). All
   extraction (Vision OCR alone caches ~100 MB) and embedding happen there; the process exits after
   `linger` seconds idle. Protocol: one command per line on stdin (`sync`, `sync code`, `stop`,
   `backfill on/off`, EOF), one JSON event per line on stdout (`IndexService.swift`).
2. **Query encoder** — `digup worker --text-only`, spawned lazily when the search panel opens and
   killed after `encoderIdleSeconds` (600 s) idle, so the ~0.25 GB text model leaves with it
   (`Engine.scheduleIdleExit`). Protocol: JSON lines (see A.7).

The app itself runs three serial queues: `indexQueue` (utility, talks to the helper), `searchQueue`
(user-initiated, owns `Searcher` + a read-only SQLite connection), `encoderQueue` (user-initiated,
owns the query encoder). WAL mode lets `searchQueue` read while the helper writes. The main thread
never blocks.

### A.2 Data model — one SQLite file per index, fp16 vectors inline

`IndexStore` (`DigUpKit/IndexStore.swift`), schema v5, WAL, `synchronous=NORMAL`:

- `meta(key,value)` — `fingerprint` (`"<vectorSource> | <options fingerprint>"`) and `vector_source`.
- `files(id, path UNIQUE, kind, size, mtime, inode, device, state, detail, info JSON, indexed_at)`.
  `state ∈ {pending, done, partial, skipped, failed}`. Indices on `(device,inode)` and `(state,kind)`.
- `segments(id, file→files ON DELETE CASCADE, kind, modality, loc, loc_end, excerpt, vec BLOB)`.
  `vec` = **768 × float16, L2-normalized** (1.5 KB each). `SegmentKind ∈ {name, image, ocr, page,
  page_image, chunk, audio, frame, lines, cells}`; `Modality ∈ {none, text, image, audio}`.
- `fts` — FTS5 virtual table: `fts5(name, body, original UNINDEXED, tokenize = "porter unicode61
  remove_diacritics 2 categories 'L* N* Co M*'")`. `name` = file/folder keywords; `body` = normalized
  segment text; `original` = the text as written when normalization changed it (for display).
  The custom `categories` keep Bengali vowel signs / Arabic harakat *inside* words (default SQLite
  splits them).

Two files: `index.sqlite` (main) and `code.sqlite` (code index — a *separate* index so code never
mixes with other results and turning it off deletes one file). Migrations are additive and each
queues work rather than re-reading where avoidable (`migrateKeywordSearch` rebuilds FTS from stored
text; `migrateLongFiles` reclassifies).

### A.3 Crawl → classify → reconcile

`Crawler.crawl` walks the chosen roots (`FileManager.enumerator`, hidden files and package
descendants skipped) and applies `Rules`:

- **Files**: `Rules.classify` (or `classifyCode` for the code index). Handles the `.ts`/`.mts`
  ambiguity (TypeScript vs MPEG-TS, sniffed from 0x47 sync bytes), screenshot detection
  (macOS extended attribute `com.apple.metadata:kMDItemIsScreenCapture`, else name prefixes).
- **Code projects**: a folder with `.git`/`package.json`/`Package.swift`/… is a project; inside one,
  only screenshots are indexed for the main index. A dataset detector flags folders of ≥300
  machine-named media files (`Rules.datasetFolders`).
- **Skips**: `node_modules`, `venv`, `__pycache__`, `~/Library`, `.noindex`, app bundles, in-cloud
  (iCloud) dataless files, secrets (`.pem/.key/.env/id_rsa/…`), >1 MB source files in code index,
  minified/generated ("DO NOT EDIT", "@generated", `PRIVATE KEY-----`).
- Only lstat-regular files: symlinks/special files skipped. inode+device recorded.

`Indexer.sync`: builds `byPath` and `byInode` maps, then in one transaction:
new path → insert (pending); same path, changed (size/mtime/kind) → requeue; **same inode+size+mtime
at a new path → `move` (re-path, no re-embed)**; path gone → delete. A root that vanished mid-crawl
(an ejected drive) is *not* treated as "deleted". Returns `SyncSummary{added,changed,moved,removed,
unchanged, queued}`.

### A.4 Extraction (per file kind)

`Indexer.prepare` runs on a utility queue, **one batch ahead of** the embedding/commit stage, so
reading and embedding overlap (`Indexer.process`). Batches: images/screenshots 8 at a time, others
one file. Per kind:

- **Image/screenshot** (`ImageExtractor`): ImageIO decodes **straight to a downscaled thumbnail**
  (screenshots 1536 px PNG, photos 1024 px JPEG; min side 256), reads EXIF `DateTimeOriginal`.
  Screenshots additionally get **OCR** (Apple Vision, `.accurate`, on the *full-res* image) for
  keyword search only — the OCR text gets **no vector** (a vector of OCR words made screenshots
  outrank better matches; schema-3 migration strips them).
- **PDF** (`PDFExtractor`): per page, PDFKit text; a page with <200 chars of text or a garbled text
  layer is **rendered** (1400 px) for the vision encoder. Text-layer problems are detected
  (`PageTextProblem`): **glyph-order** (Bengali/Devanagari vowel signs displaced → use the picture +
  a "garbled" meaning-only text vector) and **lost-letters** (Arabic lam-ligature loses letters →
  re-OCR if Arabic OCR is available). Pages that are mostly digits get keyword search only
  (`SearchText.hasWords`).
- **Document** (`DocExtractor`): txt/md direct, HTML tag-stripped, rtf/doc/docx/odt via Cocoa.
  Split into ~1800-char chunks with 200-char overlap at word boundaries.
- **Audio** (`AudioExtractor`): AVFoundation decodes to 16 kHz mono; **overlapping 30-s windows,
  25-s hop**, silent windows (< RMS −50 dBFS) skipped; each window written as a temp WAV.
- **Video** (`VideoExtractor`): a frame every `frameInterval` (3 s) via AVAssetImageGenerator, one
  kept per shot (8×8 average hash, Hamming ≤ 4 = near-duplicate); plus the audio track as above.
  Each kept frame "stands for" the stretch until the next kept frame.
- **Code** (`CodeExtractor`, code index only): stretches of whole lines ≤1800 chars, cut at natural
  break points (blank line / new block), each tagged with `first`/`last` line (or notebook cell);
  title = the file's path within its repo. Git-ignores honored via `git ls-files -z --cached
  --others --exclude-standard`.

### A.5 Embedding & the index fingerprint

`Embedder` is a protocol (`DigUpKit/Worker.swift`): `embedTexts`, `embedImages(paths, budget)`,
`embedAudio(paths)`, `info`. Two backends: `LlamaEmbedder` (in-process llama.cpp, production) and
`EmbeddingWorker` (a worker process over JSON lines — the mlx-vlm reference or the bundled helper).

`LlamaEmbedder` (`LlamaRuntime/LlamaEmbedder.swift`) links llama.cpp + `mtmd`:

- Context: `n_ctx = n_batch = n_ubatch = 2048` (512 for text-only), `embeddings = true`,
  `pooling = MEAN`, `kv_unified = true`, `n_gpu_layers = 999` (Metal; → Vulkan on Linux).
- **Framing** (must match the reference exactly):
  - text: `<bos> text <eos>`, cut to `maxTokens` keeping both special tokens.
  - image/audio: `mtmd_default_marker()` with `add_special=true, parse_special=true`; the encoder
    rows are injected as `llama_embd` into the batch via `mtmd_encode_chunk` + `mtmd_get_output_embd`.
  - The model is non-causal with no KV cache; each input goes through **one `llama_process` call**
    (`LLAMA_PROCESS_TYPE_ENCODE`), each input its own sequence (`llama_batch_ext_*`). Output is
    **mean-pooled then L2-normalized**.
- Image sizing (`ImageSizing`): transformers' Gemma-4 image processor — scale to fill the soft-token
  budget (70/140/280/560/1120), aspect kept, each side floored to a multiple of 48 px, with a
  **Pillow-exact bicubic** resize (a=−0.5, 22-bit fixed point). mtmd is told `image_min_tokens=1` so
  it never rescales.
- Audio past 30 s is truncated to 30 s (the model ignores the rest of one input).
- A `scripts/llama.cpp-no-logits.patch` makes the context skip reserving/clearing `n_vocab` logits
  per token (1 MiB/token with a 262k vocab), for `embeddings && !causal_attn`.

**Index fingerprint.** `WorkerInfo.vectorSource = "<model>@<rev7> <runtime> <dtype>"`, and the model
reports `runtime = "llama.cpp-vectors<1>"`. `LlamaEmbedder.vectorVersion` is bumped only when the
*vectors change* (not for every llama.cpp bump). `IndexStore.fingerprint = "<vectorSource> |
<options fingerprint>"`; a mismatch empties the index and re-embeds (or throws in the CLI).
`ReferenceVectorTests` asserts cosine ≥ 0.999 against stored vectors on every llama.cpp update — this
is the contract that lets a runtime upgrade keep every user's index.

### A.6 The indexing pipeline

`Indexer.run` takes `pending` files ordered by `FileKind.indexingOrderSQL` (screenshot, image, pdf,
doc, audio, video, code) then **mtime DESC** (newest first). For each job it does `prepare` (stage 1,
utility queue) then `commit` (stage 2, index thread: model + DB), overlapping the next job's prepare
with the current commit. `shouldStop` is polled between jobs (pause / new files).

**Long files in two passes.** A PDF > 50 pages contributes only its first 30 pages in the first pass
(state `partial`, `info.read_to/read_end`); a document its first 40 chunks. After everything is
pending-free, `finishLongFiles` reads the rest a step at a time (pdf 30 pages / doc 40 chunks), each
step one transaction, resumable, so a book's tail never delays everything else's first page. Caps:
5000 pages / 5000 chunks.

Per-file state machine: `done` / `partial` (searchable, more to read) / `skipped` / `failed`, always
recorded on the file so a run resumes. The code index is processed after the main index is quiet.

### A.7 Worker protocol (JSON lines)

`worker/digup_worker.py` documents it and `Sources/digup/WorkerService.swift` implements it:

```
{"id":1,"op":"info"}
{"id":2,"op":"text","inputs":["task: search result | query: cat", ...]}
{"id":3,"op":"image","inputs":["/tmp/a.jpg"],"budget":280}
{"id":4,"op":"audio","inputs":["/tmp/a.wav"]}
{"id":5,"op":"quit"}
→ {"id":2,"ok":true,"dim":768,"count":N,"vectors":"<base64 f32 LE, N×768>","errors":[...]}
```

First line out is `{"ready":true,"info":{...}}`. A failed *item* gets a zero vector + a message in
`errors`; a failed *request* gets `{"ok":false,...}`. Text arrives **already prompted** (prompting is
the caller's business).

### A.8 Search — hybrid, per-modality z-scored, with a "words rescue"

`Searcher` (`DigUpKit/Searcher.swift`). Two signals:

**Meaning.** All vectors are loaded into an **mmap'd fp16 matrix** (`VectorStorage`; `munmap` returns
memory immediately, unlike malloc). A query vector is scored against every row by a **blocked
matmul**: convert 1024 rows fp16→fp32 (`vImageConvert_Planar16FtoPlanarF`), `vDSP_mmul` `(1024×768)·
(768×1)`, repeat. Scores are then **normalized per query**: subtract each vector's **modality mean**
(text/image/audio vectors sit at different levels for the same query), then divide by the **overall
standard deviation** of the centered scores → a z-score. Only `z ≥ zGate` (2.0) counts as a meaning
match. Vectors added by the *later* pass of a long file (`finishLongFiles`) are scored but **do not
set the scale** (a book's hundreds of pages otherwise pull the text mean down for every query).

**Words.** FTS5 `MATCH` over names, folder names, OCR, page and chunk text, ranked by
`bm25(fts, 3.0, 1.0)` (name weighted 3×). `Searcher.terms` normalizes the query (`SearchText`),
drops stopwords and kind-words, and builds quoted terms (+ `*` prefix for the last word and for
inflecting scripts — Bengali/Arabic). Up to 100 files kept per match. Word "coverage" = fraction of
query terms found anywhere in the file.

**Fusion.** A file qualifies by `z ≥ zGate` **or** covering ≥ half the query's words. Score =
`meaningWeight·max(z,0) + wordsWeight·coverage^1.5 + 1 if a kind hint matched`. Lookups (codes, IDs,
long numbers — no embedding value) flip to `meaningWeight 0.5, wordsWeight 2·wordWeight`. After
ordering, `strongCount` folds everything below **0.5× the top score** into "weaker matches".

Prompts (`Prompts`): query `task: search result | query: X`; document `title: T | text: B`; code query
`task: code retrieval | query: X`.

### A.9 `SearchText` — Unicode correctness

`DigUpKit/SearchText.swift` is subtle and matters for multilingual parity:

- `normalized`: NFKC compatibility decomposition; drops zero-width joiners/direction marks/BOM; maps
  the several Arabic alefs to one; strips tatweel and harakat; treats ZWSP as a word break.
- `inflects`: Indic/Arabic words match as **prefixes** (so "কুমির" finds "কুমিরও"). The FTS tokenizer
  `categories` string keeps combining marks in-word.
- `hasWords`: ≥ 1/5 of non-space scalars are letters → worth an embedding; else keyword-only.
- `matches`/`snippet`: locate query words in the *original* text (for highlighting) through a
  per-character folded index, and build the `«…»`-marked snippet used in results.
- `Landmark`: find a word on a PDF page that isn't on an earlier page, to hand a viewer a search
  string.

### A.10 The macOS app (what the Linux UI must reproduce)

`Sources/DigUpApp` — 29 files, documented in `TESTING.md`/`DECISIONS.md`. Highlights:

- **Quick panel** (`⇧⌘Space`, `SearchPanel`): 780 px wide, Spotlight placement; two-stage search
  (instant keyword with `prefixLast`, then hybrid after a 120 ms typing pause); per-keystroke
  **embed-ahead** keeps the GPU warm and caches the vector; results grouped by kind, weak ones folded;
  Space = Quick Look **at the matched page/moment**, ↩ open, ⌘↩ reveal, ⌘C copy, ⌘O open in window.
- **Main window**: 1040×700 grid + filters (All/Screenshots/Images/Documents/Video/Audio/Code) +
  preview pane (image, PDF page with the matched band highlighted, code lines, document passage,
  audio waveform, video frame/playback at the matched moment).
- **Menu bar**: status (indexing %, finishing long files, waiting-for-model, missing/unreadable
  roots, paused, reason), pause/resume, choose folders, check for updates.
- **Onboarding/Settings**: folder picker with per-folder estimates (`digup estimate`), skips, code
  search opt-in, hotkey recorder, editor picker, battery switch, model download, storage/privacy.
- **Behaviors**: FSEvents watcher (debounced 0.75 s; code watcher 2 s quiet, ≤10 min max), power
  policy (battery/thermal/Low-Power gating of the *backfill* only — new files always index),
  code-editor integration, Quick Look, Sparkle updates, login item.

---

## B. Target Linux architecture

Design goal: **same semantics, same vectors, same search math**; Linux-native processes and UI.
Mirror the three-process design because its memory discipline is exactly right for a resident
indexer on a laptop.

```
┌──────────────────────────── UI (Wayland, all QML) ───────────────────┐
│  omarchy-shell plugin `trowel`: quick panel (SUPER+D) + bar widget     ┼──┐
│  trowel-app (own Quickshell): main window, Quick Look, Settings, Onb.  ┼──┤
│  trowel CLI / test clients                                             ┼──┤
└──────────────────────────────────────────────────────────────────────┘  │
                          $XDG_RUNTIME_DIR/trowel/troweld.sock (PROTOCOL.md)│
┌──────────────────────────── troweld (Rust daemon) ────────────────────┐  │
│  watcher · Searcher (brute-force fp16) · SQLite (read) · previews     │◄─┘
│  supervisor (backoff, poison files, parity self-test) · config owner  │
│  spawns/owns:                                                         │
│    • trowel index-service   (Rust): crawl+extract+embed, per-task      │
│    • trowel worker [--text-only] (C++): llama.cpp VULKAN + mtmd        │
│  model memory lives ONLY in the worker; freed on idle                 │
└───────────────────────────────────────────────────────────────────────┘
        │ inotify                    │ Vulkan/Mesa ANV (Arc 140V)
   ~/Desktop ~/Documents ...      EmbeddingGemma 2 Q8_0 GGUF (text + mmproj)
```

- **`troweld`**: the resident Rust daemon. It runs the watcher and the read-side `Searcher`, owns
  `config.toml`, generates thumbnails and previews, and supervises the indexing helper and the query
  worker (ROBUSTNESS.md). It serves every UI over the socket in `PROTOCOL.md`. It holds no model and
  releases the vector matrix when idle (as the app releases it with the encoder).
  **Single-writer rule:** only `trowel index-service` writes the index DBs. The daemon opens them
  read-only (WAL), except for config-driven deletes (code search off → delete `code.sqlite`), which
  happen only while the helper is stopped.
- **`trowel index-service`** — Rust. Port of `IndexService.swift` + `Indexer` + extractors. Runs as a
  short-lived process (spawned on change), exits after `linger`. Talks the **same stdin/stdout JSON
  event protocol** so it can also run headless/testing.
- **`trowel worker`** — C++ (~500 LOC). Port of `LlamaEmbedder.swift` + `ImageSizing.swift` +
  `WorkerService.swift` against llama.cpp (Vulkan) + `mtmd`. **Exactly the upstream JSON-lines
  protocol**, so the Rust engine treats it as one `Embedder` backend and can be swapped.
- **`trowel`** — Rust CLI: `estimate | index | search | status | eval` (+ `worker`, `index-service`).
- **UI**: all QML (`DECISIONS.md` D4, `UI-DESIGN.md`). An omarchy-shell plugin hosts the quick
  panel and bar widget; `trowel-app` (a separate Quickshell instance) hosts the main window, Quick
  Look, Settings and Onboarding. Both import the shared `Trowel` QML module and are thin clients of
  `troweld`.

### B.1 Why Rust for the engine

`rusqlite` (bundled SQLite with FTS5) reproduces the schema and tokenizer exactly; `notify` gives
inotify; `image`/`kamadak-exif` + `libvips` cover images; `half` gives fp16; `memmap2` gives the
mmap'd vector store; `matrixmultiply`/`gemm` gives the blocked matmul (BLAS also present). One static
binary per tool (dynamically linking libvips and the system libraries), a simple PKGBUILD, and no
GC pauses on the search hot path.

### B.2 Why the embedder stays C++

The production path uses llama.cpp APIs that are recent/niche (`llama_batch_ext_*`, `llama_process`,
`llama_embd`, `LLAMA_PROCESS_TYPE_ENCODE`, and the `mtmd` audio/video helpers). Reimplementing the
framing in another binding risks vector drift. Compiling a tiny C++ program against the *same*
pinned llama.cpp source, with the *same* no-logits patch, is the lowest-risk way to keep cosine ≥
0.999 — the whole point of the design.

### B.3 Backend choice on this machine

llama.cpp has Vulkan and SYCL backends for Intel Arc. Current guidance (Mesa 26.1+ ANV, and a known
SYCL stall bug on the 258V) makes **Vulkan** the correct default here; this machine already has Mesa
26.2.2 / Vulkan 1.4.354. Build with `-DGGML_VULKAN=ON`. SYCL remains a benchmark alternative.

### B.4 UI material/behavior mapping

| macOS | Linux |
|---|---|
| `NSVisualEffectView`/`NSGlassEffectView` behind-window blur, radius 18 | `PanelWindow` on the **Overlay** layer (namespace `trowel-panel`) plus a Hyprland Lua `layer_rule` with `blur` + `ignore_alpha` + `no_anim`; the radius comes from the Omarchy theme |
| Floating key panel, Spotlight placement | omarchy-shell `panel` plugin (`keepLoaded`), exclusive keyboard focus, same placement math (top at 16 % of the focused monitor) |
| `NSMenu` status item | none: dropped by D25 (no bar widget) |
| `QLPreviewPanel` Quick Look | `trowel-app` Quick Look window at the matched place; Sushi as a fallback |
| SwiftUI windows (main, Settings, Onboarding) | `trowel-app` Quickshell `FloatingWindow`s, same QML components |
| `UserDefaults` (`AppSettings`) | `~/.config/trowel/config.toml`, owned by `troweld` |
| FSEvents | inotify via `notify` |
| Carbon global hotkey | Hyprland Lua bind (default `SUPER + D`; `SUPER+SHIFT+SPACE` is taken on Omarchy) → `omarchy-shell trowel toggle` |
| `open`/Apple Events | `xdg-open`/`gio open`; `org.freedesktop.FileManager1.ShowItems` for reveal |
| IOKit power/thermal | `/sys/class/power_supply`, UPower DBus, `power-profiles-daemon` |
| `SMAppService` login item | systemd `--user` socket + service (`troweld.socket`/`.service`) |
| FSEvents persistent history | **none**: full reconcile on start and resume (ROBUSTNESS F1) |
| Sparkle | pacman/AUR |

### B.5 Nothing about search semantics may drift

The Linux `Searcher` must reproduce, exactly: FTS5 tokenizer string, `bm25(3.0,1.0)`, the coverage
computation, the fusion weights (`wordWeight 2.0`, `codeWordWeight 1.0`, lookup flip), `zGate 2.0`,
`strongCount` 0.5, the modality-mean/z-score normalization, the `setsScale` exclusion for later-pass
vectors, and the prompt strings. These are the outputs of the evals; changing any invalidates
`evals/` comparisons and the user-visible ranking. See `PORT-MAP.md` §Fidelity invariants.
