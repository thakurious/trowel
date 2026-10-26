//! Request operations. `PROTOCOL.md` §3–4.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::desk::{IslandActivity, KillSignal, ProcGroup, ProcSort, Snippet};
use crate::launcher::{ClipKind, ClipOffer, PasteMode};
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
        #[serde(default)]
        code: bool,
    },
    Preview {
        segment_id: i64,
        width: u32,
        #[serde(default)]
        code: bool,
    },
    TextOfSegment {
        segment_id: i64,
        #[serde(default)]
        code: bool,
    },
    Open {
        #[serde(default)]
        segment_id: Option<i64>,
        #[serde(default)]
        path: Option<PathValue>,
        #[serde(default)]
        code: bool,
        #[serde(default)]
        loc: Option<f64>,
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
    // Protocol v2 (D22): applications, clipboard, calculator. `LAUNCHER-PLAN.md` §3.
    /// Supersedes the connection's older `apps_query`. Empty `text` is frecency order.
    AppsQuery {
        #[serde(default)]
        text: String,
        #[serde(default = "default_apps_limit")]
        limit: u32,
        gen: u64,
        /// Command ids whose learned signals to return with the rows (`AppsResult::commands`).
        #[serde(default)]
        commands: Vec<String>,
    },
    // Params are `app_id` / `clip_id`: a bare `id` would collide with the envelope's request id.
    AppLaunch {
        app_id: String,
        #[serde(default)]
        action: Option<String>,
        /// What was typed when the app was picked; learned as a search term.
        #[serde(default)]
        query: Option<String>,
    },
    /// A built-in command ran from the search (learned like an app launch).
    CommandRun {
        command_id: String,
        #[serde(default)]
        query: Option<String>,
    },
    AppHide {
        app_id: String,
        hidden: bool,
    },
    ClipQuery {
        #[serde(default)]
        text: String,
        #[serde(default)]
        kinds: Option<Vec<ClipKind>>,
        #[serde(default = "default_clip_limit")]
        limit: u32,
        #[serde(default)]
        offset: u32,
        gen: u64,
    },
    ClipGet {
        clip_id: i64,
    },
    /// The UI hides the panel before sending this (D21).
    ClipPaste {
        clip_id: i64,
        #[serde(default)]
        mode: PasteMode,
    },
    ClipPin {
        clip_id: i64,
        pinned: bool,
    },
    ClipDelete {
        clip_id: i64,
    },
    ClipClear {
        #[serde(default = "yes")]
        keep_pinned: bool,
    },
    /// Incognito. `minutes: 0` resumes; absent pauses until resumed.
    ClipPause {
        #[serde(default)]
        minutes: Option<u32>,
    },
    /// Internal: `trowel clip-capture` hands over spooled offers. Refused for other peers.
    ClipIngest {
        types: Vec<String>,
        offers: Vec<ClipOffer>,
        #[serde(default)]
        source_app: Option<String>,
    },
    /// Copies Omarchy's clipboard history into Trowel's (D20). Existing entries are left alone.
    ClipImport {
        #[serde(default = "omarchy")]
        source: String,
    },
    /// Supersedes the connection's older `calc`.
    Calc {
        text: String,
        #[serde(default)]
        force: bool,
        gen: u64,
    },
    CalcHistory {
        #[serde(default = "default_calc_history_limit")]
        limit: u32,
    },
    /// Copies or pastes a result through the clipboard path (D21); `expr` is recorded in history.
    CalcCopy {
        text: String,
        #[serde(default)]
        expr: Option<String>,
        #[serde(default)]
        mode: PasteMode,
    },
    // ---- v3 (Phase 9, PHASE9-PLAN §3) ----
    /// Adds or replaces an activity; subscribers to `island` get the new list.
    IslandPost {
        activity: IslandActivity,
    },
    IslandDismiss {
        activity_id: String,
    },
    /// A button on an expanded activity was pressed; routed to its producer.
    IslandAction {
        activity_id: String,
        action: String,
    },
    IslandList,
    /// `proc` events every `interval_ms` on this connection until `proc_unsubscribe` or close.
    ProcSubscribe {
        #[serde(default = "default_proc_interval")]
        interval_ms: u32,
        #[serde(default)]
        sort: ProcSort,
        #[serde(default)]
        group: ProcGroup,
        #[serde(default)]
        all_users: bool,
    },
    ProcUnsubscribe,
    ProcDetail {
        pid: i32,
    },
    /// `confirm` carries the typed name for a protected process (P-D30).
    ProcKill {
        pids: Vec<i32>,
        signal: KillSignal,
        #[serde(default)]
        confirm: Option<String>,
    },
    SystemCommands,
    SystemRun {
        command_id: String,
    },
    /// `text` ("25m tea", "at 15:30") or an explicit `seconds` + `label`.
    TimerStart {
        #[serde(default)]
        text: Option<String>,
        #[serde(default)]
        seconds: Option<f64>,
        #[serde(default)]
        label: Option<String>,
    },
    TimerParse {
        text: String,
    },
    TimerList,
    TimerPause {
        timer_id: i64,
    },
    TimerResume {
        timer_id: i64,
    },
    TimerAdd {
        timer_id: i64,
        seconds: f64,
    },
    TimerCancel {
        timer_id: i64,
    },
    SnippetQuery {
        #[serde(default)]
        text: String,
        #[serde(default = "default_snippet_limit")]
        limit: u32,
    },
    SnippetGet {
        snippet_id: i64,
    },
    /// `id` 0 creates; otherwise updates.
    SnippetSave {
        snippet: Snippet,
    },
    SnippetDelete {
        snippet_id: i64,
    },
    /// Expands placeholders (`args` fills `{argument:name}`) and pastes or copies (D21 path).
    SnippetPaste {
        snippet_id: i64,
        #[serde(default)]
        mode: PasteMode,
        #[serde(default)]
        args: std::collections::BTreeMap<String, String>,
    },
    SnippetImport {
        path: String,
    },
    SnippetExport {
        path: String,
    },
}

