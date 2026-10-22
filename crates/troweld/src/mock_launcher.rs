//! Canned protocol v2 (apps, clipboard, calculator) for `troweld --mock`. `LAUNCHER-PLAN.md` §3:
//! the UI track builds against this before L2/L3 land. Ranking here is a stand-in, not the
//! `AppSearch.js` port (that is `apps/rank.rs`, L2).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Mutex;

use serde::Deserialize;
use serde_json::{json, Value};
use trowel_proto::{
    AppRow, AppsResult, CalcHistoryRow, CalcReply, CalcResult, ClipContent, ClipImage,
    ClipQueryResult, ClipRow, ErrorCode, Op, PathValue,
};

use crate::learn;

pub const DEFAULT_LAUNCHER_FIXTURE: &str = include_str!("fixture_launcher.json");

#[derive(Clone, Default, Deserialize)]
pub struct ClipFixtureContent {
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    html: Option<String>,
    #[serde(default)]
    files: Option<Vec<PathValue>>,
    #[serde(default)]
    image: Option<ClipImage>,
}

#[derive(Clone, Default, Deserialize)]
pub struct LauncherFixture {
    #[serde(default)]
    apps: Vec<AppRow>,
    #[serde(default)]
    clips: Vec<ClipRow>,
    #[serde(default)]
    clip_content: BTreeMap<String, ClipFixtureContent>,
    #[serde(default)]
    calcs: Vec<CalcResult>,
    #[serde(default)]
    calc_history: Vec<CalcHistoryRow>,
}

impl LauncherFixture {
    pub fn default_fixture() -> Self {
        serde_json::from_str(DEFAULT_LAUNCHER_FIXTURE).expect("fixture_launcher.json")
    }
}

pub struct LauncherMock {
    apps: Vec<AppRow>,
    hidden: Mutex<HashSet<String>>,
    clips: Mutex<Vec<ClipRow>>,
    content: BTreeMap<String, ClipFixtureContent>,
    calcs: Vec<CalcResult>,
    history: Mutex<Vec<CalcHistoryRow>>,
    paused_until: Mutex<i64>,
    /// Learning is real (`crate::learn`) but in memory; the stand-in ranking ignores it.
    learned: Mutex<HashMap<String, learn::Learned>>,
}

pub type OpResult = Result<Value, (ErrorCode, String)>;

impl LauncherMock {
    pub fn new(fixture: LauncherFixture) -> Self {
        Self {
            apps: fixture.apps,
            hidden: Mutex::new(HashSet::new()),
            clips: Mutex::new(fixture.clips),
            content: fixture.clip_content,
            calcs: fixture.calcs,
            history: Mutex::new(fixture.calc_history),
            paused_until: Mutex::new(0),
            learned: Mutex::new(HashMap::new()),
        }
    }

    fn learn(&self, item: &str, query: Option<&str>) {
        let mut learned = self.learned.lock().expect("learned");
        let next = learn::record(
            learned.get(item),
            crate::engine::unix_now(),
            query.unwrap_or(""),
        );
        learned.insert(item.to_string(), next);
    }

