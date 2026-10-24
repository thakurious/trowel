# Launcher & clipboard plan (Phase 8+)

**Status: approved 2026-10-13.** The human accepted every recommendation. P-D18…P-D24 are now
**D18–D24** in `DECISIONS.md`, and §7 records the answers. The protocol, UI, testing and budget
tables move into `PROTOCOL.md`, `UI-DESIGN.md`, `TESTING.md` and `BENCHMARKS.md` in the phase
commit that implements them (L1, L4, L2–L3, L5). Until then this file is their source of truth.

Goal: make Super+D a **Raycast-style command bar for Omarchy**. One field that launches apps, calculates
inline (math, units, bases, dates, percentages), recalls rich clipboard history (text, links,
colors, code, images, files) and keeps doing what it does today, which is semantic file search. Every integration uses Omarchy's own mechanisms (launch path, hidden
entries, theme tokens, plugin host, paste keystroke), so it behaves like a first-party Omarchy
surface instead of a foreign launcher.

Hard constraint: **file search does not change.** No fidelity invariant, search constant, prompt,
schema column or eval gate is touched. Calculator, apps and clipboard are separate result sources that are
merged in the UI, and they never enter `Searcher`'s z-scoring. The P4 evals are re-run as a
regression gate at the end of L4.

---

## 0. What exists today (verified 2026-10-13 on this machine)

### Trowel

- `troweld` is a resident user daemon (socket-activated) that owns the index and answers PROTOCOL
  v1 over `$XDG_RUNTIME_DIR/trowel/troweld.sock`. The ops are `search`, `typing`, `preview`,
  `thumbnail`, `open`, `reveal`, `copy_path`, `quick_look`, config, status and doctor.
- The quick panel is the omarchy-shell plugin `io.github.thakurious.trowel` (`kind: panel`,
  `keepLoaded`). It is a 780 px panel: a 330 px list plus a 450 px preview column, using two-phase
  keyword→hybrid search with `gen` stale-dropping. The flat `ListModel` in `Client.qml` holds
  `rowType` `row|header|fold`.
- `trowel-app` is a separate Quickshell instance that holds the main window, Quick Look, Settings
  and Onboarding.
- Rule (UI-DESIGN §1): **QML never does work.** Ranking and filtering live in the daemon.
- PROTOCOL §4 `copy_path`: "the daemon never touches the clipboard". **This plan changes that**
  (P-D21).

### Omarchy

- **Launch path**: `services/AppLibrary.qml` launches with
  `uwsm-app -- gtk-launch <id>.desktop`. That places the app in a scope under `app-graphical.slice`,
  not in the caller's cgroup. Hidden entries are `$OMARCHY_PATH/default/omarchy/launcher.hides` plus
  `services/hidden-entries.sh` (`Hidden`/`NoDisplay`/`OnlyShowIn`/`NotShowIn` against
  `XDG_CURRENT_DESKTOP`, first-seen desktop-id wins, `~/.local/share/applications` first).
  Ranking is `services/AppSearch.js` `fuzzyScore` (name prefix 10000, id prefix 9500, substring,
  haystack, acronym ≤5 chars). Icons come from a scan of `*/apps/*` and `*/devices/*` that prefers
  SVG, with a fallback to `Quickshell.iconPath`.
- **App-library facade**: third-party plugins get `shell.appLibrary` **only if the manifest lists
  `kind: "menu"`** (`shell.qml:603`). We do not rely on it (P-D19).
- **Clipboard**: the first-party `omarchy.clipboard` overlay (`keepLoaded`, Super+Ctrl+V) runs
  `wl-paste --type text --watch` and `wl-paste --type image/png --watch` into `capture.sh`. The
  history is `~/.local/state/omarchy/clipboard-history.json`, capped at 300 entries, with images in
  `clipboard-images/<sha256>.<ext>`. It skips `x-kde-passwordManagerHint` and
  `CLIPBOARD_STATE=sensitive`. Pasting runs `wl-copy`, sleeps 0.15 s, then
  `wtype -M shift -k Insert -m shift` (works in terminals and GUI apps).
  - The gaps are what Trowel can fill:
    - It captures only plain text and PNG: no files (`text/uri-list` / `x-special/gnome-copied-files`), no HTML, JPEG or WebP.
    - Timestamps are display strings like `"Tuesday 14:03"`.
    - Search is a substring match in JS.
    - There is no pinning and no source app.
    - **Images are never garbage-collected.** Here it holds 401 files (145 MB) for 50 live image entries.
  - On this machine the history has 250 text entries, 50 image entries and 0 file entries.
- Tools present: `wl-clipboard 2.3.0`, `wtype 0.4`, `uwsm-app`, `gtk-launch`. The systemd user env
  carries `WAYLAND_DISPLAY=wayland-1`, `HYPRLAND_INSTANCE_SIGNATURE` and `DISPLAY` (uwsm imports
  them), so `troweld` can talk to the compositor.
- Taken binds: Super+Space (Omarchy menu), Super+Alt+Space (Apps menu), Super+Ctrl+V (clipboard
  manager), Super+C/V (universal copy and paste). Trowel owns Super+D.
- 166 `.desktop` files are installed (the Omarchy webapps are ordinary `.desktop` files in
  `~/.local/share/applications`).

---

## 1. Product shape

```
┌ Super+D ────────────────────────────────────────────────────────────┐
│ 🔍 fire                                                         12  │
├───────────────────────────────┬─────────────────────────────────────┤
│ APPLICATIONS                  │  [Firefox icon 96]                  │
│ ▸ Firefox            ⏎ Open   │  Firefox · Web Browser              │
│   Firefox — Private window    │  /usr/share/applications/firefox…   │
│ CLIPBOARD                     │  Launched 14× · last 2 h ago        │
│   🖼 Screenshot 14:02  ⌘1     │                                     │
│   "firewall rules for…"       │  Actions: New window · Private      │
│ FILES · DOCUMENTS             │                                     │
│   firewall-audit.pdf  p. 3    │                                     │
├───────────────────────────────┴─────────────────────────────────────┤
│ ⏎ Open  ⌃K Actions  ⌃⏎ Reveal  Tab Code                             │
└─────────────────────────────────────────────────────────────────────┘
```

Typing a calculation puts a card at the top. It is selected, so `Enter` copies the result:

```
┌ Super+D ────────────────────────────────────────────────────────────┐
│ 🔍 1920*1.25 + 15%                                                  │
├─────────────────────────────────────────────────────────────────────┤
│  1920 × 1.25 + 15%                  →                     2,760     │
│  Expression                                               Result    │
├───────────────────────────────┬─────────────────────────────────────┤
│ FILES · DOCUMENTS             │                                     │
│   display-scaling.md          │                                     │
├───────────────────────────────┴─────────────────────────────────────┤
│ ⏎ Copy  ⌃⏎ Paste  ⇧⏎ Copy expr = result  ⌃K Actions                 │
└─────────────────────────────────────────────────────────────────────┘
```

