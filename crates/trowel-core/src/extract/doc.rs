//! Documents. HTML strip and chunker are bit-faithful to
//! `upstream/Sources/DigUpKit/Extractors.swift` `DocExtractor`.

use std::ffi::OsStr;
use std::fs;
use std::path::Path;

use super::error::ExtractError;
use super::run::{abs_path, run_tool, stdout_text, TIMEOUT_DOCX};

pub struct DocExtractor;

impl DocExtractor {
    pub fn text_of(path: &Path) -> Result<String, ExtractError> {
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();
        match ext.as_str() {
            "html" | "htm" => Ok(Self::plain_text_from_html(&read_text(path)?)),
            "docx" => docx_text(path),
            "odt" => odt_text(path),
            "rtf" => rtf_text(path),
            "doc" => doc_text(path),
            _ => read_text(path),
        }
    }

    /// `upstream/Sources/DigUpKit/Extractors.swift:186`
    pub fn plain_text_from_html(html: &str) -> String {
        let mut text = html.to_string();
        for pattern in [("script", true), ("style", true), ("head", true)] {
            text = strip_container(&text, pattern.0);
        }
        text = strip_comments(&text);
        text = replace_block_ends(&text);
        text = strip_tags(&text);
        let entities = [
            ("&nbsp;", " "),
            ("&amp;", "&"),
            ("&lt;", "<"),
            ("&gt;", ">"),
            ("&quot;", "\""),
            ("&#39;", "'"),
            ("&apos;", "'"),
            ("&mdash;", "—"),
            ("&ndash;", "–"),
            ("&hellip;", "…"),
        ];
        for (entity, ch) in entities {
            text = text.replace(entity, ch);
        }
        text = decode_numeric_entities(&text);
        collapse_spaces(&text)
    }

    /// `upstream/Sources/DigUpKit/Extractors.swift:222`
    pub fn chunks(text: &str, size: usize, overlap: usize, limit: usize) -> Vec<String> {
        let clean = collapse_blank_lines(&collapse_horizontal(text));
        let chars: Vec<char> = clean.chars().collect();
        if chars.is_empty() {
            return Vec::new();
        }
        let mut chunks = Vec::new();
        let mut start = 0usize;
        while start < chars.len() && chunks.len() < limit {
            let mut end = (start + size).min(chars.len());
            if end < chars.len() {
                if let Some(rel) = chars[start..end].iter().rposition(|c| c.is_whitespace()) {
                    if rel > size / 2 {
                        end = start + rel;
                    }
                }
            }
            let chunk: String = chars[start..end].iter().collect();
            let chunk = chunk.trim().to_string();
            if !chunk.is_empty() {
                chunks.push(chunk);
            }
            if end >= chars.len() {
                break;
            }
            let mut next = end.saturating_sub(overlap).max(start);
            if next <= start {
                next = end;
            }
            while next < end && !chars[next].is_whitespace() {
                next += 1;
            }
            start = if next < end { next } else { end };
        }
        chunks
    }
}

fn read_text(path: &Path) -> Result<String, ExtractError> {
    match fs::read_to_string(path) {
        Ok(s) => Ok(s),
        Err(_) => {
            let bytes = fs::read(path).map_err(|err| ExtractError::Unreadable(err.to_string()))?;
            Ok(String::from_utf8_lossy(&bytes).into_owned())
        }
    }
}

fn strip_container(html: &str, tag: &str) -> String {
    let mut out = String::new();
    let lower = html.to_ascii_lowercase();
    let open = format!("<{tag}");
    let close = format!("</{tag}");
    let mut i = 0;
    while i < html.len() {
        if let Some(rel) = lower[i..].find(&open) {
            let start = i + rel;
            out.push_str(&html[i..start]);
            let after = start + open.len();
            if let Some(end_rel) = lower[after..].find(&close) {
                let close_at = after + end_rel;
                if let Some(gt) = html[close_at..].find('>') {
                    i = close_at + gt + 1;
                    out.push(' ');
                    continue;
                }
            }
            break;
        }
        out.push_str(&html[i..]);
        break;
    }
    out
}

fn strip_comments(html: &str) -> String {
    let mut out = String::new();
    let mut rest = html;
    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        out.push(' ');
        rest = match rest[start + 4..].find("-->") {
            Some(end) => &rest[start + 4 + end + 3..],
            None => "",
        };
    }
    out.push_str(rest);
    out
}

fn replace_block_ends(html: &str) -> String {
    let mut out = String::new();
    let lower = html.to_ascii_lowercase();
    let mut i = 0;
    while i < html.len() {
        if html.as_bytes()[i] == b'<' {
            if let Some(gt) = html[i..].find('>') {
                let tag = &lower[i + 1..i + gt];
                let name = tag
                    .trim_start_matches('/')
                    .split([' ', '\t', '\n'])
                    .next()
                    .unwrap_or("");
                let closing = tag.starts_with('/');
                if closing
                    && matches!(
                        name,
                        "p" | "div"
                            | "li"
                            | "tr"
                            | "section"
                            | "article"
                            | "h1"
                            | "h2"
                            | "h3"
                            | "h4"
                            | "h5"
                            | "h6"
                    )
                    || name == "br"
                {
                    out.push('\n');
                } else {
                    out.push_str(&html[i..=i + gt]);
                    i += gt + 1;
                    continue;
                }
                i += gt + 1;
                continue;
            }
        }
        out.push(html[i..].chars().next().unwrap());
        i += html[i..].chars().next().unwrap().len_utf8();
    }
    out
}

fn strip_tags(html: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out
}

