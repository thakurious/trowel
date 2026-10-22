//! Learning what the user opens (D19 as amended 2026-10-22): Raycast's frecency and search terms.
//! Ported from oma-cast (`typecraft-dev/oma-cast` at `90efedd`) `plugin/core/Frecency.js` and
//! `plugin/core/Ranker.js` `searchTermMatch`, which reproduce Raycast's machine code (oma-cast
//! `SPEC.md` §7.4). Items are `app:<desktop id>` and `cmd:<command id>`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Half-life 10 days (`Frecency.js` `LAMBDA`).
const LAMBDA: f64 = std::f64::consts::LN_2 / 10.0;
const DAY: f64 = 86_400.0;
/// `exp` overflows past this (`EXP_GUARD`).
const EXP_GUARD: f64 = 709.78;
/// Each visit adds this to the score (`VISIT_BONUS`).
const VISIT_BONUS: f64 = 100.0;
/// Search terms count only while the item was opened within 17 days (`SEARCH_TERM_WINDOW_MS`).
const TERM_WINDOW_SECS: i64 = 1_468_800;
/// The last 20 distinct queries are kept per item (`MAX_SEARCH_TERMS`).
const MAX_TERMS: usize = 20;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Term {
    pub term: String,
    /// Unix seconds.
    pub at: i64,
}

/// One item's row in `learned`.
#[derive(Debug, Clone, PartialEq)]
pub struct Learned {
    /// Unix seconds at which the score decays to 1.
    pub anchor: f64,
    /// Last visit, Unix seconds.
    pub opened_at: i64,
    /// Oldest first.
    pub terms: Vec<Term>,
}

/// `score`: ≥ 1, and exactly 1 for an item never visited (or long forgotten).
pub fn score(anchor: f64, now: i64) -> f64 {
    if !anchor.is_finite() {
        return 1.0;
    }
    (LAMBDA * (anchor - now as f64) / DAY)
        .min(EXP_GUARD)
        .exp()
        .max(1.0)
}

/// The anchor after one more visit at `now` (`visit`).
fn visit(anchor: Option<f64>, now: i64) -> f64 {
    let t = now as f64;
    let a = anchor.filter(|a| a.is_finite()).map_or(t, |a| a.max(t));
    let s = (LAMBDA * (a - t) / DAY).min(EXP_GUARD).exp().max(1.0) + VISIT_BONUS;
    if s.is_nan() || s < 1.0 {
        return t;
    }
    t + DAY * s.min(f64::MAX).ln() / LAMBDA
}

/// The anchor whose score at `at` is `score` (seeding from the old 14-day launch sums).
pub fn anchor_for(score: f64, at: i64) -> f64 {
    at as f64 + DAY * score.max(1.0).ln() / LAMBDA
}

/// `recordVisit`: bump the score and remember `query` (trimmed, lower-cased) as a search term.
pub fn record(prior: Option<&Learned>, now: i64, query: &str) -> Learned {
    let mut terms = prior.map(|p| p.terms.clone()).unwrap_or_default();
    let term = query.trim().to_lowercase();
    if !term.is_empty() {
        terms.retain(|t| !t.term.is_empty() && t.term != term);
        terms.push(Term { term, at: now });
        if terms.len() > MAX_TERMS {
            terms.drain(..terms.len() - MAX_TERMS);
        }
    }
    Learned {
        anchor: visit(prior.map(|p| p.anchor), now),
        opened_at: now,
        terms,
    }
}

/// How the typed query relates to a remembered search term, weakest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TermMatch {
    #[default]
    None,
    /// A remembered term plus 1–3 more typed characters (`count`).
    Overbounds(u8),
    /// The query is a prefix of a remembered term.
    Prefix,
    Exact,
}

impl TermMatch {
    fn class(self) -> u8 {
        match self {
            Self::None => 0,
            Self::Overbounds(_) => 1,
            Self::Prefix => 2,
            Self::Exact => 3,
        }
    }

    /// `searchTermRank`: class, then the overbounds count.
    pub fn rank(self) -> u8 {
        self.class() * 10 + self.overbounds()
    }

    pub fn overbounds(self) -> u8 {
        match self {
            Self::Overbounds(n) => n,
            _ => 0,
        }
    }

    pub fn as_str(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::Overbounds(_) => Some("overbounds"),
            Self::Prefix => Some("prefix"),
            Self::Exact => Some("exact"),
        }
    }
}

