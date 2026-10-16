# Progress

Agents: update this at the end of every session (see `CLAUDE.md`). Newest session first.

## Current state

- **Phase:** 8 (`phase-8-launcher`). D18–D24 approved. **L1, L2c, L2 and L3 done** (protocol v2, `state.sqlite`, mock; calculator, app index and clipboard history served by the real daemon). L0.1–L0.4, L0.6, L0.7 verified; L0.5 partly (no hint seen, but no real password-manager copy yet). Phase 7 leftovers (72 h soak, fresh-account `makepkg -si`) still open and independent.
- **Live (deployed 2026-10-16 12:54, human's request):** `troweld` (L4 build, protocol v2, clipboard capture on) and the plugin panel (`~/.local/share/trowel/omarchy-plugin/{Panel.qml,manifest.json}` copied; `Trowel/` links to this repo's `ui/qml/Trowel`). The bar widget is removed (D25). Verified live: `hello` capabilities incl. apps/clipboard/calc; `apps_query chro` 2.7 ms → Chromium, Google Chrome; `calc 5 ft to cm` → 152.4 cm; clipboard doctor row OK (watcher running); panel opens on `omarchy-shell trowel open` and closes. Earlier, at 10:17 the old pre-L1 daemon was killed by the systemd watchdog (30 s, just after resume) and restarted on the L3 build, so clipboard capture has run since then.
- **Branches:** hubness experiment parked on local `experiment/hubness` (`a855b2d`, not adopted, not pushed).
- **Next step:** the one-step live deploy, **with the human's go-ahead** (it restarts troweld and omarchy-shell and replaces the installed plugin's `Panel.qml`/`BarWidget.qml`): `cargo build --release && cp ui/omarchy-plugin/{Panel,BarWidget}.qml ~/.local/share/trowel/omarchy-plugin/ && systemctl --user restart troweld.service && omarchy restart shell`. Then the L4 live acceptance and the L3 24 h orphan check, then L5. Open: question 2 (HTML restore; `clip_paste` is wired as the plan says), the L0.5 password-manager copy.

### Phase 8 map

| Step | What | Status |
|---|---|---|
| L0.1 | launch from the daemon's context | ✅ (Omacalc, see Verified facts) |
| L0.2–L0.4 | MIME offers from real apps, file re-offer, paste keystroke | ✅ (2026-10-15, see Verified facts) |
| L0.5 | password-manager hints | ◐ no hint seen in 20 real copies; a real password-manager copy still untested |
| L0.6 | one `wl-paste --watch` for every type | ✅ |
| L0.7 | fend-core | ✅ (2026-10-13) |
| L1 | proto v2, `state.sqlite`, mock, v2 transcripts | ✅ this session |
| L2 | apps index: Desktop Entry parser, `AppSearch.js` port + parity gate, icons, `trowel apps/launch` | ✅ this session |
| L2c | calculator: fend-core wrapper, gate/reject rules, 146-row table, false-positive corpus | ✅ |
| L3 | clipboard engine: capture, classify, dedup, retention/GC, restore, import | ✅ 2026-10-15 (live script + budgets pass); the 24 h zero-orphan check waits for the live deploy |
| L4 | UI: merged list, CalcCard, AppRow/ClipRow/ActionPanel, clipboard view, settings tab | ◐ built and checked offscreen against `--mock` (2026-10-16, slices 1–2). Left: the **live acceptance** (needs the deploy: `fire⏎` < 150 ms, paste into Alacritty/Chromium, UI-DESIGN §5 budgets, P4 evals unchanged) and the theme/scale screenshot matrix |
| L5 | setup prompts, doctor rows, F-C1..F-C5, PKGBUILD | after L3/L4 |
| L6 | extras: Omarchy commands → window switcher → clipboard OCR | backlog |
| 8U | upstream 0.6.1 parity U1–U12 | U1 first; independent of L* |

## Gates

| Gate | Status | Evidence |
|---|---|---|
| P0 vector parity ≥ 0.999 (Vulkan) | ✅ | `./scripts/parity.sh vulkan` worst=`frame-70` cosine=**0.999975**. Full output in session log. |
| P0 vector parity ≥ 0.999 (CPU) | ✅ | `./scripts/parity.sh cpu` worst=`speech` cosine=**0.999740**. Full output in session log. |
| P0 panel spike (blur, focus, IME) | ✅ | Blur A/B + exclusive focus (earlier). Overlay IME: panel open → Mozc for ~1s → `wtype nihongo` → field shows **にほんご** with Mozc candidate popup (日本語 / 日本語と英語 / 日本語教育) over `trowel-panel`. Screenshot `/tmp/trowel-spike/panel-preedit.png`. English restored immediately (`keyboard-us`, DefaultIM unchanged, panel dismissed). |
| P1 unit tests + protocol frozen + `--mock` | ✅ | `cargo test --workspace`: trowel-core **31/31**, trowel-proto **5/5**, proto_replay **3/3** (golden transcripts, `too_large`, slow-reader keeps responses / drops stale status). FTS tokenizer holds কুমির* / المكتبة. Schema 1→5 migrates. `python3 scripts/extract-strings.py` → **202** strings in `ui/strings.json`. |
| P2 extractors + traps | ✅ | `cargo test --workspace`: trowel-core **70/70**, trowel-proto **5/5**, proto_replay **3/3**. Traps classify (TS vs MPEG-TS, project screenshots-only, leading-dash / newline / 255-byte / invalid-UTF-8 names, 0-byte MP4, symlink loop, truncated PDF and fake docx fail with a reason). `sleep 30` killed in 400 ms. `trowel estimate --json` over a Linux-built testbed: 22 files, dataset 400 skipped, 3 screenshots, git code crawl keeps tracked+untracked-not-ignored only. No Mac in this environment — counts are Rules-faithful Linux goldens, not a numeric macOS dump. |
| P3 pipeline + crash/resume | ✅ | `cargo test --workspace`: trowel-core **71/71** + long_files **13/13**, trowel-cli pipeline **3/3**, proto **5/5**, proto_replay **3/3**. `scripts/crash-resume.sh`: vectors **10→10** after `kill -9` ×50, no re-embed of finished files. Poison n≥2 → `Couldn't be read safely`. Touch one file → `changed=1`. Keyword search finds the indexed note. |
| P4 evals meet TESTING §3 | ✅ | Linux testbed `/tmp/trowel-eval` (no Mac). `synthetic.json` ALL **37/37 R@1=1.00** (TESTING: 0.88). `code.json` **41/41**. `multilingual.json` ALL **R@1=0.92** / R@5=0.98 / R@10=1.00. 0 expected answers folded on all three. `real/` not run (CLAUDE.md: no `$HOME` until Phase 5). |
| P5 daemon + robustness + doctor | ✅ | `cargo test -p troweld --test daemon` **9/9** (folder-change, overflow SIGUSR1, suspend/resume SIGUSR2, idle RSS ≤120 MB + `release_vectors`, estimate+subscribe+doctor). `trowel doctor` exit 0, no FAIL (daemon/index WARN: units not enabled, `$HOME` not indexed). systemd written, **not enabled**. F7 via `gdbus` PrepareForSleep + sysfs, no zbus. |
| P6 UI checklist + budgets + visual QA | ✅ | `cargo test --workspace` green including `ui_phase6` **3/3** (two-phase place badges `p. 3` / `0:06` / `L12–40`, F18 SIGSTOP stall + CONT reconnect, kill connect fails &lt;2s) and `cli_ui` **4/4** (`ui-trace` vs `--mock` meets IPC budgets). `trowel debug ui-trace --json`: hello 0.75 ms / panel_opened 0.29 ms / keyword_refund 0.57 ms / hybrid_zebra 0.56 ms (budgets 50/50/40/200). Layout snapshots in `design/linux/*.json`. Honest gaps: no macOS refs, no live QSG on keepLoaded shell, IME not retested this phase. |
| P7 fault injection + packaging + soak | 🟡 | `trowel setup` installed plugin+socket on this user. `cargo test` includes `setup --no-enable` and F10 corrupt-index quarantine. **Not done:** 72 h `$HOME` soak, `makepkg -si` on a fresh account, SYCL vs Vulkan bench, remaining TESTING §7 rows (hang stub, device-lost ×3, tmpfs disk-full, unmount). |

## Verified facts

