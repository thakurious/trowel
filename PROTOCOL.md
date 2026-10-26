# `troweld` socket protocol (v2)

The contract between the daemon and every UI surface (Omarchy panel plugin, `trowel-app`, bar widget,
`trowel` CLI, test clients). **Freeze it at the end of Phase 1** so UI work can proceed against a mock
daemon while the engine is built (see `EXECUTION-PLAN.md`). Any later change must bump `protocol`.

**v2** (D22, Phase 8 L1) adds the applications, clipboard and calculator ops in §4 "Launcher". Every
v1 op is unchanged. Plugin, app and daemon ship in one package, so there is no cross-version
negotiation: a v1 client gets the restart message (§3).

Upstream reference: the app talks to its engine in-process (`Engine.swift`, `AppModel.swift`); this
protocol is the Linux replacement for that boundary. The *worker* protocol (`ARCHITECTURE.md` §A.7) and
the *index-service* protocol (`IndexService.swift`) are separate, internal, and stay byte-compatible
with upstream.

---

## 1. Transport

- Unix stream socket at `$XDG_RUNTIME_DIR/trowel/troweld.sock`. The directory is created `0700` and the
  socket `0600`. Reject peers whose `SO_PEERCRED` uid differs from the daemon's.
- Framing: one UTF-8 JSON object per line (`\n`). Max line **4 MiB**; a longer line closes the
  connection with `{"error":"too_large"}`.
- Many concurrent clients. Each connection is independent; the daemon never blocks on a slow reader.
  Each connection has a bounded outbound queue (256 messages). On overflow the daemon drops queued
  `status` events (only the newest one is kept) and never drops responses.
- Socket activation: `troweld.socket` (systemd) so the first client starts the daemon. A UI that
  can't connect within 2 s shows the "Trowel isn't running" state with a **Start** action
  (`systemctl --user start troweld`), never a spinner forever.

## 2. Envelope

```jsonc
// request (client → daemon)
{"id": 17, "op": "search", ...params}
// response (exactly one per request, same id)
{"id": 17, "ok": true, ...result}
{"id": 17, "ok": false, "error": {"code": "not_ready", "message": "Model is downloading", "retry_ms": 1000}}
// event (daemon → subscribed clients, no id)
{"event": "status", ...}
```

- `id` is a client-chosen positive integer, unique per connection.
- Error `code`s are a closed set: `bad_request`, `unknown_op`, `not_ready`, `cancelled`, `superseded`,
  `not_found`, `io`, `busy`, `internal`. UIs map codes to copy and never show raw `message`s except
  in a "Details" disclosure.
- Unknown fields are ignored (forward compatibility). Missing optional fields take the documented
  defaults.

## 3. Handshake

The first request on every connection:

```json
{"id":1,"op":"hello","protocol":2,"client":"panel","pid":1234}
→ {"id":1,"ok":true,"protocol":2,"daemon":"0.1.0","capabilities":["code","audio","video","ocr"],
   "state":"ready|onboarding|model_missing|model_downloading"}
```

The daemon speaks v2 and v3 (D34: v3 only adds ops). It answers `hello` with the client's own
version, so a v2 panel keeps working beside a v3 one. Any other version gets `ok:false,
code:"bad_request"` and the UI shows "Trowel was updated, restart it".

`capabilities` gains `apps`, `clipboard` and `calc` (D22) as the daemon serves them: `calc` since
L2c, `apps` since L2, `clipboard` since L3. `troweld --mock` advertises and serves all three from
canned fixtures.

## 4. Operations

### Query path (hot, per keystroke)

| op | params | result | notes |
|---|---|---|---|
| `panel_opened` | `{}` | `{}` | Same as upstream `prewarm`: starts the text worker if it's down and loads the vector matrix. Idempotent. |
| `typing` | `{text, code}` | `{}` | Embed-ahead. The daemon embeds the latest text in the background and caches it (`QueryVectorCache(256)`). It is fire-and-forget: if a newer `typing` arrives first, the older one is dropped. |
| `search` | `{text, mode:"keyword"\|"hybrid", code, kinds?, limit=60, prefix_last, gen}` | `{gen, groups, weaker, took_ms, meaning:"used"\|"skipped"\|"unavailable", counts?}` | `gen` is a client counter. `kinds` filters after scoring, so a filtered list ranks as the unfiltered one does; `counts` (`{screenshots, images, documents, video, audio, code}`, absent when empty) is the number of strong results per kind for the query **without** `kinds` — the UI's kind-tab counts (`SearchWindow.swift:311`). Added 2026-10-14; older clients ignore it. A new `search` **supersedes** all older in-flight searches from the same connection, which are answered `superseded`. The UI renders a response only if `gen` is its latest. `keyword` mode must never wait for the model. |
| `cancel` | `{target_id}` | `{}` | Best effort. |

