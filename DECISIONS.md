# Decisions

Each decision: the choice, why, and the alternatives rejected. Ordered by how much rework a wrong
call would cost.

## D1. Engine language — **Rust**

**Why.** The engine is I/O- and math-bound, not UI-bound. Rust gives `rusqlite` (bundled SQLite with
FTS5) to reproduce the schema/tokenizer exactly, `memmap2` for the mmap'd fp16 vector store,
`half` + `matrixmultiply` (or `cblas`, already installed) for the blocked matmul, `notify` for
inotify, `walkdir`, `serde_json`, `reqwest`, and a `std::thread` pipeline that mirrors the
`prepare`/`commit` overlap without a GC pause on the search path. It produces small self-contained binaries and a
simple PKGBUILD.

**Rejected.**
- *Swift on Linux* — `DigUpKit` depends on AppKit/ImageIO/PDFKit/Vision/AVFoundation/Accelerate/
  UniformTypeIdentifiers/CoreGraphics/CoreServices/IOKit/CryptoKit; none exist on Linux, and SwiftUI
  doesn't exist. You'd rewrite every reader anyway, and Swift-on-Linux tooling/packaging is worse.
- *Python* — fastest to write (the reference worker is Python) but ~10–50× slower per-vector on the
  hot path, a heavy resident daemon, and painful packaging (offsets the whole "fast on this system"
  goal). Fine for *prototyping* extractors/evals, not the product.
- *Go* — cgo for llama.cpp and weaker numeric ecosystem; no real advantage over Rust here.
- *C++ throughout* — matches llama.cpp but slows engine development (SQLite, JSON, CLI, watcher) and
  invites memory bugs in long-running code.

## D2. Embedder — **a small C++ helper binary, same llama.cpp + mtmd**

**Why.** The production `LlamaEmbedder.swift` uses recent/niche llama.cpp APIs
(`llama_batch_ext_*`, `llama_process`, `llama_embd`, `LLAMA_PROCESS_TYPE_ENCODE`) and a custom
`llama.cpp-no-logits.patch`, plus `mtmd` image+audio paths. Reproducing that framing through a
third-party Rust binding risks silent vector drift — the one thing that must never happen. A ~500
LOC C++ program compiled against the *same pinned* llama.cpp, speaking the *exact* upstream JSON-lines
protocol, is the lowest-risk path and keeps the engine runtime-agnostic (it's just another `Embedder`
backend, exactly like the mlx-vlm reference).

**Rejected.**
- *Rust `llama-cpp-2`/`llama-cpp-sys` bindings* — may not expose the batch/process/mtmd-audio APIs;
  even if patched locally, the value is only "one language", not correctness.
- *`llama-server` HTTP* — can't express the custom per-input framing / mixed encoder batches.
- *Reimplement mean-pool/L2 in Rust over raw ggml* — needless risk.

## D3. Backend — **Vulkan** (not SYCL), on this Arc 140V

**Why.** llama.cpp's Vulkan path on Intel via Mesa ANV is the current best; it got a large boost at
Mesa 26.1 (coopmat) and this machine already runs Mesa 26.2.2 / Vulkan 1.4.354. There is a known
SYCL stall-after-first-request bug specifically on the 258V, and SYCL's batched decode lags Vulkan.
**SYCL is kept as a benchmark variant** but not the default.
A **CPU build** (`GGML_NATIVE=OFF`) ships alongside as the automatic fallback after repeated GPU
device loss (ROBUSTNESS F6). That is only allowed because it passes the same parity gate.

## D4. UI toolkit: **all QML. An Omarchy shell plugin for the panel and bar, plus a separate Quickshell instance for the app windows**

> **Revised** (supersedes the earlier "Quickshell panel + GTK4/libadwaita app" split). Details in
> `UI-DESIGN.md` §1.

**Why.**
- **One visual language.** libadwaita deliberately ignores GTK themes and draws GNOME's look. Next to
  a Quickshell panel that follows the Omarchy theme, the main window and Settings would look like a
  different product, and they wouldn't recolor when the user switches Omarchy themes. Upstream
  DigUp's polish comes from the panel, window and settings sharing one component vocabulary
  (`Components.swift`); QML on both sides keeps that.
