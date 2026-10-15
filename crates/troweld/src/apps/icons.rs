//! Icon name → file. The current GTK icon theme's `Inherits=` chain first (Omarchy sets
//! `org.gnome.desktop.interface icon-theme` per theme; PROGRESS §Verified facts), then Omarchy's
//! own context-limited scan (`AppLibrary.qml:122` `iconIndexScanCommand`): `apps`/`devices`
//! icons across the XDG icon dirs, SVG before PNG, then `/usr/share/pixmaps`, first hit wins.
//! Rebuilt when the app list changes, as `AppLibrary` does, rather than watching every icon dir.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Default, Clone)]
pub struct IconIndex {
    map: HashMap<String, PathBuf>,
}

/// `$HOME/.icons`, `$HOME/.local/share/icons`, then each `XDG_DATA_DIRS/icons`.
fn icon_bases() -> Vec<PathBuf> {
    let mut bases = Vec::new();
    if let Ok(home) = std::env::var("HOME") {
        bases.push(PathBuf::from(&home).join(".icons"));
        bases.push(PathBuf::from(&home).join(".local/share/icons"));
    }
    let data_dirs = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    for dir in data_dirs.split(':').filter(|d| !d.is_empty()) {
        bases.push(PathBuf::from(dir).join("icons"));
    }
    bases
}

fn gtk_theme() -> Option<String> {
    let out = Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "icon-theme"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let name = text.trim().trim_matches('\'').trim();
    (!name.is_empty()).then(|| name.to_string())
}

/// The theme, its `Inherits=` (depth first, deduplicated), then `hicolor`.
fn theme_chain(theme: &str, bases: &[PathBuf]) -> Vec<String> {
    fn visit(theme: &str, bases: &[PathBuf], seen: &mut HashSet<String>, out: &mut Vec<String>) {
        if !seen.insert(theme.to_string()) || out.len() > 16 {
            return;
        }
        out.push(theme.to_string());
        let inherits = bases
            .iter()
            .find_map(|b| fs::read_to_string(b.join(theme).join("index.theme")).ok())
            .and_then(|text| {
                text.lines()
                    .find_map(|l| l.strip_prefix("Inherits=").map(str::to_string))
            })
            .unwrap_or_default();
        for parent in inherits.split(',').map(str::trim).filter(|p| !p.is_empty()) {
            visit(parent, bases, seen, out);
        }
    }
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    visit(theme, bases, &mut seen, &mut out);
    if !seen.contains("hicolor") {
        out.push("hicolor".into());
    }
    out
}

fn is_app_context(path: &Path) -> bool {
    path.components()
        .any(|c| matches!(c.as_os_str().to_str(), Some("apps" | "devices")))
}

/// Larger is better; `scalable` (SVG) beats every bitmap. `48x48@2x` counts as 96.
fn quality(path: &Path) -> u32 {
    if path.extension().and_then(|e| e.to_str()) == Some("svg") {
        return u32::MAX;
    }
    path.components()
        .filter_map(|c| c.as_os_str().to_str())
        .find_map(|c| {
            let (size, scale) = match c.split_once('@') {
                Some((s, f)) => (s, f.trim_end_matches('x').parse::<u32>().unwrap_or(1)),
                None => (c, 1),
            };
            let (w, _) = size.split_once('x')?;
            Some(w.parse::<u32>().ok()? * scale)
        })
        .unwrap_or(0)
}

/// Icon contexts that never hold app icons. Skipping them cuts the walk from ~33k files to the
/// few thousand under `apps`/`devices` (the bulk is `mimetypes`, `actions`, `status`).
const SKIP_DIRS: &[&str] = &[
    "actions",
    "animations",
    "categories",
    "cursors",
    "emblems",
    "emotes",
    "filesystems",
    "intl",
    "mimetypes",
    "mimes",
    "panel",
    "places",
    "status",
    "stock",
    "ui",
];

/// Readdir order (what `find` sees), following symlinks, pruning [`SKIP_DIRS`].
fn walk(dir: &Path, depth: u32, visit: &mut dyn FnMut(&Path)) {
    if depth > 8 {
        return;
    }
    let Ok(read) = fs::read_dir(dir) else {
        return;
    };
    for item in read.filter_map(Result::ok) {
        let path = item.path();
        match fs::metadata(&path) {
            Ok(m) if m.is_dir() => {
                let name = item.file_name();
                if !SKIP_DIRS.contains(&name.to_string_lossy().as_ref()) {
                    walk(&path, depth + 1, visit);
                }
            }
            Ok(m) if m.is_file() => visit(&path),
            _ => {}
        }
    }
}

fn stem(path: &Path) -> Option<String> {
    path.file_stem()
        .and_then(|s| s.to_str())
        .map(str::to_string)
}

impl IconIndex {
    pub fn build() -> Self {
        Self::build_with(
            &icon_bases(),
            gtk_theme().as_deref(),
            Path::new("/usr/share/pixmaps"),
        )
    }

