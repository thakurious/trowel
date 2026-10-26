//! Phase 9 (`PHASE9-PLAN.md`): the Island, process monitor, system commands, timers and snippets.
//! The same code serves `troweld` and `troweld --mock`; what differs (state file, clipboard, the
//! commands it may run, where process samples come from) is the [`Host`].

use std::sync::Arc;

use serde_json::Value;
use trowel_proto::{ErrorCode, Op};

use crate::state::StateDb;

pub mod island;

/// A connection's outgoing lines, for events pushed to a subscriber. `id` is unique per
/// connection; `alive` turns false once it closed.
#[derive(Clone)]
pub struct Sink {
    pub id: u64,
    push: Arc<dyn Fn(String) + Send + Sync>,
    alive: Arc<dyn Fn() -> bool + Send + Sync>,
}

impl Sink {
    pub fn new(
        push: impl Fn(String) + Send + Sync + 'static,
        alive: impl Fn() -> bool + Send + Sync + 'static,
    ) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Self {
            id: NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            push: Arc::new(push),
            alive: Arc::new(alive),
        }
    }

    pub fn send(&self, line: String) {
        (self.push)(line)
    }

    pub fn alive(&self) -> bool {
        (self.alive)()
    }
}

pub type OpResult = Result<Value, (ErrorCode, String)>;

/// What the desk needs from the daemon around it.
pub trait Host: Send + Sync {
    /// Runs `f` on `state.sqlite`, opening it on first use.
    fn with_state(
        &self,
        f: &mut dyn FnMut(&StateDb) -> Result<Value, trowel_core::DbError>,
    ) -> Result<Value, String>;
    /// Puts `text` on the clipboard and, unless `copy_only`, sends the paste keystroke (D21).
    fn paste_text(&self, text: &str, copy_only: bool) -> Result<(), String>;
    /// Runs an allow-listed argv detached (system commands, keyboard unlock).
    fn spawn(&self, argv: &[String]) -> Result<(), String>;
}

pub struct Desk {
    #[allow(dead_code)]
    host: Arc<dyn Host>,
}

impl Desk {
    pub fn new(host: Arc<dyn Host>) -> Arc<Self> {
        Arc::new(Self { host })
    }

    /// Capabilities this build serves, for `hello`.
    pub fn capabilities(&self) -> &'static [&'static str] {
        &[]
    }

    /// Handles one v3 op (`Op::is_v3`). `sink` is the caller's connection, for subscriptions.
    pub fn handle(&self, op: Op, _sink: Sink) -> OpResult {
        Err((
            ErrorCode::NotReady,
            format!("{} is not built yet", op.name()),
        ))
    }

    /// The connection closed: drop its subscriptions.
    pub fn forget(&self, _sink: &Sink) {}
}
