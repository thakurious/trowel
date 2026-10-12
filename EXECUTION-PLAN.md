# Execution plan

Phased and dependency-ordered. Each phase ends with a **verifiable deliverable** and an
**acceptance criterion**. The plan front-loads the two existential risks, *vector parity* and
*search-math parity*, so a failure is discovered in week 1, not week 6. It runs as **two tracks**
(engine and UI) that meet at the frozen socket protocol (`PROTOCOL.md`).

## How to use this plan (for whoever implements it, human or agent)

- **Source of truth for behavior** is upstream DigUp at the pinned SHA (`DECISIONS.md` D15), cloned
  at `upstream/`. When porting a file, open the Swift side by side. Every **[P]** row in
  `PORT-MAP.md` names the file, and the port's tests come from the matching `Tests/` file.
- **Source of truth for numbers**: search/index constants → `PORT-MAP.md` §Fidelity invariants;
  UI sizes/motion → `UI-DESIGN.md`; failure handling → `ROBUSTNESS.md`; wire format →
  `PROTOCOL.md`; perf targets → `BENCHMARKS.md`.
- A task is done when **its tests pass in CI and its acceptance line below is demonstrated**, not
  when the code compiles. Do not start a phase whose upstream gate (see the graph at the end) is red.
- Never change a fidelity invariant to make a test pass. If parity fails, the port is wrong.

## Repo layout

```
trowel/
├─ Cargo.toml                      # workspace
├─ rust-toolchain.toml
├─ upstream/                       # ARahim3/DigUp @ pinned SHA (submodule, read-only)
├─ crates/
│  ├─ trowel-core/                  # DigUpKit port: model, rules, crawl, extract, index, search, text
│  ├─ trowel-proto/                 # PROTOCOL.md serde types (shared by daemon, CLI, tests)
│  ├─ troweld/                      # daemon: watcher, socket server, supervisor, previews; --mock
│  └─ trowel-cli/                   # `trowel` (estimate|index|search|status|eval|doctor|index-service|debug-*)
├─ cpp/                            # trowel-worker: llama.cpp + mtmd (Vulkan / CPU)
│  ├─ main.cpp  embedder.{h,cpp}  image_sizing.{h,cpp}
│  └─ llama/ (TAG=b11461, llama.cpp-no-logits.patch from upstream/scripts)
├─ ui/
│  ├─ qml/Trowel/                   # shared module: Client, Theme, Motion, components, icons
│  ├─ omarchy-plugin/              # manifest.json + Panel.qml + BarWidget.qml (thin)
│  ├─ app/                         # trowel-app Quickshell config: main window, Quick Look, settings, onboarding
│  ├─ gallery/                     # component gallery for visual QA
│  └─ strings.json                 # every user-visible string, extracted from upstream Swift
├─ design/reference/macos/         # reference screenshots of the macOS app
├─ evals/                          # verbatim from upstream
├─ reference/                      # verbatim (photo.jpg, screen.png, speech.wav, vectors.json)
├─ tests/proto/                    # golden protocol transcripts
├─ packaging/                      # PKGBUILD, systemd units, hypr/trowel.lua, .desktop, metainfo, icons
└─ testbed/                        # make-testbed port output (gitignored) + generator scripts
```

---

## Phase 0: Scaffold and de-risk the runtime (week 1)

**Goal:** prove the two "must match" contracts on this machine before writing the engine.

1. Workspace scaffold; `upstream/` pinned to `6faa29e`; `cargo fmt`/`clippy -D warnings`; CI on an
   `archlinux` container (see TESTING §5).
2. **Build llama.cpp**: tag `b11461`, apply upstream's `scripts/llama.cpp-no-logits.patch`. Mirror
   upstream's flags from `scripts/build-llama.sh` with Metal swapped for Vulkan:
   `-DBUILD_SHARED_LIBS=OFF -DGGML_VULKAN=ON -DGGML_NATIVE=OFF -DLLAMA_BUILD_MTMD=ON
   -DLLAMA_BUILD_COMMON=OFF -DLLAMA_BUILD_TOOLS=OFF -DLLAMA_BUILD_SERVER=OFF -DLLAMA_BUILD_EXAMPLES=OFF
   -DLLAMA_BUILD_TESTS=OFF`, target `llama mtmd`. (`GGML_NATIVE=OFF` because the package must run on
   other CPUs; a `-native` local build is a benchmark variant only.) Also build a **CPU-only**
   variant. Confirm `vulkaninfo --summary` shows the Arc 140V and llama.cpp logs the device with
   coopmat.
