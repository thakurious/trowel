//! Desktop Entry files, read exactly the way Omarchy's launcher sees them (D19, L2).
//!
//! Omarchy's app list is Quickshell's `DesktopEntries` (`services/AppLibrary.qml:53`), so the
//! parser and scanner are ports of Quickshell 0.3.1 `src/core/desktopentry.cpp` (`parseText`,
//! `DesktopEntryScanner`, `onScanCompleted`). Omarchy then hides ids computed by
//! `services/hidden-entries.sh`, ported in [`computed_hides`].

use std::collections::{HashMap, HashSet};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// `desktopentry.cpp:33`: `lang_COUNTRY.CODESET@MODIFIER`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Locale {
    language: String,
    territory: String,
    modifier: String,
}

impl Locale {
    pub fn parse(text: &str) -> Self {
        let mut end = text.len();
        let mut locale = Self::default();
        if let Some(at) = text.find('@') {
            locale.modifier = text[at + 1..].to_string();
            end = at;
        }
        if let Some(dot) = text.find('.') {
            end = dot;
        }
        if let Some(us) = text.find('_') {
            if us < end {
                locale.territory = text[us + 1..end].to_string();
                end = us;
            }
        }
        locale.language = text[..end.min(text.len())].to_string();
        locale
    }

    /// `LC_MESSAGES`, else `LANG` (`desktopentry.cpp:80`).
    pub fn system() -> Self {
        let value = std::env::var("LC_MESSAGES")
            .ok()
            .filter(|v| !v.is_empty())
            .or_else(|| std::env::var("LANG").ok())
            .unwrap_or_default();
        Self::parse(&value)
    }

    fn is_valid(&self) -> bool {
        !self.language.is_empty()
    }

