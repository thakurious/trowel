//! System commands (P-D31, `PHASE9-PLAN.md` §1.3): a fixed allow-list of ids → argv, with a live
//! On/Off for toggles read without side effects (P0 item 5), plus "All Omarchy commands": every
//! `omarchy-*` script with `# omarchy:summary=` and without `omarchy:hidden`. The panel only ever
//! sends an id; nothing it types becomes a command line.
//!
//! Every script was read before it went on this list (2026-10-26, after the night-light incident
//! of P0): `omarchy-toggle-nightlight` takes only `--status`, any other call toggles.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{json, Value};
use trowel_proto::SystemCommand;

use super::Host;

/// How a toggle's state is read: a flag file under `$HOME` (On when it exists, or when it
/// doesn't with `invert`), or a command's output.
#[derive(Clone, Copy)]
enum Probe {
    None,
    File(&'static str, bool),
    /// `omarchy-toggle-nightlight --status` → `{"enabled": …}`.
    NightLight,
    /// `omarchy-shell notifications isDnd` → `on|off`.
    Dnd,
}

struct Entry {
    id: &'static str,
    name: &'static str,
    subtitle: &'static str,
    group: &'static str,
    glyph: &'static str,
    keywords: &'static [&'static str],
    confirm: bool,
    argv: &'static [&'static str],
    probe: Probe,
    /// Run and wait (≤ 4 s) so the reply carries the new state.
    wait: bool,
}

