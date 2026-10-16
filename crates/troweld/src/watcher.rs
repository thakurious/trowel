//! inotify watcher. `upstream/Sources/DigUpApp/FolderWatcher.swift` + Engine 0.75 s debounce.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use notify::event::{CreateKind, ModifyKind, RenameMode};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use trowel_core::Rules;

pub const DEBOUNCE: Duration = Duration::from_millis(750);
/// Code watcher: wait until quiet this long, or at most 10 minutes while it keeps changing.
pub const CODE_QUIET: Duration = Duration::from_secs(2);
pub const CODE_LONGEST_WAIT: Duration = Duration::from_secs(600);

/// Whether the crawl goes into this folder once it reached the folder's parent. Hidden folders
/// below a root are never walked. The watcher puts an inotify watch only where this holds, and a
/// change anywhere else is dropped: it would ask for a sync that finds nothing new
/// (`upstream/Sources/DigUpApp/Engine.swift` `foldersChanged` + `Rules.isWalked`, 75d95d0).
/// A recursive watch also covered `target/`, `node_modules/` and `.git/` (49k watches for 10k
/// walked folders here), and every build there restarted the code sync.
pub type Walks = Arc<dyn Fn(&Path) -> bool + Send + Sync>;

pub struct FolderWatch {
    _watcher: Arc<Mutex<RecommendedWatcher>>,
    overflow: Arc<Mutex<bool>>,
}

impl FolderWatch {
    /// Every folder below the roots that is not hidden.
    pub fn start(
        roots: Vec<PathBuf>,
        debounce: Duration,
        on_change: impl Fn(Vec<PathBuf>) + Send + 'static,
    ) -> notify::Result<Self> {
        start_inner(roots, debounce, None, Arc::new(|_| true), on_change)
    }

    /// Only the folders `walks` lets the crawl into.
    pub fn start_scoped(
        roots: Vec<PathBuf>,
        debounce: Duration,
        walks: Walks,
        on_change: impl Fn(Vec<PathBuf>) + Send + 'static,
    ) -> notify::Result<Self> {
        start_inner(roots, debounce, None, walks, on_change)
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
        walks: Walks,
        on_change: impl Fn(Vec<PathBuf>) + Send + 'static,
    ) -> notify::Result<Self> {
        start_inner(roots, quiet, Some(longest), walks, on_change)
    }
}

fn start_inner(
    roots: Vec<PathBuf>,
    debounce: Duration,
    longest: Option<Duration>,
    walks: Walks,
    on_change: impl Fn(Vec<PathBuf>) + Send + 'static,
) -> notify::Result<FolderWatch> {
    let (tx, rx) = mpsc::channel::<Result<Event, notify::Error>>();
    let overflow = Arc::new(Mutex::new(false));
    let watcher = Arc::new(Mutex::new(RecommendedWatcher::new(
        move |res| {
            let _ = tx.send(res);
        },
        notify::Config::default(),
    )?));
    let scope = Scope {
        roots: roots.iter().filter(|r| r.is_dir()).cloned().collect(),
        walks,
    };
    for root in &scope.roots {
        // A root that can't be watched at all is an error (the caller falls back to an
        // overflow rescan); a folder deeper down that fails only sets the overflow flag.
        watcher
            .lock()
            .expect("watcher")
            .watch(root, RecursiveMode::NonRecursive)?;
        watch_below(&watcher, root, &scope, &overflow);
    }
    let flag = Arc::clone(&overflow);
    let adder = Arc::clone(&watcher);
    thread::spawn(move || debounce_loop(rx, debounce, longest, flag, &adder, &scope, on_change));
    Ok(FolderWatch {
        _watcher: watcher,
        overflow,
    })
}

struct Scope {
    roots: Vec<PathBuf>,
    walks: Walks,
}

impl Scope {
    /// `Rules::walks_to`, except that the folder rule is not asked about the changed path when
    /// it is a file (a file named `build` is still a change; the config file is in no folder).
    fn accepts(&self, path: &Path) -> bool {
        let is_file = !path.is_dir();
        self.roots.iter().any(|root| {
            Rules::walks_to(path, root, |folder| {
                (is_file && folder == path) || (self.walks)(folder)
            })
        })
    }

    fn enters(&self, folder: &Path) -> bool {
        let hidden = folder
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with('.'));
        !hidden && (self.walks)(folder)
    }
}

/// Watch every folder below `dir` that the crawl walks (not `dir` itself). Symlinks are not
/// followed, as in the crawl.
fn watch_below(
    watcher: &Mutex<RecommendedWatcher>,
    dir: &Path,
    scope: &Scope,
    overflow: &Mutex<bool>,
) {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(folder) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            if !entry.file_type().is_ok_and(|t| t.is_dir()) {
                continue;
            }
            let path = entry.path();
            if !scope.enters(&path) {
                continue;
            }
            match watcher
                .lock()
                .expect("watcher")
                .watch(&path, RecursiveMode::NonRecursive)
            {
                Ok(()) => stack.push(path),
                Err(err) if is_gone(&err) => {}
                Err(_) => {
                    // Out of inotify watches (`max_user_watches`): F3, a rescan on the poll.
                    *overflow.lock().expect("overflow") = true;
                    return;
                }
            }
        }
    }
}

