//! File/segment kinds, index options, and the `code:` query prefix.
//! String values are stored in the DB and must match `upstream/Sources/DigUpKit/Model.swift`.

use std::collections::BTreeSet;
use std::str::FromStr;

/// What a file is, for indexing and for grouping results.
/// `upstream/Sources/DigUpKit/Model.swift:4`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    Screenshot,
    Image,
    Pdf,
    Doc,
    Audio,
    Video,
    /// Source code, a repo's docs and notebooks: only in the code index.
    Code,
}

impl FileKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Screenshot => "screenshot",
            Self::Image => "image",
            Self::Pdf => "pdf",
            Self::Doc => "doc",
            Self::Audio => "audio",
            Self::Video => "video",
            Self::Code => "code",
        }
    }
}

impl FromStr for FileKind {
    type Err = ();

    fn from_str(raw: &str) -> Result<Self, ()> {
        match raw {
            "screenshot" => Ok(Self::Screenshot),
            "image" => Ok(Self::Image),
            "pdf" => Ok(Self::Pdf),
            "doc" => Ok(Self::Doc),
            "audio" => Ok(Self::Audio),
            "video" => Ok(Self::Video),
            "code" => Ok(Self::Code),
            _ => Err(()),
        }
    }
}

impl FileKind {
    /// The order of the first indexing pass: what people look for most comes first.
    /// `upstream/Sources/DigUpKit/Model.swift:10`
    pub const INDEXING_ORDER_SQL: &'static str = "\
CASE kind WHEN 'screenshot' THEN 0 WHEN 'image' THEN 1 WHEN 'pdf' THEN 2 WHEN 'doc' THEN 3
WHEN 'audio' THEN 4 WHEN 'video' THEN 5 ELSE 6 END";
}

/// One searchable piece of a file.
/// `upstream/Sources/DigUpKit/Model.swift:17`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SegmentKind {
    Name,
    Image,
    Ocr,
    Page,
    PageImage,
    Chunk,
    Audio,
    Frame,
    Lines,
    Cells,
}

impl SegmentKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Image => "image",
            Self::Ocr => "ocr",
            Self::Page => "page",
            Self::PageImage => "page_image",
            Self::Chunk => "chunk",
            Self::Audio => "audio",
            Self::Frame => "frame",
            Self::Lines => "lines",
            Self::Cells => "cells",
        }
    }
}

impl FromStr for SegmentKind {
    type Err = ();

    fn from_str(raw: &str) -> Result<Self, ()> {
        match raw {
            "name" => Ok(Self::Name),
            "image" => Ok(Self::Image),
            "ocr" => Ok(Self::Ocr),
            "page" => Ok(Self::Page),
            "page_image" => Ok(Self::PageImage),
            "chunk" => Ok(Self::Chunk),
            "audio" => Ok(Self::Audio),
            "frame" => Ok(Self::Frame),
            "lines" => Ok(Self::Lines),
            "cells" => Ok(Self::Cells),
            _ => Err(()),
        }
    }
}

/// What produced a segment's vector.
/// `upstream/Sources/DigUpKit/Model.swift:32`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Modality {
    None,
    Text,
    Image,
    Audio,
}

impl Modality {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Text => "text",
            Self::Image => "image",
            Self::Audio => "audio",
        }
    }
}

impl FromStr for Modality {
    type Err = ();

    fn from_str(raw: &str) -> Result<Self, ()> {
        match raw {
            "none" => Ok(Self::None),
            "text" => Ok(Self::Text),
            "image" => Ok(Self::Image),
            "audio" => Ok(Self::Audio),
            _ => Err(()),
        }
    }
}

/// Indexing settings. Anything that changes vectors is part of `fingerprint`.
/// `upstream/Sources/DigUpKit/Model.swift:37`
#[derive(Debug, Clone, PartialEq)]
pub struct IndexOptions {
    pub code: bool,
    pub code_stretch: i32,
    pub max_code_bytes: i64,
    pub max_notebook_bytes: i64,
    pub include_code: bool,
    pub include_datasets: bool,
    pub min_image_side: i32,
    pub big_pdf_pages: i32,
    pub pdf_step: i32,
    pub doc_step: i32,
    pub max_pages: i32,
    pub max_chunks: i32,
    pub min_page_text: i32,
    pub photo_budget: i32,
    pub screenshot_budget: i32,
    pub page_budget: i32,
    pub frame_budget: i32,
    pub frame_interval: f64,
    pub audio_window: f64,
    pub audio_hop: f64,
    pub excluded_folders: Vec<String>,
    pub excluded_extensions: BTreeSet<String>,
}

