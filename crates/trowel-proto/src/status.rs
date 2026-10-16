//! Status snapshot/event. `PROTOCOL.md` §4.

use serde::{Deserialize, Serialize};

use crate::path::PathValue;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Status {
    pub phase: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paused_reason: Option<String>,
    pub main: Progress,
    pub code: Progress,
    pub missing_roots: Vec<PathValue>,
    pub unreadable_roots: Vec<PathValue>,
    pub counts: Counts,
    pub worker: String,
    pub vectors_loaded: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    /// Clipboard incognito (Phase 8 L4): `clip_pause`'s `paused_until`, `-1` until resumed, a Unix
    /// time otherwise (past = capturing again). Absent while capturing.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub clip_paused_until: i64,
}

fn is_zero(v: &i64) -> bool {
    *v == 0
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Progress {
    pub done: u64,
    pub total: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Counts {
    pub screenshots: u64,
    pub images: u64,
    pub documents: u64,
    pub video: u64,
    pub audio: u64,
    pub code: u64,
}

impl Default for Status {
    fn default() -> Self {
        Self {
            phase: "idle".into(),
            paused_reason: None,
            main: Progress {
                done: 0,
                total: 0,
                current: None,
            },
            code: Progress {
                done: 0,
                total: 0,
                current: None,
            },
            missing_roots: Vec::new(),
            unreadable_roots: Vec::new(),
            counts: Counts {
                screenshots: 0,
                images: 0,
                documents: 0,
                video: 0,
                audio: 0,
                code: 0,
            },
            worker: "down".into(),
            vectors_loaded: false,
            last_error: None,
            clip_paused_until: 0,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ModelStatus {
    pub present: bool,
    pub verified: bool,
    pub bytes: u64,
    pub total: u64,
    pub downloading: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
