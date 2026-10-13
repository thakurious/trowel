#!/usr/bin/env python3
"""Linux generators for evals/synthetic.json and evals/multilingual.json.

Replaces macOS-only bits (sips, say, Swift/AppKit, /Library/User Pictures):
  photos     Wikimedia Commons (real pictures, so vision evals have meaning)
  speech     Google translate_tts (no extra package; espeak-ng if present)
  screenshots Pillow + pango, sized to the current Hyprland mode so Rules'
              monitor heuristic classifies them as screenshots (Linux stand-in
              for kMDItemIsScreenCapture)
  PDFs/docs  Chromium print-to-pdf (logical Unicode text layer), docx zip, RTF
  long files bit-faithful LCG from upstream/scripts/testbed/make_long.swift

Never samples $HOME. Run via scripts/make-testbed.sh.
"""
from __future__ import annotations

import io
import json
import os
import subprocess
import sys
import time
import urllib.parse
import urllib.request
import zipfile
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

UA = "TrowelTestbed/0.1 (Linux port of DigUp evals; https://github.com/thakurious/digup-linux)"
HDRS = {"User-Agent": UA}

# Commons titles that match the eval queries (not the macOS User Pictures set).
PHOTOS = {
    "Animals-Penguin.jpg": [
        "File:Aptenodytes forsteri -Snow Hill Island.jpg",
        "File:Emperor Penguin Manchot empereur.jpg",
        "File:Emperor penguins.jpg",
    ],
    "Animals-Zebra.jpg": [
        "File:Plains Zebra Equus quagga.jpg",
        "File:Equus quagga.jpg",
        "File:Zebra Botswana.jpg",
    ],
    "Animals-Eagle.jpg": [
        "File:Bald Eagle Portrait.jpg",
        "File:Haliaeetus leucocephalus Washington.jpg",
        "File:Bald eagle close up.jpg",
    ],
    "Animals-Owl.jpg": [
        "File:Athene noctua (cropped).jpg",
        "File:Tyto alba 1 Luc Viatour.jpg",
        "File:Eurasian Eagle-Owl.jpg",
    ],
    "Animals-Parrot.jpg": [
        "File:Ara macao -on a branch.jpg",
        "File:Scarlet Macaw Macaw.jpg",
        "File:Blue-and-yellow Macaw.jpg",
        "File:Ara ararauna Luc Viatour.jpg",
    ],
    "Instruments-Violin.jpg": [
        "File:Violin VL100.png",
        "File:Violin.jpg",
        "File:Front of a violin.jpg",
    ],
    "Instruments-Guitar.jpg": [
        "File:Acoustic guitar.jpg",
        "File:C.F. Martin guitar.jpg",
        "File:Classical Guitar two views.jpg",
    ],
    "Nature-Earth.jpg": [
        "File:The Earth seen from Apollo 17.jpg",
        "File:NASA Blue Marble 2002.png",
        "File:Earth Western Hemisphere.jpg",
    ],
    "Nature-Lightning.jpg": [
        "File:Lightning over Oradea Romania 2.jpg",
        "File:Lightning 14.07.2009 20-42-14.JPG",
        "File:Lightning strike.jpg",
    ],
    "Flowers-Sunflower.jpg": [
        "File:Sunflower sky backdrop.jpg",
        "File:Helianthus_whorl.jpg",
        "File:Sunflowers.jpg",
    ],
    "Flowers-Yellow Daisy.jpg": [
        "File:Bellis perennis white yellow (aka).jpg",
        "File:Rudbeckia hirta.jpg",
        "File:Yellow daisy.jpg",
    ],
    "Flowers-Dandelion.jpg": [
        "File:TaraxacumOfficinaleSeed.jpg",
        "File:Dandelion flower.jpg",
        "File:Taraxacum officinale flower.jpg",
    ],
    "Fun-Gingerbread Man.jpg": [
        "File:Gingerbread man.jpg",
        "File:Gingerbread men.jpg",
        "File:Gingerbread-man.jpg",
    ],
    "Nature-Cactus.jpg": [
        "File:Carnegiea gigantea in Saguaro National Park near Tucson, Arizona 2.jpg",
        "File:Saguaro cactus arizona.jpg",
        "File:Carnegiea gigantea.jpg",
    ],
    "Nature-Leaf.jpg": [
        "File:Maple leaf.jpg",
        "File:Autumn maple leaf.jpg",
        "File:Green leaf.jpg",
    ],
    "Nature-Zen.jpg": [
        "File:RyoanJi-Dry garden.jpg",
        "File:Japanese rock garden at Ryogen-in, Kyoto.jpg",
        "File:Zen garden.jpg",
    ],
    "Fun-Medal.jpg": [
        "File:Gold medal icon.svg",
        "File:Medal of Honor.jpg",
        "File:Gold-medal.svg",
        "File:Medaille.png",
    ],
    "Sports-Target.jpg": [
        "File:Archery target.jpg",
        "File:Target archery.jpg",
        "File:FITA target.jpg",
    ],
}

FONT_SANS = "/usr/share/fonts/liberation/LiberationSans-Regular.ttf"
FONT_SANS_B = "/usr/share/fonts/liberation/LiberationSans-Bold.ttf"
FONT_MONO = "/usr/share/fonts/liberation/LiberationMono-Regular.ttf"
FONT_BN = "/usr/share/fonts/noto/NotoSansBengali-Regular.ttf"
FONT_BN_B = "/usr/share/fonts/noto/NotoSansBengali-Bold.ttf"
FONT_AR = "/usr/share/fonts/noto/NotoSansArabic-Regular.ttf"
FONT_AR_B = "/usr/share/fonts/noto/NotoSansArabic-Bold.ttf"


