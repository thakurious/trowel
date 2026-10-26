//! Socket protocol. v1 frozen in Phase 1; v2 (D22) adds apps, clipboard and calculator ops
//! (`PROTOCOL.md`).

pub mod config;
pub mod desk;
pub mod error;
pub mod launcher;
pub mod ops;
pub mod path;
pub mod results;
pub mod status;

pub use config::{ClipboardConfig, Config, UiConfig};
pub use desk::{
    IslandActivity, IslandButton, IslandState, KillSignal, ProcGroup, ProcRow, ProcSort,
    ProcTotals, Snippet, SystemCommand, Timer, TimerSpec, TimerState,
};
pub use error::{ErrorBody, ErrorCode};
pub use launcher::{
    AppAction, AppRow, AppsResult, CalcAlt, CalcHistoryRow, CalcKind, CalcReply, CalcResult,
    ClipContent, ClipImage, ClipKind, ClipOffer, ClipQueryResult, ClipRow, CommandSignal,
    LearnedSignal, PasteMode,
};
pub use ops::{HelloOk, Op, Request};
pub use path::PathValue;
pub use results::{Meaning, ResultGroup, ResultRow, SearchMode, SearchResult};
pub use status::{Counts, ModelStatus, Progress, Status};

/// v3 (Phase 9) is additive: the daemon still accepts a v2 `hello` ([`OLDEST_PROTOCOL`]).
pub const PROTOCOL_VERSION: u32 = 3;
pub const OLDEST_PROTOCOL: u32 = 2;
/// Max JSON line length. A longer line closes the connection with `{"error":"too_large"}`.
pub const MAX_LINE_BYTES: usize = 4 * 1024 * 1024;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_roundtrip() {
        let req = Request {
            id: 1,
            op: Op::Hello {
                protocol: 2,
                client: "panel".into(),
                pid: 1234,
            },
        };
        let v = serde_json::to_value(&req).unwrap();
        assert_eq!(v["op"], "hello");
        assert_eq!(v["protocol"], 2);
        let back: Request = serde_json::from_value(v).unwrap();
        assert_eq!(back, req);
    }

    #[test]
    fn search_limit_defaults_to_60() {
        let req: Request = serde_json::from_str(
            r#"{"id":2,"op":"search","text":"refund","mode":"keyword","prefix_last":true,"gen":1}"#,
        )
        .unwrap();
        match req.op {
            Op::Search { limit, .. } => assert_eq!(limit, 60),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn every_known_op_has_a_variant_name() {
        // `name()` and `KNOWN_OPS` must stay one closed set.
        let samples = [
            r#"{"id":1,"op":"apps_query","gen":1}"#,
            r#"{"id":1,"op":"app_launch","app_id":"firefox"}"#,
            r#"{"id":1,"op":"clip_query","gen":2,"kinds":["image","link"]}"#,
            r#"{"id":1,"op":"clip_paste","clip_id":812}"#,
            r#"{"id":1,"op":"clip_clear"}"#,
            r#"{"id":1,"op":"calc","text":"5 ft to cm","gen":3}"#,
            r#"{"id":1,"op":"calc_history"}"#,
            r#"{"id":1,"op":"calc_copy","text":"152.4","mode":"copy"}"#,
        ];
        for raw in samples {
            let req: Request = serde_json::from_str(raw).unwrap_or_else(|e| panic!("{raw}: {e}"));
            assert_eq!(req.id, 1, "{raw}: params must not shadow the envelope id");
            assert!(ops::KNOWN_OPS.contains(&req.op.name()), "{raw}");
            assert!(req.op.is_v2(), "{raw}");
        }
    }

    #[test]
    fn every_v3_op_parses_and_is_known() {
        let samples = [
            r#"{"id":1,"op":"island_post","activity":{"id":"keylock","title":"Keyboard locked","ends_at":1.0}}"#,
            r#"{"id":1,"op":"island_dismiss","activity_id":"keylock"}"#,
            r#"{"id":1,"op":"island_action","activity_id":"keylock","action":"unlock"}"#,
            r#"{"id":1,"op":"island_list"}"#,
            r#"{"id":1,"op":"proc_subscribe","sort":"gpu"}"#,
            r#"{"id":1,"op":"proc_unsubscribe"}"#,
            r#"{"id":1,"op":"proc_detail","pid":42}"#,
            r#"{"id":1,"op":"proc_kill","pids":[42],"signal":"term"}"#,
            r#"{"id":1,"op":"system_commands"}"#,
            r#"{"id":1,"op":"system_run","command_id":"lock"}"#,
            r#"{"id":1,"op":"timer_start","text":"25m tea"}"#,
            r#"{"id":1,"op":"timer_parse","text":"at 15:30"}"#,
            r#"{"id":1,"op":"timer_list"}"#,
            r#"{"id":1,"op":"timer_pause","timer_id":7}"#,
            r#"{"id":1,"op":"timer_resume","timer_id":7}"#,
            r#"{"id":1,"op":"timer_add","timer_id":7,"seconds":60}"#,
            r#"{"id":1,"op":"timer_cancel","timer_id":7}"#,
            r#"{"id":1,"op":"snippet_query","text":"sig"}"#,
            r#"{"id":1,"op":"snippet_get","snippet_id":3}"#,
            r#"{"id":1,"op":"snippet_save","snippet":{"name":"Sig","body":"Thanks,\nS"}}"#,
            r#"{"id":1,"op":"snippet_delete","snippet_id":3}"#,
            r#"{"id":1,"op":"snippet_paste","snippet_id":3,"args":{"who":"Ana"}}"#,
            r#"{"id":1,"op":"snippet_import","path":"/tmp/s.json"}"#,
            r#"{"id":1,"op":"snippet_export","path":"/tmp/s.json"}"#,
        ];
        let mut seen = std::collections::HashSet::new();
        for raw in samples {
            let req: Request = serde_json::from_str(raw).unwrap_or_else(|e| panic!("{raw}: {e}"));
            assert_eq!(req.id, 1, "{raw}: params must not shadow the envelope id");
            assert!(ops::KNOWN_OPS.contains(&req.op.name()), "{raw}");
            assert!(req.op.is_v3() && !req.op.is_v2(), "{raw}");
            seen.insert(req.op.name());
        }
        let v3_names = ops::KNOWN_OPS
            .iter()
            .skip_while(|n| **n != "island_post")
            .collect::<Vec<_>>();
        assert_eq!(v3_names.len(), seen.len(), "a v3 op has no sample");
    }

    #[test]
    fn v2_defaults() {
        let req: Request = serde_json::from_str(r#"{"id":1,"op":"apps_query","gen":1}"#).unwrap();
        assert_eq!(
            req.op,
            Op::AppsQuery {
                text: String::new(),
                limit: 8,
                gen: 1,
                commands: vec![]
            }
        );
        let req: Request =
            serde_json::from_str(r#"{"id":1,"op":"clip_paste","clip_id":5}"#).unwrap();
        assert_eq!(
            req.op,
            Op::ClipPaste {
                clip_id: 5,
                mode: PasteMode::Paste
            }
        );
        let req: Request = serde_json::from_str(r#"{"id":1,"op":"clip_clear"}"#).unwrap();
        assert_eq!(req.op, Op::ClipClear { keep_pinned: true });
        assert!(
            serde_json::from_str::<Request>(
                r#"{"id":1,"op":"clip_query","gen":1,"kinds":["sound"]}"#
            )
            .is_err(),
            "an unknown clip kind is a bad_request"
        );
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let req: Request =
            serde_json::from_str(r#"{"id":1,"op":"panel_opened","extra":true}"#).unwrap();
        assert!(matches!(req.op, Op::PanelOpened));
    }
}
