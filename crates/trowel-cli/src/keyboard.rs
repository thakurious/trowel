//! Lock Keyboard (the panel command): ignore every physical keyboard for a while, to clean it.
//!
//! Hyprland 0.56 disables a device at runtime with `hl.device({ name, enabled = false })` through
//! `hyprctl eval`, exactly as Omarchy's own `omarchy-toggle-input-device` does. Nothing is written
//! to the Hyprland config, so a reload brings the keyboards back. Safety, in order:
//! 1. the unlock is scheduled first, as a transient systemd user timer that runs
//!    `trowel keyboard unlock`; it fires even if troweld, the panel or omarchy-shell dies, and no
//!    key is disabled if it can't be scheduled;
//! 2. the lock is capped at [`MAX_SECONDS`];
//! 3. the power button, lid and other non-typing devices stay on, and so do virtual keyboards
//!    (fcitx5, wtype);
//! 4. the panel's countdown pill has an Unlock button for the mouse.
//!
//! State lives in `$XDG_RUNTIME_DIR/trowel/keyboard-lock.json` (tmpfs, gone after a reboot), which
//! the panel watches for the countdown.

use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

pub const MAX_SECONDS: u64 = 30 * 60;
const UNIT: &str = "trowel-keyboard-unlock";

/// Runs one external command; tests swap in a recorder.
pub trait Runner {
    fn run(&mut self, program: &str, args: &[String]) -> Result<String, String>;
}

pub struct System;

impl Runner for System {
    fn run(&mut self, program: &str, args: &[String]) -> Result<String, String> {
        let out = Command::new(program)
            .args(args)
            .output()
            .map_err(|e| format!("{program}: {e}"))?;
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            return Err(format!("{program} failed: {}", err.trim()));
        }
        Ok(text)
    }
}

/// A keyboard device whose keys someone could press while cleaning. Power, sleep and lid switches,
/// the video bus (brightness keys report there), headset buttons and virtual keyboards stay on.
pub fn lockable(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    const KEEP: [&str; 8] = [
        "virtual",
        "power-button",
        "sleep-button",
        "lid-switch",
        "video-bus",
        "jack",
        "avrcp",
        "headset",
    ];
    !n.is_empty() && !KEEP.iter().any(|k| n.contains(k))
}

/// A device name as a Lua string literal. Names come from USB descriptors: control characters are
/// refused, backslashes and quotes escaped (as `omarchy-toggle-input-device` does).
pub fn lua_string(name: &str) -> Option<String> {
    if name.chars().any(char::is_control) {
        return None;
    }
    Some(format!(
        "\"{}\"",
        name.replace('\\', "\\\\").replace('"', "\\\"")
    ))
}

fn state_path() -> PathBuf {
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("trowel/keyboard-lock.json")
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn keyboards(r: &mut dyn Runner) -> Result<Vec<String>, String> {
    let text = r.run("hyprctl", &["-j".into(), "devices".into()])?;
    let v: Value = serde_json::from_str(&text).map_err(|e| format!("hyprctl devices: {e}"))?;
    let mut out: Vec<String> = v["keyboards"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|k| k["name"].as_str())
        .filter(|n| lockable(n))
        .map(str::to_string)
        .collect();
    out.sort();
    out.dedup();
    Ok(out)
}

fn set_enabled(r: &mut dyn Runner, name: &str, enabled: bool) -> Result<(), String> {
    let lit = lua_string(name).ok_or_else(|| format!("unsafe device name {name:?}"))?;
    let out = r.run(
        "hyprctl",
        &[
            "eval".into(),
            format!("hl.device({{ name = {lit}, enabled = {enabled} }})"),
        ],
    )?;
    // `hyprctl eval` prints "ok" or the Lua error, with exit status 0 either way.
    if out.trim() != "ok" {
        return Err(format!("hyprctl eval: {}", out.trim()));
    }
    Ok(())
}

fn trowel_exe() -> String {
    std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "trowel".into())
}

