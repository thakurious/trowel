//! `trowel apps` / `trowel launch` (`LAUNCHER-PLAN.md` L2), and the parity gate's plumbing.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use troweld::apps::icons::IconIndex;
use troweld::apps::rank::{self, Rankable};
use troweld::apps::{self, AppIndex, AppsEnv};

pub struct AppsArgs {
    pub query: Option<String>,
    pub json: bool,
    pub limit: usize,
    pub icons: bool,
    pub dump_entries: bool,
    pub entries: Option<PathBuf>,
    pub rank_queries: Option<PathBuf>,
}

pub fn apps(args: AppsArgs) -> i32 {
    if let Some(queries) = args.rank_queries {
        return rank_queries(args.entries.as_deref(), &queries);
    }
    let env = AppsEnv::from_env();
    let (mut index, hidden) = AppIndex::scan(&env);
    if args.dump_entries {
        let rows: Vec<Value> = index.rankables().iter().map(entry_json).collect();
        println!("{}", Value::Array(rows));
        return 0;
    }
    let mut icons_ms = None;
    if args.icons {
        let started = Instant::now();
        index.set_icons(IconIndex::build());
        icons_ms = Some(started.elapsed().as_secs_f64() * 1000.0);
    }
    // The CLI ranks without learning: launches and search terms live in the daemon's state.sqlite.
    let query = args.query.unwrap_or_default();
    let started = Instant::now();
    let rows = index.query(&query, args.limit, &HashMap::new(), &HashMap::new(), 0);
    let took_us = started.elapsed().as_micros() as u64;
    if args.json {
        println!(
            "{}",
            json!({
                "rows": rows,
                "took_us": took_us,
                "apps": index.len(),
                "scan_ms": index.scanned_in.as_secs_f64() * 1000.0,
                "icons_ms": icons_ms,
                "hidden": {"computed": hidden.computed, "omarchy": hidden.omarchy,
                           "trowel": hidden.trowel, "try_exec": hidden.try_exec},
            })
        );
    } else {
        for row in &rows {
            let generic = row.generic.as_deref().unwrap_or("");
            println!("{:<32} {:<28} {}", row.name, generic, row.id);
        }
    }
    0
}

/// The fields `AppSearch.js` reads, named as Quickshell exposes them to QML.
fn entry_json(e: &Rankable) -> Value {
    json!({"id": e.id, "name": e.name, "genericName": e.generic_name,
           "comment": e.comment, "keywords": e.keywords, "noDisplay": false})
}

/// Ranks each line of `queries` over the entries in `entries` (a `--dump-entries` file) with no
/// frecency, printing `{query: [ids]}`: the Rust side of `scripts/apps-parity.sh`.
fn rank_queries(entries: Option<&Path>, queries: &Path) -> i32 {
    let Some(entries) = entries else {
        eprintln!("trowel apps --rank-queries needs --entries FILE (from --dump-entries)");
        return 2;
    };
    let parsed: Vec<Value> = match fs::read_to_string(entries)
        .map_err(|e| e.to_string())
        .and_then(|t| serde_json::from_str(&t).map_err(|e| e.to_string()))
    {
        Ok(v) => v,
        Err(err) => {
            eprintln!("{}: {err}", entries.display());
            return 2;
        }
    };
    let text = |v: &Value, k: &str| v[k].as_str().unwrap_or("").to_string();
    let rankables: Vec<Rankable> = parsed
        .iter()
        .map(|v| Rankable {
            id: text(v, "id"),
            name: text(v, "name"),
            generic_name: text(v, "genericName"),
            comment: text(v, "comment"),
            keywords: v["keywords"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|k| k.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
        })
        .collect();
    let zero = vec![troweld::learn::Signal::NONE; rankables.len()];
    let mut out = BTreeMap::new();
    for q in fs::read_to_string(queries).unwrap_or_default().lines() {
        let ids: Vec<&str> = rank::sorted(&rankables, q, &zero)
            .iter()
            .map(|r| rankables[r.index].id.as_str())
            .collect();
        out.insert(q.to_string(), ids);
    }
    println!("{}", json!(out));
    0
}

fn socket_path() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("trowel/troweld.sock")
}

/// Asks the running daemon (so the launch counts for frecency); `None` when there is none.
fn via_daemon(id: &str, action: Option<&str>) -> Option<Result<(), String>> {
    let stream = UnixStream::connect(socket_path()).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    let mut writer = stream.try_clone().ok()?;
    let mut reader = BufReader::new(stream);
    let mut send = |v: Value| writeln!(writer, "{v}").ok();
    send(
        json!({"id":1,"op":"hello","protocol":trowel_proto::PROTOCOL_VERSION,"client":"cli","pid":std::process::id()}),
    )?;
    send(json!({"id":2,"op":"app_launch","app_id":id,"action":action}))?;
    let mut line = String::new();
    for _ in 0..2 {
        line.clear();
        reader.read_line(&mut line).ok()?;
        let v: Value = serde_json::from_str(line.trim()).ok()?;
        if v["ok"] == false {
            let message = v["error"]["message"]
                .as_str()
                .unwrap_or("failed")
                .to_string();
            // An older daemon (protocol 1, or one without apps): launch here instead.
            if v["id"] == 1 || v["error"]["code"] == "not_ready" {
                return None;
            }
            return Some(Err(message));
        }
    }
    Some(Ok(()))
}

pub fn launch(id: &str, action: Option<&str>) -> i32 {
    match via_daemon(id, action) {
        Some(Ok(())) => return 0,
        Some(Err(err)) => {
            eprintln!("trowel launch: {err}");
            return 1;
        }
        None => {}
    }
    let (index, _) = AppIndex::scan(&AppsEnv::from_env());
    let Some(entry) = index.get(id) else {
        eprintln!("trowel launch: no application {id}");
        return 1;
    };
    let argv = match apps::launch_argv(entry, action, apps::uwsm_available()) {
        Ok(argv) => argv,
        Err(err) => {
            eprintln!("trowel launch: {err}");
            return 1;
        }
    };
    match apps::spawn(&argv) {
        Ok(()) => {
            eprintln!("trowel launch: no daemon with apps support answered; launched without recording it");
            0
        }
        Err(err) => {
            eprintln!("trowel launch: {err}");
            1
        }
    }
}
