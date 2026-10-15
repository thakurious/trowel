//! Spooled offers → one [`NewEntry`] (`LAUNCHER-PLAN.md` L3 "Capture" step 2 order: files, then an
//! image, then HTML with its plain text, then text).

use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use trowel_proto::{ClipKind, PathValue};

use super::classify::{classify, link_domain};
use super::store::{blob_ext, BlobData, NewBlob, NewEntry};

/// Text kept in the row (and searched). Longer text keeps its full copy in a `txt` blob, and
/// `clip_get` caps what it returns at the same size (PROTOCOL §4).
pub const BODY_CAP: usize = 256 * 1024;
const PREVIEW_CHARS: usize = 200;

pub const FILE_MIMES: [&str; 2] = ["text/uri-list", "x-special/gnome-copied-files"];
pub const IMAGE_MIMES: [&str; 6] = [
    "image/png",
    "image/jpeg",
    "image/webp",
    "image/gif",
    "image/bmp",
    "image/tiff",
];
/// Best first. `wl-copy` and browsers offer all of them for the same text.
pub const TEXT_MIMES: [&str; 5] = [
    "text/plain;charset=utf-8",
    "UTF8_STRING",
    "text/plain",
    "STRING",
    "TEXT",
];
/// The page a browser copy came from (Chromium, Electron); gives `html` entries a domain.
pub const SOURCE_URL_MIME: &str = "chromium/x-source-url";
/// Password managers that mark their copies (KDE's convention, also what Omarchy checks).
pub const SENSITIVE_MIME: &str = "x-kde-passwordManagerHint";

#[derive(Debug, Clone)]
pub struct Offer {
    pub mime: String,
    pub path: PathBuf,
}

/// The MIME types `clip-capture` should fetch for this offer list, in the order above.
pub fn wanted(types: &[String]) -> Vec<String> {
    let has = |m: &str| types.iter().any(|t| t == m);
    let mut out = Vec::new();
    if let Some(m) = FILE_MIMES.iter().find(|m| has(m)) {
        out.push((*m).to_string());
        // A text fallback, for uri-lists that hold no file URIs (a browser's link drag).
    } else if let Some(m) = IMAGE_MIMES.iter().find(|m| has(m)) {
        return vec![(*m).to_string()];
    } else if has("text/html") {
        out.push("text/html".into());
        if has(SOURCE_URL_MIME) {
            out.push(SOURCE_URL_MIME.into());
        }
    }
    if let Some(m) = TEXT_MIMES.iter().find(|m| has(m)) {
        out.push((*m).to_string());
    } else if let Some(m) = types.iter().find(|t| t.starts_with("text/plain")) {
        out.push(m.clone());
    }
    out
}

pub fn sha256_hex(parts: &[&[u8]]) -> String {
    let mut h = Sha256::new();
    for (i, p) in parts.iter().enumerate() {
        if i > 0 {
            h.update([0u8]);
        }
        h.update(p);
    }
    format!("{:x}", h.finalize())
}

/// Whitespace collapsed, at most 200 chars.
pub fn preview_of(text: &str) -> String {
    let mut out = String::new();
    for word in text.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
        if out.chars().count() >= PREVIEW_CHARS {
            break;
        }
    }
    if out.chars().count() > PREVIEW_CHARS {
        let cut: String = out.chars().take(PREVIEW_CHARS - 1).collect();
        return format!("{}…", cut.trim_end());
    }
    out
}

/// Clipboard text bytes: UTF-16 when it carries a BOM, else UTF-8 (lossy).
pub fn decode_text(raw: &[u8]) -> String {
    let utf16 = |bytes: &[u8], le: bool| {
        let units: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&c| {
                if le {
                    u16::from_le_bytes(c)
                } else {
                    u16::from_be_bytes(c)
                }
            })
            .collect();
        String::from_utf16_lossy(&units)
    };
    match raw {
        [0xFF, 0xFE, rest @ ..] => utf16(rest, true),
        [0xFE, 0xFF, rest @ ..] => utf16(rest, false),
        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8_lossy(rest).into_owned(),
        _ => String::from_utf8_lossy(raw).into_owned(),
    }
}

/// `file://` URIs (percent-decoded) from a `text/uri-list` or `x-special/gnome-copied-files`
/// body. Comments, the `copy`/`cut` verb and non-file URIs are skipped.
pub fn file_paths(body: &[u8]) -> Vec<PathBuf> {
    use std::os::unix::ffi::OsStringExt;
    let text = String::from_utf8_lossy(body);
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let rest = l.strip_prefix("file://")?;
            let rest = rest.strip_prefix("localhost").unwrap_or(rest);
            if !rest.starts_with('/') {
                return None;
            }
            let bytes = percent_decode(rest.as_bytes());
            Some(PathBuf::from(std::ffi::OsString::from_vec(bytes)))
        })
        .collect()
}

