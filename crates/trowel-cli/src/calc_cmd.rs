//! `trowel calc`: the calculator's CLI and test harness (`LAUNCHER-PLAN.md` L2c).

use std::fs;
use std::path::PathBuf;

use serde_json::json;
use troweld::calc::{Calculator, NumberStyle};

/// Rounds per row for `--bench`, so p95 is not one cold sample.
const BENCH_ROUNDS: usize = 20;
const BUDGET_P95_US: u64 = 1000;

pub fn run(
    text: Option<String>,
    force: bool,
    json: bool,
    bench: Option<PathBuf>,
    corpus: Vec<PathBuf>,
    quiet: bool,
) -> i32 {
    // Tables expect the English separators; a single expression follows the user's locale.
    if let Some(path) = bench {
        return run_bench(&Calculator::new(NumberStyle::EN), &path, json);
    }
    if !corpus.is_empty() {
        return run_corpus(&Calculator::new(NumberStyle::EN), &corpus, quiet);
    }
    let Some(text) = text else {
        eprintln!("trowel calc: give an expression, --bench FILE or --corpus FILE");
        return 2;
    };
    let calc = Calculator::default();
    let out = calc.eval(&text, force);
    if json {
        println!(
            "{}",
            json!({"result": out.result, "error": out.error, "took_us": out.took_us,
                   "warmup_us": calc.warmup.as_micros() as u64})
        );
    } else if let Some(r) = &out.result {
        let approx = if r.approx { "≈ " } else { "" };
        println!("{approx}{}", r.display);
    } else if let Some(err) = &out.error {
        eprintln!("{err}");
    }
    i32::from(out.result.is_none())
}

fn rows(path: &PathBuf) -> Result<Vec<(String, Option<String>)>, String> {
    // Lossy: the testbed has non-UTF-8 trap file names; typed text is always UTF-8.
    let raw = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let raw = String::from_utf8_lossy(&raw);
    Ok(raw
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with("# "))
        .map(|l| match l.split_once('\t') {
            Some((expr, want)) => (expr.to_string(), Some(want.trim().to_string())),
            None => (l.to_string(), None),
        })
        .collect())
}

fn run_bench(calc: &Calculator, path: &PathBuf, json: bool) -> i32 {
    let rows = match rows(path) {
        Ok(r) => r,
        Err(err) => {
            eprintln!("{err}");
            return 2;
        }
    };
    let mut times = Vec::new();
    let mut wrong = 0;
    for (expr, want) in &rows {
        for round in 0..BENCH_ROUNDS {
            let out = calc.eval(expr, false);
            times.push(out.took_us);
            if round > 0 {
                continue;
            }
            let got = out.result.as_ref().map(|r| r.copy.as_str());
            let want = want.as_deref().filter(|w| *w != "-");
            if got != want {
                wrong += 1;
                println!("MISMATCH {expr:?}: got {got:?}, want {want:?}");
            }
        }
    }
    times.sort_unstable();
    let pct = |p: f64| times[((times.len() as f64 - 1.0) * p).round() as usize];
    let (p50, p95, max) = (pct(0.5), pct(0.95), *times.last().unwrap_or(&0));
    if json {
        println!(
            "{}",
            json!({"rows": rows.len(), "wrong": wrong, "p50_us": p50, "p95_us": p95, "max_us": max, "budget_p95_us": BUDGET_P95_US})
        );
    } else {
        println!(
            "{} rows, {wrong} wrong, p50 {p50} µs, p95 {p95} µs, max {max} µs (budget p95 < {BUDGET_P95_US} µs)",
            rows.len()
        );
    }
    i32::from(wrong > 0 || p95 >= BUDGET_P95_US)
}

/// `quiet` prints line numbers only, for private corpora (clipboard history).
fn run_corpus(calc: &Calculator, paths: &[PathBuf], quiet: bool) -> i32 {
    let mut lines = 0;
    let mut cards = 0;
    let mut worst = 0;
    for path in paths {
        let rows = match rows(path) {
            Ok(r) => r,
            Err(err) => {
                eprintln!("{err}");
                return 2;
            }
        };
        for (n, (line, _)) in rows.into_iter().enumerate() {
            lines += 1;
            let out = calc.eval(&line, false);
            worst = worst.max(out.took_us);
            if let Some(r) = out.result {
                cards += 1;
                if quiet {
                    println!("CARD {}:{}", path.display(), n + 1);
                } else {
                    println!("CARD {line:?} → {}", r.display);
                }
            }
        }
    }
    println!("{lines} lines, {cards} cards, worst {worst} µs");
    i32::from(cards > 0)
}
