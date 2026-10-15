//! L2c gates (`LAUNCHER-PLAN.md`): the expression table, the false-positive corpus, the deadline,
//! and inputs that must never crash the daemon. The tables live in `tests/calc/` and are shared
//! with `trowel calc --bench / --corpus`.

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use troweld::calc::{Calculator, NumberStyle, MAX_DEPTH, MAX_INPUT};

fn data(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/calc")
        .join(name);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn lines(name: &str) -> Vec<String> {
    data(name)
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with("# "))
        .map(str::to_string)
        .collect()
}

#[test]
fn expression_table() {
    let calc = Calculator::new(NumberStyle::EN);
    let rows = lines("table.tsv");
    assert!(rows.len() >= 140, "about 150 rows, got {}", rows.len());
    let mut wrong = Vec::new();
    for row in &rows {
        let (expr, want) = row.split_once('\t').expect("expr<TAB>expected");
        let want = Some(want.trim()).filter(|w| *w != "-");
        let out = calc.eval(expr, false);
        let got = out.result.as_ref().map(|r| r.copy.as_str());
        if got != want {
            wrong.push(format!("{expr:?}: got {got:?}, want {want:?}"));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn false_positive_corpus_shows_no_cards() {
    let calc = Calculator::new(NumberStyle::EN);
    let cards: Vec<String> = lines("false-positives.txt")
        .iter()
        .filter_map(|line| {
            calc.eval(line, false)
                .result
                .map(|r| format!("{line:?} → {}", r.display))
        })
        .collect();
    assert!(cards.is_empty(), "cards: {cards:#?}");
}

#[test]
fn slow_inputs_return_within_the_deadline() {
    let calc = Calculator::new(NumberStyle::EN);
    let nested = format!("{}1+1{}", "(".repeat(MAX_DEPTH), ")".repeat(MAX_DEPTH));
    for text in ["10^10^10", "2^2^2^2^2^2", "a-5d66-4b5c", nested.as_str()] {
        let started = Instant::now();
        let out = calc.eval(text, false);
        let took = started.elapsed();
        assert!(took < Duration::from_millis(25), "{text:?} took {took:?}");
        if text != nested {
            assert!(out.result.is_none(), "{text:?} should not show a card");
        }
    }
    // The deepest allowed nesting still evaluates.
    assert_eq!(calc.eval(&nested, false).result.unwrap().copy, "2");
}

/// Every recursive shape that fits the input cap, at every depth. A stack overflow aborts the
/// test binary, so passing means the guard holds.
#[test]
fn deep_input_never_crashes() {
    let calc = Calculator::new(NumberStyle::EN);
    let shapes: [(&str, &str, &str); 7] = [
        ("(", "1", ")"),
        ("[", "1", "]"),
        ("-", "1", ""),
        ("2^", "1", ""),
        ("sqrt(", "4", ")"),
        ("1+(", "1", ")"),
        ("-(", "1", ")"),
    ];
    for (open, mid, close) in shapes {
        for depth in [MAX_DEPTH, MAX_DEPTH + 1, 64, 128, 300, 1000] {
            let text = format!("{}{mid}{}", open.repeat(depth), close.repeat(depth));
            let _ = calc.eval(&text, false);
            let _ = calc.eval(&text, true);
            let capped: String = text.chars().take(MAX_INPUT).collect();
            let _ = calc.eval(&capped, false);
            let _ = calc.eval(&capped, true);
        }
    }
}

/// Random strings never panic or block. xorshift so failures replay.
#[test]
fn fuzz_random_strings() {
    let calc = Calculator::new(NumberStyle::EN);
    let alphabet: Vec<char> =
        "0123456789+-*/^%()., xXbBoOeEdtoinasqrtpi:=#~/×÷°CFkmgbB\u{9a6}\u{627}"
            .chars()
            .collect();
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let mut worst = Duration::ZERO;
    // `TROWEL_CALC_FUZZ=200000` for a long run; CI keeps it short.
    let rounds: usize = std::env::var("TROWEL_CALC_FUZZ")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(3000);
    for _ in 0..rounds {
        let len = (next() % 40) as usize;
        let text: String = (0..len)
            .map(|_| alphabet[(next() % alphabet.len() as u64) as usize])
            .collect();
        let started = Instant::now();
        let _ = calc.eval(&text, next() % 4 == 0);
        worst = worst.max(started.elapsed());
    }
    assert!(worst < Duration::from_millis(25), "worst {worst:?}");
}

#[test]
fn five_feet_in_centimetres() {
    let calc = Calculator::new(NumberStyle::EN);
    let r = calc.eval("5 ft to cm", false).result.unwrap();
    assert_eq!((r.display.as_str(), r.copy.as_str()), ("152.4 cm", "152.4"));
}
