//! Protocol v2: applications, clipboard and calculator rows. `PROTOCOL.md` §4 (v2),
//! `LAUNCHER-PLAN.md` §3.

use serde::{Deserialize, Serialize};

use crate::path::PathValue;

/// A `[Desktop Action x]` group of a `.desktop` file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppAction {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppRow {
    /// Desktop id without `.desktop` (`firefox`, `org.gnome.Nautilus`).
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generic: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    /// Resolved icon file. Absent when the theme scan found none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<PathValue>,
    /// The `Icon=` value, for a QML-side `Quickshell.iconPath` fallback.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_name: Option<String>,
    #[serde(default)]
    pub terminal: bool,
    #[serde(default)]
    pub actions: Vec<AppAction>,
    pub source: PathValue,
    #[serde(default)]
    pub launches: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_launched: Option<i64>,
    /// Highlight ranges in `name`, `[start, end)` in chars.
    #[serde(default)]
    pub matched: Vec<[u32; 2]>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppsResult {
    pub gen: u64,
    pub rows: Vec<AppRow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub took_us: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClipKind {
    Text,
    Link,
    Color,
    Code,
    Image,
    Files,
    Html,
}

impl ClipKind {
    pub const ALL: [ClipKind; 7] = [
        Self::Text,
        Self::Link,
        Self::Color,
        Self::Code,
        Self::Image,
        Self::Files,
        Self::Html,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Link => "link",
            Self::Color => "color",
            Self::Code => "code",
            Self::Image => "image",
            Self::Files => "files",
            Self::Html => "html",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == text)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClipRow {
    pub id: i64,
    pub kind: ClipKind,
    /// At most 200 chars, whitespace collapsed.
    pub preview: String,
    #[serde(default)]
    pub mimes: Vec<String>,
    pub bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumb: Option<PathValue>,
    /// Number of files, for `files` entries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub files: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    /// Hyprland `activewindow` class at capture time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_app: Option<String>,
    pub created: i64,
    pub last_used: i64,
    #[serde(default)]
    pub uses: u32,
    #[serde(default)]
    pub pinned: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClipQueryResult {
    pub gen: u64,
    pub rows: Vec<ClipRow>,
    pub total: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClipImage {
    pub file: PathValue,
    pub w: u32,
    pub h: u32,
}

/// `clip_get`: the full content for the preview. Text is capped at 256 KiB (`truncated`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClipContent {
    pub row: ClipRow,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub html: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<PathValue>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<ClipImage>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
}

/// How `clip_paste` / `calc_copy` hand content back (D21).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PasteMode {
    /// Put it on the clipboard and send the paste keystroke to the previous window.
    #[default]
    Paste,
    /// Only put it on the clipboard.
    Copy,
    /// Paste as plain text (drops HTML).
    Plain,
}

/// One MIME type that `trowel clip-capture` spooled to a file. Internal (`clip_ingest`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClipOffer {
    pub mime: String,
    pub file: PathValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CalcKind {
    Number,
    Unit,
    Base,
    Date,
    Duration,
}

/// An alternate representation for the action panel (`Hex`, `Binary`, another unit).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalcAlt {
    pub label: String,
    pub copy: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalcResult {
    pub expr: String,
    pub pretty_expr: String,
    /// Grouped with the user's `LC_NUMERIC`.
    pub display: String,
    /// What `Enter` copies: no grouping, full precision.
    pub copy: String,
    pub kind: CalcKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    /// The UI shows `≈`.
    #[serde(default)]
    pub approx: bool,
    #[serde(default)]
    pub alts: Vec<CalcAlt>,
}

/// `calc` reply. No `result` means "not a calculation": the UI shows no card.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalcReply {
    pub gen: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<CalcResult>,
    /// Only with `force` (the `=` view): why the expression is invalid.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub took_us: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalcHistoryRow {
    pub expr: String,
    pub result: String,
    pub at: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn app_row_roundtrip_drops_absent_fields() {
        let row = AppRow {
            id: "firefox".into(),
            name: "Firefox".into(),
            generic: Some("Web Browser".into()),
            comment: None,
            icon: None,
            icon_name: Some("firefox".into()),
            terminal: false,
            actions: vec![AppAction {
                id: "new-private-window".into(),
                name: "New Private Window".into(),
            }],
            source: PathValue::Utf8("/usr/share/applications/firefox.desktop".into()),
            launches: 14,
            last_launched: Some(1_791_869_000),
            matched: vec![[0, 4]],
        };
        let v = serde_json::to_value(&row).unwrap();
        assert!(v.get("comment").is_none());
        assert!(v.get("icon").is_none());
        assert_eq!(v["matched"], json!([[0, 4]]));
        assert_eq!(serde_json::from_value::<AppRow>(v).unwrap(), row);
    }

    #[test]
    fn clip_kinds_are_snake_case_and_closed() {
        for kind in ClipKind::ALL {
            let v = serde_json::to_value(kind).unwrap();
            assert_eq!(v, json!(kind.as_str()));
            assert_eq!(ClipKind::parse(kind.as_str()), Some(kind));
        }
        assert!(serde_json::from_value::<ClipKind>(json!("video")).is_err());
    }

    #[test]
    fn calc_reply_without_result_is_just_gen() {
        let reply = CalcReply {
            gen: 7,
            result: None,
            error: None,
            took_us: None,
        };
        assert_eq!(serde_json::to_value(reply).unwrap(), json!({"gen": 7}));
    }

    #[test]
    fn clip_content_omits_false_truncated() {
        let row: ClipRow = serde_json::from_value(json!({
            "id": 812, "kind": "text", "preview": "hi", "bytes": 2,
            "created": 1, "last_used": 1
        }))
        .unwrap();
        assert!(!row.pinned);
        let content = ClipContent {
            row,
            text: Some("hi".into()),
            html: None,
            files: None,
            image: None,
            truncated: false,
        };
        assert!(serde_json::to_value(content)
            .unwrap()
            .get("truncated")
            .is_none());
    }
}