- **Smoothness.** Qt Quick's threaded scene graph, GPU-only animation of `opacity`/`scale`/`y`, and
  `ListView` item reuse are what make it feel like macOS. One motion spec (`Motion.qml`) covers
  every surface.
- **Omarchy is a Quickshell plugin host** (verified: `omarchy-shell` = Quickshell 0.3.1 running
  `/usr/share/omarchy/shell` with a plugin registry for the `panel`, `bar-widget`, `overlay` and
  `service` kinds). As a plugin, the panel is pre-mounted (`keepLoaded`), so the hotkey toggles
  visibility in an already-running process. It inherits `Color`/`Style` tokens, and users manage it
  with `omarchy plugin enable/disable`.
- **Crash isolation where it matters.** omarchy-shell also hosts the bar, notifications and the
  **lock screen**. Only the thin panel and bar widget run there. The grid, video playback, Quick
  Look and long forms run in `trowel-app` (`quickshell -p /usr/share/trowel/app`, `FloatingWindow`),
  so a bug there can't touch the session.

**Shared code.** `ui/qml/Trowel/` (protocol client, theme bridge, motion tokens, components) is
installed to `/usr/share/trowel/qml` and imported by both hosts.

**Rejected.**
- *Quickshell panel + GTK4/libadwaita app* (the previous choice): it is less work for forms, but
  you get two looks, no live Omarchy theming, two component libraries and two motion systems. The
  Settings and onboarding forms are about 10 screens; QML forms are fine at that size.
- *Everything inside omarchy-shell*: one process, but a heavy grid, video decoding and a QML bug
  would then share a process with the lock screen.
- *Standalone `qs -c trowel` for the panel*: it cold-starts or duplicates a resident Quickshell and
  ignores Omarchy's plugin lifecycle and theme singletons. It is kept only as the non-Omarchy
  fallback.
- *Electron/Tauri/web*: heavy, a poor fit for a launcher, and mismatched with the theme.
- *Qt Widgets / C++ Qt Quick app with Rust via cxx-qt*: viable, but it adds a build system and FFI
  layer for no visual gain over a Quickshell `FloatingWindow`. Reconsider only if Quickshell's window
  APIs block something (for example, file pickers. Use `xdg-desktop-portal` FileChooser via
  `Quickshell.Io.Process` running `zenity`/portal, or a small helper).

**Open risk to spike in Phase 6.0:** a native folder picker from QML on Wayland. Prefer the
`org.freedesktop.portal.FileChooser` portal through a `trowel portal-pick-folder` CLI helper that
prints the path. Budget one day.

## D5. Process model — **resident `troweld` + short-lived helper + lazy worker**

**Why.** DigUp's insight is that heavy memory (Vision OCR models, the model itself) must leave when
idle. On Linux there's no single "app process", and multiple UI surfaces (panel, window, bar widget)
attach/detach, so we add a **resident daemon** that owns the index/watcher and *spawns* the heavy
work exactly as the Mac app does. The daemon stays small (SQLite + a lazily-loaded/released vector
matrix). This keeps the memory discipline and lets the UI restart without reindexing.

**Rejected.**
- *UI-process-owns-everything (exact Mac mirror)* — each UI restart would re-scan and re-warm;
  awkward with several UI processes (shell plugin, `trowel-app`, CLI).
- *One always-on process holding the model* — violates the memory goal on a 30 GB laptop.

## D6. Vector store — **SQLite fp16 blobs + in-memory brute force** (no ANN index)

