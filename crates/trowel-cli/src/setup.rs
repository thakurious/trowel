//! `trowel setup` / `trowel uninstall` (DECISIONS D12, ROBUSTNESS §4).
//! Tests pass a fake XDG tree and never touch the live session.

use std::fs;
use std::io;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const PLUGIN_ID: &str = "io.github.thakurious.trowel";
pub const PLUGIN_ID_DEV: &str = "io.github.thakurious.trowel-dev";

#[derive(Clone, Debug)]
pub struct SetupDirs {
    pub home: PathBuf,
    pub config_home: PathBuf,
    pub data_home: PathBuf,
    pub bin_dir: PathBuf,
    pub from_tree: Option<PathBuf>,
    pub enable_live: bool,
}

impl SetupDirs {
    pub fn from_env() -> Self {
        let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_else(|| "/tmp".into()));
        let config_home = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"));
        let data_home = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local/share"));
        let bin_dir = std::env::var_os("TROWEL_BIN_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local/bin"));
        Self {
            home,
            config_home,
            data_home,
            bin_dir,
            from_tree: None,
            enable_live: true,
        }
    }
}

pub fn setup(dirs: &SetupDirs) -> Result<(), String> {
    let share = dirs.data_home.join("trowel");
    fs::create_dir_all(&dirs.bin_dir).map_err(io_err)?;
    fs::create_dir_all(&share).map_err(io_err)?;
    fs::create_dir_all(dirs.config_home.join("systemd/user")).map_err(io_err)?;
    fs::create_dir_all(dirs.config_home.join("hypr")).map_err(io_err)?;
    fs::create_dir_all(dirs.config_home.join("omarchy/plugins")).map_err(io_err)?;

    install_bins(dirs)?;
    install_qml(dirs, &share)?;
    write_systemd(dirs)?;
    write_hotkey(dirs)?;
    link_plugin(dirs, &share)?;

    if dirs.enable_live {
        enable_live(dirs)?;
    }
    println!("Trowel is installed.");
    println!("  plugin  {}", PLUGIN_ID);
    println!("  hotkey  SUPER+D");
    println!("  daemon  troweld.socket (user)");
    if dirs.enable_live {
        println!("Restart omarchy-shell once (or log out) so Super+D loads the panel.");
        println!("Onboarding runs the first time the panel opens; indexing starts after that.");
    }
    Ok(())
}

pub fn uninstall(dirs: &SetupDirs, purge: bool) -> Result<(), String> {
    if dirs.enable_live {
        let _ = Command::new("systemctl")
            .args(["--user", "disable", "--now", "troweld.socket"])
            .status();
        let _ = Command::new("systemctl")
            .args(["--user", "disable", "--now", "troweld.service"])
            .status();
        let _ = Command::new("omarchy-shell")
            .args(["shell", "disablePlugin", PLUGIN_ID])
            .status();
        let _ = Command::new("omarchy")
            .args(["plugin", "disable", PLUGIN_ID, "--yes"])
            .status();
    }
    let _ = fs::remove_file(dirs.config_home.join("omarchy/plugins").join(PLUGIN_ID));
    let _ = fs::remove_file(dirs.config_home.join("omarchy/plugins").join(PLUGIN_ID_DEV));
    let _ = fs::remove_file(dirs.config_home.join("systemd/user/troweld.service"));
    let _ = fs::remove_file(dirs.config_home.join("systemd/user/troweld.socket"));
    strip_hypr_require(&dirs.config_home.join("hypr/bindings.lua"))?;
    if purge {
        let _ = fs::remove_dir_all(dirs.data_home.join("trowel"));
        let cache = std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| dirs.home.join(".cache"));
        let _ = fs::remove_dir_all(cache.join("trowel"));
        let _ = fs::remove_dir_all(dirs.config_home.join("trowel"));
        let _ = fs::remove_file(dirs.config_home.join("hypr/trowel.lua"));
        let _ = fs::remove_file(dirs.config_home.join("hypr/trowel-app.lua"));
    }
    println!("Trowel plugin and user units removed.");
    if purge {
        println!("Index, models cache, and config deleted (--purge).");
    }
    Ok(())
}

fn io_err(err: io::Error) -> String {
    err.to_string()
}

fn install_bins(dirs: &SetupDirs) -> Result<(), String> {
    let troweld = locate_bin("troweld", dirs.from_tree.as_deref())?;
    let trowel = locate_bin("trowel", dirs.from_tree.as_deref())?;
    let worker = locate_worker(dirs.from_tree.as_deref())?;
    copy_or_link(&troweld, &dirs.bin_dir.join("troweld"))?;
    copy_or_link(&trowel, &dirs.bin_dir.join("trowel"))?;
    copy_or_link(&worker, &dirs.bin_dir.join("trowel-worker"))?;
    Ok(())
}