- **upstream/** is the submodule at `6faa29e` (`v0.6.0-3-g6faa29e`). Read-only.
- **llama.cpp** `b11461` cloned at `cpp/llama/src` (gitignored). `llama.cpp-no-logits.patch` applied. CMake: `GGML_NATIVE=OFF`, Vulkan build `GGML_VULKAN=ON`, CPU build `GGML_VULKAN=OFF`, `LLAMA_BUILD_MTMD=ON`, `BUILD_SHARED_LIBS=OFF`.
- **llama.cpp b11461 APIs used by the worker** (headers in `cpp/llama/src`): `llama_batch_ext_init/clear/free/add_token/add_embd/set_pos/set_output_embd`, `llama_process(..., LLAMA_PROCESS_TYPE_ENCODE, ...)`, `mtmd_init_from_file`, `mtmd_tokenize`, `mtmd_default_marker()`, `mtmd_bitmap_*`. ggml Vulkan logs at DEBUG are filtered by our WARN-only callback, so the device name does **not** appear on the worker stderr; GPU identity is from `vulkaninfo`.
- **GPU:** `vulkaninfo --summary` → `Intel(R) Graphics (LNL)`, Mesa 26.2.2-arch1.1, Vulkan 1.4.354, `VK_KHR_cooperative_matrix` rev 2, `cooperativeMatrix = true`.
- **Models** at `~/.local/share/trowel/models`, HF rev `bfcd298762cc34d0357ece5ebdd31791a3a374d8`:
  - `embeddinggemma-2-Q8_0.gguf` 310 MB SHA-256 `2188ac1deca4b77dffefd603c2776a9d76d9d74ec01841392982ebb840b09135`
  - `mmproj-embeddinggemma-2-Q8_0.gguf` 555 MB SHA-256 `c4a8a52691ecef40618438928bdf9e68379b854e24166f292592353db0aab64f`
- **Hyprland 0.56 Lua** (`/usr/share/hypr/stubs/hl.meta.lua:555`): `HL.LayerRuleSpec` fields include `match`, `blur`, `ignore_alpha`, `no_anim`, `animation`. `hl.layer_rule` at stub line 855. Runtime `hyprctl keyword` **does not work** with the Lua parser (`keyword can't work with non-legacy parsers. Use eval`); use `hyprctl eval 'hl.layer_rule({...})'` or edit the Lua file + `hyprctl reload`.
- **omarchy-shell 0.3.1 / plugin contract:** user plugins live in `~/.config/omarchy/plugins/<id>/manifest.json`. Catalog uses `find -L` mindepth 2. After a new symlink, `omarchy-shell shell rescanPlugins` must run **before** `omarchy plugin enable`. IPC: `omarchy-shell trowel toggle` hits `IpcHandler { target: "trowel" }`. `omarchy-shell shell toggle <id>` is the generic path. `listPlugins` reports the spike `enabled: true`.
- **SUPER+D bind:** `hyprctl -j binds` shows modmask 64 (SUPER), key `D`, description `Trowel search`, dispatcher `__lua`.
- **omarchy-shell IME env** (pid of the shell, `/proc/<pid>/environ`): `QT_IM_MODULE=fcitx`, `INPUT_METHOD=fcitx`, `XMODIFIERS=@im=fcitx`, `SDL_IM_MODULE=fcitx`.
- **fcitx5:** running (`fcitx5 --disable notificationitem`). `fcitx5-mozc` installed (`/usr/lib/fcitx5/fcitx5-mozc.so`, IM unique name `mozc`). Adding an IM by rewriting `~/.config/fcitx5/profile` while fcitx is running is overwritten on exit; use dbus `SetInputMethodGroupInfo`. **DefaultIM is `keyboard-us` again** (human asked to revert to English). Mozc remains a second item in the Default group. State=1 inactive.
- **qmllint** is `/usr/lib/qt6/bin/qmllint`, not on `PATH`. Plugin QML imports `qs.Commons` / `qs.Ui` which only resolve inside omarchy-shell; standalone qmllint warns `Failed to import` + `Unqualified access` on `Color`/`Style`. Runtime is fine.
- **CI** cannot run the vector-parity gate (no Arc 140V on GitHub-hosted runners). Documented in `.github/workflows/ci.yml`.
- **pdftotext argv:** `pdftotext -layout -f N -l N - -- FILE` treats `-` as the PDF and errors `Document stream is empty`. Correct: `pdftotext -layout -f N -l N -- FILE -` (file then stdout `-`, after `--`).
- **F5c vs later steps:** `linux_attempts` is per file. Overlapping prepare of the next step of the *same* long file must not increment `n` (kept in `Indexer.in_flight`); otherwise `finishLongFiles` poisons a healthy PDF/doc after two steps.
- **Code eval testbed must not sit inside another git repo.** `GitRepos.enter_root` walks parents for `.git` (Swift does too). `testbed/out/code` under this repo made `scripts/backup_photos.sh` look gitignored → 39/41. `/tmp/trowel-eval/code` → **41/41**.
- **Eval `@range`:** upstream `evaluate` uses a closed range `[a,b]` (`loc` contained; code stretches overlap). TESTING.md said `[a,b)`; the doc now matches Swift.
- **Query encoder:** text-only `trowel-worker --text-only`. Fake indexes (`vector_source` starts with `fake@`) reuse `FakeEmbedder` so CLI tests do not spawn the GPU worker.
- **Linux eval corpus** (`scripts/testbed/make_eval_corpus.py`, default `/tmp/trowel-eval`): Wikimedia Commons photos (not `/Library/User Pictures`); Chromium `--print-to-pdf` for a logical Unicode text layer (pangocairo/pango-view `--paper` is missing here and also splits Arabic letters); Google `translate_tts` HTTP (no `say` / espeak); Pillow screenshots at the Hyprland mode (1920×1200) so the monitor heuristic classifies them; RTF via a builtin `\uN` decoder when `unrtf` is absent. Never samples `$HOME`.
- **pango-view** on this distro has no `--paper` / no cairo paper backend. Do not pass `--paper=letter`.
- **Eval index** should `--exclude $TB/traps/linux`. Those crawler-poison files (`x.txt`, `-notes.txt`) are Phase 2 traps, not in upstream evals; leaving them in the index stole R@1 on image/multilingual queries.
- **`trowel index` fingerprint:** do not pre-check a guessed `worker_source()` string. The live worker reports `google/embeddinggemma-2@bfcd298 llama.cpp-vectors1 text-q8_0+mmproj-q8_0`; a second index run would refuse to resume. `Indexer::full_worker` already `adopt()`s the real line.
- **Phase 5 daemon does not crawl default `~/Desktop`… roots.** `onboarded=false` until the user finishes onboarding; F1 sync and the watcher start only then. Tests pass `--index` / `--config` under `/tmp`.
- **New crates (named in DECISIONS / PROTOCOL):** `notify` 6.1 (inotify), `toml` 0.8 (`config.toml`), `libc` 0.2 (`flock`, `SO_PEERCRED`, `statvfs`/`statfs`, signals). `base64` already a workspace dep (PROTOCOL paths + worker vectors). **No zbus:** F7 is `gdbus monitor --system --dest org.freedesktop.login1` for `PrepareForSleep`, plus `/sys/power/suspend_stats/success`. Tests inject SIGUSR1 (overflow rescan) and SIGUSR2 (sleep toggle).
- **Helper binary:** `$TROWEL`, else sibling of `troweld` named `trowel`, else `PATH`. Tests set `TROWEL` to `target/debug/trowel` and `TROWEL_LINGER=3`. Child gets `PR_SET_PDEATHSIG=SIGTERM` so a test `kill()` of troweld does not leave `index-service` holding the DB.
- **PowerPolicy:** sysfs `/sys/class/power_supply/*/type` Mains `online` / Battery `Discharging`. Gates `--backfill` only; `index_on_battery=true` never blocks.
- **Query encoder:** text-only `trowel-worker --text-only` (`TROWEL_WORKER` or `cpp/build-{vulkan,cpu}/trowel-worker`). Idle `encoder_idle_seconds` (default 600) closes the worker and `release_vectors`. Fake indexes still use FakeEmbedder.
- **F3/F4:** `statfs` NFS/SMB/CIFS/FUSE → 30 min poll, no inotify. Watch setup failure sets overflow → F1 sync.
- **F9:** `statvfs` <500 MB or <5% → `paused_reason=disk_full`.
- **F10:** `PRAGMA quick_check` on open; failure quarantines to `index.sqlite.corrupt-<ts>` and starts fresh. Index dir `0700`, DB `0600`.
- **set_hotkey** writes `$XDG_CONFIG_HOME/hypr/trowel.lua` and does **not** `hyprctl reload`.
- **Hyprland 0.56 `hl.window_rule`** (`/usr/share/hypr/hyprland.lua`): `{ name, match = { class, title, float }, float, size, move, ... }`. `packaging/hypr/trowel-app.lua` matches class `trowel-app` **and** title `Trowel` because Quickshell `appId` is readonly and the window class may be `quickshell`.
- **`trowel debug ui-trace` / `snapshot`:** IPC timings vs `--mock` are the measurable PROTOCOL/UI-DESIGN §5 proxy. Live QSG frame times need a keepLoaded omarchy-shell restart (not done). Snapshots write layout+results JSON under `design/linux/`; no grim, no macOS references.
- **Quickshell 0.3.1 qmlscanner:** QML modules must live **inside** the `-p` config folder. `import "../qml/Trowel"` is rejected (`Client is not a type`; log: `Module path is outside of the config folder` / `qrc:/qs-blackhole`). Fix: `ui/{app,gallery,omarchy-plugin}/Trowel` → symlink `../qml/Trowel` and `import "Trowel"`. Verified: `quickshell -p ui/app` → `Configuration Loaded`. `font.pixelSize` must be int (9.5 broke `WeakerFold`).

- **`worker_source()` must equal the live worker hello line.** The old guess was `embeddinggemma-2@bfcd298 llama.cpp-vulkan q8_0`. Live is `google/embeddinggemma-2@bfcd298 llama.cpp-vectors1 text-q8_0+mmproj-q8_0` (`cpp/embedder.h`). `index-service` sets `embedder_source` and `reset_on_model_change`, so `check_model` called `reset_all()` on every helper start after the first embed — vectors vanished, fingerprint dropped. Fixed to the C++ `WorkerInfo` string (backend must not appear). Tests: `matching_embedder_source_keeps_vectors`, `guessed_backend_source_resets_when_callback_is_set`, `worker_source_matches_llama_cpp_hello`.
- **Power:** plug-in must send `backfill on` (or start a helper with `--backfill`), not only `sync`. A helper started on battery keeps `backfill_allowed=false` until that command. `apply_backfill_policy` matches upstream `setBackfillBlocker`. Clear `helper_starting` on helper exit or the next spawn is skipped forever.
- **fend-core 1.5.8** (MIT, `cargo tree`: **zero** transitive deps). API: `Context::new()`, `evaluate_preview_with_interrupt(&str, &Context, &impl Interrupt) -> FendResult` (clones context, no RNG, drops empty / >50 chars / multiline / == input), `Interrupt::should_interrupt`, `get_main_result()`, `set_exchange_rate_handler_v{1,2}` (left unset → `5 USD to EUR` = no card). Spike output (release, preview mode): `5 ft to cm`→`152.4 cm` 111 µs, `3 GiB in MB`→`3221.225472 MB`, `90 min to hours`→`1.5 hours`, `0xff to binary`→`11111111`, `2^64`→`18446744073709551616`, `10/3`→`approx. 3.3333333333`, `10^10^10` interrupted at **20.0 ms**. `200 + 15%` natively = `200.15` (Raycast = 230 → rewrite). **Dice (`5d66`) ignores the interrupt**: `a-5d66-4b5c` 67 ms. False positives without the gate: URLs (lambda results), `key:2026-06-30`, `M-417`. **With the LAUNCHER-PLAN L2c gate:** apps 179 / files 509 / eval queries 126 / clipboard lines 807 → **0 cards**, worst 347 µs; 50 k random strings → no panic, max 20.03 ms. Spike code in the session scratchpad only (not committed); the rules are recorded in L2c.
- **Icon theme:** `gsettings get org.gnome.desktop.interface icon-theme` → `'Yaru-purple'` (Omarchy sets it per theme). Omarchy's own `AppLibrary` icon scan ignores the theme (first hit across all `*/apps/*`). L2 should walk `Inherits=` from the current theme → hicolor, then fall back to Omarchy's scan order.
- **`troweld` must open `code.sqlite` for `code:` searches.** CLI `cmd_search` already did (`IndexStore::CODE_FILE`); the daemon only had a main `Searcher`. `code: RouteValidato` therefore ranked Downloads/Pictures docs. Live after the fix: keyword `RouteValidator` + `code:true` returns `RouteValidator.tsx` etc., and `preview` with `code:true` is `{type:"code", lines:…}`. Segment ids are per-store; preview/open need `code` (or the last `typing` code flag) so they do not bind the same integer in `index.sqlite`.
- **This machine's `cursor` is the agent shim**, not the IDE (`cursor --help` → "No Cursor IDE installation found"). Auto editor skipped it and now picks **nvim**, launched as `xdg-terminal-exec nvim +LINE FILE`. If nvim is missing: hx, then `xdg-open` (the file's default app). TUI editors must not be spawned with stdin/stdout/stderr null.
- **Panel preview:** `Client.previewArrived` is how PROTOCOL preview replies are delivered (`ok` replies are not `event`s). `SearchWindow.qml` already used `onPreviewArrived`; `Panel.qml` only watched `onLineEvent`, so PreviewPane stayed on EmptyState (`row.name` + `row.reason` — matches the screenshot “filename + Matches what's on screen”).

- **L0.1 launch from a user service** (2026-10-15): `systemd-run --user --wait --collect uwsm-app -- gtk-launch omacalc.desktop` (user-manager env, no TTY, like `troweld.service`) → `uwsm-app` returned in **184 ms**; the app's cgroup is `app.slice/app-graphical.slice/app-Hyprland-gtk\x2dlaunch-<hash>.scope` (not the launching unit); parent pid 1193 (systemd --user); Hyprland gave it focus (`focusHistoryID 0`, was Ghostty) on the current workspace. No `XDG_ACTIVATION_TOKEN` needed. The user's default browser is Chrome (`xdg-settings`), so spikes use Omacalc, never a browser.
- **Hyprland 0.56 Lua close:** `hyprctl dispatch closewindow …` fails under the Lua parser. Use `hyprctl eval 'hl.dispatch(hl.dsp.window.close({window="class:omacalc"}))'`.
- **L0.6 `wl-paste --watch CMD` (no `--type`, wl-clipboard 2.3.0)** fires on every selection change regardless of type: text, `text/html`, `image/png` and `text/uri-list` each fired once. It also fires **once at startup** for the current selection, so capture must dedupe (content hash). The command's env has `CLIPBOARD_STATE=data`. `wl-copy --type text/html` / `--type text/uri-list` also offer `text/plain`, `UTF8_STRING`, `TEXT`, `STRING` aliases; `--type image/png` offers only `image/png`. (Matters for L0.3: one `wl-copy --type text/uri-list` already advertises plain-text aliases.) The human's clipboard was saved and restored around the spike.

- **L0.2 MIME offers** (2026-10-15, `wl-paste --watch` hook logging only types, `CLIPBOARD_STATE` and the focused window class; 20 human copies). The human's file manager is **Strata** (`io.github.lgse.Strata`, default for `inode/directory`), not Nautilus.
  - Strata file copy: `text/plain;charset=utf-8 text/uri-list application/vnd.portal.filetransfer application/vnd.portal.files` (no `x-special/gnome-copied-files`).
  - Chrome selected text with formatting: text aliases + `text/html` + `chromium/x-source-url` + `chromium/x-internal-source-rfh-token`. Address bar: text aliases only. Copy Image: `text/x-moz-url text/html image/png` + the two chromium markers (another image copy: `image/png` + markers only).
  - Omarchy screenshot: `image/png` only. Slack: `chromium/x-web-custom-data` + `text/html` + text aliases + markers. Electron apps (T3Code, Slack) look like Chrome.
  - Not observed: LibreOffice, Ghostty text, Alacritty/Telegram (not installed).
- **L0.5 sensitive hints:** in all 20 real copies and ~70 scripted ones, `CLIPBOARD_STATE=data`; no offer carried `x-kde-passwordManagerHint`. 1Password/Bitwarden desktop are not installed; whether any human copy came from a password manager is unknown. So L3's sensitive skip cannot rely on hints alone: keep the source-app exclusion list (L3 design) as the main guard.
- **Ghostty Shift+Insert:** Ghostty's default binds `shift+insert=paste_from_selection` (primary); the human's `~/.config/ghostty/config:26` overrides it to `paste_from_clipboard` (Omarchy's config). Omarchy's paste keystroke depends on that override → L5 doctor row.
- **L0.3 file re-offer** (scratch PNG, three single-type `wl-copy` offers, pasted with guarded Ctrl+V):

  | offer | Chromium page (`paste` event) | Nautilus | Strata |
  |---|---|---|---|
  | `wl-copy --type text/uri-list` | `types=Files`, file `trowel-l0.png image/png 296 B`; textarea also gets the URI text | copies `trowel-l0.png` | copies `trowel-l0.png` |
  | `--type x-special/gnome-copied-files` | nothing | copies `trowel-l0.png` | nothing |
  | `--type image/png < file` | `Files`, `image.png` (name lost) | `Pasted image.png` | `image.png` |

  **One `text/uri-list` offer pastes files everywhere tested, with the real name. No multi-type offer is needed, so no `wl-clipboard-rs`.** (`wl-copy --type text/uri-list` also advertises the text aliases, so text fields get the URI.)
- **L0.4 paste keystroke** (`scratchpad/l0/partb.py`; per trial: focus scratch window, `omarchy-shell trowel toggle` open, wait for `trowel-panel` layer, `wl-copy` token, `toggle` close, sleep d, focus guard, `wtype -M shift -k Insert -m shift`; 3 trials × 5 delays × 4 apps):

  | delay after `toggle` | Ghostty | Chromium `--app` | GTK4 `Gtk.Entry` | Qt6 `TextField` |
  |---|---|---|---|---|
  | 0 ms (sent at ~35–48 ms) | 2/3 | 3/3 | 3/3 | 3/3 |
  | 25 / 50 / 100 / 150 ms | 3/3 each | 3/3 each | 3/3 each | 3/3 each |

  The layer unmaps 93–162 ms after the close `toggle` returns. Keys sent while the layer was still mapped mostly landed anyway (the panel releases keyboard focus before it unmaps). The one miss was the first trial into a fresh Ghostty, cause not isolated. At 150 ms the layer was gone in every trial. **Keep Omarchy's 150 ms**; it clears the slowest unmap observed (162 ms from `toggle`, ~190 ms when sent) with no failures.

- **fend-core 1.5.8 stack overflow:** ~300 nested `(` overflow a 2 MiB thread stack and **abort the process** (`fatal runtime error: stack overflow`, exit 134; not catchable). 200 nested is fine; `2^` chains of 400 are fine. Guarded in `calc::gate` (≤ 256 chars, ≤ 32 deep) + a 16 MiB `troweld-calc` stack.
- **fend-core first evaluation** builds unit tables lazily: ~0.45–0.65 ms in release. Warmed in `Calculator::new`.
- **fend-core names:** `nmi` / `nautical_mile` / `NM` (not "nautical mile"); `100 c to f` is speed-of-light × 100 → no card (use `°C`/`celsius`). Approximations print 10 places, zero-padded (`314.1592653590`).

- **Quickshell 0.3.1 `DesktopEntries`** (source at tag `v0.3.1`, `src/core/desktopentry.cpp`): dirs `$XDG_DATA_HOME/applications` + `XDG_DATA_DIRS` (no nix-profile), lowest priority read first, `Hidden=true` masks, invalid files don't replace valid ones, `NoDisplay` dropped, no `TryExec`/`OnlyShowIn` handling, locale `LC_MESSAGES` then `LANG`, keys/values untrimmed, `parseExecString` keeps `%` inside quotes. Omarchy adds `hidden-entries.sh` (desktop names `XDG_CURRENT_DESKTOP:XDG_SESSION_DESKTOP:DESKTOP_SESSION` = `Hyprland:Hyprland:omarchy`) and `$OMARCHY_PATH/default/omarchy/launcher.hides` (18 ids here).
- **troweld's systemd env** has the shell's `OMARCHY_PATH`, desktop names, `XDG_DATA_DIRS` (flatpak exports included) and a `PATH` with `~/.local/bin` + mise shims, so the daemon sees the same apps and `TryExec` results as the shell.
- **GIO refuses entries with a missing `TryExec`:** `Gio.DesktopAppInfo.new("dev.zed.Zed.desktop")` → NULL (`~/.local/zed.app/bin/zed` absent), so Omarchy's Zed entry cannot launch. Trowel hides it.
- **Live app index:** 94 visible apps; hidden: 18 by Omarchy's hides, 1 by `TryExec`, 0 computed. 92/94 resolve an icon file; misses: a Wine shortcut with no `Icon=`, and `trowel` (Trowel's own icon is not installed — packaging gap for L5).

## Open questions for the human

1. **Watchdog kill after resume (2026-10-16 10:17, old v1 binary):** systemd's 30 s watchdog fired right after the laptop woke; core dump in `coredumpctl`. Check whether the current build's watchdog thread survives suspend/resume (ROBUSTNESS F7) before L5 closes.
1. **L0.5 leftover:** copy one test password from Chrome's password manager (or whatever manager is used) with the metadata logger running, to see whether it marks itself sensitive.
2. **HTML entries pasted into plain-text targets paste markup.** `wl-copy --type text/html` serves the same HTML bytes for `text/plain`/`UTF8_STRING` too (L0.6), so `clip_paste` mode `paste` of an `html` entry into a terminal or plain field inserts `<p><b>…`. Options: (a) approve `wl-clipboard-rs` to offer `text/html` + real `text/plain` in one selection (Raycast-like rich paste); (b) make `paste` restore HTML entries as plain text and keep rich HTML for an explicit "Paste formatted" action; (c) pick by the focused window's class (terminal → plain), a heuristic. Recommendation: (b) now, (a) later if rich paste is wanted. Currently implemented as the plan says (rich HTML in `paste`, text in `plain`).

Resolved 2026-10-15: `wl-clipboard-rs` is **not needed** (L0.3: a single `text/uri-list` offer pastes files into Chromium, Nautilus and Strata).

Resolved 2026-10-13 (human accepted every recommendation, see DECISIONS D18–D24): no Jira; plugin `qmllint` stays local-only; no zbus; Phase 8 decisions approved.

## Desktop changes to revert

Timestamp `20261012-133406`. Spike plugin id `io.github.thakurious.trowel-dev` (not the lock-screen, not a first-party plugin).

Backups:

- `~/.config/hypr/bindings.lua.bak.trowel-20261012-133406`
- `~/.config/omarchy/shell.json.bak.trowel-20261012-133406`
- `~/.config/fcitx5/profile.bak.trowel-20261012-133406`
- `~/.config/fcitx5/profile.bak.trowel-20261012-134601`

Live changes:

- 2026-10-16: `~/.config/omarchy/shell.json` → the Trowel bar-widget slot removed from `bar.layout.center` (D25), then `omarchy plugin enable io.github.thakurious.trowel` added it to `plugins[]` (a panel-only plugin needs that entry; the bar slot used to count). Backup `~/.config/omarchy/shell.json.bak.trowel-20261016-125417`. Revert: restore the backup and `omarchy restart shell`.

- symlink `~/.config/omarchy/plugins/io.github.thakurious.trowel-dev` → `/home/saurabh/Projects/digup-linux/ui/omarchy-plugin`
- plugin enabled in `~/.config/omarchy/shell.json`
- `~/.config/hypr/trowel.lua` copied from `packaging/hypr/trowel.lua`
- `require("hypr.trowel")` appended to `~/.config/hypr/bindings.lua`
- fcitx5 Default group now has `keyboard-us` (default) **and** `mozc` (optional). Current IM is `keyboard-us`. To drop mozc from the group: `cp ~/.config/fcitx5/profile.bak.trowel-20261012-133406 ~/.config/fcitx5/profile && fcitx5-remote -r`

Revert:

```bash
omarchy plugin disable io.github.thakurious.trowel-dev
rm -f ~/.config/omarchy/plugins/io.github.thakurious.trowel-dev
cp ~/.config/omarchy/shell.json.bak.trowel-20261012-133406 ~/.config/omarchy/shell.json
cp ~/.config/hypr/bindings.lua.bak.trowel-20261012-133406 ~/.config/hypr/bindings.lua
rm -f ~/.config/hypr/trowel.lua
hyprctl reload
```

Leave these in place until the IME re-test is done so `SUPER+D` still summons the panel.

## Session log

### 2026-10-16 — Deployed; bar widget removed; README rewritten

Human: build and make it available on this system; commit and push everything; update the README; remove the bar plugin icon.

- Bar widget removed (D25): manifest `kinds: ["panel"]`, `BarWidget.qml` deleted, docs updated (DECISIONS, PORT-MAP, UI-DESIGN, ARCHITECTURE, EXECUTION-PLAN, LAUNCHER-PLAN). Clipboard pause stays in Ctrl+K, Settings and `trowel clip pause`.
- Deployed (see Current state and Desktop changes). Found: removing the bar slot disabled the plugin, because the slot was its only `shell.json` entry; fixed with `omarchy plugin enable`.
- Found: `setup.rs` `patch_shell_json` treats `plugins` as an object, but omarchy-shell's is an array of `{id}`; it never matched (setup enables through `omarchy-shell shell enablePlugin` instead). Fix in L5.
- README rewritten for users: what Super+D does now, install, CLI, privacy, how it works, status, development, documents.
- Pushed `phase-8-launcher` to origin (not main).

### 2026-10-16 — L4 slice 2: actions, image grid, bar, settings, gallery

- `ActionPanel.qml` (Ctrl+K) with `ListMerge.filterActions` (node test); actions per selection type.
- Clipboard Images chip → 3-column `GridView` (header-less rows), ↑/↓ by three.
- `status.clip_paused_until` (proto `Status`, engine broadcasts on `clip_pause`, PROTOCOL updated; daemon test asserts it). Bar widget: paused dot, "Clipboard history…" (summon payload), pause/resume.
- `trowel-app` Settings → "Launcher & Clipboard" tab (rendered offscreen against the mock and checked).
- Gallery: CalcCard ×2, AppRow ×2, ClipRow per kind, KindChips, Chip, Toast, ClipPreview (color), ActionPanel, AppPreview.
- `trowel debug ui-trace` adds `apps_fire`, `calc_5ft`, `clip_query` (16 ms each).

Evidence:

```
$ node scripts/listmerge-test.mjs          → listmerge: 9 tests passed
$ python3 -I ui/harness/check.py           → 13/13 ok (adds app/clip action lists, Images grid, bar summon payload)
$ trowel debug ui-trace --socket <mock>    → apps_fire 0.44 ms, calc_5ft 0.29 ms, clip_query 0.55 ms (budget 16)
$ cargo fmt --check; cargo clippy -D warnings; cargo test --workspace   → clean / green
```

### 2026-10-16 — L4 slice 1: merged list, calculator card, clipboard view

Human: continue (after L3).

- `ui/qml/Trowel/ListMerge.js` (pure; `node scripts/listmerge-test.mjs`, 8 tests): apps section placement, clip rows with Pinned/Recent headers, selectable ids, default selection (calculator card first), `ago`.
- `Client.qml`: `appsQuery`, `calc`, `calcCopy`, `appLaunch`, `appHide`, `clipQuery`/`Get`/`Paste`/`Pin`/`Delete`/`Clear`/`Pause`/`Import`, one gen per source, replies routed by request id (`opFailed`/`opDone`), `clipboard` topic subscribed, `rowIndex`/`clipIndex`, `hasApps`/`hasClipboard`/`hasCalc` from `hello`.
- New components: `AppRow`, `ClipRow`, `CalcCard`, `Toast`, `Chip`, `AppPreview`, `ClipPreview`, `KindChips`; launcher glyphs in `KindIcon`.
- `Panel.qml`: calculator card above the columns (row 0, selected), apps merged into the All tab, per-kind previews, clipboard view (`omarchy-shell trowel clipboard`, toggles), keys per UI-DESIGN §3.4, footer hints follow the selection, toast.
- Fixed existing bugs: non-UTF-8 path ids vs the ListModel `rowId` role; blank group headers / zero fold count (self-bound properties).
- `troweld --mock` advertises `calc` too (transcripts updated).
- `ui/harness/` offscreen Quickshell harness + `check.py` (9 checks).

Evidence:

```
$ node scripts/listmerge-test.mjs
listmerge: 8 tests passed
$ python3 -I ui/harness/check.py
ok   strong app match is row 0 and selected
ok   weak app match follows the files
ok   calculator card is selected
ok   ↓ leaves the card only when there is a row
ok   a kind tab drops the launcher sources
ok   clipboard view: pinned first, first item selected
ok   ↓ skips headers
ok   Links chip filters
ok   opening again returns to search
$ cargo test --workspace      # all green
$ qmllint (new components, ListMerge.js, QueryField, KindIcon): 0 warnings; Client 2, PreviewPane 10, SearchWindow 46 = unchanged from before
```

- Not done: the slice-2 items in Current state; nothing deployed (the live shell still runs the v1 panel).

### 2026-10-15 — L3 clipboard engine

Human: go ahead with the clipboard engine.

- `troweld/src/clip/`: `classify.rs` (link/color/code/text; colors → hex incl. `oklch()`), `ingest.rs` (offer choice per L0.2, URI-list decode, UTF-16 BOM, image header sizes, previews, HTML tag strip, 256 KiB body cap + `txt` blob), `store.rs` (upsert/bump, query FTS AND-prefix + literal preview substring, pin/delete/clear, retention by age/count/blob bytes sparing pins, refcounted blobs written tmp+rename, GC healing refcounts and deleting orphan files), `thumbs.rs` (libvips `@64/132/264`), `watch.rs` (supervised `wl-paste --watch trowel clip-capture`, PDEATHSIG, backoff, stopped while paused/off, peer-pid ancestry check), `import.rs` (Omarchy JSON), `mod.rs` (restore per kind, coalesced `clipboard` events ≤ 4/s).
- Engine: all clip ops served, `clipboard` + `calc` capabilities, `clip_import` op (new; PROTOCOL updated), clipboard doctor row, hourly GC thread, 1 s self-copy suppression after `clip_paste`. Queries on a machine with no history never create `state.sqlite`.
- State schema 3: `clip_entries.seq` for recency (found by the daemon test: same-second re-copies didn't reorder). `[clipboard]` config section with defaults (old config files load).
- CLI: hidden `trowel clip-capture`; `trowel clip list|get|paste|pin|delete|clear|pause|resume|import-omarchy|bench`.
- `actions::copy_as(mime, bytes|file)` (one type per `wl-copy`); `PASTE_DELAY` comment cites L0.4.
- Tests: 25 clip unit tests (classifier table, store, retention, GC crash-orphan, import, events, spool check), state v2→v3 migration, `tests/clipboard.rs` (2 daemon tests with stub `wl-paste`/`hyprctl`/`wl-copy`/`wtype`: capture of text/image/files/html/color/link, dedup bump, sensitive + KDE hint + excluded app skipped, forged `clip_ingest` refused, byte-identical restore, uri-list restore + Shift+Insert, plain drops HTML, self-copy ignored, pause stops the watcher and resume restarts it, pin/delete/clear with blob release, spool emptied, events; capture off + import). `v2-clipboard.jsonl` gains `clip_import`.

Evidence:

```
$ cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings   # clean
$ cargo test --workspace   # all green: troweld lib 89, clipboard 2, daemon 18, proto_replay 3, calc 6, …
$ target/release/trowel clip bench
{"entries":1000,"query_ms":{"p50":0.3825,"p95":0.6589,"runs":300},"image_bytes":8388608,"ingest_ms":8.35,"pass":true,…}
$ target/release/trowel clip bench --image <18.5 MB real PNG>
{"image_bytes":18468537,"ingest_ms":18.21,"query_ms":{"p95":0.6593},"pass":true,…}
$ scripts/clip-live.sh      # scratch daemon on the real compositor (4 runs; last run shown)
text id=2 visible after 119 ms
png id=3 visible after 149 ms
file id=4 visible after 115 ms
html id=5 visible after 110 ms
png thumbnail 37x23 uchar, 3 bands, srgb, pngload
roundtrip image/png ok
roundtrip text/plain ok
roundtrip text/uri-list ok
html entry kind=html
PASS
```

- Found: a bare `wl-copy` spends ~150 ms in `xdg-mime` sniffing (first live runs showed text at 270–298 ms; with `--type` 100–142 ms). The daemon always passes `--type`.
- The live script overwrote the clipboard (cleared at the end) and its four test copies also went into Omarchy's own history.
- Not done: the 24 h zero-orphan acceptance (needs the live deploy); doctor row exists only in the daemon's `doctor` op (CLI `trowel doctor` runs out of process); HTML restore (open question 2); L5 setup prompts (Super+Ctrl+V takeover, disabling `omarchy.clipboard`, offering the import).

**Next step:** L4 UI on `--mock`.

### 2026-10-15 — L0.2–L0.5 spikes

Human: let's do the L0 checks now; did the Part A copies; "do it" for the guarded keystroke run.

- Logger: `wl-paste --watch hook.sh` writing only time, `CLIPBOARD_STATE`, window class and `--list-types` (never contents). Stopped afterwards.
- Part B (`scratchpad/l0/partb.py`): every keystroke sent only while the focused window was one the script opened (abort otherwise); panel opened/closed with `toggle`; all scratch windows closed; clipboard cleared at the end (the human's clipboard was not saved).
- Results in Verified facts (L0.2, L0.3, L0.4, L0.5, Ghostty Shift+Insert). Decisions that follow: files restore as `wl-copy --type text/uri-list` only; no `wl-clipboard-rs`; paste delay stays 150 ms; sensitive skip relies on the app exclusion list as well as hints.
- LAUNCHER-PLAN updated (L0 results, restore of files, open question 3 closed).
- Not done: LibreOffice/Ghostty-text offers; a real password-manager copy (open question 1).

**Next step:** L3 clipboard engine.

### 2026-10-15 — L2 app index

Human: go ahead with the app index.

- `troweld/src/apps/`: `desktop_entry.rs` (Quickshell `parseText`/scanner port, `hidden-entries.sh` port, `launcher.hides`, `TryExec`, `exec_argv`), `rank.rs` (`AppSearch.js` port in UTF-16 units; frecency tier `score > 9500`; decayed sum), `icons.rs` (GTK theme `Inherits=` chain → Omarchy scan order, one pruned walk), `mod.rs` (visible set, `AppRow`s, `launch_argv`/`spawn`, `write_hide`, inotify `watch`). `state.sqlite` schema 2 (`apps_launches.score`, v1 migrates). Daemon: `apps_query`, `app_launch` (records frecency), `app_hide`; `hello` advertises `apps`. CLI: `trowel apps [q] [--json] [--icons] [--dump-entries] [--entries F --rank-queries F]`, `trowel launch <id> [--action a]`.
- Bugs found and fixed while building: sorting by `fuzzyScore` branch first would not have reproduced Omarchy (branches aren't monotonic in score) → score-threshold tier; watcher created after the icon walk missed early installs (daemon test) → watch first; icon walk 0.7–2.4 s → 100–180 ms.
- Daemon tests now also isolate `XDG_DATA_HOME`, `XDG_DATA_DIRS`, `OMARCHY_PATH` and stub `uwsm-app`/`gtk-launch`, so no test reads or launches the real desktop's apps.
- Not done: doctor rows (L5); Trowel's own icon isn't installed; live daemon still not redeployed.

Evidence:

```
$ cargo fmt --all --check && cargo clippy --workspace --all-targets --offline -- -D warnings   # clean
$ cargo test --workspace --offline
troweld lib 65/65 (apps:: 18 — Quickshell locale/precedence, Type=Link invalid, action order, exec_argv,
  hidden-entries rules incl. CRLF, TryExec, score branches, camelCase regex, UTF-16 units, frecency tier,
  highlight ranges, decay; fixture tree: user-over-system, Hidden mask, OnlyShowIn=GNOME, TryExec, NoDisplay,
  Type=Link, subdir id kde-konsole, de_DE name, Omarchy hides, hide round trip; state:: frecency decayed sum, v1→v2)
troweld daemon 18/18 (new apps_query_launch_hide_and_install: Omarchy order chromium>calc, hides applied,
  launch argv "-- gtk-launch chromium.desktop" and "-- chromium --incognito", 3 calc launches flip the prefix
  tier, app_hide writes $XDG_CONFIG_HOME/trowel/launcher.hides, new zed.desktop visible < 1 s) — 3 consecutive runs green
everything else unchanged and green
$ scripts/apps-parity.sh
94 entries, 551 queries, 3502 ranked rows, 0 diffs
$ # mutation check: inner-match penalty ×10 → ×1 in rank.rs
94 entries, 551 queries, 3502 ranked rows, 34 diffs   (exit 1; file restored, back to 0 diffs)
$ trowel apps <q> --json   # release, 94 apps
'ghost' Ghostty 313 us | 'chro' Chromium 312 us | 'web browser' Helium 315 us | 'terminal' Foot 329 us
scan 4–8 ms, icons 100–182 ms, hidden {omarchy 18, try_exec 1, computed 0, trowel 0}
$ trowel launch omacalc      # live daemon is v1 → in-process fallback, same argv
exit=0 after 15 ms
0::/user.slice/user-1000.slice/user@1000.service/app.slice/app-graphical.slice/app-Hyprland-gtk\x2dlaunch-b25d0adf.scope
active: omacalc            # then closed with hl.dsp.window.close
$ # install latency, hand-started debug daemon: new .desktop found after 544 ms (before the entries-first swap)
```

**Next step:** L0.2–L0.5 with the human, then L3.

### 2026-10-15 — L2c calculator

Human: go ahead with the calculator.

- `fend-core = "=1.5.8"` (workspace, approved D24; `cargo tree`: no transitive deps); `[profile.dev.package.fend-core] opt-level = 3` so debug tests time like the shipped build.
- `troweld/src/calc.rs`: gate (rule 1 + every L0.7 reject rule + new ≤ 256 chars / ≤ 32 deep guard), Raycast percent rewrite (recursive), base-literal rewrite, `shape()` → `CalcResult` (approx, unit split, kinds, Hex/Binary alts, trimmed padding), locale `display`, thin-space `pretty_expr`, `Calculator` thread (16 MiB stack, `catch_unwind`, 20 ms deadline, overrun → no card, warm-up). Daemon serves `calc` / `calc_history` / `calc_copy` (`actions::copy_text` + `paste_keystroke`), `state.sqlite` opened lazily under `--state-dir` (default `$XDG_DATA_HOME/trowel`).
- Bugs the harness found and fixed: percent rewrite skipped `A + 10% + 10%`; `0XFF`; `1e-5` taken as an ID; `m^2` treated as mixed units; **multi-byte char before a `%` number panicked** (fuzz, Bengali); **deep nesting aborted the process** (above).
- Daemon tests now always run with `WAYLAND_DISPLAY=trowel-test-no-compositor` and `--state-dir` under the temp root, so `cargo test` can never write the developer's clipboard, send keystrokes, or touch `~/.local/share/trowel`.
- Not done: `calc` is not advertised in `hello` capabilities (none was specified; UI just sends it). Paste delay is Omarchy's 150 ms, unmeasured until L0.4. Live daemon not redeployed (still v1, see Current state).

Evidence:

```
$ cargo fmt --all --check && cargo clippy --workspace --all-targets --offline -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.78s
$ cargo test --workspace --offline
troweld lib 45/45 (calc:: 8 incl. rewrites/reject rules/display/shapes/dice)
troweld --test calc 6/6: expression_table (146 rows), false_positive_corpus_shows_no_cards (277 lines),
  slow_inputs_return_within_the_deadline (<25 ms), deep_input_never_crashes, fuzz_random_strings, five_feet_in_centimetres
troweld daemon 17/17 (new: calc_copy_and_history with stub wl-copy/wtype — clipboard got "152.4",
  wtype got "-M shift -k Insert -m shift", history row; calc_copy_without_a_compositor_is_an_error_and_records_nothing)
everything else unchanged and green
$ TROWEL_CALC_FUZZ=200000 cargo test --release -p troweld --test calc fuzz   # ok, 5.4 s, worst < 25 ms
$ ./target/release/trowel calc --bench tests/calc/table.tsv    # 3 runs
146 rows, 0 wrong, p50 64 µs, p95 408 µs, max 20302 µs (budget p95 < 1000 µs)
146 rows, 0 wrong, p50 52 µs, p95 275 µs, max 20165 µs
146 rows, 0 wrong, p50 43 µs, p95 237 µs, max 20327 µs      # max = 10^10^10 hitting the deadline
$ ./target/release/trowel calc "5 ft to cm" --json   # 8 one-shot runs: took_us 238–565, warmup_us 443–641
{"result":{"copy":"152.4","display":"152.4 cm","kind":"unit","unit":"cm",…},"took_us":…}
$ scripts/calc-corpus.sh    # local: installed apps, /tmp/trowel-eval names, Omarchy clipboard (--quiet)
apps 485  files 500  clipboard 193 (single-line text)
985 lines, 0 cards, worst 25 µs
193 lines, 0 cards, worst 1505 µs
```

Before the warm-up fix, the one-shot command measured 1509/1247/1081/1024/914/1008/1104/1037 µs, failing the < 1000 line on 7 of 8 runs.

**Next step:** L2 apps index (see Current state).

### 2026-10-15 — Phase 8 mapped; L0.1/L0.6 verified; L1 built

Human: map out Phase 8 and start building. Chose: commit pending work, drop hubness from the branch; asked why Firefox (it was the plan's placeholder; spikes now use Omacalc).

- Committed the pending kind-tabs / code-progress work as `95a90cb` (full suite green first). Hubness parked on local `experiment/hubness` `a855b2d`.
- L0.1 and L0.6 run and recorded (Verified facts).
- **L1:** `trowel-proto` `launcher.rs` (AppRow, ClipRow, ClipKind, ClipContent, PasteMode, ClipOffer, CalcResult, CalcReply, CalcHistoryRow) + 14 v2 ops in `ops.rs`, `PROTOCOL_VERSION = 2`. **Plan bug fixed:** `app_launch {id}` / `clip_get {id}` collide with the envelope `id` (ops are `#[serde(flatten)]`ed), so params are `app_id` / `clip_id` (test asserts `req.id` is not shadowed). `troweld/src/state.rs`: `state.sqlite` (`apps_launches`, `clip_blobs`, `clip_entries`, `clip_fts` external-content FTS5 with `IndexStore::TOKENIZER` + sync triggers, `calc_history`), `0700`/`0600`, `quick_check` + quarantine, newer schema moved aside. `troweld --mock` serves every v2 op from `fixture_launcher.json` (30 apps, all 7 clip kinds, 6 calc stubs) via `mock_launcher.rs`; capabilities gain `apps`, `clipboard`. Real daemon: v2 ops → `not_ready`, capabilities unchanged until L2/L3. `Client.qml`, `trowel debug ui-trace/snapshot`, all tests and transcripts send protocol 2; `hello-mismatch.jsonl` is now the v1-client-refused case. New transcripts `tests/proto/v2-{apps,clipboard,calc}.jsonl`.
- Docs: PROTOCOL.md v2 + §4 "Launcher (v2)"; §7 no longer claims transcripts replay against the real daemon (they never did; `daemon.rs` covers it). LAUNCHER-PLAN §3 params + L1 acceptance amended to match.
- Not done: no `state.sqlite` opened by the real daemon yet (lazily in L2 when `app_launch` needs it, with a `--state-dir` override so tests never touch `~/.local/share/trowel`). Live daemon/shell not redeployed (see Current state).

Evidence:

```
$ cargo fmt --all --check && cargo clippy --workspace --all-targets --offline -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.32s
$ cargo test --workspace --offline
trowel-proto 11/11 (new: every_known_op_has_a_variant_name, v2_defaults, 4 launcher row tests)
troweld lib 37/37 (new: 7 state::tests, 6 mock_launcher::tests)
daemon 15/15 (new: protocol_v2_handshake_and_unbuilt_ops: v1 hello → "Trowel was updated, restart it"; apps_query/clip_get → not_ready; no apps/clipboard capability)
proto_replay 3/3 (golden_transcripts now includes v2-apps, v2-clipboard, v2-calc)
core 87, long_files 13, searcher 1, cli 1+6+1+1+4, faults 1, ui_phase6 3 — unchanged, all ok
$ /usr/lib/qt6/bin/qmllint ui/qml/Trowel/Client.qml   # exit 0; 2 pre-existing warnings (lines 543/553, Quickshell socket/process types)
```

**Next step:** L2c `calc.rs` (fend-core 1.5.8 is approved: add it to `troweld/Cargo.toml`, port the L0.7 gate/reject/rewrite rules, the ~150-row table, timeout + fuzz tests, `trowel calc --json`), then L2 apps. Then run L0.2–L0.5 with the human before L3.


### 2026-10-14 — Hubness experiment (not adopted)

Human: `photo` returned `apiKey.txt` etc.; asked for a documented technical fix that doesn't hurt search.

- Measured on the live index: 105 held-out probe queries, `apiKey.txt` in 63% of top-10s, `test.html` 45%, REAPER `lyrics.html` 42%, knob sprite PNGs 20–31%; k-occurrence skewness 6.30. Same in upstream (identical `classify` (secrets skipped only in `classifyCode`), `hasWords` passes hex keys, same math).
- Built an opt-in experiment (env `TROWEL_HUB_BANK`/`_MODE`/`_BETA`/`_K`, CLI only; daemon never reads it): `Searcher::set_hub_bank_mode` with CSLS and QB-Norm Dynamic Inverted Softmax (Bogolin et al. CVPR 2022 Eq. 2), `trowel hubness` report, 1014-query bank with 0 eval overlap. Unit test `hub_correction_demotes_a_vector_close_to_every_query`.
- Result (full table in `experiments/hubness/README.md`): CSLS β 0.5 → skewness 4.61, apiKey 36%, but synthetic R@1 1.00→0.97, multilingual R@10 1.00→0.98. DIS β 20 k 1 → skewness 3.71, apiKey out of the top, but multilingual R@1 0.92→0.81. **No setting passes the eval gate → not adopted**; defaults unchanged.
- Why: testbed has 56 files with vectors, so most are in DIS's activation set and correct matches with many neighbours (Arabic Petra report, Bengali wallet screenshot, `M-417`) are punished like hubs.
- Next candidate (not built): index-side — keep content-free text (hex/base64/UUID-like) keyword-only and skip degenerate images (sprite strips), so hubs never get vectors. Needs the human's OK (changes what DigUp embeds).

### 2026-10-14 — Kind tabs in the panel

Human: group results into tabs (All, Documents, Images…), Tab switches, each tab shows only its kinds.

- Ported DigUp's search-window kind filters (`SearchWindow.swift:133/311`) into the panel: `FilterBar` under the field (All · Screenshots · Images · Documents · Video · Audio · Code). Tab / Shift+Tab cycle and skip kinds with no strong result (dimmed); Ctrl+1…7 jump (⌘1…7 upstream). Code is a tab and replaces the old Tab code-mode toggle; typing `code:` selects it; leaving it strips the prefix. A `tab Type` hint sits at the right of the strip.
- Daemon: search replies carry `counts` (strong results per kind of the **unfiltered** query); with `kinds` it runs the unfiltered search too, under the same lock. `kinds` filters after scoring, so a tab ranks exactly as All. PROTOCOL §4 + UI-DESIGN keyboard map updated; `tests/proto/{search,search-superseded,non-utf8-path}.jsonl` now expect `counts` (additive field, exact matcher kept).
- trowel-app window: Documents now sends `["doc","pdf"]` (PDFs were left out), Code switches to the code index (sent `kinds:["code"]` to the main index before), counts are per query instead of index totals.
- Evidence: `cargo test --workspace` green incl. new `kind_filter_keeps_unfiltered_tab_counts`; fmt + clippy clean; qmllint clean on changed QML. Live (guarded): `payment` → All shows Screenshots 36 / Documents 19 with Images/Video/Audio dimmed; Tab → Screenshots (32 · 7 weaker); Tab → Documents (Images skipped); Ctrl+7 → Code (60 results, `</>` icon).
- Known nit: on the Code tab the footer can say "Still reading code (0%)" while the helper is still on the main index (pass progress, not overall). Upstream sets code progress only once the main index has nothing waiting.
- Follow-ups from the human, fixed and verified live (guarded run):
  1. Tab counts blanked or jumped for a moment on a tab switch: the switch re-ran a keyword pass (other counts) before hybrid, a code reply replaced every count, and an omitted `counts` kept stale ones. Now a tab switch goes straight to hybrid (vector cached), a keyword reply never replaces the same text's hybrid counts, code replies only set Code, and no `counts` means zero. Live: Screenshots 32 / Documents 16 identical at +120 ms and +1.5 s after Tab; Ctrl+7 keeps them beside Code 60; Ctrl+1 keeps all.
  2. Kind icons near-invisible: SVGs had `stroke="#000"` and `MultiEffect` colorization multiplies the tint by luminance (black stays black). `KindIcon` now draws the same geometry with `QtQuick.Shapes` in `Theme.foreground` (new `color` property); also fixes onboarding/settings folder icons. Offscreen render of all 8 glyphs checked.
  3. Arrow keys did not scroll the list (selection was never given to the `ListView`). Panel and trowel-app lists now `reveal()` the selected row (`ListView.Contain`, with its group header when moving onto a group's first row). Live: 14 × Down keeps the selection in view.
  4. Empty kinds lit up on the Code tab: `dimEmpty` was off whenever Code was current. Now it holds whenever the query's file counts are known (a query typed on the Code tab only knows Code, so nothing is dimmed then). Live: `payment` → Ctrl+7 keeps Images/Video/Audio dimmed beside Screenshots 32 / Documents 16 / Code 60.
  5. "Indexing 0%" while code was being indexed (human). The engine's `sync` handler ignored `code`, so a code sync set `main` to 0/N and nothing corrected it (upstream routes it to `code.indexing`). Now code syncs set `status.code`, a code `file` event clears `main` (code is read only once main has nothing waiting), `idle` clears both. `Client.activePass` (main, else code) drives the badge (`Indexing N%` / `Indexing code N%`) and the bar ring. The bar ring had never shown progress: it read `status.index`/`status.indexing`, which don't exist; its paused state read `status.paused` (it is `paused_reason`). `apply_counts` was an empty stub: `status.counts` now has searchable files per kind from both stores (live: screenshots 222, images 156, documents 133, video 2, audio 1, code 12738; main sum = 514 done).
  6. Found while testing 5: `trowel index-service` required at least one main folder, so a code-only setup never indexed (usage error). PATH is optional now; both empty is still an error.
  - Test `code_sync_progress_is_not_main_progress` (code-only config, every status event must keep `main.total == 0`): passes; with the old routing restored it fails with "code indexing reported as main progress".
  - Live code pass could not be watched: the code index finished (12738 done) while the session was paused.
- Not committed (human asked for the feature, not a commit).

### 2026-10-14 — Pre-Phase-8 audit: bugs fixed, upstream 0.6.1 fixes ported

Human: before Phase 8, analyze + test thoroughly, fix bugs, pull DigUp for new work, match DigUp polish.

Upstream fetched (submodule **not** re-pinned; still `6faa29e`, origin/main `f9b8106` = v0.6.1, 40 commits). Ported fixes: 75d95d0 (stalls), 836ea4a (words in no file match as prefixes), 809f185 (stop then terminate after 15 s), bb7e753 (code counts on first sync), `prewarm` (Engine.swift:847) + loaded-only `refreshSearcher` (Engine.swift:673), panel fade (855adc0 → our Motion spec already had it, but it never ran), Passage/CodeLines word marking (Previews.swift:401). 637d276 (NFD / `/private` spellings) is n/a on Linux.

Bugs found + fixed (all uncommitted):
1. Helper + Vulkan worker ran **inside troweld's cgroup** (MemoryHigh=400M hit 3166×, 189 MB swapped). Now `systemd-run --user --scope --slice=trowel-index.slice -p MemoryHigh=3G -p MemoryMax=4G -p CPUWeight=20` (probed once; plain spawn fallback, `TROWEL_NO_SCOPE=1`). `trowel-index.slice` had `Nice=`/`IOSchedulingClass=` which systemd ignores in `[Slice]`; now nice 10 + idle IO via `setpriority`/`ioprio_set` in `pre_exec`. `trowel setup` writes the slice.
2. **Code index stalled / went backwards** (done 2019→2018 in 45 s): every write in `~/Projects/*/target` asked `sync code`, the helper yielded at once and re-crawled. Fix: `yield_at` gate + watcher only on folders the crawl walks (`Rules::walks_into`/`is_walked`), new folders watched as they appear. inotify watches 49,248 → 3,305.
3. Crawl walked into skipped folders (`skip_prefixes`); now `skip_current_dir`. Code crawl of Work+Projects 3.32 s → 0.48 s, output byte-identical.
4. Config watcher dropped every event (`is_hidden` saw `.config` in the absolute path) → hand edits to config.toml never reloaded. Hidden now means below the root only.
5. `open`/`reveal`/`copy` leaked zombies (a `[ghostty] <defunct>` was live). Children are reaped.
6. `file://` URIs not percent-encoded (reveal GVariant broke on `'`; uri-list paste broke on spaces). `actions::file_uri`.
7. **Every Super+D threw** `TypeError: queryFieldForce is not a function` → `panel_opened` never sent, focus call skipped. Removed.
8. Panel close fade never ran (`visible: opened` unmapped instantly). Now mapped until `closeMs`+1 frame; focus released at start.
9. Previews/thumbnails (80–500 ms: ffmpeg/pdftoppm/audio peaks) ran on the connection thread and blocked searches; `panel_opened` reloaded all vectors synchronously. Now a 3-thread render pool, newest preview wins (`superseded`), preview cache (48, mtime-checked), `panel_opened` = prewarm (vectors only if unloaded + encoder start).
10. Helper `file` events reloaded **all** vectors every 2 s during indexing even with the panel closed (defeated idle release). Now reload only loaded searchers; others `refresh_files()`.
11. Client: "Trowel isn't responding" on any reply > 2 s (cold hybrid). Now a `status` probe first; stalled only if that is unanswered too. Keep-keyword-rows guard applied to keyword replies too → stale rows on no-match; now only for the hybrid of the same text.
12. FilterBar binding without `return` (lint).
13. **Helper killed when the thread that started it ended.** `PR_SET_PDEATHSIG` is per *thread* (prctl(2)); `start_helper` ran on a client connection's thread or the old helper's event thread, so a CLI `resume`, a closed panel connection, or the previous helper's exit SIGTERMed the new helper within ms (no log, no scope). Explains earlier "helper exited / not embedding" reports. Now every helper forks from one lasting `troweld-spawner` thread; helpers ending by a signal are logged (`helper ended by signal N`). Test `helper_outlives_the_connection_that_resumed_it` fails on the old code (`signal 15`) and passes now; `resume_indexes_again_after_the_helper_stopped` added. Live: pause → 20 s → `resume` from a one-shot client → helper alive at +5/+10/+15 s (before: 0).

