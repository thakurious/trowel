# Trowel

**A Raycast-style command bar for [Omarchy](https://omarchy.org): app launcher, inline calculator,
clipboard history, and on-device search inside your files, all on Super+D.**

The file search is a Linux port of [ARahim3/DigUp](https://github.com/ARahim3/DigUp) (macOS). It finds
screenshots, photos, PDFs, documents, audio, video and code by what's *in* them ("payment error
screenshot", "the part where they talk about sleep"), using Google DeepMind's **EmbeddingGemma 2** on
your own GPU. Nothing leaves the machine.

## What you get

Press **Super+D** and type.

| You type | You see | Enter does |
|---|---|---|
| `fire`, `chro`, `slack` | **Applications**, ranked the way Omarchy's launcher ranks them, plus how often you launch them. An app whose name starts with what you typed comes first. | Launches it (`uwsm-app -- gtk-launch`, like Omarchy) |
| `5 ft to cm`, `1920*1.25 + 15%`, `0xff to binary`, `3 GiB in MB` | A **calculator card** above the results (offline: units, bases, percentages, functions) | Copies the result (`Ctrl+Enter` pastes it) |
| `invoice from march`, `zebra in a video` | **Files**, grouped by kind, with a preview: picture, PDF page with the match tinted, code lines, a passage, an audio or video moment | Opens it at the place that matched |

**Commands** work like Raycast's: type `clipboard` (or `clip`, `paste`, `history`) and press Enter
on **Clipboard History** to open your clipboard inside the panel: text, links, colors, code,
formatted text, images and copied files, searchable, with a preview and pins. Enter pastes into the
app you were in; `Ctrl+Enter` only copies; `Ctrl+Delete` removes it; `Shift+Enter` pastes as plain
text; Tab cycles All · Text · Images · Files · Links · Colors (Images is a thumbnail grid); `Esc`
goes back to your search. (`omarchy-shell trowel clipboard` opens the history directly, for a key
binding.)

More commands, each a view inside the panel:

- **Timer**: `25m tea`, `1h 30m`, `at 15:30` (typed in the main search too). It counts down on
  **the Island**, a small pill that grows out of the bar's bottom edge (hover it for Pause,
  +1 min, Cancel) and rings, notifies and expands when it's done. Timers survive a restart.
- **Process Monitor**: what's using the CPU, graphics, memory and disk, per app or per process,
  sorted with Tab; ↵ twice ends one (processes your session needs are protected).
- **Snippets** and **Create Snippet**: text you type often, with `{date}`, `{clipboard}` and
  `{argument:name}` blanks; type a snippet's keyword (`;sig`) in the main search to paste it.
- **System commands**: Lock Screen, Sleep, Log Out / Restart / Shut Down (↵ twice), toggles with
  their state (Night Light, Keep Awake, Do Not Disturb, Hide Bar, Touchpad), Restart Wi-Fi /
  Bluetooth / Audio / Shell, Next Background, **Pick Theme**, and **Omarchy Commands** (every
  `omarchy-*` script, searchable).
- **Lock Keyboard** (to clean it; the Island shows the countdown and an Unlock button) and
  **Trowel Settings** (`Ctrl+,`).

`omarchy-shell trowel desk timers|procs|snippets` opens a view directly.

Everywhere: **`Ctrl+K`** opens the actions for the selected item (an app's desktop actions, hide an
app, pin/delete a clip, the calculator's hex/binary forms, reveal a file, …). `↑`/`↓` move, `Esc`
closes. Full keyboard map: [UI-DESIGN.md §3.4](UI-DESIGN.md).

The Trowel app window ("Trowel" in the app launcher, or `quickshell -p /usr/share/trowel/app`) has search,
Quick Look and Settings: folders, code folders, the shortcut, and **Launcher & Clipboard** (capture
on/off, how much to keep, apps never to record, import Omarchy's history, hidden apps).

## Install

Requirements: Omarchy (Hyprland 0.56 with the Lua config, omarchy-shell / Quickshell 0.3.x),
`wl-clipboard`, `wtype`, `libvips`, `poppler`, `tesseract`, `ffmpeg`, a Rust toolchain, and ~1 GB for
the model.

```bash
git clone --recurse-submodules https://github.com/thakurious/trowel.git && cd trowel
scripts/build-llama.sh                 # the embedding worker (llama.cpp, Vulkan)
cargo build --release
target/release/trowel setup --from-tree "$PWD"
```

`setup` installs `trowel`, `troweld` and `trowel-worker` into `~/.local/bin`, the omarchy-shell plugin
(`io.github.thakurious.trowel`), the Super+D bind, and the `troweld.socket` user unit. Every config
file it touches is backed up as `<file>.bak.trowel-<timestamp>`. **Restart omarchy-shell once**
(`omarchy restart shell`), press **Super+D**, and pick the folders to search; the model (865 MB)
downloads once. A packaged install is `makepkg -si` in `packaging/`, then `trowel setup`.

```bash
trowel doctor              # what's wrong, and how to fix it
trowel uninstall           # plugin, bind and units
trowel uninstall --purge   # also ~/.local/share/trowel (index, clipboard history) and config
```

Clipboard history coexists with Omarchy's own (Super+Ctrl+V): both can run, and reading the
clipboard is harmless.

## Command line

```bash
trowel calc "5 ft to cm"            # 152.4 cm
trowel apps fire                    # what the launcher would show
trowel launch firefox               # launch and count it, like the panel
trowel clip list [query] [--kind link]
trowel clip get 812 | paste 812 --mode paste | pin 812 | delete 812 | clear [--all]
trowel clip pause [minutes] | resume | import-omarchy
trowel search "payment error screenshot"
trowel status | doctor
trowel timer start 25m tea | list | cancel 3
trowel island post backup "Backing up" --progress 0.4 --button stop:Stop   # any script can
trowel island watch backup --once                                           # prints the button pressed
trowel keyboard lock 30 | unlock | status
```

## Privacy

Everything runs on this computer. Trowel goes online only to download its model, once. Clipboard
history lives in `~/.local/share/trowel/state.sqlite` and `~/.local/share/trowel/clipboard/` (owner
only, `0600`), is capped at 1000 items / 30 days / 512 MB by default (pins are kept), and skips copies
an app marks as a password, copies from 1Password, Bitwarden and KeePassXC, and any app you add in
Settings. Pausing (Ctrl+K → "Pause capturing", or `trowel clip pause`) stops the watcher entirely.

## How it works

- **`troweld`** (Rust, systemd user service): the file index (crawler, extractors, `trowel-worker`
  running EmbeddingGemma 2 through llama.cpp on Vulkan, hybrid vector + SQLite FTS5 search), the app
  index (Desktop Entry parser, Omarchy's ranking, inotify), the calculator
  ([`fend-core`](https://github.com/printfn/fend), in-process, 20 ms limit), and the clipboard (a
  supervised `wl-paste --watch`, a content-addressed blob store with refcounted GC).
- **UI**: an omarchy-shell plugin panel on the Overlay layer and a separate `trowel-app` Quickshell
  window, both following the live Omarchy theme. They speak JSON lines to `troweld` over
  `$XDG_RUNTIME_DIR/trowel/troweld.sock` ([PROTOCOL.md](PROTOCOL.md), v3). Timers, snippets and
  the process sampler live in the daemon too; the panel and the Island only draw.
- **Fidelity:** file search matches upstream DigUp's vectors (cosine ≥ 0.999 against reference
  vectors), prompts, tokenizer and search math, and its recall evals, so results are the same as on a
  Mac.

## Status

| Part | State |
|---|---|
| File search (Phases 0–7) | Done and in daily use: indexing, hybrid search, previews, code search, onboarding, settings, `doctor`, packaging. The 72-hour soak and a fresh-account `makepkg -si` are still open. |
| Launcher, calculator, clipboard (Phase 8, L0–L4) | Built and installed on the development machine. Engine budgets measured (apps 0.3 ms, calc p95 0.4 ms, clipboard query p95 0.7 ms, image ingest 18 ms for 18 MB); UI checked offscreen against the mock; live acceptance in progress. |
| Next | L5 hardening (setup prompts for the Super+Ctrl+V takeover, more `doctor` rows, failure handling), then upstream DigUp 0.6.1 parity (8U). |

Day-to-day progress, measurements and open questions are in [PROGRESS.md](PROGRESS.md).

## Development

```bash
cargo test --workspace                        # Rust: units, daemon, protocol transcripts, clipboard
cargo clippy --workspace --all-targets -- -D warnings
node scripts/listmerge-test.mjs               # the panel's list rules (ListMerge.js)
python3 -I ui/harness/check.py                # the real panel, offscreen, against troweld --mock
scripts/clip-live.sh                          # clipboard round trips on the real compositor
troweld --mock --socket /tmp/t.sock           # canned daemon for UI work
quickshell -p ui/gallery                      # every component, live-themed
```

Rules for contributors (human or agent) are in [CLAUDE.md](CLAUDE.md): upstream source in `upstream/`
is the behavioral truth, fidelity invariants and test gates never move, and docs change with the
code.

## Documents

| Doc | What it covers |
|---|---|
| [ARCHITECTURE.md](ARCHITECTURE.md) | How DigUp works (engine, runtimes, data model, algorithms) and the Linux architecture. |
| [PORT-MAP.md](PORT-MAP.md) | Every DigUp component and its Linux replacement, and what must match bit for bit. |
| [DECISIONS.md](DECISIONS.md) | The load-bearing decisions (D1–D25) with rationale and rejected alternatives. |
| [EXECUTION-PLAN.md](EXECUTION-PLAN.md) | Phases, tasks, deliverables and acceptance gates. |
| [LAUNCHER-PLAN.md](LAUNCHER-PLAN.md) | Phase 8: launcher, calculator and clipboard design, and what building it found. |
| [PHASE9-PLAN.md](PHASE9-PLAN.md) | Phase 9 (draft): the Island, Process Monitor, System Commands, Timers, Snippets. |
| [PROTOCOL.md](PROTOCOL.md) | The `troweld` socket protocol v2 (search, previews, status, config, apps, clipboard, calculator). |
| [UI-DESIGN.md](UI-DESIGN.md) | Theme mapping, panel geometry, keyboard map, motion, smoothness budgets, visual QA. |
| [ROBUSTNESS.md](ROBUSTNESS.md) | Failure modes (inotify limits, poison files, GPU loss, suspend, disk full, corruption) and their handling. |
| [BENCHMARKS.md](BENCHMARKS.md) | This machine's performance targets and measurements. |
| [TESTING.md](TESTING.md) | Vector parity, recall evals, protocol conformance and fault injection. |
| [PROGRESS.md](PROGRESS.md) | Where things stand, session by session. |

## Non-goals

- No self-updater (pacman/AUR), no telemetry, no cloud, no account.
- Not a fork of DigUp: an independent Linux implementation that shares its model, index semantics,
  prompt framing and search math so results match. Upstream is pinned at `upstream/` (`6faa29e`, MIT).