/// Lock for `seconds` (1…MAX_SECONDS). Locking again while locked sets a new end time.
pub fn lock(r: &mut dyn Runner, seconds: u64, path: &std::path::Path) -> Result<Value, String> {
    if seconds == 0 || seconds > MAX_SECONDS {
        return Err(format!(
            "lock for 1 to {MAX_SECONDS} seconds, not {seconds}"
        ));
    }
    let devices = keyboards(r)?;
    if devices.is_empty() {
        return Err("no keyboard to lock".into());
    }
    // 1. The unlock first. A running lock's timer is replaced; a missing one is fine.
    let _ = r.run(
        "systemctl",
        &["--user".into(), "stop".into(), format!("{UNIT}.timer")],
    );
    r.run(
        "systemd-run",
        &[
            "--user".into(),
            format!("--unit={UNIT}"),
            format!("--on-active={seconds}s"),
            "--timer-property=AccuracySec=100ms".into(),
            "--collect".into(),
            "--quiet".into(),
            trowel_exe(),
            "keyboard".into(),
            "unlock".into(),
        ],
    )
    .map_err(|e| format!("couldn't schedule the unlock, nothing was locked: {e}"))?;
    // 2. The state the panel counts down from, before any key goes quiet.
    let until = now() + seconds;
    let state = json!({"until": until, "seconds": seconds, "devices": devices});
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, state.to_string()).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())?;
    // 3. The keyboards. Any failure undoes the ones already off.
    for (i, name) in devices.iter().enumerate() {
        if let Err(e) = set_enabled(r, name, false) {
            for done in &devices[..i] {
                let _ = set_enabled(r, done, true);
            }
            let _ = std::fs::remove_file(path);
            let _ = r.run(
                "systemctl",
                &["--user".into(), "stop".into(), format!("{UNIT}.timer")],
            );
            return Err(e);
        }
    }
    Ok(state)
}

/// Turn every locked keyboard back on. Without state (already unlocked, or the file is gone),
/// every lockable keyboard is enabled, which is harmless.
pub fn unlock(r: &mut dyn Runner, path: &std::path::Path) -> Result<Vec<String>, String> {
    let recorded: Vec<String> = std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| {
            v["devices"].as_array().map(|a| {
                a.iter()
                    .filter_map(|d| d.as_str().map(str::to_string))
                    .collect()
            })
        })
        .unwrap_or_default();
    let devices = if recorded.is_empty() {
        keyboards(r)?
    } else {
        recorded
    };
    let mut errors = Vec::new();
    for name in &devices {
        if let Err(e) = set_enabled(r, name, true) {
            errors.push(e);
        }
    }
    let _ = std::fs::remove_file(path);
    // From the timer itself this stops a timer that has already fired: harmless.
    let _ = r.run(
        "systemctl",
        &["--user".into(), "stop".into(), format!("{UNIT}.timer")],
    );
    if errors.is_empty() {
        Ok(devices)
    } else {
        Err(errors.join("; "))
    }
}

pub fn status(path: &std::path::Path) -> Value {
    let state = std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok());
    match state {
        Some(v) if v["until"].as_u64().unwrap_or(0) > now() => {
            let left = v["until"].as_u64().unwrap_or(0) - now();
            json!({"locked": true, "until": v["until"], "seconds_left": left, "devices": v["devices"]})
        }
        _ => json!({"locked": false}),
    }
}

