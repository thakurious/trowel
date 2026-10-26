//! `trowel island …` and `trowel timer …`: the Island and timers from a shell (Phase 9). Any
//! script can post an activity: `trowel island post backup "Backing up" --progress 0.4`.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use serde_json::{json, Value};

use crate::clip_cmd::{request_at, socket_path};

pub struct Post {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub icon: Option<String>,
    pub seconds: Option<f64>,
    pub progress: Option<f64>,
    pub value: Option<String>,
    pub priority: Option<i32>,
    pub ttl: Option<f64>,
    pub buttons: Vec<String>,
    pub alert: bool,
}

fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// `id:Label` (or just `Label`, whose id is the label lowercased).
fn button(spec: &str) -> Value {
    let (id, label) = match spec.split_once(':') {
        Some((id, label)) => (id.to_string(), label.to_string()),
        None => (spec.to_lowercase().replace(' ', "-"), spec.to_string()),
    };
    json!({"id": id, "label": label})
}

pub fn activity(p: &Post) -> Value {
    let mut a = json!({"id": p.id, "title": p.title, "icon": p.icon.clone().unwrap_or_default()});
    if let Some(s) = &p.subtitle {
        a["subtitle"] = json!(s);
    }
    if let Some(s) = p.seconds {
        a["ends_at"] = json!(now() + s);
        a["total"] = json!(s);
    }
    if let Some(v) = p.progress {
        a["progress"] = json!(v);
    }
    if let Some(v) = &p.value {
        a["value"] = json!(v);
    }
    if let Some(v) = p.priority {
        a["priority"] = json!(v);
    }
    if let Some(v) = p.ttl {
        a["ttl"] = json!(v);
    }
    if p.alert {
        a["alert"] = json!(true);
    }
    a["actions"] = Value::Array(p.buttons.iter().map(|b| button(b)).collect());
    a
}

fn call(req: Value) -> Result<Value, String> {
    request_at(&socket_path(), req, Duration::from_secs(10))
}

pub fn island_post(p: &Post) -> Result<(), String> {
    call(json!({"op": "island_post", "activity": activity(p)}))?;
    Ok(())
}

pub fn island_dismiss(id: &str) -> Result<(), String> {
    call(json!({"op": "island_dismiss", "activity_id": id}))?;
    Ok(())
}

pub fn island_list() -> Result<(), String> {
    let v = call(json!({"op": "island_list"}))?;
    println!("{}", v["activities"]);
    Ok(())
}

/// Prints each button press (`island_action` events) as a JSON line until interrupted, or the
/// first one for `id` with `once`.
pub fn island_watch(id: Option<&str>, once: bool) -> Result<(), String> {
    let stream = UnixStream::connect(socket_path()).map_err(|e| format!("no daemon ({e})"))?;
    let mut w = stream.try_clone().map_err(|e| e.to_string())?;
    for msg in [
        json!({"id":1,"op":"hello","protocol":trowel_proto::PROTOCOL_VERSION,"client":"cli","pid":std::process::id()}),
        json!({"id":2,"op":"subscribe","topics":["island"]}),
    ] {
        writeln!(w, "{msg}").map_err(|e| e.to_string())?;
    }
    for line in BufReader::new(stream).lines() {
        let line = line.map_err(|e| e.to_string())?;
        let Ok(v) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if v["event"] != "island_action" || id.is_some_and(|i| v["activity_id"] != i) {
            continue;
        }
        println!(
            "{}",
            json!({"activity_id": v["activity_id"], "action": v["action"]})
        );
        if once {
            return Ok(());
        }
    }
    Ok(())
}

pub fn timer_start(text: &str) -> Result<(), String> {
    let t = call(json!({"op": "timer_start", "text": text}))?;
    println!("{}", strip_envelope(t));
    Ok(())
}

pub fn timer_list() -> Result<(), String> {
    let v = call(json!({"op": "timer_list"}))?;
    println!("{}", v["timers"]);
    Ok(())
}

pub fn timer_cancel(id: i64) -> Result<(), String> {
    call(json!({"op": "timer_cancel", "timer_id": id}))?;
    Ok(())
}

fn strip_envelope(mut v: Value) -> Value {
    if let Some(o) = v.as_object_mut() {
        o.remove("id");
        o.remove("ok");
    }
    v
}

/// Best effort, for `trowel keyboard`: the lock works without a daemon, the island just stays
/// empty.
pub fn try_request(req: Value) {
    let _ = request_at(&socket_path(), req, Duration::from_millis(1500));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn post_builds_an_activity() {
        let a = activity(&Post {
            id: "backup".into(),
            title: "Backing up".into(),
            subtitle: None,
            icon: Some("progress".into()),
            seconds: None,
            progress: Some(0.4),
            value: Some("40%".into()),
            priority: None,
            ttl: Some(30.0),
            buttons: vec!["stop:Stop".into(), "Open Log".into()],
            alert: false,
        });
        assert_eq!(a["progress"], 0.4);
        assert_eq!(a["actions"][0], json!({"id": "stop", "label": "Stop"}));
        assert_eq!(
            a["actions"][1],
            json!({"id": "open-log", "label": "Open Log"})
        );
        assert!(a.get("ends_at").is_none());
        let parsed: trowel_proto::IslandActivity = serde_json::from_value(a).unwrap();
        assert_eq!(parsed.priority, 50);
    }
}
