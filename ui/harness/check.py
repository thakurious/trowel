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
    {"do": "type", "text": "fire", "wait": 700}, {"do": "actions", "wait": 300}, {"do": "log", "what": "app-actions"}, {"do": "shot", "name": "app-actions"},
    {"do": "type", "text": "5 ft to cm", "wait": 700}, {"do": "log", "what": "calc"}, {"do": "shot", "name": "calc"},
    {"do": "down", "wait": 200}, {"do": "log", "what": "calc-down"},
    {"do": "tab", "id": "doc", "wait": 600}, {"do": "log", "what": "doc-tab"},
    {"do": "clipboard", "wait": 700}, {"do": "log", "what": "clip"}, {"do": "shot", "name": "clip"},
    {"do": "down", "wait": 200}, {"do": "down", "wait": 200}, {"do": "down", "wait": 200}, {"do": "down", "wait": 500},
    {"do": "log", "what": "color"}, {"do": "shot", "name": "color"},
    {"do": "actions", "wait": 300}, {"do": "log", "what": "clip-actions"},
    {"do": "tab", "id": "image", "wait": 600}, {"do": "log", "what": "images"}, {"do": "shot", "name": "images"},
    {"do": "tab", "id": "link", "wait": 600}, {"do": "log", "what": "links"},
    {"do": "open", "wait": 600}, {"do": "log", "what": "back"},
    {"do": "open", "payload": {"view": "clipboard"}, "wait": 600}, {"do": "log", "what": "bar-summon"},
    {"do": "open", "wait": 400}, {"do": "tab", "id": "all", "wait": 300}, {"do": "type", "text": "clip", "wait": 700}, {"do": "log", "what": "cmd"}, {"do": "shot", "name": "command"},
    {"do": "enter", "wait": 700}, {"do": "log", "what": "cmd-open"}, {"do": "shot", "name": "command-clipboard"},
    {"do": "back", "wait": 600}, {"do": "log", "what": "cmd-back"},
    {"do": "type", "text": "notes", "wait": 700}, {"do": "log", "what": "drawn-files"},
    {"do": "command", "id": "clipboard", "wait": 800}, {"do": "log", "what": "drawn-clips"},
    # Pop to root (90 s by default): reopening soon keeps the view, query and selection.
    {"do": "down", "wait": 200}, {"do": "down", "wait": 300}, {"do": "log", "what": "pop-clip-before"},
    {"do": "close", "wait": 300}, {"do": "open", "wait": 700}, {"do": "log", "what": "pop-clip-kept"},
    {"do": "back", "wait": 600}, {"do": "type", "text": "fire", "wait": 700},
    {"do": "close", "wait": 300}, {"do": "open", "wait": 700}, {"do": "log", "what": "pop-search-kept"},
    {"do": "close", "wait": 300}, {"do": "age", "seconds": 91}, {"do": "open", "wait": 700}, {"do": "log", "what": "popped"},
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
    ("app actions: open, desktop actions, hide", lambda s: s["app-actions"]["actions"] == ["open", "action:new-window", "action:new-private-window", "hide"]),
    ("clip actions for a color", lambda s: s["clip-actions"]["actions"] == ["paste", "copy", "pin", "delete", "clear", "pause"]),
    ("Images chip: a grid without headers", lambda s: s["images"]["grid"] is True and s["images"]["rows"] == ["clip:808"] and s["images"]["selection"] == "clip:808"),
    ("Links chip filters", lambda s: s["links"]["rows"] == ["clip:811"] and s["links"]["selection"] == "clip:811"),
    ("opening again returns to search", lambda s: s["back"]["view"] == "search"),
    ("'clip' puts the Clipboard History command first and selects it", lambda s: s["cmd"]["rows"][:2] == ["hdr:commands", "cmd:clipboard"] and s["cmd"]["selection"] == "cmd:clipboard"),
    ("Enter on it opens the history inside the panel", lambda s: s["cmd-open"]["view"] == "clipboard" and s["cmd-open"]["fromSearch"] is True and s["cmd-open"]["selection"] == "clip:808"),
    ("Back returns to the search, query and selection restored", lambda s: s["cmd-back"]["view"] == "search" and s["cmd-back"]["query"] == "clip" and s["cmd-back"]["selection"] == "cmd:clipboard"),
    ("the search list draws file rows", lambda s: "notes\ufffd.txt" in s["drawn-files"]["drawn"]),
    ("after the switch the clipboard list draws clips, never files", lambda s: s["drawn-clips"]["drawn"][:2] == ["Screenshot 1920×1200", "Refund policy: items may be returned within 30 days"] and not any("notes" in d for d in s["drawn-clips"]["drawn"])),
    ("reopened within 90 s: the clipboard view, its selection kept", lambda s: s["pop-clip-kept"]["view"] == "clipboard" and s["pop-clip-kept"]["selection"] == s["pop-clip-before"]["selection"] and s["pop-clip-before"]["selection"] != "clip:808"),
    ("reopened within 90 s: the search and its selection kept", lambda s: s["pop-search-kept"]["view"] == "search" and s["pop-search-kept"]["query"] == "fire" and s["pop-search-kept"]["selection"] == "app:firefox"),
    ("reopened after 90 s: a blank search", lambda s: s["popped"]["view"] == "search" and s["popped"]["query"] == "" and s["popped"]["rows"] == []),
    ("the summon payload opens the clipboard view", lambda s: s["bar-summon"]["view"] == "clipboard" and s["bar-summon"]["selection"] == "clip:808"),
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