const ENTRIES: &[Entry] = &[
    Entry {
        id: "lock-screen",
        name: "Lock Screen",
        subtitle: "Lock the computer and turn off the display",
        group: "Session",
        glyph: "lock",
        keywords: &["lock", "screen", "away"],
        confirm: false,
        argv: &["omarchy-system-lock"],
        probe: Probe::None,
        wait: false,
    },
    Entry {
        id: "sleep",
        name: "Sleep",
        subtitle: "Suspend the computer",
        group: "Session",
        glyph: "sleep",
        keywords: &["sleep", "suspend"],
        confirm: false,
        argv: &["systemctl", "suspend"],
        probe: Probe::None,
        wait: false,
    },
    Entry {
        id: "log-out",
        name: "Log Out",
        subtitle: "Close every window and end the session",
        group: "Session",
        glyph: "power",
        keywords: &["log out", "logout", "sign out", "exit", "session"],
        confirm: true,
        argv: &["omarchy-system-logout"],
        probe: Probe::None,
        wait: false,
    },
    Entry {
        id: "restart",
        name: "Restart",
        subtitle: "Close every window and reboot",
        group: "Session",
        glyph: "power",
        keywords: &["restart", "reboot"],
        confirm: true,
        argv: &["omarchy-system-reboot"],
        probe: Probe::None,
        wait: false,
    },
    Entry {
        id: "shut-down",
        name: "Shut Down",
        subtitle: "Close every window and power off",
        group: "Session",
        glyph: "power",
        keywords: &["shut down", "shutdown", "power off", "poweroff", "turn off"],
        confirm: true,
        argv: &["omarchy-system-shutdown"],
        probe: Probe::None,
        wait: false,
    },
    Entry {
        id: "night-light",
        name: "Toggle Night Light",
        subtitle: "Warmer screen colors",
        group: "Toggles",
        glyph: "moon",
        keywords: &[
            "night",
            "light",
            "nightlight",
            "warm",
            "blue light",
            "hyprsunset",
        ],
        confirm: false,
        argv: &["omarchy-toggle-nightlight"],
        probe: Probe::NightLight,
        wait: true,
    },
    Entry {
        id: "keep-awake",
        name: "Toggle Keep Awake",
        subtitle: "Don't lock or dim when idle",
        group: "Toggles",
        glyph: "coffee",
        keywords: &["keep awake", "awake", "idle", "caffeine", "stay awake"],
        confirm: false,
        argv: &["omarchy-toggle-idle", "toggle"],
        probe: Probe::File(".local/state/omarchy/indicators/stay-awake", false),
        wait: true,
    },
    Entry {
        id: "do-not-disturb",
        name: "Toggle Do Not Disturb",
        subtitle: "Silence notifications",
        group: "Toggles",
        glyph: "bell",
        keywords: &["do not disturb", "dnd", "notifications", "silence", "focus"],
        confirm: false,
        argv: &["omarchy-toggle-notification-silencing"],
        probe: Probe::Dnd,
        wait: true,
    },
    Entry {
        id: "hide-bar",
        name: "Toggle Hide Bar",
        subtitle: "Show or hide the top bar",
        group: "Toggles",
        glyph: "bar",
        keywords: &["bar", "hide bar", "panel", "waybar"],
        confirm: false,
        argv: &["omarchy-toggle-bar", "toggle"],
        probe: Probe::File(".local/state/omarchy/toggles/bar-off", false),
        wait: true,
    },
    Entry {
        id: "touchpad",
        name: "Toggle Touchpad",
        subtitle: "Turn the touchpad on or off",
        group: "Toggles",
        glyph: "touchpad",
        keywords: &["touchpad", "trackpad", "mouse"],
        confirm: false,
        argv: &["omarchy-toggle-touchpad", "toggle"],
        probe: Probe::File(
            ".local/state/omarchy/toggles/hypr/touchpad-disabled-name",
            true,
        ),
        wait: true,
    },
    Entry {
        id: "suspend-in-menu",
        name: "Toggle Suspend in Menu",
        subtitle: "Offer Suspend in the system menu",
        group: "Toggles",
        glyph: "sleep",
        keywords: &["suspend", "menu", "system menu"],
        confirm: false,
        argv: &["omarchy-toggle-suspend"],
        probe: Probe::File(".local/state/omarchy/toggles/suspend-off", true),
        wait: true,
    },
    Entry {
        id: "restart-wifi",
        name: "Restart Wi-Fi",
        subtitle: "Unblock and rescan Wi-Fi",
        group: "Restart",
        glyph: "wifi",
        keywords: &["wifi", "wi-fi", "network", "wireless", "restart"],
        confirm: false,
        argv: &["omarchy-restart-wifi"],
        probe: Probe::None,
        wait: false,
    },
    Entry {
        id: "restart-bluetooth",
        name: "Restart Bluetooth",
        subtitle: "Unblock Bluetooth",
        group: "Restart",
        glyph: "bluetooth",
        keywords: &["bluetooth", "restart"],
        confirm: false,
        argv: &["omarchy-restart-bluetooth"],
        probe: Probe::None,
        wait: false,
    },
    Entry {
        id: "restart-audio",
        name: "Restart Audio",
        subtitle: "Restart PipeWire and WirePlumber",
        group: "Restart",
        glyph: "sound",
        keywords: &["audio", "sound", "pipewire", "restart", "speakers"],
        confirm: false,
        argv: &["omarchy-restart-audio"],
        probe: Probe::None,
        wait: false,
    },
    Entry {
        id: "restart-shell",
        name: "Restart Shell",
        subtitle: "Restart the Omarchy shell (bar, launcher, Trowel's panel)",
        group: "Restart",
        glyph: "refresh",
        keywords: &["shell", "bar", "restart", "omarchy shell"],
        confirm: false,
        argv: &["omarchy-restart-shell"],
        probe: Probe::None,
        wait: false,
    },
    Entry {
        id: "next-background",
        name: "Next Background",
        subtitle: "Cycle the current theme's wallpaper",
        group: "Look",
        glyph: "image",
        keywords: &["background", "wallpaper", "next"],
        confirm: false,
        argv: &["omarchy-theme-bg-next"],
        probe: Probe::None,
        wait: false,
    },
];

fn entry(id: &str) -> Option<&'static Entry> {
    ENTRIES.iter().find(|e| e.id == id)
}

fn probe(host: &dyn Host, p: Probe) -> Option<bool> {
    match p {
        Probe::None => None,
        Probe::File(rel, invert) => Some(host.home().join(rel).exists() != invert),
        Probe::NightLight => {
            let out = host
                .capture(
                    &["omarchy-toggle-nightlight".into(), "--status".into()],
                    Duration::from_millis(800),
                )
                .ok()?;
            serde_json::from_str::<Value>(out.trim()).ok()?["enabled"].as_bool()
        }
        Probe::Dnd => {
            let out = host
                .capture(
                    &[
                        "omarchy-shell".into(),
                        "notifications".into(),
                        "isDnd".into(),
                    ],
                    Duration::from_millis(800),
                )
                .ok()?;
            match out.trim() {
                "on" => Some(true),
                "off" => Some(false),
                _ => None,
            }
        }
    }
}

fn command(host: &dyn Host, e: &Entry, with_state: bool) -> SystemCommand {
    SystemCommand {
        id: e.id.into(),
        name: e.name.into(),
        subtitle: e.subtitle.into(),
        group: e.group.into(),
        glyph: e.glyph.into(),
        keywords: e.keywords.iter().map(|k| k.to_string()).collect(),
        confirm: e.confirm,
        state: if with_state {
            probe(host, e.probe)
        } else {
            None
        },
    }
}

