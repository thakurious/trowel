//! Files git keeps in a repo. `upstream/Sources/DigUpKit/Crawler.swift` `GitFiles` / `GitRepos`.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

/// `git ls-files -z --cached --others --exclude-standard` relative paths, or `None` when git cannot say.
pub fn list(repo: &Path) -> Option<HashSet<String>> {
    let repo_s = repo.to_str()?;
    let mut cmd = Command::new("git");
    cmd.args([
        "-C",
        repo_s,
        "ls-files",
        "-z",
        "--cached",
        "--others",
        "--exclude-standard",
    ])
    .env("GIT_OPTIONAL_LOCKS", "0")
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::null());
    let child = cmd.spawn().ok()?;
    let output = wait_short(child, Duration::from_secs(30))?;
    if !output.status.success() {
        return None;
    }
    Some(
        output
            .stdout
            .split(|b| *b == 0)
            .filter(|p| !p.is_empty())
            .filter_map(|p| String::from_utf8(p.to_vec()).ok())
            .collect(),
    )
}

fn wait_short(child: std::process::Child, timeout: Duration) -> Option<std::process::Output> {
    let pid = child.id();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    match rx.recv_timeout(timeout) {
        Ok(Ok(out)) => Some(out),
        _ => {
            unsafe {
                kill(pid as i32, 9);
            }
            let _ = rx.recv();
            None
        }
    }
}

unsafe extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
}

/// Repos a code crawl has entered, and the files each keeps.
#[derive(Default)]
pub struct GitRepos {
    files: HashMap<String, Option<HashSet<String>>>,
    repo_of: HashMap<String, Option<String>>,
}

impl GitRepos {
    pub fn enter_root(&mut self, path: &Path) {
        let mut folder = path.to_path_buf();
        loop {
            if folder.join(".git").exists() {
                let key = folder.to_string_lossy().into_owned();
                self.files
                    .entry(key.clone())
                    .or_insert_with(|| list(&folder));
                self.repo_of
                    .insert(path.to_string_lossy().into_owned(), Some(key));
                return;
            }
            match folder.parent() {
                Some(parent) if parent != folder && parent.as_os_str().len() > 1 => {
                    folder = parent.to_path_buf();
                }
                _ => break,
            }
        }
        self.repo_of
            .insert(path.to_string_lossy().into_owned(), None);
    }

    pub fn enter(&mut self, path: &Path) {
        let git = path.join(".git");
        if git.is_dir() {
            let key = path.to_string_lossy().into_owned();
            self.files.entry(key.clone()).or_insert_with(|| list(path));
            self.repo_of.insert(key.clone(), Some(key));
        }
    }

    fn repo_of_folder(&mut self, folder: &str) -> Option<String> {
        if let Some(known) = self.repo_of.get(folder) {
            return known.clone();
        }
        let parent = Path::new(folder)
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let answer = if parent != folder && parent.len() > 1 {
            self.repo_of_folder(&parent)
        } else {
            None
        };
        self.repo_of.insert(folder.to_string(), answer.clone());
        answer
    }

    pub fn ignores(&mut self, path: &Path) -> bool {
        let folder = path
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let Some(repo) = self.repo_of_folder(&folder) else {
            return false;
        };
        let Some(Some(kept)) = self.files.get(&repo) else {
            return false;
        };
        let rel = path
            .strip_prefix(&repo)
            .ok()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        !kept.contains(&rel)
    }
}
