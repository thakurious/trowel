//! What gets indexed. `upstream/Sources/DigUpKit/Rules.swift` plus Linux screenshot names (PORT-MAP).

use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

use crate::model::{FileKind, IndexOptions};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Classification {
    Index(FileKind),
    Skip(String),
    Ignore,
}

pub struct Rules;

impl Rules {
    pub const SKIPPED_FOLDER_NAMES: &'static [&'static str] = &[
        "node_modules",
        "bower_components",
        "venv",
        "site-packages",
        "__pycache__",
        "DerivedData",
        "Pods",
        "Carthage",
        "target",
    ];

    pub const PROJECT_MARKERS: &'static [&'static str] = &[
        ".git",
        "package.json",
        "Package.swift",
        "pyproject.toml",
        "setup.py",
        "requirements.txt",
        "Cargo.toml",
        "go.mod",
        "pom.xml",
        "build.gradle",
        "build.gradle.kts",
        "Gemfile",
        "composer.json",
        "CMakeLists.txt",
    ];

    pub const PROJECT_BUNDLE_EXTENSIONS: &'static [&'static str] = &["xcodeproj", "xcworkspace"];

    pub const DOC_EXTENSIONS: &'static [&'static str] = &[
        "txt", "text", "md", "markdown", "rtf", "docx", "doc", "odt", "html", "htm",
    ];

    pub const CODE_EXTENSIONS: &'static [&'static str] = &[
        "py", "ipynb", "c", "h", "cc", "cpp", "cxx", "hpp", "hh", "m", "mm", "swift", "tsx", "cts",
        "js", "jsx", "mjs", "cjs", "go", "rs", "java", "kt", "kts", "scala", "rb", "php", "cs",
        "fs", "sh", "bash", "zsh", "fish", "ps1", "lua", "pl", "r", "jl", "dart", "sql", "vue",
        "svelte", "css", "scss", "sass", "less", "json", "jsonl", "yaml", "yml", "toml", "xml",
        "plist", "gradle", "cmake", "mk", "lock", "log", "csv", "tsv", "ini", "cfg", "conf",
        "proto", "graphql", "tf", "nix", "zig", "hs", "ex", "exs", "erl", "clj", "elm", "ml",
        "vim", "el", "asm", "metal", "glsl", "hlsl", "wgsl", "cu", "pyi", "pyc",
    ];

    pub const SOURCE_EXTENSIONS: &'static [&'static str] = &[
        "py", "pyi", "pyx", "c", "h", "cc", "cpp", "cxx", "hpp", "hh", "hxx", "m", "mm", "swift",
        "ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs", "go", "rs", "java", "kt", "kts",
        "scala", "groovy", "rb", "php", "cs", "fs", "sh", "bash", "zsh", "fish", "ps1", "lua",
        "pl", "pm", "r", "jl", "dart", "sql", "vue", "svelte", "astro", "html", "htm", "css",
        "scss", "sass", "less", "gradle", "cmake", "mk", "proto", "graphql", "gql", "tf", "hcl",
        "nix", "zig", "hs", "ex", "exs", "erl", "clj", "cljs", "elm", "ml", "mli", "vim", "el",
        "lisp", "asm", "s", "metal", "glsl", "hlsl", "wgsl", "cu", "cuh", "ino", "v", "sv", "tex",
    ];

    pub const CODE_TEXT_EXTENSIONS: &'static [&'static str] = &[
        "md", "markdown", "rst", "adoc", "org", "txt", "yaml", "yml", "toml",
    ];

    pub const CODE_FILE_NAMES: &'static [&'static str] = &[
        "Makefile",
        "makefile",
        "GNUmakefile",
        "Dockerfile",
        "Containerfile",
        "Justfile",
        "justfile",
        "Rakefile",
        "Gemfile",
        "Podfile",
        "Brewfile",
        "Procfile",
        "Vagrantfile",
        "Jenkinsfile",
    ];

    pub const CODE_SKIPPED_FOLDER_NAMES: &'static [&'static str] = &[
        "vendor",
        "Vendor",
        "vendored",
        "third_party",
        "third-party",
        "thirdparty",
        "3rdparty",
        "external",
        "extern",
        "deps",
        "build",
        "Build",
        "dist",
        "out",
        "target",
        "obj",
        "coverage",
        "htmlcov",
        "generated",
        "__generated__",
        "wandb",
        "mlruns",
        "lightning_logs",
    ];

    const IMAGE_EXTENSIONS: &'static [&'static str] = &[
        "jpg", "jpeg", "png", "gif", "webp", "tif", "tiff", "bmp", "heic", "heif", "avif", "jxl",
        "cr2", "nef", "arw", "dng", "orf", "rw2", "raf", "sr2",
    ];
    const AUDIO_EXTENSIONS: &'static [&'static str] = &[
        "mp3", "m4a", "aac", "wav", "flac", "ogg", "opus", "aiff", "aif", "wma",
    ];
    const VIDEO_EXTENSIONS: &'static [&'static str] = &[
        "mp4", "mov", "mkv", "webm", "avi", "m4v", "mpeg", "mpg", "m2ts",
    ];

    const SCREENSHOT_PREFIXES: &'static [&'static str] =
        &["Screenshot", "Screen Shot", "CleanShot", "SCR-"];

    const CAMERA_PREFIXES: &'static [&'static str] = &[
        "img",
        "dsc",
        "pxl",
        "mvimg",
        "vid",
        "gopr",
        "dji",
        "photo",
        "whatsapp",
        "screenshot",
    ];

    pub fn classify_code(path: &Path, options: &IndexOptions) -> Classification {
        let name = file_name(path);
        let ext = extension(path);
        if options.excluded_extensions.contains(&ext) {
            return Classification::Skip("excluded type".into());
        }
        if Self::is_secret(&name) {
            return Classification::Skip("key or password file".into());
        }
        let lower = name.to_lowercase();
        if lower.ends_with(".min.js") || lower.ends_with(".min.mjs") || lower.ends_with(".min.css")
        {
            return Classification::Skip("minified".into());
        }
        if ["license", "licence", "copying", "notice"]
            .iter()
            .any(|p| lower.starts_with(p))
        {
            return Classification::Skip("license".into());
        }
        if (ext == "ts" || ext == "mts") && Self::is_mpeg_transport_stream(path) {
            return Classification::Ignore;
        }
        if contains(Self::SOURCE_EXTENSIONS, &ext)
            || contains(Self::CODE_TEXT_EXTENSIONS, &ext)
            || ext == "ipynb"
            || contains(Self::CODE_FILE_NAMES, &name)
            || name.starts_with("Dockerfile")
        {
            return Classification::Index(FileKind::Code);
        }
        if contains(Self::CODE_EXTENSIONS, &ext) {
            return Classification::Skip("data file".into());
        }
        Classification::Ignore
    }

    pub fn is_secret(name: &str) -> bool {
        let lower = name.to_lowercase();
        let ext = Path::new(&lower)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        const SECRET_TYPES: &[&str] = &[
            "pem",
            "key",
            "p12",
            "pfx",
            "keystore",
            "jks",
            "ppk",
            "kdbx",
            "gpg",
            "asc",
            "crt",
            "cer",
            "der",
            "p8",
            "mobileprovision",
        ];
        if SECRET_TYPES.contains(&ext) {
            return true;
        }
        [
            "id_rsa",
            "id_dsa",
            "id_ecdsa",
            "id_ed25519",
            ".env",
            "secret",
            "credential",
            ".netrc",
            ".npmrc",
            ".pypirc",
            "htpasswd",
            ".htpasswd",
        ]
        .iter()
        .any(|p| lower.starts_with(p))
    }

    pub fn code_skip_reason(folder: &Path) -> Option<&'static str> {
        let name = file_name(folder);
        if contains(Self::CODE_SKIPPED_FOLDER_NAMES, &name) {
            return Some("vendored or built code");
        }
        let git = folder.join(".git");
        if git.is_file() {
            return Some("submodule or worktree");
        }
        None
    }

    pub fn classify(path: &Path, options: &IndexOptions) -> Classification {
        let ext = extension(path);
        if options.excluded_extensions.contains(&ext) {
            return Classification::Skip("excluded type".into());
        }
        if ext == "ts" || ext == "mts" || ext == "m2ts" {
            if Self::is_mpeg_transport_stream(path) {
                return Classification::Index(FileKind::Video);
            }
            return if options.include_code {
                Classification::Index(FileKind::Doc)
            } else {
                Classification::Skip("code or data file".into())
            };
        }
        if contains(Self::DOC_EXTENSIONS, &ext) {
            return Classification::Index(FileKind::Doc);
        }
        if contains(Self::CODE_EXTENSIONS, &ext) {
            return if options.include_code {
                Classification::Index(FileKind::Doc)
            } else {
                Classification::Skip("code or data file".into())
            };
        }
        if ext == "pdf" {
            return Classification::Index(FileKind::Pdf);
        }
        if contains(Self::IMAGE_EXTENSIONS, &ext) {
            return Classification::Index(if Self::is_screenshot(path) {
                FileKind::Screenshot
            } else {
                FileKind::Image
            });
        }
        if contains(Self::VIDEO_EXTENSIONS, &ext) {
            return Classification::Index(FileKind::Video);
        }
        if contains(Self::AUDIO_EXTENSIONS, &ext) {
            return Classification::Index(FileKind::Audio);
        }
        Classification::Ignore
    }

    /// MPEG-TS packets start with 0x47 every 188 bytes (or every 192 with a 4-byte timecode).
    /// `upstream/Sources/DigUpKit/Rules.swift:129`
    pub fn is_mpeg_transport_stream(path: &Path) -> bool {
        let Ok(mut file) = File::open(path) else {
            return false;
        };
        let mut data = [0u8; 600];
        let Ok(n) = file.read(&mut data) else {
            return false;
        };
        if n < 400 {
            return false;
        }
        (data[0] == 0x47 && data[188] == 0x47 && data[376] == 0x47)
            || (data[4] == 0x47 && data[196] == 0x47 && data[388] == 0x47)
    }

    /// Filename prefixes: Apple, Omarchy `screenshot-YYYY-MM-DD_HH-MM-SS`, grim/hyprshot/flameshot/GNOME/KDE.
    /// `upstream/Sources/DigUpKit/Rules.swift:139` and PORT-MAP Linux deltas.
    pub fn is_screenshot(path: &Path) -> bool {
        let name = file_name(path);
        if Self::SCREENSHOT_PREFIXES
            .iter()
            .any(|p| name.starts_with(p))
        {
            return true;
        }
        let lower = name.to_lowercase();
        if looks_like_omarchy_screenshot(&lower)
            || lower.starts_with("grim-")
            || lower.starts_with("grim_")
            || lower.starts_with("hyprshot")
            || lower.starts_with("flameshot")
            || name.starts_with("Screenshot from ")
            || name.starts_with("Screenshot_")
        {
            return true;
        }
        // Any image saved into a Screenshots folder: omasnap's and GNOME's default is
        // ~/Pictures/Screenshots, whatever the file is called. A region capture is smaller than the
        // monitor, so the size check below misses it.
        if path
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|d| d.to_string_lossy().eq_ignore_ascii_case("screenshots"))
        {
            return true;
        }
        // Weak signal: a PNG whose pixel size equals a connected monitor's mode.
        if let Some((w, h)) = png_ihdr_size(path) {
            if connected_monitor_modes()
                .iter()
                .any(|(mw, mh)| *mw == w && *mh == h)
            {
                return true;
            }
        }
        false
    }

    /// Test helper: PNG IHDR size vs injected monitor modes (no hyprctl).
    pub fn png_matches_monitor(path: &Path, modes: &[(u32, u32)]) -> bool {
        png_ihdr_size(path).is_some_and(|(w, h)| modes.iter().any(|(mw, mh)| *mw == w && *mh == h))
    }

    pub fn skip_reason_for_folder(
        folder: &Path,
        home: &Path,
        excluded: &[String],
    ) -> Option<String> {
        let name = file_name(folder);
        if contains(Self::SKIPPED_FOLDER_NAMES, &name) {
            return Some("developer folder".into());
        }
        if name.ends_with(".noindex") {
            return Some(".noindex folder".into());
        }
        if folder == home.join("Library") {
            return Some("~/Library".into());
        }
        // PORT-MAP: ~/Library → skip ~/.cache, Trash, .var, flatpak/snap, Steam.
        let linux_skips = [
            home.join(".cache"),
            home.join(".local/share/Trash"),
            home.join(".var"),
            home.join(".local/share/flatpak"),
            home.join("snap"),
            home.join(".local/share/Steam"),
        ];
        if linux_skips.iter().any(|p| folder == p) {
            return Some("system folder".into());
        }
        let folder_str = folder.to_string_lossy();
        if excluded
            .iter()
            .any(|ex| folder_str == *ex || folder_str.starts_with(&format!("{ex}/")))
        {
            return Some("excluded folder".into());
        }
        None
    }

    /// Whether a change the folder watcher reports at `path` is where the crawl of `root` goes: not inside a
    /// folder it skips (hidden, `.noindex` and developer folders, the Linux system folders, the skipped ones).
    /// A change anywhere else asks for a sync that finds nothing new.
    /// `upstream/Sources/DigUpKit/Rules.swift` `isWalked` (75d95d0).
    pub fn is_walked(path: &Path, root: &Path, home: &Path, excluded: &[String]) -> bool {
        Self::walks_to(path, root, |folder| {
            Self::skip_reason_for_folder(folder, home, excluded).is_none()
        })
    }

    /// `is_walked` for a code root: the code crawl also skips vendored and built folders and submodules
    /// (`code_skip_reason`). Linux-only: inotify reports every write under `target/` or `node_modules/`.
    pub fn is_code_walked(path: &Path, root: &Path, home: &Path, excluded: &[String]) -> bool {
        Self::walks_to(path, root, |folder| {
            Self::skip_reason_for_folder(folder, home, excluded).is_none()
                && Self::code_skip_reason(folder).is_none()
        })
    }

    /// Whether the crawl enters `folder` once it reached its parent (one step of `is_walked`).
    pub fn walks_into(folder: &Path, home: &Path, excluded: &[String], code: bool) -> bool {
        !file_name(folder).starts_with('.')
            && Self::skip_reason_for_folder(folder, home, excluded).is_none()
            && !(code && Self::code_skip_reason(folder).is_some())
    }

    /// Whether every folder from `root` down to `path` is one the crawl walks: not hidden, and `walks`.
    pub fn walks_to(path: &Path, root: &Path, walks: impl Fn(&Path) -> bool) -> bool {
        let Ok(rest) = path.strip_prefix(root) else {
            return false;
        };
        let mut folder = root.to_path_buf();
        for part in rest.components() {
            folder.push(part);
            if part.as_os_str().to_string_lossy().starts_with('.') || !walks(&folder) {
                return false;
            }
        }
        true
    }

    pub fn is_project_root(folder: &Path) -> bool {
        let Ok(entries) = fs::read_dir(folder) else {
            return false;
        };
        let mut names = HashSet::new();
        for entry in entries.flatten() {
            names.insert(entry.file_name().to_string_lossy().into_owned());
        }
        if Self::PROJECT_MARKERS.iter().any(|m| names.contains(*m)) {
            return true;
        }
        names.iter().any(|n| {
            Path::new(n)
                .extension()
                .and_then(|s| s.to_str())
                .is_some_and(|ext| contains(Self::PROJECT_BUNDLE_EXTENSIONS, ext))
        })
    }

    /// `upstream/Sources/DigUpKit/Rules.swift:165`
    pub fn looks_machine_named(stem: &str) -> bool {
        let name = stem.to_lowercase();
        if Self::CAMERA_PREFIXES.iter().any(|p| name.starts_with(p)) {
            return false;
        }
        if is_hex_uuidish(&name) {
            return true;
        }
        is_short_alpha_then_digits(&name)
    }

    /// `upstream/Sources/DigUpKit/Rules.swift:174`
    pub fn dataset_folders(
        files: &[(String, String, FileKind)],
        roots: &HashSet<String>,
        minimum: usize,
    ) -> HashMap<String, usize> {
        let mut groups: HashMap<String, Vec<String>> = HashMap::new();
        for (folder, stem, kind) in files {
            if !matches!(kind, FileKind::Image | FileKind::Audio | FileKind::Video) {
                continue;
            }
            if roots.contains(folder) {
                continue;
            }
            groups
                .entry(format!("{folder}\u{0}{}", kind.as_str()))
                .or_default()
                .push(stem.clone());
        }
        let mut datasets: HashMap<String, usize> = HashMap::new();
        for (key, stems) in groups {
            if stems.len() < minimum {
                continue;
            }
            let machine = stems
                .iter()
                .filter(|s| Self::looks_machine_named(s))
                .count();
            if (machine as f64) / (stems.len() as f64) >= 0.7 {
                let folder = key.split('\u{0}').next().unwrap_or("").to_string();
                *datasets.entry(folder).or_default() += stems.len();
            }
        }
        datasets
    }
}