fn percent_decode(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len());
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == b'%' && i + 2 < raw.len() {
            let hex = std::str::from_utf8(&raw[i + 1..i + 3]).ok();
            if let Some(b) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(raw[i]);
        i += 1;
    }
    out
}

/// Width and height from the header of a PNG, GIF, JPEG, WebP or BMP. TIFF and anything odd
/// answer `None` (the row still shows; the size is only for the preview).
pub fn image_size(b: &[u8]) -> Option<(u32, u32)> {
    let be32 = |o: usize| Some(u32::from_be_bytes(b.get(o..o + 4)?.try_into().ok()?));
    let le16 = |o: usize| {
        Some(u32::from(u16::from_le_bytes(
            b.get(o..o + 2)?.try_into().ok()?,
        )))
    };
    let le24 = |o: usize| {
        let s = b.get(o..o + 3)?;
        Some(u32::from(s[0]) | u32::from(s[1]) << 8 | u32::from(s[2]) << 16)
    };
    if b.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some((be32(16)?, be32(20)?));
    }
    if b.starts_with(b"GIF8") {
        return Some((le16(6)?, le16(8)?));
    }
    if b.starts_with(b"BM") {
        let w = i32::from_le_bytes(b.get(18..22)?.try_into().ok()?);
        let h = i32::from_le_bytes(b.get(22..26)?.try_into().ok()?);
        return Some((w.unsigned_abs(), h.unsigned_abs()));
    }
    if b.len() > 30 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        return match &b[12..16] {
            b"VP8 " => Some((le16(26)? & 0x3FFF, le16(28)? & 0x3FFF)),
            b"VP8L" => {
                let bits = u32::from_le_bytes(b.get(21..25)?.try_into().ok()?);
                Some(((bits & 0x3FFF) + 1, ((bits >> 14) & 0x3FFF) + 1))
            }
            b"VP8X" => Some((le24(24)? + 1, le24(27)? + 1)),
            _ => None,
        };
    }
    if b.starts_with(&[0xFF, 0xD8]) {
        let mut i = 2;
        while i + 9 < b.len() {
            if b[i] != 0xFF {
                i += 1;
                continue;
            }
            let marker = b[i + 1];
            if marker == 0xFF {
                i += 1;
                continue;
            }
            let len = usize::from(u16::from_be_bytes([b[i + 2], b[i + 3]]));
            // SOF0..SOF15 except DHT (C4), JPG (C8) and DAC (CC).
            if (0xC0..=0xCF).contains(&marker) && ![0xC4, 0xC8, 0xCC].contains(&marker) {
                let h = u32::from(u16::from_be_bytes([b[i + 5], b[i + 6]]));
                let w = u32::from(u16::from_be_bytes([b[i + 7], b[i + 8]]));
                return Some((w, h));
            }
            i += 2 + len;
        }
    }
    None
}

