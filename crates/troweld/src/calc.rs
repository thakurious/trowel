//! Inline calculator (D24, `LAUNCHER-PLAN.md` L2c): `fend-core` in-process, offline.
//!
//! The rules below were found in the L0.7 spike (PROGRESS §Verified facts) and each fixes a real
//! false positive or stall. False positives are the main UX risk: typing `firefox` or
//! `2024 report` must never show a card.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use trowel_proto::{CalcAlt, CalcKind, CalcResult};

/// fend is interrupted after this, and the caller never waits longer.
pub const DEADLINE: Duration = Duration::from_millis(20);
/// Significant digits shown for a fractional result. `copy` keeps fend's full value.
const DISPLAY_DIGITS: usize = 16;
const THIN_SPACE: char = '\u{2009}';
/// Longer input is not a calculation. Also bounds fend's recursion: its parser overflows a 2 MiB
/// stack at ~300 nested parentheses and aborts the whole daemon (found in L2c, not catchable).
pub const MAX_INPUT: usize = 256;
/// Bracket depth allowed; real expressions stay far below.
pub const MAX_DEPTH: usize = 32;
/// fend runs on its own thread with 8× the stack the worst allowed input needs.
const CALC_STACK: usize = 16 * 1024 * 1024;

/// Constants and functions that make a digit-free input worth evaluating (`sqrt(pi)`), and that
/// are not mistaken for an ID prefix (`pi-3`).
const NAMES: &[&str] = &[
    "pi", "π", "e", "tau", "τ", "phi", "sqrt", "cbrt", "sin", "cos", "tan", "asin", "acos", "atan",
    "sinh", "cosh", "tanh", "asinh", "acosh", "atanh", "ln", "log", "log2", "log10", "exp", "abs",
    "floor", "ceil", "round",
];
const FUNCTIONS_START: usize = 6;
const CONVERSION_WORDS: &[&str] = &["to", "in", "as"];
const OPERATORS: &[char] = &['+', '-', '*', '×', '/', '÷', '^', '%'];
const BASE_TARGETS: &[&str] = &[
    "binary",
    "bin",
    "hex",
    "hexadecimal",
    "octal",
    "oct",
    "base",
];
const TIME_UNITS: &[&str] = &[
    "ns", "µs", "us", "ms", "s", "sec", "secs", "second", "seconds", "min", "mins", "minute",
    "minutes", "h", "hr", "hrs", "hour", "hours", "day", "days", "week", "weeks", "month",
    "months", "year", "years",
];

/// What `calc` answers. `result: None` is "not a calculation" (no card).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Outcome {
    pub result: Option<CalcResult>,
    /// Only with `force`.
    pub error: Option<String>,
    pub took_us: u64,
}

/// Rule 1 plus the reject rules and rewrites. Returns the text to hand to fend, or `None` when
/// the input is not a calculation. `force` (the `=` view) skips rule 1 only.
pub fn gate(text: &str, force: bool) -> Option<String> {
    let input = text.trim();
    let input = if force {
        input.trim_start_matches('=').trim()
    } else {
        input
    };
    if input.is_empty() || input.contains('\n') || input.chars().count() > MAX_INPUT {
        return None;
    }
    if nesting(input) > MAX_DEPTH {
        return None;
    }
    // URLs evaluate to lambdas; paths are never sums.
    if input.contains("://") || input.starts_with('/') || input.starts_with('~') {
        return None;
    }
    // `key:2026-06-30` evaluates as subtraction.
    if has_iso_date(input) {
        return None;
    }
    let words = words(input);
    if words.iter().any(|w| is_id_token(w)) {
        return None;
    }
    // fend computes dice distributions without checking the interrupt (`a-5d66-4b5c`: 67 ms).
    if words.iter().any(|w| is_dice(w)) {
        return None;
    }
    let has_digit = input.chars().any(|c| c.is_ascii_digit());
    if !has_digit
        && !words
            .iter()
            .any(|w| NAMES.contains(&w.to_lowercase().as_str()))
    {
        return None;
    }
    let base_literal = is_base_literal(input);
    if !force && !base_literal && !looks_like_math(input, &words) {
        return None;
    }
    if base_literal {
        // Otherwise fend echoes it in the same base and the preview filter hides it.
        // fend reads `0x`, not `0X`.
        return Some(format!(
            "{}{} to decimal",
            input[..2].to_ascii_lowercase(),
            &input[2..]
        ));
    }
    Some(rewrite_percent(input))
}