impl Default for IndexOptions {
    fn default() -> Self {
        Self {
            code: false,
            code_stretch: 1800,
            max_code_bytes: 1 << 20,
            max_notebook_bytes: 20 << 20,
            include_code: false,
            include_datasets: false,
            min_image_side: 256,
            big_pdf_pages: 50,
            pdf_step: 30,
            doc_step: 40,
            max_pages: 5_000,
            max_chunks: 5_000,
            min_page_text: 200,
            photo_budget: 140,
            screenshot_budget: 280,
            page_budget: 280,
            frame_budget: 70,
            frame_interval: 3.0,
            audio_window: 30.0,
            audio_hop: 25.0,
            excluded_folders: Vec::new(),
            excluded_extensions: BTreeSet::new(),
        }
    }
}

impl IndexOptions {
    /// File extensions as people write them ("HEIC, .txt *.md") → ["heic", "txt", "md"].
    /// `upstream/Sources/DigUpKit/Model.swift:74`
    pub fn extensions(text: &str) -> BTreeSet<String> {
        text.split(|c: char| c == ',' || c.is_whitespace())
            .map(|word| word.trim_start_matches(['*', '.']).to_lowercase())
            .filter(|s| !s.is_empty())
            .collect()
    }

    /// `upstream/Sources/DigUpKit/Model.swift:80`
    pub fn fingerprint(&self) -> String {
        if self.code {
            format!("code1 lines{}", self.code_stretch)
        } else {
            format!(
                "photo{} shot{} page{} frame{}/{:.1}s audio{:.1}/{:.1}s",
                self.photo_budget,
                self.screenshot_budget,
                self.page_budget,
                self.frame_budget,
                self.frame_interval,
                self.audio_window,
                self.audio_hop
            )
        }
    }
}

/// "code: retry with backoff": a search of the code index, and nothing else.
/// `upstream/Sources/DigUpKit/Model.swift:104`
pub struct CodeQuery;

impl CodeQuery {
    pub const PREFIX: &'static str = "code:";

    /// What's typed after "code:" ("" for "code:" alone), or None for a search that isn't one of code.
    pub fn text_of(query: &str) -> Option<String> {
        let trimmed = query.trim_start();
        let prefix_len = Self::PREFIX.len();
        if trimmed.len() < prefix_len {
            return None;
        }
        if !trimmed.is_char_boundary(prefix_len)
            || !trimmed[..prefix_len].eq_ignore_ascii_case(Self::PREFIX)
        {
            return None;
        }
        Some(trimmed[prefix_len..].trim().to_string())
    }
}

/// A file the crawler decided to index.
/// `upstream/Sources/DigUpKit/Crawler.swift:5`
#[derive(Debug, Clone)]
pub struct Candidate {
    pub path: String,
    pub kind: FileKind,
    pub size: i64,
    pub mtime: f64,
    pub inode: u64,
    pub device: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_strings_match_db_values() {
        assert_eq!(FileKind::Screenshot.as_str(), "screenshot");
        assert_eq!(FileKind::Pdf.as_str(), "pdf");
        assert_eq!(SegmentKind::PageImage.as_str(), "page_image");
        assert_eq!(Modality::None.as_str(), "none");
    }

    #[test]
    fn default_fingerprint_matches_upstream() {
        assert_eq!(
            IndexOptions::default().fingerprint(),
            "photo140 shot280 page280 frame70/3.0s audio30.0/25.0s"
        );
    }

    #[test]
    fn code_query_prefix() {
        assert_eq!(CodeQuery::text_of("code: retry"), Some("retry".into()));
        assert_eq!(CodeQuery::text_of("CODE:"), Some("".into()));
        assert_eq!(CodeQuery::text_of("payment"), None);
        assert_eq!(CodeQuery::text_of(" code: x"), Some("x".into()));
        assert_eq!(
            CodeQuery::text_of("code: retry with backoff"),
            Some("retry with backoff".into())
        );
        assert_eq!(
            CodeQuery::text_of("  Code:useDebounce"),
            Some("useDebounce".into())
        );
        assert_eq!(CodeQuery::text_of("code:"), Some("".into()));
        assert_eq!(CodeQuery::text_of("code review notes"), None);
        assert_eq!(CodeQuery::text_of("the code: 1234"), None);
    }

    #[test]
    fn extensions_parse() {
        let set = IndexOptions::extensions("HEIC, .txt *.md");
        assert!(set.contains("heic") && set.contains("txt") && set.contains("md"));
    }
}
