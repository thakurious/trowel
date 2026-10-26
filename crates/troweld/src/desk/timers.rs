//! Timers (P-D32, `PHASE9-PLAN.md` §1.4): the parser for what people type (`25m tea`,
//! `1h 30m`, `at 15:30`, `timer 5 min pizza`), and the store. Running timers keep a wall-clock end
//! time in `state.sqlite`, so they survive the panel closing, a daemon restart and a suspend; the
//! desk's ticker fires them (one that ended while the machine slept fires on wake, marked late).

use serde_json::json;
use trowel_core::{DbError, SqlValue};
use trowel_proto::{IslandActivity, IslandButton, Timer, TimerSpec, TimerState};

use super::{clock, with_db, Host};

/// Longest timer: a day.
pub const MAX_SECONDS: f64 = 24.0 * 3600.0;
/// A timer that fired this much after its end (the machine was asleep) is marked late.
const LATE_AFTER: f64 = 5.0;
/// Finished timers stay listed (and on the island) this long unless dismissed.
pub const DONE_KEEP: f64 = 10.0 * 60.0;

fn unit_seconds(unit: &str) -> Option<f64> {
    match unit {
        "s" | "sec" | "secs" | "second" | "seconds" => Some(1.0),
        "m" | "min" | "mins" | "minute" | "minutes" => Some(60.0),
        "h" | "hr" | "hrs" | "hour" | "hours" => Some(3600.0),
        _ => None,
    }
}

fn number(s: &str) -> Option<f64> {
    if s.is_empty() || !s.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return None;
    }
    s.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// `25m`, `1h30m`, `1h30` (minutes after hours), `90s`, `1.5h`. None when any part isn't a unit.
fn compact_duration(token: &str) -> Option<f64> {
    let mut total = 0.0;
    let mut rest = token;
    let mut last_unit = 0.0;
    let mut parts = 0;
    while !rest.is_empty() {
        let n_end = rest
            .find(|c: char| !(c.is_ascii_digit() || c == '.'))
            .unwrap_or(rest.len());
        let n = number(&rest[..n_end])?;
        rest = &rest[n_end..];
        let u_end = rest
            .find(|c: char| c.is_ascii_digit() || c == '.')
            .unwrap_or(rest.len());
        let unit = &rest[..u_end];
        rest = &rest[u_end..];
        let mul = if unit.is_empty() {
            // A bare trailing number counts in the next smaller unit: `1h30` = 1 h 30 min.
            if parts == 0 || last_unit <= 1.0 {
                return None;
            }
            last_unit / 60.0
        } else {
            unit_seconds(unit)?
        };
        total += n * mul;
        last_unit = mul;
        parts += 1;
    }
    (parts > 0).then_some(total)
}

/// `15:30`, `3pm`, `3:30pm`, `9am` → (hour, minute).
fn clock_token(token: &str, meridiem: Option<&str>) -> Option<(i32, i32)> {
    let (body, suffix) = match token
        .strip_suffix("am")
        .or_else(|| token.strip_suffix("pm"))
    {
        Some(b) => (b, Some(&token[b.len()..])),
        None => (token, meridiem),
    };
    let (h, m) = match body.split_once(':') {
        Some((h, m)) if m.len() == 2 => (h.parse::<i32>().ok()?, m.parse::<i32>().ok()?),
        Some(_) => return None,
        None if suffix.is_some() => (body.parse::<i32>().ok()?, 0),
        None => return None,
    };
    if !(0..60).contains(&m) {
        return None;
    }
    let h = match suffix {
        Some(s) => {
            if !(1..=12).contains(&h) {
                return None;
            }
            (h % 12) + if s == "pm" { 12 } else { 0 }
        }
        None => h,
    };
    (0..24).contains(&h).then_some((h, m))
}

/// The next time the local clock shows `h:m` after `now` (today, else tomorrow).
fn next_clock(h: i32, m: i32, now: f64) -> f64 {
    let today = clock::local(now);
    let mut at = clock::from_local(clock::Local {
        hour: h,
        minute: m,
        second: 0,
        ..today
    });
    if at <= now {
        at = clock::from_local(clock::Local {
            day: today.day + 1,
            hour: h,
            minute: m,
            second: 0,
            ..today
        });
    }
    at
}

const FILLER: [&str; 4] = ["for", "in", "a", "and"];

