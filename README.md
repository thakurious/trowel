# Trowel

**On-device multimodal file search for Omarchy.** Trowel is a Linux port of
[ARahim3/DigUp](https://github.com/ARahim3/DigUp) (a macOS app).

This repo holds the plan to port DigUp — on-device, multimodal semantic
file search built on Google DeepMind's **EmbeddingGemma 2** (text + images + audio + video, 768-d,
Apache-2.0) — to Linux, targeting **Omarchy (Arch + Hyprland + Quickshell)** on this machine:
**Intel Core Ultra 7 258V (Lunar Lake), Arc 140V iGPU, 30 GB RAM, Wayland**.

**You do not need a Mac.** Trowel runs on Omarchy (Arch + Hyprland + Quickshell). The product
name is Trowel (DECISIONS D17); DigUp is the upstream macOS app this port follows.

## Use it

```bash
# from this checkout, after a workspace build:
export PATH="$PWD/target/debug:$PATH"
trowel setup --from-tree "$PWD"
```

That installs the Omarchy plugin (`io.github.thakurious.trowel`), the Super+D bind, and
`troweld.socket` for your user. **Restart omarchy-shell once** (or log out), then press
**Super+D**. First open is onboarding; indexing starts after you pick folders.

```bash
trowel doctor              # why isn't it working
trowel uninstall           # plugin + units
trowel uninstall --purge   # also deletes ~/.local/share/trowel and config
```

Packaged install (Phase 7 PKGBUILD): `makepkg -si` then `trowel setup`.

## Documents

| Doc | What it covers |
|---|---|
| **[ARCHITECTURE.md](ARCHITECTURE.md)** | How DigUp actually works (engine, runtimes, data model, algorithms), restated precisely from the source, and the target Linux architecture. |
| **[PORT-MAP.md](PORT-MAP.md)** | File-by-file mapping of every DigUp component to its Linux replacement, with the exact tools/crates and what must be preserved bit-for-bit. |
| **[DECISIONS.md](DECISIONS.md)** | The load-bearing decisions (language, UI toolkit, process model, vector store, embedder, IPC) with rationale and rejected alternatives. |
| **[EXECUTION-PLAN.md](EXECUTION-PLAN.md)** | Phased, task-level plan with deliverables and acceptance criteria. |
| **[BENCHMARKS.md](BENCHMARKS.md)** | This machine's capability profile and expected performance, with the method to verify it. |
| **[TESTING.md](TESTING.md)** | Unit/eval/vector-parity test strategy — the quality bar DigUp holds itself to (cosine ≥ 0.999, R@1/R@5/MRR evals) — plus protocol conformance and fault injection. |
| **[UI-DESIGN.md](UI-DESIGN.md)** | The visual and motion spec: Omarchy plugin hosting, theme-token mapping, exact panel/window geometry from the Swift source, keyboard map, motion curves, smoothness budgets, visual QA. |
| **[PROTOCOL.md](PROTOCOL.md)** | The `troweld` socket protocol v1 that every UI speaks (search with stale-dropping, previews, status, config). It is frozen in Phase 1 so the UI can be built against `troweld --mock`. |
| **[ROBUSTNESS.md](ROBUSTNESS.md)** | The Linux failure-mode matrix (no FSEvents history, inotify limits, poison files, GPU loss, suspend, disk full, corruption) with the required handling, supervision layout and `trowel doctor`. |
| **[LAUNCHER-PLAN.md](LAUNCHER-PLAN.md)** | *Proposal (Phase 8+, awaiting sign-off):* Raycast-style app launching, inline calculator and rich clipboard history on Omarchy, with proposed decisions, protocol v2 ops, phases L0–L6, budgets and open questions. |

**Reading order for implementers:** README → ARCHITECTURE §A → DECISIONS → EXECUTION-PLAN (then,
per phase: PORT-MAP rows + PROTOCOL / UI-DESIGN / ROBUSTNESS as referenced) → TESTING.
Upstream source: ARahim3/DigUp @ `6faa29e` (MIT), pinned at `upstream/` (DECISIONS D15).

## One-paragraph summary

DigUp is a local hybrid search engine over your own files: a crawler classifies files, per-modality
extractors turn images/PDFs/documents/audio/video/code into text and sampled media, an
`EmbeddingGemma 2` model (llama.cpp, Q8_0 GGUF + an `mtmd` vision/audio projector) embeds everything
into a single 768-d space, and results are a **fusion of vector similarity (per-modality z-scored)
and SQLite FTS5 keyword search**, with an honest per-query "nothing stands out" gate. The macOS app
is a SwiftUI/AppKit menu-bar app; the intelligence is portable. The hard parts on Linux are (a)
reproducing **identical vectors** on a different backend so indexes/evals stay valid, and (b)
replacing Apple's extractors (ImageIO/PDFKit/Vision/AVFoundation/Spotlight) with `libvips`/`poppler`/
`tesseract`/`ffmpeg`, Apple's UI shells (menu bar, Quick Look, Sparkle) with an omarchy-shell plugin and a
QML app that follow the Omarchy theme, and FSEvents with inotify plus startup reconciliation under systemd.

## Non-goals

- No Windows/macOS-style self-updater (we use pacman/AUR).
- No telemetry, no cloud, no account — same as upstream.
- Not a fork of upstream; this is an independent Linux implementation that shares its model choice,
  index semantics, prompt framing, and search math so results match.