**Why.** DigUp deliberately uses brute force ("At this scale ~20k vectors, ~30 MB… a few
milliseconds; no vector database needed") *and* the brute force is load-bearing: the per-query
z-score normalization needs the full score distribution, and `setsScale` needs to see all vectors.
An approximate index would break both. fp16 halves memory; the blocked matmul is fast. We port it
as-is.

**Rejected.** *sqlite-vec/LanceDB/usearch/HNSW* — approximate, extra dependency, and unusable for
the exact per-modality z-scoring. Reconsider only if a user has millions of vectors (then a
two-stage: ANN shortlist → exact rescore, preserving the distribution from a sample).

## D7. IPC — **Unix-domain socket, newline-delimited JSON**

**Why.** The UI needs streaming status while indexing, per-keystroke embed-ahead with low latency,
and `search`. A `$XDG_RUNTIME_DIR/trowel/troweld.sock` JSON-lines protocol (full spec: `PROTOCOL.md`) is trivial to implement in
Rust and QML (Quickshell `Socket`), with no introspection boilerplate and minimal
latency. It mirrors the existing worker protocol style the team already validated.

**Rejected.** *D-Bus* — better discoverability and a natural fit for a shell widget, but heavier
introspection/serialization for the hot per-keystroke path. **Option:** expose a *thin* D-Bus facade
for third parties later. It isn't needed: the bar widget is part of our own shell plugin and uses
the socket.

## D8. Storage/config paths — **XDG**

`~/.local/share/trowel/{models,index}` (or `$XDG_DATA_HOME`), `~/.config/trowel/config.toml`,
`$XDG_RUNTIME_DIR/trowel/troweld.sock`, thumbnails and previews in `~/.cache/trowel`, logs in journald plus
the index dir. `$TROWEL_MODELS` and `$TROWEL_WORKER_DIR` kept
for parity/testing.

## D9. Hotkey: **Hyprland Lua bind → omarchy-shell IPC**, default `SUPER + D`

Upstream's `⇧⌘Space` maps to `SUPER+SHIFT+SPACE`, which **Omarchy already uses** ("Toggle top
bar"). `SUPER+SPACE`, `SUPER+ALT+SPACE` and `SUPER+CTRL+SPACE` are taken too. Default to
`SUPER + D`, bound in a Trowel-owned `~/.config/hypr/trowel.lua`
(`o.bind("SUPER + D", "Trowel search", "omarchy-shell trowel toggle")`) that is included from
`bindings.lua`. The in-app recorder rewrites that file after a conflict check against
`omarchy menu keybindings --print`. The `GlobalShortcuts` portal is the non-Omarchy fallback.
Avoid `ydotool`/evdev key grabs. See `UI-DESIGN.md` §8.

## D10. Quick Look / Open / Reveal

- **Quick Look**: our own `trowel-app` Quick Look window, which opens at the *matched place* (PDF page
  band, code focus line, video/audio at the moment) and matches the theme. **Sushi** is only a
  fallback for kinds we can't render.
- **Reveal**: `org.freedesktop.FileManager1.ShowItems` (Nautilus present), else open the parent dir.
- **Open**: `xdg-open`/`gio open`. For a page hit, open the PDF at that page with the user's default PDF
  app when it supports it (`evince -i <n>`, `zathura -P <n>`, `okular -p <n>`, `xdg-open` otherwise);
  media at moment via `mpv --start=<t>`; code via the chosen editor (D11).

## D11. Code editor integration

Detect on PATH and support: `cursor` (and `cursor --goto file:line:col`), VS Code/Codium
(`code -g`), Zed (`zed file:line`), Sublime (`subl file:line`), JetBrains (`idea/clion/pycharm…
--line`), `nvim` (via `--server`/`nvr` or a `nvim --listen` convention), `hx`, `kate --line`.
Present here: `cursor`, `nvim`, `hx`. If none chosen, `xdg-open`.

## D12. Packaging & updates — **PKGBUILD (AUR), systemd user units, no self-updater**

`trowel`, `troweld`, `trowel-worker` installed to `/usr/bin`; a `troweld.service` (`--user`, wanted by
`graphical-session.target`), `troweld.socket`, a `.desktop` entry for `trowel-app`, AppStream metainfo, an icon, the shared QML
module in `/usr/share/trowel/qml`, the app in `/usr/share/trowel/app`, and the **Omarchy shell plugin**
in `/usr/share/trowel/omarchy-plugin`. First run (or `trowel setup`) symlinks the plugin into
`~/.config/omarchy/plugins/io.github.thakurious.trowel` and runs `omarchy plugin enable io.github.thakurious.trowel`.
Also see `ROBUSTNESS.md` §2 for the units and §4 for uninstall. Model downloaded on first run to `~/.local/share/trowel/models`. Updates via
`pacman`/`paru`. Login item = enabling the user unit.

## D13. Model & licensing

Keep EmbeddingGemma 2 GGUF (Apache-2.0) with the **same pinned revision and SHA-256**, the same
`llama.cpp` (MIT) pin + patch, and reproduce `Acknowledgements`. No telemetry; the only network use
is the one-time model download (and optional update check, which we drop).

## D14. Testing bar

Port `swift test` equivalents and the three evals. The **vector-parity test is the gate for any
llama.cpp/Mesa change** (cosine ≥ 0.999 vs shipped reference vectors). Evals (`synthetic`, `code`,
`multilingual`) must meet/exceed DigUp's published numbers on this machine before UI polish.

## D15. Upstream tracking

This port follows **ARahim3/DigUp @ `6faa29e4c315a3a591de1b41bd78751bdf39588d`** (2026-10-11, MIT),
read from `/tmp/opencode/DigUp`. Keep a pinned clone at `upstream/` (a git submodule or a
`scripts/fetch-upstream.sh` with the SHA) so implementers can always read the Swift being ported;
`/tmp` doesn't survive a reboot. Upstream is moving, so each week:
`git -C upstream log --stat <pin>..origin/main -- Sources/DigUpKit Sources/LlamaRuntime evals`.
Any change to search math, prompts, schema or `vectorVersion` is ported **with its eval delta**
before the pin is bumped. UI-only upstream changes are triaged and logged in
`UPSTREAM-SYNC.md`. Keep upstream's MIT notice and `Acknowledgements` in the package.

## D16. Linux-only data stays out of upstream's schema

Anything Linux needs that upstream doesn't have (raw byte paths, poison-file attempt counts, polling
roots) goes in **`linux_*` tables** or `meta` keys, never as new columns on `files`/`segments`. That
keeps the schema-parity test (open an upstream DB, and have upstream open ours) meaningful and keeps
upstream migrations portable verbatim.