fn contains(set: &[&str], item: &str) -> bool {
    set.contains(&item)
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase()
}

fn png_ihdr_size(path: &Path) -> Option<(u32, u32)> {
    let mut file = File::open(path).ok()?;
    let mut buf = [0u8; 24];
    file.read_exact(&mut buf).ok()?;
    if buf[0..8] != [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A] {
        return None;
    }
    if &buf[12..16] != b"IHDR" {
        return None;
    }
    let w = u32::from_be_bytes(buf[16..20].try_into().ok()?);
    let h = u32::from_be_bytes(buf[20..24].try_into().ok()?);
    (w > 0 && h > 0).then_some((w, h))
}

fn connected_monitor_modes() -> &'static [(u32, u32)] {
    static MODES: OnceLock<Vec<(u32, u32)>> = OnceLock::new();
    MODES.get_or_init(|| {
        let Ok(out) = Command::new("hyprctl").args(["monitors", "-j"]).output() else {
            return Vec::new();
        };
        if !out.status.success() {
            return Vec::new();
        }
        let Ok(json) = serde_json::from_slice::<serde_json::Value>(&out.stdout) else {
            return Vec::new();
        };
        json.as_array()
            .into_iter()
            .flatten()
            .filter_map(|m| {
                let w = m.get("width")?.as_u64()? as u32;
                let h = m.get("height")?.as_u64()? as u32;
                (w > 0 && h > 0).then_some((w, h))
            })
            .collect()
    })
}

