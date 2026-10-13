//! SearchHit → PROTOCOL result rows/groups. `upstream/Sources/DigUpApp/Results.swift`.

use std::collections::BTreeMap;
use std::path::Path;

use trowel_core::{FileKind, SearchHit, Searcher, SegmentKind};
use trowel_proto::{Meaning, ResultGroup, ResultRow, SearchResult};

pub fn result_set(hits: &[SearchHit], gen: u64, took_ms: u64, meaning: Meaning) -> SearchResult {
    let strong = Searcher::strong_count(hits, 0.5);
    let ranked: Vec<ResultRow> = hits.iter().take(strong).map(row_of).collect();
    let weaker: Vec<ResultRow> = hits.iter().skip(strong).map(row_of).collect();
    let mut order: Vec<String> = Vec::new();
    let mut by: BTreeMap<String, Vec<ResultRow>> = BTreeMap::new();
    for row in ranked {
        let key = group_kind(&row.kind);
        if !order.iter().any(|k| k == &key) {
            order.push(key.clone());
        }
        by.entry(key).or_default().push(row);
    }
    let groups = order
        .into_iter()
        .map(|kind| {
            let rows = by.remove(&kind).unwrap_or_default();
            let title = group_title(&kind);
            let total = rows.len() as u64;
            ResultGroup {
                kind,
                title,
                rows,
                total,
            }
        })
        .collect();
    SearchResult {
        gen,
        groups,
        weaker,
        took_ms,
        meaning,
    }
}

fn group_kind(kind: &str) -> String {
    match kind {
        "screenshot" => "screenshots",
        "image" => "images",
        "pdf" | "doc" => "documents",
        "video" => "video",
        "audio" => "audio",
        "code" => "code",
        other => other,
    }
    .into()
}

fn group_title(kind: &str) -> String {
    match kind {
        "screenshots" => "Screenshots",
        "images" => "Images",
        "documents" => "Documents",
        "video" => "Video",
        "audio" => "Audio",
        "code" => "Code",
        other => other,
    }
    .into()
}

fn row_of(hit: &SearchHit) -> ResultRow {
    let path = Path::new(&hit.path);
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| hit.path.clone());
    let folder = path
        .parent()
        .map(|p| tilde(&p.display().to_string()))
        .unwrap_or_default();
    let matched_meaning = hit.z.unwrap_or(0.0) >= Searcher::Z_GATE;
    let matched_words = hit.keyword_rank.is_some();
    ResultRow {
        id: trowel_proto::PathValue::Utf8(hit.path.clone()),
        name,
        folder,
        kind: hit.kind.as_str().into(),
        segment: hit.segment.as_str().into(),
        segment_id: hit.segment_id,
        loc: hit.loc,
        loc_end: hit.loc_end,
        focus: hit.focus.map(serde_json::Value::from),
        place: place(hit),
        excerpt: hit.excerpt.clone().filter(|s| !s.is_empty()),
        matched_meaning,
        matched_words,
        reason: Some(reason(hit, matched_meaning, matched_words)),
        modified: Some(hit.modified as i64),
        size: Some(hit.size),
        duration: hit.info.get("duration").and_then(|s| s.parse().ok()),
        info: serde_json::to_value(&hit.info).unwrap_or(serde_json::json!({})),
        thumb: Some(format!("thumb://{}@132", hit.segment_id)),
    }
}

fn reason(hit: &SearchHit, meaning: bool, words: bool) -> String {
    if !meaning {
        return if words {
            "Has your words".into()
        } else {
            "Matches its name".into()
        };
    }
    match hit.segment {
        SegmentKind::Image if hit.kind == FileKind::Screenshot => "Matches what's on screen".into(),
        SegmentKind::Image => "Matches the picture".into(),
        SegmentKind::Ocr => "Matches the text in it".into(),
        SegmentKind::Frame => "Matches the frame".into(),
        SegmentKind::Audio => "Matches what's heard".into(),
        SegmentKind::Page => "Matches the text on a page".into(),
        SegmentKind::PageImage => "Matches a page".into(),
        SegmentKind::Chunk => "Matches its text".into(),
        SegmentKind::Name => "Matches its name".into(),
        SegmentKind::Lines | SegmentKind::Cells => "Matches the code".into(),
    }
}

fn place(hit: &SearchHit) -> Option<String> {
    let loc = hit.loc?;
    Some(match hit.segment {
        SegmentKind::Page | SegmentKind::PageImage => format!("p. {}", loc as i64),
        SegmentKind::Frame => clock(loc),
        SegmentKind::Audio if hit.kind == FileKind::Video => format!("♪ {}", clock(loc)),
        SegmentKind::Audio => clock(loc),
        SegmentKind::Lines => {
            let end = hit.loc_end.unwrap_or(loc);
            if loc as i64 == end as i64 {
                format!("L{}", loc as i64)
            } else {
                format!("L{}–{}", loc as i64, end as i64)
            }
        }
        SegmentKind::Cells => {
            let end = hit.loc_end.unwrap_or(loc);
            if loc as i64 == end as i64 {
                format!("cell {}", loc as i64)
            } else {
                format!("cells {}–{}", loc as i64, end as i64)
            }
        }
        _ => return None,
    })
}

fn clock(seconds: f64) -> String {
    let total = seconds as i64;
    if total >= 3600 {
        format!(
            "{}:{:02}:{:02}",
            total / 3600,
            (total / 60) % 60,
            total % 60
        )
    } else {
        format!("{}:{:02}", total / 60, total % 60)
    }
}

fn tilde(path: &str) -> String {
    if let Ok(home) = std::env::var("HOME") {
        if let Some(rest) = path.strip_prefix(&home) {
            return format!("~{rest}");
        }
    }
    path.to_string()
}
