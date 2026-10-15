//! `open` / `reveal` / clipboard copy. PROTOCOL.md §4 Actions.

use std::ffi::OsString;
use std::fs::File;
use std::io::{self, Write};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use trowel_core::{FileKind, Rules};

pub fn open(path: &Path, kind: FileKind, loc: Option<f64>, editor: &str) {
    let argv = open_argv(path, kind, loc, editor);
    if argv.is_empty() {
        return;
    }
    let mut cmd = Command::new(&argv[0]);
    cmd.args(&argv[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0);
    spawn_reaped(&mut cmd);
}

/// Start `cmd` and wait for it on a thread, so a launcher that exits (`xdg-open`, a terminal
/// that forks) doesn't stay a zombie of troweld for the life of the daemon.
pub(crate) fn spawn_reaped(cmd: &mut Command) {
    if let Ok(mut child) = cmd.spawn() {
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
}

/// Puts plain text on the clipboard (D21). `wl-copy` forks a server that keeps the selection, so
/// the parent exits once the text is handed over.
pub fn copy_text(text: &str) -> io::Result<()> {
    if !which("wl-copy") {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "wl-copy is not installed",
        ));
    }
    let mut child = Command::new("wl-copy")
        .args(["--type", "text/plain;charset=utf-8"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(text.as_bytes())?;
    }
    let status = child.wait()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("wl-copy exited with {status}")))
    }
}

/// Omarchy's paste: `wtype -M shift -k Insert -m shift` after the panel has unmapped
/// (`LAUNCHER-PLAN.md` §0). The 150 ms is Omarchy's; L0.4 has not measured it on this machine yet.
pub const PASTE_DELAY: Duration = Duration::from_millis(150);

/// Sends the paste keystroke to whatever window has focus once the delay has passed.
pub fn paste_keystroke() {
    if !which("wtype") {
        return;
    }
    std::thread::spawn(|| {
        std::thread::sleep(PASTE_DELAY);
        spawn_reaped(
            Command::new("wtype")
                .args(["-M", "shift", "-k", "Insert", "-m", "shift"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null()),
        );
    });
}

/// `file://` URI with the path percent-encoded (RFC 8089/3986). A raw path breaks on spaces,
/// `#`, `%` or an apostrophe (which also ended the GVariant string `ShowItems` gets).
pub fn file_uri(path: &Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    let mut uri = String::from("file://");
    for &b in path.as_os_str().as_bytes() {
        if b.is_ascii_alphanumeric() || b"/-._~".contains(&b) {
            uri.push(b as char);
        } else {
            uri.push_str(&format!("%{b:02X}"));
        }
    }
    uri
}

pub fn reveal(path: &Path) {
    let uri = file_uri(path);
    if Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest",
            "org.freedesktop.FileManager1",
            "--object-path",
            "/org/freedesktop/FileManager1",
            "--method",
            "org.freedesktop.FileManager1.ShowItems",
            &format!("['{uri}']"),
            "",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        return;
    }
    let parent = path.parent().unwrap_or(path);
    spawn_reaped(
        Command::new("xdg-open")
            .arg(parent)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0),
    );
}

/// Put the file on the clipboard so paste works in a file manager or chat.
/// Pictures go as image bytes; everything else as a `file://` URI.
pub fn copy(path: &Path) {
    if !which("wl-copy") {
        return;
    }
    let abs = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let ext = abs
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match copy_mime(&ext) {
        Some(mime) => {
            let Ok(file) = File::open(&abs) else {
                return;
            };
            spawn_reaped(
                Command::new("wl-copy")
                    .args(["--type", mime])
                    .stdin(file)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null()),
            );
        }
        None => {
            let uri = format!("{}\r\n", file_uri(&abs));
            let Ok(mut child) = Command::new("wl-copy")
                .args(["--type", "text/uri-list"])
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            else {
                return;
            };
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(uri.as_bytes());
            }
            let _ = child.wait();
        }
    }
}

pub fn copy_mime(ext: &str) -> Option<&'static str> {
    match ext.to_ascii_lowercase().as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "svg" => Some("image/svg+xml"),
        "bmp" => Some("image/bmp"),
        _ => None,
    }
}

