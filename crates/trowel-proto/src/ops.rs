//! Request operations. `PROTOCOL.md` §3–4.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::path::PathValue;
use crate::results::SearchMode;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub id: u64,
    #[serde(flatten)]
    pub op: Op,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Op {
    Hello {
        protocol: u32,
        client: String,
        #[serde(default)]
        pid: u32,
    },
    PanelOpened,
    Typing {
        text: String,
        #[serde(default)]
        code: bool,
    },
    Search {
        text: String,
        mode: SearchMode,
        #[serde(default)]
        code: bool,
        #[serde(default)]
        kinds: Option<Vec<String>>,
        #[serde(default = "default_limit")]
        limit: u32,
        #[serde(default)]
        prefix_last: bool,
        gen: u64,
    },
    Cancel {
        target_id: u64,
    },
    Thumbnail {
        #[serde(default)]
        segment_id: Option<i64>,
        #[serde(default)]
        path: Option<PathValue>,
        size: u32,
    },
    Preview {
        segment_id: i64,
        width: u32,
    },
    TextOfSegment {
        segment_id: i64,
    },
    Open {
        #[serde(default)]
        segment_id: Option<i64>,
        #[serde(default)]
        path: Option<PathValue>,
    },
    Reveal {
        path: PathValue,
    },
    CopyPath {
        path: PathValue,
    },
    QuickLook {
        segment_id: i64,
    },
    Subscribe {
        topics: Vec<String>,
    },
    Status,
    Pause,
    Resume,
    Sync {
        #[serde(default)]
        code: bool,
    },
    ConfigGet,
    ConfigSet {
        patch: Value,
    },
    Estimate {
        paths: Vec<PathValue>,
        #[serde(default)]
        code: bool,
    },
    ModelStatus,
    ModelDownload {
        action: String,
    },
    Doctor,
    SetHotkey {
        chord: String,
    },
}

fn default_limit() -> u32 {
    60
}

/// Closed set of `op` names in protocol v1. Anything else is `unknown_op`.
pub const KNOWN_OPS: &[&str] = &[
    "hello",
    "panel_opened",
    "typing",
    "search",
    "cancel",
    "thumbnail",
    "preview",
    "text_of_segment",
    "open",
    "reveal",
    "copy_path",
    "quick_look",
    "subscribe",
    "status",
    "pause",
    "resume",
    "sync",
    "config_get",
    "config_set",
    "estimate",
    "model_status",
    "model_download",
    "doctor",
    "set_hotkey",
];

impl Op {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Hello { .. } => "hello",
            Self::PanelOpened => "panel_opened",
            Self::Typing { .. } => "typing",
            Self::Search { .. } => "search",
            Self::Cancel { .. } => "cancel",
            Self::Thumbnail { .. } => "thumbnail",
            Self::Preview { .. } => "preview",
            Self::TextOfSegment { .. } => "text_of_segment",
            Self::Open { .. } => "open",
            Self::Reveal { .. } => "reveal",
            Self::CopyPath { .. } => "copy_path",
            Self::QuickLook { .. } => "quick_look",
            Self::Subscribe { .. } => "subscribe",
            Self::Status => "status",
            Self::Pause => "pause",
            Self::Resume => "resume",
            Self::Sync { .. } => "sync",
            Self::ConfigGet => "config_get",
            Self::ConfigSet { .. } => "config_set",
            Self::Estimate { .. } => "estimate",
            Self::ModelStatus => "model_status",
            Self::ModelDownload { .. } => "model_download",
            Self::Doctor => "doctor",
            Self::SetHotkey { .. } => "set_hotkey",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HelloOk {
    pub protocol: u32,
    pub daemon: String,
    pub capabilities: Vec<String>,
    pub state: String,
}
