//! Wall-clock helpers for timers and snippet placeholders: Unix seconds and the local time zone
//! through libc (`localtime_r`, `mktime`, `strftime`), so no date crate is needed.

use std::time::{SystemTime, UNIX_EPOCH};

pub fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Local {
    pub year: i32,
    /// 1…12
    pub month: i32,
    pub day: i32,
    pub hour: i32,
    pub minute: i32,
    pub second: i32,
}

fn tm_of(t: f64) -> libc::tm {
    let secs = t.floor() as libc::time_t;
    // SAFETY: localtime_r writes into the zeroed tm we own.
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&secs, &mut tm);
        tm
    }
}

pub fn local(t: f64) -> Local {
    let tm = tm_of(t);
    Local {
        year: tm.tm_year + 1900,
        month: tm.tm_mon + 1,
        day: tm.tm_mday,
        hour: tm.tm_hour,
        minute: tm.tm_min,
        second: tm.tm_sec,
    }
}

/// Unix seconds of a local date and time (DST resolved by `mktime`).
pub fn from_local(l: Local) -> f64 {
    // SAFETY: mktime reads and normalizes the tm we own.
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        tm.tm_year = l.year - 1900;
        tm.tm_mon = l.month - 1;
        tm.tm_mday = l.day;
        tm.tm_hour = l.hour;
        tm.tm_min = l.minute;
        tm.tm_sec = l.second;
        tm.tm_isdst = -1;
        libc::mktime(&mut tm) as f64
    }
}

/// `strftime` in the local zone. Formats longer than 64 bytes or with a NUL are refused (they
/// come from snippet bodies), and so is output over 256 bytes.
pub fn format(fmt: &str, t: f64) -> Option<String> {
    if fmt.len() > 64 || fmt.contains('\0') {
        return None;
    }
    let cfmt = std::ffi::CString::new(fmt).ok()?;
    let tm = tm_of(t);
    let mut buf = [0u8; 256];
    // SAFETY: the buffer and its length are ours; strftime writes at most that many bytes.
    let n = unsafe {
        libc::strftime(
            buf.as_mut_ptr() as *mut libc::c_char,
            buf.len(),
            cfmt.as_ptr(),
            &tm,
        )
    };
    if n == 0 && !fmt.is_empty() {
        return None;
    }
    Some(String::from_utf8_lossy(&buf[..n]).into_owned())
}

/// `m:ss` / `h:mm:ss`, rounding up so a timer never shows 0:00 while it still runs.
pub fn countdown(seconds: f64) -> String {
    let s = seconds.max(0.0).ceil() as u64;
    let (h, m, s) = (s / 3600, (s / 60) % 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_roundtrip_and_format() {
        let t = 1_793_000_000.0;
        let l = local(t);
        assert_eq!(from_local(l), t);
        assert_eq!(format("%Y", t).unwrap(), l.year.to_string());
        assert!(format(&"%Y".repeat(40), t).is_none());
    }

    #[test]
    fn countdowns() {
        assert_eq!(countdown(0.2), "0:01");
        assert_eq!(countdown(65.0), "1:05");
        assert_eq!(countdown(3725.0), "1:02:05");
        assert_eq!(countdown(-3.0), "0:00");
    }
}