fn decode_numeric_entities(text: &str) -> String {
    if !text.contains("&#") {
        return text.to_string();
    }
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("&#") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let (digits, hex) = if after.starts_with('x') || after.starts_with('X') {
            (&after[1..], true)
        } else {
            (after, false)
        };
        let end = digits.find(';').unwrap_or(0);
        let code = &digits[..end];
        let value = if hex {
            u32::from_str_radix(code, 16).ok()
        } else {
            code.parse().ok()
        };
        if let Some(ch) = value.and_then(char::from_u32) {
            out.push(ch);
        } else {
            out.push(' ');
        }
        rest = if end < digits.len() {
            &digits[end + 1..]
        } else {
            ""
        };
    }
    out.push_str(rest);
    out
}

fn collapse_spaces(text: &str) -> String {
    let mut out = String::new();
    let mut prev_space = false;
    let mut pending_nl = false;
    for ch in text.chars() {
        match ch {
            ' ' | '\t' => {
                if !pending_nl {
                    prev_space = true;
                }
            }
            '\n' => {
                pending_nl = true;
                prev_space = false;
            }
            _ => {
                if pending_nl {
                    if !out.is_empty() {
                        out.push('\n');
                    }
                    pending_nl = false;
                } else if prev_space && !out.is_empty() {
                    out.push(' ');
                }
                prev_space = false;
                out.push(ch);
            }
        }
    }
    out.trim().to_string()
}

fn collapse_horizontal(text: &str) -> String {
    let mut out = String::new();
    let mut space = false;
    for ch in text.chars() {
        if ch == ' ' || ch == '\t' || ch == '\u{00A0}' {
            space = true;
        } else {
            if space && !out.is_empty() && ch != '\n' {
                out.push(' ');
            }
            space = false;
            out.push(ch);
        }
    }
    out
}

fn collapse_blank_lines(text: &str) -> String {
    let mut out = String::new();
    let mut newlines = 0;
    for ch in text.chars() {
        if ch == '\n' {
            newlines += 1;
            if newlines <= 2 {
                out.push('\n');
            }
        } else if !ch.is_whitespace() || ch == ' ' {
            newlines = 0;
            out.push(ch);
        } else {
            out.push(ch);
            if ch != '\r' {
                newlines = 0;
            }
        }
    }
    out.trim().to_string()
}

fn xml_text(xml: &str) -> String {
    strip_tags(xml)
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

fn unzip_member(path: &Path, member: &str) -> Result<String, ExtractError> {
    let abs = abs_path(path);
    let output = run_tool(
        "unzip",
        &["-p", "-qq"],
        &[abs.as_os_str(), OsStr::new(member)],
        TIMEOUT_DOCX,
    )?;
    stdout_text(&output, "unzip")
}

fn docx_text(path: &Path) -> Result<String, ExtractError> {
    let xml = unzip_member(path, "word/document.xml")?;
    let text = xml_text(&xml);
    if text.trim().is_empty() {
        Err(ExtractError::Empty("empty document".into()))
    } else {
        Ok(collapse_spaces(&text))
    }
}

fn odt_text(path: &Path) -> Result<String, ExtractError> {
    let xml = unzip_member(path, "content.xml")?;
    Ok(collapse_spaces(&xml_text(&xml)))
}

fn rtf_text(path: &Path) -> Result<String, ExtractError> {
    let abs = abs_path(path);
    match run_tool("unrtf", &["--text"], &[abs.as_os_str()], TIMEOUT_DOCX) {
        Ok(out) if out.status.success() => {
            let raw = String::from_utf8_lossy(&out.stdout);
            Ok(raw
                .lines()
                .skip_while(|l| l.starts_with("###") || l.is_empty())
                .collect::<Vec<_>>()
                .join("\n")
                .trim()
                .to_string())
        }
        _ => Err(ExtractError::Unreadable(
            "rtf needs unrtf (not installed)".into(),
        )),
    }
}

fn doc_text(path: &Path) -> Result<String, ExtractError> {
    let abs = abs_path(path);
    for (bin, flags) in [("antiword", &[][..]), ("pandoc", &["-t", "plain"][..])] {
        if let Ok(out) = run_tool(bin, flags, &[abs.as_os_str()], TIMEOUT_DOCX) {
            if out.status.success() {
                return stdout_text(&out, bin);
            }
        }
    }
    Err(ExtractError::LeftOut(
        "legacy .doc (install pandoc or antiword)".into(),
    ))
}

/// One line, at most `limit` characters: for result lists.
/// `upstream/Sources/DigUpKit/Extractors.swift:249`
pub fn excerpt(text: &str, limit: usize) -> String {
    let line: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() > limit {
        format!("{}…", line.chars().take(limit).collect::<String>())
    } else {
        line
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_becomes_plain_text() {
        let html = r#"<html><head><title>T</title><style>p { color: red }</style></head>
            <body><h1>Trip &amp; plans</h1><p>Fly to <b>Tokyo</b>&nbsp;on Friday.</p><script>var x = 1;</script>
            <p>Caf&#233; &#x2014; ramen</p><!-- hidden --></body></html>"#;
        assert_eq!(
            DocExtractor::plain_text_from_html(html),
            "Trip & plans\nFly to Tokyo on Friday.\nCafé — ramen"
        );
    }

    #[test]
    fn chunks_overlap_and_respect_words() {
        let text = (1..=600)
            .map(|i| format!("word{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        let chunks = DocExtractor::chunks(&text, 500, 100, 100);
        assert!(chunks.len() > 5);
        assert!(chunks.iter().all(|c| c.chars().count() <= 500));
        assert!(chunks.iter().all(|c| c.starts_with("word")));
        assert!(chunks.last().unwrap().ends_with("word600"));
    }
}
