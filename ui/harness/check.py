#!/usr/bin/env python3
"""L4 panel checks on the offscreen harness against `troweld --mock` (build troweld first):
    python3 -I ui/harness/check.py
Asserts the selection and rows the plugin's Panel shows for scripted input; screenshots land in
ui/harness/out/ for a look."""
import json, pathlib, re, subprocess, sys

here = pathlib.Path(__file__).resolve().parent
steps = [
    {"do": "ready"}, {"do": "open"},
    {"do": "type", "text": "fire", "wait": 700}, {"do": "log", "what": "fire"}, {"do": "shot", "name": "fire"},
    {"do": "type", "text": "notes", "wait": 700}, {"do": "log", "what": "notes"}, {"do": "shot", "name": "notes"},
    {"do": "type", "text": "5 ft to cm", "wait": 700}, {"do": "log", "what": "calc"}, {"do": "shot", "name": "calc"},
    {"do": "down", "wait": 200}, {"do": "log", "what": "calc-down"},
    {"do": "tab", "id": "doc", "wait": 600}, {"do": "log", "what": "doc-tab"},
    {"do": "clipboard", "wait": 700}, {"do": "log", "what": "clip"}, {"do": "shot", "name": "clip"},
    {"do": "down", "wait": 200}, {"do": "down", "wait": 200}, {"do": "down", "wait": 200}, {"do": "down", "wait": 500},
    {"do": "log", "what": "color"}, {"do": "shot", "name": "color"},
    {"do": "tab", "id": "link", "wait": 600}, {"do": "log", "what": "links"},
    {"do": "open", "wait": 600}, {"do": "log", "what": "back"},
]
out = subprocess.run([str(here / "run.sh"), json.dumps(steps)], capture_output=True, text=True).stdout
states = {}
for line in out.splitlines():
    m = re.search(r"HARNESS (\S+) (\{.*\})", line)
    if m:
        states[m.group(1)] = json.loads(m.group(2))
noise = [l for l in out.splitlines() if "HARNESS" not in l and not re.search(r"Cannot open: file:///tmp/trowel-mock|undefined member", l)]

checks = [
    ("strong app match is row 0 and selected", lambda s: s["fire"]["selection"] == "app:firefox" and s["fire"]["rows"][:2] == ["hdr:apps", "app:firefox"]),
    ("weak app match follows the files", lambda s: s["notes"]["rows"] == ["hdr:documents", "b64:L3RtcC9ub3Rlc/8udHh0", "hdr:apps", "app:obsidian"] and s["notes"]["type"] == "row"),
    ("calculator card is selected", lambda s: s["calc"]["selection"] == "calc:" and s["calc"]["calc"] == "152.4 cm"),
    ("↓ leaves the card only when there is a row", lambda s: s["calc-down"]["selection"] == "calc:"),
    ("a kind tab drops the launcher sources", lambda s: s["doc-tab"]["calc"] is None and not any(r.startswith("app:") for r in s["doc-tab"]["rows"])),
    ("clipboard view: pinned first, first item selected", lambda s: s["clip"]["view"] == "clipboard" and s["clip"]["rows"][:2] == ["hdr:pinned", "clip:808"] and s["clip"]["selection"] == "clip:808"),
    ("↓ skips headers", lambda s: s["color"]["selection"] == "clip:810"),
    ("Links chip filters", lambda s: s["links"]["rows"] == ["clip:811"] and s["links"]["selection"] == "clip:811"),
    ("opening again returns to search", lambda s: s["back"]["view"] == "search"),
]
failed = 0
for name, check in checks:
    try:
        ok = check(states)
    except KeyError as e:
        ok = False
        name += f" (no state {e})"
    print(("ok   " if ok else "FAIL ") + name)
    failed += not ok
if noise:
    print("QML warnings/errors:\n  " + "\n  ".join(noise))
    failed += 1
if failed:
    print(json.dumps(states, indent=1))
sys.exit(1 if failed else 0)
