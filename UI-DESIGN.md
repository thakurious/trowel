# UI design & motion spec

Goal: **feel like the macOS DigUp** (calm, dense, keyboard-first, instant) while **looking native on
Omarchy**: it follows the active Omarchy theme, fonts and corner rounding instead of imitating Aqua.
Every number below comes from the upstream SwiftUI source (`Sources/DigUpApp/*`) unless it is marked
**[Linux]**. If this doc and the Swift source disagree, the source wins; fix this doc.

Verified environment: omarchy-shell (Quickshell 0.3.1, Qt 6.11) running from
`/usr/share/omarchy/shell`, Hyprland 0.56 with a **Lua** config, eDP-1 1920×1200 @ 60 Hz,
**scale 1.25**, fcitx5 installed.

---

## 1. Surfaces and where they live

| Surface | Host | Why |
|---|---|---|
| **Quick panel** (upstream `SearchPanel`) | Omarchy shell **plugin** `trowel` (kind `panel`, `keepLoaded: true`), namespace `trowel-panel` | Summoning a panel is an IPC call into the already-running shell, so the open is instant. It gets `Color`/`Style` theme tokens for free and behaves like Omarchy's own launcher, clipboard and emoji overlays. |
| **Bar status** (upstream `StatusMenu`) | Same plugin, kind `bar-widget` | Placed through `omarchy bar move`, with a popup that uses the kit's `PopupCard`. |
| **Main window, Quick Look, Settings, Onboarding, Model download** | `trowel-app`, a **separate Quickshell instance** (`quickshell -p /usr/share/trowel/app`) using `FloatingWindow` | Image grids, video playback (QtMultimedia) and long forms are heavy. Keeping them out of omarchy-shell means a Trowel bug can never take down the user's bar, notifications or **lock screen** (which run in that process). It uses the same QML components, so both look identical. |
| Non-Omarchy fallback | `quickshell -p /usr/share/trowel/panel` runs the panel plugin's QML standalone, with a theme shim | So the panel still works on plain Hyprland or Sway. This is secondary; do not let it constrain the Omarchy design. |

Shared code: `ui/qml/Trowel/` is a QML module with `Client.qml` (the protocol client), `Theme.qml`,
`Motion.qml`, and the components in §6. It is installed once and imported by both hosts. Quickshell
0.3.1's qmlscanner will not register types from a path **outside** the `-p` config folder (relative
`import "../qml/Trowel"` fails with `Client is not a type`). Each host therefore has a `Trowel`
symlink to `../qml/Trowel` and uses `import "Trowel"`. The packaged layout can still install the
module under `/usr/share/trowel/qml` once Phase 7 sets an import path.

> Rule: **QML never does work.** No file I/O, no image decoding without `asynchronous: true`, no
> sorting or filtering in JS, no `XMLHttpRequest`. The UI renders what `troweld` sends (PROTOCOL.md).

## 2. Theme: follow Omarchy, keep DigUp's character

### 2.1 Token mapping

Inside the shell, bind to `Color.*` and `Style.*` from `/usr/share/omarchy/shell/Commons`. In
`trowel-app`, `Theme.qml` reproduces them by reading
`~/.local/state/omarchy/current/theme/{colors.toml,shell.toml}` with `FileView { watchChanges: true }`.
That way a theme switch (`SUPER+SHIFT+CTRL+SPACE`) recolors both live, with **no restart**.

| Role | macOS source | Omarchy token |
|---|---|---|
| Panel background | `windowBackgroundColor` @ **0.75** + `.popover` material | `Color.background` @ 0.75 under Hyprland layer blur. Panel-specific values from `shell.toml` take precedence if the theme defines them. |
| Primary text | `.primary` | `Color.foreground` |
| Secondary text | `.secondary` | `Color.foreground` @ 0.62, or `Color.muted` if the theme sets it |
| Hairline / keycap stroke | `primary @ 0.14`, 0.5 pt | `Color.foreground` @ 0.14, **1 physical px** (`1/Screen.devicePixelRatio`) |
| Keycap fill | `primary @ 0.07` | `Color.foreground` @ 0.07 |
| Selected row | `accentColor @ 0.24`, radius 8 | `Color.accent` @ `Style.selectedFillAlpha` (0.18 default). Use 0.24 only if it reads too faint across the test themes (§9). |
| Badge capsule (place, counts) | `primary @ 0.07`; on images `black @ 0.6` | same |
| Code-mode chips | `Brand.leaf @ 0.12` | `Color.accent` @ 0.12 |
| Corner radius (panel) | **18** | `max(Style.cornerRadius, 0)` — Omarchy themes may set rounding to 0, which must look intentional, not broken. Inner radii scale proportionally: row 8 → `round(r·0.45)`, thumbnail 8/6. |