/// Tag strip for HTML copies that came without a plain-text offer: block tags become line
/// breaks, inline tags vanish, the common entities are decoded.
fn html_text(html: &str) -> String {
    const BLOCK: &[&str] = &[
        "p", "br", "div", "li", "tr", "h1", "h2", "h3", "h4", "h5", "h6", "pre", "table", "ul",
        "ol",
    ];
    let mut out = String::with_capacity(html.len() / 2);
    let mut tag: Option<String> = None;
    for c in html.chars() {
        match (&mut tag, c) {
            (None, '<') => tag = Some(String::new()),
            (Some(name), '>') => {
                let n = name.trim_start_matches('/').to_ascii_lowercase();
                let n = n.split([' ', '/']).next().unwrap_or("");
                if BLOCK.contains(&n) && !out.ends_with('\n') {
                    out.push('\n');
                }
                tag = None;
            }
            (Some(name), c) => name.push(c),
            (None, c) => out.push(c),
        }
    }
    out.replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

fn cap(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

fn read(offers: &[Offer], mime: &str, max: u64) -> Result<Option<Vec<u8>>, String> {
    let Some(offer) = offers.iter().find(|o| o.mime == mime) else {
        return Ok(None);
    };
    let len = fs::metadata(&offer.path).map_err(|e| e.to_string())?.len();
    if len > max {
        return Err(format!("{mime} is {len} bytes, over the {max}-byte cap"));
    }
    fs::read(&offer.path).map(Some).map_err(|e| e.to_string())
}

/// `None` when there is nothing worth keeping (empty, whitespace, unknown types only).
pub fn build(
    offers: &[Offer],
    source_app: Option<String>,
    at: i64,
    max_bytes: u64,
) -> Result<Option<NewEntry>, String> {
    let base = NewEntry {
        source_app: source_app.clone(),
        at,
        ..Default::default()
    };
    for mime in FILE_MIMES {
        if let Some(body) = read(offers, mime, max_bytes)? {
            let paths = file_paths(&body);
            if paths.is_empty() {
                continue;
            }
            return Ok(Some(files_entry(&paths, base)));
        }
    }
    for mime in IMAGE_MIMES {
        if let Some(bytes) = read(offers, mime, max_bytes)? {
            if bytes.is_empty() {
                return Ok(None);
            }
            return Ok(Some(image_entry(mime, bytes, base)));
        }
    }
    let text = TEXT_MIMES
        .iter()
        .map(|m| (*m).to_string())
        .chain(
            offers
                .iter()
                .map(|o| o.mime.clone())
                .filter(|m| m.starts_with("text/plain")),
        )
        .find_map(|m| read(offers, &m, max_bytes).transpose())
        .transpose()?
        .map(|raw| decode_text(&raw));
    if let Some(html) = read(offers, "text/html", max_bytes)?.map(|raw| decode_text(&raw)) {
        let plain = text.clone().unwrap_or_else(|| html_text(&html));
        if plain.trim().is_empty() {
            return Ok(None);
        }
        let class = classify(&plain, source_app.as_deref());
        if class.kind != ClipKind::Link {
            let page = read(offers, SOURCE_URL_MIME, 4096)?
                .map(|raw| decode_text(&raw))
                .and_then(|url| link_domain(url.trim()));
            return Ok(Some(NewEntry {
                hash: sha256_hex(&[b"html", html.as_bytes(), plain.as_bytes()]),
                kind: Some(ClipKind::Html),
                preview: preview_of(&plain),
                mimes: vec!["text/html".into(), "text/plain".into()],
                bytes: (html.len() + plain.len()) as u64,
                body: Some(cap(&plain, BODY_CAP).to_string()),
                html: Some(cap(&html, BODY_CAP).to_string()),
                domain: page,
                ..base
            }));
        }
        return Ok(Some(text_entry(&plain, base)));
    }
    match text {
        Some(t) if !t.trim().is_empty() => Ok(Some(text_entry(&t, base))),
        _ => Ok(None),
    }
}

pub fn text_entry(text: &str, base: NewEntry) -> NewEntry {
    let class = classify(text, base.source_app.as_deref());
    let bytes = text.as_bytes();
    let blob = (bytes.len() > BODY_CAP).then(|| NewBlob {
        sha256: sha256_hex(&[bytes]),
        ext: blob_ext(&[], class.kind),
        data: BlobData::Bytes(bytes.to_vec()),
    });
    NewEntry {
        hash: sha256_hex(&[b"text", bytes]),
        kind: Some(class.kind),
        preview: preview_of(cap(text, 4096)),
        mimes: vec!["text/plain".into()],
        bytes: bytes.len() as u64,
        body: Some(cap(text, BODY_CAP).to_string()),
        blob,
        color: class.color,
        domain: class.domain,
        ..base
    }
}

pub fn image_entry(mime: &str, bytes: Vec<u8>, base: NewEntry) -> NewEntry {
    let sha = sha256_hex(&[&bytes]);
    let size = image_size(&bytes);
    let preview = match size {
        Some((w, h)) => format!("Image {w}×{h}"),
        None => "Image".into(),
    };
    let mimes = vec![mime.to_string()];
    NewEntry {
        hash: format!("image:{sha}"),
        kind: Some(ClipKind::Image),
        preview,
        bytes: bytes.len() as u64,
        blob: Some(NewBlob {
            sha256: sha,
            ext: blob_ext(&mimes, ClipKind::Image),
            data: BlobData::Bytes(bytes),
        }),
        mimes,
        width: size.map(|s| s.0),
        height: size.map(|s| s.1),
        ..base
    }
}

fn files_entry(paths: &[PathBuf], base: NewEntry) -> NewEntry {
    use std::os::unix::ffi::OsStrExt;
    let names: Vec<String> = paths
        .iter()
        .map(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| p.to_string_lossy().into_owned())
        })
        .collect();
    let joined: Vec<&[u8]> = paths.iter().map(|p| p.as_os_str().as_bytes()).collect();
    let mut parts: Vec<&[u8]> = vec![b"files"];
    parts.extend(joined);
    let bytes = paths
        .iter()
        .filter_map(|p| fs::metadata(p).ok())
        .filter(|m| m.is_file())
        .map(|m| m.len())
        .sum();
    NewEntry {
        hash: sha256_hex(&parts),
        kind: Some(ClipKind::Files),
        preview: preview_of(&names.join(", ")),
        mimes: vec!["text/uri-list".into()],
        bytes,
        // File names are searchable.
        body: Some(names.join("\n")),
        files: Some(paths.iter().map(|p| PathValue::from_path(p)).collect()),
        ..base
    }
}