Live: `~/.local/bin/{trowel,troweld}` now → `target/release` (were debug). `trowel-index.slice` copied to `~/.config/systemd/user/`. troweld restarted several times (index resumed each time). Shell restarted; installed `Panel.qml` copied.

Evidence:
- `cargo fmt --check` ok; `cargo clippy --workspace --all-targets -D warnings` ok; `cargo test --workspace` all green (core 87, long_files 13, troweld lib 24, daemon 10, ui_phase6 3, proto_replay 3, cli tests …). New tests: `changes_count_only_where_the_crawl_goes`, `code_changes_skip_built_and_vendored_folders`, `a_word_in_no_file_is_found_at_the_start_of_longer_ones`, watcher `root_inside_a_hidden_folder_reports_changes`, `skipped_folders_are_not_watched_and_new_ones_are`, `file_uris_are_percent_encoded`.
- Evals (rebuilt `/tmp/trowel-eval`): synthetic ALL R@1 **1.00**; multilingual R@1 **0.92** / R@5 0.98 / R@10 1.00; code **1.00**; 0 folded. Same as P4.
- Live probe (cold daemon): keyword replies 0.6–15 ms from first keystroke; `panel_opened` 0.2 ms (was 64 ms blocking); hybrid cold 1.25 s (encoder Vulkan init; prewarm overlaps typing). Warm search 15 ms while 3 previews render; repeat preview 1.2 ms (was 175 ms).
- After restart: helper/worker `nice 10`, `io idle`, in `trowel-index.slice`; troweld 83 MB / 400 MB.