pub fn open_argv(path: &Path, kind: FileKind, loc: Option<f64>, editor: &str) -> Vec<OsString> {
    match kind {
        FileKind::Pdf => pdf_argv(path, loc),
        FileKind::Video | FileKind::Audio => {
            let mut args = vec![OsString::from("mpv")];
            if let Some(start) = loc {
                args.push(format!("--start={start}").into());
            }
            args.push(path.as_os_str().to_os_string());
            args
        }
        FileKind::Code => editor_argv(path, loc, editor),
        FileKind::Doc | FileKind::Image | FileKind::Screenshot if looks_like_code(path) => {
            editor_argv(path, loc, editor)
        }
        _ => vec![OsString::from("xdg-open"), path.as_os_str().to_os_string()],
    }
}

fn looks_like_code(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    if Rules::CODE_FILE_NAMES.contains(&name) {
        return true;
    }
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    Rules::SOURCE_EXTENSIONS.contains(&ext.as_str())
}

fn pdf_argv(path: &Path, loc: Option<f64>) -> Vec<OsString> {
    let page = loc.map(|n| n.max(1.0) as i64).unwrap_or(1);
    if which("evince") {
        vec![
            "evince".into(),
            "-p".into(),
            page.to_string().into(),
            path.as_os_str().to_os_string(),
        ]
    } else if which("zathura") {
        vec![
            "zathura".into(),
            "-P".into(),
            page.to_string().into(),
            path.as_os_str().to_os_string(),
        ]
    } else {
        vec!["xdg-open".into(), path.as_os_str().to_os_string()]
    }
}

fn editor_argv(path: &Path, loc: Option<f64>, editor: &str) -> Vec<OsString> {
    let line = loc.map(|n| n.max(1.0) as i64).unwrap_or(1);
    let bin = resolve_editor(editor);
    match bin.as_str() {
        "cursor" | "code" | "codium" | "zed" => vec![
            bin.into(),
            "-g".into(),
            format!("{}:{line}", path.display()).into(),
        ],
        "nvim" => nvim_argv(path, line),
        "hx" => terminal_exec(&["hx", &format!("{path}:{line}", path = path.display())]),
        "kate" => vec![
            "kate".into(),
            "--line".into(),
            line.to_string().into(),
            path.as_os_str().to_os_string(),
        ],
        other if other != "xdg-open" && which(other) => {
            vec![other.into(), path.as_os_str().to_os_string()]
        }
        _ => vec!["xdg-open".into(), path.as_os_str().to_os_string()],
    }
}

/// Chosen editor, or the first installed one. A missing choice (or the
/// agent-only `cursor` shim) falls through. If nothing is installed, `xdg-open`.
fn resolve_editor(editor: &str) -> String {
    let wanted = if editor.is_empty() {
        None
    } else {
        PathBuf::from(editor)
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
    };
    if let Some(name) = wanted {
        if editor_present(&name) {
            return name;
        }
    }
    first_editor().unwrap_or_else(|| "xdg-open".into())
}

fn first_editor() -> Option<String> {
    // GUI editors first, then TUI. Skip the Cursor *agent* shim that is not
    // an IDE (`cursor --help` → "No Cursor IDE installation found").
    ["code", "codium", "zed", "nvim", "hx", "kate", "cursor"]
        .into_iter()
        .find(|n| editor_present(n))
        .map(str::to_string)
}

fn editor_present(name: &str) -> bool {
    if !which(name) {
        return false;
    }
    if name == "cursor" {
        return cursor_ide_present();
    }
    true
}

fn cursor_ide_present() -> bool {
    let output = Command::new("cursor")
        .arg("--help")
        .stdin(Stdio::null())
        .output();
    match output {
        Ok(out) => {
            let text = format!(
                "{}{}",
                String::from_utf8_lossy(&out.stderr),
                String::from_utf8_lossy(&out.stdout)
            );
            if text.contains("No Cursor IDE installation found") {
                return false;
            }
            out.status.success() || text.to_ascii_lowercase().contains("goto")
        }
        Err(_) => false,
    }
}

fn nvim_argv(path: &Path, line: i64) -> Vec<OsString> {
    if which("nvr") {
        return vec![
            "nvr".into(),
            "-s".into(),
            format!("+{line}").into(),
            path.as_os_str().to_os_string(),
        ];
    }
    terminal_exec(&["nvim", &format!("+{line}"), &path.to_string_lossy()])
}

