//! `open` / `reveal`. PROTOCOL.md §4 Actions.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use trowel_core::FileKind;

pub fn open(path: &Path, kind: FileKind, loc: Option<f64>, editor: &str) {
    let argv = open_argv(path, kind, loc, editor);
    if argv.is_empty() {
        return;
    }
    let mut cmd = Command::new(&argv[0]);
    cmd.args(&argv[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let _ = cmd.spawn();
}

pub fn reveal(path: &Path) {
    let uri = format!("file://{}", path.display());
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
    let _ = Command::new("xdg-open")
        .arg(parent)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
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
        _ => vec![OsString::from("xdg-open"), path.as_os_str().to_os_string()],
    }
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
    let bin = if editor.is_empty() {
        first_which(&["cursor", "code", "codium", "zed", "nvim", "hx", "kate"])
            .unwrap_or_else(|| "xdg-open".into())
    } else {
        PathBuf::from(editor)
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| editor.to_string())
    };
    match bin.as_str() {
        "cursor" | "code" | "codium" | "zed" => vec![
            bin.into(),
            "-g".into(),
            format!("{}:{line}", path.display()).into(),
        ],
        "nvim" | "hx" => vec![
            bin.into(),
            format!("+{line}").into(),
            path.as_os_str().to_os_string(),
        ],
        "kate" => vec![
            "kate".into(),
            "--line".into(),
            line.to_string().into(),
            path.as_os_str().to_os_string(),
        ],
        _ => vec!["xdg-open".into(), path.as_os_str().to_os_string()],
    }
}

fn which(name: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(name).is_file()))
}

fn first_which(names: &[&str]) -> Option<String> {
    names.iter().find(|n| which(n)).map(|s| (*s).to_string())
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
}
