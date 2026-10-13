//! Backfill gate. `PORT-MAP.md` PowerPolicy: sysfs AC vs battery.

use std::fs;
use std::path::Path;

/// Why the backfill must wait, or `None` when it may run.
/// `upstream/Sources/DigUpApp/PowerPolicy.swift` — battery only; thermal/ppd later.
pub fn backfill_blocker(index_on_battery: bool) -> Option<String> {
    if index_on_battery || !on_battery() {
        None
    } else {
        Some("on battery".into())
    }
}

pub fn on_battery() -> bool {
    let dir = Path::new("/sys/class/power_supply");
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    let mut saw_mains = false;
    let mut mains_online = false;
    let mut discharging = false;
    for entry in entries.flatten() {
        let path = entry.path();
        let kind = fs::read_to_string(path.join("type")).unwrap_or_default();
        let kind = kind.trim();
        if kind.eq_ignore_ascii_case("Mains") {
            saw_mains = true;
            let online = fs::read_to_string(path.join("online")).unwrap_or_default();
            mains_online |= online.trim() == "1";
        }
        if kind.eq_ignore_ascii_case("Battery") {
            let status = fs::read_to_string(path.join("status")).unwrap_or_default();
            discharging |= status.trim().eq_ignore_ascii_case("Discharging");
        }
    }
    if saw_mains {
        !mains_online
    } else {
        discharging
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn battery_gate_respects_setting() {
        assert!(backfill_blocker(true).is_none());
        let blocked = backfill_blocker(false);
        if on_battery() {
            assert_eq!(blocked.as_deref(), Some("on battery"));
        } else {
            assert!(blocked.is_none());
        }
    }
}
