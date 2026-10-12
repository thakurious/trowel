# Testing & the quality bar

DigUp's own comments are a changelog of measured regression thresholds. The Linux port holds to the
same bar: **no search-math change ships without before/after evals**, and **no runtime change ships
without the vector-parity test**.

## 1. Vector parity (the keystone gate) — Phase 0, permanent

`Tests/LlamaRuntimeTests/ReferenceVectorTests.swift` stores reference vectors and requires **cosine
≥ 0.999** for each input on every llama.cpp update. Copy the reference inputs *verbatim*
(`Reference/photo.jpg`, `screen.png`, `speech.wav`, `vectors.json`) and the same 5 text cases
(`query`, `query-bn`, `query-ar`, `document`, `document-long`).

- Run: `trowel-worker` driven by a Rust test / shell script; assert cosine ≥ 0.999.
- Any failure = either keep the old llama.cpp or bump `vectorVersion` **and re-embed every index**
  (a user-visible decision, never silent).
- This must pass on this machine's **Vulkan** build **and the CPU build** (the CPU build is the
  automatic fallback in ROBUSTNESS F6, so it must be vector-compatible); add SYCL to the same harness.
- The same check ships inside the daemon as the **parity self-test** (ROBUSTNESS F14).

## 2. Unit tests ported from `Tests/DigUpKitTests` + `Tests/LlamaRuntimeTests`

| Upstream | Port covers |
|---|---|
| `SearchTextTests.swift` | NFKC/normalization, `inflects`, `hasWords`, `matches`/`snippet`, `Landmark`, page-text problems. |
| `RulesTests.swift` | classify (TS vs MPEG-TS sniff), secrets, datasets, machine-named, screenshots. |
| `FolderSelectionTests.swift` | chosen/skipped/kept semantics, marks, inside math. |
| `IndexStoreTests.swift` | schema, segments/FTS, moves, prune, counts, fingerprints. |
| `LongFilesTests.swift` | PDF/document stepping, resume, caps, digits→keyword-only, float16 across blocks. |
| `CodeTests.swift` | stretches, notebook cells, `CodeLookup` definition strength, first-line. |
| `ImageSizingTests.swift` | target-size math + Pillow-exact bicubic. |
| `ReferenceVectorTests.swift` | see §1. |

Add Linux-specific tests: HEIC/RAW via vips, ffmpeg windowing/silence gate, average-hash dedupe,
`notify` debounce, XDG path resolution, resume/verify of the model download.

## 3. Evals — Phase 4 gate, permanent

Three JSON suites copied verbatim; run via `trowel eval`. An eval entry is `{q, expect:[paths],
tag?, kind?}`; `path@a-b` requires the hit to start in `[a,b)` (a moment/page/lines overlap).
Metrics: **R@1, R@5, R@10, MRR**, plus the weaker-fold stats.

Published targets to match or beat on this machine, from DigUp's own comments:

| Signal | DigUp figure |
|---|---|
| Synthetic eval, fusion vs RRF | **R@1 0.88** vs 0.73 |
| Real (testbed `real/`) eval | R@1 0.86 vs 0.82 |
| Combined meaning scaling (3 evals, 105 q) | **94/105** with the right answer first (was 91) |
| Real-code eval (27 q): wordWeight 2.0 | 18 → **20** found first |
| Synthetic code eval (41 q) | **41/41** |
| Code title = repo-relative path | **24/27** top-5, MRR **0.79** (name-only 22/27, 0.69) |
| Code query prompt | 24 vs 22 top-5, MRR 0.79 vs 0.75 |
| `strongCount` 0.5 fold | 26.2 → **5.7 results/query**, never folds an expected answer |
| `setsScale` exclusion of later-pass vectors | **24 → 22 of 26** first (i.e. keeping it is better) |
| Glyph-order page: picture + garbled vector vs picture alone | **R@1 0.75** vs 0.73, MRR 0.84 vs 0.81 |