def font(path, size):
    try:
        return ImageFont.truetype(path, size)
    except OSError:
        return ImageFont.load_default()


def run(cmd, **kw):
    subprocess.run(cmd, check=True, **kw)


def monitor_size():
    try:
        out = subprocess.check_output(["hyprctl", "monitors", "-j"], text=True)
        m = json.loads(out)[0]
        return int(m["width"]), int(m["height"])
    except Exception:
        return 1920, 1200


def fetch(url, dest: Path, tries=3):
    dest.parent.mkdir(parents=True, exist_ok=True)
    err = None
    for _ in range(tries):
        try:
            req = urllib.request.Request(url, headers=HDRS)
            with urllib.request.urlopen(req, timeout=40) as r:
                data = r.read()
            if len(data) < 500:
                raise RuntimeError(f"tiny reply {len(data)} for {url}")
            dest.write_bytes(data)
            return
        except Exception as e:
            err = e
            time.sleep(0.4)
    raise RuntimeError(f"download {url}: {err}")


def commons_search(query: str) -> list[str]:
    url = "https://commons.wikimedia.org/w/api.php?" + urllib.parse.urlencode(
        {
            "action": "query",
            "list": "search",
            "srsearch": query,
            "srnamespace": 6,
            "srlimit": 8,
            "format": "json",
        }
    )
    req = urllib.request.Request(url, headers=HDRS)
    with urllib.request.urlopen(req, timeout=40) as r:
        body = json.load(r)
    titles = []
    for hit in body.get("query", {}).get("search", []):
        raw = hit.get("title") or ""
        name = raw.split(":", 1)[-1]
        titles.append("File:" + name)
    return titles


def commons_thumb(title: str, width=768) -> str | None:
    q = urllib.parse.urlencode(
        {
            "action": "query",
            "titles": title,
            "prop": "imageinfo",
            "iiprop": "url",
            "iiurlwidth": str(width),
            "format": "json",
        }
    )
    url = "https://commons.wikimedia.org/w/api.php?" + q
    req = urllib.request.Request(url, headers=HDRS)
    with urllib.request.urlopen(req, timeout=40) as r:
        body = json.load(r)
    pages = body.get("query", {}).get("pages", {})
    for page in pages.values():
        info = (page.get("imageinfo") or [None])[0]
        if not info:
            continue
        return info.get("thumburl") or info.get("url")
    return None


def jpeg_from_photo(raw: bytes, dest: Path):
    im = Image.open(io.BytesIO(raw)).convert("RGB")
    im.thumbnail((768, 768))
    dest.parent.mkdir(parents=True, exist_ok=True)
    im.save(dest, "JPEG", quality=88)


def try_commons_title(title: str, dest: Path, cache: Path) -> bool:
    key = title.replace(" ", "_").replace("File:", "").replace("/", "_")
    cached = cache / (key + ".bin")
    try:
        if not cached.exists():
            thumb = commons_thumb(title)
            if not thumb:
                return False
            fetch(thumb, cached)
        jpeg_from_photo(cached.read_bytes(), dest)
        print(f"  photo {dest.name} ← {title}")
        return True
    except Exception as e:
        print(f"  skip {title}: {e}", file=sys.stderr)
        cached.unlink(missing_ok=True)
        return False


def fallback_photo(name: str, dest: Path):
    """Last resort: a labeled still so the file exists; vision evals will be weaker."""
    stem = Path(name).stem.replace("-", " ")
    im = Image.new("RGB", (768, 768), (40, 70, 90))
    d = ImageDraw.Draw(im)
    d.ellipse((180, 160, 588, 560), fill=(200, 180, 120))
    f = font(FONT_SANS_B, 36)
    d.text((40, 640), stem, fill=(255, 255, 255), font=f)
    dest.parent.mkdir(parents=True, exist_ok=True)
    im.save(dest, "JPEG", quality=85)
    print(f"  fallback (no commons hit): {name}", file=sys.stderr)


def download_photos(img_dir: Path, cache: Path):
    img_dir.mkdir(parents=True, exist_ok=True)
    cache.mkdir(parents=True, exist_ok=True)
    for name, titles in PHOTOS.items():
        dest = img_dir / name
        if dest.exists() and dest.stat().st_size > 1000:
            continue
        got = False
        for title in titles:
            if try_commons_title(title, dest, cache):
                got = True
                break
        if not got:
            query = Path(name).stem.replace("-", " ")
            try:
                extra = commons_search(query)
            except Exception as e:
                extra = []
                print(f"  search {query}: {e}", file=sys.stderr)
            seen = set(titles)
            for title in extra:
                if title in seen:
                    continue
                if try_commons_title(title, dest, cache):
                    got = True
                    break
        if not got:
            fallback_photo(name, dest)


def pango_png(text: str, dest: Path, face: str, width: int, extra=None):
    src = dest.with_suffix(".txt")
    src.write_text(text, encoding="utf-8")
    cmd = [
        "pango-view",
        "--no-display",
        f"--font={face}",
        f"--width={width}",
        "-qo",
        str(dest),
        str(src),
    ]
    if extra:
        cmd[1:1] = extra
    run(cmd)
    src.unlink(missing_ok=True)


def _html(text: str) -> str:
    return (
        text.replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
        .replace("\n", "<br>")
    )


