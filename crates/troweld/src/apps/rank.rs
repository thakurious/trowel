//! Application ranking: a port of Omarchy's `AppSearch.js` (D19). Cites
//! `/usr/share/omarchy/shell/services/AppSearch.js:<line>`. JS `indexOf` / `.length` count UTF-16
//! code units and `<` compares UTF-16, so this does too: scores must match to the unit for the
//! parity gate (`scripts/apps-parity.sh`).
//!
//! Frecency is Trowel's addition. It reorders only (a) name-prefix matches among themselves,
//! (b) exact score ties, and (c) the empty-query list. The sort key is lexicographic on a tier
//! that is a monotonic function of the score, so with no launches recorded the order is exactly
//! Omarchy's, and a typed name prefix always beats an id or substring match.

use std::cmp::Ordering;

/// The fields `AppSearch.js` reads from a Quickshell `DesktopEntry`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Rankable {
    pub id: String,
    pub name: String,
    pub generic_name: String,
    pub comment: String,
    pub keywords: Vec<String>,
}

fn utf16_len(s: &str) -> i64 {
    s.encode_utf16().count() as i64
}

/// `String.prototype.indexOf`, in UTF-16 units. An empty needle is found at 0.
fn index_of(hay: &str, needle: &str) -> Option<i64> {
    hay.find(needle).map(|byte| utf16_len(&hay[..byte]))
}

fn cmp_utf16(a: &str, b: &str) -> Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

/// `entryName` (:1): name, else id.
fn entry_name(e: &Rankable) -> &str {
    if e.name.is_empty() {
        &e.id
    } else {
        &e.name
    }
}

/// `keywordText` (:13).
fn keyword_text(e: &Rankable) -> String {
    e.keywords.join(" ")
}

/// `entrySearchText` (:21).
fn search_text(e: &Rankable) -> String {
    [
        e.name.as_str(),
        &e.generic_name,
        &e.comment,
        &keyword_text(e),
        &e.id,
    ]
    .join(" ")
    .to_lowercase()
}

/// `wordText` (:26): split camelCase (`/([a-z0-9])([A-Z])/g`, non-overlapping), then
/// `/[._:/\\-]+/g` → space, then lower-case.
fn word_text(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    let mut spaced = String::with_capacity(value.len() + 4);
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if (c.is_ascii_lowercase() || c.is_ascii_digit())
            && chars.get(i + 1).is_some_and(char::is_ascii_uppercase)
        {
            spaced.push(c);
            spaced.push(' ');
            spaced.push(chars[i + 1]);
            i += 2;
            continue;
        }
        spaced.push(c);
        i += 1;
    }
    let mut out = String::with_capacity(spaced.len());
    let mut in_run = false;
    for c in spaced.chars() {
        if matches!(c, '.' | '_' | ':' | '/' | '\\' | '-') {
            if !in_run {
                out.push(' ');
            }
            in_run = true;
        } else {
            out.push(c);
            in_run = false;
        }
    }
    out.to_lowercase()
}

/// `words` (:33): runs of `[a-z0-9]` after `wordText`.
fn words(value: &str) -> Vec<String> {
    word_text(value)
        .split(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit()))
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

/// `entryAcronym` (:42).
fn acronym(e: &Rankable) -> String {
    let joined = [e.name.as_str(), &e.generic_name, &keyword_text(e), &e.id].join(" ");
    words(&joined)
        .iter()
        .filter_map(|w| w.chars().next())
        .collect()
}

/// `termMatches` (:49).
fn term_matches(e: &Rankable, term: &str, hay: &str, acro: &str) -> bool {
    if term.is_empty() {
        return true;
    }
    let name = entry_name(e).to_lowercase();
    let id = e.id.to_lowercase();
    name.contains(term)
        || id.contains(term)
        || hay.contains(term)
        || (utf16_len(term) <= 5 && acro.contains(term))
}

/// `allTermsMatch` (:63): every whitespace-separated term (JS `\s`) matches.
fn all_terms_match(e: &Rankable, query: &str, hay: &str, acro: &str) -> bool {
    query
        .to_lowercase()
        .trim()
        .split(char::is_whitespace)
        .all(|t| t.is_empty() || term_matches(e, t, hay, acro))
}

