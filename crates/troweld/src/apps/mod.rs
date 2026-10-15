//! Application index (D19, `LAUNCHER-PLAN.md` L2): what Omarchy's launcher lists, ranked the way
//! it ranks, launched the way it launches.

pub mod desktop_entry;
pub mod icons;
pub mod rank;

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use trowel_proto::{AppAction, AppRow, PathValue};

use crate::actions;
use desktop_entry::{DesktopEntry, Locale};
use icons::IconIndex;
use rank::Rankable;

/// Where entries and hides come from. `from_env` is the live desktop; tests build their own.
#[derive(Debug, Clone)]
pub struct AppsEnv {
    pub app_dirs: Vec<PathBuf>,
    pub hide_scan_dirs: Vec<PathBuf>,
    pub desktops: Vec<String>,
    /// `$OMARCHY_PATH/default/omarchy/launcher.hides` (read-only; `AppLibrary.qml:221`).
    pub omarchy_hides: PathBuf,
    /// `$XDG_CONFIG_HOME/trowel/launcher.hides`, written by `app_hide`.
    pub trowel_hides: PathBuf,
    pub locale: Locale,
}

impl AppsEnv {
    pub fn from_env() -> Self {
        let omarchy = std::env::var("OMARCHY_PATH")
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "/usr/share/omarchy".into());
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var_os("HOME").unwrap_or_else(|| "/tmp".into()))
                    .join(".config")
            });
        Self {
            app_dirs: desktop_entry::application_dirs(),
            hide_scan_dirs: desktop_entry::hide_scan_dirs(),
            desktops: desktop_entry::desktop_names(),
            omarchy_hides: PathBuf::from(omarchy).join("default/omarchy/launcher.hides"),
            trowel_hides: config.join("trowel/launcher.hides"),
            locale: Locale::system(),
        }
    }

    /// Paths whose changes mean a rescan.
    pub fn watched(&self) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = self.app_dirs.clone();
        for hides in [&self.omarchy_hides, &self.trowel_hides] {
            if let Some(dir) = hides.parent() {
                out.push(dir.to_path_buf());
            }
        }
        out
    }
}

/// The visible applications, in scan order (the order `AppSearch.sortedEntries` gets them in).
pub struct AppIndex {
    entries: Vec<DesktopEntry>,
    rankables: Vec<Rankable>,
    icons: IconIndex,
    pub scanned_in: Duration,
}

/// Why an entry Quickshell lists is not shown.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct HiddenCounts {
    pub computed: usize,
    pub omarchy: usize,
    pub trowel: usize,
    pub try_exec: usize,
}

impl AppIndex {
    pub fn scan(env: &AppsEnv) -> (Self, HiddenCounts) {
        let started = Instant::now();
        let all = desktop_entry::scan(&env.app_dirs, &env.locale);
        let computed = desktop_entry::computed_hides(&env.hide_scan_dirs, &env.desktops);
        let omarchy = desktop_entry::read_hides(&env.omarchy_hides);
        let trowel = desktop_entry::read_hides(&env.trowel_hides);
        let mut counts = HiddenCounts::default();
        let entries: Vec<DesktopEntry> = all
            .into_iter()
            .filter(|e| {
                // `AppLibrary.isHiddenEntry` (configured, then computed), then Trowel's own.
                if omarchy.contains(&e.id) {
                    counts.omarchy += 1;
                    false
                } else if computed.contains(&e.id) {
                    counts.computed += 1;
                    false
                } else if trowel.contains(&e.id) {
                    counts.trowel += 1;
                    false
                } else if !desktop_entry::try_exec_ok(&e.try_exec) {
                    counts.try_exec += 1;
                    false
                } else {
                    true
                }
            })
            .collect();
        let rankables = entries
            .iter()
            .map(|e| Rankable {
                id: e.id.clone(),
                name: e.name.clone(),
                generic_name: e.generic_name.clone(),
                comment: e.comment.clone(),
                keywords: e.keywords.clone(),
            })
            .collect();
        let index = Self {
            entries,
            rankables,
            icons: IconIndex::default(),
            scanned_in: started.elapsed(),
        };
        (index, counts)
    }