fn nesting(input: &str) -> usize {
    let (mut depth, mut max) = (0usize, 0usize);
    for c in input.chars() {
        match c {
            '(' | '[' | '{' => {
                depth += 1;
                max = max.max(depth);
            }
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    max
}

/// Alphanumeric runs (plus `_` and `.`), the units the reject rules look at.
fn words(input: &str) -> Vec<&str> {
    input
        .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.' || c == '-'))
        .filter(|w| !w.is_empty())
        .collect()
}

fn has_iso_date(input: &str) -> bool {
    let b = input.as_bytes();
    let d = |i: usize| b.get(i).is_some_and(u8::is_ascii_digit);
    (0..b.len()).any(|i| {
        (i == 0 || !b[i - 1].is_ascii_digit())
            && (0..4).all(|k| d(i + k))
            && b.get(i + 4) == Some(&b'-')
            && d(i + 5)
            && d(i + 6)
            && b.get(i + 7) == Some(&b'-')
            && d(i + 8)
            && d(i + 9)
    })
}

/// `M-417`, `covid-19`, `a-5d66-4b5c`: letters, `-`, then more, with no spaces, where the left
/// part is not a number (`1e3-5` still evaluates) or a known constant (`pi-3`).
fn is_id_token(word: &str) -> bool {
    // `1e-5` is a number, not `1e` minus 5.
    if word.parse::<f64>().is_ok() {
        return false;
    }
    let Some((left, right)) = word.split_once('-') else {
        return false;
    };
    !left.is_empty()
        && left.chars().all(char::is_alphanumeric)
        && left.chars().any(char::is_alphabetic)
        && !is_numeric_literal(left)
        && !NAMES.contains(&left.to_lowercase().as_str())
        && right.chars().next().is_some_and(char::is_alphanumeric)
}

fn is_numeric_literal(text: &str) -> bool {
    is_base_literal(text) || text.parse::<f64>().is_ok()
}

/// `5d66`, `d6`: digits, `d`, digit at the start of a word.
fn is_dice(word: &str) -> bool {
    word.split('-').any(|part| {
        let rest = part.trim_start_matches(|c: char| c.is_ascii_digit());
        rest.strip_prefix('d')
            .and_then(|r| r.chars().next())
            .is_some_and(|c| c.is_ascii_digit())
    })
}

fn is_base_literal(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    let (digits, ok): (&str, fn(char) -> bool) = if let Some(d) = lower.strip_prefix("0x") {
        (d, |c| c.is_ascii_hexdigit())
    } else if let Some(d) = lower.strip_prefix("0b") {
        (d, |c| c == '0' || c == '1')
    } else if let Some(d) = lower.strip_prefix("0o") {
        (d, |c| ('0'..='7').contains(&c))
    } else {
        return false;
    };
    !digits.is_empty() && digits.chars().all(ok)
}

/// Rule 1: an operator, a function call, or a conversion word.
fn looks_like_math(input: &str, words: &[&str]) -> bool {
    if input.contains(OPERATORS) {
        return true;
    }
    let lower = input.to_lowercase();
    if NAMES[FUNCTIONS_START..].iter().any(|f| {
        lower.match_indices(f).any(|(at, _)| {
            let before_ok = !lower[..at]
                .chars()
                .next_back()
                .is_some_and(char::is_alphanumeric);
            let after = lower[at + f.len()..].trim_start();
            before_ok && (after.starts_with('(') || after.starts_with(|c: char| c.is_ascii_digit()))
        })
    }) {
        return true;
    }
    words
        .iter()
        .any(|w| CONVERSION_WORDS.contains(&w.to_lowercase().as_str()))
}

/// Raycast percent: fend reads `200 + 15%` as `200.15`, so a trailing `A ± B%` becomes
/// `(A) * (1 ± B/100)` = 230. `15% of 80` already works natively.
fn rewrite_percent(input: &str) -> String {
    let Some(body) = input.strip_suffix('%') else {
        return input.to_string();
    };
    let body = body.trim_end();
    let number_start = body
        .char_indices()
        .rev()
        .find(|(_, c)| !(c.is_ascii_digit() || *c == '.'))
        .map_or(0, |(i, c)| i + c.len_utf8());
    let number = &body[number_start..];
    if number.is_empty() || number.parse::<f64>().is_err() {
        return input.to_string();
    }
    let before = body[..number_start].trim_end();
    let Some(sign) = before
        .chars()
        .next_back()
        .filter(|c| *c == '+' || *c == '-')
    else {
        return input.to_string();
    };
    let left = before[..before.len() - 1].trim_end();
    let dangling = left.ends_with(|c: char| c != '%' && OPERATORS.contains(&c));
    if left.is_empty() || dangling || left.ends_with('(') {
        return input.to_string();
    }
    // `100 + 10% + 10%` = 121: the left side may end in a percent step too.
    format!("({}) * (1 {sign} {number}/100)", rewrite_percent(left))
}

/// `*` → ` × `, `/` → ` ÷ `, thin-space grouping of long integer literals.
pub fn pretty_expr(input: &str) -> String {
    let mut out = String::with_capacity(input.len() + 8);
    let chars: Vec<char> = input.trim().chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let prev = i.checked_sub(1).map(|p| chars[p]);
        if c.is_ascii_digit() && !prev.is_some_and(|p| p.is_alphanumeric() || p == '.') {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            let run: String = chars[start..i].iter().collect();
            // `0x…`, `1e10`: not a decimal integer to group.
            let next = chars.get(i).copied();
            if next.is_some_and(|n| n.is_alphabetic()) {
                out.push_str(&run);
            } else {
                out.push_str(&group(&run, THIN_SPACE, 5));
            }
            continue;
        }
        let symbol = match c {
            '*' | '×' => Some('×'),
            '/' | '÷' => Some('÷'),
            _ => None,
        };
        match symbol {
            Some(sym) => {
                // `1920*1.25` → `1920 × 1.25`: one space each side.
                while out.ends_with(' ') {
                    out.pop();
                }
                out.push(' ');
                out.push(sym);
                out.push(' ');
                i += 1;
                while chars.get(i) == Some(&' ') {
                    i += 1;
                }
            }
            None => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

/// Groups by thousands when the run has at least `min` digits.
fn group(digits: &str, sep: char, min: usize) -> String {
    if digits.len() < min || sep == '\0' {
        return digits.to_string();
    }
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(sep);
        }
        out.push(c);
    }
    out
}

/// Separators for `display`, from `LC_ALL` / `LC_NUMERIC` / `LANG`. Input always takes `.`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NumberStyle {
    pub decimal: char,
    pub group: char,
}

impl NumberStyle {
    pub const EN: Self = Self {
        decimal: '.',
        group: ',',
    };

    pub fn from_env() -> Self {
        let locale = ["LC_ALL", "LC_NUMERIC", "LANG"]
            .iter()
            .filter_map(|k| std::env::var(k).ok())
            .find(|v| !v.is_empty())
            .unwrap_or_default();
        Self::for_locale(&locale)
    }

    /// glibc's `LC_NUMERIC` for the common languages; anything else reads like English.
    pub fn for_locale(locale: &str) -> Self {
        let lang = locale
            .split(['_', '.', '@'])
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        let dot_group = [
            "de", "es", "it", "nl", "pt", "tr", "da", "id", "el", "ro", "hr", "sl", "sr", "vi",
        ];
        let space_group = [
            "fr", "ru", "pl", "sv", "nb", "nn", "no", "fi", "cs", "sk", "uk", "bg", "hu", "et",
            "lt", "lv",
        ];
        if dot_group.contains(&lang.as_str()) {
            Self {
                decimal: ',',
                group: '.',
            }
        } else if space_group.contains(&lang.as_str()) {
            Self {
                decimal: ',',
                group: '\u{202F}',
            }
        } else {
            Self::EN
        }
    }
}

/// Leading number of fend's output: `-?digits(.digits)?(e±digits)?`, then a space or the end.
fn split_number(text: &str) -> Option<(&str, &str)> {
    let b = text.as_bytes();
    let mut i = 0;
    if b.first() == Some(&b'-') {
        i += 1;
    }
    let int_start = i;
    while b.get(i).is_some_and(u8::is_ascii_digit) {
        i += 1;
    }
    if i == int_start {
        return None;
    }
    if b.get(i) == Some(&b'.') && b.get(i + 1).is_some_and(u8::is_ascii_digit) {
        i += 1;
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
    }
    if matches!(b.get(i), Some(b'e' | b'E')) {
        let mut j = i + 1;
        if matches!(b.get(j), Some(b'+' | b'-')) {
            j += 1;
        }
        if b.get(j).is_some_and(u8::is_ascii_digit) {
            while b.get(j).is_some_and(u8::is_ascii_digit) {
                j += 1;
            }
            i = j;
        }
    }
    let rest = &text[i..];
    if !rest.is_empty() && !rest.starts_with(' ') {
        return None;
    }
    Some((&text[..i], rest.trim_start()))
}

/// fend pads approximations to 10 places (`314.1592653590`); the value is the same without.
fn trim_fraction(num: &str) -> &str {
    if !num.contains('.') || num.contains(['e', 'E']) {
        return num;
    }
    num.trim_end_matches('0').trim_end_matches('.')
}

/// The number as the user sees it: at most 16 significant digits when it has a fraction, and
/// grouped integer digits (`2,760`). Integers are never rounded.
fn display_number(num: &str, style: NumberStyle, grouped: bool) -> String {
    let (mantissa, exp) = match num.find(['e', 'E']) {
        Some(at) => (&num[..at], Some(&num[at..])),
        None => (num, None),
    };
    let (sign, unsigned) = match mantissa.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", mantissa),
    };
    let (int, frac) = match unsigned.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (unsigned, None),
    };
    let mut frac = frac.map(str::to_string);
    if let Some(f) = frac.as_mut() {
        let int_sig = int.trim_start_matches('0').len();
        let keep = DISPLAY_DIGITS.saturating_sub(int_sig);
        if f.len() > keep {
            // Round half up on the decimal string; carry into the integer part is rare enough
            // (…999 past 16 digits) that we fall back to f64 for it.
            let rounded = round_fraction(int, f, keep);
            return format_parts(sign, &rounded.0, rounded.1.as_deref(), exp, style, grouped);
        }
    }
    format_parts(sign, int, frac.as_deref(), exp, style, grouped)
}

