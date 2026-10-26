//! Closed error-code set. `PROTOCOL.md` §2.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    BadRequest,
    UnknownOp,
    NotReady,
    Cancelled,
    Superseded,
    NotFound,
    Io,
    Busy,
    Internal,
    /// v3: a process the kill policy protects (P-D30).
    NotAllowed,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BadRequest => "bad_request",
            Self::UnknownOp => "unknown_op",
            Self::NotReady => "not_ready",
            Self::Cancelled => "cancelled",
            Self::Superseded => "superseded",
            Self::NotFound => "not_found",
            Self::Io => "io",
            Self::Busy => "busy",
            Self::Internal => "internal",
            Self::NotAllowed => "not_allowed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorBody {
    pub code: ErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_ms: Option<u64>,
}

impl ErrorBody {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            retry_ms: None,
        }
    }
}