Vectors never cross this socket. The UI sends text, and the daemon owns embedding, caching and
fusion. This replaces the earlier `embed_query`/`search{vector}` draft: shipping 768 floats to QML
would only add latency.

**Result row** (one shape for all surfaces; it mirrors `ResultRow` in `Results.swift`):

```jsonc
{
  "id": "/home/u/Documents/tax.pdf",       // path (see §6 for non-UTF-8)
  "name": "tax.pdf", "folder": "~/Documents",
  "kind": "pdf", "segment": "page", "segment_id": 812,
  "loc": 3, "loc_end": null, "focus": null,
  "place": "p. 3",                          // computed by the daemon: "0:06", "♪ 0:25", "L12–40"
  "excerpt": "… the «refund» was …",        // « » marks matched words
  "matched_meaning": true, "matched_words": false,
  "reason": "Similar meaning",              // computed by the daemon from upstream `reason`
  "modified": 1760000000, "size": 182233,
  "duration": null, "info": {},
  "thumb": "thumb://812@132"                // resolve with `thumbnail` (§4 Previews)
}
```

Groups: `{"kind":"screenshots","title":"Screenshots","rows":[...],"total":N}` in upstream order.
`weaker` is the rows folded by `strongCount`.

### Previews & thumbnails (all generated off the UI thread by the daemon)

| op | params | result |
|---|---|---|
| `thumbnail` | `{segment_id or path, size, code?}` | `{file}` is a PNG/JPEG path in `$XDG_CACHE_HOME/trowel/thumbs/` (sizes 64/132/264/528). For whole files, check the freedesktop thumbnail cache (`~/.cache/thumbnails/{normal,large,x-large}`) first. `code:true` looks up the segment in `code.sqlite` (ids are per-store). |
| `preview` | `{segment_id, width, code?}` | `{type:"image"\|"pdf_page"\|"code"\|"text"\|"audio"\|"video", file?, band?:[y0,y1], lines?:[{n,text}], focus_line?, text?, peaks?:[...], frame?, start?, end?, duration?}` For a PDF page, `band` is the matched band in page coordinates 0–1. For audio, `peaks` holds about 400 normalized values for the waveform. `code:true` uses the code index; segment ids do not cross `index.sqlite` / `code.sqlite`. |
| `text_of_segment` | `{segment_id, code?}` | `{text, original?}` |

The daemon caches previews (LRU, 256 MB on disk under `$XDG_CACHE_HOME/trowel/previews`) and serves
repeat requests in under 5 ms.

### Actions

| op | params | effect |
|---|---|---|
| `open` | `{segment_id\|path, code?, loc?}` | Opens at the matched place: `evince -p` / `zathura -P` for PDF pages, `mpv --start=` for media, the editor at the line for code, otherwise `xdg-open`. Neovim (and Helix) run in `xdg-terminal-exec`. If the chosen editor is missing, the next installed one, then `xdg-open`. See `PORT-MAP.md` §D `CodeEditors`. |
| `reveal` | `{path}` | `org.freedesktop.FileManager1.ShowItems`, falling back to opening the parent folder. |
| `copy_path` | `{path}` | Returns `{text}`. The daemon also copies the file to the clipboard with `wl-copy` (D21): `image/*` for pictures, `text/uri-list` for everything else. |
| `quick_look` | `{segment_id}` | Asks `trowel-app` to open the large preview at the place, starting the app if needed. |

### Indexing control & settings

