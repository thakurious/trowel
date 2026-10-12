# Trowel: rules for implementing agents

Trowel is a Linux/Omarchy port of upstream DigUp (macOS), pinned as the `upstream/` submodule. The
plan is complete; your job is to execute it. Read this file at the start of **every** session.

## Start of every session

1. `git submodule update --init` (upstream at `6faa29e`; it is read-only, never commit inside it).
2. Read `PROGRESS.md`: the current phase, the last session's handoff and open questions.
3. Read the sections of the plan docs that the current task references. The reading order is in
   `README.md`. Don't re-read everything each time, but never work from memory of a doc you
   haven't opened in this session.

## Sources of truth (in order)

1. **Upstream Swift source** in `upstream/` for behavior. Port by reading the file named in
   `PORT-MAP.md`, side by side. Cite `upstream/<path>:<line>` in comments where the port encodes a
   non-obvious upstream decision.
2. `PORT-MAP.md` §Fidelity invariants for search/index constants, `PROTOCOL.md` for the wire
   format, `UI-DESIGN.md` for UI numbers and motion, `ROBUSTNESS.md` for failure handling,
   `BENCHMARKS.md` for targets, and `TESTING.md` for gates.
3. If a plan doc contradicts upstream or reality, **fix the doc in the same commit** and explain why
   in the commit message. Never let code and docs silently diverge.

## Non-negotiables

- **Never change a fidelity invariant to make a test pass**: vectors/framing, the FTS tokenizer
  string, normalization, the search math constants, prompts, the fingerprint format, or the
  chunk/window parameters. If parity or evals fail, the port is wrong. Debug it.
- **Never weaken a gate**: the cosine ≥ 0.999 threshold, the eval targets, the reference files in
  `reference/` and `evals/`, or the performance budgets. Don't skip, `#[ignore]` or loosen a test to
  get green. If a gate can't be met, stop and report with evidence.
- **Verify, don't assume, APIs that move fast**: llama.cpp at `b11461` (`llama_batch_ext_*`,
  `llama_process`, mtmd), Quickshell 0.3.x, the Hyprland 0.56 **Lua** API (`hl.layer_rule` keys),
  and the omarchy-shell plugin contract (`/usr/share/omarchy/shell/README.md`,
  `services/PluginRegistry.qml`). Check the source or headers, then record what you found in
  `PROGRESS.md` §Verified facts.
- Follow phase order and gates in `EXECUTION-PLAN.md`. Don't start a phase whose gate is red. The
  UI track may run against `troweld --mock` once Phase 1 has frozen the protocol.
- Only add dependencies the docs name. Ask before adding anything else.

## Ask the human before

- changing any decision in `DECISIONS.md`, a fidelity invariant, or a gate;
- touching the user's live desktop config (`~/.config/hypr/*`, `~/.config/omarchy/shell.json`,
  enabled plugins). For spikes, use a dev plugin id (`io.github.thakurious.trowel-dev`), back the
  file up first (`<file>.bak.trowel-<timestamp>`), and record how to revert it in `PROGRESS.md`.
  Never touch the lock-screen plugin or anything that could leave the session unusable;
- deleting user data, or indexing anything outside `testbed/` until Phase 5 acceptance (the real
  home directory is for the Phase 7 soak only);
- pushing to `main`, force-pushing, or making the repo public.

## How to work

- **One phase per branch** (`phase-0-runtime`, `phase-1-core`, …), small commits, and a PR to `main`
  when the phase's acceptance is demonstrated. Commit messages say *what and why*.
- **Tests come with each task**, not afterwards. Port the matching upstream test file first.
- A task is **done** when its tests pass, CI is green, and the acceptance line in
  `EXECUTION-PLAN.md` is demonstrated, with the **command and its output pasted into
  `PROGRESS.md`**. "It compiles" and "it should work" are not done.
- Keep `cargo fmt`, `cargo clippy -D warnings`, and `qmllint` clean.
- Report honestly: if something is stubbed, partial or failing, say so in `PROGRESS.md` and in your
  reply. Never claim a gate passed without its output.

## End of every session

Update `PROGRESS.md`: what changed, gate status with evidence, verified facts learned, open
questions for the human, and the **exact next step**, so a fresh session can continue with no other
context.

## Machine

Intel Core Ultra 7 258V, Arc 140V (Vulkan via Mesa ANV), 30 GB RAM, Omarchy (Arch, Hyprland 0.56
with a Lua config, omarchy-shell = Quickshell 0.3.1, Qt 6.11), eDP-1 1920×1200@60 at scale 1.25,
fcitx5. Model files (~865 MB) go in `~/.local/share/trowel/models`.