/// `searchTermMatch`: the best match of `query` against the item's terms. Only items with a
/// score above 1 that were opened within the window qualify.
pub fn term_match(l: &Learned, query: &str, now: i64) -> TermMatch {
    if l.terms.is_empty() || score(l.anchor, now) <= 1.0 || now - l.opened_at > TERM_WINDOW_SECS {
        return TermMatch::None;
    }
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return TermMatch::None;
    }
    let qlen = q.chars().count();
    let mut best = TermMatch::None;
    for t in &l.terms {
        let s = t.term.as_str();
        if s.is_empty() {
            continue;
        }
        let slen = s.chars().count();
        let m = if s == q {
            TermMatch::Exact
        } else if slen > qlen && s.starts_with(&q) {
            TermMatch::Prefix
        } else if q.starts_with(s) && (1..=3).contains(&(qlen - slen)) {
            TermMatch::Overbounds((qlen - slen) as u8)
        } else {
            TermMatch::None
        };
        if m.rank() > best.rank() {
            best = m;
        }
    }
    best
}

/// What ranking needs from an item for one query.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Signal {
    /// `score` now; 1 when never opened.
    pub frecency: f64,
    pub term: TermMatch,
}

impl Signal {
    pub const NONE: Self = Self {
        frecency: 1.0,
        term: TermMatch::None,
    };

    pub fn of(l: Option<&Learned>, query: &str, now: i64) -> Self {
        l.map_or(Self::NONE, |l| Self {
            frecency: score(l.anchor, now),
            term: term_match(l, query, now),
        })
    }

    /// The wire form; `None` when there is nothing to tell (never opened, or forgotten).
    pub fn to_wire(self) -> Option<trowel_proto::LearnedSignal> {
        (self.frecency > 1.0 || self.term != TermMatch::None).then(|| trowel_proto::LearnedSignal {
            frecency: self.frecency,
            term: self.term.as_str().map(str::to_string),
        })
    }
}

/// `apps_query`'s `commands`: the requested ids that have something learned, in request order.
pub fn command_signals(
    ids: &[String],
    learned: &HashMap<String, Learned>,
    query: &str,
    now: i64,
) -> Vec<trowel_proto::CommandSignal> {
    ids.iter()
        .filter_map(|id| {
            let signal = Signal::of(learned.get(&format!("cmd:{id}")), query, now).to_wire()?;
            Some(trowel_proto::CommandSignal {
                id: id.clone(),
                signal,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: i64 = 1_800_000_000;

    // oma-cast `tests/frecency.test.js`: a first visit scores 101, decays by half in 10 days.
    #[test]
    fn visit_adds_one_hundred_and_halves_in_ten_days() {
        let l = record(None, T0, "");
        assert!((score(l.anchor, T0) - 101.0).abs() < 1e-6);
        assert!((score(l.anchor, T0 + 10 * 86_400) - 50.5).abs() < 1e-6);
        let l2 = record(Some(&l), T0, "");
        assert!((score(l2.anchor, T0) - 201.0).abs() < 1e-6);
        // Each visit adds 100 to the decayed score.
        let later = record(Some(&l), T0 + 10 * 86_400, "");
        assert!((score(later.anchor, T0 + 10 * 86_400) - 150.5).abs() < 1e-6);
        // Never below 1, even long after.
        assert_eq!(score(l.anchor, T0 + 3650 * 86_400), 1.0);
        assert_eq!(score(f64::NAN, T0), 1.0);
        assert!((score(anchor_for(42.0, T0), T0) - 42.0).abs() < 1e-6);
    }

    #[test]
    fn terms_are_deduped_lowercased_and_capped() {
        let mut l = record(None, T0, " Chr ");
        l = record(Some(&l), T0 + 1, "ch");
        l = record(Some(&l), T0 + 2, "CHR");
        let terms: Vec<&str> = l.terms.iter().map(|t| t.term.as_str()).collect();
        assert_eq!(terms, ["ch", "chr"]);
        for i in 0..30 {
            l = record(Some(&l), T0 + 3 + i, &format!("q{i}"));
        }
        assert_eq!(l.terms.len(), MAX_TERMS);
        assert_eq!(l.terms.last().unwrap().term, "q29");
    }

    #[test]
    fn term_matches() {
        let l = record(None, T0, "chr");
        assert_eq!(term_match(&l, "chr", T0), TermMatch::Exact);
        assert_eq!(term_match(&l, "CH", T0), TermMatch::Prefix);
        assert_eq!(term_match(&l, "chrom", T0), TermMatch::Overbounds(2));
        assert_eq!(term_match(&l, "chromium", T0), TermMatch::None);
        assert_eq!(term_match(&l, "x", T0), TermMatch::None);
        assert_eq!(term_match(&l, "", T0), TermMatch::None);
        // Forgotten 17 days after the last open.
        assert_eq!(
            term_match(&l, "chr", T0 + TERM_WINDOW_SECS),
            TermMatch::Exact
        );
        assert_eq!(
            term_match(&l, "chr", T0 + TERM_WINDOW_SECS + 1),
            TermMatch::None
        );
        assert!(TermMatch::Exact.rank() > TermMatch::Prefix.rank());
        assert!(TermMatch::Prefix.rank() > TermMatch::Overbounds(3).rank());
    }
}