    pub fn set_icons(&mut self, icons: IconIndex) {
        self.icons = icons;
    }

    pub fn icons(&self) -> &IconIndex {
        &self.icons
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entries(&self) -> &[DesktopEntry] {
        &self.entries
    }

    pub fn rankables(&self) -> &[Rankable] {
        &self.rankables
    }

    pub fn get(&self, id: &str) -> Option<&DesktopEntry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// `apps_query`. `launches` maps id → `(count, last, decayed score at last)`.
    pub fn query(
        &self,
        text: &str,
        limit: usize,
        launches: &HashMap<String, Launches>,
        now: i64,
    ) -> Vec<AppRow> {
        let frecency: Vec<f64> = self
            .entries
            .iter()
            .map(|e| {
                launches
                    .get(&e.id)
                    .map_or(0.0, |l| rank::decayed(l.score, l.last, now))
            })
            .collect();
        rank::sorted(&self.rankables, text, &frecency)
            .into_iter()
            .take(limit)
            .map(|r| {
                self.row(
                    &self.entries[r.index],
                    text,
                    launches.get(&self.entries[r.index].id),
                )
            })
            .collect()
    }

    fn row(&self, e: &DesktopEntry, text: &str, launches: Option<&Launches>) -> AppRow {
        let some = |s: &str| (!s.is_empty()).then(|| s.to_string());
        AppRow {
            id: e.id.clone(),
            name: e.name.clone(),
            generic: some(&e.generic_name),
            comment: some(&e.comment),
            icon: self
                .icons
                .resolve(&e.icon)
                .map(|p| PathValue::from_bytes(p.as_os_str().as_encoded_bytes())),
            icon_name: some(&e.icon).filter(|i| !i.starts_with('/')),
            terminal: e.terminal,
            actions: e
                .actions
                .iter()
                .map(|a| AppAction {
                    id: a.id.clone(),
                    name: a.name.clone(),
                })
                .collect(),
            source: PathValue::from_bytes(e.source.as_os_str().as_encoded_bytes()),
            launches: launches.map_or(0, |l| l.count),
            last_launched: launches.map(|l| l.last),
            matched: rank::name_matches(&e.name, text),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Launches {
    pub count: u32,
    pub last: i64,
    /// Σ 0.5^(age/14 d) at `last`.
    pub score: f64,
}

/// The command `app_launch` runs. The app itself goes through `gtk-launch` exactly as
/// `AppLibrary.launch` does (`AppLibrary.qml:85`): inside `uwsm-app` so it lands in
/// `app-graphical.slice` (verified L0.1), keeping the `.desktop` suffix so ids like
/// `org.telegram.desktop` resolve. `gtk-launch` can't run desktop actions, so an action's `Exec`
/// is split like Quickshell does and run under `uwsm-app` (in a terminal when `Terminal=true`).
pub fn launch_argv(
    entry: &DesktopEntry,
    action: Option<&str>,
    uwsm: bool,
) -> Result<Vec<String>, String> {
    let mut argv: Vec<String> = if uwsm {
        vec!["uwsm-app".into(), "--".into()]
    } else {
        Vec::new()
    };
    match action {
        None => {
            argv.push("gtk-launch".into());
            argv.push(format!("{}.desktop", entry.id));
        }
        Some(action) => {
            let found = entry
                .actions
                .iter()
                .find(|a| a.id == action)
                .ok_or_else(|| format!("{} has no action {action}", entry.id))?;
            let exec = desktop_entry::exec_argv(&found.exec);
            if exec.is_empty() {
                return Err(format!("action {action} has no Exec"));
            }
            if entry.terminal {
                argv.push("xdg-terminal-exec".into());
            }
            argv.extend(exec);
        }
    }
    Ok(argv)
}

/// Starts the launch detached (own process group, reaped), never through a shell: ids may hold
/// spaces and quotes (`No Man's Sky`).
pub fn spawn(argv: &[String]) -> io::Result<()> {
    let (program, args) = argv
        .split_first()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "empty command"))?;
    if !actions::which(program) && !Path::new(program).is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("{program} is not installed"),
        ));
    }
    use std::os::unix::process::CommandExt;
    actions::spawn_reaped(
        Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0),
    );
    Ok(())
}