    /// Handles one v2 op. Callers route only `Op::is_v2()` here.
    pub fn handle(&self, op: Op) -> OpResult {
        match op {
            Op::AppsQuery {
                text,
                limit,
                gen,
                commands,
            } => {
                let mut out = self.apps_query(&text, limit, gen);
                let learned = self.learned.lock().expect("learned");
                let now = crate::engine::unix_now();
                for row in &mut out.rows {
                    row.learned =
                        learn::Signal::of(learned.get(&format!("app:{}", row.id)), &text, now)
                            .to_wire();
                }
                out.commands = learn::command_signals(&commands, &learned, &text, now);
                Ok(json!(out))
            }
            Op::AppLaunch {
                app_id,
                action,
                query,
            } => {
                let app = self.app(&app_id)?;
                if let Some(action) = action {
                    if !app.actions.iter().any(|a| a.id == action) {
                        return Err((ErrorCode::NotFound, format!("no action {action}")));
                    }
                }
                self.learn(&format!("app:{app_id}"), query.as_deref());
                Ok(json!({}))
            }
            Op::CommandRun { command_id, query } => {
                self.learn(&format!("cmd:{command_id}"), query.as_deref());
                Ok(json!({}))
            }
            Op::AppHide { app_id, hidden } => {
                self.app(&app_id)?;
                let mut set = self.hidden.lock().expect("hidden");
                if hidden {
                    set.insert(app_id);
                } else {
                    set.remove(&app_id);
                }
                Ok(json!({}))
            }
            Op::ClipQuery {
                text,
                kinds,
                limit,
                offset,
                gen,
            } => Ok(json!(self.clip_query(
                &text,
                kinds.as_deref(),
                limit,
                offset,
                gen
            ))),
            Op::ClipGet { clip_id } => {
                let row = self.clip(clip_id)?;
                let content = self
                    .content
                    .get(&clip_id.to_string())
                    .cloned()
                    .unwrap_or_default();
                Ok(json!(ClipContent {
                    row,
                    text: content.text,
                    html: content.html,
                    files: content.files,
                    image: content.image,
                    truncated: false,
                }))
            }
            Op::ClipPaste { clip_id, .. } => {
                self.clip(clip_id)?;
                Ok(json!({}))
            }
            Op::ClipPin { clip_id, pinned } => {
                let mut clips = self.clips.lock().expect("clips");
                let row = clips
                    .iter_mut()
                    .find(|r| r.id == clip_id)
                    .ok_or_else(|| not_found(clip_id))?;
                row.pinned = pinned;
                Ok(json!({}))
            }
            Op::ClipDelete { clip_id } => {
                let mut clips = self.clips.lock().expect("clips");
                let before = clips.len();
                clips.retain(|r| r.id != clip_id);
                if clips.len() == before {
                    return Err(not_found(clip_id));
                }
                Ok(json!({}))
            }
            Op::ClipClear { keep_pinned } => {
                let mut clips = self.clips.lock().expect("clips");
                let before = clips.len();
                clips.retain(|r| keep_pinned && r.pinned);
                Ok(json!({"removed": before - clips.len()}))
            }
            Op::ClipPause { minutes } => {
                let mut until = self.paused_until.lock().expect("paused");
                *until = match minutes {
                    Some(0) => 0,
                    Some(m) => MOCK_NOW + i64::from(m) * 60,
                    // Until resumed.
                    None => -1,
                };
                Ok(json!({"paused_until": *until}))
            }
            Op::ClipIngest { .. } => Err((
                ErrorCode::BadRequest,
                "clip_ingest is only accepted from trowel clip-capture".into(),
            )),
            // The mock has no Omarchy history to read.
            Op::ClipImport { .. } => Ok(json!({"imported": 0, "skipped": 0})),
            Op::Calc { text, force, gen } => Ok(json!(self.calc(&text, force, gen))),
            Op::CalcHistory { limit } => {
                let rows: Vec<_> = self
                    .history
                    .lock()
                    .expect("history")
                    .iter()
                    .take(limit as usize)
                    .cloned()
                    .collect();
                Ok(json!({"rows": rows}))
            }
            Op::CalcCopy { text, expr, .. } => {
                if let Some(expr) = expr {
                    self.history.lock().expect("history").insert(
                        0,
                        CalcHistoryRow {
                            expr,
                            result: text,
                            at: MOCK_NOW,
                        },
                    );
                }
                Ok(json!({}))
            }
            other => Err((
                ErrorCode::Internal,
                format!("{} is not a v2 op", other.name()),
            )),
        }
    }

    fn app(&self, id: &str) -> Result<&AppRow, (ErrorCode, String)> {
        self.apps
            .iter()
            .find(|a| a.id == id)
            .ok_or_else(|| (ErrorCode::NotFound, format!("no app {id}")))
    }

    fn clip(&self, id: i64) -> Result<ClipRow, (ErrorCode, String)> {
        self.clips
            .lock()
            .expect("clips")
            .iter()
            .find(|r| r.id == id)
            .cloned()
            .ok_or_else(|| not_found(id))
    }