/// Which `fuzzyScore` branch matched, best first (for the UI's "strong match" choice). Not a sort
/// key: bands are not monotonic in score (a late haystack hit can score below an acronym).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Band {
    Words = 0,
    AcronymInside = 1,
    AcronymStart = 2,
    Haystack = 3,
    IdInside = 4,
    NameInside = 5,
    IdStart = 6,
    NameStart = 7,
}

/// `fuzzyScore` (:71). `None` is JS `-1` (no match). An empty query is `Some((0, Words))`.
pub fn fuzzy_score(e: &Rankable, query: &str) -> Option<(i64, Band)> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Some((0, Band::Words));
    }
    let hay = search_text(e);
    let acro = acronym(e);
    if !all_terms_match(e, &q, &hay, &acro) {
        return None;
    }
    let name = entry_name(e).to_lowercase();
    let id = e.id.to_lowercase();
    let direct_name = index_of(&name, &q);
    let direct_id = index_of(&id, &q);
    if direct_name == Some(0) {
        return Some((10000 - utf16_len(&name), Band::NameStart));
    }
    if direct_id == Some(0) {
        return Some((9500 - utf16_len(&id), Band::IdStart));
    }
    if let Some(at) = direct_name {
        return Some((8000 - at * 10 - utf16_len(&name), Band::NameInside));
    }
    if let Some(at) = direct_id {
        return Some((7600 - at * 10 - utf16_len(&id), Band::IdInside));
    }
    if let Some(at) = index_of(&hay, &q) {
        return Some((6000 - at, Band::Haystack));
    }
    match index_of(&acro, &q) {
        Some(0) => Some((5000 - utf16_len(&acro), Band::AcronymStart)),
        Some(at) => Some((4600 - at * 10 - utf16_len(&acro), Band::AcronymInside)),
        None => Some((4000 - utf16_len(&name), Band::Words)),
    }
}

/// Name-prefix scores are `10000 − len(name)`; every other branch is ≤ 9500. A threshold on the
/// score itself keeps the tier monotonic (names over 499 UTF-16 units simply never get the tier).
const PREFIX_TIER: i64 = 9500;

pub struct Ranked {
    pub index: usize,
    pub score: i64,
    pub band: Band,
    pub frecency: f64,
}

/// `sortedEntries` (:97) over already-visible entries (hidden and `NoDisplay` filtered by the
/// caller). `frecency[i]` belongs to `entries[i]`; all zeros reproduces Omarchy's order exactly.
/// Stable, like V8's `Array.prototype.sort`, so equal keys keep input order.
pub fn sorted(entries: &[Rankable], query: &str, frecency: &[f64]) -> Vec<Ranked> {
    let q = query.trim();
    let mut rows: Vec<(Ranked, String, String)> = entries
        .iter()
        .enumerate()
        .filter(|(_, e)| !entry_name(e).is_empty())
        .filter_map(|(i, e)| {
            let (score, band) = fuzzy_score(e, q)?;
            let name = entry_name(e).to_lowercase();
            Some((
                Ranked {
                    index: i,
                    score,
                    band,
                    frecency: frecency.get(i).copied().unwrap_or(0.0),
                },
                name.clone(),
                name,
            ))
        })
        .collect();
    rows.sort_by(|(a, a_key, a_name), (b, b_key, b_name)| {
        let by_frecency = || {
            b.frecency
                .partial_cmp(&a.frecency)
                .unwrap_or(Ordering::Equal)
        };
        let ranked = if q.is_empty() {
            by_frecency()
        } else {
            let (a_tier, b_tier) = (a.score > PREFIX_TIER, b.score > PREFIX_TIER);
            b_tier.cmp(&a_tier).then_with(|| {
                if a_tier {
                    by_frecency().then(b.score.cmp(&a.score))
                } else {
                    b.score.cmp(&a.score).then_with(by_frecency)
                }
            })
        };
        ranked
            .then_with(|| cmp_utf16(a_key, b_key))
            .then_with(|| cmp_utf16(a_name, b_name))
    });
    rows.into_iter().map(|(r, _, _)| r).collect()
}

/// Highlight ranges for the panel: each query term's first occurrence in the name, as char
/// offsets `[start, end)`, merged.
pub fn name_matches(name: &str, query: &str) -> Vec<[u32; 2]> {
    let lower = name.to_lowercase();
    if lower.chars().count() != name.chars().count() {
        return Vec::new();
    }
    let mut ranges: Vec<[u32; 2]> = query
        .to_lowercase()
        .split_whitespace()
        .filter_map(|t| {
            let byte = lower.find(t)?;
            let start = lower[..byte].chars().count() as u32;
            Some([start, start + t.chars().count() as u32])
        })
        .collect();
    ranges.sort_unstable();
    let mut merged: Vec<[u32; 2]> = Vec::new();
    for r in ranges {
        match merged.last_mut() {
            Some(last) if r[0] <= last[1] => last[1] = last[1].max(r[1]),
            _ => merged.push(r),
        }
    }
    merged
}