## D17. Name: **Trowel**

A trowel is the small hand tool archaeologists use to dig things up carefully, which is what this
does to your files, and it nods to upstream DigUp without reusing its name. Identifiers: binaries
`trowel` (CLI), `troweld` (daemon), `trowel-worker` (C++ embedder), `trowel-app` (QML windows);
Omarchy plugin id `io.github.thakurious.trowel`; XDG dirs `~/.local/share/trowel`,
`~/.config/trowel`, `~/.cache/trowel`, `$XDG_RUNTIME_DIR/trowel/`. Upstream file and type names
(`DigUpKit`, `Sources/digup`, …) keep their spelling when cited.

---

> **D18–D24** were approved by the human on 2026-10-13 ("accept all recommendations"). The
> full rationale, rejected alternatives and phases are in `LAUNCHER-PLAN.md`. The entries below are
> the binding summary.

## D18. Scope: **launcher + calculator + clipboard + file search, one panel, Super+D**

Trowel becomes a Raycast-style command bar for Omarchy. Super+D stays the only default bind.
Trowel does **not** offer to take Super+Space (the Omarchy menu). `trowel setup` *offers* the
Super+Ctrl+V takeover as an opt-in that defaults to **no** (D23). File search behavior, fidelity
invariants and eval gates are unchanged, and the new sources never enter `Searcher`.

## D19. Apps: **indexed by `troweld`, launched the Omarchy way**