pub fn uwsm_available() -> bool {
    actions::which("uwsm-app")
}

/// `app_hide`: adds or removes the id in Trowel's own `launcher.hides` (Omarchy's file under
/// `$OMARCHY_PATH` belongs to the package and is never written). Atomic replace.
pub fn write_hide(path: &Path, id: &str, hidden: bool) -> io::Result<()> {
    let mut ids: Vec<String> = fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    let normalized = |l: &str| l.strip_suffix(".desktop").unwrap_or(l).to_string();
    ids.retain(|l| normalized(l) != id);
    if hidden {
        ids.push(id.to_string());
    }
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("hides.tmp");
    let mut text = ids.join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    fs::write(&tmp, text)?;
    fs::rename(tmp, path)
}

/// How long a burst of `.desktop` changes (a package install) settles before one rescan.
pub const RESCAN_DEBOUNCE: Duration = Duration::from_millis(300);

/// Watches the application dirs (recursively) and the dirs holding the hide files; calls
/// `on_change` once per burst. Missing paths are skipped.
pub fn watch(
    env: &AppsEnv,
    on_change: impl Fn() + Send + 'static,
) -> notify::Result<notify::RecommendedWatcher> {
    use notify::{RecursiveMode, Watcher};
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if event.is_ok_and(|e| !e.kind.is_access()) {
            let _ = tx.send(());
        }
    })?;
    for dir in &env.app_dirs {
        if dir.is_dir() {
            let _ = watcher.watch(dir, RecursiveMode::Recursive);
        }
    }
    for hides in [&env.omarchy_hides, &env.trowel_hides] {
        if let Some(dir) = hides.parent().filter(|d| d.is_dir()) {
            let _ = watcher.watch(dir, RecursiveMode::NonRecursive);
        }
    }
    std::thread::Builder::new()
        .name("troweld-apps-watch".into())
        .spawn(move || {
            while rx.recv().is_ok() {
                // Settle: keep draining until the burst is quiet.
                while rx.recv_timeout(RESCAN_DEBOUNCE).is_ok() {}
                on_change();
            }
        })?;
    Ok(watcher)
}

