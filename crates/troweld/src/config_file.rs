//! `~/.config/trowel/config.toml`. The daemon is the only writer (`PROTOCOL.md` §5).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use trowel_proto::Config;

pub fn load_or_create(path: &Path) -> io::Result<Config> {
    if path.exists() {
        return load(path);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let cfg = Config::default();
    save(path, &cfg)?;
    Ok(cfg)
}

pub fn load(path: &Path) -> io::Result<Config> {
    let raw = fs::read_to_string(path)?;
    toml::from_str(&raw).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
}

pub fn save(path: &Path, cfg: &Config) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let body = toml::to_string_pretty(cfg)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, body)?;
    fs::rename(tmp, path)
}

pub fn apply_patch(cfg: &Config, patch: &serde_json::Value) -> Result<Config, String> {
    let mut base = serde_json::to_value(cfg).map_err(|e| e.to_string())?;
    merge_objects(&mut base, patch);
    serde_json::from_value(base).map_err(|e| e.to_string())
}

fn merge_objects(base: &mut serde_json::Value, patch: &serde_json::Value) {
    let (serde_json::Value::Object(dst), serde_json::Value::Object(src)) = (base, patch) else {
        return;
    };
    for (k, v) in src {
        if (k == "ui" || k == "clipboard") && v.is_object() {
            merge_objects(
                dst.entry(k.clone())
                    .or_insert_with(|| serde_json::json!({})),
                v,
            );
        } else {
            dst.insert(k.clone(), v.clone());
        }
    }
}

pub fn default_config_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var_os("HOME").unwrap_or_else(|| "/tmp".into());
            PathBuf::from(home).join(".config")
        });
    base.join("trowel/config.toml")
}

pub fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        let home = std::env::var_os("HOME").unwrap_or_else(|| "/tmp".into());
        return PathBuf::from(home).join(rest);
    }
    if path == "~" {
        let home = std::env::var_os("HOME").unwrap_or_else(|| "/tmp".into());
        return PathBuf::from(home);
    }
    PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_default_config() {
        let dir = std::env::temp_dir().join(format!(
            "trowel-cfg-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        let cfg = load_or_create(&path).unwrap();
        assert_eq!(cfg.hotkey, "SUPER + D");
        assert!(!cfg.onboarded);
        let again = load(&path).unwrap();
        assert_eq!(cfg, again);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn patch_sets_onboarded() {
        let cfg = Config::default();
        let next = apply_patch(&cfg, &serde_json::json!({"onboarded": true})).unwrap();
        assert!(next.onboarded);
        assert_eq!(next.hotkey, cfg.hotkey);
    }
}