A hand-written Desktop Entry parser (no new crate), `notify` watches, hidden entries exactly as
Omarchy's `launcher.hides` + `services/hidden-entries.sh`, and ranking ported from Omarchy's
`services/AppSearch.js` `fuzzyScore` (parity-gated), with frecency only as a tiebreak. Launch is
`uwsm-app -- gtk-launch <id>.desktop`. *Rejected:* the `shell.appLibrary` facade (it needs
`kind: "menu"`, does JS work in the shell process, and isn't available to `trowel-app` or the CLI).

**Amended (human, 2026-10-22): Raycast-style learning.** "Frecency only as a tiebreak" is replaced
by Raycast's model, ported from oma-cast (`typecraft-dev/oma-cast` `90efedd`,
`plugin/core/Frecency.js` and `Ranker.js`): one frecency anchor per item (10-day half-life, +100 per
open) and the last 20 queries the item was opened with (forgotten 17 days after its last open).
A query the user opened an app or command with before (exact, or as a prefix) now ranks it above
better text matches, and an exact name still wins over a remembered prefix. This covers apps and
built-in commands only (human: not clipboard entries, not files; `Searcher` is untouched). With
nothing learned the order is still exactly Omarchy's, and the parity gate runs that way. The panel
also "pops to root" Raycast's way: it reopens where it was left for `ui.pop_to_root_seconds`
(default 90) after closing, then opens blank.

## D20. Clipboard: **Trowel's own capture and store**

`troweld` supervises `wl-paste --watch trowel clip-capture`, with metadata in
`~/.local/share/trowel/state.sqlite` (separate from `index.sqlite`, so D16 is untouched) and blobs
in `~/.local/share/trowel/clipboard/` with refcounted GC. Omarchy's history is imported once and
never modified. Retention defaults: **1000 entries, 30 days, 512 MB of blobs**, and pins are never
evicted.

## D21. **The daemon may write the clipboard and send the paste keystroke**

This supersedes the PROTOCOL v1 note that "the daemon never touches the clipboard". The UI hides
the panel, then the daemon runs `wl-copy` (with the stored MIME) and, for paste, Omarchy's
`wtype -M shift -k Insert -m shift` after the measured delay. `copy_path` keeps its v1 behavior.

## D22. **Protocol v2**

PROTOCOL v1 plus the `apps_*`, `clip_*` and `calc*` ops and the `clipboard` event, as specified in
`LAUNCHER-PLAN.md` §3. `hello` capabilities gain `apps`, `clipboard` and `calc`. The tables move
into `PROTOCOL.md` in the L1 commit that implements them.

## D23. Coexistence with `omarchy.clipboard`

Both watchers may run. `trowel setup` **offers** two changes and defaults both to no: disabling
`omarchy.clipboard` (`disabledPlugins[]`), and rebinding Super+Ctrl+V to
`omarchy-shell trowel clipboard`. Each change is backed up as `<file>.bak.trowel-<ts>` and
reverted by `trowel uninstall`.

## D24. Calculator: **`fend-core`, in-process, offline**

`fend-core` is approved as a dependency. It evaluates inside `troweld` with a 20 ms interrupt.
**No currency conversion** (D13: no network). *Rejected:* `qalc` (a system package, subprocess per
keystroke, network for currency) and a hand-written evaluator (no units or bases).

## D25. **No bar widget** (human, 2026-10-16)

The `trowel` plugin is a `panel` only. The bar icon (upstream `StatusMenu`: index progress ring,
paused state, a popup with search/pause/folders/settings/quit) is removed because the human doesn't
need it. Its jobs live elsewhere: the panel's footer shows indexing progress, `Ctrl+K` pauses the
clipboard, and `trowel-app` holds settings. `status.clip_paused_until` stays in the protocol.

## Related approvals (2026-10-13)

- `wl-clipboard-rs` is **not** approved yet. Decide after spike L0.3 shows whether file paste needs
  a multi-MIME offer.
- Order of the extras: Omarchy commands → window switcher → clipboard OCR.
- `qmllint` for plugin QML stays a **local-only** check (no Omarchy tree in CI).
- No zbus. F7 stays on `gdbus monitor` + sysfs.
- No Jira logging.