**Brand colors** (`Components.swift`: amber `#F2A22A`, ember `#D97B1A`, moss `#356652`,
leaf `#4F8058`, sprout `#A2C78E`) are used **only** for the app icon, the onboarding hero and the
empty-state illustration. Everywhere else, the Omarchy accent drives the UI. This is the main
deliberate departure from macOS, and it is what makes the app feel at home on Omarchy.

### 2.2 Typography

macOS uses SF Pro at fixed point sizes. Map them to Omarchy's font (`Style.font.*`, rem root
`[font] base-size`) by **ratio**, so a user who enlarges the shell font enlarges Trowel too:

| Element | macOS pt / weight | Token |
|---|---|---|
| Panel query field | **20 semibold** (17 in code mode) | `base × 1.67` / `1.42`, DemiBold |
| Row title | 13 semibold | `base × 1.08`, DemiBold |
| Row subtitle, excerpt, footer hints | 11 | `base × 0.92` |
| Group header | 10 semibold, uppercase, tracking | `base × 0.83`, DemiBold, `font.letterSpacing: 0.6` |
| Place badge / counts | 11 medium, **monospaced digits** | `base × 0.92`, Medium, `font.features: {"tnum": 1}` |
| Empty-state title | 15 semibold | `base × 1.25` |
| Empty-state icon | 26–30 light | `base × 2.3` |

Code excerpts and code paths use the theme's monospace font (Omarchy defaults to a Nerd Font, which
also provides glyph icons).

### 2.3 Icons

Kind icons (screenshot, image, pdf, doc, audio, video, code, folder) ship as **monochrome SVGs** in
`ui/qml/Trowel/icons/`, tinted with `Color.foreground` through `MultiEffect.colorization`. Do not rely
on the Nerd Font for kind icons, because themes change fonts. Thumbnails always replace icons once
they load (§5).

## 3. Quick panel spec

### 3.1 Geometry (`SearchPanel.swift`, `SearchPanelView.swift`)

- Width **780** logical px; list column **330**, preview column the remaining **450**.
- Placement: horizontally centered on the **focused monitor** (`Hyprland.focusedMonitor`). The
  **top edge sits 16 % of the usable height below the top** (`area.maxY - area.height * 0.16`) and
  the panel grows **downward only**, so the query field never moves while results arrive.
- Field row: 18 px horizontal padding; the mode icon in a 24 px column (magnifier, or `</>` in code
  mode); the result-count capsule on the right (`11 medium tnum`, padding 8×3).
- Rows: padding 8×5, radius 8, thumbnail square (`side > 60 ? 8 : 6` radius).
- Group headers: padding 10 top, 3 bottom, 10 horizontal.
- Footer hint bar: `11`, 16 px horizontal padding, keycaps (§6).
- Max height **[Linux]**: `min(560, 0.6 × monitor height)`. The list scrolls inside it.