/// The curated list, toggles' states read in parallel (each probe ≤ 800 ms).
pub fn curated(host: &dyn Host) -> Vec<SystemCommand> {
    std::thread::scope(|s| {
        let handles: Vec<_> = ENTRIES
            .iter()
            .map(|e| s.spawn(move || command(host, e, true)))
            .collect();
        handles
            .into_iter()
            .zip(ENTRIES)
            .map(|(h, e)| h.join().unwrap_or_else(|_| command(host, e, false)))
            .collect()
    })
}

/// One `omarchy-*` script with Omarchy's metadata header.
#[derive(Debug, Clone, PartialEq)]
pub struct Script {
    pub name: String,
    pub path: PathBuf,
    pub summary: String,
    pub group: String,
    /// `omarchy:args`; a required `<arg>` keeps it off the list (it can't run without one).
    pub args: String,
    /// Reads input (gum, `read`, fzf, sudo): runs in a terminal.
    pub interactive: bool,
}

/// Parses the `# omarchy:key=value` header of a script; None for hidden or summary-less ones.
pub fn parse_script(name: &str, path: &Path, text: &str) -> Option<Script> {
    let mut summary = None;
    let mut group = String::new();
    let mut args = String::new();
    for line in text.lines().take(40) {
        let Some(meta) = line.strip_prefix("# omarchy:") else {
            continue;
        };
        let Some((k, v)) = meta.split_once('=') else {
            continue;
        };
        match k.trim() {
            "summary" => summary = Some(v.trim().to_string()),
            "group" => group = v.trim().to_string(),
            "args" => args = v.trim().to_string(),
            "hidden" if v.trim() == "true" => return None,
            _ => {}
        }
    }
    let summary = summary.filter(|s| !s.is_empty())?;
    if args.starts_with('<') {
        return None;
    }
    let interactive = [
        "gum ", "gum\n", "read -", "read ", "fzf", "sudo ", "$EDITOR", "less ",
    ]
    .iter()
    .any(|m| text.contains(m));
    Some(Script {
        name: name.into(),
        path: path.into(),
        summary,
        group,
        args,
        interactive,
    })
}

/// `$OMARCHY_PATH/bin` (default `/usr/share/omarchy/bin`) and `~/.local/share/omarchy/bin`.
pub fn script_dirs(home: &Path) -> Vec<PathBuf> {
    let base = std::env::var_os("OMARCHY_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/usr/share/omarchy"));
    vec![base.join("bin"), home.join(".local/share/omarchy/bin")]
}

