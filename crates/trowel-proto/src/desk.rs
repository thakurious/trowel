//! Protocol v3 (Phase 9, `PHASE9-PLAN.md` §3): the Island, the process monitor, system commands,
//! timers and snippets. All additive: a v2 client never sends these ops.

use serde::{Deserialize, Serialize};

/// A button on an expanded island activity. `island_action` sends its `id` back to the producer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IslandButton {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub danger: bool,
}

/// Something the Island shows (P-D29). The daemon keeps these in memory only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IslandActivity {
    /// Unique per producer thing, e.g. `keylock`, `timer:7`. Posting an existing id replaces it.
    pub id: String,
    /// A glyph name the island knows (`lock`, `timer`, `progress`, …) or a single character.
    #[serde(default)]
    pub icon: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    /// Unix seconds: the island counts down to it by itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ends_at: Option<f64>,
    /// Total seconds of the countdown, for its progress ring.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<f64>,
    /// 0…1, drawn as a bar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<f64>,
    /// A trailing value shown compact, e.g. `42%`, when there is no countdown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// Higher shows first. Keyboard lock 100, a finished timer 90, a running timer 50.
    #[serde(default = "default_priority")]
    pub priority: i32,
    #[serde(default)]
    pub actions: Vec<IslandButton>,
    /// Expand and bounce once (a timer finished), then settle back to compact.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub alert: bool,
    /// Seconds after which the daemon drops it; none = until dismissed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl: Option<f64>,
}

fn default_priority() -> i32 {
    50
}

/// `island` event: the whole list, highest priority first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IslandState {
    pub activities: Vec<IslandActivity>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcSort {
    #[default]
    Cpu,
    Gpu,
    Ram,
    Disk,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcGroup {
    /// One row per app (its cgroup scope), pids underneath.
    #[default]
    App,
    Pid,
}

/// One process or app in a `proc` event. Percentages are of the whole machine (CPU: all cores =
/// 100 %); `None` = not readable (another user's `/proc/<pid>/io`), never shown as 0.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcRow {
    /// `pid:1234` or `app:<scope>`.
    pub key: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
    pub pids: Vec<i32>,
    pub user: String,
    pub cpu: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpu: Option<f64>,
    /// Resident memory, bytes.
    pub ram: u64,
    /// GPU memory (DRM resident), bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vram: Option<u64>,
    /// Bytes per second.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disk_read: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disk_write: Option<f64>,
    /// Terminate needs typing the name (or is refused: `kill_refused`) — P-D30.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub protected: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub kill_refused: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProcTotals {
    pub cpu: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpu: Option<f64>,
    pub ram_used: u64,
    pub ram_total: u64,
    pub disk_read: f64,
    pub disk_write: f64,
    pub processes: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KillSignal {
    Term,
    Kill,
}

/// An allow-listed system command (P-D31).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemCommand {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub subtitle: String,
    pub group: String,
    pub glyph: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    /// ↵ twice (Log Out, Restart, Shut Down).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub confirm: bool,
    /// For toggles: on/off now; None for plain actions or when unknown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimerState {
    Running,
    Paused,
    Done,
}

/// A timer (P-D32). Times are Unix seconds; `left` is what remains (frozen while paused).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Timer {
    pub id: i64,
    pub label: String,
    pub total: f64,
    pub state: TimerState,
    /// When it ends (running) — None while paused.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ends_at: Option<f64>,
    pub left: f64,
    pub created: f64,
    /// Set when it fired; `late` when that was after a suspend.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fired_at: Option<f64>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub late: bool,
}

/// `timer_parse` result: what `text` would start.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimerSpec {
    pub seconds: f64,
    #[serde(default)]
    pub label: String,
    /// Set for "at 15:30": the clock time it ends.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snippet {
    #[serde(default)]
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub keyword: String,
    pub body: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub created: i64,
    #[serde(default)]
    pub last_used: i64,
    #[serde(default)]
    pub uses: i64,
    /// `{argument:name}` placeholders, in order, for the pane's form.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub arguments: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn activity_defaults_and_roundtrip() {
        let a: IslandActivity =
            serde_json::from_value(json!({"id": "timer:7", "title": "Tea", "ends_at": 1.0e9}))
                .unwrap();
        assert_eq!(a.priority, 50);
        assert!(a.actions.is_empty() && !a.alert);
        let back = serde_json::to_value(&a).unwrap();
        assert!(back.get("progress").is_none(), "{back}");
        assert_eq!(serde_json::from_value::<IslandActivity>(back).unwrap(), a);
    }

    #[test]
    fn enums_are_snake_case() {
        assert_eq!(serde_json::to_value(ProcSort::Disk).unwrap(), json!("disk"));
        assert_eq!(
            serde_json::to_value(KillSignal::Term).unwrap(),
            json!("term")
        );
        assert_eq!(
            serde_json::to_value(TimerState::Paused).unwrap(),
            json!("paused")
        );
        assert_eq!(serde_json::to_value(ProcGroup::App).unwrap(), json!("app"));
    }

    #[test]
    fn unreadable_is_absent_not_zero() {
        let row = ProcRow {
            key: "pid:1".into(),
            name: "systemd".into(),
            app_id: None,
            pids: vec![1],
            user: "root".into(),
            cpu: 0.0,
            gpu: None,
            ram: 4096,
            vram: None,
            disk_read: None,
            disk_write: None,
            protected: true,
            kill_refused: true,
        };
        let v = serde_json::to_value(&row).unwrap();
        assert!(
            v.get("disk_read").is_none() && v.get("gpu").is_none(),
            "{v}"
        );
        assert_eq!(v["kill_refused"], json!(true));
    }
}
