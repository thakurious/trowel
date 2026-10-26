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
    /// How hard indexing may work (D28): "gentle", "balanced" or "full". Absent in older files,
    /// so it defaults; an unknown value reads as gentle.
    #[serde(default = "default_index_speed")]
    pub index_speed: String,
    pub ui: UiConfig,
    /// `[clipboard]` (Phase 8 L3). Absent in older files, so it defaults.
    #[serde(default)]
    pub clipboard: ClipboardConfig,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiConfig {
    pub reduce_motion: bool,
    /// The panel reopens where it was left within this many seconds of closing, then blank
    /// (Raycast's "pop to root"; 0 = always blank). Absent in older files, so it defaults.
    #[serde(default = "default_pop_to_root_seconds")]
    pub pop_to_root_seconds: u32,
}

fn default_pop_to_root_seconds() -> u32 {
    90
}

fn default_index_speed() -> String {
    "gentle".into()
}

/// Clipboard history (D20, `LAUNCHER-PLAN.md` L3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClipboardConfig {
    /// Run the `wl-paste --watch` capture.
    pub capture: bool,
    pub max_entries: u32,
    pub max_age_days: u32,
    pub max_blob_mb: u64,
    /// Largest single entry kept (`clip_max_bytes`).
    pub max_entry_mb: u64,
    /// Hyprland window classes never captured (case-insensitive).
    pub excluded_apps: Vec<String>,
}

impl Default for ClipboardConfig {
    fn default() -> Self {
        Self {
            capture: true,
            max_entries: 1000,
            max_age_days: 30,
            max_blob_mb: 512,
            max_entry_mb: 64,
            excluded_apps: vec![
                "1password".into(),
                "bitwarden".into(),
                "com.bitwarden.desktop".into(),
                "keepassxc".into(),
                "org.keepassxc.keepassxc".into(),
            ],
        }
    }
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
            index_speed: default_index_speed(),
            ui: UiConfig {
                reduce_motion: false,
                pop_to_root_seconds: default_pop_to_root_seconds(),
            },
            clipboard: ClipboardConfig::default(),
        }
    }
}