pub fn scan(dirs: &[PathBuf]) -> Vec<Script> {
    let mut out: Vec<Script> = Vec::new();
    for dir in dirs {
        let Ok(rd) = std::fs::read_dir(dir) else {
            continue;
        };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if !name.starts_with("omarchy-") || out.iter().any(|s| s.name == name) {
                continue;
            }
            let path = e.path();
            // The header is in the first lines; 16 KiB covers the interactive check too.
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            let text = String::from_utf8_lossy(&bytes[..bytes.len().min(16 * 1024)]);
            if let Some(s) = parse_script(&name, &path, &text) {
                out.push(s);
            }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// `omarchy-toggle-nightlight` → "Toggle Nightlight".
pub fn title(name: &str) -> String {
    name.trim_start_matches("omarchy-")
        .split('-')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect::<String>())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn script_json(s: &Script) -> Value {
    json!({
        "id": format!("omarchy:{}", s.name),
        "name": title(&s.name),
        "script": s.name,
        "subtitle": s.summary,
        "group": s.group,
        "terminal": s.interactive,
    })
}

/// What `system_run {command_id}` would start: argv, and whether to wait for it.
pub enum Plan {
    Run(Vec<String>, bool),
    Unknown,
}

pub fn plan(id: &str, scripts: &[Script], themes: &[String]) -> Plan {
    if let Some(e) = entry(id) {
        return Plan::Run(e.argv.iter().map(|s| s.to_string()).collect(), e.wait);
    }
    if let Some(name) = id.strip_prefix("theme:") {
        if themes.iter().any(|t| t == name) {
            return Plan::Run(vec!["omarchy-theme-set".into(), name.into()], false);
        }
        return Plan::Unknown;
    }
    if let Some(name) = id.strip_prefix("omarchy:") {
        let Some(s) = scripts.iter().find(|s| s.name == name) else {
            return Plan::Unknown;
        };
        let path = s.path.display().to_string();
        if s.interactive {
            return Plan::Run(
                vec![
                    "omarchy-launch-floating-terminal-with-presentation".into(),
                    path,
                ],
                false,
            );
        }
        return Plan::Run(vec![path], false);
    }
    Plan::Unknown
}

/// The toggle's state after it ran (None for plain actions).
pub fn state_after(host: &dyn Host, id: &str) -> Option<bool> {
    entry(id).and_then(|e| probe(host, e.probe))
}

/// `omarchy-theme-list` (names as Omarchy prints them) and the current one.
pub fn themes(host: &dyn Host) -> (Vec<String>, String) {
    let list = host
        .capture(&["omarchy-theme-list".into()], Duration::from_secs(2))
        .unwrap_or_default();
    let current = host
        .capture(&["omarchy-theme-current".into()], Duration::from_secs(2))
        .unwrap_or_default();
    (
        list.lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect(),
        current.trim().to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_headers() {
        let p = Path::new("/x");
        let s = parse_script(
            "omarchy-theme-bg-next",
            p,
            "#!/bin/bash\n# omarchy:summary=Cycle to the next background\n# omarchy:examples=x\necho",
        )
        .unwrap();
        assert_eq!(s.summary, "Cycle to the next background");
        assert!(!s.interactive);
        assert!(parse_script("omarchy-x", p, "#!/bin/bash\necho hi").is_none());
        assert!(parse_script(
            "omarchy-x",
            p,
            "# omarchy:summary=Internal\n# omarchy:hidden=true\n"
        )
        .is_none());
        assert!(parse_script(
            "omarchy-theme-set",
            p,
            "# omarchy:summary=Apply a theme\n# omarchy:args=<theme-name>\n"
        )
        .is_none());
        let opt = parse_script(
            "omarchy-toggle-bar",
            p,
            "# omarchy:summary=Toggle bar\n# omarchy:args=[toggle|on|off]\n",
        )
        .unwrap();
        assert_eq!(opt.args, "[toggle|on|off]");
        let tui = parse_script(
            "omarchy-pkg-install",
            p,
            "# omarchy:summary=Install a package\nchoice=$(gum choose a b)\n",
        )
        .unwrap();
        assert!(tui.interactive);
        assert_eq!(title("omarchy-toggle-nightlight"), "Toggle Nightlight");
    }

    #[test]
    fn scan_fixture_dir_and_plans() {
        let dir = std::env::temp_dir().join(format!("trowel-scripts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("omarchy-a"), "# omarchy:summary=A\n").unwrap();
        std::fs::write(
            dir.join("omarchy-b"),
            "# omarchy:summary=B\n# omarchy:hidden=true\n",
        )
        .unwrap();
        std::fs::write(dir.join("omarchy-c"), "# omarchy:summary=C\nread -r x\n").unwrap();
        std::fs::write(dir.join("other"), "# omarchy:summary=Nope\n").unwrap();
        let scripts = scan(std::slice::from_ref(&dir));
        let names: Vec<_> = scripts.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["omarchy-a", "omarchy-c"]);
        match plan("omarchy:omarchy-c", &scripts, &[]) {
            Plan::Run(argv, _) => {
                assert_eq!(
                    argv[0],
                    "omarchy-launch-floating-terminal-with-presentation"
                )
            }
            Plan::Unknown => panic!(),
        }
        assert!(matches!(
            plan("omarchy:omarchy-b", &scripts, &[]),
            Plan::Unknown
        ));
        assert!(matches!(
            plan("omarchy:../../bin/sh", &scripts, &[]),
            Plan::Unknown
        ));
        assert!(matches!(plan("rm -rf", &scripts, &[]), Plan::Unknown));
        match plan("night-light", &scripts, &[]) {
            Plan::Run(argv, wait) => {
                // Never `status`: any argument but `--status` toggles it.
                assert_eq!(argv, ["omarchy-toggle-nightlight"]);
                assert!(wait);
            }
            Plan::Unknown => panic!(),
        }
        let themes = vec!["Tokyo Night".to_string()];
        assert!(matches!(
            plan("theme:Tokyo Night", &scripts, &themes),
            Plan::Run(..)
        ));
        assert!(matches!(
            plan("theme:Evil; rm", &scripts, &themes),
            Plan::Unknown
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ids_are_unique() {
        let mut ids: Vec<_> = ENTRIES.iter().map(|e| e.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), ENTRIES.len());
    }
}