fn default_proc_interval() -> u32 {
    1000
}

fn default_snippet_limit() -> u32 {
    50
}

fn default_limit() -> u32 {
    60
}

fn default_apps_limit() -> u32 {
    8
}

fn default_clip_limit() -> u32 {
    50
}

fn default_calc_history_limit() -> u32 {
    20
}

fn omarchy() -> String {
    "omarchy".into()
}

fn yes() -> bool {
    true
}

/// Closed set of `op` names (v3). Anything else is `unknown_op`.
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
    "apps_query",
    "app_launch",
    "app_hide",
    "command_run",
    "clip_query",
    "clip_get",
    "clip_paste",
    "clip_pin",
    "clip_delete",
    "clip_clear",
    "clip_pause",
    "clip_ingest",
    "clip_import",
    "calc",
    "calc_history",
    "calc_copy",
    "island_post",
    "island_dismiss",
    "island_action",
    "island_list",
    "proc_subscribe",
    "proc_unsubscribe",
    "proc_detail",
    "proc_kill",
    "system_commands",
    "system_run",
    "timer_start",
    "timer_parse",
    "timer_list",
    "timer_pause",
    "timer_resume",
    "timer_add",
    "timer_cancel",
    "snippet_query",
    "snippet_get",
    "snippet_save",
    "snippet_delete",
    "snippet_paste",
    "snippet_import",
    "snippet_export",
];

impl Op {
    /// Ops added in protocol v2 (apps, clipboard, calculator).
    pub fn is_v2(&self) -> bool {
        matches!(
            self,
            Self::AppsQuery { .. }
                | Self::AppLaunch { .. }
                | Self::AppHide { .. }
                | Self::CommandRun { .. }
                | Self::ClipQuery { .. }
                | Self::ClipGet { .. }
                | Self::ClipPaste { .. }
                | Self::ClipPin { .. }
                | Self::ClipDelete { .. }
                | Self::ClipClear { .. }
                | Self::ClipPause { .. }
                | Self::ClipIngest { .. }
                | Self::ClipImport { .. }
                | Self::Calc { .. }
                | Self::CalcHistory { .. }
                | Self::CalcCopy { .. }
        )
    }

    /// Ops added in protocol v3 (Phase 9).
    pub fn is_v3(&self) -> bool {
        matches!(
            self,
            Self::IslandPost { .. }
                | Self::IslandDismiss { .. }
                | Self::IslandAction { .. }
                | Self::IslandList
                | Self::ProcSubscribe { .. }
                | Self::ProcUnsubscribe
                | Self::ProcDetail { .. }
                | Self::ProcKill { .. }
                | Self::SystemCommands
                | Self::SystemRun { .. }
                | Self::TimerStart { .. }
                | Self::TimerParse { .. }
                | Self::TimerList
                | Self::TimerPause { .. }
                | Self::TimerResume { .. }
                | Self::TimerAdd { .. }
                | Self::TimerCancel { .. }
                | Self::SnippetQuery { .. }
                | Self::SnippetGet { .. }
                | Self::SnippetSave { .. }
                | Self::SnippetDelete { .. }
                | Self::SnippetPaste { .. }
                | Self::SnippetImport { .. }
                | Self::SnippetExport { .. }
        )
    }

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
            Self::AppsQuery { .. } => "apps_query",
            Self::AppLaunch { .. } => "app_launch",
            Self::AppHide { .. } => "app_hide",
            Self::CommandRun { .. } => "command_run",
            Self::ClipQuery { .. } => "clip_query",
            Self::ClipGet { .. } => "clip_get",
            Self::ClipPaste { .. } => "clip_paste",
            Self::ClipPin { .. } => "clip_pin",
            Self::ClipDelete { .. } => "clip_delete",
            Self::ClipClear { .. } => "clip_clear",
            Self::ClipPause { .. } => "clip_pause",
            Self::ClipIngest { .. } => "clip_ingest",
            Self::ClipImport { .. } => "clip_import",
            Self::Calc { .. } => "calc",
            Self::CalcHistory { .. } => "calc_history",
            Self::CalcCopy { .. } => "calc_copy",
            Self::IslandPost { .. } => "island_post",
            Self::IslandDismiss { .. } => "island_dismiss",
            Self::IslandAction { .. } => "island_action",
            Self::IslandList => "island_list",
            Self::ProcSubscribe { .. } => "proc_subscribe",
            Self::ProcUnsubscribe => "proc_unsubscribe",
            Self::ProcDetail { .. } => "proc_detail",
            Self::ProcKill { .. } => "proc_kill",
            Self::SystemCommands => "system_commands",
            Self::SystemRun { .. } => "system_run",
            Self::TimerStart { .. } => "timer_start",
            Self::TimerParse { .. } => "timer_parse",
            Self::TimerList => "timer_list",
            Self::TimerPause { .. } => "timer_pause",
            Self::TimerResume { .. } => "timer_resume",
            Self::TimerAdd { .. } => "timer_add",
            Self::TimerCancel { .. } => "timer_cancel",
            Self::SnippetQuery { .. } => "snippet_query",
            Self::SnippetGet { .. } => "snippet_get",
            Self::SnippetSave { .. } => "snippet_save",
            Self::SnippetDelete { .. } => "snippet_delete",
            Self::SnippetPaste { .. } => "snippet_paste",
            Self::SnippetImport { .. } => "snippet_import",
            Self::SnippetExport { .. } => "snippet_export",
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
