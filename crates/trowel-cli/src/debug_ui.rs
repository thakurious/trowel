//! `trowel debug ui-trace` / `snapshot`. PROTOCOL timings vs UI-DESIGN §5.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

pub fn default_socket() -> PathBuf {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    runtime.join("trowel/troweld.sock")
}

fn connect(socket: &Path) -> Result<UnixStream, String> {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match UnixStream::connect(socket) {
            Ok(s) => {
                s.set_read_timeout(Some(Duration::from_secs(2)))
                    .map_err(|e| e.to_string())?;
                s.set_write_timeout(Some(Duration::from_millis(500)))
                    .map_err(|e| e.to_string())?;
                return Ok(s);
            }
            Err(_) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(err) => {
                return Err(format!(
                    "cannot connect to {} within 2s: {err}",
                    socket.display()
                ));
            }
        }
    }
}

fn send(stream: &mut UnixStream, v: &Value) -> Result<(), String> {
    writeln!(stream, "{v}").map_err(|e| e.to_string())?;
    stream.flush().map_err(|e| e.to_string())
}

fn recv(reader: &mut BufReader<UnixStream>) -> Result<Value, String> {
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|e| e.to_string())
        .and_then(|n| {
            if n == 0 {
                Err("eof".into())
            } else {
                serde_json::from_str(line.trim()).map_err(|e| e.to_string())
            }
        })
}

fn roundtrip(
    stream: &mut UnixStream,
    reader: &mut BufReader<UnixStream>,
    req: Value,
) -> Result<(Value, Duration), String> {
    let t0 = Instant::now();
    send(stream, &req)?;
    let mut last = recv(reader)?;
    while last.get("event").is_some() {
        last = recv(reader)?;
    }
    Ok((last, t0.elapsed()))
}

pub fn ui_trace(socket: PathBuf, as_json: bool) -> Result<(), String> {
    let mut stream = connect(&socket)?;
    let mut reader = BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
    let mut rows = Vec::new();

    let (hello, d) = roundtrip(
        &mut stream,
        &mut reader,
        json!({"id":1,"op":"hello","protocol":1,"client":"ui-trace","pid":0}),
    )?;
    if hello["ok"] != true {
        return Err(format!("hello failed: {hello}"));
    }
    rows.push(("hello", d, 50u64));

    let (_, d) = roundtrip(
        &mut stream,
        &mut reader,
        json!({"id":2,"op":"panel_opened"}),
    )?;
    rows.push(("panel_opened", d, 50));

    let (kw, d) = roundtrip(
        &mut stream,
        &mut reader,
        json!({"id":3,"op":"search","text":"refund","mode":"keyword","prefix_last":true,"gen":1}),
    )?;
    if kw["groups"]
        .as_array()
        .map(|a| a.is_empty())
        .unwrap_or(true)
    {
        return Err(format!("keyword search empty: {kw}"));
    }
    rows.push(("keyword_refund", d, 40));

    let (hy, d) = roundtrip(
        &mut stream,
        &mut reader,
        json!({"id":4,"op":"search","text":"zebra","mode":"hybrid","prefix_last":false,"gen":2}),
    )?;
    let weaker = hy["weaker"].as_array().map(|a| a.len()).unwrap_or(0);
    if weaker == 0 {
        return Err(format!("hybrid zebra expected weaker rows: {hy}"));
    }
    rows.push(("hybrid_zebra", d, 200));

    let mut failed = Vec::new();
    if as_json {
        let out: Vec<Value> = rows
            .iter()
            .map(|(name, d, budget)| {
                let ms = d.as_secs_f64() * 1000.0;
                let ok = ms <= *budget as f64;
                if !ok {
                    failed.push(*name);
                }
                json!({
                    "name": name,
                    "ms": ms,
                    "budget_ms": budget,
                    "ok": ok,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&out).unwrap());
    } else {
        println!("{:<18} {:>8} {:>8} pass", "probe", "ms", "budget");
        for (name, d, budget) in &rows {
            let ms = d.as_secs_f64() * 1000.0;
            let ok = ms <= *budget as f64;
            if !ok {
                failed.push(*name);
            }
            println!(
                "{:<18} {:>8.2} {:>8} {}",
                name,
                ms,
                budget,
                if ok { "yes" } else { "NO" }
            );
        }
    }
    if !failed.is_empty() {
        return Err(format!(
            "ui-trace exceeded UI-DESIGN §5 budget: {}",
            failed.join(", ")
        ));
    }
    Ok(())
}

pub fn snapshot(socket: PathBuf, surface: &str, state: &str, out: PathBuf) -> Result<(), String> {
    let mut stream = connect(&socket)?;
    let mut reader = BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
    let _ = roundtrip(
        &mut stream,
        &mut reader,
        json!({"id":1,"op":"hello","protocol":1,"client":"snapshot","pid":0}),
    )?;
    let query = match (surface, state) {
        ("panel", "empty") | ("app", "home") => "",
        ("panel", "no-matches") => "zzzznotfound",
        ("panel", "results") | ("app", "results") => "refund",
        ("panel", "weaker") => "zebra",
        ("panel", "code") => "retry",
        _ => "refund",
    };
    let search = if query.is_empty() {
        json!({"groups": [], "weaker": [], "meaning": ""})
    } else {
        roundtrip(
            &mut stream,
            &mut reader,
            json!({"id":2,"op":"search","text":query,"mode":"hybrid","prefix_last":false,"gen":1}),
        )?
        .0
    };
    let preview = if surface == "app" { 324 } else { 450 };
    let body = json!({
        "surface": surface,
        "state": state,
        "geometry": {"width": 780, "list": 330, "preview": preview, "top": 0.16, "app": [1040, 700]},
        "keyboard": ["Esc","Up","Down","Return","Ctrl+Return","Space","Ctrl+Y","Ctrl+C","Ctrl+O","Tab"],
        "query": query,
        "groups": search["groups"],
        "weaker": search["weaker"],
        "meaning": search["meaning"],
    });
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&out, serde_json::to_string_pretty(&body).unwrap() + "\n")
        .map_err(|e| e.to_string())?;
    println!("wrote {}", out.display());
    Ok(())
}
