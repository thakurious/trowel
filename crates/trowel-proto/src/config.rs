//! `config.toml` mirror. `PROTOCOL.md` §5.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub roots: Vec<String>,
    pub excluded_folders: Vec<String>,
    pub excluded_types: Vec<String>,
    pub index_dir: String,
    pub models_dir: String,
    pub hotkey: String,
    pub index_on_battery: bool,
    pub encoder_idle_seconds: u64,
    pub onboarded: bool,
    pub code_roots: Vec<String>,
    pub code_excluded: Vec<String>,
    pub code_editor: String,
    pub code_quiet_seconds: u64,
    pub ui: UiConfig,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiConfig {
    pub reduce_motion: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            roots: vec![
                "~/Desktop".into(),
                "~/Documents".into(),
                "~/Downloads".into(),
                "~/Pictures".into(),
            ],
            excluded_folders: Vec::new(),
            excluded_types: Vec::new(),
            index_dir: "~/.local/share/trowel/index".into(),
            models_dir: "~/.local/share/trowel/models".into(),
            hotkey: "SUPER + D".into(),
            index_on_battery: false,
            encoder_idle_seconds: 600,
            onboarded: false,
            code_roots: Vec::new(),
            code_excluded: Vec::new(),
            code_editor: String::new(),
            code_quiet_seconds: 2,
            ui: UiConfig {
                reduce_motion: false,
            },
        }
    }
}
