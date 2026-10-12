# Progress

Agents: update this at the end of every session (see `CLAUDE.md`). Newest session first.

## Current state

- **Phase:** 0 acceptance demonstrated (vector parity both backends + panel blur/focus/IME)
- **Branch:** `phase-0-runtime`
- **Next step:** Open/land the Phase 0 PR, then Phase 1 on `phase-1-core` (Database + IndexStore first). fcitx5 stays on `keyboard-us`.

## Gates

| Gate | Status | Evidence |
|---|---|---|
| P0 vector parity ≥ 0.999 (Vulkan) | ✅ | `./scripts/parity.sh vulkan` worst=`frame-70` cosine=**0.999975**. Full output in session log. |
| P0 vector parity ≥ 0.999 (CPU) | ✅ | `./scripts/parity.sh cpu` worst=`speech` cosine=**0.999740**. Full output in session log. |
| P0 panel spike (blur, focus, IME) | ✅ | Blur A/B + exclusive focus (earlier). Overlay IME: panel open → Mozc for ~1s → `wtype nihongo` → field shows **にほんご** with Mozc candidate popup (日本語 / 日本語と英語 / 日本語教育) over `trowel-panel`. Screenshot `/tmp/trowel-spike/panel-preedit.png`. English restored immediately (`keyboard-us`, DefaultIM unchanged, panel dismissed). |
| P1 unit tests + protocol frozen + `--mock` | ⬜ | |
| P2 extractors + traps | ⬜ | |
| P3 pipeline + crash/resume | ⬜ | |
| P4 evals meet TESTING §3 | ⬜ | |
| P5 daemon + robustness + doctor | ⬜ | |
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

## Open questions for the human

1. **Commit / PR?** Phase 0 acceptance is demonstrated. Source is still uncommitted on `phase-0-runtime`. Say if you want a commit (and a PR against `main`). I will not push `main`.
2. **Jira ticket?** Branch name has no key. Skip logging unless you give one.
3. **qmllint import path.** For CI, should we add `-I /usr/share/omarchy/shell` (needs that tree in the arch container) or keep plugin QML lint as a local-only check?

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