    /// `desktopentry.cpp:62`. `self` is the system locale.
    fn match_score(&self, other: &Locale) -> u8 {
        if self.language != other.language {
            return 0;
        }
        if !other.modifier.is_empty() && self.modifier != other.modifier {
            return 0;
        }
        if !other.territory.is_empty() && self.territory != other.territory {
            return 0;
        }
        let mut score = 1;
        if !other.territory.is_empty() {
            score += 2;
        }
        if !other.modifier.is_empty() {
            score += 1;
        }
        score
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopAction {
    pub id: String,
    pub name: String,
    pub exec: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DesktopEntry {
    /// Desktop id without `.desktop`.
    pub id: String,
    pub source: PathBuf,
    pub name: String,
    pub generic_name: String,
    pub comment: String,
    pub icon: String,
    pub exec: String,
    pub try_exec: String,
    pub terminal: bool,
    pub no_display: bool,
    pub hidden: bool,
    pub keywords: Vec<String>,
    pub startup_wm_class: String,
    pub actions: Vec<DesktopAction>,
}

impl DesktopEntry {
    /// `DesktopEntry::isValid`: a name. Non-`Application` types never get one.
    pub fn is_valid(&self) -> bool {
        !self.name.is_empty()
    }
}

/// `DesktopEntry::parseText`. Keys and values are not trimmed, a later unlocalized value replaces
/// an earlier non-matching localized one, and groups other than `[Desktop Entry]` and
/// `[Desktop Action …]` are ignored.
pub fn parse(id: &str, source: &Path, text: &str, system: &Locale) -> DesktopEntry {
    let mut data = DesktopEntry {
        id: id.to_string(),
        source: source.to_path_buf(),
        ..Default::default()
    };
    let mut group = String::new();
    let mut entries: HashMap<String, (Locale, String)> = HashMap::new();
    let mut action_order: Vec<String> = Vec::new();
    let mut pending: HashMap<String, DesktopAction> = HashMap::new();

    let mut finish = |group: &str,
                      entries: &mut HashMap<String, (Locale, String)>,
                      data: &mut DesktopEntry,
                      action_order: &mut Vec<String>| {
        if group == "Desktop Entry" {
            if entries.get("Type").map(|(_, v)| v.as_str()) == Some("Application") {
                for (key, (_, value)) in entries.iter() {
                    let split = |v: &str| {
                        v.split(';')
                            .filter(|s| !s.is_empty())
                            .map(str::to_string)
                            .collect::<Vec<_>>()
                    };
                    match key.as_str() {
                        "Name" => data.name = value.clone(),
                        "GenericName" => data.generic_name = value.clone(),
                        "StartupWMClass" => data.startup_wm_class = value.clone(),
                        "NoDisplay" => data.no_display = value == "true",
                        "Hidden" => data.hidden = value == "true",
                        "Comment" => data.comment = value.clone(),
                        "Icon" => data.icon = value.clone(),
                        "Exec" => data.exec = value.clone(),
                        "TryExec" => data.try_exec = value.clone(),
                        "Terminal" => data.terminal = value == "true",
                        "Keywords" => data.keywords = split(value),
                        "Actions" => *action_order = split(value),
                        _ => {}
                    }
                }
            }
        } else if let Some(action) = group.strip_prefix("Desktop Action ") {
            let get = |k: &str| entries.get(k).map(|(_, v)| v.clone()).unwrap_or_default();
            pending.insert(
                action.to_string(),
                DesktopAction {
                    id: action.to_string(),
                    name: get("Name"),
                    exec: get("Exec"),
                },
            );
        }
        entries.clear();
    };

    for line in text.split('\n').filter(|l| !l.is_empty()) {
        if line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') && line.len() >= 2 {
            finish(&group, &mut entries, &mut data, &mut action_order);
            group = line[1..line.len() - 1].to_string();
            continue;
        }
        let Some(split) = line.find('=') else {
            continue;
        };
        let mut key = &line[..split];
        let value = &line[split + 1..];
        let mut locale = Locale::default();
        if let Some(open) = key.find('[') {
            if open != key.len() - 1 {
                // `key.sliced(localeIdx + 1, length - localeIdx - 2)`: drops the last char (`]`).
                let inner = &key[open + 1..key.len() - 1];
                locale = Locale::parse(inner);
                key = &key[..open];
            }
        }
        match entries.get(key) {
            Some((old, _)) => {
                let old_score = system.match_score(old);
                let new_score = system.match_score(&locale);
                if new_score > old_score || (old_score == 0 && !locale.is_valid()) {
                    entries.insert(key.to_string(), (locale, value.to_string()));
                }
            }
            None => {
                entries.insert(key.to_string(), (locale, value.to_string()));
            }
        }
    }
    finish(&group, &mut entries, &mut data, &mut action_order);
    data.actions = action_order
        .iter()
        .filter_map(|a| pending.get(a).cloned())
        .collect();
    data
}

/// `DesktopEntryManager::desktopPaths`, highest priority first.
pub fn application_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let home = std::env::var("XDG_DATA_HOME")
        .ok()
        .filter(|v| !v.is_empty())
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| format!("{h}/.local/share"))
        });
    if let Some(home) = home {
        dirs.push(PathBuf::from(home).join("applications"));
    }
    let data_dirs = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    for dir in data_dirs.split(':').filter(|d| !d.is_empty()) {
        dirs.push(PathBuf::from(dir).join("applications"));
    }
    dirs
}

/// `DesktopEntryScanner::scanDirectory`: recursive, ids joined with `-`, `completeBaseName`.
/// Follows symlinks like `QFileInfo::isDir/isFile`.
fn scan_directory(dir: &Path, prefix: &str, out: &mut Vec<(String, PathBuf)>, depth: u32) {
    if depth > 16 {
        return;
    }
    let Ok(read) = fs::read_dir(dir) else {
        return;
    };
    let mut items: Vec<_> = read.filter_map(Result::ok).collect();
    // QDir's default sort: name, case-insensitive.
    items.sort_by_key(|e| e.file_name().to_string_lossy().to_lowercase());
    for item in items {
        let path = item.path();
        let name = item.file_name().to_string_lossy().into_owned();
        let Ok(meta) = fs::metadata(&path) else {
            continue;
        };
        if meta.is_dir() {
            let sub = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}-{name}")
            };
            scan_directory(&path, &sub, out, depth + 1);
        } else if meta.is_file() && path.to_string_lossy().ends_with(".desktop") {
            // `completeBaseName`: everything before the last `.`.
            let base = name.rsplit_once('.').map_or(name.as_str(), |(b, _)| b);
            let id = if prefix.is_empty() {
                base.to_string()
            } else {
                format!("{prefix}-{base}")
            };
            out.push((id, path));
        }
    }
}

