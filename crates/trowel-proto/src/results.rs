//! Search result rows and groups. `PROTOCOL.md` §4.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::path::PathValue;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResultRow {
    pub id: PathValue,
    pub name: String,
    pub folder: String,
    pub kind: String,
    pub segment: String,
    pub segment_id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loc: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loc_end: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focus: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub place: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub excerpt: Option<String>,
    pub matched_meaning: bool,
    pub matched_words: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<f64>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub info: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResultGroup {
    pub kind: String,
    pub title: String,
    pub rows: Vec<ResultRow>,
    pub total: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchMode {
    Keyword,
    Hybrid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Meaning {
    Used,
    Skipped,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchResult {
    pub gen: u64,
    pub groups: Vec<ResultGroup>,
    pub weaker: Vec<ResultRow>,
    pub took_ms: u64,
    pub meaning: Meaning,
}