At scale 1.25 every one of these is a fractional physical size. Set
`QT_SCALE_FACTOR_ROUNDING_POLICY=PassThrough` (Quickshell's default on fractional-scale-v1) and snap
**hairlines and 1 px separators** to physical pixels. Never snap text or radii.

### 3.2 Layer-shell & Hyprland

```qml
PanelWindow {
  WlrLayershell.namespace: "trowel-panel"
  WlrLayershell.layer: WlrLayer.Overlay
  WlrLayershell.keyboardFocus: visible ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None
  exclusionMode: ExclusionMode.Ignore
  color: "transparent"
  // anchors: top + left + right on the focused screen; content positioned per 3.1
}
```

Ship `/usr/share/trowel/hypr/trowel.lua` and source it from the user's Hyprland config the same way
`omarchy-shell.lua` is sourced:

```lua
-- verify keys against Hyprland 0.56's Lua layer_rule API before shipping
hl.layer_rule({ match = { namespace = "^trowel-(panel|popup)$" }, no_anim = true, animation = "none",
                blur = true, ignore_alpha = 0.3 })
```

- `no_anim`: Omarchy disables compositor layer animations for its own panels. Trowel does the same and
  owns all motion in QML (§4), so the two can never double-animate.
- `blur` + `ignore_alpha`: gives the macOS "vibrancy" behind the 0.75 background, and makes the blur
  follow the QML rounded corners instead of the rectangular surface.
- The panel's outer `Item` uses `layer.enabled: false`. Rounded corners come from a `Rectangle`
  with `radius` and `clip: true` on a **single** container, never from nested `OpacityMask`s, which
  are expensive at 780 px.

### 3.3 Behavior (must match upstream)

- **Two-phase search**: each keystroke sends `typing` (embed-ahead) and `search{mode:"keyword",
  prefix_last:true}` immediately. After a **120 ms** typing pause (`SearchPanel.typingPause`) it
  sends `search{mode:"hybrid"}`. Keyword results must paint in the **next frame** after their
  response arrives.
- **Stale-proof rendering**: each `search` carries `gen`, and older generations are discarded
  (PROTOCOL.md §4). Without this, results flicker on fast typing.
- **Result-set swaps don't jump**: when new results arrive, keep the selection on the **same path**
  if it's still present, otherwise on row 0. Keep the scroll position unless the selection changed.
- Groups in upstream order, then a "**N weaker matches**" disclosure row (collapsed by default).
- A `code:` prefix (or `⇥` on an empty field) switches to code mode: the icon morphs, the field font
  drops to 17-equivalent, and the empty state shows example chips.
- Empty states (copy verbatim from `SearchPanelView.swift`): "No folders chosen yet", "No good
  matches", "No good matches in your code", "Code search is off", "Search your code".
- **Preview column** shows the selected row's `preview` (image; PDF page with the matched band
  tinted `accent @ 0.18`; code lines with the focus line highlighted; passage; audio waveform with a
  moment marker; video frame with a moment pill).

### 3.4 Keyboard map

