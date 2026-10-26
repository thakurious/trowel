# Phase 9 plan: the Island, Process Monitor, System Commands, Timers, Snippets

**Status: draft, 2026-10-26.** Requested by the human on 2026-10-26 ("Process Monitor, System
Commands, Timers, Snippets", and the keyboard-lock pill reworked into an iPhone-style Dynamic Island
that becomes a shared surface for other things). Nothing here is built yet. §2 lists the decisions
that need the human's sign-off **before** P1 starts; until then this file is the source of truth,
and the protocol, UI, testing and budget tables move into `PROTOCOL.md`, `UI-DESIGN.md`,
`TESTING.md` and `BENCHMARKS.md` in the commit that implements each part (as Phase 8 did with
`LAUNCHER-PLAN.md`).

Hard constraints, unchanged from Phase 8:

- **File search does not change.** No fidelity invariant, search constant, prompt, schema column
  of `index.sqlite` or eval gate is touched. The P4 evals re-run with identical numbers at the end.
- **QML never does work** (UI-DESIGN §1). Sampling, timers, ranking and storage live in `troweld`;
  the panel and the island only draw.
- **No bar widget** (D25 stands). The island is a layer surface of the existing `trowel` plugin,
  under the bar, not a bar entry; `shell.json` is not touched.
- Every command runs through an allow-list in the daemon, never a string the UI builds.

---

## 0. What exists today (verified 2026-10-26 on this machine)

### Trowel

- Phase 8 shipped: launcher (apps, commands with learned frecency D19), calculator, clipboard
  history, Settings in the panel (D26), Lock Keyboard (D27), index speed (D28).
- Built-in commands live in `ui/qml/Trowel/ListMerge.js` `COMMANDS` and open **views** inside the
  panel: `clipboard`, `settings`, `keylock` (Panel.qml `root.view`). Each view has its own
  `ListView` (never a swapped model: 2026-10-16 recycled-delegate bug) and its own right pane.
  `Esc`/Backspace on an empty field returns to the search it came from (`goBack`).
- The keyboard-lock countdown is `KeyLockPill.qml`: a `PanelWindow` (namespace `trowel-popup`,
  `WlrLayer.Overlay`, anchored top, `margins.top: 2`, 22 px high). **It sits on top of the bar**
  (bar layer `omarchy-bar` is 0,0 1536×26 logical) and hides the clock. The human wants it below
  the bar, animated like a Dynamic Island, and the same surface reused for other activities.
- The daemon already: writes the clipboard and sends the paste keystroke (D21, `clip_paste`),
  launches apps from a user-service context (L0.1), keeps `state.sqlite` (v4: apps, learned,
  clipboard, calc history), pushes events to subscribed clients (`subscribe` topics).
- Harness: `ui/harness/run.sh` renders the plugin's Panel offscreen against `troweld --mock`
  (`check.py`, 35 checks). Layer surfaces other than the panel are stripped there (no layer-shell
  offscreen), so the island needs its own harness entry point (P2).

### The machine and Omarchy

- **Per-process GPU** (verified): the `xe` driver exposes DRM fdinfo per open render node:
  `drm-cycles-rcs` / `drm-total-cycles-rcs` (render), `-ccs` (compute: our Vulkan worker), `-vcs`
  / `-vecs` (video), `-bcs` (copy), plus memory `drm-resident-gtt`, `drm-resident-system`.
  GPU % of a process = Δ`drm-cycles-*` / Δ`drm-total-cycles-*` summed over its DRM fds. Hyprland,
  quickshell and the trowel worker all show up.
- **Per-process disk**: `/proc/<pid>/io` `read_bytes`/`write_bytes` is readable for the user's own
  processes; root's (`/proc/1/io` is `0400 root`) are not. Rows we can't read show "—", never 0.
- **CPU / RAM**: `/proc/<pid>/stat` (utime+stime), `/proc/<pid>/statm` (RSS);
  `smaps_rollup` (PSS) is accurate but costs ~0.2–1 ms per process, so only for the selected row.
- **App grouping**: launched apps run in `app-*.scope` cgroups (L0.1: `app.slice/app-graphical.
  slice/app-Hyprland-gtk\x2dlaunch-<hash>.scope`); `/proc/<pid>/cgroup` maps a pid to its app.