    pub fn build_with(bases: &[PathBuf], theme: Option<&str>, pixmaps: &Path) -> Self {
        // One walk per base: every app/device SVG or PNG, in readdir order, with its theme.
        let mut found: Vec<(String, PathBuf)> = Vec::new();
        for base in bases {
            if !base.is_dir() {
                continue;
            }
            walk(base, 0, &mut |path| {
                let ext = path.extension().and_then(|e| e.to_str());
                if matches!(ext, Some("svg" | "png")) && is_app_context(path) {
                    let theme = path
                        .strip_prefix(base)
                        .ok()
                        .and_then(|rel| rel.components().next())
                        .and_then(|c| c.as_os_str().to_str())
                        .unwrap_or("")
                        .to_string();
                    found.push((theme, path.to_path_buf()));
                }
            });
        }
        let mut map: HashMap<String, PathBuf> = HashMap::new();
        // 1. The theme chain: per theme, the best size of each name; earlier themes win.
        if let Some(theme) = theme {
            for name in theme_chain(theme, bases) {
                let mut best: HashMap<String, (u32, &PathBuf)> = HashMap::new();
                for (_, path) in found.iter().filter(|(t, _)| *t == name) {
                    let Some(key) = stem(path) else { continue };
                    let q = quality(path);
                    if best.get(&key).is_none_or(|(have, _)| q > *have) {
                        best.insert(key, (q, path));
                    }
                }
                for (key, (_, path)) in best {
                    map.entry(key).or_insert_with(|| path.clone());
                }
            }
        }
        // 2. Omarchy's order for names no theme has: all SVGs, then pixmaps SVGs, then PNGs.
        for ext in ["svg", "png"] {
            for (_, path) in &found {
                if path.extension().and_then(|e| e.to_str()) == Some(ext) {
                    if let Some(key) = stem(path) {
                        map.entry(key).or_insert_with(|| path.clone());
                    }
                }
            }
            if let Ok(read) = fs::read_dir(pixmaps) {
                for item in read.filter_map(Result::ok) {
                    let path = item.path();
                    if path.extension().and_then(|e| e.to_str()) == Some(ext) {
                        if let Some(key) = stem(&path) {
                            map.entry(key).or_insert(path);
                        }
                    }
                }
            }
        }
        Self { map }
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// `Icon=` value → file. Absolute paths are used as they are (`AppLibrary.qml:61`).
    pub fn resolve(&self, icon: &str) -> Option<PathBuf> {
        if icon.is_empty() {
            return None;
        }
        if icon.starts_with('/') {
            let path = PathBuf::from(icon);
            return path.is_file().then_some(path);
        }
        let name = ["svg", "png", "xpm"]
            .iter()
            .find_map(|ext| icon.strip_suffix(&format!(".{ext}")))
            .unwrap_or(icon);
        self.map.get(name).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"x").unwrap();
    }

    #[test]
    fn theme_chain_then_omarchy_order() {
        let root = std::env::temp_dir().join(format!("trowel-icons-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let base = root.join("icons");
        fs::create_dir_all(base.join("Child")).unwrap();
        fs::write(
            base.join("Child/index.theme"),
            "[Icon Theme]\nInherits=Parent,hicolor\n",
        )
        .unwrap();
        touch(&base.join("Child/48x48/apps/firefox.png"));
        touch(&base.join("Parent/scalable/apps/firefox.svg"));
        touch(&base.join("Parent/256x256/apps/nvim.png"));
        touch(&base.join("Parent/16x16/apps/nvim.png"));
        touch(&base.join("hicolor/scalable/apps/mpv.svg"));
        touch(&base.join("Other/scalable/apps/only-elsewhere.svg"));
        touch(&base.join("Other/scalable/actions/edit.svg"));
        touch(&root.join("pixmaps/legacy.png"));
        let idx = IconIndex::build_with(
            std::slice::from_ref(&base),
            Some("Child"),
            &root.join("pixmaps"),
        );
        // The theme's own icon beats a parent's SVG.
        assert!(idx
            .resolve("firefox")
            .unwrap()
            .ends_with("Child/48x48/apps/firefox.png"));
        // Largest bitmap within a theme.
        assert!(idx
            .resolve("nvim")
            .unwrap()
            .ends_with("256x256/apps/nvim.png"));
        assert!(idx
            .resolve("mpv")
            .unwrap()
            .ends_with("hicolor/scalable/apps/mpv.svg"));
        // Not in the chain: Omarchy's scan still finds it.
        assert!(idx.resolve("only-elsewhere").is_some());
        assert!(idx.resolve("edit").is_none(), "actions are not app icons");
        assert!(idx
            .resolve("legacy")
            .unwrap()
            .ends_with("pixmaps/legacy.png"));
        assert!(
            idx.resolve("mpv.svg").is_some(),
            "extension in Icon= tolerated"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn quality_order() {
        assert!(
            quality(Path::new("x/scalable/apps/a.svg"))
                > quality(Path::new("x/512x512/apps/a.png"))
        );
        assert_eq!(quality(Path::new("x/48x48@2x/apps/a.png")), 96);
    }
}