/// Every valid, displayable entry, as `DesktopEntries.applications` holds it. Lowest-priority
/// dirs are read first so a higher one with the same id replaces it; `Hidden=true` masks the id;
/// an invalid file does not replace a valid one (`onScanCompleted`).
pub fn scan(dirs: &[PathBuf], system: &Locale) -> Vec<DesktopEntry> {
    let mut by_id: HashMap<String, DesktopEntry> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for dir in dirs.iter().rev() {
        if !dir.is_dir() {
            continue;
        }
        let mut files = Vec::new();
        scan_directory(dir, "", &mut files, 0);
        for (id, path) in files {
            let Ok(bytes) = fs::read(&path) else {
                continue;
            };
            let text = String::from_utf8_lossy(&bytes);
            let data = parse(&id, &path, &text, system);
            if data.hidden {
                by_id.remove(&id);
                continue;
            }
            if !data.is_valid() {
                continue;
            }
            if !by_id.contains_key(&id) {
                order.push(id.clone());
            }
            by_id.insert(id, data);
        }
    }
    order
        .into_iter()
        .filter_map(|id| by_id.remove(&id))
        .filter(|e| !e.no_display)
        .collect()
}

/// `hidden-entries.sh` desktop names: `XDG_CURRENT_DESKTOP:XDG_SESSION_DESKTOP:DESKTOP_SESSION`
/// (`AppLibrary.qml:151`).
pub fn desktop_names() -> Vec<String> {
    [
        "XDG_CURRENT_DESKTOP",
        "XDG_SESSION_DESKTOP",
        "DESKTOP_SESSION",
    ]
    .iter()
    .filter_map(|k| std::env::var(k).ok())
    .flat_map(|v| v.split(':').map(str::to_string).collect::<Vec<_>>())
    .filter(|v| !v.is_empty())
    .collect()
}

/// The dirs `hidden-entries.sh` scans, in its order: home, `XDG_DATA_DIRS`, `~/.nix-profile`.
pub fn hide_scan_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(home) = std::env::var("HOME") {
        dirs.push(PathBuf::from(&home).join(".local/share/applications"));
    }
    let data_dirs = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    for dir in data_dirs.split(':') {
        dirs.push(PathBuf::from(dir).join("applications"));
    }
    if let Ok(home) = std::env::var("HOME") {
        dirs.push(PathBuf::from(home).join(".nix-profile/share/applications"));
    }
    dirs
}

/// `find "$dir" -type f -name '*.desktop'`: regular files only, symlinks neither followed nor
/// listed. Sorted like `sort -z` (byte order here; it only decides duplicates within one dir).
fn find_regular(dir: &Path, out: &mut Vec<PathBuf>, depth: u32) {
    if depth > 16 {
        return;
    }
    let Ok(read) = fs::read_dir(dir) else {
        return;
    };
    for item in read.filter_map(Result::ok) {
        let Ok(kind) = item.file_type() else {
            continue;
        };
        let path = item.path();
        if kind.is_dir() {
            find_regular(&path, out, depth + 1);
        } else if kind.is_file() && item.file_name().to_string_lossy().ends_with(".desktop") {
            out.push(path);
        }
    }
}

/// `hidden-entries.sh`: ids whose first-seen file says `Hidden`/`NoDisplay=true`, or whose
/// `OnlyShowIn` names none of `desktops`, or whose `NotShowIn` names one.
pub fn computed_hides(dirs: &[PathBuf], desktops: &[String]) -> HashSet<String> {
    let mut seen = HashSet::new();
    let mut hidden = HashSet::new();
    for dir in dirs {
        if !dir.is_dir() {
            continue;
        }
        let mut files = Vec::new();
        find_regular(dir, &mut files, 0);
        files.sort();
        for file in files {
            let rel = file.strip_prefix(dir).unwrap_or(&file).to_string_lossy();
            let rel = rel.strip_suffix(".desktop").unwrap_or(&rel);
            let id = rel.replace('/', "-");
            if !seen.insert(id.clone()) {
                continue;
            }
            let Ok(bytes) = fs::read(&file) else {
                continue;
            };
            if is_hidden_file(&String::from_utf8_lossy(&bytes), desktops) {
                hidden.insert(id);
            }
        }
    }
    hidden
}