pub fn run_cli(action: &str, seconds: Option<u64>) -> Result<(), String> {
    let path = state_path();
    let mut sys = System;
    match action {
        "lock" => {
            let secs = seconds.ok_or("how long? trowel keyboard lock <seconds>")?;
            let state = lock(&mut sys, secs, &path)?;
            println!("{state}");
        }
        "unlock" => {
            let devices = unlock(&mut sys, &path)?;
            println!("{}", json!({"unlocked": devices}));
        }
        _ => println!("{}", status(&path)),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Records every command; `fail` makes the first command containing that text fail.
    struct Fake {
        calls: Vec<String>,
        fail: Option<&'static str>,
    }

    impl Runner for Fake {
        fn run(&mut self, program: &str, args: &[String]) -> Result<String, String> {
            let line = format!("{program} {}", args.join(" "));
            self.calls.push(line.clone());
            if let Some(f) = self.fail {
                if line.contains(f) {
                    self.fail = None;
                    return Err("boom".into());
                }
            }
            if line == "hyprctl -j devices" {
                return Ok(r#"{"keyboards":[
                    {"name":"power-button"},{"name":"video-bus"},
                    {"name":"at-translated-set-2-keyboard"},{"name":"zeb-sheer-a-keyboard"},
                    {"name":"hl-virtual-keyboard-fcitx5"},{"name":"oneplus-nord-buds-3-pro-(avrcp)"},
                    {"name":"intel-hid-events"}]}"#
                    .into());
            }
            Ok(if program == "hyprctl" { "ok" } else { "" }.into())
        }
    }

    fn tmp_state() -> PathBuf {
        std::env::temp_dir().join(format!(
            "trowel-kbd-{}-{}/keyboard-lock.json",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn only_typing_devices_lock() {
        assert!(lockable("at-translated-set-2-keyboard"));
        assert!(lockable("zeb-sheer-a-keyboard"));
        assert!(lockable("intel-hid-events"));
        assert!(!lockable("power-button"));
        assert!(!lockable("hl-virtual-keyboard-fcitx5"));
        assert!(!lockable("video-bus"));
        assert!(!lockable("sof-soundwire-jack"));
        assert!(!lockable("oneplus-nord-buds-3-pro-(avrcp)"));
    }

    #[test]
    fn names_are_lua_string_literals() {
        assert_eq!(lua_string("kbd").unwrap(), "\"kbd\"");
        assert_eq!(lua_string("a\"b\\c").unwrap(), "\"a\\\"b\\\\c\"");
        assert!(lua_string("evil\n}) os.execute(").is_none());
    }

    #[test]
    fn unlock_is_scheduled_before_any_key_goes_off() {
        let path = tmp_state();
        let mut r = Fake {
            calls: vec![],
            fail: None,
        };
        let state = lock(&mut r, 60, &path).unwrap();
        let timer = r
            .calls
            .iter()
            .position(|c| c.starts_with("systemd-run"))
            .unwrap();
        let first_off = r
            .calls
            .iter()
            .position(|c| c.contains("enabled = false"))
            .unwrap();
        assert!(timer < first_off, "{:?}", r.calls);
        assert!(r.calls[timer].contains("--on-active=60s"));
        assert!(r.calls[timer].ends_with("keyboard unlock"));
        let off: Vec<_> = r
            .calls
            .iter()
            .filter(|c| c.contains("enabled = false"))
            .collect();
        assert_eq!(off.len(), 3, "{off:?}");
        assert!(!off
            .iter()
            .any(|c| c.contains("power-button") || c.contains("virtual")));
        assert_eq!(state["devices"].as_array().unwrap().len(), 3);
        assert_eq!(status(&path)["locked"], true);

        let mut r = Fake {
            calls: vec![],
            fail: None,
        };
        let back = unlock(&mut r, &path).unwrap();
        assert_eq!(back.len(), 3);
        assert_eq!(
            r.calls
                .iter()
                .filter(|c| c.contains("enabled = true"))
                .count(),
            3
        );
        assert!(!path.exists());
        assert_eq!(status(&path)["locked"], false);
    }

    #[test]
    fn no_timer_no_lock() {
        let path = tmp_state();
        let mut r = Fake {
            calls: vec![],
            fail: Some("systemd-run"),
        };
        assert!(lock(&mut r, 60, &path).is_err());
        assert!(!r.calls.iter().any(|c| c.contains("enabled = false")));
        assert!(!path.exists());
    }

    #[test]
    fn a_failed_disable_turns_the_others_back_on() {
        let path = tmp_state();
        let mut r = Fake {
            calls: vec![],
            fail: Some("zeb-sheer-a-keyboard\", enabled = false"),
        };
        assert!(lock(&mut r, 60, &path).is_err());
        // at-translated (sorted first) went off, then came back on.
        assert!(r
            .calls
            .iter()
            .any(|c| c.contains("at-translated-set-2-keyboard\", enabled = true")));
        assert!(!path.exists());
        assert!(r
            .calls
            .last()
            .unwrap()
            .contains("stop trowel-keyboard-unlock.timer"));
    }

    #[test]
    fn duration_is_capped() {
        let path = tmp_state();
        let mut r = Fake {
            calls: vec![],
            fail: None,
        };
        assert!(lock(&mut r, 0, &path).is_err());
        assert!(lock(&mut r, MAX_SECONDS + 1, &path).is_err());
        assert!(r.calls.is_empty());
    }
}