fn round_fraction(int: &str, frac: &str, keep: usize) -> (String, Option<String>) {
    let mut digits: Vec<u8> = int.bytes().chain(frac.bytes().take(keep)).collect();
    let round_up = frac.as_bytes().get(keep).is_some_and(|d| *d >= b'5');
    if round_up {
        let mut i = digits.len();
        loop {
            if i == 0 {
                digits.insert(0, b'1');
                break;
            }
            i -= 1;
            if digits[i] == b'9' {
                digits[i] = b'0';
            } else {
                digits[i] += 1;
                break;
            }
        }
    }
    let int_len = digits.len() - keep;
    let int_part = String::from_utf8_lossy(&digits[..int_len]).into_owned();
    let frac_part = String::from_utf8_lossy(&digits[int_len..])
        .trim_end_matches('0')
        .to_string();
    (int_part, (!frac_part.is_empty()).then_some(frac_part))
}

fn format_parts(
    sign: &str,
    int: &str,
    frac: Option<&str>,
    exp: Option<&str>,
    style: NumberStyle,
    grouped: bool,
) -> String {
    let mut out = String::from(sign);
    if grouped {
        out.push_str(&group(int, style.group, 4));
    } else {
        out.push_str(int);
    }
    if let Some(f) = frac {
        out.push(style.decimal);
        out.push_str(f);
    }
    if let Some(e) = exp {
        out.push_str(e);
    }
    out
}