fn locate_bin(name: &str, tree: Option<&Path>) -> Result<PathBuf, String> {
    if let Some(tree) = tree {
        for sub in ["target/release", "target/debug"] {
            let p = tree.join(sub).join(name);
            if p.is_file() {
                return Ok(p);
            }
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        let sib = exe.with_file_name(name);
        if sib.is_file() {
            return Ok(sib);
        }
    }
    Err(format!("cannot find {name}; build the workspace first"))
}

fn locate_worker(tree: Option<&Path>) -> Result<PathBuf, String> {
    if let Some(tree) = tree {
        for sub in [
            "cpp/build-vulkan/trowel-worker",
            "cpp/build-cpu/trowel-worker",
        ] {
            let p = tree.join(sub);
            if p.is_file() {
                return Ok(p);
            }
        }
    }
    if let Ok(p) = std::env::var("TROWEL_WORKER") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Ok(p);
        }
    }
    Err("cannot find trowel-worker".into())
}

fn copy_or_link(src: &Path, dest: &Path) -> Result<(), String> {
    if src == dest {
        return Ok(());
    }
    if dest.exists() || dest.symlink_metadata().is_ok() {
        let _ = fs::remove_file(dest);
    }
    symlink(src, dest)
        .or_else(|_| fs::copy(src, dest).map(|_| ()))
        .map_err(|e| format!("install {}: {e}", dest.display()))
}

fn install_qml(dirs: &SetupDirs, share: &Path) -> Result<(), String> {
    let tree = dirs.from_tree.clone().or_else(repo_root);
    let Some(tree) = tree else {
        return Ok(());
    };
    let plugin_src = tree.join("ui/omarchy-plugin");
    let app_src = tree.join("ui/app");
    if plugin_src.is_dir() {
        let dest = share.join("omarchy-plugin");
        copy_dir(&plugin_src, &dest)?;
        let link = dest.join("Trowel");
        let _ = fs::remove_file(&link);
        let _ = fs::remove_dir_all(&link);
        symlink(tree.join("ui/qml/Trowel"), &link).map_err(io_err)?;
    }
    if app_src.is_dir() {
        let dest = share.join("app");
        copy_dir(&app_src, &dest)?;
        let link = dest.join("Trowel");
        let _ = fs::remove_file(&link);
        let _ = fs::remove_dir_all(&link);
        symlink(tree.join("ui/qml/Trowel"), &link).map_err(io_err)?;
    }
    let desktop_src = tree.join("packaging/trowel-app.desktop");
    if desktop_src.is_file() {
        let apps = dirs.data_home.join("applications");
        fs::create_dir_all(&apps).map_err(io_err)?;
        let mut body = fs::read_to_string(&desktop_src).map_err(io_err)?;
        body = body.replace(
            "Exec=quickshell -p /usr/share/trowel/app",
            &format!("Exec=quickshell -p {}", share.join("app").display()),
        );
        fs::write(apps.join("trowel.desktop"), body).map_err(io_err)?;
    }
    Ok(())
}

fn repo_root() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let mut dir = exe.parent()?;
    for _ in 0..8 {
        if dir.join("ui/omarchy-plugin/manifest.json").is_file() {
            return Some(dir.to_path_buf());
        }
        dir = dir.parent()?;
    }
    None
}

fn copy_dir(src: &Path, dest: &Path) -> Result<(), String> {
    fs::create_dir_all(dest).map_err(io_err)?;
    for entry in fs::read_dir(src).map_err(io_err)? {
        let entry = entry.map_err(io_err)?;
        let ty = entry.file_type().map_err(io_err)?;
        let to = dest.join(entry.file_name());
        if ty.is_dir() {
            copy_dir(&entry.path(), &to)?;
        } else if ty.is_symlink() {
            continue;
        } else {
            fs::copy(entry.path(), to).map_err(io_err)?;
        }
    }
    Ok(())
}

fn write_systemd(dirs: &SetupDirs) -> Result<(), String> {
    let troweld = dirs.bin_dir.join("troweld");
    let worker = dirs.bin_dir.join("trowel-worker");
    let unit_dir = dirs.config_home.join("systemd/user");
    let service = format!(
        "[Unit]\nDescription=Trowel search daemon\nAfter=troweld.socket\nRequires=troweld.socket\n\n\
[Service]\nType=notify\nExecStart={}\nEnvironment=TROWEL_WORKER={}\nRestart=on-failure\nRestartSec=2\n\
MemoryHigh=400M\nWatchdogSec=30\nNotifyAccess=main\n\n[Install]\nWantedBy=default.target\n",
        troweld.display(),
        worker.display()
    );
    let socket = "\
[Unit]\nDescription=Trowel daemon socket\n\n\
[Socket]\nListenStream=%t/trowel/troweld.sock\nSocketMode=0600\nDirectoryMode=0700\n\n\
[Install]\nWantedBy=sockets.target\n";
    fs::write(unit_dir.join("troweld.service"), service).map_err(io_err)?;
    fs::write(unit_dir.join("troweld.socket"), socket).map_err(io_err)?;
    // ROBUSTNESS §2: troweld runs the helper and worker in this slice, outside its own 400M.
    fs::write(
        unit_dir.join("trowel-index.slice"),
        include_str!("../../../packaging/systemd/trowel-index.slice"),
    )
    .map_err(io_err)?;
    Ok(())
}