fn is_hidden_file(text: &str, desktops: &[String]) -> bool {
    let mut in_entry = false;
    let mut hidden = false;
    let mut only = String::new();
    let mut not = String::new();
    for line in text.lines() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.starts_with('[') && line.ends_with(']') && line.len() >= 2 {
            in_entry = &line[1..line.len() - 1] == "Desktop Entry";
            continue;
        }
        if !in_entry {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key {
            "Hidden" | "NoDisplay" if value == "true" => hidden = true,
            "OnlyShowIn" => only = value.to_string(),
            "NotShowIn" => not = value.to_string(),
            _ => {}
        }
    }
    let matches = |list: &str| {
        desktops
            .iter()
            .any(|d| list.split(';').any(|e| !e.is_empty() && e == d))
    };
    hidden || (!only.is_empty() && !matches(&only)) || (!not.is_empty() && matches(&not))
}

/// `launcher.hides`: one id per line, `.desktop` optional (`AppLibrary.qml:94`).
pub fn read_hides(path: &Path) -> HashSet<String> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(|l| {
            let l = l.trim();
            l.strip_suffix(".desktop").unwrap_or(l).to_string()
        })
        .filter(|l| !l.is_empty())
        .collect()
}

/// Desktop Entry spec `TryExec`: GIO (and so `gtk-launch`) refuses an entry whose `TryExec` is
/// not an executable file, so Trowel hides it. Quickshell does not check it, so Omarchy's menu
/// lists such an entry and it fails to launch (found in L2: `dev.zed.Zed`).
pub fn try_exec_ok(try_exec: &str) -> bool {
    if try_exec.is_empty() {
        return true;
    }
    let executable = |p: &Path| {
        fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    };
    if try_exec.starts_with('/') {
        return executable(Path::new(try_exec));
    }
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| executable(&dir.join(try_exec))))
}