/// Frecency: Σ over launches of `0.5^(age_days / 14)`, kept as one decayed sum
/// (`LAUNCHER-PLAN.md` L2). `score` was the sum at `last`; it decays to `now`.
pub fn decayed(score: f64, last: i64, now: i64) -> f64 {
    let age_days = (now - last).max(0) as f64 / 86_400.0;
    score * 0.5f64.powf(age_days / 14.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(id: &str, name: &str, generic: &str) -> Rankable {
        Rankable {
            id: id.into(),
            name: name.into(),
            generic_name: generic.into(),
            ..Default::default()
        }
    }

    #[test]
    fn score_branches() {
        let ff = app("firefox", "Firefox", "Web Browser");
        assert_eq!(fuzzy_score(&ff, "fire"), Some((10000 - 7, Band::NameStart)));
        assert_eq!(
            fuzzy_score(&ff, "fox"),
            Some((8000 - 40 - 7, Band::NameInside))
        );
        // "web" is in the haystack "firefox web browser  firefox" at 8.
        assert_eq!(fuzzy_score(&ff, "web"), Some((6000 - 8, Band::Haystack)));
        assert_eq!(fuzzy_score(&ff, "zzz"), None);
        let naut = app("org.gnome.Nautilus", "Files", "File Manager");
        assert_eq!(fuzzy_score(&naut, "org"), Some((9500 - 18, Band::IdStart)));
        // Acronym of "Files File Manager org gnome nautilus" = "ffmogn".
        assert_eq!(
            fuzzy_score(&naut, "ffm"),
            Some((5000 - 6, Band::AcronymStart))
        );
    }

    #[test]
    fn word_text_matches_the_regexes() {
        assert_eq!(word_text("visualStudioCode"), "visual studio code");
        assert_eq!(word_text("aB1C"), "a b1 c");
        assert_eq!(word_text("org.gnome--Nautilus"), "org gnome nautilus");
        assert_eq!(words("LibreOffice Writer"), ["libre", "office", "writer"]);
    }

    #[test]
    fn utf16_units_like_js() {
        // 𝔸 is one char but two UTF-16 units.
        let e = app("x", "𝔸pp", "");
        assert_eq!(
            fuzzy_score(&e, "pp"),
            Some((8000 - 2 * 10 - 4, Band::NameInside))
        );
    }

    #[test]
    fn frecency_stays_inside_the_prefix_tier() {
        let entries = vec![
            app("calc", "Calculator", ""),
            app("chromium", "Chromium", ""),
            app("x", "Xc", "c thing"),
        ];
        // No frecency: shorter name prefix first.
        let plain = sorted(&entries, "c", &[0.0, 0.0, 0.0]);
        assert_eq!(plain.iter().map(|r| r.index).collect::<Vec<_>>(), [1, 0, 2]);
        // Frecency lifts Calculator among the prefix matches, never "Xc" above them.
        let used = sorted(&entries, "c", &[5.0, 0.0, 99.0]);
        assert_eq!(used.iter().map(|r| r.index).collect::<Vec<_>>(), [0, 1, 2]);
        // Empty query: frecency, then A–Z.
        let root = sorted(&entries, "", &[0.0, 1.0, 0.0]);
        assert_eq!(root.iter().map(|r| r.index).collect::<Vec<_>>(), [1, 0, 2]);
    }

    #[test]
    fn highlight_ranges() {
        assert_eq!(name_matches("Firefox", "fire"), vec![[0, 4]]);
        assert_eq!(
            name_matches("Visual Studio Code", "code vis"),
            vec![[0, 3], [14, 18]]
        );
        assert!(name_matches("Firefox", "zzz").is_empty());
    }

    #[test]
    fn decay_halves_every_two_weeks() {
        assert!((decayed(4.0, 0, 14 * 86_400) - 2.0).abs() < 1e-9);
        assert!((decayed(4.0, 100, 50) - 4.0).abs() < 1e-9);
    }
}