/// The shortcut in `config.toml`, so running setup again keeps one changed in Settings.
fn load_hotkey(dirs: &SetupDirs) -> String {
    troweld::config_file::load(&dirs.config_home.join("trowel/config.toml"))
        .map(|cfg| cfg.hotkey)
        .unwrap_or_else(|_| "SUPER + D".into())
}

fn write_hotkey(dirs: &SetupDirs) -> Result<(), String> {
    let hypr = dirs.config_home.join("hypr");
    let lua = hypr.join("trowel.lua");
    let chord = load_hotkey(dirs);
    let body = troweld::hotkey::lua_body(&chord, "trowel setup");
    fs::write(&lua, body).map_err(io_err)?;
    let bindings = hypr.join("bindings.lua");
    if bindings.is_file() {
        let text = fs::read_to_string(&bindings).map_err(io_err)?;
        if !text.contains("hypr.trowel") && !text.contains("trowel.lua") {
            backup(&bindings)?;
            let mut next = text;
            if !next.ends_with('\n') {
                next.push('\n');
            }
            next.push_str("require(\"hypr.trowel\")\n");
            fs::write(&bindings, next).map_err(io_err)?;
        }
    }
    Ok(())
}

fn link_plugin(dirs: &SetupDirs, share: &Path) -> Result<(), String> {
    let dest = dirs.config_home.join("omarchy/plugins").join(PLUGIN_ID);
    let src = share.join("omarchy-plugin");
    if dest.symlink_metadata().is_ok() || dest.exists() {
        let _ = fs::remove_file(&dest);
        let _ = fs::remove_dir_all(&dest);
    }
    let dev = dirs.config_home.join("omarchy/plugins").join(PLUGIN_ID_DEV);
    if dev.symlink_metadata().is_ok() {
        let _ = fs::remove_file(&dev);
    }
    symlink(&src, &dest).map_err(io_err)?;
    Ok(())
}

fn enable_live(dirs: &SetupDirs) -> Result<(), String> {
    let _ = Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status();
    let st = Command::new("systemctl")
        .args(["--user", "enable", "--now", "troweld.socket"])
        .status()
        .map_err(|e| format!("systemctl: {e}"))?;
    if !st.success() {
        return Err("systemctl --user enable --now troweld.socket failed".into());
    }
    let _ = Command::new("omarchy-shell")
        .args(["shell", "rescanPlugins"])
        .status();
    let _ = Command::new("omarchy")
        .args(["plugin", "disable", PLUGIN_ID_DEV, "--yes"])
        .status();
    let en = Command::new("omarchy-shell")
        .args(["shell", "enablePlugin", PLUGIN_ID, "{}"])
        .status();
    if en.map(|s| !s.success()).unwrap_or(true) {
        let _ = Command::new("omarchy")
            .args(["plugin", "enable", PLUGIN_ID, "--yes"])
            .status();
    }
    patch_shell_json(&dirs.config_home.join("omarchy/shell.json"))?;
    Ok(())
}

fn patch_shell_json(path: &Path) -> Result<(), String> {
    if !path.is_file() {
        return Ok(());
    }
    let text = fs::read_to_string(path).map_err(io_err)?;
    let Ok(mut v) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Ok(());
    };
    backup(path)?;
    if let Some(obj) = v.as_object_mut() {
        let plugins = obj
            .entry("plugins")
            .or_insert_with(|| serde_json::json!({}));
        if let Some(p) = plugins.as_object_mut() {
            p.insert(PLUGIN_ID.into(), serde_json::json!(true));
            p.insert(PLUGIN_ID_DEV.into(), serde_json::json!(false));
        }
    }
    fs::write(path, serde_json::to_string_pretty(&v).unwrap() + "\n").map_err(io_err)?;
    Ok(())
}

fn strip_hypr_require(path: &Path) -> Result<(), String> {
    if !path.is_file() {
        return Ok(());
    }
    let text = fs::read_to_string(path).map_err(io_err)?;
    let next: String = text
        .lines()
        .filter(|l| !l.contains("hypr.trowel") && !l.contains("trowel.lua"))
        .map(|l| format!("{l}\n"))
        .collect();
    if next != text {
        backup(path)?;
        fs::write(path, next).map_err(io_err)?;
    }
    Ok(())
}

fn backup(path: &Path) -> Result<(), String> {
    if !path.is_file() {
        return Ok(());
    }
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let bak = path.parent().unwrap().join(format!(
        "{}.bak.trowel-{ts}",
        path.file_name().unwrap().to_string_lossy()
    ));
    fs::copy(path, bak).map_err(io_err)?;
    Ok(())
}