| Key | Action | Note |
|---|---|---|
| `Esc` | Close (clear the field first if it's non-empty, as upstream does) | |
| `↑` / `↓`, `Ctrl+P` / `Ctrl+N` | Move the selection; it wraps within groups and moves across them | |
| `Enter` | Open at the place | |
| `Ctrl+Enter` | Reveal in the file manager | ⌘ maps to Ctrl everywhere |
| `Space` | Quick Look at the place (`quick_look`). On the "weaker matches" row it toggles the fold instead. | Only when `navigated` (↑/↓ moved the selection since the last edit) **or** Quick Look is already open. Otherwise it types a space. Clicking back into the field resets `navigated` (`SearchPanel.swift:60,223,297`). |
| `Ctrl+Y` | Toggle Quick Look | upstream `⌘Y` |
| `Ctrl+C` | Copy the path (when there is no text selection in the field) | |
| `Ctrl+O` | Open the query in the main window | |
| `Tab` / `Shift+Tab` | Next / previous kind tab: All · Screenshots · Images · Documents · Video · Audio · Code. Skips kinds with no strong result for the query (All and Code always stop). Each tab re-runs the query with `search.kinds`; counts are the unfiltered query's strong results | Port of DigUp's window kind filters (`SearchWindow.swift:133`) into the panel; Code replaces the old code-mode toggle, and typing `code:` selects it |
| `Ctrl+1` … `Ctrl+7` | All, Screenshots, Images, Documents, Video, Audio, Code | DigUp ⌘1…⌘7 |

**Focus loss** hides the panel (upstream hides on `didResignActive`, Spotlight-style). Also hide
when another Trowel window comes forward. On Hyprland, watch `HyprlandFocusedMonitor` and
`activewindow` changes, and a click outside the panel (a full-screen transparent catcher on the
Overlay layer, the way Omarchy's menus do it).

**IME**: the field must honor `Qt.inputMethod` pre-edit: no search on pre-edit text, and `Enter`
commits the IME first. Test it with fcitx5 (installed) Bengali or Japanese input on the Overlay
layer with `Exclusive` focus, since text-input-v3 on layer-shell is the risky path.

## 4. Motion spec

Principle (macOS-like): **fast in, faster out; geometry eases, content cross-fades; nothing bounces
in a productivity tool.** All durations live in `Motion.qml`, and every one becomes 0 when
`reduce_motion` is on (config, or `gsettings org.gnome.desktop.interface enable-animations` false).

| Moment | Animation | Duration / curve |
|---|---|---|
| Panel open | opacity 0→1, scale 0.985→1 (origin: top-center), y −6→0 | **140 ms**, `Easing.OutCubic` |
| Panel close | opacity 1→0, scale →0.99 | **90 ms**, `Easing.InCubic`. Release keyboard focus at **start**. |
| Panel height change | `Behavior on height` | **160 ms**, `Easing.OutQuint`. The top edge stays fixed. |
| Selection move | `ListView.highlightMoveDuration` | **90 ms**, OutCubic. `highlightResizeDuration` 0. |
| Result-set swap | Whole list cross-fade old→new | **80 ms**. Rows **never** animate individually on refresh. |
| Preview change | Cross-fade | **120 ms**; a thumbnail-to-full-res swap is a 100 ms fade |
| Weaker-matches disclosure | Height + chevron rotation | 160 ms OutQuint |
| Hybrid results replacing keyword results | No motion beyond the swap cross-fade; a 2 px progress hairline under the field while hybrid is pending (indeterminate, 900 ms loop) | |
| Bar icon progress ring | `NumberAnimation` on the arc | 250 ms per update, linear |
| Main-window grid cell hover | fill alpha | 80 ms |

Never animate `width` of text-bearing items (it re-lays out text every frame). Animate `scale`,
`opacity` and `y`, which stay on the GPU.

## 5. Smoothness budget (acceptance criteria)

This display runs at 60 Hz, so the frame budget is **16.6 ms**. Measured with
`QSG_RENDER_TIMING=1` and the frame-time probe in `trowel debug ui-trace`:

| Interaction | Budget |
|---|---|
| Hotkey → first panel frame (warm, `keepLoaded`) | **≤ 50 ms** (the macOS bar is <150 ms; we can beat it because the window is pre-mounted) |
| Keystroke → keyword results painted | **≤ 40 ms** p95 (FTS ≤ 10 ms + IPC ≤ 2 ms + QML ≤ 16 ms) |
| Typing pause → hybrid results painted | ≤ 120 ms + 80 ms p95 |
| Frame time during open animation, scrolling, typing | p99 ≤ 16.6 ms, **0 dropped frames** in a 60-s scripted session |
| Thumbnail appears | ≤ 100 ms (cached), with an icon placeholder until then; **no layout shift**, since cell sizes are fixed |

How to hit it:

1. **Pre-mount**: `keepLoaded: true`, and toggle `visible` instead of creating the window per
   summon. On summon, focus the field synchronously in the same frame.
2. **Fixed row heights per row type** (`ListView` with `reuseItems: true`, `cacheBuffer` of about
   one screen). Never use `Repeater` for results.
3. **Images**: `asynchronous: true`, `sourceSize` set to the exact displayed physical size,
   `cache: true`. Thumbnails come pre-sized from the daemon (64/132/264/528), so the UI thread never
   decodes a 12 MP photo.
4. **Model updates by diff**: `Client.qml` applies results to a `ListModel` with
   insert/remove/move, never `clear()` followed by `append()`.
5. **Debounce status** at the daemon (≤ 4 Hz) and coalesce text property updates.
6. No `layer.enabled` effects on scrolling content. Use `MultiEffect` only on static decoration.
7. Profile with `QSG_RENDER_LOOP=threaded` (the default) and verify on battery with the
   `power-saver` profile, since Lunar Lake clocks down hard.

## 6. Component library (`ui/qml/Trowel/`)

Build these once, test them in a **gallery** (`quickshell -p ui/gallery`), and use them on every
surface. Omarchy has a `dev-gallery` plugin you can copy the pattern from.

| Component | macOS source | Notes |
|---|---|---|
| `Keycap` | `Components.swift` (size-relative padding 0.45/0.28, radius 0.42·size, fill 0.07, stroke 0.14) | Used in footer hints and the hotkey recorder |
| `PlaceBadge` | `Components.swift` (10 semibold tnum, capsule, 6×2; `black@0.6` on images) | |
| `MomentBar` | `Components.swift` (track `primary@0.12`, span ≥4 px, 9 px playhead) | Audio/video span within the duration |
| `ResultRow` | `SearchPanelView.swift` | Icon or thumbnail, title (LRM-prefixed), folder, excerpt with «» highlighted in DemiBold + accent underline, reason, place badge |
| `GroupHeader`, `WeakerFold` | `SearchPanelView.swift` | |
| `QueryField` | `SearchPanelView.swift` | IME-safe, with mode icon and count capsule |
| `PreviewPane` | `Previews.swift` | Image, PDF band, code, passage, waveform, video (video only in `trowel-app`) |
| `HotkeyRecorder` | `Components.swift` (13 pt, padding 12, radius 9, accent stroke while recording) | Writes through `set_hotkey` |
| `FolderRow` | `FolderChoices.swift` / `Estimates.swift` | Checkbox, path, live estimate (files · time · size), skip reasons |
| `ProgressRing` | `StatusMenu.swift` | Bar icon states: idle, progress, paused, error |
| `EmptyState`, `Banner` | various | |

Reuse Omarchy's kit (`/usr/share/omarchy/shell/Ui`: `Toggle`, `Button`, `TextField`,
`PopupCard`, `PanelSectionHeader`) **inside the shell plugin** for anything generic. `trowel-app`
cannot import those, so it carries look-alikes built on the same tokens.

## 7. `trowel-app` surfaces

- **Main window** (`SearchWindow.swift`): default **1040×700**, minimum **820×520**. Query field
  and kind filters (All/Screenshots/Images/Documents/Video/Audio/Code with live counts) as a
  segmented control; an adaptive grid (cell **176–230 px**, image box **132 px**), with a code list
  in code mode; a **324 px** preview pane; a status bar; and a home view with example queries when
  the field is empty. Hyprland floats it (window rule on class `trowel-app`: `float`, `center`,
  size 1040×700).
- **Quick Look** (replaces `QLPreviewPanel`): a borderless floating window, 80 % of the monitor,
  opening at the matched place (PDF page scrolled to the band, video seeked to the moment and
  paused, code scrolled to the focus line). `Space`/`Esc` close it and `←`/`→` move through results.
  Sushi is only a fallback for kinds we can't render.
- **Settings** (`SettingsWindow.swift`): tabs General / Folders / Code / Storage. Layout and
  meaning follow upstream; **user-visible product name is Trowel** (DECISIONS D17), not DigUp,
  and Linux wording replaces “this Mac” / macOS. Extract remaining copy into `ui/strings.json`. General covers the hotkey recorder, start at login (systemd
  enable), index on battery (`AppSettings.batteryTitle/Detail`) and about. Folders shows per-root
  status (missing/unreadable), Add, and Skip. Code covers folders, the editor picker and turning it
  off. Storage shows the model, index size, locations, "Delete index" and privacy copy.
- **Onboarding** (`Onboarding.swift`): welcome → folders (machine-adaptive candidates from the XDG
  user dirs, live estimates) → code opt-in → shortcut → ready (autostart, battery). It runs on first
  launch when `onboarded = false`. The model download starts in the background **at step 1** so it
  is usually done by step 5.
- **Model download**: progress, speed, ETA, resume after network loss, SHA-256 verify, retry. It is
  never modal over search once a model is present.

## 8. Hotkey

Upstream's default is `⇧⌘Space`. **On Omarchy, `SUPER+SHIFT+SPACE` is taken** ("Toggle top bar",
`/usr/share/omarchy/default/hypr/bindings/utilities.lua:16`), and so are `SUPER+SPACE`,
`SUPER+CTRL+SPACE` and `SUPER+ALT+SPACE`. Default to **`SUPER + D`** (free on this machine; "D" for "dig", a nod to
upstream DigUp), and let the user rebind it.