fn terminal_exec(args: &[&str]) -> Vec<OsString> {
    if which("xdg-terminal-exec") {
        let mut out = vec![OsString::from("xdg-terminal-exec")];
        out.extend(args.iter().map(OsString::from));
        return out;
    }
    for (term, flag) in [
        ("kitty", Some("-e")),
        ("ghostty", Some("-e")),
        ("alacritty", Some("-e")),
        ("foot", None),
    ] {
        if !which(term) {
            continue;
        }
        let mut out = vec![OsString::from(term)];
        if let Some(flag) = flag {
            out.push(flag.into());
        }
        out.extend(args.iter().map(OsString::from));
        return out;
    }
    args.iter().map(OsString::from).collect()
}

pub(crate) fn which(name: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(name).is_file()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pdf_uses_page_flag() {
        let args = pdf_argv(Path::new("/tmp/a.pdf"), Some(3.0));
        let joined: Vec<String> = args
            .iter()
            .map(|s| s.to_string_lossy().into_owned())
            .collect();
        assert!(
            joined.contains(&"3".into()) || joined[0] == "xdg-open",
            "{joined:?}"
        );
    }

    #[test]
    fn media_passes_start() {
        let args = open_argv(Path::new("/tmp/a.mp4"), FileKind::Video, Some(6.5), "");
        assert_eq!(args[0], "mpv");
        assert_eq!(args[1], "--start=6.5");
    }

    #[test]
    fn nvim_opens_in_a_terminal() {
        if !which("nvim") {
            return;
        }
        let args = editor_argv(Path::new("/tmp/a.ts"), Some(12.0), "nvim");
        let joined: Vec<String> = args
            .iter()
            .map(|s| s.to_string_lossy().into_owned())
            .collect();
        assert!(joined.iter().any(|s| s == "nvim"), "{joined:?}");
        assert!(joined.iter().any(|s| s == "+12"), "{joined:?}");
        if which("xdg-terminal-exec") {
            assert_eq!(joined[0], "xdg-terminal-exec", "{joined:?}");
        }
    }

    #[test]
    fn missing_editor_does_not_run_a_ghost() {
        let args = editor_argv(
            Path::new("/tmp/a.rs"),
            Some(1.0),
            "definitely-not-an-editor-xyz",
        );
        assert_ne!(args[0], "definitely-not-an-editor-xyz");
    }

    #[test]
    fn tsx_from_the_document_index_still_uses_the_editor() {
        if !which("nvim") && first_editor().is_none() {
            return;
        }
        let args = open_argv(
            Path::new("/tmp/RouteValidator.tsx"),
            FileKind::Doc,
            Some(8.0),
            "nvim",
        );
        let joined: Vec<String> = args
            .iter()
            .map(|s| s.to_string_lossy().into_owned())
            .collect();
        assert!(
            joined
                .iter()
                .any(|s| s.contains("nvim") || s.contains("xdg-open")),
            "{joined:?}"
        );
        if which("nvim") {
            assert!(joined.iter().any(|s| s == "nvim"), "{joined:?}");
        }
    }

    #[test]
    fn png_stays_xdg_open() {
        let args = open_argv(Path::new("/tmp/shot.png"), FileKind::Screenshot, None, "");
        assert_eq!(args[0], "xdg-open");
    }

    #[test]
    fn file_uris_are_percent_encoded() {
        assert_eq!(file_uri(Path::new("/tmp/a.txt")), "file:///tmp/a.txt");
        assert_eq!(
            file_uri(Path::new("/home/me/My Notes/it's #1 100%.md")),
            "file:///home/me/My%20Notes/it%27s%20%231%20100%25.md"
        );
        assert_eq!(file_uri(Path::new("/tmp/café")), "file:///tmp/caf%C3%A9");
    }

    #[test]
    fn copy_mime_for_pictures() {
        assert_eq!(copy_mime("png"), Some("image/png"));
        assert_eq!(copy_mime("JPEG"), Some("image/jpeg"));
        assert_eq!(copy_mime("jpeg"), Some("image/jpeg"));
        assert_eq!(copy_mime("rs"), None);
    }
}
