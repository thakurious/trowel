//! Socket protocol v1. Frozen in Phase 1 (`PROTOCOL.md`).

pub mod config;
pub mod error;
pub mod ops;
pub mod path;
pub mod results;
pub mod status;

pub use config::{Config, UiConfig};
pub use error::{ErrorBody, ErrorCode};
pub use ops::{HelloOk, Op, Request};
pub use path::PathValue;
pub use results::{Meaning, ResultGroup, ResultRow, SearchMode, SearchResult};
pub use status::{Counts, ModelStatus, Progress, Status};

pub const PROTOCOL_VERSION: u32 = 1;
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
                protocol: 1,
                client: "panel".into(),
                pid: 1234,
            },
        };
        let v = serde_json::to_value(&req).unwrap();
        assert_eq!(v["op"], "hello");
        assert_eq!(v["protocol"], 1);
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
    fn unknown_fields_are_ignored() {
        let req: Request =
            serde_json::from_str(r#"{"id":1,"op":"panel_opened","extra":true}"#).unwrap();
        assert!(matches!(req.op, Op::PanelOpened));
    }
}