fn base_target(input: &str) -> bool {
    let lower = input.to_lowercase();
    let mut words = lower.split_whitespace().peekable();
    while let Some(w) = words.next() {
        if CONVERSION_WORDS.contains(&w) {
            if let Some(next) = words.peek() {
                if BASE_TARGETS.contains(next) {
                    return true;
                }
            }
        }
    }
    false
}

/// Turns fend's main result into a card, or `None` when it is not worth one.
pub fn shape(
    original: &str,
    evaluated: &str,
    output: &str,
    is_date: bool,
    style: NumberStyle,
) -> Option<CalcResult> {
    let output = output.trim();
    // Function values (`\x.…`) and anything multi-line are not results.
    if output.is_empty() || output.contains('\\') || output.contains(|c: char| c < ' ') {
        return None;
    }
    let (approx, value) = match output.strip_prefix("approx. ") {
        Some(rest) => (true, rest),
        None => (false, output),
    };
    let expr = original.trim().trim_start_matches('=').trim().to_string();
    let base_literal = is_base_literal(&expr);
    let base = base_literal || base_target(evaluated);
    let parsed = split_number(value);
    let (copy, display, unit, kind) = match parsed {
        // Mixed units (`1 ft 3 in`) and anything else non-numeric copy as shown; `m^2` is one unit.
        Some((num, unit))
            if !unit
                .split_whitespace()
                .any(|w| w.starts_with(|c: char| c.is_ascii_digit())) =>
        {
            let num = trim_fraction(num);
            let unit = (!unit.is_empty()).then(|| unit.to_string());
            let kind = if is_date {
                CalcKind::Date
            } else if base {
                CalcKind::Base
            } else if let Some(u) = unit.as_deref() {
                if TIME_UNITS.contains(&u) {
                    CalcKind::Duration
                } else {
                    CalcKind::Unit
                }
            } else {
                CalcKind::Number
            };
            let number = display_number(num, style, !base || base_literal);
            let display = match unit.as_deref() {
                Some(u) => format!("{number} {u}"),
                None => number,
            };
            (num.to_string(), display, unit, kind)
        }
        _ => {
            let kind = if is_date {
                CalcKind::Date
            } else if base {
                CalcKind::Base
            } else {
                CalcKind::Number
            };
            (value.to_string(), value.to_string(), None, kind)
        }
    };
    let mut alts = Vec::new();
    if unit.is_none() && !base_target(evaluated) {
        if let Ok(n) = copy.parse::<u64>() {
            alts.push(CalcAlt {
                label: "Hex".into(),
                copy: format!("{n:#x}"),
            });
            alts.push(CalcAlt {
                label: "Binary".into(),
                copy: format!("{n:#b}"),
            });
        }
    }
    Some(CalcResult {
        pretty_expr: pretty_expr(&expr),
        expr,
        display,
        copy,
        kind,
        unit,
        approx,
        alts,
    })
}