- **System commands**: Omarchy ships 448 scripts with `# omarchy:summary=` metadata (and
  `omarchy:hidden=true` on internal ones): `omarchy-system-{lock,logout,reboot,shutdown}`,
  `omarchy-toggle-{nightlight,idle,notification-silencing,bar,touchpad,suspend,…}`,
  `omarchy-toggle-enabled <name>` (state of a toggle), `omarchy-restart-{wifi,bluetooth,audio,…}`,
  `omarchy-theme-bg-next`, `omarchy-theme-list`, `omarchy-update`, `omarchy-menu`.
- **Sounds / notifications**: `pw-play`, `paplay`, `notify-send`, `omarchy-notification-send`;
  `/usr/share/sounds/freedesktop/stereo/alarm-clock-elapsed.oga` exists.
- **Click-through layers**: Quickshell `mask: Region { … }` is what omarchy-shell's own bar and
  OSD use (`plugins/bar/Bar.qml:1439`, `plugins/osd/Osd.qml`), so an island that only takes input
  on its shape is a proven pattern here.
- **Text expansion**: Wayland gives no global key hook (and UI-DESIGN §8 forbids evdev/ydotool
  grabs). fcitx5 is the active IME (`fcitx5-mozc` installed, **no `fcitx5-lua`**), so as-you-type
  snippet expansion would need an fcitx5 addon: a new dependency (§2 P-D33).

---

## 1. Product shape

Five things, one surface rule: **commands open views inside the panel** (like Clipboard History and
Settings), and **anything that keeps running after the panel closes shows in the Island**.

### 1.1 The Island (Dynamic Island)

A pill centered **just below the bar** (top = bar height + 6 px; bar height read from omarchy-shell
`Style`, 26 fallback), on its own layer surface with an input mask on the shape only, so the rest
of the screen stays clickable. It never takes keyboard focus.

States (all transitions are one morph of width/height/radius with `SpringAnimation`, damping ~0.7,
~220 ms; `reduce_motion` makes them instant):

| State | Size (logical) | Shows | Enters |
|---|---|---|---|
| hidden | 0 | nothing (surface unmapped) | no activities |
| compact | ~200×30 | icon · title · trailing value (e.g. `0:42`) | an activity starts; after an alert |
| split | compact + a 30×30 bubble | the second activity's icon/value | two activities at once |
| expanded | ~380×(72–160) | title, subtitle, progress bar, up to 3 buttons | hover (300 ms), click, or an alert |
| alert | expanded + a short bounce | e.g. "Timer done" | an activity raises one; back to compact after 4 s or a click |

It grows out of the bar's bottom edge on appear (scale-y from 0 at the top center) and shrinks
back into it on dismiss. Activities are ordered by priority (keyboard lock > timer done > running
timer > others), at most two visible (split), the rest counted (`+2`).

**Producers** post activities to the daemon (`island_post`), not to QML, so any part of Trowel and
any script (`trowel island post …`) can use it:

```json
{"id": "timer:7", "icon": "timer", "title": "Tea", "subtitle": "Timer",
 "ends_at": 1793010000, "progress": null, "value": null, "priority": 50,
 "actions": [{"id": "pause", "label": "Pause"}, {"id": "add60", "label": "+1 min"},
             {"id": "cancel", "label": "Cancel", "danger": true}],
 "alert": false, "ttl": null}
```

`ends_at` makes the island count down by itself (no per-second messages); `progress` 0…1 draws a
bar; an action click goes back as `island_action {id, action}` to the producer. The keyboard lock
(D27) becomes the first producer and `KeyLockPill.qml` is deleted. Later producers the human
mentioned wanting ("this philosophy for other purposes"): timers, indexing a big batch (progress),
a running process-kill confirmation, now-playing, recording indicator: each is just a producer.

### 1.2 Process Monitor (command "Process Monitor", keywords: task manager, top, kill, cpu, ram)

- List of processes (own user by default; a chip shows all users read-only), **grouped by app**
  (cgroup scope → desktop entry name and icon), expandable to pids. Columns as row accessories:
  CPU %, GPU %, RAM, Disk (read+write /s).
- **Sort**: Tab cycles CPU → GPU → RAM → Disk (chips like the clipboard kinds); the header shows the
  active sort; the list re-sorts each sample without moving the selection off its process.
- Right pane: the process (or app) with name, command line, pid(s), user, uptime, and **sparklines**
  of the last 60 s for CPU/GPU/RAM/Disk; GPU split by engine (render/compute/video); VRAM.
- **Kill**: `Ctrl+K` → Terminate (SIGTERM), Force quit (SIGKILL, offered after 3 s if still alive),
  Copy PID, Show in btop. ↵ = Terminate with a second-↵ confirm (like Clear History). Protected
  processes (Hyprland, omarchy-shell/quickshell, systemd --user, pipewire/wireplumber, dbus, troweld,
  the lock screen, the user's login shell's session leader) need typing the name, or are refused
  (P-D30).