fn looks_like_omarchy_screenshot(lower: &str) -> bool {
    // screenshot-YYYY-MM-DD_HH-MM-SS.png  (`omarchy-capture-screenshot`), optionally followed by
    // omasnap's `-{app}` slug and/or a `-2` collision suffix:
    // screenshot-2026-10-26_14-35-58-ghostty.png, screenshot-2026-10-26_14-35-58-2.png.
    let rest = match lower.strip_prefix("screenshot-") {
        Some(r) => r,
        None => return false,
    };
    let stem = rest.rsplit_once('.').map(|(s, _)| s).unwrap_or(rest);
    let mut chars = stem.chars();
    for expected in "0000-00-00_00-00-00".chars() {
        match (expected, chars.next()) {
            ('0', Some(c)) if c.is_ascii_digit() => {}
            ('-' | '_', Some(c)) if c == expected => {}
            _ => return false,
        }
    }
    let tail: String = chars.collect();
    tail.is_empty()
        || tail.strip_prefix('-').is_some_and(|t| {
            !t.is_empty()
                && t.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
        })
}

fn is_hex_uuidish(name: &str) -> bool {
    // ^[0-9a-f]{8,}([-_][0-9a-f]{4,})*$
    let mut parts = name.split(['-', '_']);
    let Some(first) = parts.next() else {
        return false;
    };
    if first.len() < 8 || !first.chars().all(|c| c.is_ascii_hexdigit()) {
        return false;
    }
    parts.all(|p| p.len() >= 4 && p.chars().all(|c| c.is_ascii_hexdigit()))
}