- **Root view** (the field is empty): *Suggestions* (top 6 frecent apps), *Recent clipboard*
  (top 5), then the existing empty-state copy. Raycast's root behaves the same way.
- **Typing**: sections appear in a fixed order: the **Calculator** card (only when the query is a
  calculation, see L2c), **Applications** (≤ 5 rows), **Clipboard** (≤ 3 rows), then the **file
  groups** exactly as today (keyword then hybrid, folds unchanged).
  Applications and clipboard come back in < 5 ms, so they paint before the keyword file hits and the
  first row stays stable while file results stream in under it.
- **Scoped modes** (Raycast "commands"). Each is a prefix or a keyword, and `Backspace` on an empty
  scoped field returns to root:
  - `clip ` / dedicated bind → **Clipboard view**: the full history, a kind filter
    (All/Text/Images/Files/Links/Colors) and a large preview.
  - `=` → **Calculator view**: forces calculation (no apps or files), and lists recent
    calculations under the live result.
  - `code:` → the existing code mode (unchanged).
  - `apps ` → all applications, A–Z.
- **Action panel (`Ctrl+K`)**: a popover over the selected row that lists every action for it with
  its keycap. This is Raycast's signature interaction. File rows get Open, Reveal, Quick Look and
  Copy path. App rows get Launch, desktop actions (`[Desktop Action …]`), Hide from launcher (writes
  the Omarchy-compatible hide, see L2) and Copy .desktop path. Clipboard rows get Paste, Copy, Paste
  as plain text, Pin/Unpin, Open (the URL/file/image editor through `omarchy-clipboard-open`
  semantics) and Delete. The Calculator card gets Copy result, Paste result, Copy expression,
  Copy as `expr = result`, and Copy in another base or unit when that applies.

### Keyboard additions (UI-DESIGN §3.4 gets these rows)

| Key | Root / mixed | Clipboard view |
|---|---|---|
| `Enter` | Calc → **copy result** and close. App → launch. Clip → **paste** into the previous window. File → open (unchanged) | Paste |
| `Ctrl+Enter` | Calc → paste result into the previous window. File → reveal (unchanged). Clip → copy only (no paste) | Copy only |
| `Shift+Enter` | Calc → copy `expr = result`. Clip → paste as plain text | Paste as plain text |
| `Ctrl+K` | Action panel | Action panel |
| `Ctrl+1…9` | — | Paste the Nth item (Raycast/Alfred muscle memory) |
| `Ctrl+P` | (already "up") | — so pin uses `Ctrl+Shift+P` |
| `Ctrl+Backspace` | — | Delete the entry (with undo toast, 5 s) |
| `Tab` | Code mode (unchanged) | Cycle kind filter |

---

## 2. Proposed decisions (need sign-off)

**P-D18. Scope.** Trowel becomes a launcher plus clipboard manager plus file search. The product
is still Trowel, and Super+D stays the bind. We do **not** take or offer to take Super+Space (the
Omarchy menu). *(Approved: only the Super+Ctrl+V opt-in of D23 is offered.)*

**P-D19. Apps are indexed by `troweld`, launched the Omarchy way.** The daemon parses `.desktop`
files itself. It does this with a hand-written Desktop Entry parser (the format is INI-like, about
200 lines, so no new crate is needed). It watches the application dirs with the existing `notify`
crate. Ranking is a **Rust port of Omarchy's `AppSearch.js` `fuzzyScore`** plus a frecency term. It
launches with `uwsm-app -- gtk-launch <id>.desktop`, byte-for-byte what `AppLibrary.launch` runs.
*Rejected:* declaring `kind: "menu"` to get `shell.appLibrary`. That breaks the "QML never does
work" rule (JS sort per keystroke inside the lock-screen process), isn't available to
`trowel-app` or the CLI, couples us to a facade that is undocumented beyond one README sentence, and
offers no frecency. Desktop actions (`[Desktop Action x]`) can't go through `gtk-launch`, so for
those the daemon expands `Exec` field codes per the spec and runs `uwsm-app -- <argv>`.

**P-D20. Trowel captures its own clipboard history, in its own store.** The daemon supervises
`wl-paste --watch trowel clip-capture` (no new dependency; Omarchy does the same). It keeps metadata
in `~/.local/share/trowel/state.sqlite` and blobs in `~/.local/share/trowel/clipboard/` (`0700` dir,
`0600` files). It is a separate database from `index.sqlite`, so D16 schema parity and F10 are
untouched. On first enable it **imports** Omarchy's `clipboard-history.json` (and the images it
references) once. *Rejected:* reading Omarchy's JSON as the source of truth. It is written by
another process, is limited to 300 text/PNG entries, has no real timestamps and supports no files or
pins, and two writers on one JSON file would race.

**P-D21. The daemon may write the clipboard (amends PROTOCOL §4 `copy_path`).** Restoring an image
or a file list needs the stored bytes and the right MIME types, which QML cannot do without file
I/O. The UI hides the panel first, and the daemon then runs `wl-copy` plus the Omarchy paste
keystroke (`wtype -M shift -k Insert -m shift` after 150 ms). `copy_path` keeps its old behavior for
compatibility.

**P-D22. Protocol v2.** Adds the ops in §3. The `hello` capabilities gain `apps` and `clipboard`.
v1 ops are unchanged. Plugin, app and daemon ship in one package, so the existing "versions differ
→ restart" handshake rule is enough.

**P-D23. Coexistence with `omarchy.clipboard`.** Both watchers can run, since reading the clipboard
is harmless. `trowel setup` *offers* (it never does this silently) two changes: rebinding
Super+Ctrl+V to `omarchy-shell trowel clipboard`, and adding `omarchy.clipboard` to
`disabledPlugins[]` so there is one history. Each change is backed up as
`<file>.bak.trowel-<ts>` and recorded for `trowel uninstall`. That is the CLAUDE.md desktop-config
rule.

**P-D24. Calculator engine: `fend-core`, in-process, offline.** `fend-core` is a Rust crate (MIT,
no transitive dependencies) that evaluates arbitrary-precision arithmetic, units (`5 ft to cm`,
`3 GiB in MB`), number bases (`0xff to binary`), percentages, functions (`sqrt`, `sin`, `ln`) and
some dates/times. It runs inside `troweld` in well under a millisecond, with an interrupt hook for
timeouts. That makes it the only option that fits the per-keystroke budget without a subprocess.
**This is a new dependency and needs approval.** Verify its API and features at the pinned version
in L0. *Rejected:* `qalc` (libqalculate 5.12, 16 MB plus curl/icu/mpfr/gmp, not installed). It is
more powerful, but it is a subprocess per keystroke (or a long-lived REPL to supervise), it is a
new system package, and it would fetch currency rates over the network. *Rejected:* a hand-written
evaluator. It handles arithmetic fine, but Raycast-level units and bases would mean writing a
calculator library. **Currency is off by default** (D13: no network beyond the model download).
An opt-in daily exchange-rate fetch is a separate question for the human.

---

## 3. Protocol v2 additions (`PROTOCOL.md` §4 gets these tables)