3. **`cpp/trowel-worker`**: `info`/`text`/`image`/`audio`/`quit`, porting `WorkerService.swift`,
   `LlamaEmbedder.swift` and `ImageSizing.swift`. Add a `--backend vulkan|cpu` flag.
4. **Vector-parity harness**: copy `Tests/LlamaRuntimeTests/Reference/*` verbatim; `trowel-cli`
   test plus `scripts/parity.sh` drive the worker and compute cosines; include the 5 text cases.
5. **Spike: Omarchy plugin hello-world**: a `panel`-kind plugin with namespace `trowel-panel`,
   summoned by a Lua bind on `SUPER + D`, with the blur `layer_rule`. Confirm the blur follows the
   rounded corners at scale 1.25 and that keyboard focus and fcitx5 pre-edit work. Half a day; it
   retires the biggest UI-platform risk early.

**Deliverable:** `trowel-worker` on Vulkan and CPU prints vectors for the reference inputs, and a blurred
panel appears on the hotkey.

**Acceptance:** every reference cosine ≥ **0.999** on **both** Vulkan and CPU (text, 280/140/70
images, 16 kHz WAV). If image or audio misses, fix `ImageSizing`/framing *now*. If audio can't match,
ship text+image first with audio gated behind a flag; never silently change vectors.

---

## Phase 1: Core data model, text, and the frozen protocol (week 2)

Port with unit tests that mirror `DigUpKitTests`:

1. `Database` + `IndexStore`: schema v0–v5, the FTS5 tokenizer string, `scope`, `pending` ordering,
   `addSegment`, `keywordText`, counts, migrations. Add `linux_*` tables (D16). **Schema-parity
   test**: create an index with the upstream CLI (on a Mac, or from the SQL in `IndexStore.swift`),
   open it with the port, and assert the reads.
2. `Model` enums + `IndexOptions` + `Prompts` + `CodeQuery` (string values equal the DB values).
3. `Vectors`/`VectorStorage`: fp16, mmap, blocked conversion + matmul (`blockRows = 1024`).
4. `SearchText`: port `SearchTextTests` plus the Bengali/Arabic cases.
5. `FolderSelection` + its tests.
6. **Freeze `PROTOCOL.md` v1**: `crates/trowel-proto` types, the golden transcripts, and
   **`troweld --mock`** (canned results from a fixture JSON, configurable latency, a scripted status
   stream). From here on, the UI track runs in parallel.
7. **Strings extraction**: a script that pulls every user-visible string literal from
   `upstream/Sources/DigUpApp/*.swift` into `ui/strings.json` with source file:line.

**Acceptance:** all ported unit tests green; the FTS tokenizer behaves identically on the
Bengali/Arabic strings; the mock daemon passes the golden transcripts.

---

## Phase 2: Extractors (weeks 2–3, engine track)

Each extractor sits behind a trait and **runs external tools as subprocesses with timeouts and
`RLIMIT_AS`** (ROBUSTNESS F5). Subprocess args always go after `--`.

1. **Image** (libvips + kamadak-exif): thumbnail-at-target-size, min-side check, EXIF date; HEIC
   (libheif, installed) and RAW.
2. **Screenshot detection**: upstream name prefixes **plus Omarchy's**
   `screenshot-YYYY-MM-DD_HH-MM-SS.png` in `$OMARCHY_SCREENSHOT_DIR`/`$XDG_PICTURES_DIR`
   (`/usr/bin/omarchy-capture-screenshot:60`), grim/hyprshot/flameshot/GNOME/KDE patterns, and PNGs
   whose dimensions equal a connected monitor's mode (a cheap heuristic; record it as a weak signal).
3. **OCR** (tesseract `--oem 1`): full-res screenshots; languages from installed packs.
4. **PDF** (poppler): per-page text/render, `PageTextProblem`, `pdfinfo` metadata and encryption,
   long-file first step.