Also verify the *deliberate* losses that are correct: code stretches **1800** beats 1200/3600;
screenshot budget **280** (560 is 2× slower, no better); OCR text gets no vector; screenshot exact
lookups still work via keyword.

## 4. UI parity checklist: Phase 6 gate (measurements in UI-DESIGN §5, visual QA in §9)

- Panel: opens warm < 150 ms; two-phase search; grouped + folded results; per-result reason
  (meaning vs words); preview box; place badge (`p. 3`, `0:06`, `♪ 0:25`, `L12–40`); keyboard map
  (Esc/↑↓/↩/Ctrl↩/Space-after-navigation/Ctrl+Y/Ctrl+C/Ctrl+O/Tab, IME-safe with fcitx5);
  Spotlight placement on the focused monitor; focus-loss hide; live Omarchy theme switch.
- Smoothness: every UI-DESIGN §5 budget is measured and recorded (`trowel debug ui-trace`).
- Main window: filters with live counts; grid + code list; preview pane (image/PDF band/code
  focus/passage/waveform/frame); home examples.
- Settings/onboarding: every tab and step; folder checkboxes with estimates; skips/types; code
  opt-in + editor picker; battery switch; model download.
- Behaviors: watcher→sync; pause/resume; backfill gating; open/reveal/Quick Look/editor-at-line;
  missing/unreadable-root states; `code:` prefix routing.
- Screen recordings/screenshots captured for regression (`trowel debug snapshot`) across the theme and
  scale matrix (UI-DESIGN §9).

## 5. CI

GitHub Actions on an `archlinux` container:
1. build (no GPU) → unit tests.
2. build llama.cpp Vulkan (or use a cached artifact) + run the **vector-parity** test on a runner
   with a GPU; otherwise run it locally and upload results.
3. run evals on the testbed; fail the build if recall drops below the table (§3).

## 6. Protocol conformance

Golden transcripts in `tests/proto/` are replayed against `troweld --mock` and the real daemon. They
include out-of-order `gen` handling, `superseded` responses, the slow-reader queue cap, the
non-UTF-8 path encoding, and the version-mismatch `hello`.

## 7. Fault injection (Phase 7 gate): one automated test per ROBUSTNESS row

| Test | Method |
|---|---|
| Crash mid-index ×50 | `kill -9` the helper at random offsets; assert no lost or duplicated segments and that finished files aren't re-embedded |
| Poison file | An extractor stub that segfaults on one file → skipped as `failed` after 2 attempts |
| Hang | A stub tool that sleeps forever → killed at its timeout |
| Worker device loss | `kill -9` the worker mid-batch, and a fake `VK_ERROR_DEVICE_LOST` exit code ×3 → CPU fallback |
| Daemon down | Restart troweld during a search; the UI reconnects and shows the right state |
| inotify overflow | Set `max_queued_events` low in a user namespace (or inject the event) → rescan |
| Watch budget | Set `max_user_watches` low → the polling fallback shows in `doctor` |
| Suspend | Send a fake logind `PrepareForSleep` true/false → sync + worker ping |
| Disk full | A small loop-mounted tmpfs as the index dir → pause, no corruption |
| Corrupt DB | Flip bytes in `index.sqlite` → quarantine + rebuild |
| Offline changes | Stop the daemon, mutate the testbed (add/move/delete/edit), start it → the index equals a fresh reindex |
| Unmount | Remove a bind-mounted root → `missing`, rows kept, results hidden; remount → back |
| Upgrade drift | Swap in a worker build with perturbed output → self-test fails, never mixed into the index |

## 8. Definition of done

Linux port is "done" when: vector parity ≥ 0.999 on Vulkan; all ported unit tests green; all three
evals meet §3; the UI checklist passes; `makepkg` installs and a fresh-install onboarding completes;
`BENCHMARKS.md` and UI-DESIGN §5 targets are met on this machine; the fault-injection suite (§7) is
green; and the 72 h soak (EXECUTION-PLAN Phase 7) is clean.