struct Deadline(Instant);

impl fend_core::Interrupt for Deadline {
    fn should_interrupt(&self) -> bool {
        Instant::now() >= self.0
    }
}

struct Job {
    original: String,
    evaluated: String,
    force: bool,
    deadline: Instant,
    reply: Sender<Outcome>,
}

/// Owns the fend `Context` on a dedicated thread, so an evaluation that ignores the interrupt
/// (dice) is abandoned instead of blocking a connection. Preview mode works on a context clone,
/// so abandoning one is safe.
pub struct Calculator {
    jobs: Mutex<Sender<Job>>,
    busy: Arc<AtomicBool>,
    style: NumberStyle,
    /// How long fend's first evaluation took at startup (unit tables are built lazily).
    pub warmup: Duration,
}

impl Default for Calculator {
    fn default() -> Self {
        Self::new(NumberStyle::from_env())
    }
}

impl Calculator {
    pub fn new(style: NumberStyle) -> Self {
        let (tx, rx) = mpsc::channel::<Job>();
        let busy = Arc::new(AtomicBool::new(false));
        let worker_busy = Arc::clone(&busy);
        let (ready_tx, ready) = mpsc::channel();
        thread::Builder::new()
            .name("troweld-calc".into())
            .stack_size(CALC_STACK)
            .spawn(move || worker(rx, worker_busy, style, ready_tx))
            .expect("spawn calc thread");
        // The first unit conversion costs ~1 ms; pay it here, not on the first keystroke.
        let warmup = ready
            .recv_timeout(Duration::from_millis(500))
            .unwrap_or(Duration::ZERO);
        Self {
            jobs: Mutex::new(tx),
            busy,
            style,
            warmup,
        }
    }