- Samples every 1 s **only while the view is open** (subscribe/unsubscribe); nothing runs otherwise.
- Footer: totals (CPU %, RAM used/total, GPU %, disk /s).

### 1.3 System Commands (searchable from the main list, like apps)

A curated set, each an allow-listed command id with a glyph, keywords and, for toggles, a live
state accessory (`On`/`Off` from `omarchy-toggle-enabled`):

| Group | Commands |
|---|---|
| Session | Lock Screen, Sleep, Log Out, Restart, Shut Down (last three confirm: ↵ twice) |
| Toggles | Night Light, Keep Awake (idle), Do Not Disturb, Hide Bar, Touchpad, Suspend in menu |
| Restart | Restart Wi-Fi, Bluetooth, Audio, Shell |
| Look | Next Background, Pick Theme (opens a theme list view: `omarchy-theme-list` / `-set`) |
| Trowel | Pause/Resume Indexing, Pause Clipboard, Lock Keyboard, Trowel Settings |
| Omarchy | "All Omarchy commands": every script with `omarchy:summary` and without `omarchy:hidden`, searchable, run in a terminal when it takes args |

They join the main list's **Commands** section and learn frecency (D19). The daemon runs them from
the L0.1 user-service context.

### 1.4 Timers (command "Timer"; also typed: `timer 25m tea`, `5 min`, `at 15:30`, `25m`)

