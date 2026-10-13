//! Thumbnails and segment text. PROTOCOL.md §4.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde_json::json;
use trowel_core::{FileKind, MediaInfo, PdfExtractor, SearchText, Searcher, SegmentKind};

pub const SIZES: [u32; 4] = [64, 132, 264, 528];

pub struct SegmentMeta {
    pub path: PathBuf,
    pub file_kind: FileKind,
    pub segment: SegmentKind,
    pub loc: Option<f64>,
}

pub fn segment_meta(searcher: &Searcher, id: i64) -> Option<SegmentMeta> {
    let mut found = None;
    searcher
        .store
        .db
        .query(
            "SELECT f.path, f.kind, s.kind, s.loc FROM segments s JOIN files f ON f.id = s.file WHERE s.id = ?",
            &[trowel_core::SqlValue::Int(id)],
            |row| {
                let path = row.text(0).unwrap_or_default();
                let file_kind = row
                    .text(1)
                    .as_deref()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(FileKind::Doc);
                let segment = row
                    .text(2)
                    .as_deref()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(SegmentKind::Name);
                let loc = if row.is_null(3) {
                    None
                } else {
                    Some(row.double(3))
                };
                found = Some(SegmentMeta {
                    path: PathBuf::from(path),
                    file_kind,
                    segment,
                    loc,
                });
                Ok(())
            },
        )
        .ok()?;
    found
}

pub fn text_of(searcher: &Searcher, id: i64) -> serde_json::Value {
    let mut original = None;
    let mut body = None;
    if searcher
        .store
        .db
        .query(
            "SELECT original, body FROM fts WHERE rowid = ?",
            &[trowel_core::SqlValue::Int(id)],
            |row| {
                original = row.text(0);
                body = row.text(1);
                Ok(())
            },
        )
        .is_err()
    {
        return json!({"error": {"code": "io", "message": "fts"}});
    }
    let text = original.clone().or(body);
    match text {
        Some(text) => json!({"text": text, "original": original}),
        None => json!({"error": {"code": "not_found", "message": "unknown segment"}}),
    }
}

pub fn thumbnail(
    searcher: &Searcher,
    segment_id: Option<i64>,
    path: Option<&Path>,
    size: u32,
) -> serde_json::Value {
    let size = nearest_size(size);
    let src = if let Some(id) = segment_id {
        segment_meta(searcher, id).map(|m| m.path)
    } else {
        path.map(Path::to_path_buf)
    };
    let Some(src) = src else {
        return json!({"error": {"code": "not_found", "message": "no file"}});
    };
    if let Some(cached) = freedesktop_thumb(&src, size) {
        return json!({"file": cached});
    }
    match render_thumb(&src, size) {
        Ok(file) => json!({"file": file}),
        Err(err) => json!({"error": {"code": "io", "message": err}}),
    }
}

pub fn preview(
    searcher: &Searcher,
    segment_id: i64,
    width: u32,
    query: Option<&str>,
) -> serde_json::Value {
    let Some(meta) = segment_meta(searcher, segment_id) else {
        return json!({"error": {"code": "not_found", "message": "unknown segment"}});
    };
    let text = searcher.text_of_segment(segment_id).ok().flatten();
    let terms: Vec<String> = query.map(Searcher::terms).unwrap_or_default();
    match meta.segment {
        SegmentKind::Image | SegmentKind::Ocr => {
            let file = render_thumb(&meta.path, nearest_size(width)).ok();
            json!({"type": "image", "file": file, "text": text})
        }
        SegmentKind::Page | SegmentKind::PageImage => {
            let page = meta.loc.map(|n| n.max(1.0) as i32).unwrap_or(1);
            let file = PdfExtractor::render(&meta.path, page, width.max(64) as i32, &preview_dir())
                .ok()
                .map(|p| remember_preview(&p));
            let band = text
                .as_deref()
                .and_then(|t| pdf_band(t, &terms))
                .map(|b| json!([b[0], b[1]]))
                .unwrap_or(serde_json::Value::Null);
            json!({"type": "pdf_page", "file": file, "text": text, "band": band})
        }
        SegmentKind::Lines | SegmentKind::Cells => {
            let lines = text.as_deref().map(code_lines);
            json!({
                "type": "code",
                "text": text,
                "lines": lines,
                "focus_line": meta.loc,
            })
        }
        SegmentKind::Audio => {
            let peaks = audio_peaks(&meta.path);
            let duration = MediaInfo::duration(&meta.path);
            json!({
                "type": "audio",
                "text": text,
                "start": meta.loc,
                "duration": duration,
                "peaks": peaks,
            })
        }
        SegmentKind::Frame => {
            let start = meta.loc.unwrap_or(0.0);
            let frame = video_frame(&meta.path, start, width).map(|p| remember_preview(&p));
            let duration = MediaInfo::duration(&meta.path);
            json!({
                "type": "video",
                "text": text,
                "start": start,
                "duration": duration,
                "frame": frame,
            })
        }
        SegmentKind::Chunk | SegmentKind::Name => json!({"type": "text", "text": text}),
    }
}