    pub fn style(&self) -> NumberStyle {
        self.style
    }

    /// Never waits longer than [`DEADLINE`].
    pub fn eval(&self, text: &str, force: bool) -> Outcome {
        let started = Instant::now();
        let took = |o: Outcome| Outcome {
            took_us: started.elapsed().as_micros() as u64,
            ..o
        };
        // The gate is pure string work, but a slip there must cost one card, not a connection.
        let gated = catch_unwind(|| gate(text, force)).unwrap_or(None);
        let Some(evaluated) = gated else {
            return took(Outcome {
                error: force.then(|| "Not a calculation".to_string()),
                ..Outcome::default()
            });
        };
        // One caller at a time; each holds the lock for at most the deadline.
        let jobs = self.jobs.lock().expect("calc jobs");
        if self.busy.load(Ordering::Acquire) {
            // An earlier evaluation overran and is still running: no card rather than a queue.
            return took(Outcome::default());
        }
        let deadline = started + DEADLINE;
        let (reply, answer) = mpsc::channel();
        self.busy.store(true, Ordering::Release);
        let job = Job {
            original: text.to_string(),
            evaluated,
            force,
            deadline,
            reply,
        };
        if jobs.send(job).is_err() {
            self.busy.store(false, Ordering::Release);
            return took(Outcome::default());
        }
        drop(jobs);
        let wait = deadline.saturating_duration_since(Instant::now()) + Duration::from_millis(1);
        match answer.recv_timeout(wait) {
            Ok(outcome) => took(outcome),
            Err(_) => took(Outcome {
                error: force.then(|| "Took too long".to_string()),
                ..Outcome::default()
            }),
        }
    }
}

fn worker(rx: Receiver<Job>, busy: Arc<AtomicBool>, style: NumberStyle, ready: Sender<Duration>) {
    let mut context = fend_core::Context::new();
    // Offline (D13/D24): no exchange-rate handler, so currencies are unknown units.
    context.disable_rng();
    let started = Instant::now();
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let int = Deadline(started + Duration::from_millis(200));
        fend_core::evaluate_preview_with_interrupt("1 ft to cm + 0x1 * 2^2", &context, &int)
    }));
    let _ = ready.send(started.elapsed());
    for job in rx {
        let outcome =
            catch_unwind(AssertUnwindSafe(|| run_job(&context, &job, style))).unwrap_or_default();
        busy.store(false, Ordering::Release);
        let _ = job.reply.send(outcome);
    }
}

