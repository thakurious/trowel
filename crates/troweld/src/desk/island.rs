//! The Island's activity list (P-D29, `PHASE9-PLAN.md` §1.1). Memory only: producers (the
//! keyboard lock, timers, `trowel island post`) post, subscribers (the panel plugin) get the whole
//! list, highest priority first, on every change.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::json;
use trowel_proto::{IslandActivity, IslandState};

use super::Sink;

struct Entry {
    activity: IslandActivity,
    /// Posting order, so equal priorities keep the older one first.
    seq: u64,
    expires: Option<Instant>,
}

#[derive(Default)]
pub struct Island {
    entries: Mutex<HashMap<String, Entry>>,
    subscribers: Mutex<Vec<Sink>>,
    seq: Mutex<u64>,
}

impl Island {
    /// Adds or replaces `activity` (same id) and tells subscribers.
    pub fn post(&self, activity: IslandActivity) {
        let seq = {
            let mut seq = self.seq.lock().expect("seq");
            *seq += 1;
            *seq
        };
        let expires = activity
            .ttl
            .filter(|t| t.is_finite() && *t > 0.0)
            .map(|t| Instant::now() + Duration::from_secs_f64(t));
        {
            let mut entries = self.entries.lock().expect("island");
            // A replaced activity keeps its place among equals.
            let seq = entries.get(&activity.id).map(|e| e.seq).unwrap_or(seq);
            entries.insert(
                activity.id.clone(),
                Entry {
                    activity,
                    seq,
                    expires,
                },
            );
        }
        self.broadcast();
    }

    /// True when something was removed.
    pub fn dismiss(&self, id: &str) -> bool {
        let removed = self.entries.lock().expect("island").remove(id).is_some();
        if removed {
            self.broadcast();
        }
        removed
    }

    pub fn get(&self, id: &str) -> Option<IslandActivity> {
        self.prune();
        self.entries
            .lock()
            .expect("island")
            .get(id)
            .map(|e| e.activity.clone())
    }

    /// The list subscribers see: expired ones dropped, highest priority first, then oldest.
    pub fn list(&self) -> IslandState {
        self.prune();
        let entries = self.entries.lock().expect("island");
        let mut all: Vec<&Entry> = entries.values().collect();
        all.sort_by(|a, b| {
            b.activity
                .priority
                .cmp(&a.activity.priority)
                .then(a.seq.cmp(&b.seq))
        });
        IslandState {
            activities: all.into_iter().map(|e| e.activity.clone()).collect(),
        }
    }

    pub fn subscribe(&self, sink: Sink) {
        let state = self.list();
        sink.send(json!({"event": "island", "activities": state.activities}).to_string());
        let mut subs = self.subscribers.lock().expect("subs");
        subs.retain(|s| s.alive() && s.id != sink.id);
        subs.push(sink);
    }

    pub fn forget(&self, sink: &Sink) {
        self.subscribers
            .lock()
            .expect("subs")
            .retain(|s| s.id != sink.id);
    }

    /// Drops expired activities; tells subscribers when that changed anything.
    pub fn prune(&self) -> bool {
        let now = Instant::now();
        let changed = {
            let mut entries = self.entries.lock().expect("island");
            let before = entries.len();
            entries.retain(|_, e| e.expires.is_none_or(|t| t > now));
            entries.len() != before
        };
        if changed {
            self.broadcast();
        }
        changed
    }

    /// The next TTL deadline, for the desk's ticker.
    pub fn next_expiry(&self) -> Option<Instant> {
        self.entries
            .lock()
            .expect("island")
            .values()
            .filter_map(|e| e.expires)
            .min()
    }

    /// Sends one event line (e.g. `island_action`) to every subscriber.
    pub fn notify(&self, event: serde_json::Value) {
        let line = event.to_string();
        let mut subs = self.subscribers.lock().expect("subs");
        subs.retain(|s| s.alive());
        for s in subs.iter() {
            s.send(line.clone());
        }
    }

    fn broadcast(&self) {
        let state = {
            let entries = self.entries.lock().expect("island");
            let mut all: Vec<&Entry> = entries.values().collect();
            all.sort_by(|a, b| {
                b.activity
                    .priority
                    .cmp(&a.activity.priority)
                    .then(a.seq.cmp(&b.seq))
            });
            all.into_iter()
                .map(|e| e.activity.clone())
                .collect::<Vec<_>>()
        };
        let line = json!({"event": "island", "activities": state}).to_string();
        let mut subs = self.subscribers.lock().expect("subs");
        subs.retain(|s| s.alive());
        for s in subs.iter() {
            s.send(line.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn act(id: &str, priority: i32) -> IslandActivity {
        serde_json::from_value(json!({"id": id, "title": id, "priority": priority})).unwrap()
    }

    fn recorder() -> (Sink, Arc<Mutex<Vec<serde_json::Value>>>) {
        let got = Arc::new(Mutex::new(Vec::new()));
        let g = Arc::clone(&got);
        let sink = Sink::new(
            move |line| g.lock().unwrap().push(serde_json::from_str(&line).unwrap()),
            || true,
        );
        (sink, got)
    }

    fn ids(v: &serde_json::Value) -> Vec<String> {
        v["activities"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["id"].as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn priority_then_age_and_replace_keeps_place() {
        let island = Island::default();
        let (sink, got) = recorder();
        island.subscribe(sink);
        island.post(act("timer:1", 50));
        island.post(act("timer:2", 50));
        island.post(act("keylock", 100));
        let last = got.lock().unwrap().last().cloned().unwrap();
        assert_eq!(ids(&last), ["keylock", "timer:1", "timer:2"]);
        // Re-posting timer:1 (a new countdown) keeps it ahead of timer:2.
        island.post(act("timer:1", 50));
        let last = got.lock().unwrap().last().cloned().unwrap();
        assert_eq!(ids(&last), ["keylock", "timer:1", "timer:2"]);
        assert!(island.dismiss("keylock"));
        assert!(!island.dismiss("keylock"));
        assert_eq!(
            ids(&got.lock().unwrap().last().cloned().unwrap()),
            ["timer:1", "timer:2"]
        );
        // First message is the snapshot on subscribe (empty), then one per change.
        assert_eq!(got.lock().unwrap().len(), 1 + 3 + 1 + 1);
    }

    #[test]
    fn ttl_expires() {
        let island = Island::default();
        let mut a = act("toast", 10);
        a.ttl = Some(0.05);
        island.post(a);
        assert_eq!(island.list().activities.len(), 1);
        assert!(island.next_expiry().is_some());
        std::thread::sleep(Duration::from_millis(80));
        assert!(island.list().activities.is_empty());
    }

    #[test]
    fn dead_subscribers_are_dropped() {
        let island = Island::default();
        let alive = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let a = Arc::clone(&alive);
        let count = Arc::new(Mutex::new(0));
        let c = Arc::clone(&count);
        island.subscribe(Sink::new(
            move |_| *c.lock().unwrap() += 1,
            move || a.load(std::sync::atomic::Ordering::Relaxed),
        ));
        island.post(act("x", 1));
        alive.store(false, std::sync::atomic::Ordering::Relaxed);
        island.post(act("y", 1));
        assert_eq!(*count.lock().unwrap(), 2, "snapshot + first post only");
    }
}
