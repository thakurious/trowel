//! `upstream/Sources/DigUpKit/Extractors.swift:8`

use std::fmt;

#[derive(Debug)]
pub enum ExtractError {
    Unreadable(String),
    TooSmall { width: i32, height: i32 },
    Locked,
    Empty(String),
    LeftOut(String),
}

impl fmt::Display for ExtractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreadable(why) => write!(f, "unreadable: {why}"),
            Self::TooSmall { width, height } => write!(f, "small image {width}×{height}"),
            Self::Locked => write!(f, "password-protected"),
            Self::Empty(why) => write!(f, "{why}"),
            Self::LeftOut(why) => write!(f, "{why}"),
        }
    }
}

impl std::error::Error for ExtractError {}
