//! One-time import of Omarchy's `~/.local/state/omarchy/clipboard-history.json` (D20). The file is
//! newest first and has no real timestamps, so entries get `now`, `now - 1`, … The referenced
//! images are copied into Trowel's blob store; Omarchy's files are never touched.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::ingest::{image_entry, text_entry};
use super::store::NewEntry;

pub fn default_path() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_else(|| "/tmp".into()))
                .join(".local/state")
        })
        .join("omarchy/clipboard-history.json")
}

#[derive(Debug, Default)]
pub struct Parsed {
    /// Oldest first, so inserting in order keeps ids rising with time.
    pub entries: Vec<NewEntry>,
    /// Malformed items and images whose file is gone or too large.
    pub skipped: u64,
}

/// Mirrors `ClipboardHistory.js` `normalizeEntry`: a bare string or `{type:"text", text}` is
/// text, `{type:"image", path, mime}` is an image; anything else is skipped.
pub fn parse(file: &Path, now: i64, max_bytes: u64) -> Result<Parsed, String> {
    let raw = fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let items: Vec<Value> = serde_json::from_str(&raw)
        .map_err(|e| format!("{}: not a JSON list ({e})", file.display()))?;
    let mut out = Parsed::default();
    for (i, item) in items.iter().enumerate() {
        let base = NewEntry {
            at: now - i as i64,
            ..Default::default()
        };
        let kind = item
            .get("type")
            .or_else(|| item.get("kind"))
            .and_then(Value::as_str);
        let entry = match (item, kind) {
            (Value::String(text), _) => text_or_skip(text, base),
            (_, Some("text")) => item
                .get("text")
                .and_then(Value::as_str)
                .and_then(|t| text_or_skip(t, base)),
            (_, Some("image")) => {
                let path = item.get("path").and_then(Value::as_str).unwrap_or("");
                let mime = item
                    .get("mime")
                    .and_then(Value::as_str)
                    .unwrap_or("image/png");
                let ok = fs::metadata(path)
                    .map(|m| m.is_file() && m.len() > 0 && m.len() <= max_bytes)
                    .unwrap_or(false);
                ok.then(|| fs::read(path).ok())
                    .flatten()
                    .map(|bytes| image_entry(mime, bytes, base))
            }
            _ => None,
        };
        match entry {
            Some(e) => out.entries.push(e),
            None => out.skipped += 1,
        }
    }
    out.entries.reverse();
    Ok(out)
}

fn text_or_skip(text: &str, base: NewEntry) -> Option<NewEntry> {
    (!text.trim().is_empty()).then(|| text_entry(text, base))
}

#[cfg(test)]
mod tests {
    use super::*;
    use trowel_proto::ClipKind;

    #[test]
    fn omarchy_history_converts_newest_first() {
        let dir = std::env::temp_dir().join(format!("trowel-import-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let img = dir.join("abc.png");
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend(4u32.to_be_bytes());
        png.extend(3u32.to_be_bytes());
        fs::write(&img, &png).unwrap();
        let json = serde_json::json!([
            {"type": "text", "text": "newest https://x.org"},
            {"type": "image", "mime": "image/png", "path": img, "capturedAt": "Tuesday 14:03"},
            "a bare string",
            {"type": "image", "path": dir.join("gone.png")},
            {"type": "text", "text": "   "},
            {"type": "weird"},
            {"type": "text", "text": "#F2A22A"}
        ]);
        let file = dir.join("clipboard-history.json");
        fs::write(&file, json.to_string()).unwrap();
        let parsed = parse(&file, 1000, 1 << 20).unwrap();
        assert_eq!(parsed.skipped, 3);
        let kinds: Vec<_> = parsed.entries.iter().map(|e| (e.kind, e.at)).collect();
        assert_eq!(
            kinds,
            vec![
                (Some(ClipKind::Color), 994),
                (Some(ClipKind::Text), 998),
                (Some(ClipKind::Image), 999),
                (Some(ClipKind::Text), 1000),
            ]
        );
        assert_eq!(parsed.entries[2].preview, "Image 4×3");
        assert!(img.is_file(), "Omarchy's image stays");
        let _ = fs::remove_dir_all(&dir);
    }
}
