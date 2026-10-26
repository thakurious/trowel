#!/usr/bin/env python3
"""Island checks on the offscreen harness (PHASE9-PLAN.md P2):
    python3 -I ui/harness/island/check.py
Each state's shape: size, centering under the bar, the split bubble, alert expand and settle,
reduce motion. Pictures land in ui/harness/out/island-*.png."""
import json, pathlib, re, subprocess, sys

here = pathlib.Path(__file__).resolve().parent
out = subprocess.run([str(here / "run.sh")], capture_output=True, text=True).stdout
st = {}
for line in out.splitlines():
    m = re.search(r"HARNESS (\S+) (\{.*\})", line)
    if m:
        st[m.group(1)] = json.loads(m.group(2))
noise = [l for l in out.splitlines() if "HARNESS" not in l]

def centered(s):
    total = s["w"] + (s["bubble"] + 6 if s["bubble"] else 0)
    return abs(s["x"] + total / 2 - s["viewWidth"] / 2) <= 1

checks = [
    ("no activity: nothing mapped", lambda s: s["hidden"]["mapped"] is False and s["hidden"]["h"] == 0),
    ("one activity: a 30 px compact pill, centered", lambda s: s["compact"]["mode"] == "compact" and s["compact"]["h"] == 30 and 168 <= s["compact"]["w"] <= 340 and centered(s["compact"])),
    ("a countdown ticks only while one is shown", lambda s: s["compact"]["counting"] is True and s["hidden"]["counting"] is False),
    ("two activities: split with a 30 px bubble, centered together", lambda s: s["split"]["mode"] == "split" and s["split"]["bubble"] == 30 and centered(s["split"])),
    ("expanded: 380 wide, title + bar + buttons + the other line", lambda s: s["expanded"]["mode"] == "expanded" and s["expanded"]["w"] == 380 and s["expanded"]["h"] >= 110 and centered(s["expanded"])),
    ("an alert expands by itself", lambda s: s["alert"]["mode"] == "expanded" and s["alert"]["alert"] == "timer:7"),
    ("and settles back to compact after 4 s", lambda s: s["alert-settled"]["mode"] == "compact" and s["alert-settled"]["alert"] == ""),
    ("more than two: still split (the rest counted)", lambda s: s["more"]["mode"] == "split"),
    ("the last one gone: it shrinks away and unmaps", lambda s: s["gone"]["mapped"] is False and s["gone"]["h"] == 0),
    ("reduce motion: the final size at once", lambda s: s["reduced"]["h"] == 30 and s["reduced"]["mode"] == "compact"),
]
failed = 0
for name, check in checks:
    try:
        ok = check(st)
    except KeyError as e:
        ok, name = False, name + f" (no state {e})"
    print(("ok   " if ok else "FAIL ") + name)
    failed += not ok
if noise:
    print("QML warnings/errors:\n  " + "\n  ".join(noise))
    failed += 1
if failed:
    print(json.dumps(st, indent=1))
sys.exit(1 if failed else 0)
