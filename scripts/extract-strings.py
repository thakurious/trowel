#!/usr/bin/env python3
"""Pull user-visible string literals from DigUpApp into ui/strings.json.

Phase 1, EXECUTION-PLAN.md task 7. Walks upstream/Sources/DigUpApp/*.swift and
records each UI literal with source file:line so the QML port can keep copy in
sync. Interpolated Swift strings are kept as templates (the `\\(...)` stays).
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SWIFT_DIR = ROOT / "upstream" / "Sources" / "DigUpApp"
OUT = ROOT / "ui" / "strings.json"

# Calls / attributes whose string arguments are shown to the user.
UI_START = re.compile(
    r"""(?x)
    (?:
        \b(?:Text|Button|Label|Section|Toggle|TextField|SecureField|Picker|
            Title|ActionMenuItem|NSMenuItem|Link)
        \s*\(
      | \.(?:help|accessibilityLabel|navigationTitle|navigationBarTitle)
        \s*\(
      | (?:withTitle|accessibilityDescription|toolTip)\s*=
      | addItem\s*\(\s*withTitle:
    )
    """
)

STRING = re.compile(r'"(?:\\.|[^"\\])*"')
SKIP_SYMBOLS = {
    "plus", "minus", "xmark", "gear", "magnifyingglass", "folder", "photo",
    "doc", "play", "pause", "chevron.right", "chevron.left", "info.circle",
}


def decode_swift_string(literal: str) -> str:
    inner = literal[1:-1]
    out = []
    i = 0
    while i < len(inner):
        if inner[i] == "\\" and i + 1 < len(inner):
            nxt = inner[i + 1]
            mapping = {"n": "\n", "t": "\t", "r": "\r", '"': '"', "\\": "\\"}
            if nxt in mapping:
                out.append(mapping[nxt])
                i += 2
                continue
        out.append(inner[i])
        i += 1
    return "".join(out)


def extract_from_call(src: str, start: int) -> list[tuple[int, str]]:
    """String literals in the argument list of a UI call, plus adjacent concat."""
    depth = 0
    i = start
    seen_open = False
    end = start
    while i < len(src):
        ch = src[i]
        if src.startswith("//", i):
            nl = src.find("\n", i)
            i = len(src) if nl < 0 else nl + 1
            continue
        if ch == "(":
            depth += 1
            seen_open = True
            i += 1
            continue
        if ch == ")":
            depth -= 1
            i += 1
            if seen_open and depth <= 0:
                end = i
                break
            continue
        if ch == '"':
            m = STRING.match(src, i)
            if m:
                i = m.end()
                continue
        i += 1
    else:
        end = min(start + 400, len(src))

    chunk = src[start:end]
    merged: list[tuple[int, str]] = []
    last_end = None
    for m in STRING.finditer(chunk):
        text = decode_swift_string(m.group())
        if not text or text in SKIP_SYMBOLS or len(text) <= 1:
            continue
        if "." in text and " " not in text and not text.endswith("…"):
            continue
        line = src.count("\n", 0, start + m.start()) + 1
        gap = chunk[last_end:m.start()] if last_end is not None else None
        if gap is not None and re.fullmatch(r"[\s+]*", gap):
            prev_line, prev = merged[-1]
            merged[-1] = (prev_line, prev + text)
        else:
            merged.append((line, text))
        last_end = m.end()
    return merged


def extract_file(path: Path) -> list[dict]:
    src = path.read_text(encoding="utf-8")
    rows: list[dict] = []
    seen: set[tuple[int, str]] = set()
    for m in UI_START.finditer(src):
        for line, text in extract_from_call(src, m.start()):
            key = (line, text)
            if key in seen:
                continue
            seen.add(key)
            rows.append({"file": path.name, "line": line, "text": text})
    return rows


def main() -> int:
    if not SWIFT_DIR.is_dir():
        print(f"missing {SWIFT_DIR} (init the upstream submodule)", file=sys.stderr)
        return 1
    rows: list[dict] = []
    for path in sorted(SWIFT_DIR.glob("*.swift")):
        rows.extend(extract_file(path))
    rows.sort(key=lambda r: (r["file"], r["line"], r["text"]))
    OUT.write_text(json.dumps(rows, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"wrote {len(rows)} strings to {OUT.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