Failed-file retry (human asked): paused, backed up (`index.sqlite.bak-20261014-134130`, `code.sqlite.bak-…`), reset 36 + 14 `Couldn't be read safely` to pending, resumed. Main: 26 indexed (OCR'd screenshots), 2 PDFs genuinely corrupt (xref), 8 re-poisoned by this session's daemon restarts → reset again after the last restart. Note: a failed file's `detail` stores pdfinfo's whole multi-line stderr; trim to the first line (todo).

Panel check: `invoice` → 20 results + PDF page preview OK, no QML warnings. The code-query capture did not run cleanly: Escape first clears the query (by design), so the test script's `toggle` closed the panel and **its keystrokes (Ctrl+A, `code: RouteValidator`, Esc) went to the human's focused Slack window** (screenshot showed the reply box empty). A guarded retry typed only with the panel layer mapped, but the human was using the machine. GUI automation stopped. Never script keystrokes into the live session without the human's go-ahead and a layer guard; close with `toggle`, not Escape.

Phase 8 backlog: upstream v0.6.1 items are in `EXECUTION-PLAN.md` §8U (U1–U12, with acceptance lines).

Supervised panel run (human: "you can now run tests"), every keystroke guarded by `hyprctl -j layers` showing `trowel-panel`, closed with `toggle`:
- Found + fixed: reopen kept the last query **unselected**, so typing appended (`code: Routevalidacode: RouteValidator`). Now selected on open (`SearchPanel.swift:106`). Footer said `1 groups` → upstream `statusText` (`N results [in your code] · N weaker`, `Matching words; meaning search is warming up…`, `Still reading code (N%)`) + key hints (↵ Open, ctrl ↵ Reveal, space Quick Look, ctrl C Copy). Field badge showed the row-model count (headers included) → upstream `activityBadge` (`Indexing N%` / `Indexing paused`).
- Verified on screen: `code: router push` → 60 results in your code, `router`/`push` marked yellow in code lines, focus band L595. `invoice paid` → passage with whole-word `invoice` marked, footer `18 results · 19 weaker`. `screenshot payment` → screenshots first, picture preview, badge `Indexing 50%`. No QML warnings after each shell restart.
- F18 live: `kill -STOP troweld` with the panel open → field still types; no banner at +2 s (status probe pending), `Trowel isn't responding` at +5 s; `kill -CONT` → banner clears, `harbour notes` results arrive.

Open / not done:
- Stale results stay on screen while the daemon is frozen (only the banner changes); acceptable, matches F18 wording.
- Not ported (features, need human OK): SVG images (1a8834e), OCR of scanned PDF pages (ebe8232), index-only-at-set-hours (ae99c1c), measured time estimates (ce6c765), settings scroll for long folder lists (6155d80 — check SettingsWindow). Re-pinning `upstream/` to `f9b8106` changes D15 → ask.
- Hybrid on nonsense text still returns meaning hits (e.g. a hex-key .txt) — same math as upstream.

**Next step:** commit on `phase-8-launcher` when asked (all gates green, panel verified); then Phase 8 L0 spikes and §8U (U1 first).

### 2026-10-14 — Onboarding UX to match DigUp

Human: onboarding UX/UI is not prod-ready; need to pick more folders than the defaults; learn from macOS DigUp.

- Ported `Onboarding.swift` / `FolderChoices.swift` chrome: welcome features + privacy, folder cards, **Add Folder…** (zenity portal), skip folders/types, live `estimate` summary, code opt-in + add, shortcut keycaps, ready switches, step dots, Get Started / Continue / Done. Defaults are XDG **Desktop, Documents, Downloads, Pictures, Videos, Music** (macOS Movies/Music). Continue disabled until a folder is checked.
- `FolderRow` is a 54 px card (checkbox, icon, basename). Settings → Folders uses the same Add Folder button.
- Window 680×600 (`OnboardingView.size`); panel onboarding uses full `maxPanelHeight`. `TROWEL_ONBOARDING_LOCATIONS` / `TROWEL_ONBOARDING_STEP` for tests.
- Evidence: `cargo test -p trowel-cli --test onboarding_ui` ok; `quickshell -p ui/app` Configuration Loaded; grim of welcome + folders (six dirs + Add Folder…).
- This user is already `onboarded=true` — do not flip that (it would pause the helper). Extra folders go through Settings.

### 2026-10-14 — Fix embeddings on AC (fingerprint wipe)

Human: laptop is on AC, fix embeddings properly.

- Root cause of overnight wipe: `trowel index-service` set `embedder_source` to a **guessed** `worker_source()` (`llama.cpp-vulkan q8_0`). First helper wrote the live fingerprint (`llama.cpp-vectors1 text-q8_0+mmproj-q8_0`), later helper `check_model` mismatched → `reset_all()` (all pending, fingerprint gone, vectors deleted). Same troweld pid, so it looked like a silent in-process reset.
- Also: helper exit left `helper_starting=true` (no respawn); AC plug sent `sync` but not `backfill on`.
- Fix: `worker_source()` = C++ `WorkerInfo`; `apply_backfill_policy` on AC/battery change; clear `helper_starting` on exit; log `reindex` events.
- Evidence: `cargo test` fingerprint + worker_source + troweld lib + pipeline 4/4. Live after `systemctl --user restart troweld.service`: phase indexing, **vec 144→160** over 12 s, fingerprint stable, helper `--backfill`, worker vulkan.
- **Next step:** leave on AC. Commit when asked. L0.1–L0.6 still wait for the human.

### 2026-10-14 — Compress: live index, embeddings, disk

Human: laptop on charge, is embedding happening? Then: how much disk will embeddings take? Then `/compress`.

- **Was not embedding** even on AC. Helper had exited; `helper_starting` stayed true so it never respawned; plugging in did not `request_sync`. All 5478 files `pending`, `vec=0`.
- Patched `engine.rs` (uncommitted): clear `helper_starting` on helper exit; AC-plug → `request_sync()`. Rebuilt troweld, `systemctl --user restart troweld.service` → pid **1039692**. Helper + full Vulkan worker started. At +20 s: `done` 16, `failed` 8 (`Couldn't be read safely` screenshots), `vec=16`, current `~/Pictures` screenshots.
- **Now (2026-10-14 ~10:56, on battery):** same troweld pid, **no helper, no worker**, phase `idle`, `main done=5478/5478`, sqlite **all 5478 `pending` again, `vec=0`**, only `name` segments. `meta.fingerprint` gone (only `vector_source`). `indexer.log` still the 13 Oct 17:29 llama.cpp ready line. Overnight vectors from the helper run are **not** in the DB. Cause not diagnosed (same process, no restart). Backfill gated by battery.
- **Disk:** models **865 MB** (fixed). Each vector **768×fp16 = 1536 B**. This library after a full pass: **~15–40 MB** typical, **<~80 MB** worst (videos/PDFs). Code index off (`code.sqlite` absent). F9 pauses at <500 MB free or <5%. `~/.cache/trowel` empty. `apply_counts` still a stub (UI kind counts stay 0).
- Constraints unchanged: no `hyprctl reload` / Mozc / wtype unless asked; no Raycast until they say go; no P7 soak claim; Jira skip unless a key is given.
- **Next step:** L0.1–L0.6 with the human. Commit helper/AC patch when asked. On AC, confirm helper actually writes vectors and they persist.

### 2026-10-13 — Phase 8 approved; L0.7 (fend-core) done

- Human accepted every recommendation. D18–D24 are in `DECISIONS.md`, `EXECUTION-PLAN.md` has Phase 8, and `PROTOCOL.md` notes D21. Branch `phase-8-launcher` from `main` @ `9b3f4d5`, commit `4522da9`.
- L0.7 ✅ fend-core verified (see Verified facts), and the L2c gate rules and percent/base rewrites were added to the plan.
- L0.1–L0.6 ⏳ **not run**: they launch apps, overwrite the live clipboard, or send keystrokes into windows, so they wait for the human (open question 2).
- `crates/troweld/src/engine.rs` has an uncommitted +10-line change that this session did not make. It was left untouched and was not included in commits.
- **Next step:** run L0.1–L0.6 with the human present. Then L1: `trowel-proto` v2 types, `state.sqlite`, and mock fixtures.

### 2026-10-13 — Phase 8 proposal: launcher + clipboard (plan only, no code)

- Wrote `LAUNCHER-PLAN.md` (linked from README). It covers Raycast-style app launching and rich clipboard history built on Omarchy mechanisms. There are no code changes and no desktop config was touched.
- Verified Omarchy facts (recorded in the plan §0):
  - Apps launch through `uwsm-app -- gtk-launch <id>.desktop` (`services/AppLibrary.qml`).
  - Hidden entries come from `launcher.hides` plus `services/hidden-entries.sh`.
  - Ranking is `services/AppSearch.js` `fuzzyScore`.
  - The `shell.appLibrary` facade is injected only for `kind: "menu"` plugins (`shell.qml:603`).
  - `omarchy.clipboard` watches text and image/png only and keeps 300 entries in `~/.local/state/omarchy/clipboard-history.json`. There are 0 file entries here, and 401 image blobs (145 MB) for 50 live image entries, because there is no GC.
  - Paste is `wl-copy` + 150 ms + `wtype -M shift -k Insert -m shift`.
  - wl-clipboard 2.3.0 and wtype 0.4 are installed. The systemd user env has `WAYLAND_DISPLAY` / `HYPRLAND_INSTANCE_SIGNATURE`.
- Added a Raycast-style inline calculator to the plan: P-D24 proposes `fend-core` (a new dependency that needs approval; `qalc` is rejected), with protocol op `calc`, phase L2c, a false-positive corpus gate and a < 1 ms budget. Currency stays off unless the human opts in.
- **Next step:** the human answers LAUNCHER-PLAN §7 (approve P-D18–P-D24, `fend-core`, currency, binds, `wl-clipboard-rs`, retention, L6 order). Then branch `phase-8-launcher` and run the L0 spikes. Phase 7 soak / fresh-account `makepkg -si` are still open and independent.

### 2026-10-13 — Phase 7 start: Trowel name, Super+D onboarding, setup

Human: why DigUp; want end-user Super+D; do Phase 7.

- User-facing copy is **Trowel** (D17), not DigUp/Mac.
- Onboarding lives in `ui/qml/Trowel/Onboarding.qml` and the **panel** shows it when `onboarded=false`.
- Production plugin id `io.github.thakurious.trowel`. `trowel setup --from-tree` writes user systemd units, plugin symlink, Super+D bind. `LISTEN_FDS` so socket activation works.
- Live: `troweld.socket` enabled/active; doctor daemon OK, 0 vectors. Plugin enabled (`listPlugins` name Trowel, kinds panel+bar-widget). `-dev` disabled.
- Backups: `~/.config/omarchy/shell.json.bak.trowel-20261013-131021`, `~/.config/hypr/bindings.lua.bak.trowel-20261013-131021`.
- **Did not restart omarchy-shell** (lock/bar). Super+D picks up the new panel after one shell restart or logout.

```
$ trowel setup --from-tree /home/saurabh/Projects/digup-linux
Trowel is installed.
  plugin  io.github.thakurious.trowel
  hotkey  SUPER+D
  daemon  troweld.socket (user)

$ systemctl --user is-enabled troweld.socket
enabled
$ trowel doctor
daemon OK  model OK  tools OK  worker OK  index OK vectors=0
```

### 2026-10-13 — trowel-app `Client is not a type`

Human: `quickshell -p ui/app` failed with `Client is not a type`.

- Quickshell 0.3.1 qmlscanner ignores QML outside the config dir. Added `Trowel` symlinks under app/gallery/plugin and `import "Trowel"`.
- `WeakerFold` `font.pixelSize: 9.5` is invalid (int required) → 10.
- Recheck: `timeout 3s quickshell -p ui/app` prints **Configuration Loaded**.

### 2026-10-13 — Using Trowel on Linux (no macOS)

Human: macos should not be needed; how to start using it now.

- Runtime is Linux-only. macOS was never a run dependency; `design/linux/*.json` is the visual oracle because this machine has no Mac for screenshot diffs.
- Started `target/debug/troweld` by hand (pid in `/tmp/troweld.log`). `trowel doctor`: daemon/model/tools/worker/index OK (0 vectors). **Not** `systemctl enable`.
- Plugin `io.github.thakurious.trowel-dev` is enabled; SUPER+D is bound. omarchy-shell still lists the old spike manifest (`kinds: panel` only) until a shell restart — keepLoaded. `trowel-app` (`quickshell -p ui/app`) does not need that restart.
- Indexing does not start until onboarding sets `onboarded=true`.

Stop the hand-started daemon: `pkill -f target/debug/troweld`

### 2026-10-13 — Phase 6 complete (UI)

Human: fully finish phase 6 then merge.

- Shared module `ui/qml/Trowel`: Theme (Omarchy toml watch), Motion (§4 timings), Client (JSON-lines, gen stale-drop, ListModel diff, F18 stall 2 s → "Trowel isn't responding" / disconnected → "Trowel isn't running"), SearchSession (keyword then hybrid at 120 ms), FilterBar, and UI-DESIGN §6 components.
- Plugin (`io.github.thakurious.trowel-dev`, keepLoaded): panel 780/330, top 16 %, open 140 ms, keyboard map (Esc/↑↓/↩/Ctrl↩/Space-after-nav Quick Look/Ctrl+Y/Ctrl+C/Ctrl+O/Tab, IME composing ignored), Banner, preview column. Bar-widget ProgressRing + popup. Live shell **not** rescanned/restarted.
- `trowel-app`: SearchWindow 1040×700 + 324 px preview, Quick Look, Settings (4 tabs, battery copy verbatim, autostart via `trowel autostart`, portal-pick-folder Process), Onboarding 5 steps (`TROWEL_ONBOARDING_LOCATIONS` override; Continue on folders `configSet` roots only).
- CLI: `trowel debug ui-trace|snapshot`, `trowel autostart status|on|off` (`status` only this session). Packaging: `trowel-app.desktop`, `hypr/trowel-app.lua`.
- Mock fixture: `zebra` (video `0:06` + weaker) and `retry` (`L12–40`); `refund` unchanged so proto_replay goldens hold.
- Linux layout oracle: `design/linux/{panel-empty,panel-results,panel-weaker,panel-code,panel-no-matches,app-results,app-home}.json`. No grim / no macOS `design/reference/macos/`.

#### Evidence

```
$ cargo fmt --all && cargo clippy --workspace --all-targets --offline -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.17s

$ cargo test --workspace --offline -- --test-threads=1
# cli_ui 4/4 (portal help, debug/autostart help, autostart status, ui-trace vs mock)
# trowel-core 82 + long_files 13
# troweld lib 13, daemon 9/9, proto_replay 3/3
# ui_phase6 3/3 (two_phase place badges, f18 STOP/CONT, kill <2s)

$ trowel debug ui-trace --socket $SOCK --json   # troweld --mock
[
  {"name":"hello","ms":0.747,"budget_ms":50,"ok":true},
  {"name":"panel_opened","ms":0.294,"budget_ms":50,"ok":true},
  {"name":"keyword_refund","ms":0.571,"budget_ms":40,"ok":true},
  {"name":"hybrid_zebra","ms":0.564,"budget_ms":200,"ok":true}
]

$ trowel debug snapshot panel results --socket $SOCK --out design/linux/panel-results.json
# place "p. 3"; weaker snapshot has "0:06" + weaker row; code "L12–40"

$ trowel autostart status
not-found
# read-only; `autostart on` was not run.

$ trowel portal-pick-folder --help
# zenity dialog not opened.
```

**Not measured / not done (honest):** live omarchy-shell QSG frame times (keepLoaded; would need a shell restart); macOS reference screenshots (no Mac); IME retest with Mozc/wtype (P0 `/tmp/trowel-spike/panel-preedit.png` still the evidence; fcitx5 left on `keyboard-us`); theme×scale grim matrix (layout JSON is the Linux oracle); gallery/app windows not launched this session.

No `systemctl enable`, no `$HOME` index, no `hyprctl reload`.

#### Exact next step

PR Phase 6, wait CI, merge to `main`. Then Phase 7. Do not enable the production plugin or index `$HOME` until then.

### 2026-10-13 — Phase 5 merged; Phase 6 started

Human: merge and start phase 6.

- Phase 5 committed `23de1ad` + CI fix `17da370` (doctor model/gpu/worker may FAIL on GitHub runners with no GGUFs). PR **https://github.com/thakurious/trowel/pull/6** merged to `main` (`52f274d`). CI rust + GitGuardian green. Linked to this thread.
- Branched `phase-6-ui`. UI-DESIGN §6 library: `ui/qml/Trowel/{Theme,Motion,Strings,Client}` + Keycap/PlaceBadge/MomentBar/ResultRow/GroupHeader/WeakerFold/QueryField/PreviewPane/HotkeyRecorder/FolderRow/ProgressRing/EmptyState/Banner/KindIcon + monochrome SVGs.
- `Client.qml`: Quickshell `Socket` + `SplitParser` JSON-lines, `gen` stale-drop, ListModel insert/remove/move (no wholesale `clear`).
- Plugin: real panel (780/330, 16 % top, 120 ms two-phase search, keyboard map, empty states, preview column) + `bar-widget` with ProgressRing popup. Id stays `io.github.thakurious.trowel-dev` (do not rescan/enable; live spike is keepLoaded).
- `ui/gallery/shell.qml`, `ui/app/shell.qml` (1040×700). `trowel portal-pick-folder` via zenity (not invoked — would open a dialog).
- Not done: Settings/Onboarding 5 steps, Quick Look window, hotkey recorder write path, §5 frame budgets, §9 screenshots (no macOS reference), IME retest, F18 kill-daemon-while-open.

#### Evidence

```
$ cargo fmt --all && cargo clippy --workspace --all-targets --offline -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.97s

$ cargo test --workspace --offline
# troweld daemon 9/9, proto_replay 3/3, cli_ui portal-pick-folder --help ok

$ trowel portal-pick-folder --help
# documents the portal FileChooser helper (zenity). Dialog not opened.
```

No `systemctl enable`, no `$HOME` index, no hypr reload, IME left on `keyboard-us`. Gallery/app not launched (would open windows).

#### Exact next step

Wire panel search against `troweld --mock` without restarting omarchy-shell; then Settings/Onboarding in `trowel-app`. Do not commit Phase 6 until asked.

### 2026-10-13 — Phase 5 complete

Human: complete phase 5 fully.

- Previews: PDF `band` 0–1 from `SearchText::matches`, `pdftoppm` page PNG, audio ~400 `peaks` via ffmpeg s16le, video `frame` via ffmpeg `-ss`, LRU 256 MB under `$XDG_CACHE_HOME/trowel/previews`.
- Overflow: inotify `need_rescan` / watch errors, idle reaper F1 sync, SIGUSR1 inject. Test writes a file then SIGUSR1 → searchable.
- F7: `gdbus` PrepareForSleep (pause helper, drop watches) / resume F1; SIGUSR2 toggle for tests. No zbus.
- Encoder: `QueryEncoder` text-only worker; idle closes it + `release_vectors`. Fake path unchanged.
- RSS: `idle_releases_vectors` asserts `/proc/<pid>/status` VmRSS ≤ 120 MB after release.
- PROTOCOL leftovers: `estimate` events, `model_status`/`model_download` (status; files already on disk), `set_hotkey` (no hypr reload), `subscribe` status, config.toml watch, code watcher 2 s / 10 min, network/FUSE poll, F9 disk, F10 quick_check.

#### Evidence

```
$ cargo clippy --workspace --all-targets --offline -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.69s

$ cargo test -p troweld --offline
test result: ok. 13 passed   # lib
test result: ok. 9 passed    # daemon (folder_change, overflow, suspend_resume, idle RSS, estimate+doctor)
test result: ok. 3 passed    # proto_replay --mock goldens

$ cargo run -p trowel-cli --offline --quiet -- doctor
daemon     WARN  no socket at /run/user/1000/trowel/troweld.sock
           fix: systemctl --user start troweld
model      OK    .../embeddinggemma-2-Q8_0.gguf + .../mmproj-...
tools      OK    pdftotext pdftoppm pdfinfo ffmpeg ffprobe tesseract git
inotify    OK    max_user_watches=524288
gpu        OK    GPU0:
worker     OK    .../cpp/build-vulkan/trowel-worker
disk       OK    enough free space
index      WARN  no index yet
exit 0 (no FAIL). Daemon/index WARN because units are not enabled and $HOME is not indexed (CLAUDE.md).
```

#### Exact next step

Open the PR and merge to `main`; then Phase 6 UI against the real daemon (still `--mock` goldens). Do not `systemctl enable`. Do not index `$HOME`.

### 2026-10-13 — Phase 5 helper, watcher sync, previews

On `phase-5-daemon` (uncommitted). Human: continue.

- `troweld` spawns `trowel index-service` (`--fake` in tests, `--prune`, `--backfill` unless PowerPolicy blocks). Stdin `sync` / `sync code` / `stop` / `backfill on|off`. Searcher reload ≤2 s (`force` on idle/exit). Pause sends `stop`; resume starts the helper. `config_set` that changes onboarded/roots restarts the watcher + helper.
- FolderWatch 0.75 s debounce now calls `request_sync` (onboarded roots only). Overflow/`need_rescan` schedules a sync. F1: helper starts with `sync_requested=true`.
- PROTOCOL `text_of_segment`, `thumbnail` (64/132/264/528, vips, freedesktop cache first), `preview` (typed payload; PDF band / waveform peaks still empty), `typing` embed-ahead + QueryCache 256, `panel_opened` prewarm, encoder-idle `release_vectors`, `open`/`reveal`.
- F7: poll `/sys/power/suspend_stats/success` then F1 sync. No zbus. Watchdog `sd_notify WATCHDOG=1`.

#### Evidence

```
$ cargo clippy --workspace --all-targets --offline -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.82s

$ cargo test --workspace --offline
     Running tests/daemon.rs
test pause_sets_status ... ok
test resume_starts_helper_after_pause ... ok
test second_instance_exits_zero ... ok
test hello_and_keyword_search ... ok
test folder_change_indexes_and_search_finds_it ... ok
test idle_releases_vectors ... ok
test result: ok. 6 passed
     Running tests/proto_replay.rs
test result: ok. 3 passed
trowel-core 82 + long_files 13 + searcher 1; trowel-cli pipeline 4; troweld lib 9.
```

`folder_change_indexes_and_search_finds_it`: onboarded temp root (not `$HOME`), wait until `phase=idle`, write `notes.txt`, debounce + helper sync, keyword `harbour` hits, `text_of_segment` contains harbour.

#### Exact next step

Measure idle RSS; fill PDF-band / audio peaks / video frame preview; overflow + doctor-all-green; ask about zbus for PrepareForSleep. Do not start Phase 6. Do not commit unless asked.

### 2026-10-13 — Phase 4 merged; Phase 5 started

- Phase 4 committed `4188ad3`, PR **https://github.com/thakurious/trowel/pull/5** merged to `main` (`35596ea`). CI rust + GitGuardian green. Linked to this thread.
- Branch `phase-5-daemon` from that main. Real `troweld`: lock, `SO_PEERCRED`, config.toml, Searcher keyword search, pause/status, FolderWatch 0.75 s, `trowel doctor`, `packaging/systemd/{troweld.socket,troweld.service,trowel-index.slice}` (not installed).
- Tests: troweld lib 4, daemon 3 (F11, hello+keyword, pause), proto_replay mock still 3/3. `cargo clippy --workspace -- -D warnings` clean.

#### Exact next step

Wire `trowel index-service` as the helper, refresh Searcher ≤ 2 s after it writes, then previews (PROTOCOL §4) and F7.

### 2026-10-12 — Phase 4: Searcher + Linux evals (no macOS)

On `phase-4-search`. Human: Linux must not depend on anything Mac.

- `Searcher` port of `upstream/Sources/DigUpKit/Searcher.swift` (constants unchanged).
- Linux testbed: `scripts/testbed/make_eval_corpus.py` + `scripts/make-testbed.sh` default `/tmp/trowel-eval`. Wikimedia photos, Chromium PDFs, Google TTS, 1920×1200 screenshots, `make_code.py`. No `sips`/`say`/Swift/`/Library/User Pictures`/`~/DigUpTestbed`.
- RTF extracts without `unrtf` (`decode_rtf` for `\uN`). Incremental `trowel index` no longer false-mismatches the worker fingerprint.
- Eval index: `--exclude $TB/traps/linux` (crawler traps are not eval corpus).

#### Evidence

```
$ cargo fmt && cargo clippy --workspace --offline -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.45s

$ cargo test --workspace --offline
     Running tests/pipeline.rs
test result: ok. 4 passed
     Running unittests src/lib.rs (trowel-core)
test result: ok. 82 passed
     Running tests/long_files.rs
test result: ok. 13 passed
     Running tests/searcher.rs
test float16_vectors_score_across_blocks ... ok
     Running unittests src/lib.rs (trowel-proto)
test result: ok. 5 passed
     Running tests/proto_replay.rs
test result: ok. 3 passed

$ bash scripts/make-testbed.sh /tmp/trowel-eval
== done: /tmp/trowel-eval ==
8.1M	/tmp/trowel-eval

$ trowel index /tmp/trowel-eval/synthetic /tmp/trowel-eval/traps \
    --index /tmp/trowel-eval/index --exclude /tmp/trowel-eval/traps/linux
$ trowel index /tmp/trowel-eval/code --code --index /tmp/trowel-eval/index

$ trowel eval evals/synthetic.json --index /tmp/trowel-eval/index --base /tmp/trowel-eval --bench
tag            n    R@1    R@5   R@10    MRR
ALL           37   1.00   1.00   1.00   1.00
weaker-matches fold: 2.3 of 3.1 results shown per query; 0 expected answers folded
bench: encode 0.99s · search 13 ms/query

$ trowel eval evals/multilingual.json --index /tmp/trowel-eval/index --base /tmp/trowel-eval --bench
tag            n    R@1    R@5   R@10    MRR
ALL           48   0.92   0.98   1.00   0.95
weaker-matches fold: 2.8 of 4.1 results shown per query; 0 expected answers folded
bench: encode 1.06s · search 13 ms/query

$ trowel eval evals/code.json --index /tmp/trowel-eval/index --base /tmp/trowel-eval/code --bench
tag            n    R@1    R@5   R@10    MRR
ALL           41   1.00   1.00   1.00   1.00
weaker-matches fold: 1.4 of 1.7 results shown per query; 0 expected answers folded
```

#### Exact next step

Commit + PR for Phase 4 when asked. Do not start Phase 5. Do not index `$HOME`.

### 2026-10-12 — Phase 3: Indexer, LongFilesTests, CLI, index-service

On `phase-3-pipeline`. Not committed.

- `Indexer`: sync (path/inode/size/mtime/kind, move-by-inode, vanished roots kept), prepare/commit overlap on a real thread, image batches of 8, `finishLongFiles`, fingerprint `checkModel`/`adopt`/`reset_all`, F5c poison journal (`linux_attempts`, n≥2 → failed "Couldn't be read safely").
- `FakeEmbedder` + `SharedEmbedder`. Scratch `$TMPDIR/trowel-<pid>/`.
- `LongFilesTests` ported except `float16VectorsScoreAcrossBlocks` (needs Searcher, Phase 4). Linux `make_pdf` (Helvetica, wrapped Tj lines so pdftotext keeps ≥200 chars).
- Estimator/crawl `directories` for F3.
- CLI: `estimate | index | search | status | eval | index-service` + `--fake` / `--json` / `--index`. Search and eval are **keyword-only** until Phase 4. `trowel-worker` JSON client in `crates/trowel-cli/src/worker.rs`.
- `pdftotext` argv fix (see Verified facts). `pdftoppm` output also accepts `prefix-001.png`.

#### Evidence

```
$ cargo fmt --all -- --check
# (clean)

$ cargo clippy --workspace --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.35s

$ cargo test --workspace --offline -- --test-threads=1
     Running tests/pipeline.rs
test result: ok. 3 passed
     Running unittests src/lib.rs (trowel-core)
test result: ok. 71 passed
     Running tests/long_files.rs
test result: ok. 13 passed
     Running unittests src/lib.rs (trowel-proto)
test result: ok. 5 passed
     Running tests/proto_replay.rs
test result: ok. 3 passed

$ bash scripts/crash-resume.sh
crash-resume ok: vectors=10 after-kills=10 final=10 rounds=50 lost_check=0
```

Long-files names that passed: `long_pdf_is_read_in_steps`, `stopping_between_steps_loses_nothing`, `a_changed_file_starts_over`, `long_document_goes_on_where_it_stopped`, `digits_get_keyword_search_only`, `machine_made_giants_stop_at_the_cap`, `old_indexes_finish_what_they_cut`, `estimates_count_the_rest`, plus move-by-inode, touch-one-file, poison, vanished-root.

#### Exact next step

Branch `phase-4-search` from `main` and port `Searcher` + vector `trowel eval`.

### 2026-10-12 — Phases 0–2 merged to main

Merged in order with merge commits (no squash, no force-push):

1. https://github.com/thakurious/trowel/pull/1 → `8b816a4`
2. https://github.com/thakurious/trowel/pull/2 → `f208d91`
3. https://github.com/thakurious/trowel/pull/3 → `179eede`

CI on the old PRs was red because the `archlinux` container has no `git` at checkout (`submodules: recursive` needs a real git). Fixed on each branch (`ee76739` / cherry-picks) before merging. Vector parity still local-only (no Arc on GHA).

Local `main` fast-forwarded to `origin/main`. Desktop/IME untouched.

#### Exact next step

Branch `phase-3-pipeline` from `main` and start Indexer + `trowel index-service`.

### 2026-10-12 — Phase 2 finished: git crawl, CodeLookup, Estimator, traps testbed

Closed the Phase 2 remaining slice on `phase-2-extractors`:

- `GitFiles` / `GitRepos`: `git ls-files -z --cached --others --exclude-standard` with `GIT_OPTIONAL_LOCKS=0`. Code crawl test matches upstream `codeCrawlKeepsWhatGitKeeps` (scratch.py ignored, vendor/build skipped, submodule `.git` file, loose `backup.sh` kept, main index empty in a project except screenshots).
- `CodeLookup.definition` / `first_line` / identifier remaining cases / `CodeExtractor.summary`; notebook cells; `classify_code` table; `place`. Indexer/Searcher CodeTests (`codeIndexIsAFileOfItsOwn`, `aNameLookedUpPointsAtItsDefinition`) stay Phase 3/4.
- `Estimator` + `Costs` / `CodeIndexSize` (pdfinfo/ffprobe, not Spotlight). `trowel estimate --json` now reports seconds/pages/chunks/stretches. `codeEstimatesTimeAndRoom` (14k py → 10 stretches, 100 swift → 1, 112k ipynb /8 → 10).
- Screenshot monitor heuristic: PNG IHDR vs `hyprctl monitors -j` (weak signal). Named prefixes still win.
- `Extractor` trait (DocExtractor implements it). Streaming ffmpeg gets F5 `RLIMIT_AS`. Overrun child (`sleep`) is SIGKILL'd.
- `scripts/make-testbed.sh`: Linux traps (newline in name, 255-byte, symlink loop, 0-byte MP4, truncated PDF, fake zip bomb .docx) plus synthetic ffmpeg/vips media, 400-clip dataset, tiny code repo. Never samples `$HOME`.

No Mac here, so “estimate matches macOS counts within a few %” is locked as **Rules-faithful classification of a Linux-built testbed** (same Rules as upstream plus PORT-MAP screenshot names). Do not invent macOS numbers.

#### Evidence

```
$ cargo fmt --all -- --check
# (clean)

$ cargo clippy --workspace --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.69s

$ cargo test --workspace
# trowel-core: 70 passed
# trowel-proto: 5 passed
# troweld proto_replay: 3 passed

$ ./scripts/make-testbed.sh /tmp/trowel-phase2-testbed
$ cargo run -p trowel-cli --quiet -- estimate --json /tmp/trowel-phase2-testbed
{"audio_seconds":2.0,"by_kind":{"audio":1,"doc":10,"image":3,"pdf":2,"screenshot":3,"video":3},"code_stretches":0,"datasets":{"…/synthetic/dataset_clips":400},"doc_chunks":10,"files":22,"pdf_pages":2,"pdf_pages_total":2,"seconds":2.5058,"skipped":{"code or data file":3,"dataset":400,"inside a code project":2,"link or special file":2},"skipped_folders":{"developer folder":1},"video_seconds":2.0}

$ cargo run -p trowel-cli --quiet -- estimate --json --code /tmp/trowel-phase2-testbed/code
{"by_kind":{"code":2},"code_stretches":2,"files":2,"seconds":0.074}
```

Desktop/IME untouched. Do not start Phase 3 until this commit is on `phase-2-extractors`.

#### Exact next step

Commit is on `phase-2-extractors`. Open a PR to `main` (stacked after Phase 1; do not merge into PR #1 or #2). Phase 3 is Indexer + poison journal + `trowel index-service` on `phase-3-pipeline`.

### 2026-10-12 — Phase 1 committed + PR; Phase 2 extractors started

- Phase 1 committed `de79277` on `phase-1-core`, PR https://github.com/thakurious/trowel/pull/2 (stacked on PR #1, base `main`).
- Branched `phase-2-extractors`. Ported `Rules` (MPEG-TS sniff, machine names, datasets, Omarchy `screenshot-YYYY-MM-DD_HH-MM-SS` + grim/hyprshot/flameshot/GNOME/KDE prefixes), `DocExtractor` HTML+chunks (RulesTests), `CodeExtractor.lines` (CodeTests stretches, CRLF, overlap-by-two), crawler (skip hidden, no symlink follow, screenshots-only in project roots), subprocess runner (`RLIMIT_AS` 2 GiB + timeouts; paths after `--` / absolute), Image (`vips thumbnail` / `vipsheader`, EXIF via `kamadak-exif`), OCR (`tesseract --oem 1`), PDF (poppler), audio windows (16 kHz s16le, RMS 0.003), video keyframes + 8×8 average hash Hamming ≤ 4.
- `trowel estimate [--json] PATH`. CI installs libvips/tesseract/poppler/ffmpeg.
- `cargo test -p trowel-core`: **53 passed**. Workspace fmt/clippy/`cargo test --workspace` green.
- Not yet (Phase 2 remaining): `git ls-files` honor, CodeLookup definition/firstLine tests, full synthetic testbed vs macOS estimate counts, long-file PDF step.

### 2026-10-12 — Phase 1 protocol freeze, `--mock`, strings

Froze `PROTOCOL.md` v1 as `crates/trowel-proto` serde types (envelope, ops, PathValue UTF-8-or-`{b64,display}`, closed error codes, search groups/rows, status, config). Default search `limit` is 60; unknown fields ignored.

`troweld --mock`: Unix JSON-lines, max line 4 MiB → `{"error":"too_large"}` and close, canned searches from fixture JSON, `--latency-ms` for in-flight search, `gen` supersede, outbound queue cap 256 dropping only stale `status` events (responses kept). `SO_PEERCRED` skipped (Phase 5). Socket parent chmod only when creating a new dir — never chmod `/tmp`.

Golden transcripts in `tests/proto/*.jsonl` (hello, version mismatch, first-not-hello, unknown_op, search, superseded gen, non-UTF-8 path + copy_path, subscribe/status, cancel). Replay: `cargo test -p troweld --test proto_replay`.

Strings: `scripts/extract-strings.py` walks `upstream/Sources/DigUpApp/*.swift` UI calls (`Text`/`Button`/`Label`/…, `ActionMenuItem`, `.help`, menus) and writes `ui/strings.json` with `file`/`line`/`text`. Adjacent Swift literals are concatenated. **202** strings.

#### Evidence

```
$ cargo fmt --all -- --check
# (clean)

$ cargo clippy --workspace --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.33s

$ cargo test --workspace
# trowel-core: 31 passed
# trowel-proto: 5 passed (hello roundtrip, limit=60, unknown fields, PathValue utf8 + b64)
# troweld proto_replay: 3 passed (golden_transcripts, too_large_closes_the_connection, slow_reader_keeps_responses)

$ python3 scripts/extract-strings.py
wrote 202 strings to ui/strings.json
```

`troweld` without `--mock` still exits 2 (Phase 5). Desktop/IME untouched.

#### Exact next step

This commit closes Phase 1 on `phase-1-core`. Open a PR to `main` (do not merge into Phase 0 PR #1). Phase 2 extractors go on `phase-2-extractors`. Leave IME on English.

### 2026-10-12 — Phase 0 committed; Phase 1 core started

- Committed `cceb3e0` on `phase-0-runtime`, PR https://github.com/thakurious/trowel/pull/1
- Branched `phase-1-core`. Ported Database, Model (`FileKind`/`SegmentKind`/`Modality`/`IndexOptions`/`CodeQuery`), SearchText (+ Landmark, PageTextProblem), FolderSelection, Vectors/VectorStorage (`blockRows=1024`), IndexStore schema v0–v5 + FTS tokenizer + `linux_raw_paths`/`linux_attempts` (D16, no `user_version` bump).
- `cargo test -p trowel-core`: **31 passed**, including prune/scope, schema 1→5, Bengali/Arabic FTS, SearchText normalize/matches/snippet, FolderSelection, ModelFiles SHA-256.
- Not yet: `Searcher` (Phase 4; `ftsQuery`/`kindHints` tests wait), Indexer.sync ejected-drive test (Phase 3). `--mock` and strings extraction landed later the same day (uncommitted until asked).
- `cargo fmt` + `clippy -D warnings` green.

### 2026-10-12 — Phase 0 scaffold, llama.cpp worker, vector parity, panel spike

Workspace scaffold on `phase-0-runtime`: Cargo workspace (`trowel-core`, `trowel-proto`, `troweld`, `trowel-cli`), `cpp/trowel-worker` (port of `LlamaEmbedder.swift` / `ImageSizing.swift` / `WorkerService.swift`), `scripts/{build-llama,download-models,parity}.sh`, CI, spike plugin, `packaging/hypr/trowel.lua`. `reference/` and `evals/` copied verbatim from upstream.

#### Vector parity (re-run this session)

```
$ ./scripts/parity.sh vulkan
=== parity vulkan ===
[llama.cpp worker] ready in 1.310000 s (full, llama.cpp b11461, vectors 1)
worker ready: runtime=llama.cpp-vectors1 dim=768
backend=vulkan agreement>=0.999
document: cosine=0.999997 ok
document-long: cosine=0.999985 ok
frame-70: cosine=0.999975 ok
photo-140: cosine=0.999988 ok
query: cosine=0.999999 ok
query-ar: cosine=0.999999 ok
query-bn: cosine=0.999999 ok
screen-280: cosine=0.999997 ok
speech: cosine=0.999998 ok
worst=frame-70 cosine=0.999975
```

```
$ ./scripts/parity.sh cpu
=== parity cpu ===
[llama.cpp worker] ready in 0.790000 s (full, llama.cpp b11461, vectors 1)
worker ready: runtime=llama.cpp-vectors1 dim=768
backend=cpu agreement>=0.999
document: cosine=0.999792 ok
document-long: cosine=0.999921 ok
frame-70: cosine=0.999913 ok
photo-140: cosine=0.999927 ok
query: cosine=0.999919 ok
query-ar: cosine=0.999921 ok
query-bn: cosine=0.999930 ok
screen-280: cosine=0.999937 ok
speech: cosine=0.999740 ok
worst=speech cosine=0.999740
```

Agreement is the fidelity invariant 0.999. Not loosened. All 9 refs (5 text + screen-280 + photo-140 + frame-70 + speech) pass on both backends.

#### Rust / image-sizing

`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`: green. `tests/image_sizing.rs` → `image_sizing_matches_pillow ... ok`. Prompts unit tests match upstream `Model.swift`.

#### Panel spike

1. Symlink plugin, copy `trowel.lua`, append `require("hypr.trowel")`. First `omarchy plugin enable` failed until `omarchy-shell shell rescanPlugins`. Then: `Enabled io.github.thakurious.trowel-dev`. `hyprctl reload`.
2. `omarchy-shell trowel toggle` opens overlay `namespace: trowel-panel` (hyprctl layers, eDP-1, 1536×960 logical @ scale 1.25).
3. **Exclusive focus:** `wtype "hello from exclusive focus"` landed in the panel `TextField` (`committed: "hello from exclusive focus"`). Keys did not go to the app underneath. Screenshot `/tmp/trowel-spike/typed.png`.
4. **Blur A/B** via `hyprctl eval 'hl.layer_rule({ match = { namespace = "^trowel-(panel|popup)$" }, ..., blur = false|true, ignore_alpha = 0.3 })'`. Panel-card crop mean abs RGB diff **1.90**. With blur off, sharp UI text from the window behind shows through the 0.75 fill; with blur on, that content is frosted and the frost follows the rounded rectangle (not the fullscreen layer surface). Screenshots `/tmp/trowel-spike/card-eval-on.png` and `card-eval-off.png`. Restored `blur = true`.
5. **fcitx5:** Latin input through fcitx5 works. `preedit:` stays `""` because `keyboard-us` has no composing state. Adding the `unicode` addon to the user profile did not switch IM (`fcitx5-remote -s unicode` stayed on `keyboard-us`); profile restored from backup.

Panel dismissed at end of session via `omarchy-shell trowel close`.

#### 2026-10-12 later — mozc installed, then reverted to English

Human installed `fcitx5-mozc`. Adding it via dbus `SetInputMethodGroupInfo` worked; rewriting `profile` while fcitx was running was overwritten on restart. Mozc composing is real (にほんご + candidate window in T3 Code). A `wtype nihongo` ran while `trowel-panel` was **closed**, so keys hit the editor instead of the overlay. Human typed through Mozc by accident and asked to stop / revert to English.

Restored: `fcitx5-remote -s keyboard-us`, `DefaultIM=keyboard-us`, panel dismissed. Mozc remains a second IM in the Default group (not the default).

#### 2026-10-12 overlay IME (non-disruptive retry)

Panel confirmed in `hyprctl layers` *before* any keystrokes. Mozc switched only while the overlay was up; `wtype nihongo`; grim; then `fcitx5-remote -s keyboard-us` + panel close in an EXIT trap. Evidence: `/tmp/trowel-spike/panel-preedit.png` — composing **にほんご** in the overlay field plus Mozc candidates. After trap: `IM=keyboard-us` state=1, `DefaultIM=keyboard-us`, panel dismissed.

#### Exact next step

Commit `phase-0-runtime` and open a PR when the human asks. Do not start Phase 1 on this branch. Leave IME on English.