/// Matched band in page coordinates 0–1. `upstream/Sources/DigUpApp/Previews.swift` page marks.
pub fn pdf_band(text: &str, words: &[String]) -> Option<[f64; 2]> {
    if text.is_empty() {
        return None;
    }
    let len = text.len() as f64;
    let mut first: Option<f64> = None;
    for word in words {
        if word.chars().count() < 2 {
            continue;
        }
        if let Some(range) = SearchText::matches(word, text).into_iter().next() {
            let y = range.start as f64 / len;
            first = Some(first.map_or(y, |p| p.min(y)));
        }
    }
    let y = first?;
    let height = 0.40;
    let top = (y - height * 0.55).clamp(0.0, 1.0 - height);
    Some([top, (top + height).min(1.0)])
}

fn code_lines(text: &str) -> Vec<serde_json::Value> {
    text.lines()
        .take(200)
        .enumerate()
        .map(|(i, line)| json!({"n": i + 1, "text": line}))
        .collect()
}

fn audio_peaks(path: &Path) -> Vec<f32> {
    let output = std::process::Command::new("ffmpeg")
        .args([
            "-nostdin",
            "-loglevel",
            "error",
            "-i",
            &path.to_string_lossy(),
            "-ac",
            "1",
            "-ar",
            "8000",
            "-f",
            "s16le",
            "-",
        ])
        .output();
    let Ok(out) = output else {
        return Vec::new();
    };
    if !out.status.success() || out.stdout.len() < 4 {
        return Vec::new();
    }
    let samples: Vec<i16> = out
        .stdout
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| i16::from_le_bytes(*b))
        .collect();
    if samples.is_empty() {
        return Vec::new();
    }
    let buckets = 400usize;
    let chunk = (samples.len() / buckets).max(1);
    let mut peaks = Vec::with_capacity(buckets);
    for i in 0..buckets {
        let start = i * chunk;
        if start >= samples.len() {
            break;
        }
        let end = (start + chunk).min(samples.len());
        let peak = samples[start..end]
            .iter()
            .map(|s| (*s as f32).abs())
            .fold(0.0_f32, f32::max)
            / 32768.0;
        peaks.push(peak.min(1.0));
    }
    peaks
}

fn video_frame(path: &Path, start: f64, width: u32) -> Option<PathBuf> {
    let dir = preview_dir();
    let _ = std::fs::create_dir_all(&dir);
    let dest = dir.join(format!(
        "frame-{}-{start:.2}-{width}.jpg",
        path.to_string_lossy().replace(['/', '.'], "_")
    ));
    if dest.is_file() {
        return Some(dest);
    }
    let vf = format!("scale='min({width},iw)':-2");
    let status = std::process::Command::new("ffmpeg")
        .args([
            "-nostdin",
            "-loglevel",
            "error",
            "-ss",
            &format!("{start:.3}"),
            "-i",
            &path.to_string_lossy(),
            "-frames:v",
            "1",
            "-q:v",
            "3",
            "-vf",
            &vf,
            "-y",
            &dest.to_string_lossy(),
        ])
        .status()
        .ok()?;
    status.success().then_some(dest).filter(|p| p.is_file())
}

const PREVIEW_BUDGET: u64 = 256 << 20;