fn is_gone(err: &notify::Error) -> bool {
    match &err.kind {
        notify::ErrorKind::PathNotFound | notify::ErrorKind::WatchNotFound => true,
        notify::ErrorKind::Io(io) => io.kind() == std::io::ErrorKind::NotFound,
        _ => false,
    }
}

/// A folder that appeared (made, or moved in) needs its own watches.
fn adds_a_folder(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Create(CreateKind::Folder | CreateKind::Any)
            | EventKind::Modify(ModifyKind::Name(
                RenameMode::To | RenameMode::Both | RenameMode::Any
            ))
    )
}

fn debounce_loop(
    rx: mpsc::Receiver<Result<Event, notify::Error>>,
    debounce: Duration,
    longest: Option<Duration>,
    overflow: Arc<Mutex<bool>>,
    watcher: &Mutex<RecommendedWatcher>,
    scope: &Scope,
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
                    let new_folder = adds_a_folder(&event.kind);
                    for path in event.paths {
                        if !scope.accepts(&path) {
                            continue;
                        }
                        if new_folder && path.is_dir() {
                            let watched = watcher
                                .lock()
                                .expect("watcher")
                                .watch(&path, RecursiveMode::NonRecursive);
                            if watched.is_ok() {
                                watch_below(watcher, &path, scope, &overflow);
                            } else if watched.as_ref().is_err_and(|e| !is_gone(e)) {
                                *overflow.lock().expect("overflow") = true;
                            }
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
                    if matches!(err.kind, notify::ErrorKind::MaxFilesWatch)
                        || msg.contains("limit")
                        || msg.contains("ENOSPC")
                        || msg.contains("watch")
                    {
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

    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "trowel-watch-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn hidden_files_below_the_root_are_ignored() {
        let scope = Scope {
            roots: vec![PathBuf::from("/tmp/docs")],
            walks: Arc::new(|_| true),
        };
        assert!(!scope.accepts(Path::new("/tmp/docs/.cache/x")));
        assert!(scope.accepts(Path::new("/tmp/docs/note.txt")));
        assert!(!scope.accepts(Path::new("/tmp/other/note.txt")));
        // A root inside a hidden folder (`~/.config/trowel`) still counts.
        let config = Scope {
            roots: vec![PathBuf::from("/home/me/.config/trowel")],
            walks: Arc::new(|_| false),
        };
        assert!(config.accepts(Path::new("/home/me/.config/trowel/config.toml")));
    }

    /// A root under a dot-folder used to drop every event (`is_hidden` looked at the whole path),
    /// so hand edits of `~/.config/trowel/config.toml` never reloaded.
    #[test]
    fn root_inside_a_hidden_folder_reports_changes() {
        let base = temp("hidden");
        let dir = base.join(".config/trowel");
        fs::create_dir_all(&dir).unwrap();
        let (tx, rx) = channel();
        let _watch = FolderWatch::start_scoped(
            vec![dir.clone()],
            Duration::from_millis(80),
            Arc::new(|_| false),
            move |paths| {
                let _ = tx.send(paths);
            },
        )
        .unwrap();
        thread::sleep(Duration::from_millis(40));
        fs::write(dir.join("config.toml"), "a = 1").unwrap();
        let got = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(got.iter().any(|p| p.ends_with("config.toml")), "{got:?}");
        let _ = fs::remove_dir_all(base);
    }

    /// Writes under a folder the crawl skips (`target/`) don't ask for a sync; a new folder the
    /// crawl walks gets watched as it appears.
    #[test]
    fn skipped_folders_are_not_watched_and_new_ones_are() {
        let dir = temp("scoped");
        fs::create_dir_all(dir.join("target/debug")).unwrap();
        fs::create_dir_all(dir.join("src")).unwrap();
        let (tx, rx) = channel();
        let walks: Walks = Arc::new(|f: &Path| f.file_name().is_none_or(|n| n != "target"));
        let _watch = FolderWatch::start_scoped(
            vec![dir.clone()],
            Duration::from_millis(80),
            walks,
            move |paths| {
                let _ = tx.send(paths);
            },
        )
        .unwrap();
        thread::sleep(Duration::from_millis(40));
        fs::write(dir.join("target/debug/x.o"), "a").unwrap();
        fs::write(dir.join("target/y.o"), "a").unwrap();
        assert!(
            rx.recv_timeout(Duration::from_millis(400)).is_err(),
            "target/ is skipped"
        );
        fs::create_dir_all(dir.join("src/deep")).unwrap();
        let first = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(first.iter().any(|p| p.ends_with("src/deep")), "{first:?}");
        fs::write(dir.join("src/deep/lib.rs"), "fn a() {}").unwrap();
        let second = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(second.iter().any(|p| p.ends_with("lib.rs")), "{second:?}");
        let _ = fs::remove_dir_all(dir);
    }
}