Request params are `app_id` / `clip_id`, not `id`: ops are flattened into the request envelope,
which already has `id` (found in L1; `PROTOCOL.md` §4 "Launcher" is the contract).

### Apps

| op | params | result |
|---|---|---|
| `apps_query` | `{text, limit=8, gen}` | `{gen, rows:[AppRow]}`. An empty `text` returns frecency order. Supersedes the connection's older `apps_query`, like `search`. |
| `app_launch` | `{app_id, action?}` | `{}`. Records a launch for frecency. |
| `app_hide` | `{app_id, hidden:bool}` | `{}`. Writes `~/.config/trowel/launcher.hides` (Trowel-owned; Omarchy's file in `$OMARCHY_PATH` is read-only). |

```jsonc
// AppRow
{"id":"firefox","name":"Firefox","generic":"Web Browser","comment":"…",
 "icon":"/usr/share/icons/hicolor/scalable/apps/firefox.svg",  // resolved path or null
 "icon_name":"firefox",                                           // QML fallback: Quickshell.iconPath
 "terminal":false, "actions":[{"id":"new-private-window","name":"New Private Window"}],
 "source":"/usr/share/applications/firefox.desktop",
 "launches":14, "last_launched":1791869000, "matched":[[0,4]]}   // name highlight ranges
```

### Clipboard

| op | params | result |
|---|---|---|
| `clip_query` | `{text, kinds?, limit=50, offset=0, gen}` | `{gen, rows:[ClipRow], total}` |
| `clip_get` | `{clip_id}` | `{row, text?, html?, files?:[path], image?:{file,w,h}}` with the full content for the preview (text capped at 256 KiB, `truncated:true`). |
| `clip_paste` | `{clip_id, mode:"paste"\|"copy"\|"plain"}` | `{}`. The UI must already have hidden the panel. |
| `clip_pin` / `clip_delete` | `{clip_id, pinned?}` | `{}` |
| `clip_clear` | `{keep_pinned=true}` | `{removed}` |
| `clip_pause` | `{minutes?}` (`0` resumes) | `{paused_until}` (incognito mode, shown in the bar widget) |
| *event* `clipboard` | — | Topic `"clipboard"` on `subscribe`: `{added?:ClipRow, removed?:[id]}`, coalesced ≤ 4/s. |

```jsonc
// ClipRow
{"id":812, "kind":"text|link|color|code|image|files|html",
 "preview":"firewall rules for…",                    // ≤ 200 chars, whitespace-collapsed
 "mimes":["text/html","text/plain"], "bytes":1824,
 "thumb":"/home/u/.cache/trowel/clip/812@132.png",   // images and single-image files
 "files":2, "color":"#F2A22A", "domain":"github.com",
 "source_app":"firefox",                              // Hyprland activewindow class at capture time
 "created":1791869354, "last_used":1791869354, "uses":3, "pinned":false}
```

### Calculator

| op | params | result |
|---|---|---|
| `calc` | `{text, force=false, gen}` | `{gen, result?: CalcResult}`. No `result` means "not a calculation", and the UI shows no card. `force` (the `=` view) also returns `error` text for an invalid expression. Supersedes older `calc` calls on the connection. |
| `calc_history` | `{limit=20}` | `{rows:[{expr, result, at}]}` (state.sqlite, recorded when a result is copied or pasted) |

```jsonc
// CalcResult
{"expr":"1920*1.25 + 15%", "pretty_expr":"1920 × 1.25 + 15%",
 "display":"2,760",               // grouped with the user's locale (LC_NUMERIC)
 "copy":"2760",                   // what Enter copies: no grouping, full precision
 "kind":"number|unit|base|date|duration",
 "unit":null, "approx":false,     // approx → UI shows "≈"
 "alts":[{"label":"Hex","copy":"0xac8"}]}  // alternate representations for the action panel
```

Copying and pasting a result go through the same daemon clipboard path as `clip_paste`
(P-D21). That path is `calc_copy {text, expr?, mode:"copy"|"paste"}` (`expr` goes to `calc_history`).

`troweld --mock` gets canned apps (about 30 real-looking entries with icons) , clipboard (all
seven kinds) and a calculator stub (a handful of fixed expressions), so the UI track can start against the mock before L2 and L3 land, as Phase 6 did.

---

## 4. Phases

The new work is Phase 8, on branch `phase-8-launcher`. It may start while Phase 7's soak runs, since
it touches no Phase 7 gate. One PR per sub-phase.

### L0. Spikes and verification (1–2 days). Record every result in PROGRESS §Verified facts

1. **Launch from the daemon**: `troweld` (systemd user service) runs
   `uwsm-app -- gtk-launch firefox.desktop`. Verify the app lands in `app-graphical.slice` with
   `systemctl --user status`, not in `troweld.service`, and that Wayland env plus
   `XDG_ACTIVATION_TOKEN`/focus behave. If focus-stealing prevention blocks it, launch from the
   plugin with `Quickshell.execDetached` and keep the daemon as the source of the argv.
2. **MIME offers**: run `wl-paste --list-types` after copying from Nautilus (file and folder),
   Firefox (selected HTML, image "Copy Image"), Chromium, Alacritty, LibreOffice and a screenshot
   (`omarchy` screenshot tool). Record the lists in the PROGRESS facts.
3. **Re-offering files**: does a single `wl-copy --type text/uri-list` paste files into Nautilus,
   Chromium uploads and Telegram? If multiple types are required in one offer (`wl-copy` offers one
   type per call), **stop and ask** about adding `wl-clipboard-rs` (a new dependency, which needs
   approval). The fallback is to offer only `text/uri-list`.
4. **Paste keystroke** after the panel hides: verify that `wtype -M shift -k Insert -m shift` lands
   in Alacritty, Ghostty, Chromium, a GTK4 app and a Qt app, and that the 150 ms delay is enough
   after the layer surface drops exclusive focus. Measure the delay, don't assume it.
5. **Sensitive content**: copy from 1Password and Bitwarden (desktop and browser) and confirm that
   `x-kde-passwordManagerHint` and `CLIPBOARD_STATE=sensitive` appear in the capture env.
6. **`wl-paste --watch` without `--type`**: confirm it fires on every selection change regardless of
   type (wl-clipboard 2.3.0), so that one watcher replaces Omarchy's two.

**L0 results (2026-10-15, details in PROGRESS §Verified facts):** files re-offer as a single
`text/uri-list` (no `wl-clipboard-rs`); the 150 ms paste delay holds in Ghostty, Chromium, GTK4 and
Qt6; no real copy carried `x-kde-passwordManagerHint` or `CLIPBOARD_STATE=sensitive`, so the source
app exclusion list is the main sensitive guard, with the hints as an extra check. The human's file
manager is Strata, which offers `text/uri-list` plus the portal types.
7. **`fend-core`** (only after the dependency is approved): pin a version, then read its source for
   the `evaluate` / interrupt / exchange-rate-handler API. Confirm it has no transitive dependencies
   (`cargo tree`). Check `%` semantics, unit coverage (data sizes, temperature, time) and date
   support against the L2c table before relying on it.

**Acceptance:** all seven facts are recorded with the commands and outputs. Any "no" is turned into a
plan or doc change before L2.

### L1. Protocol v2, state DB, mock (2 days)

- `trowel-proto`: the §3 types, round-trip tests, and `protocol: 2`.
- `troweld/src/state.rs`: `state.sqlite` with the `apps_launches(id, count, last)`,
  `clip_entries(...)`, `clip_blobs(sha256, ext, bytes, refcount)` and `clip_fts` tables (FTS5 with
  the **same tokenizer string** as the index, for consistent Unicode behavior). `quick_check` on
  open and quarantine on failure (F10 parity).
- `troweld --mock`: §3 fixtures. Add golden transcripts in `tests/proto/v2_*.jsonl`.

**Acceptance:** `cargo test --workspace` green, `proto_replay` passes v1 **and** v2 transcripts
against the mock, the real daemon speaks v2 (`daemon.rs`: v2 hello, v2 ops `not_ready` until L2/L3,
`apps`/`clipboard` not advertised yet), and a v1 client is refused with the restart message.
*(Amended in L1: the transcripts have only ever replayed against the mock, since real results
depend on an index; the real daemon's protocol behavior is tested in `daemon.rs`.)*

### L2. Application index (engine, 3–4 days)

- `troweld/src/apps/desktop_entry.rs`: Desktop Entry spec parser that handles localized
  `Name[xx]` (use `LANG`), `Keywords`, `Actions`, `TryExec` (hide when missing), `Terminal`,
  `NoDisplay`, `Hidden`, `OnlyShowIn`/`NotShowIn`, and `Exec` field-code expansion. Desktop ids
  follow the spec: path relative to `applications/`, with `/` → `-`.
- Scan order and dedup are exactly `hidden-entries.sh`: `~/.local/share/applications`, then each
  `XDG_DATA_DIRS`, then `~/.nix-profile`. Hidden ids are Omarchy's `launcher.hides`, Trowel's
  `~/.config/trowel/launcher.hides`, and computed hides.
- `apps/rank.rs`: port `AppSearch.js` (`fuzzyScore`, `wordText`, `entryAcronym`,
  `allTermsMatch`), citing `/usr/share/omarchy/shell/services/AppSearch.js:<line>`. Frecency is
  applied **only as a tiebreak inside the same score band** (for example, two prefix matches) and
  for empty-query order. A typed exact prefix always wins, which keeps Omarchy muscle memory.
  Frecency score: `Σ launches · 0.5^(age_days/14)`. *(Superseded 2026-10-22 by "Learning and pop to
  root" in L4: D19 amended.)*
- `apps/icons.rs`: the same scan as `AppLibrary.iconIndexScanCommand` (apps/devices, SVG before
  PNG, pixmaps), done once in Rust and refreshed on icon-dir inotify with a 750 ms debounce.
  Resolve the current GTK icon theme first if `gsettings get org.gnome.desktop.interface icon-theme`
  is set (Omarchy sets it per theme). Verify this in L0.
- `notify` watches on every applications dir, plus Omarchy's `launcher.hides`.
- CLI: `trowel apps [query] [--json]` and `trowel launch <id> [--action a]`.

**Tests:** a fixture tree of `.desktop` files (webapp, flatpak export, `OnlyShowIn=GNOME`,
`TryExec` missing, actions, localized names, a duplicate id in user and system dirs).
**Ranking parity gate**: `scripts/apps-parity.sh` runs Omarchy's own `AppSearch.js` under `node`
and our Rust ranker over the **installed** entries for 200 queries (every name prefix of length
1–4, acronyms, multi-word queries) and diffs the order with frecency disabled. **Zero diffs.**

**Acceptance:** the parity gate passes; `trowel apps fire` lists Firefox first in < 2 ms
(`--json` includes `took_us`); `trowel launch firefox` opens Firefox in `app-graphical.slice`;
installing a new `.desktop` file shows it within 1 s. *(This machine has no Firefox; the human's
browser is Chrome. Demonstrated with installed apps: `ghost` → Ghostty, `chro` → Chromium, and
`trowel launch omacalc`.)*

**Found while building L2 (2026-10-15; the code follows these, `troweld/src/apps/`):**
- **The entry set is Quickshell's, not `hidden-entries.sh`'s.** Omarchy's list is
  `DesktopEntries.applications` (`AppLibrary.qml:53`), so the parser and scanner port Quickshell
  0.3.1 `src/core/desktopentry.cpp` (read at tag `v0.3.1`): dirs are `$XDG_DATA_HOME/applications`
  then `XDG_DATA_DIRS` (no `~/.nix-profile`), read lowest priority first so the higher file wins;
  ids are the relative path with `/` → `-` and `completeBaseName`; `Hidden=true` masks the id; an
  invalid file (no `Name`, or `Type` ≠ `Application`) never replaces a valid one; keys and values
  are not trimmed; the locale is `LC_MESSAGES`, then `LANG`. `hidden-entries.sh` (which does scan
  `~/.nix-profile`, regular files only) is ported separately and only decides computed hides.
- **`TryExec` hides an entry Omarchy shows.** Quickshell ignores it, but GIO refuses to load such a
  file, so `gtk-launch` cannot start it (`Gio.DesktopAppInfo.new("dev.zed.Zed.desktop")` →
  NULL on this machine). Trowel hides it; Omarchy lists a dead entry.
- **Frecency tiers, not bands.** `fuzzyScore` branches are not monotonic in score (a late haystack
  hit can score below an acronym hit), so sorting by branch first would not reproduce Omarchy's
  order. The tier is `score > 9500` (every name-prefix match on a name under 500 UTF-16 units):
  frecency orders name-prefix matches among themselves and breaks exact score ties elsewhere.
  With no launches the order is exactly Omarchy's.
- **Frecency is stored as a decayed sum** (`apps_launches.score`, `state.sqlite` schema 2): each
  launch decays the old sum to now and adds 1, which is `Σ 0.5^(age_days/14)` exactly with one row
  per app.
- **Icons are rebuilt when the app list changes** (as `AppLibrary` does with its 750 ms debounce),
  not on icon-dir inotify: `/usr/share/icons` has ~2,400 dirs. One walk per base, pruning
  contexts that never hold app icons, takes 100–180 ms (two unpruned walks took 0.7–2.4 s).
- **Watch before the icon walk.** The first version created the watcher after building icons and
  missed `.desktop` installs in that window (the daemon test caught it). The watcher starts first,
  then a rescan, then icons; a change swaps the entry list at once (~10 ms) and icons follow.
- **`Exec` of desktop actions** is split exactly like Quickshell's `parseExecString`: field codes
  dropped, `%` literal inside quotes. Actions run under `uwsm-app` (in `xdg-terminal-exec` when
  `Terminal=true`); the app itself always goes through `gtk-launch <id>.desktop`, never a shell
  (ids contain spaces and quotes, e.g. `No Man's Sky`).
- `trowel launch` asks the daemon (so the launch counts); when no daemon with apps answers, it
  launches in-process and says it wasn't recorded.
- Parity gate: `scripts/apps-parity.sh` (+ `apps-parity.mjs`) generates ~550 queries from the
  installed entries; a deliberate mutation of the inner-match penalty produced 34 diffs, so the gate
  is sensitive.

### L3. Clipboard engine (4–5 days)

- **Capture**: the daemon supervises `wl-paste --watch trowel clip-capture` (restart with backoff,
  `PR_SET_PDEATHSIG`, a `doctor` row). `clip-capture` reads `wl-paste --list-types`, then:
  1. Skips the entry when the env has `CLIPBOARD_STATE=sensitive`, the offer includes
     `x-kde-passwordManagerHint`, or the source app is excluded (default exclusions are the
     1Password and Bitwarden classes). The source app is Hyprland's `activewindow` class from
     `hyprctl -j activewindow`, read at capture time.
  2. Fetches in priority order: files (`text/uri-list` / `x-special/gnome-copied-files`), then
     image (`image/png|jpeg|webp|gif|bmp|tiff`, keeping the original bytes and MIME), then
     `text/html` **plus** `text/plain`, then text. Each fetch has a `timeout 2s`, and the total is
     capped at 64 MiB per entry (`clip_max_bytes`).
  3. Sends one `clip_ingest` request (an internal op on the socket, refused for non-helper peers)
     carrying the spool-file paths. The **daemon remains the only writer** of `state.sqlite`.
- **Classify** (in Rust, unit-tested): `link` (one URL, possibly with surrounding whitespace),
  `color` (`#rgb[a]`, `#rrggbb[aa]`, `rgb()/hsl()/oklch()`), `code` (≥ 2 lines plus a symbol density
  heuristic, or source app ∈ terminals/editors), `files`, `image`, `html`, `text`.
- **Dedup**: content hash per entry. Re-copying an existing entry bumps it to the top (`last_used`,
  `uses += 1`) instead of duplicating it, which matches Omarchy's `addEntry` semantics. Ignore
  self-originated copies within 1 s of a `clip_paste`.
- **Retention** (config `[clipboard]`):
  - `max_entries = 1000`
  - `max_age_days = 30`
  - `max_blob_mb = 512`
  - Pinned entries are never evicted.
  - Blob GC is **refcounted**, so the 145 MB orphan problem that Omarchy's store has cannot happen.
  - The GC runs on ingest and hourly.
- **Thumbnails**: reuse the `previews.rs` / libvips path to write `~/.cache/trowel/clip/<id>@{64,132,264}.png`
  off-thread. The first paint shows the kind icon, as file rows do (UI-DESIGN §5).
- **Restore** (`clip_paste`):
  - text: `wl-copy` (text/plain);
  - html: its text, `wl-copy` (text/plain), in every mode (see the HTML restore note below);
  - image: `wl-copy --type <mime> < blob`;
  - files: `wl-copy --type text/uri-list` (L0.3: one offer pastes into Chromium, Nautilus and Strata with the real file name; no multi-type offer needed).

  Paste mode then sleeps 150 ms (L0.4: 48/48 landings at ≥ 25 ms across Ghostty, Chromium, GTK4, Qt6; the panel layer unmaps ≤ 162 ms after `toggle`) and sends the `wtype` Shift+Insert.
- **Import**: `trowel clip import-omarchy` (also offered by setup) converts the JSON order into
  `created` timestamps (newest first, 1 s apart, since the real times are lost) and copies, rather
  than moves, the referenced images. It never deletes Omarchy's files.
- **Privacy**: `clip_pause` (incognito), `trowel clip clear`, `trowel uninstall --purge` removing
  `state.sqlite` and blobs, and the doctor reports blob bytes. Nothing leaves the machine.
- CLI: `trowel clip list|get|paste|pin|delete|clear|pause|import-omarchy`.

**Tests:** classifier tables, dedup/bump, retention and refcount GC (including a crash between blob
write and row insert, which leaves no orphan after the next GC), capture with stub `wl-paste`
binaries on `PATH` (the CI has no compositor), and the sensitive skip. **Live test on this machine**
(`scripts/clip-live.sh`): `wl-copy` text, PNG, a Nautilus file copy and HTML, then assert each
appears in `trowel clip list --json` within **300 ms**, and that `clip paste --mode copy` round-trips
byte-identical (`wl-paste | sha256sum`).

**Acceptance:** the live script passes; 1000 entries query in < 5 ms; an 8 MB 4K screenshot
ingests in < 150 ms (thumbnail excluded); 24 h of normal use leaves zero orphan blobs.

**Found while building L3 (2026-10-15; the code follows these, `troweld/src/clip/`):**

- **Capture plumbing.** The daemon passes `TROWEL_SOCKET`, `TROWEL_CLIP_SPOOL`
  (`$XDG_RUNTIME_DIR/trowel/clip-spool`, `0700`), `TROWEL_CLIP_MAX_BYTES` and
  `TROWEL_CLIP_EXCLUDE` to `wl-paste --watch`, which hands them to `trowel clip-capture`. Pausing
  (incognito) or `capture = false` **stops the watcher**, so nothing is even read. `clip_ingest` is
  accepted only from a process whose parent (or grandparent) is the watcher's pid, and only with
  files in the spool directory. `CLIPBOARD_STATE` `clear`/`nil` is skipped like `sensitive`.
- **What is fetched** follows the L0.2 offers: files (`text/uri-list`, then
  `x-special/gnome-copied-files`; a list with no `file://` URI falls back to text), else the first
  image type (Chrome "Copy Image" also offers HTML: the image wins, as in Omarchy), else
  `text/html` with `chromium/x-source-url` (the page's domain) and the best text type
  (`text/plain;charset=utf-8` › `UTF8_STRING` › `text/plain` › `STRING` › `TEXT`), else text. An HTML
  copy whose text is one URL is stored as a `link`.
- **Recency is a `seq` column** (state schema 3): `last_used` is whole seconds, so a re-copy within
  the same second could not move to the top. `seq` orders the list and the count/blob eviction;
  `last_used` still drives the age limit.
- **Text over 256 KiB** keeps its full copy in a `txt` blob (refcounted like images); the row holds
  the first 256 KiB for search and preview, and paste restores the whole text.
- **Classifier:** `code` needs ≥ 2 non-empty lines and either a terminal/editor window class or at
  least half the lines looking like code (statement endings, operators, keywords, indented
  symbols). Colors are normalized to `#RRGGBB[AA]` (`rgb()`, `hsl()` and `oklch()` converted, CSS
  Color 4 math) so the UI can draw a swatch.
- **Default excluded apps:** `1password`, `bitwarden`, `com.bitwarden.desktop`, `keepassxc`,
  `org.keepassxc.keepassxc` (L0.5: no real copy carried a hint, so the class list is the main guard).
- **`clip_import` op added.** The plan named `trowel clip import-omarchy` but no op, and the daemon
  is the only writer of `state.sqlite`, so the CLI asks the daemon. Existing entries are skipped,
  not bumped.
- **HTML restore (resolved 2026-10-22, human's bug report).** `wl-copy --type text/html` also offers
  the same bytes as `text/plain`, `UTF8_STRING`, … (L0.6), so restoring an `html` entry as HTML
  pasted the **markup** into terminals and plain fields. Every mode now restores the entry's text.
  Rich (formatted) paste would need a multi-type offer of `text/html` plus real `text/plain`
  (`wl-clipboard-rs`, not approved).
- A bare `wl-copy` (no `--type`) spends ~150 ms sniffing the MIME type with `xdg-mime` before it
  owns the selection. The daemon always passes `--type`; the live script does too.

### L2c. Calculator engine (2 days, after L1, in parallel with L2/L3)

- `troweld/src/calc.rs` wraps `fend-core`, with one `Context` per daemon. Settings:
  - **Interrupt after 20 ms**, so a pathological input such as `10^10^10` returns nothing instead
    of blocking the connection.
  - Precision capped at 16 significant digits for display. `copy` holds the full value.
  - Currency handler unset, so currency units are "unknown" and produce no card.
  - Locale grouping and decimal separator from `LC_NUMERIC` for `display` only. Input always
    accepts `.` as the decimal point (Raycast does the same).
- **Gate for "is this a calculation?"** False positives are the main UX risk: typing `firefox` or
  `2024 report` must never show a card. Show a card only when **all** of these hold:
  1. The input contains an operator (`+ - * × / ÷ ^ %`), a function call, or a conversion word
     (`to`, `in`, `as`), or it is a bare number in another base (`0x…`, `0b…`).
  2. It evaluates without error inside the time budget.
  3. The result's text differs from the input.

  The `=` view sets `force` and skips rule 1.

  **Reject rules from the L0.7 spike** (applied before evaluation; each fixed a real false
  positive or stall):
  - inputs containing `://`, or starting with `/` or `~`: URLs evaluate to lambdas (`\https.(…)`);
  - an ISO date `dddd-dd-dd`: `key:2026-06-30` evaluates as subtraction;
  - an ID token (letters, `-`, then a digit, with no spaces, where the left part is not a numeric
    literal): `M-417` evaluates as mega − 417. `1e3-5` still evaluates;
  - dice tokens (`d` + digit at a word start, e.g. `5d66`). fend computes dice distributions
    **without checking the interrupt**: `a-5d66-4b5c` took 67 ms against a 20 ms deadline;
  - inputs with no digit and no known constant or function name.

  Results containing `\` (function values) are dropped.
- **Rewrites before evaluation**:
  - Raycast percent: fend reads `200 + 15%` as `200.15`, so a trailing `A ± B%` is rewritten to
    `(A) * (1 ± B/100)`, which gives 230. `15% of 80` = 12 already works natively.
  - A bare base literal (`0b1010`, `0xff`) gets ` to decimal` appended. Otherwise fend echoes it in
    the same base, and the preview filter hides it.
- **API**: use `fend_core::evaluate_preview_with_interrupt(input, &ctx, &deadline)`. It clones the
  context (no variable leakage between keystrokes), disables RNG, and already drops output that is
  empty, over 50 chars, multi-line or equal to the input. An `approx. ` prefix maps to
  `approx: true`.
- **Hard deadline**: because the interrupt is not honored everywhere (dice), `calc` runs on a
  dedicated daemon thread. The connection gets its reply (no card) at 20 ms regardless, and an
  overrunning evaluation is abandoned, which is safe because preview mode works on a context
  clone. While one is still running, new `calc` requests return no card instead of queueing.
- **Pretty print**: `*` → `×`, `/` → `÷`, and thin-space grouping in `pretty_expr`.
- **History**: append to `calc_history` in `state.sqlite` when a result is copied or pasted.
  Keystrokes are never recorded.
- CLI: `trowel calc "<expr>" [--json]`, which is also the test harness.

**Tests:**
- An expression table of about 150 rows with expected `copy` strings: arithmetic, precedence,
  `%` semantics (`200 + 15%` = 230, `15% of 80` = 12), units, bases, functions, large and small
  magnitudes, and locale grouping.
- A **false-positive corpus** must produce **zero** cards. It contains every installed app name,
  500 file names from the testbed, all 250 text entries from the Omarchy clipboard import (run
  locally only, not committed), and the eval queries from `evals/`.
- A timeout test: `10^10^10` and deep nesting return within 25 ms with no result.
- A fuzz test: random strings never panic.

**Acceptance:** the table and the corpus are green; `trowel calc "5 ft to cm" --json` returns
`152.4 cm` with `took_us` < 1000; `calc` p95 < 1 ms over the table.

**Found while building L2c (2026-10-15; the code follows these, `troweld/src/calc.rs`):**
- **Crash guard.** fend's parser overflows a 2 MiB thread stack at ~300 nested parentheses and
  *aborts the process* (not catchable). The gate now refuses input over **256 chars** or nested
  deeper than **32** brackets, and the fend thread has a **16 MiB** stack (8× margin).
  `deep_input_never_crashes` sweeps seven recursive shapes up to depth 1000.
- **Warm-up.** fend builds unit tables on first use (~0.5–1 ms). `Calculator::new` evaluates one
  conversion before serving, so the first keystroke doesn't pay it; `trowel calc --json` reports
  it separately as `warmup_us`. Without it the one-shot acceptance command measured 0.9–1.5 ms.
- **Display precision.** The 16-significant-digit cap applies to fractional results only;
  integers are never rounded (`2^64` shows all 20 digits, grouped). fend's 10-place padding of
  approximations (`314.1592653590`) is trimmed in both `copy` and `display`.
- **`copy` of a unit result is the number** (`152.4`, unit in `unit`), matching L4's `Enter` →
  `152.4`. Mixed units (`1 ft 3 in`) copy as shown. `alts` are Hex and Binary for non-negative
  integers that fit in 64 bits, none when the user asked for a base.
- **Percent rewrite recurses** (`100 + 10% + 10%` = 121); `0X…` is lower-cased for fend; `1e-5` is
  a number, not an ID token; a bare `1e20` has no operator, so rule 1 shows no card.
- **Locale.** `LC_ALL` → `LC_NUMERIC` → `LANG`, mapped to glibc's separators for the common
  languages (`de` `2.760,5`, `fr` `2 760,5` with U+202F); anything else reads as English.
- **`calc_copy`** answers `io` when `wl-copy` fails (no compositor), and records history only after
  a successful copy. `plain` behaves like `paste`. The paste keystroke uses Omarchy's 150 ms until
  L0.4 measures it.
- Harness: `tests/calc/table.tsv` (146 rows), `tests/calc/false-positives.txt`,
  `trowel calc --bench FILE` / `--corpus FILE [--quiet]`, and `scripts/calc-corpus.sh` for this
  machine's apps, testbed file names and Omarchy clipboard (local only, clipboard text never
  printed).

### L4. UI (5–6 days)

- `Client.qml`: `calc`, `appsQuery`, `clipQuery`, `clipGet`, `clipPaste` and the others, with separate
  `gen` counters per source, so a slow file `search` never drops a fresh `apps_query`.
- **Merged list model**: the `rowType` set gains `app` and `clip`. Sections are inserted ahead of
  file groups by a pure function in `ListMerge.js`. It concatenates and does not sort; ranking stays
  in the daemon. Selection stability (UI-DESIGN §3.3) extends naturally: the row id is `app:<id>`,
  `clip:<id>`, or the path.
- **Calculator card**: `CalcCard.qml`. It spans the full 780 px width above the two columns: the
  expression on the left and the result on the right, at the query-field size (`base × 1.67`,
  `tnum`). It appears and disappears with the existing row-enter motion, and it never shifts the
  query field (the panel grows downward, UI-DESIGN §3.1). When it is present it is **row 0 and
  selected**. `↓` moves into the list below. After a copy, a "Copied" toast shows and the panel
  closes.
- **Default selection**: row 0. When a strong app match (score band ≥ name-prefix) exists it is row
  0, so typing `fire⏎` launches Firefox, which is the Raycast feel. Otherwise row 0 is the first
  file result, as today.
- **New components** in `ui/qml/Trowel/`: `AppRow.qml` (32 px icon, name, generic name, action-count
  badge), `ClipRow.qml` (kind glyph or thumbnail, preview, `source_app · 2 m ago`, pin glyph),
  `ActionPanel.qml` (popover with keycaps and fuzzy filter), `KindChips.qml` (clipboard filter) and
  `Toast.qml` (undo delete).
- **Preview column per kind**:
  - app: big icon, comment, path, launch stats and actions;
  - clip text and code: monospace with wrap;
  - link: domain, full URL, and "Open in browser" (`omarchy-launch-browser`);
  - color: swatch plus hex, rgb and hsl copy chips;
  - image: fit, with dimensions;
  - files: a list with thumbnails from the existing `thumbnail` op;
  - html: rendered text, with a "has formatting" pill.
- **Clipboard view**: the same 780 px panel. The list column shows rows, and **Images** filter
  switches the list to a 3-column thumbnail grid (Raycast does this). Paste flow: `dismiss()` →
  `clip_paste{mode}` → the daemon pastes after the panel is unmapped.
- ~~**Bar widget**: a clipboard-paused indicator and a toggle in the popup.~~ Built in L4 slice 2,
  then removed with the bar widget itself (D25); pause lives in `Ctrl+K` and Settings.
- **Settings**: a new tab, "Launcher & Clipboard". It holds the clipboard on/off switch, retention,
  excluded apps (picked from the running Hyprland clients), import from Omarchy, the
  Super+Ctrl+V takeover (P-D23) and the hidden-apps list.
- **Motion**: the existing `Motion.qml` tokens only. Section insertion uses the same row-enter
  transition as file groups. Nothing new to tune.
- IPC: `omarchy-shell trowel clipboard` (opens straight into the clipboard view) and
  `omarchy-shell trowel apps`.
- Add `ui/gallery` entries for every new component × 4 themes × 3 scales (UI-DESIGN §9).

**Acceptance:**
- The UI-DESIGN §5 budgets still hold, measured with `trowel debug ui-trace` extended with
  `apps_query` and `clip_query`. Apps paint in the frame after the keystroke.
- Typing `5 ft to cm` shows `152.4 cm` in the frame after the response, and `Enter` puts `152.4`
  on the clipboard (verified with `wl-paste`).
- `fire⏎` launches Firefox from a cold panel in < 150 ms (keystroke to `gtk-launch` exec).
- Clipboard paste works into Alacritty and Chromium.
- **P4 evals re-run green with identical numbers** (proof file search did not move).
- `cargo test`, `qmllint` and clippy are clean.

**Found while building L4, slice 1 (2026-10-16; the code follows these):**

- **Where Applications go.** "Sections ahead of file groups" and "row 0 is the first file unless an
  app is strong" conflict: the default selection would sit below a section. `ListMerge.js` puts
  Applications **first only when the top app's name starts with the query** (Omarchy's name-prefix
  band, checked in the UI from `AppRow.name`); otherwise the section follows the file groups, ahead
  of the weaker fold. At most 5 apps. Row 0 is then always the default selection.
- **The clipboard is its own view, not a section of the main list** (Raycast keeps clipboard history
  out of root search too). The main list is calculator + apps + files.
- **Keys** (UI-DESIGN §3.4): calculator `↵` copy, `Ctrl+↵` paste; app `↵` launch; clip `↵` paste,
  `Ctrl+↵` copy only, `Shift+↵` paste as text, `Ctrl+Shift+P` pin, `Ctrl+Delete` delete; `Tab`
  cycles the clipboard chips. A copy shows a toast and closes the panel 450 ms later.
- **Plain rows next to the ListModel.** ListModel turns nested objects and arrays into models, so the
  previews read app and clip details from `Client.rowIndex` / `clipIndex` (plain JS by row id).
- **Bugs found on the way (existing):** a non-UTF-8 path's `{b64, display}` id broke the ListModel's
  `rowId` role as soon as a string id came first (now `"b64:<…>"`, and open/reveal/copy send the
  original object back); `GroupHeader { title: title }` and `WeakerFold { count: count }` bound the
  properties to themselves, so group headers were blank and the fold count 0.
- **Offscreen harness** (`ui/harness/`): Qt's offscreen platform has no layer-shell backend, so
  `prep.py` copies the plugin's Panel with a `FloatingWindow` in place of `PanelWindow` and test
  hooks, stubs `qs.Commons`, and `check.py` asserts selection and rows against `troweld --mock`. It
  never touches the live session.

**Found while building L4, slice 2 (2026-10-16):**

- **Action panel is `Ctrl+K`** (Raycast's ⌘K): open, desktop actions and "Hide from the launcher"
  for an app; paste/copy/plain/open/pin/delete/clear/pause for a clip; copy, paste and the `alts`
  for the calculator; open/reveal/Quick Look/copy for a file. Fuzzy filter in `ListMerge.js`.
- **Images grid:** the Images chip shows the clipboard as a 3-column grid (rows without headers);
  `↑`/`↓` move by a row of three.
- **The pause state needed a home.** Nothing reported clipboard incognito to a client that didn't
  set it, so the bar widget couldn't show it. `status` gains `clip_paused_until` (absent while
  capturing; PROTOCOL §4). The bar shows a dot on its icon while paused, and its popup gains
  "Clipboard history…" (`summon` payload `{"view":"clipboard"}`) and a pause/resume entry.
- **Settings "Launcher & Clipboard"** (`trowel-app`): capture on/off, the three limits, excluded
  apps (chips, plus the classes of the windows open now from `hyprctl -j clients`), import from
  Omarchy, clear, and the hidden-apps list (read from `~/.config/trowel/launcher.hides`, "Show"
  sends `app_hide false`). The Super+Ctrl+V takeover stays in `trowel setup` (L5) because it edits
  the Hyprland config; the tab says so.
- `trowel debug ui-trace` probes `apps_query`, `calc` and `clip_query` when advertised, each with a
  one-frame (16 ms) budget.

**Commands (human, 2026-10-16):** the clipboard is reached Raycast-style, as a **command** in the
main search rather than only a hotkey. `ListMerge.COMMANDS` is the registry (id, name, subtitle,
keywords, glyph); a command matches on a prefix of its name, a name word or a keyword, shows in a
"Commands" section (first when the name or a keyword starts with the query, ahead of apps), and
Enter runs it inside the panel. Clipboard History is the first; the L6 "Omarchy commands" extra
adds entries here. Inside a command the field shows a crumb pill and `Esc` / `Backspace` on an empty
field return to the search (query and selection restored).

**Learning and pop to root (human, 2026-10-22; D19 amended).** Raycast-style, ported from oma-cast
(`typecraft-dev/oma-cast` `90efedd`):
- `troweld/src/learn.rs` ports `Frecency.js` (`score`, `visit`, `recordVisit`: λ = ln 2/10 per day,
  +100 per open, 20 terms, 17-day term window) and `Ranker.js` `searchTermMatch` (exact, prefix,
  overbounds 1–3). `state.sqlite` v4 adds `learned(item, anchor, opened_at, terms)`, keyed
  `app:<id>` / `cmd:<id>`, seeded from `apps_launches` (score at `last` = 1 + 100·the old sum).
- `apps/rank.rs` sorts with the parts of `compareMatched` that apply to apps: an exact name, then
  remembered terms (exact, prefix, 3 extra characters), each tied by frecency, then Omarchy's order
  with frecency in the name-prefix tier. No signals = Omarchy's order (`scripts/apps-parity.sh`).
- Protocol: `app_launch.query`, `command_run`, `apps_query.commands` and `AppRow.learned` (PROTOCOL
  §4). `ListMerge.js` orders the commands by their signals, treats a learned query as a strong match,
  and when both launcher sections lead, puts the one learned for the query (then the more frecent)
  first. So `c⏎` opens Chrome once the user keeps picking Chrome for `c`.
- Pop to root: `ui.pop_to_root_seconds` (default 90; Raycast's choices 0/5/15/30/60/90/120/180 in
  Settings → Launcher & Clipboard). Reopening within it keeps the view (search or clipboard), the
  query and the selection, with fresh results; after it the panel opens blank on the All tab.
- Tests: `learn.rs` (oma-cast's frecency cases), `rank.rs`, `state.rs` (v4 seed, round trip),
  `tests/daemon.rs` (learned order end to end), `v2-apps.jsonl`, `listmerge-test.mjs`, and three
  harness checks (`ui/harness/check.py`: kept within 90 s, blank after).

### L5. Omarchy integration and hardening (2–3 days)

- `trowel setup`: the opt-in prompts from P-D18 and P-D23, each with a backup and recorded in the
  uninstall manifest; `--no-prompt` defaults to "no".
- `trowel doctor` rows:
  - the clipboard watcher is alive;
  - `wl-paste` and `wtype` are present;
  - the compositor supports data-control;
  - `uwsm-app` is present;
  - blob store size;
  - the app index count and the last scan.
- ROBUSTNESS rows:
  - F-C1: watcher dies → restart with backoff, and the status shows `clipboard: down`.
  - F-C2: a huge copy (1 GB) → capped and skipped with a reason.
  - F-C3: the disk is full → capture pauses (reuses F9).
  - F-C4: `state.sqlite` is corrupt → quarantined and started fresh, keeping pins where readable.
  - F-C5: a `.desktop` file has a parse error → skipped and logged; the rest of the list survives.
- Add fault-injection tests for each row (TESTING §7 style), and include the units in the PKGBUILD.

**Acceptance:** each new ROBUSTNESS row has a passing automated test. A fresh-account
`makepkg -si` → `trowel setup` → Super+D launches an app, pastes a clipboard image and finds a file.

### L6. Raycast extras (backlog, each its own small PR after L5; prioritize with the human)

| Extra | Omarchy-native source | Notes |
|---|---|---|
| **Omarchy commands** | `default/omarchy/omarchy-menu.jsonc` + `~/.config/omarchy/extensions/omarchy-menu.jsonc` (333 labels: themes, toggles, screenshots, power) | Strongest Omarchy fit. Reuse the same `when:`/`action:` semantics. Commands section sits between Apps and Clipboard. |
| Window switcher | `hyprctl -j clients` + `hyprctl dispatch focuswindow` | `w ` prefix; preview from `grim -T` is optional. |
| Emoji | the `omarchy.emojis` plugin's `emojis.json` | `:` prefix; paste through the clip paste path. |
| Clipboard OCR | the existing tesseract extractor | Makes screenshots text-searchable in `clip_fts`. Gated by the power policy. |
| Clipboard semantic search | the existing query encoder plus worker | Embed clipboard images and text **in `state.sqlite` only**, ranked by plain cosine plus FTS. It must **not** reuse or alter `Searcher` math. Needs its own small eval. |
| Snippets / quicklinks | `~/.config/trowel/snippets.toml` | Text expansion with `{clipboard}` / `{date}` placeholders. |

---

## 5. Budgets (to be added to `BENCHMARKS.md` on approval)

| Path | Target |
|---|---|
| `calc`, daemon side | p95 < 1 ms, hard stop at 20 ms |
| `apps_query`, daemon side, 200 entries | p95 < 2 ms |
| `clip_query`, 1000 entries, FTS + filter | p95 < 5 ms |
| Copy → visible in `clip list` | < 300 ms |
| Ingest 8 MB PNG (hash + write) | < 150 ms |
| Keystroke → app row painted | next frame after response (≤ 16.7 ms) |
| `Enter` on app → `gtk-launch` exec | < 50 ms |
| Daemon idle RSS increase from apps + clipboard | ≤ 15 MB |

## 6. Effort and order

L0 2d → L1 2d → L2 4d ∥ L2c 2d ∥ L3 5d (engine) with L4 starting on the mock after L1 → L4 6d → L5 3d.
About **3 weeks for one engineer**, or about 2 weeks with the UI track running in parallel on
`--mock`. A Raycast-feel MVP (calculator, apps, and text/image clipboard in the merged list) is reachable at
the end of week 2.

## 7. Answers (human, 2026-10-13: "accept all recommendations")

1. P-D18 through P-D24 are approved as D18–D24, including D21's reversal of the "the daemon never
   touches the clipboard" rule.
2. Binds: stay on Super+D. Setup offers the Super+Ctrl+V takeover only, defaulting to no.
   Super+Space is not offered.
3. `wl-clipboard-rs`: not needed (L0.3, 2026-10-15).
4. Clipboard retention: 1000 entries / 30 days / 512 MB, as proposed.
5. `fend-core` is approved. The calculator is **offline**, with no currency (the L6 currency row is
   dropped).
6. L6 order: Omarchy commands → window switcher → clipboard OCR, then the rest.