fn preview_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var_os("HOME").unwrap_or_else(|| "/tmp".into());
            PathBuf::from(home).join(".cache")
        });
    base.join("trowel/previews")
}

fn remember_preview(path: &Path) -> PathBuf {
    evict_previews();
    path.to_path_buf()
}

fn evict_previews() {
    let dir = preview_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    let mut files: Vec<(PathBuf, u64, SystemTime)> = Vec::new();
    let mut total = 0u64;
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }
        let len = meta.len();
        total += len;
        files.push((path, len, meta.modified().unwrap_or(SystemTime::UNIX_EPOCH)));
    }
    if total <= PREVIEW_BUDGET {
        return;
    }
    files.sort_by_key(|f| f.2);
    for (path, len, _) in files {
        if total <= PREVIEW_BUDGET {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(len);
        }
    }
}

fn nearest_size(size: u32) -> u32 {
    SIZES
        .iter()
        .copied()
        .min_by_key(|s| s.abs_diff(size.max(1)))
        .unwrap_or(132)
}

fn cache_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var_os("HOME").unwrap_or_else(|| "/tmp".into());
            PathBuf::from(home).join(".cache")
        });
    base.join("trowel/thumbs")
}

fn render_thumb(src: &Path, size: u32) -> Result<PathBuf, String> {
    let dir = cache_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let name = format!(
        "{}-{size}.jpg",
        src.to_string_lossy().replace(['/', '.'], "_")
    );
    let dest = dir.join(name);
    if dest.is_file() {
        return Ok(dest);
    }
    let status = std::process::Command::new("vips")
        .args([
            "thumbnail",
            src.to_str().unwrap_or(""),
            &format!("{}[Q=90]", dest.display()),
            &size.to_string(),
        ])
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("vips thumbnail failed".into());
    }
    Ok(dest)
}

fn freedesktop_thumb(src: &Path, size: u32) -> Option<PathBuf> {
    let abs = src.canonicalize().ok()?;
    let uri = format!("file://{}", abs.display());
    let digest = md5_hex(uri.as_bytes());
    let home = std::env::var_os("HOME")?;
    let cache = PathBuf::from(home).join(".cache/thumbnails");
    let bucket = if size <= 128 {
        "normal"
    } else if size <= 256 {
        "large"
    } else {
        "x-large"
    };
    let path = cache.join(bucket).join(format!("{digest}.png"));
    path.is_file().then_some(path)
}

/// Tiny MD5 for freedesktop thumbnail names (not a security hash).
fn md5_hex(bytes: &[u8]) -> String {
    let mut state = Md5::new();
    state.consume(bytes);
    state.finish()
}

struct Md5 {
    s: [u32; 4],
    buf: Vec<u8>,
    len: u64,
}