/// A single picture among the copied files gets a thumbnail like an image entry.
pub fn thumb_source(files: &[PathValue]) -> Option<PathBuf> {
    let [one] = files else { return None };
    let path = one.to_path_buf();
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    matches!(
        ext.as_str(),
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "tif" | "tiff" | "heic" | "avif"
    )
    .then_some(path)
    .filter(|p| Path::new(p).is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn types(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn wanted_follows_the_l0_offers() {
        // L0.2 (2026-10-15): Strata file copy.
        assert_eq!(
            wanted(&types(&[
                "text/plain;charset=utf-8",
                "text/uri-list",
                "application/vnd.portal.filetransfer",
                "application/vnd.portal.files"
            ])),
            types(&["text/uri-list", "text/plain;charset=utf-8"])
        );
        // Chrome "Copy Image": the image wins over its HTML.
        assert_eq!(
            wanted(&types(&[
                "text/x-moz-url",
                "text/html",
                "image/png",
                "chromium/x-source-url"
            ])),
            types(&["image/png"])
        );
        // Chrome formatted text.
        assert_eq!(
            wanted(&types(&[
                "text/plain",
                "text/plain;charset=utf-8",
                "UTF8_STRING",
                "chromium/x-internal-source-rfh-token",
                "TEXT",
                "chromium/x-source-url",
                "STRING",
                "text/html"
            ])),
            types(&[
                "text/html",
                "chromium/x-source-url",
                "text/plain;charset=utf-8"
            ])
        );
        assert_eq!(
            wanted(&types(&["UTF8_STRING", "STRING"])),
            types(&["UTF8_STRING"])
        );
        assert!(wanted(&types(&["application/x-foo"])).is_empty());
    }

    #[test]
    fn uri_lists_decode() {
        let body = b"# comment\r\nfile:///home/u/My%20File.pdf\r\nfile://localhost/tmp/a\r\nhttps://x.org/\r\n";
        assert_eq!(
            file_paths(body),
            vec![
                PathBuf::from("/home/u/My File.pdf"),
                PathBuf::from("/tmp/a")
            ]
        );
        assert_eq!(
            file_paths(b"copy\nfile:///tmp/b%"),
            vec![PathBuf::from("/tmp/b%")],
            "gnome-copied-files verb skipped, stray % kept"
        );
    }

    #[test]
    fn previews_collapse_and_cap() {
        assert_eq!(preview_of("  a\n\n b\tc "), "a b c");
        let long = "word ".repeat(100);
        let p = preview_of(&long);
        assert_eq!(p.chars().count(), 200);
        assert!(p.ends_with('…'));
    }

    #[test]
    fn utf16_with_bom_decodes() {
        let mut raw = vec![0xFF, 0xFE];
        for u in "héllo".encode_utf16() {
            raw.extend(u.to_le_bytes());
        }
        assert_eq!(decode_text(&raw), "héllo");
        assert_eq!(decode_text(b"plain"), "plain");
    }

    #[test]
    fn image_headers() {
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend(1920u32.to_be_bytes());
        png.extend(1200u32.to_be_bytes());
        assert_eq!(image_size(&png), Some((1920, 1200)));
        let gif = b"GIF89a\x40\x01\xf0\x00".to_vec();
        assert_eq!(image_size(&gif), Some((320, 240)));
        // A minimal JPEG: SOI, an APP0 segment, then SOF0 with 480×640.
        let jpeg = [
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x00, 0x00, 0xFF, 0xC0, 0x00, 0x11, 0x08, 0x02,
            0x80, 0x01, 0xE0, 0x03, 0x00, 0x00,
        ];
        assert_eq!(image_size(&jpeg), Some((480, 640)));
        assert_eq!(image_size(b"nope"), None);
    }

    #[test]
    fn html_without_plain_text() {
        assert_eq!(
            html_text("<p><b>Refund</b> policy &amp; terms</p><p>30&nbsp;days</p>").trim(),
            "Refund policy & terms\n30 days"
        );
    }

    #[test]
    fn long_text_keeps_a_full_copy() {
        let text = "x".repeat(BODY_CAP + 10);
        let e = text_entry(&text, NewEntry::default());
        assert_eq!(e.body.as_ref().unwrap().len(), BODY_CAP);
        assert_eq!(e.bytes, (BODY_CAP + 10) as u64);
        assert!(e.blob.is_some());
        let short = text_entry("hi", NewEntry::default());
        assert!(short.blob.is_none());
    }
}
