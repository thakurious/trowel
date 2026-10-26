//! Snippets (P-D33, `PHASE9-PLAN.md` §1.5): stored in `state.sqlite` with FTS on name, keyword
//! and body; placeholders expand at paste time. Panel-driven only: no system-wide expansion
//! while typing (that needs an fcitx5 addon, not approved).
//!
//! Placeholders: `{clipboard}`, `{date}`, `{time}`, `{datetime}`, `{date:%d %b}` (strftime),
//! `{uuid}`, `{argument:name}` (Raycast's `{argument name="name"}` too). There is no `{cursor}`:
//! a paste can't place the caret. Anything else in braces stays as typed.

use std::collections::BTreeMap;

use serde_json::{json, Value};
use trowel_core::{DbError, SqlValue};
use trowel_proto::Snippet;

use super::{clock, with_db, Host};

pub const MAX_BODY: usize = 64 * 1024;

const COLUMNS: &str = "id, name, keyword, body, tags, created, last_used, uses";

fn row(r: &trowel_core::database::QueryRow<'_>) -> Snippet {
    let body = r.text(3).unwrap_or_default();
    Snippet {
        id: r.int(0),
        name: r.text(1).unwrap_or_default(),
        keyword: r.text(2).unwrap_or_default(),
        arguments: arguments(&body),
        body,
        tags: r
            .text(4)
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default(),
        created: r.int(5),
        last_used: r.int(6),
        uses: r.int(7),
    }
}

/// One placeholder: where it is and what it asks for.
enum Part<'a> {
    Text(&'a str),
    Place(&'a str),
}

/// Splits `body` on `{…}` (no nesting; a `{` without its `}` is text).
fn parts(body: &str) -> Vec<Part<'_>> {
    let mut out = Vec::new();
    let mut rest = body;
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}') else {
            break;
        };
        let inner = &rest[open + 1..open + close];
        if inner.contains('{') {
            out.push(Part::Text(&rest[..open + 1]));
            rest = &rest[open + 1..];
            continue;
        }
        out.push(Part::Text(&rest[..open]));
        out.push(Part::Place(inner));
        rest = &rest[open + close + 1..];
    }
    out.push(Part::Text(rest));
    out
}

/// `argument:name` or Raycast's `argument name="name"` → the name.
fn argument_name(place: &str) -> Option<String> {
    let p = place.trim();
    if let Some(n) = p.strip_prefix("argument:") {
        let n = n.trim();
        return (!n.is_empty()).then(|| n.to_string());
    }
    let rest = p.strip_prefix("argument ")?;
    let at = rest.find("name=")?;
    let v = rest[at + 5..].trim_start();
    let v = v.strip_prefix('"')?;
    let end = v.find('"')?;
    (end > 0).then(|| v[..end].to_string())
}

/// The `{argument:…}` names in order of first use.
pub fn arguments(body: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for part in parts(body) {
        if let Part::Place(p) = part {
            if let Some(n) = argument_name(p) {
                if !out.contains(&n) {
                    out.push(n);
                }
            }
        }
    }
    out
}

fn uuid_v4() -> String {
    let mut b = [0u8; 16];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        use std::io::Read;
        let _ = f.read_exact(&mut b);
    }
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &h[0..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..32]
    )
}

/// The text to paste. A missing argument is an error naming it.
pub fn expand(
    body: &str,
    args: &BTreeMap<String, String>,
    clipboard: &dyn Fn() -> String,
    now: f64,
) -> Result<String, String> {
    let mut out = String::with_capacity(body.len());
    for part in parts(body) {
        match part {
            Part::Text(t) => out.push_str(t),
            Part::Place(p) => {
                let key = p.trim();
                let fmt = |f: &str| clock::format(f, now).unwrap_or_default();
                if let Some(name) = argument_name(key) {
                    match args.get(&name) {
                        Some(v) => out.push_str(v),
                        None => return Err(format!("needs {name}")),
                    }
                } else if key == "clipboard" {
                    out.push_str(&clipboard());
                } else if key == "date" {
                    out.push_str(&fmt("%Y-%m-%d"));
                } else if key == "time" {
                    out.push_str(&fmt("%H:%M"));
                } else if key == "datetime" {
                    out.push_str(&fmt("%Y-%m-%d %H:%M"));
                } else if key == "uuid" {
                    out.push_str(&uuid_v4());
                } else if let Some(f) = key
                    .strip_prefix("date:")
                    .or_else(|| key.strip_prefix("time:"))
                {
                    match clock::format(f, now) {
                        Some(s) => out.push_str(&s),
                        None => out.push_str(&format!("{{{p}}}")),
                    }
                } else {
                    out.push_str(&format!("{{{p}}}"));
                }
            }
        }
    }
    Ok(out)
}