impl Md5 {
    fn new() -> Self {
        Self {
            s: [0x6745_2301, 0xEFCD_AB89, 0x98BA_DCFE, 0x1032_5476],
            buf: Vec::new(),
            len: 0,
        }
    }
    fn consume(&mut self, data: &[u8]) {
        self.len += data.len() as u64;
        self.buf.extend_from_slice(data);
        while self.buf.len() >= 64 {
            let block: [u8; 64] = self.buf[..64].try_into().unwrap();
            self.buf.drain(..64);
            self.block(&block);
        }
    }
    fn finish(mut self) -> String {
        let bit_len = self.len * 8;
        self.buf.push(0x80);
        while self.buf.len() % 64 != 56 {
            self.buf.push(0);
        }
        self.buf.extend_from_slice(&bit_len.to_le_bytes());
        while self.buf.len() >= 64 {
            let block: [u8; 64] = self.buf[..64].try_into().unwrap();
            self.buf.drain(..64);
            self.block(&block);
        }
        let mut out = String::new();
        for word in self.s {
            for byte in word.to_le_bytes() {
                out.push_str(&format!("{byte:02x}"));
            }
        }
        out
    }
    fn block(&mut self, block: &[u8; 64]) {
        fn ff(x: u32, y: u32, z: u32) -> u32 {
            (x & y) | (!x & z)
        }
        fn gg(x: u32, y: u32, z: u32) -> u32 {
            (x & z) | (y & !z)
        }
        fn hh(x: u32, y: u32, z: u32) -> u32 {
            x ^ y ^ z
        }
        fn ii(x: u32, y: u32, z: u32) -> u32 {
            y ^ (x | !z)
        }
        let mut m = [0u32; 16];
        for (i, chunk) in block.as_chunks::<4>().0.iter().enumerate() {
            m[i] = u32::from_le_bytes(*chunk);
        }
        let mut a = self.s[0];
        let mut b = self.s[1];
        let mut c = self.s[2];
        let mut d = self.s[3];
        const T: [u32; 64] = [
            0xd76a_a478,
            0xe8c7_b756,
            0x2420_70db,
            0xc1bd_ceee,
            0xf57c_0faf,
            0x4787_c62a,
            0xa830_4613,
            0xfd46_9501,
            0x6980_98d8,
            0x8b44_f7af,
            0xffff_5bb1,
            0x895c_d7be,
            0x6b90_1122,
            0xfd98_7193,
            0xa679_438e,
            0x49b4_0821,
            0xf61e_2562,
            0xc040_b340,
            0x265e_5a51,
            0xe9b6_c7aa,
            0xd62f_105d,
            0x0244_1453,
            0xd8a1_e681,
            0xe7d3_fbc8,
            0x21e1_cde6,
            0xc337_07d6,
            0xf4d5_0d87,
            0x455a_14ed,
            0xa9e3_e905,
            0xfcef_a3f8,
            0x676f_02d9,
            0x8d2a_4c8a,
            0xfffa_3942,
            0x8771_f681,
            0x6d9d_6122,
            0xfde5_380c,
            0xa4be_ea44,
            0x4bde_cfa9,
            0xf6bb_4b60,
            0xbebf_bc70,
            0x289b_7ec6,
            0xeaa1_27fa,
            0xd4ef_3085,
            0x0488_1d05,
            0xd9d4_d039,
            0xe6db_99e5,
            0x1fa2_7cf8,
            0xc4ac_5665,
            0xf429_2244,
            0x432a_ff97,
            0xab94_23a7,
            0xfc93_a039,
            0x655b_59c3,
            0x8f0c_cc92,
            0xffef_f47d,
            0x8584_5dd1,
            0x6fa8_7e4f,
            0xfe2c_e6e0,
            0xa301_4314,
            0x4e08_11a1,
            0xf753_7e82,
            0xbd3a_f235,
            0x2ad7_d2bb,
            0xeb86_d391,
        ];
        const S: [u32; 64] = [
            7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20,
            5, 9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23,
            6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
        ];
        for i in 0..64 {
            let (f, g) = match i {
                0..=15 => (ff(b, c, d), i),
                16..=31 => (gg(b, c, d), (5 * i + 1) % 16),
                32..=47 => (hh(b, c, d), (3 * i + 5) % 16),
                _ => (ii(b, c, d), (7 * i) % 16),
            };
            let f = f.wrapping_add(a).wrapping_add(T[i]).wrapping_add(m[g]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(f.rotate_left(S[i]));
        }
        self.s[0] = self.s[0].wrapping_add(a);
        self.s[1] = self.s[1].wrapping_add(b);
        self.s[2] = self.s[2].wrapping_add(c);
        self.s[3] = self.s[3].wrapping_add(d);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn md5_empty() {
        assert_eq!(md5_hex(b""), "d41d8cd98f00b204e9800998ecf8427e");
    }

    #[test]
    fn nearest_round() {
        assert_eq!(nearest_size(128), 132);
        assert_eq!(nearest_size(64), 64);
    }

    #[test]
    fn pdf_band_covers_the_match() {
        let text = "alpha\nbeta harbour meeting\ngamma";
        let band = pdf_band(text, &["harbour".into()]).expect("band");
        assert!(band[0] < band[1]);
        assert!((0.0..=1.0).contains(&band[0]));
        assert!((0.0..=1.0).contains(&band[1]));
        let y = text.find("harbour").unwrap() as f64 / text.len() as f64;
        assert!(band[0] <= y && y <= band[1], "{band:?} y={y}");
    }
}