/// What `text` would start. `bare_minutes`: a plain number is minutes (inside the Timer view, or
/// after the word "timer"); in the main list `25` alone is not a timer.
pub fn parse(text: &str, now: f64, bare_minutes: bool) -> Option<TimerSpec> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let lower: Vec<String> = words.iter().map(|w| w.to_lowercase()).collect();
    let mut used = vec![false; words.len()];
    let mut bare = bare_minutes;
    let mut i = 0;
    // "timer …", "start timer …", "set timer …", "countdown …"
    while i < lower.len() && matches!(lower[i].as_str(), "timer" | "start" | "set" | "countdown") {
        bare |= lower[i] == "timer" || lower[i] == "countdown";
        used[i] = true;
        i += 1;
    }
    let mut seconds = 0.0;
    let mut found = false;
    let mut at = None;
    let mut j = i;
    while j < lower.len() {
        let w = lower[j].as_str();
        let next = lower.get(j + 1).map(String::as_str);
        // "at 15:30", "at 3 pm", "15:30", "3pm"
        let after_at = w == "at";
        let (tok, k) = if after_at {
            (next, j + 1)
        } else {
            (Some(w), j)
        };
        if let Some(tok) = tok {
            let mer = lower
                .get(k + 1)
                .map(String::as_str)
                .filter(|m| *m == "am" || *m == "pm");
            let clock = clock_token(tok, mer)
                .filter(|_| after_at || tok.contains(':') || tok.ends_with('m') || mer.is_some());
            if let (Some((h, m)), None) = (clock, at) {
                if !found {
                    at = Some(next_clock(h, m, now));
                    used[j..=k + usize::from(mer.is_some())].fill(true);
                    j = k + 1 + usize::from(mer.is_some());
                    continue;
                }
            }
        }
        if at.is_none() {
            if let Some(s) = compact_duration(w) {
                seconds += s;
                found = true;
                used[j] = true;
                j += 1;
                continue;
            }
            if let (Some(n), Some(mul)) = (number(w), next.and_then(unit_seconds)) {
                seconds += n * mul;
                found = true;
                used[j] = true;
                used[j + 1] = true;
                j += 2;
                continue;
            }
        }
        j += 1;
    }
    if !found && at.is_none() && bare {
        if let Some(k) = (i..lower.len()).find(|&k| number(&lower[k]).is_some()) {
            seconds = number(&lower[k]).unwrap_or(0.0) * 60.0;
            found = true;
            used[k] = true;
        }
    }
    if let Some(t) = at {
        seconds = t - now;
    } else if !found {
        return None;
    }
    if !(1.0..=MAX_SECONDS).contains(&seconds) {
        return None;
    }
    let label: Vec<&str> = words
        .iter()
        .zip(&used)
        .zip(&lower)
        .filter(|((_, u), l)| !**u && !FILLER.contains(&l.as_str()))
        .map(|((w, _), _)| *w)
        .collect();
    let mut label = label.join(" ");
    if label.chars().count() > 60 {
        label = label.chars().take(60).collect();
    }
    Some(TimerSpec {
        seconds: seconds.round(),
        label,
        at,
    })
}

fn row(r: &trowel_core::database::QueryRow<'_>) -> Timer {
    let state = match r.text(3).as_deref() {
        Some("paused") => TimerState::Paused,
        Some("done") => TimerState::Done,
        _ => TimerState::Running,
    };
    Timer {
        id: r.int(0),
        label: r.text(1).unwrap_or_default(),
        total: r.double(2),
        state,
        ends_at: (!r.is_null(4)).then(|| r.double(4)),
        left: r.double(5),
        created: r.double(6),
        fired_at: (!r.is_null(7)).then(|| r.double(7)),
        late: r.int(8) != 0,
    }
}

const COLUMNS: &str = "id, label, total, state, ends_at, left, created, fired_at, late";

fn all(db: &crate::state::StateDb) -> Result<Vec<Timer>, DbError> {
    let mut out = Vec::new();
    db.db.query(
        &format!("SELECT {COLUMNS} FROM timers ORDER BY id"),
        &[],
        |r| {
            out.push(row(r));
            Ok(())
        },
    )?;
    Ok(out)
}

fn save(db: &crate::state::StateDb, t: &Timer) -> Result<(), DbError> {
    db.db.run(
        "UPDATE timers SET label = ?2, total = ?3, state = ?4, ends_at = ?5, left = ?6,
         fired_at = ?7, late = ?8 WHERE id = ?1",
        &[
            SqlValue::Int(t.id),
            SqlValue::Text(t.label.clone()),
            SqlValue::Double(t.total),
            SqlValue::Text(state_name(t.state).into()),
            SqlValue::optional_f64(t.ends_at),
            SqlValue::Double(t.left),
            SqlValue::optional_f64(t.fired_at),
            SqlValue::Int(i64::from(t.late)),
        ],
    )
}

fn state_name(s: TimerState) -> &'static str {
    match s {
        TimerState::Running => "running",
        TimerState::Paused => "paused",
        TimerState::Done => "done",
    }
}

/// `left` as of `now` for a running one.
fn fresh(mut t: Timer, now: f64) -> Timer {
    if let (TimerState::Running, Some(end)) = (t.state, t.ends_at) {
        t.left = (end - now).max(0.0);
    }
    t
}

