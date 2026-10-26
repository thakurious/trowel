# Performance on this machine

**Hardware (verified):** Intel Core Ultra 7 258V (Lunar Lake, 8 cores, ≤4.8 GHz, no AVX-512; AVX2 +
AVX-VNNI), **Intel Arc 140V** iGPU (Xe2, UMA — shares the 30 GB LPDDR5X), `xe` driver, Mesa
**26.2.2**, Vulkan **1.4.354**, `intel-compute-runtime` + `level-zero-loader` present, no dGPU.
Linux 7.2.5-omarchy, Wayland/Hyprland 0.56.2, Quickshell running. ffmpeg (hevc/av1/vp9/opus),
poppler 26.08, libvips 8.18.6, tesseract (eng+osd). inotify watch limit 524288.

**Model:** EmbeddingGemma 2, 740 M params total (270 M text / 170 M vision / 300 M audio), 768-d,
8 K context, Q8_0 GGUF (text 310 MB + mmproj 555 MB).

## Why it will be fast here

Embedding is **encode-only** (no autoregressive decode), so it exercises llama.cpp's *prompt-
processing* path, which is exactly where Intel iGPUs are strongest. Reference points measured on the
same class of chip (Arc iGPU, Core Ultra 7 258V, via llama.cpp on Linux):

| Model / test | Backend | pp512 (tok/s) | tg128 (tok/s) |
|---|---|---|---|
| llama 7B Q4_0 | Vulkan/SYCL | ~535 | ~24.6 |
| (Lunar Lake 140V, SYCL, large models) | SYCL | ~2× M1 Pro prefill | M2-class decode |

A 740 M encoder is ~10× smaller than a 7 B model, and we only pay the prefill path, so image/text
encoding should run in the **thousands of tokens/s** and multiple images/second. Decode speed is
irrelevant to us.

## Targets (acceptance for Phase 7)

| Metric | Target | Notes |
|---|---|---|
| Query encode (270 M text, warm) | ≤ 60 ms end-to-end | panel shows keyword instantly; meaning fills in after the 120 ms pause |
| Query encode (cold worker start) | ≤ 1.5 s | masked by `prewarm` when the panel opens |
| Search latency @ 10k vectors | ≤ 15 ms | blocked fp16 matmul, 768-d |
| Search latency @ 100k vectors | ≤ 120 ms | ~150 MB fp16 resident |
| First pass, `synthetic/` testbed | ≤ ~3 min | mirrors Mac's ~160 s on M4 Pro; extraction-bound. Measured at `index_speed = "full"` (D28); gentle/balanced trade time for desktop headroom |
| Steady-state new file (screenshot) | ≤ 1.5 s | searchable ~1–2 s after it lands |
| Daemon idle RSS | ≤ 120 MB | vectors released after encoder idle-exit (600 s) |
| Worker RSS (text-only / full) | ~0.3 GB / ~1 GB | lives only in the worker, freed on exit |
| Index size | ~1.6 KB/vector + text | fp16, matches DigUp |
| Keyword search (FTS5), 100k segments | ≤ 10 ms p95 | the per-keystroke path; see UI-DESIGN §5 for the end-to-end budget |
| Hotkey → panel first frame (warm) | ≤ 50 ms | pre-mounted shell plugin |
| Keystroke → keyword results painted | ≤ 40 ms p95 | |
| UI frame time (open, type, scroll) | p99 ≤ 16.6 ms at 60 Hz, 0 dropped frames/60 s | `QSG_RENDER_TIMING=1` |
| Thumbnail (cached) | ≤ 5 ms daemon, ≤ 100 ms on screen | |
| Startup reconcile (warm cache) | ≤ 1 s per 100k files | ROBUSTNESS F1 |
| Desktop impact while indexing | no frame drops in the panel or Hyprland animations | helper in the `trowel-index.slice` (Nice 10, idle IO) |
| Index speed (D28), 80 screenshots + 2 videos | gentle: GPU net ≤ ~15 % avg, no saturated stretch > 3 s, index CPU ≤ 1.1 cores | measured 2026-10-26: full 295 s / GPU net ~36 % avg / 21 s longest at ≥ 90 % / 1.3 cores; balanced 346 s / ~26 % / 6 s / 0.9; gentle 471 s / ~14 % / 3 s / 0.8 (max 1.1). Vectors byte-identical across speeds |
| `calc`, daemon side (Phase 8 L2c) | p95 < 1 ms, hard stop at 20 ms | measured 2026-10-15 release: p50 43–64 µs, p95 237–408 µs over `tests/calc/table.tsv`; one-shot `5 ft to cm` 238–565 µs after a 0.44–0.64 ms warm-up |
| `apps_query`, 200 entries (Phase 8 L2) | p95 < 2 ms | measured 2026-10-15 release, 94 installed apps: 310–330 µs per query; entry scan 4–8 ms; icon index 100–180 ms (background) |
| `clip_query`, 1000 entries (Phase 8 L3) | p95 < 5 ms | measured 2026-10-15 release (`trowel clip bench`, 300 queries: empty, words, literal `://`, kind filters): p50 0.38 ms, p95 0.66 ms |
| Clipboard image ingest (Phase 8 L3) | < 150 ms for an 8 MB screenshot, thumbnail excluded | measured 2026-10-15 release: 8 MB synthetic PNG 8.3 ms; 18.5 MB real PNG 18.2 ms (read, SHA-256, blob write + fsync, insert, retention) |
| Copy → visible in `clip_query` (Phase 8 L3, live) | ≤ 300 ms | measured 2026-10-15 `scripts/clip-live.sh` (real compositor): text, PNG, file, HTML each 61–149 ms over 4 runs, including a `trowel clip list` poll every ~30 ms |

## What will be *slower* than an Apple Silicon Mac

- iGPU shares memory bandwidth with the CPU (UMA) and Metal is very mature; expect per-item
  embedding ~1.5–3× slower than an M-series for vision/audio-heavy work. For this workload that
  still clears the interactive bar.
- No Neural Engine — OCR is Tesseract (CPU), not Vision. Tesseract is comparable in cost to Vision
  per the overlap design; it runs on the utility thread and overlaps embedding.

## Backends to compare (Phase 7 deliverable)

Build the worker twice and benchmark with the `trowel eval --bench` harness:
1. **Vulkan (default)** — Mesa ANV coopmat; current best on Arc.
2. **SYCL** (`-DGGML_SYCL=ON -DGGML_SYCL_F16=ON`) — measure; **known stall bug on the 258V** (first
   request works, later ones hang) — if reproduced, note it and keep Vulkan.
3. **CPU**: the packaged build is `-DGGML_NATIVE=OFF` (the automatic fallback, so it must pass parity);
   `-DGGML_NATIVE=ON` is a local baseline only.

## Methodology

- Fixed testbed + fixed queries; `trowel search --json` already emits `z`, `keyword_rank`, timing.
- Time: query encode (worker handshake excluded vs included), pure search, per-file index time by
  kind, first-pass total, RSS via `/proc/<pid>/status` at idle and during a pass.
- Vector parity and eval recall measured **before** any perf claim (`TESTING.md`).
- Record the Mesa/kernel/llama.cpp versions with every run — they move fast (Mesa 26.1 alone doubled
  Battlemage Vulkan decode).

## Memory budget

30 GB total, ~17 GB typically available. Worst case: worker full model ~1 GB + index vectors
(100k → 150 MB) + extractor scratch (one window/frame at a time) + daemon. Comfortable.
