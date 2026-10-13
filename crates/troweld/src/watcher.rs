//! inotify watcher. `upstream/Sources/DigUpApp/FolderWatcher.swift` + Engine 0.75 s debounce.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};

pub const DEBOUNCE: Duration = Duration::from_millis(750);
/// Code watcher: wait until quiet this long, or at most 10 minutes while it keeps changing.
pub const CODE_QUIET: Duration = Duration::from_secs(2);
pub const CODE_LONGEST_WAIT: Duration = Duration::from_secs(600);

pub struct FolderWatch {
    _watcher: RecommendedWatcher,
    overflow: Arc<Mutex<bool>>,
}

impl FolderWatch {
    pub fn start(
        roots: Vec<PathBuf>,
        debounce: Duration,
        on_change: impl Fn(Vec<PathBuf>) + Send + 'static,
    ) -> notify::Result<Self> {
        start_inner(roots, debounce, None, on_change)
    }

    pub fn overflowed(&self) -> bool {
        *self.overflow.lock().expect("overflow")
    }

    pub fn inject_overflow(&self) {
        *self.overflow.lock().expect("overflow") = true;
    }

    pub fn take_overflow(&self) -> bool {
        let mut flag = self.overflow.lock().expect("overflow");
        let was = *flag;
        *flag = false;
        was
    }

    pub fn start_code(
        roots: Vec<PathBuf>,
        quiet: Duration,
        longest: Duration,
        on_change: impl Fn(Vec<PathBuf>) + Send + 'static,
    ) -> notify::Result<Self> {
        start_inner(roots, quiet, Some(longest), on_change)
    }
}

fn start_inner(
    roots: Vec<PathBuf>,
    debounce: Duration,
    longest: Option<Duration>,
    on_change: impl Fn(Vec<PathBuf>) + Send + 'static,
) -> notify::Result<FolderWatch> {
    let (tx, rx) = mpsc::channel::<Result<Event, notify::Error>>();
    let overflow = Arc::new(Mutex::new(false));
    let flag = Arc::clone(&overflow);
    let mut watcher = RecommendedWatcher::new(
        move |res| {
            let _ = tx.send(res);
        },
        notify::Config::default(),
    )?;
    for root in &roots {
        if root.is_dir() {
            watcher.watch(root, RecursiveMode::Recursive)?;
        }
    }
    thread::spawn(move || debounce_loop(rx, debounce, longest, flag, on_change));
    Ok(FolderWatch {
        _watcher: watcher,
        overflow,
    })
}

fn debounce_loop(
    rx: mpsc::Receiver<Result<Event, notify::Error>>,
    debounce: Duration,
    longest: Option<Duration>,
    overflow: Arc<Mutex<bool>>,
    on_change: impl Fn(Vec<PathBuf>),
) {
    let mut pending: Vec<PathBuf> = Vec::new();
    let mut deadline: Option<Instant> = None;
    let mut burst: Option<Instant> = None;
    loop {
        let wait = deadline.map(|d| d.saturating_duration_since(Instant::now()));
        let msg = match wait {
            Some(d) => match rx.recv_timeout(d) {
                Ok(m) => Some(m),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => break,
            },
            None => match rx.recv() {
                Ok(m) => Some(m),
                Err(_) => break,
            },
        };
        if let Some(msg) = msg {
            match msg {
                Ok(event) => {
                    if event.need_rescan() {
                        *overflow.lock().expect("overflow") = true;
                    }
                    for path in event.paths {
                        if is_hidden(&path) {
                            continue;
                        }
                        if !pending.iter().any(|p| p == &path) {
                            pending.push(path);
                        }
                    }
                    if !pending.is_empty() {
                        let now = Instant::now();
                        if burst.is_none() {
                            burst = Some(now);
                        }
                        let quiet = now + debounce;
                        deadline = Some(match longest {
                            Some(max) => quiet.min(burst.unwrap() + max),
                            None => quiet,
                        });
                    }
                }
                Err(err) => {
                    let msg = err.to_string();
                    if msg.contains("limit") || msg.contains("ENOSPC") || msg.contains("watch") {
                        *overflow.lock().expect("overflow") = true;
                    }
                }
            }
        } else if !pending.is_empty() {
            let batch = std::mem::take(&mut pending);
            deadline = None;
            burst = None;
            on_change(batch);
        }
    }
}

fn is_hidden(path: &Path) -> bool {
    path.components()
        .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::mpsc::channel;

    #[test]
    fn debounce_fires_after_quiet() {
        let dir = std::env::temp_dir().join(format!(
            "trowel-watch-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let (tx, rx) = channel();
        let _watch =
            FolderWatch::start(vec![dir.clone()], Duration::from_millis(80), move |paths| {
                let _ = tx.send(paths);
            })
            .unwrap();
        thread::sleep(Duration::from_millis(40));
        fs::write(dir.join("note.txt"), "a").unwrap();
        fs::write(dir.join("note.txt"), "ab").unwrap();
        let first = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(first.iter().any(|p| p.ends_with("note.txt")), "{first:?}");
        let second = rx.recv_timeout(Duration::from_millis(200));
        assert!(second.is_err(), "debounce should collapse bursts");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn hidden_files_are_ignored() {
        assert!(is_hidden(Path::new("/tmp/.cache/x")));
        assert!(!is_hidden(Path::new("/tmp/docs/note.txt")));
    }
}