fn run_job(context: &fend_core::Context, job: &Job, style: NumberStyle) -> Outcome {
    let int = Deadline(job.deadline);
    if !job.force {
        // Clones the context, no RNG, drops empty / > 50 chars / multi-line / == input.
        let result = fend_core::evaluate_preview_with_interrupt(&job.evaluated, context, &int);
        if result.get_main_result().trim() == job.original.trim() {
            return Outcome::default();
        }
        return Outcome {
            result: shape(
                &job.original,
                &job.evaluated,
                result.get_main_result(),
                is_date(&result),
                style,
            ),
            ..Outcome::default()
        };
    }
    let mut scratch = context.clone();
    match fend_core::evaluate_with_interrupt(&job.evaluated, &mut scratch, &int) {
        Ok(result) if !result.output_is_empty() => {
            let output = result.get_main_result();
            let card = (output.chars().count() <= 200)
                .then(|| {
                    shape(
                        &job.original,
                        &job.evaluated,
                        output,
                        is_date(&result),
                        style,
                    )
                })
                .flatten();
            Outcome {
                error: card.is_none().then(|| "No result to show".to_string()),
                result: card,
                ..Outcome::default()
            }
        }
        Ok(_) => Outcome {
            error: Some("No result to show".into()),
            ..Outcome::default()
        },
        Err(err) => Outcome {
            error: Some(if Instant::now() >= job.deadline {
                "Took too long".to_string()
            } else {
                err
            }),
            ..Outcome::default()
        },
    }
}