5. **Document**: txt/md/html + docx/odt (Rust) + rtf/doc (best effort); the chunker verbatim.
6. **Audio/Video** (ffmpeg/ffprobe): 16 kHz streaming windows, silence gate, frames + average-hash
   dedupe, durations.
7. **Code**: `CodeExtractor` + `CodeLookup` + `git ls-files`.
8. **Testbed builder** (port of `scripts/make-testbed.sh`), plus Linux traps: invalid-UTF-8 names,
   newline in a name, 255-byte name, leading-dash name, a truncated PDF, a zip bomb named `.docx`,
   a 0-byte MP4, a symlink loop, and an unreadable directory.

**Acceptance:** `trowel estimate` over the testbed matches the macOS counts within a few %; all
`traps/` classify correctly; every malformed file yields `failed` with a reason, and no extractor
hangs past its timeout.

---

## Phase 3: Index pipeline + CLI (weeks 3–4, engine track)

1. `Indexer`: `sync` reconcile (including move-by-inode), the prepare/commit overlap (threads +
   double buffer), batching, the state machine, `finishLongFiles`, the fingerprint
   `checkModel`/`adopt`/reset, and the **poison-file journal** (ROBUSTNESS F5c). Port
   `LongFilesTests`.
2. `Estimator` + `Costs` (poppler/ffprobe probes, no Spotlight); it also reports directory counts
   for the inotify budget (F3).
3. `trowel` CLI: `estimate | index | search | status | eval` + flags (PORT-MAP §C), `--json`
   everywhere.
4. `trowel index-service`: the upstream stdin/stdout event protocol; `SharedEmbedder`;
   missing/unreadable roots; scratch cleanup; worker supervision with backoff and per-item retry (F6).

**Acceptance:** index then search the testbed with sensible results; `kill -9` of the helper at
random points (scripted ×50) loses nothing and never re-embeds finished files; a poison file is
skipped on the second attempt; touching one file gives `changed=1`, and only it is re-embedded.

---

## Phase 4: Search math + evals (week 4), the product-quality gate

1. `Searcher` verbatim (ARCHITECTURE §A.8).
2. `trowel eval` (R@1/5/10, MRR, tags, `@range`, weaker-fold stats, `--bench`).
3. Run `evals/synthetic.json`, `code.json` and `multilingual.json` on the testbed.

**Acceptance:** evals meet or beat TESTING §3. If they lag, the cause is almost always the vectors
(Phase 0) or tokenizer/normalization; debug with `trowel search --json`.

---

## Phase 5: Daemon, watcher, socket, supervision (week 5, engine track)

1. `troweld`: owns SQLite reads and the lazy `Searcher`; watcher (`notify`; 0.75 s debounce; code
   watcher 2 s quiet / ≤10 min); helper lifecycle; searcher refresh ≤ every 2 s; encoder idle exit
   (600 s); embed-ahead + cache (256); status coalescing (≤ 4 Hz).
2. Implement **PROTOCOL.md v1** for real and pass the same golden transcripts as `--mock`.
3. Previews and thumbnails service (PROTOCOL §4): sizes 64/132/264/528, PDF band, waveform peaks,
   video frames; freedesktop thumbnail-cache reuse; LRU disk cache.
4. Robustness mechanisms: F1–F4 (startup/resume reconcile, overflow rescan, watch budget, polling
   roots), F7 (logind sleep), F9–F14, the systemd units/slices (ROBUSTNESS §2), and `trowel doctor`.
5. `PowerPolicy` (sysfs/UPower/power-profiles-daemon), event-driven, gating the backfill only.
6. `config.toml` as the single source (the AppSettings mirror), watched and validated.

**Acceptance:** a UI-less client can prewarm, search, see a folder change trigger a sync,
pause/resume, and stream status. The daemon's idle RSS is ≤ 120 MB and the vector matrix is
released on idle; model memory leaves with the worker. Suspend/resume and an inotify-overflow
simulation both end with a correct index. `trowel doctor` is all-green on this machine.

---

## Phase 6: UI (UI track: weeks 2–7, against `--mock` until week 5)

### 6.0 Design system (weeks 2–3)