/// FTS5 prefix query for each word, quoted (so `-`, `:` or `"` in the text can't be syntax).
fn fts_query(text: &str) -> Option<String> {
    let words: Vec<String> = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| format!("\"{}\"*", w.replace('"', "")))
        .collect();
    (!words.is_empty()).then(|| words.join(" "))
}

/// Exact keyword first, then name prefix, then the FTS ranking; an empty text is most recently
/// used first.
pub fn query(host: &dyn Host, text: &str, limit: u32) -> Result<Vec<Snippet>, String> {
    let text = text.trim().to_string();
    with_db(host, |db| {
        let mut out: Vec<Snippet> = Vec::new();
        let lim = SqlValue::Int(i64::from(limit.clamp(1, 500)));
        if text.is_empty() {
            db.db.query(
                &format!(
                    "SELECT {COLUMNS} FROM snippets ORDER BY last_used DESC, uses DESC, name LIMIT ?1"
                ),
                &[lim],
                |r| {
                    out.push(row(r));
                    Ok(())
                },
            )?;
            return Ok(out);
        }
        db.db.query(
            &format!(
                "SELECT {COLUMNS} FROM snippets WHERE keyword <> '' AND lower(keyword) = lower(?1)
                 UNION ALL
                 SELECT {COLUMNS} FROM snippets WHERE lower(name) LIKE lower(?2) ESCAPE '\\'
                 LIMIT ?3"
            ),
            &[
                SqlValue::Text(text.clone()),
                SqlValue::Text(format!(
                    "{}%",
                    text.replace('\\', "\\\\")
                        .replace('%', "\\%")
                        .replace('_', "\\_")
                )),
                lim.clone(),
            ],
            |r| {
                out.push(row(r));
                Ok(())
            },
        )?;
        if let Some(q) = fts_query(&text) {
            db.db.query(
                &format!(
                    "SELECT {} FROM snippets_fts JOIN snippets s ON s.id = snippets_fts.rowid
                     WHERE snippets_fts MATCH ?1 ORDER BY bm25(snippets_fts, 4.0, 4.0, 1.0)
                     LIMIT ?2",
                    COLUMNS
                        .split(", ")
                        .map(|c| format!("s.{c}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                &[SqlValue::Text(q), lim],
                |r| {
                    out.push(row(r));
                    Ok(())
                },
            )?;
        }
        let mut seen = std::collections::HashSet::new();
        out.retain(|s| seen.insert(s.id));
        out.truncate(limit as usize);
        Ok(out)
    })
}

pub fn get(host: &dyn Host, id: i64) -> Result<Option<Snippet>, String> {
    with_db(host, |db| {
        let mut out = None;
        db.db.query(
            &format!("SELECT {COLUMNS} FROM snippets WHERE id = ?1"),
            &[SqlValue::Int(id)],
            |r| {
                out = Some(row(r));
                Ok(())
            },
        )?;
        Ok(out)
    })
}

fn validate(s: &Snippet) -> Result<(), String> {
    if s.name.trim().is_empty() {
        return Err("a snippet needs a name".into());
    }
    if s.body.is_empty() {
        return Err("a snippet needs some text".into());
    }
    if s.body.len() > MAX_BODY {
        return Err("a snippet is at most 64 KB".into());
    }
    if s.keyword.chars().any(char::is_whitespace) {
        return Err("a keyword has no spaces".into());
    }
    Ok(())
}

/// Creates (`id` 0) or updates. A keyword another snippet has is refused.
pub fn save(host: &dyn Host, s: Snippet, now: i64) -> Result<Snippet, String> {
    validate(&s)?;
    let s = Snippet {
        name: s.name.trim().to_string(),
        keyword: s.keyword.trim().to_string(),
        ..s
    };
    let id = with_db(host, |db| {
        if !s.keyword.is_empty() {
            let mut taken = None;
            db.db.query(
                "SELECT name FROM snippets WHERE lower(keyword) = lower(?1) AND id <> ?2",
                &[SqlValue::Text(s.keyword.clone()), SqlValue::Int(s.id)],
                |r| {
                    taken = r.text(0);
                    Ok(())
                },
            )?;
            if let Some(name) = taken {
                return Err(DbError::new(format!("“{name}” already uses {}", s.keyword)));
            }
        }
        let tags = serde_json::to_string(&s.tags).unwrap_or_else(|_| "[]".into());
        if s.id == 0 {
            db.db.run(
                "INSERT INTO snippets(name, keyword, body, tags, created) VALUES (?1, ?2, ?3, ?4, ?5)",
                &[
                    SqlValue::Text(s.name.clone()),
                    SqlValue::Text(s.keyword.clone()),
                    SqlValue::Text(s.body.clone()),
                    SqlValue::Text(tags),
                    SqlValue::Int(now),
                ],
            )?;
            Ok(db.db.last_insert_id())
        } else {
            db.db.run(
                "UPDATE snippets SET name = ?2, keyword = ?3, body = ?4, tags = ?5 WHERE id = ?1",
                &[
                    SqlValue::Int(s.id),
                    SqlValue::Text(s.name.clone()),
                    SqlValue::Text(s.keyword.clone()),
                    SqlValue::Text(s.body.clone()),
                    SqlValue::Text(tags),
                ],
            )?;
            Ok(s.id)
        }
    })?;
    get(host, id)?.ok_or_else(|| format!("no snippet {id}"))
}

pub fn delete(host: &dyn Host, id: i64) -> Result<bool, String> {
    let had = get(host, id)?.is_some();
    with_db(host, |db| {
        db.db
            .run("DELETE FROM snippets WHERE id = ?1", &[SqlValue::Int(id)])
    })?;
    Ok(had)
}

pub fn mark_used(host: &dyn Host, id: i64, now: i64) -> Result<(), String> {
    with_db(host, |db| {
        db.db.run(
            "UPDATE snippets SET uses = uses + 1, last_used = ?2 WHERE id = ?1",
            &[SqlValue::Int(id), SqlValue::Int(now)],
        )
    })
}

/// Raycast's export: `[{name, text, keyword}]`. Same name and text as an existing one = skipped;
/// a keyword already in use is dropped from the imported one.
pub fn import(host: &dyn Host, json_text: &str, now: i64) -> Result<(u32, u32), String> {
    let items: Vec<Value> =
        serde_json::from_str(json_text).map_err(|e| format!("not a snippet export: {e}"))?;
    let existing = query(host, "", 500)?;
    let (mut imported, mut skipped) = (0, 0);
    for item in items {
        let name = item["name"].as_str().unwrap_or("").to_string();
        let body = item["text"]
            .as_str()
            .or_else(|| item["body"].as_str())
            .unwrap_or("")
            .to_string();
        let keyword = item["keyword"].as_str().unwrap_or("").to_string();
        if existing.iter().any(|s| s.name == name && s.body == body) {
            skipped += 1;
            continue;
        }
        let snippet = Snippet {
            id: 0,
            name: name.clone(),
            keyword: keyword.clone(),
            body: body.clone(),
            tags: Vec::new(),
            created: now,
            last_used: 0,
            uses: 0,
            arguments: Vec::new(),
        };
        match save(host, snippet.clone(), now) {
            Ok(_) => imported += 1,
            Err(e) if e.contains("already uses") => {
                match save(
                    host,
                    Snippet {
                        keyword: String::new(),
                        ..snippet
                    },
                    now,
                ) {
                    Ok(_) => imported += 1,
                    Err(_) => skipped += 1,
                }
            }
            Err(_) => skipped += 1,
        }
    }
    Ok((imported, skipped))
}

pub fn export(host: &dyn Host) -> Result<String, String> {
    let all = query(host, "", 500)?;
    let items: Vec<Value> = all
        .iter()
        .map(|s| json!({"name": s.name, "text": s.body, "keyword": s.keyword}))
        .collect();
    serde_json::to_string_pretty(&items).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn placeholders_expand() {
        let clip = || "CLIP".to_string();
        let t = 1_793_000_000.0;
        let out = expand(
            "Hi {argument:name}, {clipboard} on {date} at {time}. {unknown} {date:%Y} {",
            &args(&[("name", "Ana")]),
            &clip,
            t,
        )
        .unwrap();
        let date = clock::format("%Y-%m-%d", t).unwrap();
        let time = clock::format("%H:%M", t).unwrap();
        let year = clock::format("%Y", t).unwrap();
        assert_eq!(
            out,
            format!("Hi Ana, CLIP on {date} at {time}. {{unknown}} {year} {{")
        );
        let u = expand("{uuid}", &args(&[]), &clip, t).unwrap();
        assert_eq!(u.len(), 36);
        assert_eq!(&u[14..15], "4");
        assert_eq!(
            expand("{argument:who}", &args(&[]), &clip, t).unwrap_err(),
            "needs who"
        );
        assert_eq!(
            expand(
                r#"{argument name="who"}!"#,
                &args(&[("who", "x")]),
                &clip,
                t
            )
            .unwrap(),
            "x!"
        );
    }

    #[test]
    fn arguments_in_order_once() {
        assert_eq!(
            arguments(r#"{argument:b} {argument:a} {argument:b} {argument name="c"} {clipboard}"#),
            ["b", "a", "c"]
        );
        assert!(arguments("no {placeholders} here").is_empty());
    }

    #[test]
    fn fts_words_are_quoted() {
        assert_eq!(fts_query("re: \"sig\"").unwrap(), "\"re\"* \"sig\"*");
        assert!(fts_query(";;").is_none());
    }
}