- Parsing in the daemon (shared with the calculator's duration grammar where it fits): durations,
  clock times (today, or tomorrow when past), an optional label.
- The panel view lists running timers (with pause/resume, +1 min, cancel) and offers the parsed
  new one first; ↵ starts it and closes the panel.
- Running timers live in `state.sqlite` with **wall-clock** end times, so they survive the panel
  closing, a daemon restart and suspend (one that ended while asleep fires on resume, marked
  late).
- On fire: island alert, `omarchy-notification-send` (with "+5 min" and "Dismiss" actions if the
  notifier supports them), and `pw-play alarm-clock-elapsed.oga` (setting: sound on/off, file).
- Stopwatch and Pomodoro are P4-stretch (same storage, different producer).

### 1.5 Snippets (commands "Snippets", "Create Snippet"; keyword search from the main list)

- Stored in `state.sqlite` `snippets(id, name, keyword, body, tags, created, last_used, uses)`.
- Placeholders expanded by the daemon at paste time: `{clipboard}`, `{date}`, `{date:%Y-%m-%d}`,
  `{time}`, `{uuid}`, `{argument:name}` (asks in the pane before pasting). No `{cursor}`: a
  paste can't place the caret (documented).
- The view: search by name/keyword/body; right pane previews the expanded text; ↵ pastes into the
  focused app through the D21 path (Shift+Insert after the panel hides), `Ctrl+↵` copies, `Ctrl+K`
  edit/duplicate/delete. Typing a snippet's keyword (e.g. `;sig`) in the main field puts it first.
- "Create Snippet" is a form in the right pane (name, keyword, body as a multi-line field); also
  "Save clipboard as snippet" from Clipboard History's `Ctrl+K`.
- Import Raycast's snippet export (JSON `[{name, text, keyword}]`) and export the same format.
- **System-wide expansion while typing** (type `;sig` anywhere → replaced) is not in v1: it needs
  an fcitx5 addon (P-D33). v1 is panel-driven, which needs no new dependency.

---

## 2. Proposed decisions (need the human's sign-off before P1)

- **P-D29 Island service.** The daemon owns the activity list (`island_*` ops + an `island`
  subscribe topic); the plugin draws it on its own layer surface below the bar with an input mask.
  *Rejected:* each feature drawing its own popup (what the lock pill did), and an omarchy-shell
  bar widget (D25).
- **P-D30 Process kill policy.** Only the user's own processes; SIGTERM, then SIGKILL offered after
  3 s; a protected list that needs typing the name (or is refused for Hyprland and the lock screen).
  No privilege escalation, ever.
- **P-D31 System commands.** A fixed allow-list in the daemon (ids → argv), confirm on Log Out /
  Restart / Shut Down; "All Omarchy commands" only runs scripts found in `/usr/bin` +
  `~/.local/share/omarchy/bin` with `omarchy:summary` and not `omarchy:hidden`.
- **P-D32 Timers persist** in `state.sqlite` with wall-clock end times; the daemon fires them.
- **P-D33 Snippets v1 are panel-driven.** No system-wide expansion until the human approves an
  fcitx5 addon (new dependency: `fcitx5-lua`, or a small C++ addon).
- **P-D34 Protocol v3.** New ops and topics below; v2 clients keep working (capabilities gate).

---

## 3. Protocol v3 additions (`PROTOCOL.md` §4 gets these tables in P1)

| Op | Params | Reply / events |
|---|---|---|
| `island_post` | activity (§1.1) | `{}`; `island` event to subscribers |
| `island_update` | `{id, …fields}` | `{}` |
| `island_dismiss` | `{id}` | `{}` |
| `island_action` | `{id, action}` | `{}`; routed to the producer |
| `proc_subscribe` | `{interval_ms?: 1000, sort, group: "app"|"pid", all_users?}` | `proc` events: `{t, totals, rows:[{pid|app, name, icon, cpu, gpu, gpu_engines, ram, vram, disk_r, disk_w, readable}]}` |
| `proc_unsubscribe` | `{}` | `{}` |
| `proc_detail` | `{pid}` | cmdline, user, start time, PSS, history (60 samples) |
| `proc_kill` | `{pid|app, signal: "term"|"kill", confirm?: name}` | `{alive_after_ms}` or `not_allowed` |
| `system_commands` | `{}` | `[{id, name, glyph, keywords, group, confirm, state?}]` |
| `system_run` | `{id}` | `{}` / error |
| `timer_start` | `{text}` or `{seconds|at, label}` | the timer |
| `timer_list` / `timer_pause` / `timer_resume` / `timer_add` / `timer_cancel` | … | |
| `timer_parse` | `{text}` | `{seconds, at, label}` or null (for the live preview row) |
| `snippet_query` / `snippet_get` / `snippet_save` / `snippet_delete` | … | |
| `snippet_paste` | `{id, mode: "paste"|"copy", args?}` | via the D21 paste path |
| `snippet_import` / `snippet_export` | `{path}` | counts |

`state.sqlite` v5: `timers`, `snippets` (+ FTS5 on name/keyword/body), `island` is memory-only.

---

## 4. Phases

Branch `phase-9-island`, one PR per phase (P0–P2 may share one). Each phase: tests with the code,
harness checks for its view, the acceptance command and output pasted into `PROGRESS.md`.

### P0. Spikes (1 day). Record every result in PROGRESS §Verified facts

1. Island surface on the live shell (dev plugin id `io.github.thakurious.trowel-dev`, backup
   first): a `PanelWindow` anchored top with `margins.top` = bar height + 6, `mask: Region { item:
   shape }`; confirm clicks pass through beside it, it stays above windows and below the bar's
   popups, blur via the existing `trowel-(panel|popup)` layer rule, and a spring morph runs at
   60 fps (`QSG_RENDER_TIMING=1`, no dropped frames over 60 s).
2. Where omarchy-shell exposes the bar height and whether the bar can be at the bottom
   (`shell.json` `bar.position`); the island follows the top edge either way.
3. DRM fdinfo sampling cost for all processes (~400 pids): target ≤ 15 ms per tick; `xe` engine
   names; what our Vulkan worker's usage shows as (`ccs` or `rcs`).
4. `omarchy-notification-send` actions (does it support buttons?), and `pw-play` latency.
5. `omarchy-toggle-enabled` for each toggle in §1.3 (state readable without side effects).
6. Protected-process detection: Hyprland pid, omarchy-shell pid, lock-screen process name.

**Acceptance:** each item's command + output in PROGRESS; any surprise updates this plan first.

### P1. Protocol v3, state v5, mock (1–2 days)

`trowel-proto` ops above with serde tests; `state.sqlite` migration 4→5 (tested from a v4 copy);
`troweld --mock` answers every new op from fixtures (processes with fake GPU/disk, two timers, five
snippets, an island activity); golden transcripts in `proto_replay`.
**Acceptance:** `cargo test --workspace` green; `proto_replay` covers each op; a v4 `state.sqlite`
backup migrates and keeps its clipboard rows.

### P2. The Island (2–3 days)

- Daemon: activity registry, priorities, TTLs, `island` topic; `trowel island post|dismiss` CLI.
- Plugin: `Island.qml` (replaces `KeyLockPill.qml`): states and morphs of §1.1, split bubble,
  expanded buttons → `island_action`, `reduce_motion`, theme tokens, multi-monitor (the focused
  monitor's top center).
- Migrate Lock Keyboard: `trowel keyboard lock` posts `{id: "keylock", ends_at, actions: [Unlock]}`;
  the timer-based unlock (D27) is unchanged; Unlock → `island_action` → `trowel keyboard unlock`.
- Harness: an `island` entry point rendering each state offscreen (`ui/harness/island.sh`), pixel
  checks for size/position per state.
**Acceptance:** live: a 10 s keyboard lock shows the island **below** the bar (bar clock fully
visible; screenshot), it morphs compact → expanded on hover, Unlock works by click, it shrinks into
the bar when the lock ends; 0 dropped frames in `QSG_RENDER_TIMING` during the morphs.

### P3. System Commands (1–2 days)

Allow-list + toggle state + "All Omarchy commands" scanner (metadata parse, tests over fixture
scripts), `system_commands`/`system_run`, rows in the Commands section with state accessories,
confirm flow for the three session commands, a Theme view.
**Acceptance:** harness checks (search "night" → Night Light with its state; ↵ on Shut Down only
arms); live: Toggle Night Light flips it and the accessory follows; Lock Screen locks.

### P4. Timers (2 days)

Parser (table-driven tests: `25m`, `1h 30m`, `90s`, `at 15:30`, `15:30 tea`, `timer 5 min pizza`,
rejects), scheduler (wall clock, survives restart; suspend handled through the existing F7
`PrepareForSleep` hook), notifications + sound, island producer, Timer view.
**Acceptance:** `cargo test` parser table; live: `timer 10s test` → island countdown → alert +
sound + notification at 10 s (±0.5 s); a 2 min timer survives `systemctl --user restart troweld`.

### P5. Process Monitor (3–4 days)

Sampler (`/proc` + fdinfo + io + cgroup→app), sort/group, history ring, PSS on detail, kill with the
P-D30 policy; Process Monitor view (chips, live list, sparklines in the pane, Ctrl+K actions).
**Acceptance:** sampler ≤ 15 ms/tick p95 at ≥ 400 processes (BENCHMARKS row); a test process that
burns CPU (`yes > /dev/null`) is top under CPU sort within 2 s and Terminate ends it; the trowel
worker shows GPU % while indexing; protected processes refuse a plain Terminate; harness checks.

### P6. Snippets (2–3 days)

Store + FTS, placeholder expansion (tests), arguments form, paste/copy via D21, Snippets and Create
Snippet views, "Save clipboard as snippet", Raycast JSON import/export, keyword match in the main
list.
**Acceptance:** tests for expansion and import; live: `;sig` in the panel → ↵ pastes the expanded
text into a Ghostty and a Chromium text field.

### P7. Hardening, docs, merge (1–2 days)

Budgets into BENCHMARKS, UI-DESIGN §3 keyboard rows, TESTING rows, ROBUSTNESS (sampler and timer
failure modes), P4 evals re-run identical, `cargo clippy -D warnings`, `qmllint`, harness green,
PR to `main`.

---

## 5. Budgets (to `BENCHMARKS.md` when each part lands)

| Thing | Budget |
|---|---|
| Island morph | 0 dropped frames at 60 Hz; first frame ≤ 50 ms after `island_post` |
| Island idle cost | no timers firing in QML when nothing is shown; ≤ 1 wake/s while counting |
| Process sampler | ≤ 15 ms CPU per 1 s tick at 400 processes; zero cost when the view is closed |
| `system_run` | command started ≤ 100 ms after ↵ |
| Timer accuracy | fires within 500 ms of its end; survives daemon restart and suspend |
| `snippet_query` | p95 < 5 ms at 1,000 snippets |

## 6. Effort and order

P0 → P1 → **P2 (Island first: Timers and the lock depend on it)** → P3 ∥ P4 → P5 → P6 → P7.
About 13–18 working days. P3 and P6 are independent of the island and can move earlier if the human
wants Snippets sooner.

## 7. Carried over (still open from Phase 7/8, not part of Phase 9 unless the human says so)

- Phase 7: 72 h `$HOME` soak, fresh-account `makepkg -si`, remaining TESTING §7 fault rows.
- Phase 8: L4 live acceptance numbers, L3 24 h orphan check, L5 (setup prompts, doctor rows,
  PKGBUILD), L0.5 password-manager copy, L6 extras (window switcher, clipboard OCR), 8U upstream
  parity U1–U12.
- Human questions parked 2026-10-26: whether omasnap should save every capture (`--copy --save`)
  or Trowel should OCR clipboard images (L6) — "leave it for now".

## 8. Answers

(Empty until the human signs off §2.)