- Capture macOS reference screenshots into `design/reference/macos/` (UI-DESIGN §9).
- `ui/qml/Trowel`: `Theme.qml` (Omarchy token bridge with live reload), `Motion.qml`, `Client.qml`
  (PROTOCOL v1 with `gen`-based stale dropping and model diffing), and the components in UI-DESIGN §6.
- `ui/gallery` showing every component in every state, under 4 themes × 3 scales.
- Spike the folder picker (portal FileChooser via a `trowel portal-pick-folder` helper).

### 6a. Quick panel and bar widget: the Omarchy plugin (weeks 3–5)

UI-DESIGN §3 in full: geometry, layer-shell, the Hyprland rules file, two-phase search, grouping
and fold, preview column, keyboard map (with the `navigated` Space rule), focus-loss hide, IME,
empty states, code mode. The bar widget shows status states and a popup (search, open, pause,
choose folders, settings, quit).

### 6b. `trowel-app` (weeks 4–7)

Main window, Quick Look window, Settings (4 tabs), Onboarding (5 steps), model download
(UI-DESIGN §7). All copy comes from `ui/strings.json`.

### 6c. Integration (week 7)

Hotkey recorder with the Lua bind and conflict detection (UI-DESIGN §8); open/reveal/editor-at-line;
`.desktop` + Hyprland window rules for `trowel-app`; a start-at-login switch (`systemctl --user
enable troweld`).

**Acceptance:** TESTING §4 passes; every UI-DESIGN §5 budget is met with measurements recorded; the
visual review against `design/reference/macos/` is signed off *as close in layout, density and
motion* with Omarchy theming; the theme switch recolors both hosts live; and killing `troweld` while
the panel is open leaves the shell responsive (F18).

---

## Phase 7: Hardening, packaging, performance (weeks 8–9)

1. **Fault-injection suite** (TESTING §7): every ROBUSTNESS row exercised automatically.
2. **Soak**: 72 h on the real home directory with normal use, then check RSS drift, file-descriptor
   counts, failed-file reasons, and that the index matches a fresh full reindex (set-equality of
   `files` and of segments per file).
3. `packaging/PKGBUILD`: `trowel`, `troweld`, `trowel-worker`, QML module, app, plugin, systemd
   units, `hypr/trowel.lua`, `.desktop`, metainfo, icons. Plus `trowel setup` (enable plugin, add the
   bind include, enable the socket) and `trowel uninstall --purge`.
4. First run: model download to `~/.local/share/trowel/models`, onboarding, lazy index creation.
5. **Performance pass** (BENCHMARKS.md), including the Vulkan vs SYCL vs CPU comparison.
6. Docs: README install, hotkey, troubleshooting (`trowel doctor`), uninstall.

**Acceptance:** on a fresh user account, `makepkg -si` then login → onboarding → first results
works end to end; benchmarks within targets; evals still green on the packaged build; the 72 h
soak is clean.

---

## Cross-cutting workstreams

- **Fidelity harness** (from Phase 0, permanent): reference vectors (Vulkan and CPU) plus the three
  evals, run in CI on every llama.cpp/Mesa/engine change.
- **Testbed** (Phase 2): shared by evals, UI bring-up, fault injection and perf.
- **Headless debug CLI** (`trowel debug-{search,eval,index,open-dry,snapshot,ui-trace}`), the Linux
  analogue of `DebugHooks.swift`.
- **Upstream sync** (weekly, D15).

## Dependency graph

```
P0 worker+parity ─────────────────────────────► P4 search/evals ─┐
P1 store/text/PROTOCOL+mock ─► P3 pipeline ─► P4                  ├─► P5 daemon ─► 6c integ ─► P7
P2 extractors ───────────────► P3                                 │
P1 PROTOCOL+mock ─► 6.0 design system ─► 6a panel ─► 6b app ──────┘
```

Critical path: **P0 → P1 → P3 → P4 → P5 → 6c → P7**. The UI track starts in week 2 against
`troweld --mock` and switches to the real daemon in week 5.

## Effort estimate

About **9 weeks for one engineer** (engine and UI interleaved), or about 7 with two in parallel.
The risky engine parts (vectors, search math) are done by the end of week 4, and the UI-platform
risks (blur, focus, IME, plugin host) are retired by the Phase 0 spike. An MVP of "text + images +
PDFs + code, CLI + panel" is achievable by the end of week 4.