pub fn start(host: &dyn Host, seconds: f64, label: &str, now: f64) -> Result<Timer, String> {
    if !(1.0..=MAX_SECONDS).contains(&seconds) {
        return Err(format!("a timer runs 1 s to 24 h, not {seconds} s"));
    }
    let label = label.trim().to_string();
    with_db(host, |db| {
        db.db.run(
            "INSERT INTO timers(label, total, state, ends_at, left, created)
             VALUES (?1, ?2, 'running', ?3, ?2, ?4)",
            &[
                SqlValue::Text(label.clone()),
                SqlValue::Double(seconds),
                SqlValue::Double(now + seconds),
                SqlValue::Double(now),
            ],
        )?;
        Ok(Timer {
            id: db.db.last_insert_id(),
            label: label.clone(),
            total: seconds,
            state: TimerState::Running,
            ends_at: Some(now + seconds),
            left: seconds,
            created: now,
            fired_at: None,
            late: false,
        })
    })
}

/// Every timer, finished ones older than [`DONE_KEEP`] dropped.
pub fn list(host: &dyn Host, now: f64) -> Result<Vec<Timer>, String> {
    with_db(host, |db| {
        db.db.run(
            "DELETE FROM timers WHERE state = 'done' AND fired_at < ?1",
            &[SqlValue::Double(now - DONE_KEEP)],
        )?;
        Ok(all(db)?
            .into_iter()
            .map(|t| fresh(t, now))
            .collect::<Vec<_>>())
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Change {
    Pause,
    Resume,
    Add(f64),
}

/// Pause, resume or add time. Adding to a finished timer starts it again with that much (snooze).
pub fn change(host: &dyn Host, id: i64, how: Change, now: f64) -> Result<Timer, String> {
    with_db(host, |db| {
        let Some(mut t) = all(db)?.into_iter().find(|t| t.id == id) else {
            return Err(DbError::new(format!("no timer {id}")));
        };
        t = fresh(t, now);
        match (how, t.state) {
            (Change::Pause, TimerState::Running) => {
                t.state = TimerState::Paused;
                t.ends_at = None;
            }
            (Change::Resume, TimerState::Paused) => {
                t.state = TimerState::Running;
                t.ends_at = Some(now + t.left);
            }
            (Change::Add(s), TimerState::Running) => {
                t.total += s;
                t.left += s;
                t.ends_at = t.ends_at.map(|e| e + s);
            }
            (Change::Add(s), TimerState::Paused) => {
                t.total += s;
                t.left += s;
            }
            (Change::Add(s), TimerState::Done) => {
                t.state = TimerState::Running;
                t.total = s;
                t.left = s;
                t.ends_at = Some(now + s);
                t.fired_at = None;
                t.late = false;
            }
            _ => {}
        }
        if t.total > MAX_SECONDS || t.left > MAX_SECONDS {
            return Err(DbError::new("a timer runs at most 24 h"));
        }
        save(db, &t)?;
        Ok(t)
    })
}

pub fn cancel(host: &dyn Host, id: i64) -> Result<bool, String> {
    with_db(host, |db| {
        let before = all(db)?.len();
        db.db
            .run("DELETE FROM timers WHERE id = ?1", &[SqlValue::Int(id)])?;
        Ok(all(db)?.len() != before)
    })
}

/// Marks running timers whose end has passed as done and returns them (to announce).
pub fn due(host: &dyn Host, now: f64) -> Result<Vec<Timer>, String> {
    with_db(host, |db| {
        let mut fired = Vec::new();
        for mut t in all(db)? {
            match (t.state, t.ends_at) {
                (TimerState::Running, Some(end)) if end <= now => {
                    t.state = TimerState::Done;
                    t.left = 0.0;
                    t.fired_at = Some(now);
                    t.late = now - end > LATE_AFTER;
                    save(db, &t)?;
                    fired.push(t);
                }
                _ => {}
            }
        }
        Ok(fired)
    })
}

/// The earliest end among running timers.
pub fn next_end(host: &dyn Host) -> Option<f64> {
    with_db(host, |db| {
        Ok(all(db)?
            .into_iter()
            .filter(|t| t.state == TimerState::Running)
            .filter_map(|t| t.ends_at)
            .reduce(f64::min))
    })
    .ok()
    .flatten()
}

fn button(id: &str, label: &str, danger: bool) -> IslandButton {
    IslandButton {
        id: id.into(),
        label: label.into(),
        danger,
    }
}

pub fn activity_id(id: i64) -> String {
    format!("timer:{id}")
}

/// What the island shows for `t` (priorities from `PHASE9-PLAN.md` §1.1).
pub fn activity(t: &Timer, now: f64) -> IslandActivity {
    let title = if t.label.is_empty() {
        "Timer".to_string()
    } else {
        t.label.clone()
    };
    let mut a: IslandActivity = serde_json::from_value(json!({
        "id": activity_id(t.id), "icon": "timer", "title": title,
    }))
    .expect("activity");
    match t.state {
        TimerState::Running => {
            a.subtitle = Some(format!("{} timer", clock::countdown(t.total)));
            a.ends_at = t.ends_at;
            a.total = Some(t.total);
            a.priority = 50;
            a.actions = vec![
                button("pause", "Pause", false),
                button("add60", "+1 min", false),
                button("cancel", "Cancel", true),
            ];
        }
        TimerState::Paused => {
            a.subtitle = Some("Paused".into());
            a.value = Some(clock::countdown(t.left));
            a.progress = (t.total > 0.0).then(|| 1.0 - t.left / t.total);
            a.priority = 40;
            a.actions = vec![
                button("resume", "Resume", false),
                button("cancel", "Cancel", true),
            ];
        }
        TimerState::Done => {
            a.subtitle = Some(if t.late {
                "Finished while asleep".into()
            } else {
                "Time's up".into()
            });
            a.value = Some("Done".into());
            a.priority = 90;
            a.alert = true;
            a.ttl = Some((t.fired_at.unwrap_or(now) + DONE_KEEP - now).max(1.0));
            a.actions = vec![
                button("add300", "+5 min", false),
                button("dismiss", "Dismiss", false),
            ];
        }
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-10-26 10:00 local.
    fn ten_am() -> f64 {
        clock::from_local(clock::Local {
            year: 2026,
            month: 10,
            day: 26,
            hour: 10,
            minute: 0,
            second: 0,
        })
    }

    fn p(text: &str, bare: bool) -> Option<(f64, String)> {
        parse(text, ten_am(), bare).map(|s| (s.seconds, s.label))
    }

    #[test]
    fn parser_table() {
        let now = ten_am();
        type Row<'a> = (&'a str, bool, Option<(f64, &'a str)>);
        let rows: &[Row] = &[
            ("25m", false, Some((1500.0, ""))),
            ("25m tea", false, Some((1500.0, "tea"))),
            ("tea 25m", false, Some((1500.0, "tea"))),
            ("1h 30m", false, Some((5400.0, ""))),
            ("1h30m", false, Some((5400.0, ""))),
            ("1h30", false, Some((5400.0, ""))),
            ("90s", false, Some((90.0, ""))),
            ("1.5h", false, Some((5400.0, ""))),
            ("5 min", false, Some((300.0, ""))),
            ("2 hours", false, Some((7200.0, ""))),
            ("timer 5 min pizza", false, Some((300.0, "pizza"))),
            ("timer 10s test", false, Some((10.0, "test"))),
            (
                "Timer for 3 minutes Boiled Eggs",
                false,
                Some((180.0, "Boiled Eggs")),
            ),
            ("timer 25", false, Some((1500.0, ""))),
            ("25", true, Some((1500.0, ""))),
            ("at 15:30", false, Some((5.5 * 3600.0, ""))),
            ("15:30 tea", false, Some((5.5 * 3600.0, "tea"))),
            ("at 3pm", false, Some((5.0 * 3600.0, ""))),
            ("at 3 pm call mom", false, Some((5.0 * 3600.0, "call mom"))),
            // Past today: tomorrow.
            ("at 9:00", false, Some((23.0 * 3600.0, ""))),
            ("25", false, None),
            ("tea", false, None),
            ("", true, None),
            ("0m", false, None),
            ("25 hours", false, None),
            ("5 ft to cm", false, None),
            ("at 25:00", false, None),
            ("1h30x", false, None),
        ];
        for (text, bare, want) in rows {
            let got = p(text, *bare);
            let want = want.map(|(s, l)| (s, l.to_string()));
            assert_eq!(got, want, "{text:?} (bare {bare})");
        }
        assert!(parse("at 15:30", now, false).unwrap().at.is_some());
        assert!(parse("25m", now, false).unwrap().at.is_none());
    }

    #[test]
    fn activities_follow_state() {
        let t = Timer {
            id: 7,
            label: "".into(),
            total: 300.0,
            state: TimerState::Running,
            ends_at: Some(1000.0),
            left: 300.0,
            created: 700.0,
            fired_at: None,
            late: false,
        };
        let a = activity(&t, 700.0);
        assert_eq!(a.id, "timer:7");
        assert_eq!(a.title, "Timer");
        assert_eq!(a.ends_at, Some(1000.0));
        assert_eq!(a.actions.len(), 3);
        let done = activity(
            &Timer {
                state: TimerState::Done,
                fired_at: Some(1000.0),
                ..t
            },
            1000.0,
        );
        assert!(done.alert && done.priority == 90 && done.ttl == Some(DONE_KEEP));
    }
}