/// Ids in a set, for tests and the doctor row.
pub fn ids(index: &AppIndex) -> HashSet<String> {
    index.entries.iter().map(|e| e.id.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQ: AtomicU64 = AtomicU64::new(0);

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn app(name: &str, extra: &str) -> String {
        format!("[Desktop Entry]\nType=Application\nName={name}\nExec=true\n{extra}")
    }

    /// The fixture tree from the L2 plan: webapp, flatpak export, `OnlyShowIn=GNOME`, missing
    /// `TryExec`, actions, localized names, a duplicate id in user and system dirs, plus hides.
    fn fixture() -> (PathBuf, AppsEnv) {
        let root = std::env::temp_dir().join(format!(
            "trowel-apps-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        let user = root.join("home/applications");
        let flatpak = root.join("flatpak/applications");
        let system = root.join("usr/applications");
        write(&system.join("firefox.desktop"), &app("Firefox", "GenericName=Web Browser\nActions=new-private-window;\n\n[Desktop Action new-private-window]\nName=New Private Window\nExec=firefox --private-window %u\n"));
        // User override of a system id wins.
        write(
            &system.join("nvim.desktop"),
            &app("Neovim (system)", "Terminal=true\n"),
        );
        write(
            &user.join("nvim.desktop"),
            &app("Neovim", "Terminal=true\n"),
        );
        write(
            &user.join("HEY.desktop"),
            &app("HEY", "Comment=Webapp\nIcon=/nonexistent/hey.png\n"),
        );
        write(
            &flatpak.join("org.signal.Signal.desktop"),
            &app("Signal", ""),
        );
        write(
            &system.join("gnome-only.desktop"),
            &app("GNOME Thing", "OnlyShowIn=GNOME;\n"),
        );
        write(
            &system.join("ghost.desktop"),
            &app("Ghost", "TryExec=/nonexistent/ghost\n"),
        );
        write(
            &system.join("nodisplay.desktop"),
            &app("Helper", "NoDisplay=true\n"),
        );
        write(&system.join("masked.desktop"), &app("Masked", ""));
        write(
            &user.join("masked.desktop"),
            &app("Masked", "Hidden=true\n"),
        );
        write(
            &system.join("kde/konsole.desktop"),
            &app("Konsole", "Name[de]=Konsole DE\n"),
        );
        write(
            &system.join("omarchy-hidden.desktop"),
            &app("Hidden By Omarchy", ""),
        );
        write(
            &system.join("link.desktop"),
            "[Desktop Entry]\nType=Link\nName=Site\n",
        );
        write(
            &root.join("omarchy/launcher.hides"),
            "omarchy-hidden.desktop\n",
        );
        let env = AppsEnv {
            app_dirs: vec![user.clone(), flatpak.clone(), system.clone()],
            hide_scan_dirs: vec![user, flatpak, system],
            desktops: vec!["Hyprland".into(), "omarchy".into()],
            omarchy_hides: root.join("omarchy/launcher.hides"),
            trowel_hides: root.join("config/trowel/launcher.hides"),
            locale: Locale::parse("de_DE.UTF-8"),
        };
        (root, env)
    }

    #[test]
    fn visible_set_matches_omarchy_rules() {
        let (root, env) = fixture();
        let (index, counts) = AppIndex::scan(&env);
        let mut got: Vec<_> = ids(&index).into_iter().collect();
        got.sort();
        assert_eq!(
            got,
            ["HEY", "firefox", "kde-konsole", "nvim", "org.signal.Signal"]
        );
        assert_eq!(index.get("nvim").unwrap().name, "Neovim", "user dir wins");
        assert_eq!(
            index.get("kde-konsole").unwrap().name,
            "Konsole DE",
            "localized, subdir id"
        );
        assert_eq!(
            counts,
            HiddenCounts {
                computed: 1,
                omarchy: 1,
                trowel: 0,
                try_exec: 1
            }
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn hide_round_trip() {
        let (root, env) = fixture();
        write_hide(&env.trowel_hides, "HEY", true).unwrap();
        write_hide(&env.trowel_hides, "HEY", true).unwrap();
        assert_eq!(fs::read_to_string(&env.trowel_hides).unwrap(), "HEY\n");
        let (index, counts) = AppIndex::scan(&env);
        assert!(index.get("HEY").is_none());
        assert_eq!(counts.trowel, 1);
        write_hide(&env.trowel_hides, "HEY", false).unwrap();
        assert!(AppIndex::scan(&env).0.get("HEY").is_some());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn query_rows_and_frecency() {
        let (root, env) = fixture();
        let (index, _) = AppIndex::scan(&env);
        let none = HashMap::new();
        let rows = index.query("fire", 5, &none, 0);
        assert_eq!(rows[0].id, "firefox");
        assert_eq!(rows[0].matched, vec![[0, 4]]);
        assert_eq!(rows[0].actions[0].id, "new-private-window");
        assert_eq!(rows[0].generic.as_deref(), Some("Web Browser"));
        let hey = &index.query("hey", 1, &none, 0)[0];
        assert!(hey.icon.is_none(), "missing absolute icon is dropped");
        assert!(
            hey.icon_name.is_none(),
            "absolute Icon= is not a theme name"
        );
        // Empty query: frecency first, then A–Z.
        let mut used = HashMap::new();
        used.insert(
            "nvim".to_string(),
            Launches {
                count: 3,
                last: 100,
                score: 3.0,
            },
        );
        let root_rows = index.query("", 10, &used, 100);
        assert_eq!(root_rows[0].id, "nvim");
        assert_eq!(root_rows[0].launches, 3);
        assert_eq!(root_rows[1].id, "firefox");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn launch_commands() {
        let (root, env) = fixture();
        let (index, _) = AppIndex::scan(&env);
        let ff = index.get("firefox").unwrap();
        assert_eq!(
            launch_argv(ff, None, true).unwrap(),
            ["uwsm-app", "--", "gtk-launch", "firefox.desktop"]
        );
        assert_eq!(
            launch_argv(ff, Some("new-private-window"), true).unwrap(),
            ["uwsm-app", "--", "firefox", "--private-window"]
        );
        assert!(launch_argv(ff, Some("nope"), true).is_err());
        let _ = fs::remove_dir_all(root);
    }
}