def pango_pdf_pages(pages: list[tuple[str, str]], dest: Path, face: str):
    """Letter-size PDF with a logical Unicode text layer for FTS.

    pangocairo paints Arabic/Bengali as positioned glyphs, so pdftotext
    comes back with spaces between letters and keyword evals miss. Chromium
    print-to-pdf keeps copyable words (الأردن, ধানমন্ডি).
    """
    parts = face.rsplit(" ", 1)
    family, size = (parts[0], parts[1]) if len(parts) == 2 else (face, "14")
    rtl = "Arabic" in family
    sections = []
    for title, body in pages:
        sections.append(
            f"<section><h1>{_html(title)}</h1><p>{_html(body)}</p></section>"
        )
    html = f"""<!doctype html><meta charset="utf-8">
<style>
@page {{ size: letter; margin: 0.75in; }}
body {{
  font-family: "{family}", serif;
  font-size: {size}pt;
  line-height: 1.4;
  {"direction: rtl;" if rtl else ""}
}}
section {{ page-break-after: always; }}
section:last-child {{ page-break-after: auto; }}
h1 {{ font-size: {int(size) + 2}pt; font-weight: 600; }}
p {{ margin-top: 0.8em; }}
</style>
{''.join(sections)}
"""
    dest.parent.mkdir(parents=True, exist_ok=True)
    leftover = dest.parent / (dest.stem + ".pages")
    if leftover.is_dir():
        for p in leftover.iterdir():
            p.unlink()
        leftover.rmdir()
    src = dest.with_suffix(".html")
    src.write_text(html, encoding="utf-8")
    run(
        [
            "chromium",
            "--headless",
            "--disable-gpu",
            "--no-pdf-header-footer",
            f"--print-to-pdf={dest}",
            str(src),
        ],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    src.unlink(missing_ok=True)


def write_docx(path: Path, text: str):
    document = f"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body><w:p><w:r><w:t xml:space="preserve">{_xml(text)}</w:t></w:r></w:p></w:body>
</w:document>
"""
    types = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml"
    ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>
"""
    rels = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1"
    Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument"
    Target="word/document.xml"/>
</Relationships>
"""
    path.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(path, "w") as z:
        z.writestr("[Content_Types].xml", types)
        z.writestr("_rels/.rels", rels)
        z.writestr("word/document.xml", document)


def _xml(text: str) -> str:
    return (
        text.replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
    )


def write_rtf(path: Path, text: str):
    parts = [r"{\rtf1\ansi\ansicpg1252\deff0{\fonttbl{\f0 Noto Sans;}}\f0\fs28 "]
    for ch in text:
        o = ord(ch)
        if ch == "\n":
            parts.append(r"\par ")
        elif ch == "\\":
            parts.append(r"\\")
        elif ch == "{":
            parts.append(r"\{")
        elif ch == "}":
            parts.append(r"\}")
        elif o < 128:
            parts.append(ch)
        else:
            # signed 16-bit as unrtf expects
            n = o if o < 32768 else o - 65536
            parts.append(f"\\u{n}?")
    parts.append("}")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("".join(parts), encoding="ascii", errors="replace")


def tts_mp3(text: str, lang: str, dest: Path):
    q = urllib.parse.quote(text)
    url = (
        "https://translate.googleapis.com/translate_tts"
        f"?ie=UTF-8&tl={lang}&client=gtx&q={q}"
    )
    fetch(url, dest)


def speak(text: str, wav: Path, lang="en"):
    """Sentence-split Google TTS → 16 kHz mono wav (EmbeddingGemma audio)."""
    wav.parent.mkdir(parents=True, exist_ok=True)
    chunks, buf = [], ""
    for part in text.replace("!", ".").replace("?", ".").split("."):
        part = part.strip()
        if not part:
            continue
        piece = part + "."
        if len(buf) + len(piece) > 160 and buf:
            chunks.append(buf)
            buf = piece
        else:
            buf = (buf + " " + piece).strip()
    if buf:
        chunks.append(buf)
    if not chunks:
        chunks = [text[:160]]
    tmp = wav.parent / (wav.stem + ".parts")
    tmp.mkdir(exist_ok=True)
    mp3s = []
    for i, chunk in enumerate(chunks):
        p = tmp / f"{i:02d}.mp3"
        tts_mp3(chunk, lang, p)
        mp3s.append(p)
        time.sleep(0.15)
    lst = tmp / "list.txt"
    lst.write_text("".join(f"file '{p.name}'\n" for p in mp3s))
    run(
        [
            "ffmpeg",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "concat",
            "-safe",
            "0",
            "-i",
            str(lst),
            "-ac",
            "1",
            "-ar",
            "16000",
            str(wav),
        ]
    )
    for p in tmp.iterdir():
        p.unlink()
    tmp.rmdir()


def window_chrome(size, title: str, bg=(255, 255, 255), chrome=(237, 237, 237)):
    w, h = size
    im = Image.new("RGB", (w, h), (92, 115, 158))
    d = ImageDraw.Draw(im)
    margin_x, margin_y = 80, 60
    win = (margin_x, margin_y, w - margin_x, h - margin_y)
    d.rounded_rectangle(win, 12, fill=bg)
    d.rectangle((win[0], win[1], win[2], win[1] + 44), fill=chrome)
    for i, c in enumerate([(224, 81, 68), (240, 193, 48), (98, 186, 70)]):
        x = win[0] + 18 + i * 22
        d.ellipse((x, win[1] + 15, x + 13, win[1] + 28), fill=c)
    d.text(
        (win[0] + 200, win[1] + 12),
        title,
        fill=(90, 90, 90),
        font=font(FONT_SANS, 18),
    )
    body = (win[0] + 40, win[1] + 70, win[2] - 40, win[3] - 30)
    return im, d, body


def make_screenshots(out: Path, size: tuple[int, int]):
    out.mkdir(parents=True, exist_ok=True)
    w, h = size

    im, d, b = window_chrome(size, "dashboard.payments.example.com — Invoices")
    d.text((b[0], b[1]), "Invoices", fill=(0, 0, 0), font=font(FONT_SANS_B, 40))
    d.rounded_rectangle((b[0], b[1] + 70, b[2], b[1] + 150), 8, fill=(255, 230, 230))
    d.text(
        (b[0] + 24, b[1] + 95),
        "Payment failed: Your card was declined (card_declined)",
        fill=(180, 30, 30),
        font=font(FONT_SANS_B, 28),
    )
    d.text(
        (b[0], b[1] + 180),
        "Invoice INV-2041   ·   Acme Design Studio   ·   $249.00 USD   ·   Visa •••• 4242",
        fill=(20, 20, 20),
        font=font(FONT_SANS, 22),
    )
    d.text(
        (b[0], b[1] + 220),
        "Attempted Oct 3, 2026 9:41 AM — The customer's bank declined the charge.",
        fill=(80, 80, 80),
        font=font(FONT_SANS, 20),
    )
    d.rounded_rectangle((b[0], b[1] + 280, b[0] + 220, b[1] + 330), 8, fill=(40, 110, 210))
    d.text((b[0] + 40, b[1] + 292), "Retry payment", fill=(255, 255, 255), font=font(FONT_SANS_B, 20))
    im.save(out / "shot_payment_error.png")

    im, d, b = window_chrome(size, "zsh — 120×40", bg=(26, 26, 26), chrome=(56, 56, 56))
    lines = [
        ("$ python train.py --epochs 10", (230, 230, 230)),
        ("Traceback (most recent call last):", (230, 230, 230)),
        ('  File "/Users/me/project/train.py", line 3, in <module>', (230, 230, 230)),
        ("    import torch", (230, 230, 230)),
        ("ModuleNotFoundError: No module named 'torch'", (220, 60, 60)),
        ("$ pip install torch", (230, 230, 230)),
        ("error: externally-managed-environment", (220, 60, 60)),
    ]
    y = b[1]
    for line, color in lines:
        d.text((b[0], y), line, fill=color, font=font(FONT_MONO, 22))
        y += 36
    im.save(out / "shot_python_traceback.png")

    im, d, b = window_chrome(size, "Mail — Your booking is confirmed")
    d.text((b[0], b[1]), "Booking confirmed", fill=(0, 0, 0), font=font(FONT_SANS_B, 40))
    d.rounded_rectangle((b[0], b[1] + 70, b[0] + int((b[2] - b[0]) * 0.75), b[1] + 280), 14, fill=(237, 245, 255))
    d.text(
        (b[0] + 30, b[1] + 90),
        "San Francisco (SFO)  →  Tokyo Haneda (HND)",
        fill=(0, 0, 0),
        font=font(FONT_SANS_B, 28),
    )
    d.text(
        (b[0] + 30, b[1] + 140),
        "Flight NH 7   ·   Departs Fri, Nov 14 at 14:30   ·   Arrives Sat 18:55",
        fill=(20, 20, 20),
        font=font(FONT_SANS, 22),
    )
    d.text(
        (b[0] + 30, b[1] + 185),
        "Passenger: Alex Doe   ·   Seat 32A   ·   Gate 112   ·   Confirmation code: K7Q2LM",
        fill=(20, 20, 20),
        font=font(FONT_SANS, 22),
    )
    d.text(
        (b[0] + 30, b[1] + 230),
        "Check-in opens 24 hours before departure.",
        fill=(80, 80, 80),
        font=font(FONT_SANS, 18),
    )
    im.save(out / "shot_flight_booking.png")

    im, d, b = window_chrome(size, "#design-team")
    msgs = [
        ((240, 140, 50), "Sam", "Can we move the design review to Thursday? I'm out sick tomorrow."),
        ((130, 80, 180), "Priya", "Sure, Thursday 3pm works for me. I'll update the calendar invite."),
        ((240, 140, 50), "Sam", "Thanks! I'll also share the new onboarding mockups before the meeting."),
    ]
    y = b[1]
    for color, who, msg in msgs:
        d.ellipse((b[0], y, b[0] + 44, y + 44), fill=color)
        d.text((b[0] + 60, y), who, fill=(0, 0, 0), font=font(FONT_SANS_B, 22))
        d.text((b[0] + 60, y + 28), msg, fill=(20, 20, 20), font=font(FONT_SANS, 22))
        y += 110
    im.save(out / "shot_chat_meeting.png")

    im, d, b = window_chrome(size, "Weather", bg=(217, 230, 245))
    d.text((b[0], b[1]), "London", fill=(0, 0, 0), font=font(FONT_SANS_B, 48))
    d.text((b[0], b[1] + 70), "18° · Heavy rain · 90% chance of rain", fill=(20, 20, 20), font=font(FONT_SANS, 32))
    for i, day in enumerate(["Mon rain 15°", "Tue rain 14°", "Wed storm 13°", "Thu drizzle 16°", "Fri cloud 17°"]):
        x = b[0] + i * 280
        d.rounded_rectangle((x, b[1] + 160, x + 250, b[1] + 300), 16, fill=(255, 255, 255))
        d.text((x + 24, b[1] + 210), day, fill=(0, 0, 0), font=font(FONT_SANS, 22))
    im.save(out / "shot_weather.png")

    im, d, b = window_chrome(size, "SearchIndex.swift — Editor", bg=(30, 30, 30), chrome=(51, 51, 51))
    code = [
        ("import Accelerate", (220, 90, 160)),
        ("", (230, 230, 230)),
        ("struct SearchIndex {", (220, 90, 160)),
        ("    var vectors: [Float]", (230, 230, 230)),
        ("    let dimension = 768", (230, 230, 230)),
        ("", (230, 230, 230)),
        ("    func search(query: [Float], topK: Int) -> [Int] {", (220, 90, 160)),
        ("        // cosine similarity via matrix-vector product", (90, 180, 90)),
        ("        var scores = [Float](repeating: 0, count: vectors.count / dimension)", (230, 230, 230)),
        ("        cblas_sgemv(CblasRowMajor, CblasNoTrans, ...)", (230, 230, 230)),
        ("        return scores.indices.sorted { scores[$0] > scores[$1] }.prefix(topK).map { $0 }", (230, 230, 230)),
        ("    }", (230, 230, 230)),
        ("}", (230, 230, 230)),
    ]
    y = b[1]
    for line, color in code:
        d.text((b[0], y), line, fill=color, font=font(FONT_MONO, 20))
        y += 32
    im.save(out / "shot_swift_code.png")


def lang_screenshot(path: Path, size, title, language, lines):
    """UI chrome + pango-shaped body (Bengali / Arabic)."""
    bg = (255, 255, 255)
    im, d, b = window_chrome(size, title, bg=bg)
    y = b[1]
    width = b[2] - b[0]
    face = "Noto Sans Bengali" if language == "bn" else "Noto Sans Arabic"
    for text, pts, color, banner in lines:
        if banner:
            d.rounded_rectangle((b[0], y, b[2], y + 90), 10, fill=(255, 230, 230))
        tmp = Path("/tmp") / f"trowel-lang-{os.getpid()}.png"
        pango_png(text, tmp, f"{face} {pts}", max(width - 40, 400))
        strip = Image.open(tmp).convert("RGBA")
        x = b[0] + 20
        if language == "ar":
            x = b[2] - 20 - strip.width
        im.paste(strip, (x, y + 12), strip)
        tmp.unlink(missing_ok=True)
        y += max(strip.height + 24, 70)
    path.parent.mkdir(parents=True, exist_ok=True)
    im.save(path)


def make_docs(out: Path):
    out.mkdir(parents=True, exist_ok=True)
    pango_pdf_pages(
        [
            (
                "Quarterly business review",
                "Revenue grew 20% compared to last year, driven by strong sales in Europe and a new enterprise plan. "
                "Operating costs stayed flat, and the team hired four engineers. Next quarter we will focus on retention "
                "and on expanding the partner program in Japan.",
            ),
            (
                "Hiking trip to Mount Fuji",
                "We started the climb from the Yoshida trail at the fifth station before sunrise. The path was rocky and "
                "cold near the summit, but the view of the clouds below the crater was unforgettable. Pack warm gloves, "
                "a headlamp and enough water; the mountain huts sell instant noodles.",
            ),
            (
                "Sourdough bread recipe",
                "Feed the starter the night before. Mix 500 g flour, 350 g water and 100 g active starter, rest for an "
                "hour, then add 10 g salt. Stretch and fold four times, shape a boule, proof overnight in the fridge and "
                "bake in a Dutch oven at 250 °C.",
            ),
        ],
        out / "Annual notes 2026.pdf",
        "Liberation Serif 14",
    )
    # Image-only "scanned" receipt: no text layer, OCR / vision must read it.
    rec = Image.new("RGB", (900, 1200), (247, 247, 247))
    d = ImageDraw.Draw(rec)
    lines = [
        "BLUE BOTTLE COFFEE",
        "Hayes Valley, San Francisco",
        "",
        "Oat latte            5.50",
        "Almond croissant     4.75",
        "Cold brew            4.25",
        "",
        "TOTAL               14.50",
        "VISA **** 4242",
        "Thank you for visiting!",
        "coffee shop receipt",
    ]
    y = 80
    for i, line in enumerate(lines):
        d.text((80, y), line, fill=(0, 0, 0), font=font(FONT_MONO, 34 if i == 0 else 28))
        y += 70
    rec_path = out / "receipt.png"
    rec.save(rec_path)
    # Embed the PNG as a one-page PDF without a text layer.
    img_pdf = out / "Scan 2026-09-14.pdf"
    rec.convert("RGB").save(img_pdf, "PDF", resolution=72.0)
    rec_path.unlink(missing_ok=True)

    (out / "Groceries.txt").write_text(
        "Grocery list for the weekend: milk, eggs, bread, apples, coffee beans and dish soap.\n",
        encoding="utf-8",
    )
    (out / "Tokyo itinerary.md").write_text(
        "# Tokyo trip\n\nFlight to Tokyo Haneda on November 14. Hotel in Shinjuku for five nights. "
        "Day trip to Kamakura on Sunday to see the Great Buddha.\n",
        encoding="utf-8",
    )
    write_rtf(
        out / "Meeting notes.rtf",
        "Design review moved to Thursday at 3pm because Sam was sick. Priya will update the calendar invite "
        "and Sam will share the onboarding mockups.",
    )
    write_docx(
        out / "Vector search notes.docx",
        "Notes on implementing cosine similarity search over embedding vectors with Accelerate in Swift: "
        "normalize vectors, use cblas_sgemv, keep the top-k with a heap.",
    )


def make_lang(out: Path, size):
    bn = out / "bn"
    ar = out / "ar"
    bn.mkdir(parents=True, exist_ok=True)
    ar.mkdir(parents=True, exist_ok=True)
    (bn / "পরীক্ষার ফলাফল.txt").write_text(
        "এসএসসি পরীক্ষার ফলাফল প্রকাশিত হয়েছে। এ বছর পাসের হার ৮৩ শতাংশ, যা গত বছরের চেয়ে বেশি। "
        "ঢাকা বোর্ডে মেয়েরা ছেলেদের চেয়ে ভালো ফল করেছে। ফলাফল শিক্ষা বোর্ডের ওয়েবসাইটে এবং মোবাইলে "
        "এসএমএসের মাধ্যমে জানা যাবে।\n",
        encoding="utf-8",
    )
    (bn / "ইলিশ মাছের রেসিপি.md").write_text(
        "# সরিষা ইলিশ\n\nসরিষা ইলিশ রান্নার জন্য প্রথমে সরিষা বাটা, কাঁচা মরিচ, হলুদ আর লবণ দিয়ে "
        "ইলিশ মাছের টুকরোগুলো মাখিয়ে নিন। কড়াইয়ে সরিষার তেল গরম করে মাছগুলো দিন, ঢেকে দিয়ে কম আঁচে "
        "দশ মিনিট রান্না করুন। গরম ভাতের সঙ্গে পরিবেশন করুন।\n",
        encoding="utf-8",
    )
    write_docx(
        bn / "Sundarbans trip.docx",
        "সুন্দরবন ভ্রমণ\n\nখুলনা থেকে লঞ্চে করে আমরা তিন দিনের জন্য সুন্দরবনে গিয়েছিলাম। ম্যানগ্রোভ বনের "
        "খালে নৌকায় ঘুরে আমরা রয়েল বেঙ্গল টাইগারের পায়ের ছাপ দেখেছি। চিত্রা হরিণ, বানর আর একটি বড় "
        "কুমিরও চোখে পড়েছে। রাতে লঞ্চের ছাদে বসে তারা দেখা ছিল সবচেয়ে সুন্দর অভিজ্ঞতা।",
    )
    pango_pdf_pages(
        [
            (
                "বাড়ি ভাড়ার চুক্তিপত্র",
                "ধানমন্ডি ৭ নম্বর রোডের তিন বেডরুমের ফ্ল্যাটটি মাসিক ২৫,০০০ টাকা ভাড়ায় দেওয়া হলো। "
                "ভাড়াটিয়া দুই মাসের ভাড়া অগ্রিম জমা দেবেন। প্রতি মাসের পাঁচ তারিখের মধ্যে ভাড়া পরিশোধ করতে হবে। "
                "বিদ্যুৎ, পানি ও গ্যাস বিল ভাড়াটিয়া আলাদাভাবে দেবেন। চুক্তির মেয়াদ এক বছর, এবং বাড়ি ছাড়ার আগে "
                "অন্তত দুই মাস আগে বাড়িওয়ালাকে লিখিতভাবে জানাতে হবে। ফ্ল্যাটে কোনো স্থায়ী পরিবর্তন করতে হলে "
                "বাড়িওয়ালার অনুমতি লাগবে।",
            ),
            (
                "প্রোগ্রামিং ক্লাসের নোট",
                "আজকের ক্লাসে আমরা পাইথনে লুপ আর ফাংশন শিখেছি। for লুপ দিয়ে একটি তালিকার প্রতিটি উপাদানের উপর "
                "কাজ করা যায়, আর while লুপ চলতে থাকে যতক্ষণ শর্তটি সত্য থাকে। ফাংশন তৈরি করতে def শব্দটি ব্যবহার "
                "করা হয়, এবং return দিয়ে ফলাফল ফেরত পাঠানো হয়। বাড়ির কাজ: একটি ফাংশন লিখুন যা একটি সংখ্যার "
                "ফ্যাক্টোরিয়াল বের করে, তারপর লুপ দিয়ে এক থেকে দশ পর্যন্ত সব সংখ্যার ফ্যাক্টোরিয়াল ছাপুন।",
            ),
        ],
        bn / "বাড়ি ভাড়ার চুক্তি.pdf",
        "Noto Sans Bengali 14",
    )
    lang_screenshot(
        bn / "wallet_failed_bn.png",
        size,
        "মোবাইল ওয়ালেট — টাকা পাঠান",
        "bn",
        [
            ("টাকা পাঠান", 34, (0, 0, 0), False),
            ("লেনদেন ব্যর্থ হয়েছে: অপর্যাপ্ত ব্যালেন্স", 30, (180, 0, 0), True),
            ("পরিমাণ: ৳ ৫,০০০   ·   প্রাপক: ০১৭০০-০০০০০০   ·   সময়: সকাল ১০:১২", 24, (0, 0, 0), False),
            ("আপনার অ্যাকাউন্টে যথেষ্ট টাকা নেই। টাকা যোগ করে আবার চেষ্টা করুন।", 22, (80, 80, 80), False),
        ],
    )

    (ar / "وصفة الكبسة.txt").write_text(
        "الكبسة السعودية: اغسل الأرز البسمتي وانقعه في الماء لمدة نصف ساعة. في قدر كبير، اقلِ البصل حتى يذبل ثم أضف قطع "
        "الدجاج والطماطم والثوم وبهارات الكبسة. أضف الماء واترك الدجاج ينضج، ثم أضف الأرز واطبخه على نار هادئة. قدّمها مع "
        "اللوز المحمص والزبيب.\n",
        encoding="utf-8",
    )
    write_rtf(
        ar / "Meeting minutes.rtf",
        "محضر اجتماع فريق التطوير\n\nتقرر تأجيل إطلاق التطبيق إلى شهر ديسمبر بسبب مشاكل في نظام الدفع الإلكتروني. "
        "سيقوم فريق الخوادم بإصلاح الأخطاء قبل نهاية الشهر، وسيتم اختبار النسخة الجديدة مع مجموعة صغيرة من المستخدمين.",
    )
    (ar / "ملاحظة.md").write_text(
        "# مُلاحَظَة\n\nقَالَ مُحَمَّدٌ إِنَّ الْمَكْتَبَةَ الْعَامَّةَ تُغْلَقُ فِي السَّاعَةِ الثَّامِنَةِ مَسَاءً يَوْمَ الْخَمِيسِ، فَلَا تَتَأَخَّرْ فِي إِعَادَةِ الْكُتُبِ.\n",
        encoding="utf-8",
    )
    pango_pdf_pages(
        [
            (
                "التقرير الربعي",
                "ارتفعت المبيعات بنسبة خمسة عشر بالمئة في الربع الثالث مقارنة بالعام الماضي، بفضل نمو قوي في أسواق الخليج وإطلاق "
                "خدمة الاشتراك الجديدة. بقيت التكاليف التشغيلية مستقرة، وانضم ثلاثة مهندسين جدد إلى الفريق. في الربع القادم سنركز "
                "على رضا العملاء وتوسيع شبكة الشركاء.",
            ),
            (
                "رحلة إلى البتراء",
                "زرنا مدينة البتراء الأثرية في جنوب الأردن. مشينا عبر السيق الضيق حتى ظهرت الخزنة المنحوتة في الصخر الوردي. صعدنا "
                "إلى الدير في الصباح الباكر لتجنب الحر، وتناولنا الغداء في قرية وادي موسى. أنصح بارتداء حذاء مريح وحمل الكثير من "
                "الماء.",
            ),
        ],
        ar / "تقرير.pdf",
        "Noto Sans Arabic 14",
    )
    lang_screenshot(
        ar / "payment_error_ar.png",
        size,
        "متجر إلكتروني — الدفع",
        "ar",
        [
            ("إتمام الطلب", 34, (0, 0, 0), False),
            ("فشلت عملية الدفع: البطاقة مرفوضة من البنك", 30, (180, 0, 0), True),
            ("المبلغ: ٣٥٠ ريال   ·   رقم الطلب: ٤٨٢١٧   ·   بطاقة فيزا تنتهي بـ ٤٢٤٢", 24, (0, 0, 0), False),
            ("يرجى استخدام بطاقة أخرى أو المحاولة مرة أخرى لاحقاً.", 22, (80, 80, 80), False),
        ],
    )


def make_long(out: Path):
    """upstream/scripts/testbed/make_long.swift, same LCG seed 20251009."""
    out.mkdir(parents=True, exist_ok=True)
    seed = 20251009

    def pick(items):
        nonlocal seed
        seed = (seed * 6364136223846793005 + 1442695040888963407) & 0xFFFFFFFFFFFFFFFF
        return items[(seed >> 33) % len(items)]

    ports = [
        "Falmouth",
        "Fowey",
        "Plymouth",
        "Salcombe",
        "Dartmouth",
        "Weymouth",
        "Poole",
        "Yarmouth",
        "Cherbourg",
        "St Peter Port",
        "Alderney",
        "Brixham",
        "Penzance",
        "Newlyn",
        "St Mary's",
    ]
    winds = [
        "northwest 3",
        "west 4",
        "southwest 4 to 5",
        "south 3",
        "variable 2",
        "northeast 4",
        "west 5, gusting 6",
        "southwest 3",
    ]
    seas = ["slight", "moderate", "smooth", "slight to moderate", "a long swell from the west"]
    weather = [
        "Sunny spells and good visibility.",
        "Overcast, with drizzle in the afternoon.",
        "Clear skies, cold in the evening.",
        "Showers passing through, rainbow at noon.",
        "Grey and still all morning, brighter later.",
        "High cloud and a halo around the sun.",
    ]
    days = [
        "Two reefs in the main until the wind eased, then full sail and a long beat to windward.",
        "Practised man-overboard drills with the fender twice; both recoveries under two minutes.",
        "Checked the rigging at the mast foot and tightened a loose shackle on the kicker.",
        "Long reach along the coast, logged a steady six knots most of the way.",
        "Motor-sailed the last hour to make the tide at the harbour entrance.",
        "Cleaned the bilge, topped up the water tanks and bought bread and fruit ashore.",
        "Took bearings on the lighthouse and the church spire to fix our position on the chart.",
        "Changed the genoa sheets and whipped the frayed ends with waxed twine.",
        "Quiet day at anchor: read, swam off the stern and cooked a curry for dinner.",
    ]

    def log_page(number):
        frm = pick(ports)
        to = pick([p for p in ports if p != frm])
        return (
            f"Day {number}. Left {frm} at {pick(['06:10', '07:45', '08:30', '09:15', '10:40'])}, bound for {to}. "
            f"Wind {pick(winds)}, sea {pick(seas)}. {pick(weather)} {pick(days)} {pick(list(reversed(days)))} "
            f"Moored in {to} after {pick(['18', '23', '27', '31', '36', '42'])} miles. Barometer "
            f"{pick(['1012', '1016', '1019', '1021', '1008'])} and steady. Crew well; the night watch went to the "
            "skipper and the mate in two-hour turns."
        )

    standouts = {
        41: "Day 41. The engine overheated off Start Point and stopped. The raw-water impeller had lost all its vanes, so "
        "we sailed the last eight miles into Brixham under genoa alone and fitted the spare impeller on the pontoon. "
        "Ordered two more from the chandlery; never leave harbour without a spare again.",
        66: "Day 66. At sunset a pod of a dozen common dolphins came racing in from the south and rode our bow wave for "
        "twenty minutes, leaping clear of the water and rolling to look up at us. Nobody wanted to go below for "
        "dinner. The skipper said it was the best evening of the whole season.",
        88: "Day 88. Picked up visitors' mooring ball M-417 in the Helford river, paid the harbour master eighteen pounds "
        "and rowed ashore for supplies. Quiet creek, oak woods down to the water, an egret fishing on the mud.",
        100: "Day 100. Fog came down so thick off the Lizard that we could not see the bow from the cockpit. We slowed to "
        "three knots, sounded the horn every two minutes and steered by radar and the plotter while a tanker's "
        "foghorn boomed somewhere to starboard. It lifted only as we reached the harbour wall.",
    }
    pages = []
    for n in range(1, 121):
        body = standouts.get(n, log_page(n))
        pages.append((f"Sailing log 2025 · page {n}", body))
    pango_pdf_pages(pages, out / "Sailing log 2025.pdf", "Liberation Serif 13")

    topics = [
        "the release checklist",
        "on-call handover",
        "the design review backlog",
        "hiring for the data team",
        "the customer survey results",
        "budget for the next quarter",
        "the support queue",
        "flaky tests in CI",
        "the onboarding guide",
        "the API deprecation plan",
        "laptop refresh",
        "the team offsite",
    ]
    actions = [
        "Priya will draft a proposal by Friday.",
        "Sam takes the follow-up with finance.",
        "We agreed to revisit this in two weeks.",
        "Ana will write it up in the wiki.",
        "No decision yet; collect more data first.",
        "Ben volunteered to own it this sprint.",
        "Moved to the next planning meeting.",
        "Done, closing the ticket.",
    ]
    notes = "# Team notes 2026\n\n"
    week, told = 1, False
    while len(notes) < 150_000:
        notes += f"## Week {week}\n\n"
        if not told and len(notes) > 120_000:
            told = True
            notes += (
                "The big news: we agreed to move the whole team to a new office in Lisbon next March. The rent is lower, "
                "half of the team already lives in Portugal, and the building has room to grow. Marta will look at schools "
                "for the families who relocate, and the company covers the moving costs.\n\n"
            )
        for _ in range(6):
            notes += (
                f"- On {pick(topics)}: {pick(['discussed', 'reviewed', 'went through', 'looked again at'])} where it stands. "
                f"{pick(actions)} {pick(actions)}\n"
            )
        notes += "\n"
        week += 1
    (out / "Team notes 2026.md").write_text(notes, encoding="utf-8")

    readings = ["timestamp,sensor,value\n"]
    for row in range(12_000):
        readings.append(f"{1_760_000_000 + row * 60} {row % 17} {row * 7919 % 100_003}\n")
        if row == 11_500:
            readings.append("4815162342 0 0\n")
    (out / "sensor readings.txt").write_text("".join(readings), encoding="utf-8")


def make_speech(audio: Path, lang: Path):
    audio.mkdir(parents=True, exist_ok=True)
    speak(
        "Don't forget to buy milk, eggs and bread on the way home tomorrow.",
        audio / "speech_groceries.wav",
        "en",
    )
    speak(
        "Our quarterly revenue grew by twenty percent, driven by strong sales in Europe.",
        audio / "speech_revenue.wav",
        "en",
    )
    speak(
        "Attention passengers. The flight to Tokyo has been delayed by two hours because of a storm.",
        audio / "speech_flight_delay.wav",
        "en",
    )
    speak(
        "In this recipe we will bake a chocolate cake. First, preheat the oven to one hundred eighty degrees, then mix "
        "the flour, sugar and cocoa powder in a large bowl. Add the eggs, the melted butter and the milk, and stir until "
        "the batter is smooth. Pour it into a greased pan and bake for thirty five minutes. While the cake is baking, "
        "prepare the frosting by whipping cream with a little sugar and vanilla. When the cake has cooled down completely, "
        "spread the frosting on top and decorate it with fresh strawberries and some grated chocolate.",
        audio / "speech_cake_recipe.wav",
        "en",
    )
    speak(
        "আগামীকাল ঢাকায় ভারী বৃষ্টি হবে, তাই বাইরে যাওয়ার সময় ছাতা নিয়ে বের হবেন। বিকেলে বজ্রসহ ঝড়ের সম্ভাবনা আছে।",
        lang / "bn" / "speech_weather_bn.wav",
        "bn",
    )
    speak(
        "غدا سيكون الطقس ممطرا في الرياض مع رياح قوية، فلا تنس المظلة عند خروجك من البيت.",
        lang / "ar" / "speech_weather_ar.wav",
        "ar",
    )


def make_video(img: Path, video: Path):
    video.mkdir(parents=True, exist_ok=True)
    penguin, zebra, parrot = (
        img / "Animals-Penguin.jpg",
        img / "Animals-Zebra.jpg",
        img / "Animals-Parrot.jpg",
    )
    out = video / "penguin_zebra_parrot_18s.mp4"
    run(
        [
            "ffmpeg",
            "-loglevel",
            "error",
            "-y",
            "-loop",
            "1",
            "-t",
            "6",
            "-i",
            str(penguin),
            "-loop",
            "1",
            "-t",
            "6",
            "-i",
            str(zebra),
            "-loop",
            "1",
            "-t",
            "6",
            "-i",
            str(parrot),
            "-filter_complex",
            "[0:v]scale=768:768,setsar=1[a];[1:v]scale=768:768,setsar=1[b];[2:v]scale=768:768,setsar=1[c];"
            "[a][b][c]concat=n=3:v=1:a=0,format=yuv420p",
            "-r",
            "24",
            str(out),
        ]
    )


def main():
    if len(sys.argv) < 2:
        sys.exit("usage: make_eval_corpus.py <testbed_dir>")
    tb = Path(sys.argv[1])
    synthetic = tb / "synthetic"
    size = monitor_size()
    print(f"== photos (Wikimedia) ==")
    download_photos(synthetic / "img", Path("/tmp/trowel-photo-cache"))
    print(f"== screenshots {size[0]}x{size[1]} ==")
    make_screenshots(synthetic / "screenshots", size)
    print("== docs ==")
    make_docs(synthetic / "docs")
    print("== long files ==")
    make_long(synthetic / "long")
    print("== lang (bn/ar) ==")
    make_lang(synthetic / "lang", size)
    print("== speech ==")
    make_speech(synthetic / "audio", synthetic / "lang")
    print("== video ==")
    make_video(synthetic / "img", synthetic / "video")
    print("corpus ready under", synthetic)


if __name__ == "__main__":
    main()
