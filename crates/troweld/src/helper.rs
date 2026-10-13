//! Indexing helper: `trowel index-service`. `upstream/Sources/DigUpApp/Engine.swift` `startHelper`.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

use serde_json::Value;

use crate::config_file::expand_tilde;
use trowel_proto::Config;

pub struct HelperOpts {
    pub index_dir: PathBuf,
    pub fake: bool,
    pub backfill: bool,
    pub linger: f64,
}

pub struct Helper {
    stdin: Mutex<Option<ChildStdin>>,
    child: Mutex<Option<Child>>,
}

impl Helper {
    pub fn spawn(
        opts: HelperOpts,
        cfg: &Config,
        on_event: impl Fn(Value) + Send + 'static,
    ) -> std::io::Result<Arc<Self>> {
        let bin = locate_trowel();
        let mut cmd = Command::new(&bin);
        cmd.arg("index-service")
            .arg("--index")
            .arg(&opts.index_dir)
            .arg("--linger")
            .arg(opts.linger.to_string());
        if opts.fake {
            cmd.arg("--fake");
        }
        if opts.backfill {
            cmd.arg("--backfill");
        }
        cmd.arg("--prune");
        for folder in &cfg.excluded_folders {
            cmd.arg("--exclude").arg(expand_tilde(folder));
        }
        for ext in &cfg.excluded_types {
            cmd.arg("--exclude-ext").arg(ext);
        }
        for folder in &cfg.code_roots {
            cmd.arg("--code-root").arg(expand_tilde(folder));
        }
        for folder in &cfg.code_excluded {
            cmd.arg("--code-exclude").arg(expand_tilde(folder));
        }
        let roots: Vec<PathBuf> = cfg.roots.iter().map(|r| expand_tilde(r)).collect();
        for root in &roots {
            cmd.arg(root);
        }
        if roots.is_empty() && cfg.code_roots.is_empty() {
            return Err(std::io::Error::other("no folders to index"));
        }
        let log_path = opts.index_dir.join("indexer.log");
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .ok();
        if let Some(log) = log {
            cmd.stderr(Stdio::from(log));
        } else {
            cmd.stderr(Stdio::null());
        }
        cmd.stdin(Stdio::piped()).stdout(Stdio::piped());
        // Die with troweld so a SIGKILL in tests does not leave index-service holding the DB.
        unsafe {
            use std::os::unix::process::CommandExt;
            cmd.pre_exec(|| {
                // SAFETY: called in the child after fork, before exec.
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM, 0, 0, 0);
                Ok(())
            });
        }
        let mut child = cmd
            .spawn()
            .map_err(|err| std::io::Error::other(format!("spawn {}: {err}", bin.display())))?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take();
        let helper = Arc::new(Self {
            stdin: Mutex::new(stdin),
            child: Mutex::new(Some(child)),
        });
        if let Some(stdout) = stdout {
            let helper_exit = Arc::clone(&helper);
            thread::spawn(move || {
                let reader = BufReader::new(stdout);
                for line in reader.lines() {
                    let Ok(line) = line else { break };
                    if let Ok(event) = serde_json::from_str::<Value>(&line) {
                        on_event(event);
                    }
                }
                if let Ok(mut slot) = helper_exit.child.lock() {
                    if let Some(mut child) = slot.take() {
                        let _ = child.wait();
                    }
                }
                on_event(serde_json::json!({"event": "exited"}));
            });
        }
        Ok(helper)
    }

    pub fn send(&self, command: &str) {
        if let Ok(mut slot) = self.stdin.lock() {
            if let Some(stdin) = slot.as_mut() {
                let _ = writeln!(stdin, "{command}");
                let _ = stdin.flush();
            }
        }
    }

    pub fn running(&self) -> bool {
        self.child.lock().ok().is_some_and(|c| c.is_some())
    }

    pub fn stop(&self) {
        self.send("stop");
    }
}

pub fn locate_trowel() -> PathBuf {
    if let Ok(p) = std::env::var("TROWEL") {
        return PathBuf::from(p);
    }
    if let Ok(exe) = std::env::current_exe() {
        let sibling = exe.with_file_name("trowel");
        if sibling.is_file() {
            return sibling;
        }
    }
    PathBuf::from("trowel")
}

pub fn linger_seconds() -> f64 {
    std::env::var("TROWEL_LINGER")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(60.0)
}
