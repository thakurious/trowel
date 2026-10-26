# Robustness

The macOS app leans on things Linux doesn't give you: FSEvents keeps a persistent event history,
ImageIO/PDFKit/AVFoundation are hardened system frameworks, launchd/SMAppService supervise the app,
and there is one GPU driver. On Linux, every one of these becomes **our** problem. This doc is the
failure-mode inventory and the mechanism that handles each one. Every row is a test in
`TESTING.md` §7.

Principle: **the index is always resumable, the daemon never crashes because of a file, and the UI
always tells the truth about state.**

---

## 1. Failure-mode matrix

| # | Failure | Linux-specific cause | Required handling |
|---|---|---|---|
| F1 | Missed file changes while the daemon was down | inotify has **no history** (FSEvents does) | On every daemon start and every resume from suspend, run a full `sync` reconcile (the stat walk, which is cheap: about 1 s per 100k files when warm). Never assume the watcher saw everything. |
| F2 | inotify queue overflow | `IN_Q_OVERFLOW` during bulk copies or `git checkout` | Treat it as "rescan the affected root": schedule a `sync` for that root and log it. |
| F3 | Watch limit or watch-setup cost | Recursive watching = one watch per directory (about 1 KB of kernel memory each). The limit is 524288 here, but other machines default to 8192. | Count directories during `estimate`. If `dirs > 0.8 × max_user_watches`, fall back to **polling sync** for that root every 10 min and show it in `doctor`. Never fail silently. Add watches lazily on `IN_CREATE` of a directory. |
| F4 | Unwatchable filesystems | NFS/SMB/sshfs/FUSE (rclone), and removable drives | Detect them with `statfs` magic. Network and FUSE roots get polling sync (30 min) plus "Network folder: checked every 30 min" in the folder row. On unmount, the root becomes `missing` (as upstream does for ejected drives), its rows are kept, and results from it are hidden until it is back. |
| F5 | A malformed file crashes or hangs an extractor | poppler/ffmpeg/libvips/tesseract on untrusted input | (a) Run **every external tool as a subprocess** with a wall-clock timeout (PDF page 20 s, OCR 30 s, ffmpeg 5 min per hour of media, docx 10 s) and `RLIMIT_AS` 2 GiB. (b) Run in-process libvips decoding inside the **index-service** process, never in `troweld`. (c) **Poison-file journal**: before preparing a file, write `files.state='pending', detail='attempt:<n>'`, committed. If the helper dies mid-file, the next start increments `n`; at `n ≥ 2` the file becomes `failed` with "Couldn't be read safely" and is skipped. Upstream has no equivalent; without it, one bad PDF crash-loops indexing forever. |
| F6 | Worker crash / GPU hang / `VK_ERROR_DEVICE_LOST` | Mesa/xe driver resets, suspend/resume | The worker is supervised by `troweld`. On exit or a timeout (request > 30 s), kill it, restart with exponential backoff (1 s, 2 s, 4 s … 60 s), and re-queue the batch **item by item** so a single bad input is isolated (F5 logic applies to embedding too). After 3 consecutive device-lost errors, switch the worker to the **CPU backend** and set `status.worker_backend = "cpu"`. That is only allowed if CPU vectors pass the parity gate (TESTING §1), which is why the CPU build is part of the gate. |
| F7 | Suspend/resume | logind `PrepareForSleep` | Before sleep: pause the helper (`stop`), let the worker finish its in-flight request, close nothing else. After resume: run F1 `sync`, and check the worker with a 1-item `text` ping; restart it on failure. |
| F8 | Power / thermal | Lunar Lake on battery | `PowerPolicy` gating applies to the backfill only, as upstream. Additionally run the helper with `Nice=10`, `IOSchedulingClass=idle`, `CPUWeight=20` (systemd `--user` slice) so indexing never makes the desktop stutter. GPU contention: if the GPU is busy (`/sys/class/drm/card*/device/gt_busy_percent` or `intel_gpu_top`-style sampling, best effort), slow the batch rate. |
| F9 | Disk full | | Check free space before each commit batch (≥ 500 MB, or 5 % of the volume). Below that, pause with `paused_reason:"disk_full"`. SQLite errors `SQLITE_FULL` → roll back the batch and pause; never leave a half-written segment. The model download checks free space first (upstream does too). |
| F10 | DB corruption / unclean shutdown | power loss | WAL + `synchronous=NORMAL` (upstream). On open, run `PRAGMA quick_check`. On failure: move the DB to `index.sqlite.corrupt-<ts>`, start fresh, and tell the user ("The index was damaged and is being rebuilt"). Keep at most one corrupt copy. |
| F11 | Two daemons | Socket activation racing a manual start | `flock` on `$XDG_RUNTIME_DIR/trowel/troweld.lock`. The second instance exits with code 0 and a log line. |
| F12 | Index written by a newer/older version | downgrade via pacman | `meta.schema_version` > known → refuse to write, open read-only, and tell the user. Older → run the upstream migrations. Linux-only schema additions live in `linux_*` tables (PORT-MAP §Linux deltas) so a schema stays openable by the upstream code. |
| F13 | Model files missing, partial or tampered | interrupted download, disk errors | Verify sizes on every worker start and the SHA-256 once after download (stored as verified in `models/.verified`). Mismatch → `state:"model_missing"`, offer re-download, and never load an unverified GGUF. |
| F14 | Mesa/llama.cpp upgrade changes the vectors | `pacman -Syu` | The worker reports `vectorSource`; the daemon compares it with the index fingerprint (upstream `checkModel`). Before trusting a new build, the daemon runs the **built-in parity self-test** (embed the 5 reference texts plus the bundled photo, then compare with the shipped `vectors.json`) on first start after a worker binary change (keyed on binary mtime+size). On failure, mark the backend untrusted and fall back to CPU, or report it. A mismatch must never silently mix vectors from two runtimes in one index. |
| F15 | Non-UTF-8 / weird filenames | Linux paths are bytes | Store `files.path` as TEXT when valid UTF-8, else in the `linux_raw_paths` side table (see PORT-MAP). Never panic on `to_str()`. The testbed `traps/` gets invalid UTF-8, newlines in names, 255-byte names and leading dashes (subprocess args always go after `--`). |
| F16 | Permission changes, files deleted mid-read | | Treat as per-file `failed` with the reason. The `unreadable` root status is as upstream. |
| F17 | Sensitive content in the index | Screenshot OCR, documents | Index dir `0700`, DB files `0600`. Hard skip list on top of upstream's: `~/.ssh ~/.gnupg ~/.password-store ~/.local/share/keyrings ~/.mozilla ~/.config/*/Cookies ~/.local/share/Trash ~/.cache`. Storage settings provide "Delete index". Files moved to the Trash disappear from results on the next sync because Trash is skipped. |
| F18 | Shell plugin errors | QML exceptions inside omarchy-shell | The panel plugin holds **no state worth losing** and only talks to the socket. Errors are caught at the `Client.qml` boundary. Heavy work stays in `trowel-app` (UI-DESIGN §1). A plugin failure must never affect the lock screen or bar. Test it by `kill -STOP`-ing troweld while the panel is open: the panel must stay responsive and show "Trowel isn't responding". |
| F19 | UI ahead of / behind the daemon | partial upgrade | The `hello` protocol check (PROTOCOL §3). |
| F20 | Clock jumps / bad mtimes | | Reconcile compares `(size, mtime)` for **inequality**, never ordering. Files with future mtimes still sort newest-first, which is acceptable. |
| F21 | Process sampler on a busy or hostile system (Phase 9, D30) | thousands of fds, short-lived pids, other users' processes, slow `xe` fdinfo | Runs only while a client subscribes; one thread for all subscribers. Unreadable values (another user's `io`, fdinfo) are `None`, shown "—", never 0. A pid reused by a new process is detected by its start time and tracked afresh. Kept-open files are dropped with their process; the fd soft limit is raised toward 8192. Kill: only own processes, never pid 1 or the daemon, a refused/protected list (D30); `alive` after 500 ms tells the panel to offer Force quit. |
| F22 | Timers across restarts, suspend and clock changes (D32) | daemon restart, sleep, NTP step | Wall-clock `ends_at` in `state.sqlite`; on start every running/paused/recent timer is re-posted to the island and overdue ones fire (marked late when > 5 s past). The ticker waits at most 2 s while one runs (the monotonic wait doesn't advance in suspend), so a timer that ended in sleep fires within 2 s of waking. A notification or sound that fails to start (no `omarchy-notification-send`, no `pw-play`) falls back to `notify-send` / silence; the island alert stays. |
| F23 | The Island's producers misbehave | a script posts junk or never dismisses | `island_post` validates (id ≤ 64, title ≤ 120, ≤ 3 buttons, progress 0…1); a TTL drops forgotten activities; the list is memory only (a daemon restart clears it, except the keyboard lock re-posted from `keyboard-lock.json` and the timers from `state.sqlite`). Dead subscribers are dropped on the next send. |

## 2. Supervision layout (systemd `--user`)

```
troweld.socket   → ListenStream=%t/trowel/troweld.sock, SocketMode=0600
troweld.service  → Type=notify (sd_notify READY after DB open + fingerprint check)
                  Restart=on-failure, RestartSec=2, StartLimitBurst=5/60s
                  MemoryHigh=400M (daemon only — helpers run in their own scope)
                  WatchdogSec=30 (daemon pings from its main loop)
trowel-index.slice → helper + worker scopes: Nice=10, IOSchedulingClass=idle, CPUWeight=20,
                    MemoryHigh=3G, MemoryMax=4G
                    + D28 index_speed: gentle CPUQuota=100%, balanced 200%, full none;
                      gentle/balanced pinned to the efficiency cores (sched_setaffinity before
                      exec, inherited by worker/ffmpeg/tesseract; the user manager has no cpuset)
                      and the helper rests pace × each batch's embedding time (2 / 1 / 0)
```

Nice and `CPUWeight` only matter when something else wants the CPU; they never bounded the iGPU,
which embedding kept busy ~56 % on average and 100 % at p95 while the compositor shares it
(measured 2026-10-26, D28). The pace is the GPU limit; a lone new file is never rested on.

The daemon spawns helper and worker with `systemd-run --user --scope --slice=trowel-index.slice`
when available, and falls back to plain `fork/exec` with `setpriority` + `ioprio_set` when systemd
is absent (tests, CI). Logs go to journald (`journalctl --user -u troweld`), plus upstream's
`worker.log`/`indexer.log` in the index dir, rotated at 5 MB × 3.

## 3. `trowel doctor`

One command that answers "why isn't it working?" and that the Settings → Storage tab can show.
It returns OK/WARN/FAIL with a fix for each check:

- daemon reachable; protocol version; uptime
- model files present, sizes OK, verified hash
- worker starts; backend (vulkan/cpu); device name; **parity self-test result**; encode latency
- `vulkaninfo --summary` sees the GPU; Mesa version
- required tools on PATH with versions: `pdftotext pdftoppm pdfinfo ffmpeg ffprobe tesseract git`
  (plus optional ones: `wl-copy`, `evince|zathura`, `mpv`), and tesseract language packs
- inotify: `max_user_watches`, watches in use, and any roots on polling fallback
- index: schema version, `quick_check`, counts by state, `failed` files with reasons (top 20)
- disk: free space on the index volume
- hotkey: the bind is present in Hyprland (`hyprctl binds -j`), and any conflicts
- shell plugin: installed and enabled (`omarchy-shell shell listPlugins`)

## 4. Upgrade & uninstall

- **Upgrade**: on a pacman upgrade, the post-install hook prints "restart Trowel", and
  `systemctl --user try-restart troweld` runs on the next login. The daemon does the F14 self-test
  automatically.
- **Uninstall**: `pacman -R` removes binaries. A `trowel uninstall --purge` command removes
  `~/.local/share/trowel`, `~/.cache/trowel`, `~/.config/trowel`, the Hyprland bind include line, and
  disables the shell plugin. It is documented in the README and never runs implicitly.