fn is_short_alpha_then_digits(name: &str) -> bool {
    // ^[a-z]{0,16}[-_. ]?\d{3,}([-_.]\d+)*$
    let bytes = name.as_bytes();
    let mut i = 0;
    while i < bytes.len() && i < 16 && bytes[i].is_ascii_lowercase() {
        i += 1;
    }
    if i < bytes.len() && matches!(bytes[i], b'-' | b'_' | b'.' | b' ') {
        i += 1;
    }
    let start_digits = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i - start_digits < 3 {
        return false;
    }
    while i < bytes.len() {
        if !matches!(bytes[i], b'-' | b'_' | b'.') {
            return false;
        }
        i += 1;
        let d0 = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == d0 {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    /// `upstream/Tests/DigUpKitTests/RulesTests.swift` `changesCountOnlyWhereTheCrawlGoes`, with the Linux
    /// system folders in place of ~/Library.
    #[test]
    fn changes_count_only_where_the_crawl_goes() {
        let home = Path::new("/home/me");
        let walked = |path: &str, root: &str, excluded: &[&str]| {
            let excluded: Vec<String> = excluded.iter().map(|s| s.to_string()).collect();
            Rules::is_walked(Path::new(path), Path::new(root), home, &excluded)
        };
        assert!(walked("/home/me/Documents/trip.pdf", "/home/me", &[]));
        assert!(walked("/home/me", "/home/me", &[]));
        // Trowel's own index and the caches, with the home folder chosen.
        assert!(!walked(
            "/home/me/.local/share/trowel/index/index.sqlite-wal",
            "/home/me",
            &[]
        ));
        assert!(!walked(
            "/home/me/.cache/mozilla/firefox/cache2/x",
            "/home/me",
            &[]
        ));
        assert!(!walked("/home/me/snap/firefox/x", "/home/me", &[]));
        assert!(!walked(
            "/home/me/Projects/site/node_modules/left-pad/index.js",
            "/home/me",
            &[]
        ));
        assert!(!walked(
            "/home/me/Videos/Exports.noindex/clip.mov",
            "/home/me",
            &[]
        ));
        assert!(!walked("/home/me/Documents/.git/index", "/home/me", &[]));
        assert!(!walked(
            "/home/me/Downloads/big.iso",
            "/home/me",
            &["/home/me/Downloads"]
        ));
        // A chosen folder that is itself hidden is walked; only hidden folders below it are not.
        let notes = "/home/me/.local/share/notes";
        assert!(walked("/home/me/.local/share/notes/plan.md", notes, &[]));
        assert!(!walked("/home/me/Documents/trip.pdf", notes, &[]));
        assert!(walked(
            "/home/me/Documentsx/a.pdf",
            "/home/me/Documentsx",
            &[]
        ));
        assert!(!walked(
            "/home/me/Documentsx/a.pdf",
            "/home/me/Documents",
            &[]
        ));
    }

    #[test]
    fn code_changes_skip_built_and_vendored_folders() {
        let home = Path::new("/home/me");
        let code = |path: &str| {
            Rules::is_code_walked(Path::new(path), Path::new("/home/me/Projects"), home, &[])
        };
        assert!(code("/home/me/Projects/app/src/main.rs"));
        assert!(!code("/home/me/Projects/app/target/debug/build/x.o"));
        assert!(!code("/home/me/Projects/web/node_modules/react/index.js"));
        assert!(!code("/home/me/Projects/web/build/out.js"));
        assert!(!code("/home/me/Projects/app/.git/index.lock"));
        assert!(!code("/home/me/Projects/app/vendor/lib.c"));
        assert!(Rules::walks_into(
            Path::new("/home/me/Projects/app/src"),
            home,
            &[],
            true
        ));
        assert!(!Rules::walks_into(
            Path::new("/home/me/Projects/app/build"),
            home,
            &[],
            true
        ));
        assert!(Rules::walks_into(
            Path::new("/home/me/Projects/app/build"),
            home,
            &[],
            false
        ));
    }

    fn tmp() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("trowel-rules-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    #[test]
    fn type_script_is_not_video() {
        let file = tmp().join("app.ts");
        fs::write(&file, "export const x: number = 1;\n").unwrap();
        assert_eq!(
            Rules::classify(&file, &IndexOptions::default()),
            Classification::Skip("code or data file".into())
        );
    }

    #[test]
    fn real_transport_stream_is_video() {
        let mut bytes = vec![0xFFu8; 188 * 4];
        for packet in 0..4 {
            bytes[packet * 188] = 0x47;
        }
        let file = tmp().join("clip.ts");
        File::create(&file).unwrap().write_all(&bytes).unwrap();
        assert_eq!(
            Rules::classify(&file, &IndexOptions::default()),
            Classification::Index(FileKind::Video)
        );
    }

    #[test]
    fn code_can_be_included() {
        let options = IndexOptions {
            include_code: true,
            ..IndexOptions::default()
        };
        let file = tmp().join("main.py");
        assert_eq!(
            Rules::classify(&file, &options),
            Classification::Index(FileKind::Doc)
        );
    }

    #[test]
    fn machine_names() {
        assert!(Rules::looks_machine_named("clip_000123"));
        assert!(Rules::looks_machine_named("frame-0042"));
        assert!(Rules::looks_machine_named("3f9a1c0e-12ab-4cde"));
        assert!(Rules::looks_machine_named("000123"));
        assert!(!Rules::looks_machine_named("IMG_1234"));
        assert!(!Rules::looks_machine_named("PXL_20260101_123456789"));
        assert!(!Rules::looks_machine_named(
            "Screenshot 2026-10-01 at 9.41.07 PM"
        ));
        assert!(!Rules::looks_machine_named("Tokyo itinerary"));
    }

    #[test]
    fn dataset_needs_many_machine_named_files() {
        let clips: Vec<_> = (0..400)
            .map(|i| ("/x/clips".into(), format!("clip_{i:06}"), FileKind::Audio))
            .collect();
        let photos: Vec<_> = (0..400)
            .map(|i| {
                (
                    "/x/camera".into(),
                    format!("IMG_{}", 1000 + i),
                    FileKind::Image,
                )
            })
            .collect();
        let few: Vec<_> = (0..50)
            .map(|i| ("/x/few".into(), format!("clip_{i}"), FileKind::Audio))
            .collect();
        let mut all = clips;
        all.extend(photos);
        all.extend(few);
        let datasets = Rules::dataset_folders(&all, &HashSet::new(), 300);
        assert_eq!(datasets.get("/x/clips").copied(), Some(400));
        assert!(!datasets.contains_key("/x/camera"));
    }

    #[test]
    fn omarchy_and_apple_screenshot_names() {
        assert!(Rules::is_screenshot(Path::new(
            "/tmp/screenshot-2026-10-12_15-01-02.png"
        )));
        assert!(Rules::is_screenshot(Path::new(
            "/tmp/Screenshot 2026-10-02 at 8.15.40 AM.png"
        )));
        assert!(Rules::is_screenshot(Path::new("/tmp/grim-2026.png")));
        assert!(!Rules::is_screenshot(Path::new("/tmp/holiday.png")));
    }

    #[test]
    fn omasnap_names_and_screenshots_folders() {
        // omasnap's default `screenshot-{date}_{time}-{app}`, and a name collision.
        assert!(Rules::is_screenshot(Path::new(
            "/p/screenshot-2026-10-26_14-35-58-ghostty.png"
        )));
        assert!(Rules::is_screenshot(Path::new(
            "/p/screenshot-2026-10-26_14-35-58-google-chrome-2.png"
        )));
        assert!(!Rules::is_screenshot(Path::new(
            "/p/screenshot-2026-10-26_14-35-58-.png"
        )));
        assert!(!Rules::is_screenshot(Path::new("/p/screenshot-ideas.png")));
        // A region capture of any size in a Screenshots folder.
        assert!(Rules::is_screenshot(Path::new(
            "/home/u/Pictures/Screenshots/receipt.png"
        )));
        assert!(Rules::is_screenshot(Path::new("/home/u/screenshots/a.jpg")));
        assert!(!Rules::is_screenshot(Path::new(
            "/home/u/Pictures/Screenshots/old/a.png"
        )));
    }

    #[test]
    fn png_monitor_size_is_a_weak_screenshot_signal() {
        let file = tmp().join("desktop.png");
        let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        png.extend_from_slice(&13u32.to_be_bytes());
        png.extend_from_slice(b"IHDR");
        png.extend_from_slice(&1920u32.to_be_bytes());
        png.extend_from_slice(&1200u32.to_be_bytes());
        File::create(&file).unwrap().write_all(&png).unwrap();
        assert!(Rules::png_matches_monitor(&file, &[(1920, 1200)]));
        assert!(!Rules::png_matches_monitor(&file, &[(1280, 720)]));
        assert!(!Rules::is_screenshot(Path::new("/tmp/holiday.png")));
    }
}