/// `Exec` → argv for a desktop action (gtk-launch can't run actions). Quoting and escapes as
/// Quickshell's `parseExecString` (`desktopentry.cpp:268`); field codes are dropped, `%%` is `%`.
pub fn exec_argv(exec: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_string = false;
    let mut escape = 0u8;
    let mut percent = false;
    for c in exec.chars() {
        if escape == 0 && c == '\\' {
            escape = 1;
        } else if in_string {
            if c == '\\' {
                escape += 1;
                if escape == 4 {
                    current.push('\\');
                    escape = 0;
                }
            } else if escape == 2 {
                current.push(c);
                escape = 0;
            } else if escape != 0 {
                current.push(match c {
                    's' => ' ',
                    'n' => '\n',
                    't' => '\t',
                    'r' => '\r',
                    other => other,
                });
                escape = 0;
            } else if c == '"' || c == '\'' {
                in_string = false;
            } else {
                current.push(c);
            }
        } else if escape != 0 {
            current.push(c);
            escape = 0;
        } else if percent {
            if c == '%' {
                current.push('%');
            }
            percent = false;
        } else if c == '%' {
            percent = true;
        } else if c == '"' || c == '\'' {
            in_string = true;
        } else if c == ' ' {
            if !current.is_empty() {
                args.push(std::mem::take(&mut current));
            }
        } else {
            current.push(c);
        }
    }
    if !current.is_empty() {
        args.push(current);
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    fn en() -> Locale {
        Locale::parse("en_US.UTF-8")
    }

    #[test]
    fn locale_parsing_and_scores() {
        let de = Locale::parse("de_DE.UTF-8@euro");
        assert_eq!(
            (
                de.language.as_str(),
                de.territory.as_str(),
                de.modifier.as_str()
            ),
            ("de", "DE", "euro")
        );
        let sys = Locale::parse("de_DE.UTF-8");
        assert_eq!(sys.match_score(&Locale::parse("de")), 1);
        assert_eq!(sys.match_score(&Locale::parse("de_DE")), 3);
        assert_eq!(sys.match_score(&Locale::parse("de_AT")), 0);
        assert_eq!(sys.match_score(&Locale::parse("fr")), 0);
    }

    #[test]
    fn localized_names_follow_quickshell() {
        let text = "[Desktop Entry]\nType=Application\nName=Files\nName[de]=Dateien\nName[de_DE]=Dateien (DE)\nName[fr]=Fichiers\nKeywords=folder;manager;\nKeywords[de]=Ordner;\n";
        let p = Path::new("/x.desktop");
        assert_eq!(parse("x", p, text, &en()).name, "Files");
        let de = parse("x", p, text, &Locale::parse("de_DE.UTF-8"));
        assert_eq!(de.name, "Dateien (DE)");
        assert_eq!(de.keywords, vec!["Ordner"]);
        assert_eq!(parse("x", p, text, &Locale::parse("de_AT")).name, "Dateien");
        // A plain key after a non-matching localized one wins (`oldScore == 0 && !valid`).
        let late = "[Desktop Entry]\nType=Application\nName[fr]=Fichiers\nName=Files\n";
        assert_eq!(parse("x", p, late, &en()).name, "Files");
    }

    #[test]
    fn only_applications_are_valid_and_actions_keep_order() {
        let p = Path::new("/x.desktop");
        let link = "[Desktop Entry]\nType=Link\nName=Site\nURL=https://x\n";
        assert!(!parse("x", p, link, &en()).is_valid());
        let text = "[Desktop Entry]\nType=Application\nName=Firefox\nActions=private;new-window;\n\n[Desktop Action new-window]\nName=New Window\nExec=firefox --new-window %u\n\n[Desktop Action private]\nName=New Private Window\nExec=firefox --private-window %u\n\n[Other]\nName=Ignored\n";
        let e = parse("firefox", p, text, &en());
        assert_eq!(e.name, "Firefox");
        let ids: Vec<_> = e.actions.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, ["private", "new-window"]);
        assert_eq!(e.actions[0].name, "New Private Window");
    }

    #[test]
    fn exec_argv_matches_quickshell() {
        assert_eq!(
            exec_argv("firefox --new-window %u"),
            ["firefox", "--new-window"]
        );
        // Quickshell only treats `%` as a field code outside quotes.
        assert_eq!(
            exec_argv(r#"sh -c "echo 100%% done""#),
            ["sh", "-c", "echo 100%% done"]
        );
        assert_eq!(exec_argv("app --rate=50%% %F"), ["app", "--rate=50%"]);
        assert_eq!(
            exec_argv(r#""/opt/My App/run" --x"#),
            ["/opt/My App/run", "--x"]
        );
        assert_eq!(exec_argv(r"a\ b c"), ["a b", "c"]);
    }

    #[test]
    fn hidden_file_rules() {
        let names = vec!["Hyprland".to_string(), "omarchy".to_string()];
        let gnome_only = "[Desktop Entry]\nType=Application\nName=X\nOnlyShowIn=GNOME;\n";
        assert!(is_hidden_file(gnome_only, &names));
        let hypr_only = "[Desktop Entry]\nName=X\nOnlyShowIn=GNOME;Hyprland;\n";
        assert!(!is_hidden_file(hypr_only, &names));
        let not_hypr = "[Desktop Entry]\nName=X\nNotShowIn=Hyprland;\n";
        assert!(is_hidden_file(not_hypr, &names));
        let nodisplay_in_action = "[Desktop Entry]\nName=X\n[Desktop Action a]\nNoDisplay=true\n";
        assert!(!is_hidden_file(nodisplay_in_action, &names));
        assert!(is_hidden_file(
            "[Desktop Entry]\r\nNoDisplay=true\r\n",
            &names
        ));
    }

    #[test]
    fn try_exec() {
        assert!(try_exec_ok(""));
        assert!(try_exec_ok("/bin/sh"));
        assert!(try_exec_ok("sh"));
        assert!(!try_exec_ok("/nonexistent/zed"));
        assert!(!try_exec_ok("definitely-not-a-binary-trowel"));
    }
}
