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
    # Escape leaves a "code:" search: the query, then the Code tab, then the panel.
    {"do": "type", "text": "code: parse", "wait": 600}, {"do": "log", "what": "code-on"},
    {"do": "esc", "wait": 300}, {"do": "log", "what": "code-esc1"},
    {"do": "esc", "wait": 300}, {"do": "log", "what": "code-esc2"},
    {"do": "esc", "wait": 300}, {"do": "open", "wait": 600}, {"do": "log", "what": "code-reopen"},
    # Settings in the panel (D26): the command, a switch from the list, a choice with ←, the pane.
    {"do": "type", "text": "settings", "wait": 600}, {"do": "log", "what": "set-cmd"},
    {"do": "enter", "wait": 800}, {"do": "log", "what": "set-open"}, {"do": "shot", "name": "settings"},
    {"do": "type", "text": "battery", "wait": 300}, {"do": "enter", "wait": 600}, {"do": "log", "what": "set-batt"},
    {"do": "type", "text": "remember", "wait": 300}, {"do": "call", "fn": "settingStep", "args": [-1], "wait": 600}, {"do": "log", "what": "set-pop"},
    {"do": "type", "text": "search folders", "wait": 300}, {"do": "call", "fn": "settingPrimary", "wait": 300}, {"do": "log", "what": "set-pane"},
    {"do": "call", "target": "detail", "fn": "removeAt", "args": [0], "wait": 200}, {"do": "log", "what": "set-armed"},
    {"do": "call", "target": "detail", "fn": "removeAt", "args": [0], "wait": 600}, {"do": "log", "what": "set-removed"}, {"do": "shot", "name": "settings-folders"},
    {"do": "call", "target": "detail", "fn": "done"}, {"do": "esc", "wait": 200}, {"do": "esc", "wait": 400}, {"do": "log", "what": "set-back"},
    # Lock Keyboard: never ↵ here, that would lock the real keyboard.
    {"do": "command", "id": "lock-keyboard", "wait": 600}, {"do": "log", "what": "lock-open"}, {"do": "shot", "name": "lock-keyboard"},
    {"do": "type", "text": "3m", "wait": 300}, {"do": "log", "what": "lock-typed"},
    {"do": "esc", "wait": 200}, {"do": "esc", "wait": 400}, {"do": "log", "what": "lock-back"},
    # Phase 9: system commands in the main list, a typed timer, a snippet keyword (mock: nothing runs).
    {"do": "type", "text": "night", "wait": 700}, {"do": "log", "what": "sys-night"},
    {"do": "type", "text": "shut", "wait": 700}, {"do": "enter", "wait": 300}, {"do": "log", "what": "sys-armed"}, {"do": "shot", "name": "sys-shutdown"},
    {"do": "enter", "wait": 500}, {"do": "log", "what": "sys-ran"},
    {"do": "open", "wait": 400}, {"do": "type", "text": "25m tea", "wait": 800}, {"do": "log", "what": "main-timer"}, {"do": "shot", "name": "main-timer"},
    {"do": "type", "text": ";sig", "wait": 800}, {"do": "log", "what": "main-snippet"},
    {"do": "type", "text": "", "wait": 200},
    # Timer view: presets and the two seeded timers, then a typed one.
    {"do": "command", "id": "timers", "wait": 800}, {"do": "log", "what": "timers"}, {"do": "shot", "name": "timers"},
    {"do": "type", "text": "10 tea", "wait": 600}, {"do": "log", "what": "timer-typed"},
    {"do": "esc", "wait": 200}, {"do": "esc", "wait": 400}, {"do": "log", "what": "timers-back"},
    # Process Monitor: CPU first, Tab to GPU, ↵ twice ends the busy app (the mock only forgets it).
    {"do": "command", "id": "procs", "wait": 2200}, {"do": "log", "what": "procs"}, {"do": "shot", "name": "procs"},
    {"do": "call", "fn": "cycleProcSort", "args": [1], "wait": 1600}, {"do": "log", "what": "procs-gpu"},
    {"do": "call", "fn": "cycleProcSort", "args": [-1], "wait": 1600},
    {"do": "enter", "wait": 300}, {"do": "log", "what": "procs-armed"},
    {"do": "enter", "wait": 2000}, {"do": "log", "what": "procs-killed"},
    {"do": "esc", "wait": 400}, {"do": "log", "what": "procs-back"},
    # Snippets: the seeded five; a snippet with a blank asks for it first.
    {"do": "command", "id": "snippets", "wait": 800}, {"do": "log", "what": "snippets"}, {"do": "shot", "name": "snippets"},
    {"do": "type", "text": "greet", "wait": 600}, {"do": "enter", "wait": 400}, {"do": "log", "what": "snippet-args"}, {"do": "shot", "name": "snippet-args"},
    {"do": "call", "target": "snippet", "fn": "escaped", "wait": 200}, {"do": "esc", "wait": 200}, {"do": "esc", "wait": 400},
    # Create Snippet saves through the daemon and lands in the list.
    {"do": "command", "id": "snippet-new", "wait": 600}, {"do": "log", "what": "snippet-new"}, {"do": "shot", "name": "snippet-new"},
    {"do": "call", "target": "editor", "fn": "load", "args": [{"name": "Address", "keyword": "!addr", "body": "1 Main St"}], "wait": 200},
    {"do": "call", "target": "editor", "fn": "submit", "wait": 1000}, {"do": "log", "what": "snippet-saved"},
    {"do": "esc", "wait": 200}, {"do": "esc", "wait": 400}, {"do": "open", "wait": 400},
    # Omarchy Commands (fixture scripts) and Pick Theme.
    {"do": "command", "id": "omarchy", "wait": 800}, {"do": "log", "what": "omarchy"},
    {"do": "esc", "wait": 400},
    {"do": "command", "id": "themes", "wait": 800}, {"do": "log", "what": "themes"}, {"do": "shot", "name": "themes"},
    {"do": "esc", "wait": 400}, {"do": "log", "what": "island-state"},
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
    ("'code:' picks the Code tab", lambda s: s["code-on"]["tab"] == "code" and s["code-on"]["codeMode"] is True),
    ("Esc clears the query first, staying on Code", lambda s: s["code-esc1"]["query"] == "" and s["code-esc1"]["tab"] == "code"),
    ("Esc again returns to All", lambda s: s["code-esc2"]["tab"] == "all" and s["code-esc2"]["codeMode"] is False),
    ("Esc closes; reopening soon is All, not Code", lambda s: s["code-reopen"]["tab"] == "all" and s["code-reopen"]["codeMode"] is False),
    ("reopened after 90 s: a blank search", lambda s: s["popped"]["view"] == "search" and s["popped"]["query"] == "" and s["popped"]["rows"] == []),
    ("the summon payload opens the clipboard view", lambda s: s["bar-summon"]["view"] == "clipboard" and s["bar-summon"]["selection"] == "clip:808"),
    ("'settings' finds the Trowel Settings command", lambda s: s["set-cmd"]["selection"] == "cmd:settings"),
    ("Enter on it opens Settings: every section, the shortcut first", lambda s: s["set-open"]["view"] == "settings" and s["set-open"]["selection"] == "set:hotkey" and "hdr:Index & Storage" in s["set-open"]["rows"]),
    ("Enter on a switch flips it from the list", lambda s: s["set-batt"]["setting"] == "battery" and s["set-batt"]["cfg"]["index_on_battery"] is True and not s["set-batt"]["detailFocus"]),
    ("← on a choice steps it down", lambda s: s["set-pop"]["cfg"]["ui"]["pop_to_root_seconds"] == 60 and s["set-pop"]["cfg"]["ui"]["reduce_motion"] is False),
    ("Enter on a folder list hands the keys to the pane", lambda s: s["set-pane"]["detailFocus"] is True and s["set-pane"]["items"][0] == "entry:~/Desktop" and s["set-pane"]["items"][-1] == "add"),
    ("⌫ once only arms; twice removes the folder", lambda s: "~/Desktop" in s["set-armed"]["cfg"]["roots"] and "~/Desktop" not in s["set-removed"]["cfg"]["roots"]),
    ("Esc from Settings goes back to the search it came from", lambda s: s["set-back"]["view"] == "search" and s["set-back"]["query"] == "settings"),
    ("Lock Keyboard lists the durations", lambda s: s["lock-open"]["view"] == "keylock" and s["lock-open"]["rows"][:3] == ["lock:15", "lock:30", "lock:60"] and s["lock-open"]["selection"] == "lock:15"),
    ("a typed duration is offered and selected", lambda s: s["lock-typed"]["rows"] == ["lock:180"] and s["lock-typed"]["selection"] == "lock:180"),
    ("Esc from Lock Keyboard goes back", lambda s: s["lock-back"]["view"] == "search"),
    ("'night' finds Toggle Night Light with its state", lambda s: s["sys-night"]["selection"] == "cmd:sys:night-light" and s["sys-night"]["accessory"] == "Off"),
    ("↵ on Shut Down only arms it", lambda s: s["sys-armed"]["selection"] == "cmd:sys:shut-down" and s["sys-armed"]["armed"] == "cmd:sys:shut-down" and s["sys-armed"]["opened"] is True),
    ("↵ again runs it and closes the panel", lambda s: s["sys-ran"]["opened"] is False and s["sys-ran"]["armed"] == ""),
    ("'25m tea' offers Start Timer first", lambda s: s["main-timer"]["rows"][:2] == ["hdr:commands", "cmd:timer-start"] and s["main-timer"]["selection"] == "cmd:timer-start"),
    ("a snippet keyword puts that snippet first", lambda s: s["main-snippet"]["rows"][:2] == ["hdr:snippets", "snip:1"] and s["main-snippet"]["selection"] == "snip:1"),
    ("Timer: presets, then the running and paused timers", lambda s: s["timers"]["view"] == "timers" and s["timers"]["desk"][0].startswith("tnew:1|") and "timer:1|Tea|" in "".join(s["timers"]["desk"]) and any(r.startswith("timer:2|Stretch|") for r in s["timers"]["desk"])),
    ("a typed '10 tea' is offered and selected", lambda s: s["timer-typed"]["selection"] == "tnew:typed" and s["timer-typed"]["title"] == "Start 10:00 · tea"),
    ("Esc from Timer goes back", lambda s: s["timers-back"]["view"] == "search"),
    ("Process Monitor: grouped by app, the busiest first", lambda s: s["procs"]["view"] == "procs" and s["procs"]["rows"][0] == "app:app-ghostty-surface-transient-9.scope"),
    ("Tab sorts by GPU: the worker first", lambda s: s["procs-gpu"]["procSort"] == "gpu" and s["procs-gpu"]["rows"][0] == "app:troweld.service"),
    ("↵ once arms End", lambda s: s["procs-armed"]["procArmed"] == "app:app-ghostty-surface-transient-9.scope"),
    ("↵ twice ends it: gone from the list", lambda s: "app:app-ghostty-surface-transient-9.scope" not in s["procs-killed"]["rows"] and s["procs-killed"]["procArmed"] == ""),
    ("Esc from Process Monitor goes back", lambda s: s["procs-back"]["view"] == "search"),
    ("Snippets lists the five", lambda s: s["snippets"]["view"] == "snippets" and len(s["snippets"]["rows"]) == 5),
    ("↵ on a snippet with a blank asks for it", lambda s: s["snippet-args"]["selection"] == "snip:3" and s["snippet-args"]["snippetEditing"] is True and s["snippet-args"]["opened"] is True),
    ("Create Snippet opens the form", lambda s: s["snippet-new"]["view"] == "snippet-edit"),
    ("saving lands in Snippets with the new one", lambda s: s["snippet-saved"]["view"] == "snippets" and len(s["snippet-saved"]["rows"]) == 6),
    ("Omarchy Commands: summary scripts, not hidden or arg-only ones", lambda s: [r.split("|")[0] for r in s["omarchy"]["desk"]] == ["omarchy:omarchy-theme-bg-next", "omarchy:omarchy-toggle-idle", "omarchy:omarchy-update"] and s["omarchy"]["desk"][2].endswith("|Terminal")),
    ("Pick Theme marks the current one", lambda s: len(s["themes"]["rows"]) == 4 and "theme:Tokyo Night|Tokyo Night|Current" in s["themes"]["desk"]),
    ("the island holds the seeded timers", lambda s: s["island-state"]["island"] == ["timer:1", "timer:2"] and s["island-state"]["opened"] is True),
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
