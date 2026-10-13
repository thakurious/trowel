//! Start-at-login switch: `systemctl --user enable troweld.socket`. Status is
//! read-only. `on`/`off` are explicit user actions from Settings.

use std::process::Command;

pub fn status() -> Result<(), String> {
    let out = match Command::new("systemctl")
        .args(["--user", "is-enabled", "troweld.socket"])
        .output()
    {
        Ok(o) => o,
        Err(_) => {
            println!("unavailable");
            return Ok(());
        }
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let err = String::from_utf8_lossy(&out.stderr);
    let line = text.trim();
    if line.is_empty() {
        if err.contains("Failed to connect") || err.contains("not been booted") {
            println!("unavailable");
        } else {
            println!("disabled");
        }
    } else {
        println!("{line}");
    }
    Ok(())
}

pub fn set(on: bool) -> Result<(), String> {
    let action = if on { "enable" } else { "disable" };
    let status = Command::new("systemctl")
        .args(["--user", action, "--now", "troweld.socket"])
        .status()
        .map_err(|e| format!("systemctl: {e}"))?;
    if !status.success() {
        return Err(format!("systemctl {action} troweld.socket failed"));
    }
    Ok(())
}