    fn apps_query(&self, text: &str, limit: u32, gen: u64) -> AppsResult {
        let hidden = self.hidden.lock().expect("hidden");
        let needle = text.trim().to_lowercase();
        let mut scored: Vec<(u8, &AppRow, Vec<[u32; 2]>)> = self
            .apps
            .iter()
            .filter(|a| !hidden.contains(&a.id))
            .filter_map(|a| {
                if needle.is_empty() {
                    return Some((0, a, Vec::new()));
                }
                let name = a.name.to_lowercase();
                let span = |at: usize| {
                    let start = name[..at].chars().count() as u32;
                    vec![[start, start + needle.chars().count() as u32]]
                };
                if name.starts_with(&needle) {
                    Some((3, a, span(0)))
                } else if let Some(at) = name
                    .match_indices(&needle)
                    .map(|(i, _)| i)
                    .find(|&i| name[..i].ends_with(' '))
                {
                    Some((2, a, span(at)))
                } else if let Some(at) = name.find(&needle) {
                    Some((1, a, span(at)))
                } else {
                    let generic = a.generic.as_deref().unwrap_or("").to_lowercase();
                    generic.contains(&needle).then(|| (0, a, Vec::new()))
                }
            })
            .collect();
        scored.sort_by(|l, r| {
            r.0.cmp(&l.0)
                .then(r.1.launches.cmp(&l.1.launches))
                .then(r.1.last_launched.cmp(&l.1.last_launched))
                .then(l.1.name.cmp(&r.1.name))
        });
        let rows = scored
            .into_iter()
            .take(limit as usize)
            .map(|(_, a, matched)| AppRow {
                matched,
                ..a.clone()
            })
            .collect();
        AppsResult {
            gen,
            rows,
            took_us: None,
            commands: Vec::new(),
        }
    }

    fn clip_query(
        &self,
        text: &str,
        kinds: Option<&[trowel_proto::ClipKind]>,
        limit: u32,
        offset: u32,
        gen: u64,
    ) -> ClipQueryResult {
        let needle = text.trim().to_lowercase();
        let mut rows: Vec<ClipRow> = self
            .clips
            .lock()
            .expect("clips")
            .iter()
            .filter(|r| kinds.is_none_or(|k| k.contains(&r.kind)))
            .filter(|r| needle.is_empty() || r.preview.to_lowercase().contains(&needle))
            .cloned()
            .collect();
        // Pinned first, then most recently used.
        rows.sort_by(|l, r| {
            r.pinned
                .cmp(&l.pinned)
                .then(r.last_used.cmp(&l.last_used))
                .then(r.id.cmp(&l.id))
        });
        let total = rows.len() as u64;
        let rows = rows
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .collect();
        ClipQueryResult { gen, rows, total }
    }

    fn calc(&self, text: &str, force: bool, gen: u64) -> CalcReply {
        let expr = text.trim().trim_start_matches('=').trim();
        let result = self.calcs.iter().find(|c| c.expr == expr).cloned();
        let error = (force && result.is_none()).then(|| "Couldn't calculate this".to_string());
        CalcReply {
            gen,
            result,
            error,
            took_us: None,
        }
    }
}

/// Fixed clock so transcripts are deterministic (the clipboard fixture's newest entry).
const MOCK_NOW: i64 = 1_791_869_400;