fn is_date(result: &fend_core::FendResult) -> bool {
    result
        .get_main_result_spans()
        .any(|s| s.kind() == fend_core::SpanKind::Date)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_needs_math() {
        for text in ["firefox", "2024 report", "notes", "1920", "pi", "", "   "] {
            assert_eq!(gate(text, false), None, "{text:?}");
        }
        for text in [
            "1+1",
            "5 ft to cm",
            "sqrt(16)",
            "sqrt 16",
            "15% of 80",
            "2^10",
        ] {
            assert!(gate(text, false).is_some(), "{text:?}");
        }
    }

    #[test]
    fn reject_rules_from_the_spike() {
        for text in [
            "https://x.com/a-1",
            "/usr/bin/x-1",
            "~/a+b",
            "key:2026-06-30",
            "2026-06-30",
            "M-417",
            "covid-19 cases",
            "a-5d66-4b5c",
            "5d66",
            "roll d6 + 2",
        ] {
            assert_eq!(gate(text, false), None, "{text:?}");
        }
        for text in ["1e3-5", "1e-5", "pi-3", "sqrt(16)-2", "10 - 3"] {
            assert!(gate(text, false).is_some(), "{text:?}");
        }
    }

    #[test]
    fn force_skips_rule_one_only() {
        assert_eq!(gate("= 42", true).as_deref(), Some("42"));
        assert_eq!(gate("= M-417", true), None);
    }

    #[test]
    fn rewrites() {
        assert_eq!(gate("200 + 15%", false).unwrap(), "(200) * (1 + 15/100)");
        assert_eq!(gate("80 - 25 %", false).unwrap(), "(80) * (1 - 25/100)");
        assert_eq!(gate("15% of 80", false).unwrap(), "15% of 80");
        assert_eq!(
            gate("100 + 10% + 10%", false).unwrap(),
            "((100) * (1 + 10/100)) * (1 + 10/100)"
        );
        assert_eq!(gate("0xff", false).unwrap(), "0xff to decimal");
        assert_eq!(gate("0b1010", false).unwrap(), "0b1010 to decimal");
        assert_eq!(gate("-15%", true).unwrap(), "-15%");
        // Found by the fuzz test: a multi-byte char right before the number.
        assert_eq!(gate("দ5 + 5%", false).unwrap(), "(দ5) * (1 + 5/100)");
        assert_eq!(gate("5দ5%", true).unwrap(), "5দ5%");
    }

    #[test]
    fn pretty() {
        assert_eq!(pretty_expr("1920*1.25 + 15%"), "1920 × 1.25 + 15%");
        assert_eq!(pretty_expr("1000000/3"), "1\u{2009}000\u{2009}000 ÷ 3");
        assert_eq!(
            pretty_expr("0xfffff + 12345.6789"),
            "0xfffff + 12\u{2009}345.6789"
        );
        assert_eq!(pretty_expr("1e10000"), "1e10000");
    }

    #[test]
    fn display_numbers() {
        let en = NumberStyle::EN;
        assert_eq!(display_number("2760", en, true), "2,760");
        assert_eq!(display_number("-1234567.5", en, true), "-1,234,567.5");
        assert_eq!(display_number("760", en, true), "760");
        assert_eq!(
            display_number("18446744073709551616", en, true),
            "18,446,744,073,709,551,616"
        );
        assert_eq!(
            display_number("0.12345678901234567890", en, true),
            "0.1234567890123457"
        );
        assert_eq!(display_number("9.99999999999999999", en, true), "10");
        let de = NumberStyle::for_locale("de_DE.UTF-8");
        assert_eq!(display_number("2760.5", de, true), "2.760,5");
        let fr = NumberStyle::for_locale("fr_FR.UTF-8");
        assert_eq!(display_number("2760.5", fr, true), "2\u{202F}760,5");
        assert_eq!(NumberStyle::for_locale("en_US.UTF-8"), en);
        assert_eq!(NumberStyle::for_locale("C"), en);
    }

    #[test]
    fn shapes() {
        let en = NumberStyle::EN;
        let r = shape("5 ft to cm", "5 ft to cm", "152.4 cm", false, en).unwrap();
        assert_eq!(
            (r.copy.as_str(), r.display.as_str(), r.unit.as_deref()),
            ("152.4", "152.4 cm", Some("cm"))
        );
        assert_eq!(r.kind, CalcKind::Unit);
        let r = shape("90 min to hours", "90 min to hours", "1.5 hours", false, en).unwrap();
        assert_eq!(r.kind, CalcKind::Duration);
        let r = shape("10/3", "10/3", "approx. 3.3333333333", false, en).unwrap();
        assert!(r.approx);
        assert_eq!(r.copy, "3.3333333333");
        let r = shape("0xff to binary", "0xff to binary", "11111111", false, en).unwrap();
        assert_eq!(r.kind, CalcKind::Base);
        assert_eq!(r.display, "11111111", "binary digits are not grouped");
        assert!(r.alts.is_empty());
        let r = shape("1920*1.25", "1920*1.25", "2400", false, en).unwrap();
        assert_eq!(r.alts[0].copy, "0x960");
        assert_eq!(r.alts[1].copy, "0b100101100000");
        let r = shape("x", "x", "1 ft 3 in", false, en).unwrap();
        assert_eq!(r.copy, "1 ft 3 in");
        let r = shape(
            "1 acre to m^2",
            "1 acre to m^2",
            "4046.8564224 m^2",
            false,
            en,
        )
        .unwrap();
        assert_eq!(
            (r.copy.as_str(), r.unit.as_deref()),
            ("4046.8564224", Some("m^2"))
        );
        let r = shape("pi * 100", "pi * 100", "approx. 314.1592653590", false, en).unwrap();
        assert_eq!(r.copy, "314.159265359");
        assert!(shape("x", "x", "\\x.x", false, en).is_none());
    }

    #[test]
    fn dice_shaped_inputs_never_block() {
        let calc = Calculator::new(NumberStyle::EN);
        let started = Instant::now();
        let out = calc.eval("a-5d66-4b5c", false);
        assert!(out.result.is_none());
        assert!(started.elapsed() < Duration::from_millis(25));
    }
}