- Write the bind to a Trowel-owned file, `~/.config/hypr/trowel.lua`:
  `o.bind("SUPER + D", "Trowel search", "omarchy-shell trowel toggle")`. Source it from
  `~/.config/hypr/bindings.lua`, which is where Omarchy puts personal overrides. Install adds one
  `require`/`dofile` line idempotently, and uninstall removes it.
- Before writing, detect conflicts by parsing `omarchy menu keybindings --print`, and show the
  conflicting action's name in the recorder ("Already used by Toggle top bar").
- Reload with `hyprctl reload`. Never grab keys through evdev or ydotool.

## 9. Visual QA

- **Reference captures**: run the macOS app's `Snapshot.swift` / `DebugHooks` (or take manual
  screenshots) for the panel (empty, typing, results, code, no-matches), the main window, every
  Settings tab and every onboarding step. Store them in `design/reference/macos/`.
- **Theme matrix**: screenshot every surface under at least 4 Omarchy themes: one dark default, one
  light theme, one with `cornerRadius 0`, and one high-saturation accent. Check text contrast at
  ≥ 4.5:1 for body text and ≥ 3:1 for badges.
- **Scale matrix**: 1.0, 1.25 (this laptop) and 2.0.
- `trowel debug snapshot <surface> <state>` (the port of `Snapshot.swift`) renders headless
  through `grim -g` on a fixed layer and diffs against the last accepted image (pixel threshold 1 %).