fn not_found(id: i64) -> (ErrorCode, String) {
    (ErrorCode::NotFound, format!("no clipboard entry {id}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use trowel_proto::ClipKind;

    fn mock() -> LauncherMock {
        LauncherMock::new(LauncherFixture::default_fixture())
    }

    fn ok(m: &LauncherMock, op: Op) -> Value {
        m.handle(op)
            .unwrap_or_else(|(c, msg)| panic!("{c:?}: {msg}"))
    }

    #[test]
    fn fixture_covers_the_plan() {
        let f = LauncherFixture::default_fixture();
        assert!(f.apps.len() >= 30, "about 30 apps");
        let kinds: HashSet<_> = f.clips.iter().map(|c| c.kind).collect();
        assert_eq!(kinds.len(), ClipKind::ALL.len(), "all seven clip kinds");
        assert!(f.calcs.iter().any(|c| c.expr == "5 ft to cm"));
    }

    #[test]
    fn prefix_beats_frecency_and_empty_is_frecency() {
        let m = mock();
        let v = ok(
            &m,
            Op::AppsQuery {
                text: "fire".into(),
                limit: 8,
                gen: 1,
                commands: vec![],
            },
        );
        assert_eq!(v["rows"][0]["id"], "firefox");
        assert_eq!(v["rows"][0]["matched"], json!([[0, 4]]));
        let v = ok(
            &m,
            Op::AppsQuery {
                text: String::new(),
                limit: 3,
                gen: 2,
                commands: vec![],
            },
        );
        assert_eq!(v["gen"], 2);
        assert_eq!(v["rows"][0]["id"], "com.mitchellh.ghostty", "most launches");
        assert_eq!(v["rows"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn launches_and_commands_are_learned() {
        let m = mock();
        ok(
            &m,
            Op::AppLaunch {
                app_id: "firefox".into(),
                action: None,
                query: Some("fi".into()),
            },
        );
        ok(
            &m,
            Op::CommandRun {
                command_id: "clipboard".into(),
                query: Some("cl".into()),
            },
        );
        let v = ok(
            &m,
            Op::AppsQuery {
                text: "fi".into(),
                limit: 8,
                gen: 1,
                commands: vec!["clipboard".into(), "never-run".into()],
            },
        );
        let ff = v["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == "firefox")
            .unwrap();
        assert_eq!(ff["learned"]["term"], "exact", "{v}");
        assert_eq!(v["commands"].as_array().unwrap().len(), 1, "{v}");
        assert_eq!(v["commands"][0]["id"], "clipboard");
        assert!(
            v["commands"][0].get("term").is_none(),
            "\"fi\" isn't a clipboard term"
        );
    }

    #[test]
    fn hidden_apps_leave_results() {
        let m = mock();
        ok(
            &m,
            Op::AppHide {
                app_id: "firefox".into(),
                hidden: true,
            },
        );
        let v = ok(
            &m,
            Op::AppsQuery {
                text: "firefox".into(),
                limit: 8,
                gen: 1,
                commands: vec![],
            },
        );
        assert!(v["rows"].as_array().unwrap().is_empty());
    }

    #[test]
    fn clip_filters_pins_and_clear() {
        let m = mock();
        let v = ok(
            &m,
            Op::ClipQuery {
                text: String::new(),
                kinds: None,
                limit: 50,
                offset: 0,
                gen: 1,
            },
        );
        assert_eq!(v["total"], 7);
        assert_eq!(v["rows"][0]["pinned"], true, "pinned first");
        let v = ok(
            &m,
            Op::ClipQuery {
                text: String::new(),
                kinds: Some(vec![ClipKind::Image]),
                limit: 50,
                offset: 0,
                gen: 2,
            },
        );
        assert_eq!(v["total"], 1);
        assert_eq!(v["rows"][0]["kind"], "image");
        let v = ok(&m, Op::ClipClear { keep_pinned: true });
        assert_eq!(v["removed"], 5);
        assert_eq!(
            m.handle(Op::ClipGet { clip_id: 812 }).unwrap_err().0,
            ErrorCode::NotFound
        );
    }

    #[test]
    fn calc_is_silent_unless_forced() {
        let m = mock();
        let v = ok(
            &m,
            Op::Calc {
                text: "5 ft to cm".into(),
                force: false,
                gen: 4,
            },
        );
        assert_eq!(v["result"]["copy"], "152.4");
        let v = ok(
            &m,
            Op::Calc {
                text: "firefox".into(),
                force: false,
                gen: 5,
            },
        );
        assert_eq!(v, json!({"gen": 5}));
        let v = ok(
            &m,
            Op::Calc {
                text: "= 1 +".into(),
                force: true,
                gen: 6,
            },
        );
        assert!(v["error"].is_string());
    }

    #[test]
    fn copying_a_result_records_history() {
        let m = mock();
        ok(
            &m,
            Op::CalcCopy {
                text: "3.3333333333".into(),
                expr: Some("10/3".into()),
                mode: trowel_proto::PasteMode::Copy,
            },
        );
        let v = ok(&m, Op::CalcHistory { limit: 1 });
        assert_eq!(v["rows"][0]["expr"], "10/3");
    }
}
