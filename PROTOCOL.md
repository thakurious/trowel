# `troweld` socket protocol (v1)

The contract between the daemon and every UI surface (Omarchy panel plugin, `trowel-app`, bar widget,
`trowel` CLI, test clients). **Freeze it at the end of Phase 1** so UI work can proceed against a mock
daemon while the engine is built (see `EXECUTION-PLAN.md`). Any later change must bump `protocol`.

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
{"id":1,"op":"hello","protocol":1,"client":"panel","pid":1234}
→ {"id":1,"ok":true,"protocol":1,"daemon":"0.1.0","capabilities":["code","audio","video","ocr"],
   "state":"ready|onboarding|model_missing|model_downloading"}
```

If the protocol versions differ, the daemon answers `ok:false, code:"bad_request"` and the UI shows
"Trowel was updated, restart it". Do not try to negotiate across versions.

## 4. Operations

### Query path (hot, per keystroke)

| op | params | result | notes |
|---|---|---|---|
| `panel_opened` | `{}` | `{}` | Same as upstream `prewarm`: starts the text worker if it's down and loads the vector matrix. Idempotent. |
| `typing` | `{text, code}` | `{}` | Embed-ahead. The daemon embeds the latest text in the background and caches it (`QueryVectorCache(256)`). It is fire-and-forget: if a newer `typing` arrives first, the older one is dropped. |
| `search` | `{text, mode:"keyword"\|"hybrid", code, kinds?, limit=60, prefix_last, gen}` | `{gen, groups, weaker, took_ms, meaning:"used"\|"skipped"\|"unavailable"}` | `gen` is a client counter. A new `search` **supersedes** all older in-flight searches from the same connection, which are answered `superseded`. The UI renders a response only if `gen` is its latest. `keyword` mode must never wait for the model. |
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
| `thumbnail` | `{segment_id or path, size}` | `{file}` is a PNG/JPEG path in `$XDG_CACHE_HOME/trowel/thumbs/` (sizes 64/132/264/528). For whole files, check the freedesktop thumbnail cache (`~/.cache/thumbnails/{normal,large,x-large}`) first. |
| `preview` | `{segment_id, width}` | `{type:"image"\|"pdf_page"\|"code"\|"text"\|"audio"\|"video", file?, band?:[y0,y1], lines?:[{n,text}], focus_line?, text?, peaks?:[...], frame?, start?, end?, duration?}` For a PDF page, `band` is the matched band in page coordinates 0–1. For audio, `peaks` holds about 400 normalized values for the waveform. |
| `text_of_segment` | `{segment_id}` | `{text, original?}` |

The daemon caches previews (LRU, 256 MB on disk under `$XDG_CACHE_HOME/trowel/previews`) and serves
repeat requests in under 5 ms.

### Actions

| op | params | effect |
|---|---|---|
| `open` | `{segment_id\|path}` | Opens at the matched place: `evince -p` / `zathura -P` for PDF pages, `mpv --start=` for media, the editor at the line for code, otherwise `xdg-open`. The daemon picks the command; see `PORT-MAP.md` §D `CodeEditors`. |
| `reveal` | `{path}` | `org.freedesktop.FileManager1.ShowItems`, falling back to opening the parent folder. |
| `copy_path` | `{path}` | Returns `{text}`. The UI copies it with `wl-copy`. (v1 rule. From v2 the daemon may write the clipboard for `clip_paste` / `calc_copy`, see DECISIONS D21.) |
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
 "last_error": null}
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
[ui]
reduce_motion = false     # also follows the system preference
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
- `tests/proto/*.jsonl` holds golden request/response transcripts, replayed against the real daemon
  and against `troweld --mock` (canned results and synthetic latency) in CI.
- `troweld --mock` is a Phase-1 deliverable. It is what lets UI work start in week 2.