| op | params | result |
|---|---|---|
| `subscribe` | `{topics:["status","folders","model"]}` | Sends a snapshot event immediately, then a stream of changes. |
| `status` | `{}` | One `status` snapshot (same shape as the event). |
| `pause` / `resume` | `{}` | |
| `sync` | `{code?:bool}` | Forces a reconcile. |
| `config_get` | `{}` | The full config (§5). |
| `config_set` | `{patch}` | Validates, writes `config.toml` atomically, applies, and returns the new config. Folder changes restart the helper the way upstream `setFolders` does. |
| `estimate` | `{paths:[...], code?:bool}` | `{per_path:[{path, files, by_kind, est_seconds, est_bytes, skips:[...]}]}`, streamed per path as `estimate` events. |
| `model_status` | `{}` | `{present, verified, bytes, total, downloading, error?}` |
| `model_download` | `{action:"start"\|"cancel"}` | Sends progress as `model` events. |
| `doctor` | `{}` | The same report as `trowel doctor --json` (see `ROBUSTNESS.md`). |
| `set_hotkey` | `{chord}` | Writes the Hyprland bind (see `UI-DESIGN.md` §8). Returns `{conflict?: "Toggle top bar"}`. |

### Launcher (v2)

`LAUNCHER-PLAN.md` §3 is the design; this is the contract. Request params that name an app or a
clipboard entry are `app_id` / `clip_id`, because a bare `id` would collide with the envelope's
request `id` (the op's params are flattened into the envelope).

| op | params | result |
|---|---|---|
| `apps_query` | `{text="", limit=8, gen, commands?:[id]}` | `{gen, rows:[AppRow], took_us?, commands?:[{id, frecency, term?}]}`. An empty `text` returns frecency order. Rows are ranked with what was learned (D19 as amended): an app opened before with this query (`term` `exact`/`prefix`) comes first. `commands` echoes the requested built-in command ids that have been run, with the same signal for `text`, so the UI can rank its command section. Supersedes the connection's older `apps_query`. |
| `app_launch` | `{app_id, action?, query?}` | `{}`. Runs `uwsm-app -- gtk-launch <id>.desktop` (actions: their `Exec`, split like Quickshell), records a launch, and learns `query` (what was typed when the app was picked) as a search term. `not_found` for an unknown app or action, `io` when the launcher can't start. |
| `command_run` | `{command_id, query?}` | `{}`. A built-in command ran from the search: learned like an app launch (`cmd:<id>`). Any id is accepted. |
| `app_hide` | `{app_id, hidden}` | `{}`. Writes `~/.config/trowel/launcher.hides`. |
| `clip_query` | `{text="", kinds?:[ClipKind], limit=50, offset=0, gen}` | `{gen, rows:[ClipRow], total}`, pinned first, then most recently copied or pasted. `text`: every word must match, as a case-insensitive substring of the preview or the first 2 KiB of the content (`creation` finds `AccountCreation`, `#F2A`, `://`), or as a word prefix anywhere (FTS, the index's tokenizer). An unknown kind is `bad_request`. `limit` ≤ 500. |
| `clip_get` | `{clip_id}` | `{row, text?, html?, files?:[path], image?:{file,w,h}, truncated?}`. Text and HTML are capped at 256 KiB. `not_found` for an unknown id. |
| `clip_paste` | `{clip_id, mode="paste"\|"copy"\|"plain"}` | `{}`. The UI hides the panel first (D21). Restores the stored type: text as `text/plain`, HTML as `text/html` (`plain`: its text), an image with its MIME, files as one `text/uri-list` (`plain`: the paths as text). `paste`/`plain` then send Shift+Insert after 150 ms. Moves the entry to the top. `not_found` for an unknown id or when the image or every file is gone, `io` when `wl-copy` fails. The daemon ignores captures for 1 s after it, so its own copy is not stored again. |
| `clip_pin` | `{clip_id, pinned}` | `{}`, or `not_found`. |
| `clip_delete` | `{clip_id}` | `{}`, or `not_found`. |
| `clip_clear` | `{keep_pinned=true}` | `{removed}` |
| `clip_pause` | `{minutes?}` | `{paused_until}`: a Unix time, `0` = capturing, `-1` = until resumed. `minutes:0` resumes, absent pauses until resumed. |
| `clip_ingest` | `{types:[mime], offers:[{mime, file}], source_app?}` | Internal: only `trowel clip-capture` run by the daemon's own `wl-paste --watch` (checked by peer pid ancestry) may send it, and only with files in `$XDG_RUNTIME_DIR/trowel/clip-spool/`; anything else is `bad_request`. Returns `{clip_id, new}`, or `{skipped:true}` (paused, excluded app, sensitive, empty). The daemon deletes the spool files. |
| `clip_import` | `{source="omarchy"}` | `{imported, skipped}`. Copies `~/.local/state/omarchy/clipboard-history.json` and its images in (newest first, 1 s apart); entries already stored are skipped, Omarchy's files are left alone. Another `source` is `bad_request`. |
| `calc` | `{text, force=false, gen}` | `{gen, result?: CalcResult, error?, took_us?}`. No `result` means "not a calculation" (no card). `error` only with `force`. Supersedes the connection's older `calc`. |
| `calc_history` | `{limit=20}` | `{rows:[{expr, result, at}]}`, newest first. |
| `calc_copy` | `{text, expr?, mode="paste"\|"copy"}` | `{}`, or `io` when the clipboard can't be written (no compositor). Same clipboard path as `clip_paste`; `expr` is added to `calc_history` only after a successful copy. |
| *event* `clipboard` | — | Topic `"clipboard"` on `subscribe`: `{added?:ClipRow, removed?:[clip_id]}`, coalesced ≤ 4/s. |

Optional fields are **absent** when empty, never `null`. `ClipKind` is the closed set `text`,
`link`, `color`, `code`, `image`, `files`, `html`. `CalcResult.kind` is `number`, `unit`, `base`,
`date` or `duration`.

```jsonc
// AppRow
{"id":"firefox","name":"Firefox","generic":"Web Browser","comment":"…",
 "icon":"/usr/share/icons/hicolor/scalable/apps/firefox.svg", "icon_name":"firefox",
 "terminal":false, "actions":[{"id":"new-private-window","name":"New Private Window"}],
 "source":"/usr/share/applications/firefox.desktop",
 "launches":14, "last_launched":1791869000, "matched":[[0,4]],   // char ranges in name, [start,end)
 "learned":{"frecency":101.0, "term":"exact"}}  // absent if never opened; frecency ≥ 1 (a first
   // open scores 101, halving every 10 days, +100 per open); term: exact | prefix | overbounds

// ClipRow
{"id":812, "kind":"text", "preview":"firewall rules for…", "mimes":["text/html","text/plain"],
 "bytes":1824, "thumb":"/home/u/.cache/trowel/clip/812@132.png", "files":2, "color":"#F2A22A",
 "domain":"github.com", "source_app":"firefox",
 "created":1791869354, "last_used":1791869354, "uses":3, "pinned":false}

// CalcResult
{"expr":"1920*1.25 + 15%", "pretty_expr":"1920 × 1.25 + 15%", "display":"2,760", "copy":"2760",
 "kind":"number", "unit":"cm", "approx":false, "alts":[{"label":"Hex","copy":"0xac8"}]}
```

### Desk (v3, Phase 9, `PHASE9-PLAN.md`)

Island, process monitor, system commands, timers and snippets. Types are `trowel_proto::desk`.
Fields named after their object (`activity_id`, `timer_id`, `snippet_id`, `command_id`): a bare `id`
would collide with the envelope. An op whose part isn't built yet answers `not_ready`.
`capabilities` gains `island`, `proc`, `system`, `timers`, `snippets` as each lands.

| Op | Params | Reply / events |
|---|---|---|
| `island_post` | `{activity}` | `{}`; `island` event `{activities}` to subscribers |
| `island_dismiss` | `{activity_id}` | `{}` |
| `island_action` | `{activity_id, action}` | `{}`; handled by the producer |
| `island_list` | — | `{activities}` |
| `proc_subscribe` | `{interval_ms=1000, sort, group, all_users}` | `{}`; `proc` events `{totals, rows}` |
| `proc_unsubscribe` | — | `{}` |
| `proc_detail` | `{pid}` | cmdline, user, start, PSS, 60-sample history |
| `proc_kill` | `{pids, signal: term|kill, confirm?}` | `{}` / `not_allowed` (P-D30) |
| `system_commands` | — | `{commands:[SystemCommand]}` |
| `system_run` | `{command_id}` | `{}` |
| `timer_start` | `{text}` or `{seconds, label}` | `Timer` |
| `timer_parse` | `{text}` | `TimerSpec` or `null` |
| `timer_list` | — | `{timers}` |
| `timer_pause` / `timer_resume` / `timer_cancel` | `{timer_id}` | `Timer` / `{}` |
| `timer_add` | `{timer_id, seconds}` | `Timer` |
| `snippet_query` | `{text, limit=50}` | `{rows:[Snippet]}` |
| `snippet_get` / `snippet_delete` | `{snippet_id}` | `Snippet` / `{}` |
| `snippet_save` | `{snippet}` (`id` 0 creates) | `Snippet` |
| `snippet_paste` | `{snippet_id, mode, args}` | `{}` (D21 paste path) |
| `snippet_import` / `snippet_export` | `{path}` | `{imported, skipped}` / `{exported}` |

Topics for `subscribe`: `island`, `timers`.

### `status` event

```jsonc
{"event":"status",
 "phase":"idle|syncing|indexing|finishing_long_files|waiting_for_model|paused|error",
 "paused_reason": null,            // "user" | "battery" | "thermal" | "power_saver"
 "main": {"done":120,"total":900,"current":"~/Pictures/x.png"},
 "code": {"done":0,"total":0},
 "missing_roots": [], "unreadable_roots": [],
 "counts": {"screenshots":120,"images":800,"documents":44,"video":3,"audio":9,"code":0},
 "worker": "down|starting|ready", "vectors_loaded": true,
 "last_error": null,
 "clip_paused_until": -1}         // Phase 8 L4; absent while the clipboard is captured. -1 = paused until resumed,
                                  // else a Unix time (in the past = capturing again)
```

The daemon coalesces status events to at most **4 per second**, enough for a smooth progress ring
without flooding QML.

## 5. Config (`~/.config/trowel/config.toml`)

It mirrors `AppSettings.swift` field-for-field, and the daemon is the only writer:

```toml
roots = ["~/Desktop", "~/Documents", "~/Downloads", "~/Pictures"]
excluded_folders = []
excluded_types = []
index_dir = "~/.local/share/trowel/index"
models_dir = "~/.local/share/trowel/models"
hotkey = "SUPER + D"
index_on_battery = false
encoder_idle_seconds = 600
onboarded = false
code_roots = []
code_excluded = []
code_editor = ""          # "" = auto/xdg-open
code_quiet_seconds = 2
index_speed = "gentle"    # D28: "gentle" | "balanced" | "full"; absent = gentle. Paces the GPU,
                          # caps CPU and keeps indexing on efficiency cores; a change restarts the helper
[ui]
reduce_motion = false     # also follows the system preference
pop_to_root_seconds = 90  # the panel reopens where it was left this long after closing; 0 = blank
[clipboard]               # Phase 8 L3; absent in older files = these defaults
capture = true            # run the wl-paste --watch capture
max_entries = 1000        # pinned entries are never evicted
max_age_days = 30
max_blob_mb = 512
max_entry_mb = 64         # larger copies are not stored
excluded_apps = ["1password", "bitwarden", "com.bitwarden.desktop", "keepassxc", "org.keepassxc.keepassxc"]
                          # Hyprland window classes, case-insensitive
```

The daemon watches `config.toml`. A hand edit is validated: an invalid file is logged, the previous
valid config is kept, and `status.last_error` explains the problem.

## 6. Paths that aren't UTF-8

Linux paths are bytes; JSON strings are not. Any path that isn't valid UTF-8 is sent as
`{"b64": "<base64 of raw bytes>", "display": "<lossy UTF-8>"}` wherever a path appears. The UI shows
`display` and echoes the object back unchanged. The same rule applies in SQLite: see `PORT-MAP.md`
§Linux deltas.

## 7. Conformance

- `crates/trowel-proto` holds the serde types, and `ui/qml/Trowel/Client.qml` is the only QML code
  that speaks this protocol.
- `tests/proto/*.jsonl` holds golden request/response transcripts (v1 ops and `v2-*.jsonl`),
  replayed against `troweld --mock` (canned results and synthetic latency) in CI. The real daemon's
  handshake and op behavior is covered by `crates/troweld/tests/daemon.rs`, because its results
  depend on a real index rather than fixtures.
- `troweld --mock` is a Phase-1 deliverable. It is what lets UI work start in week 2.
