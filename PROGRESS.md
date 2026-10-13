# Progress

Agents: update this at the end of every session (see `CLAUDE.md`). Newest session first.

## Current state

- **Phase:** 5 on `phase-5-daemon`. Acceptance demonstrated; this commit lands it. Phase 4 is on `main` (PR **#5** merged).
- **Branch:** `phase-5-daemon` (from `main` @ `35596ea`)
- **Next step:** open the PR, merge to `main`, then Phase 6 UI. Do not index `$HOME`. Do not `systemctl enable`. fcitx5 stays on `keyboard-us`.

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
| P6 UI checklist + budgets + visual QA | ⬜ | |
| P7 fault injection + packaging + soak | ⬜ | |

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

## Open questions for the human

1. **Jira ticket?** Branch names have no key. Skip logging unless you give one.
2. **qmllint import path.** For CI, should we add `-I /usr/share/omarchy/shell` (needs that tree in the arch container) or keep plugin QML lint as a local-only check?
3. **zbus for F7?** Not added. F7 uses `gdbus monitor` + sysfs + SIGUSR2 for tests. Say if you still want zbus.

## Desktop changes to revert

Timestamp `20261012-133406`. Spike plugin id `io.github.thakurious.trowel-dev` (not the lock-screen, not a first-party plugin).

Backups:

- `~/.config/hypr/bindings.lua.bak.trowel-20261012-133406`
- `~/.config/omarchy/shell.json.bak.trowel-20261012-133406`
- `~/.config/fcitx5/profile.bak.trowel-20261012-133406`
- `~/.config/fcitx5/profile.bak.trowel-20261012-134601`

Live changes:

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

