# Trowel features

Everything Trowel does, where to find it, and its keys. Press **Super+D** to open the panel; every
command below is found by typing its name (or a keyword) there. `Ctrl+K` shows the actions for the
selected item anywhere, and `Esc` steps back one level at a time. The full key map is
[UI-DESIGN.md §3.4](UI-DESIGN.md); design decisions are numbered in [DECISIONS.md](DECISIONS.md).

## The main search

One field, several sources, merged in one list (D18):

| Source | Example | Enter | Notes |
|---|---|---|---|
| **Files** | `payment error screenshot`, `zebra in a video` | Open at the place that matched | On-device semantic + keyword search of screenshots, photos, PDFs, documents, audio, video and code (a port of DigUp, EmbeddingGemma 2 on the GPU). `Ctrl+Enter` reveals, `Space` Quick Look, `Tab` / `Ctrl+1…7` filter by kind, `code:` searches code |
| **Applications** | `fire`, `slack` | Launch (`uwsm-app`) | Omarchy's ranking plus what you launch and the words you used (D19). `Ctrl+K`: desktop actions, hide |
| **Calculator** | `5 ft to cm`, `200 + 15%`, `0xff to binary` | Copy (`Ctrl+Enter` pastes) | `fend-core`, offline, 20 ms limit (D24) |
| **Commands** | `clip`, `settings`, `timer`, `night` | Open the command / run it | Built-in commands and system commands (below) |
| **Typed timer** | `25m tea`, `1h 30m`, `at 15:30` | Start it | "Start Timer · 25:00 tea" leads the list |
| **Snippet keyword** | `;sig` | Paste the snippet | Exact keyword match puts it first |

The panel reopens where you left it within 90 s (Raycast's pop to root; set in Settings).

## Commands (views inside the panel)

### Clipboard History (D20, D21)
Text, links, colors, code, formatted text, images and files you copied, searchable, with pins.
↵ pastes into the app you were in, `Ctrl+Enter` copies, `Shift+Enter` pastes as plain text,
`Ctrl+Delete` removes, `Ctrl+Shift+P` pins, `Tab` cycles All · Text · Images · Files · Links ·
Colors. `Ctrl+K` → Save as snippet, Clear, Pause capturing. Password managers' copies and excluded
apps are never recorded. Direct: `omarchy-shell trowel clipboard`.

### Trowel Settings (D26) — `Ctrl+,`
Folders, code folders, the shortcut, clipboard limits and excluded apps, hidden apps, index speed
(gentle / balanced / full, D28), timer sound, pop to root, reduce motion, model and health checks.
↵ flips a switch or opens the editing pane; ←/→ change a choice.

### Lock Keyboard (D27)
Pick 15 s … 30 min (or type `45`, `3m`). Every physical keyboard is ignored; the mouse works, the
power button and virtual keyboards stay on, and a systemd timer unlocks it even if Trowel stops.
The countdown and an **Unlock** button are on the Island. CLI: `trowel keyboard lock 30`.

### Timer (D32)
Presets, or type `25m`, `1h 30m`, `90s`, `at 15:30`, `10 tea` (a bare number is minutes here).
↵ starts it and closes the panel; on a running timer ↵ pauses / resumes, `Ctrl+Delete` cancels,
`Ctrl+K` adds 1 or 5 minutes. Timers keep wall-clock end times in the daemon, so they survive the
panel closing, a restart and sleep. When one ends: the Island expands with **+5 min / Dismiss**, a
notification, and a chime (Settings → Timer Sound). CLI: `trowel timer start 25m tea | list | cancel`.

### Process Monitor (D30)
Each app (or, with `Ctrl+G`, each process) with CPU, GPU (from the `xe` driver's per-client
counters), memory and disk; `Tab` sorts by CPU · GPU · Memory · Disk. The pane shows the command
line and 60-second graphs. ↵ arms **End**, ↵ again sends SIGTERM; `Ctrl+K` → Force quit, Copy PID,
Open btop. Only your own processes; Hyprland, the shell, systemd and Trowel are refused, and
PipeWire, D-Bus, fcitx5 and the portals need their name typed. It samples once a second only while
the view is open.

### Snippets and Create Snippet (D33)
Saved text with a name and an optional keyword. Placeholders filled when it pastes: `{clipboard}`,
`{date}`, `{time}`, `{datetime}`, `{date:%d %b}`, `{uuid}`, `{argument:name}` (the pane asks).
↵ pastes, `Ctrl+Enter` copies, `Ctrl+N` new, `Ctrl+E` edit, `Ctrl+Delete` delete; `Ctrl+K` also
Duplicate, Export to `~/trowel-snippets.json`, Import `~/raycast-snippets.json` (Raycast's format).
Snippets don't expand while typing in other apps (that would need an fcitx5 addon).

### System commands (D31)
- **Session:** Lock Screen, Sleep, Log Out, Restart, Shut Down (the last three: ↵ twice).
- **Toggles** (On/Off shown live): Night Light, Keep Awake, Do Not Disturb, Hide Bar, Touchpad,
  Suspend in Menu.
- **Restart:** Wi-Fi, Bluetooth, Audio, Shell. **Look:** Next Background, **Pick Theme**.
- **Omarchy Commands:** every `omarchy-*` script with a summary, searchable; ones that ask
  questions open in Omarchy's terminal.
- **Trowel:** Pause / Resume Indexing, Pause / Resume Clipboard History.

Only a fixed allow-list runs; the panel sends ids, never command lines.

## The Island (D29)

A small near-black pill that grows out of the bar's bottom edge, centered on the focused monitor,
for things that keep running after the panel closes: the keyboard lock, timers, and anything a
script posts. Compact shows a ring, a title and a countdown; a second activity sits in a round
bubble beside it; hover (or a click) expands it with a progress bar and up to three buttons; an
alert expands and bounces once. It never takes the keyboard and only its shape takes clicks.

```bash
trowel island post backup "Backing up" --progress 0.4 --value 40% --button stop:Stop --ttl 600
trowel island watch backup --once      # prints {"activity_id":"backup","action":"stop"} on a click
trowel island dismiss backup
```

## Direct entry points

| Command | Opens |
|---|---|
| `omarchy-shell trowel toggle` | the panel (Super+D) |
| `omarchy-shell trowel clipboard` | Clipboard History |
| `omarchy-shell trowel settings` | Settings |
| `omarchy-shell trowel desk timers\|procs\|snippets\|omarchy\|themes` | that view |

## Privacy

Everything runs on this computer; Trowel goes online only to download its model once. The index,
clipboard history, timers and snippets live under `~/.local/share/trowel` (owner only).
