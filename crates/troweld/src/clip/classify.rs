//! Text → `link` / `color` / `code` / `text` (`LAUNCHER-PLAN.md` L3 "Classify"). `files`, `image`
//! and `html` come from the offered MIME types, not from here.

use trowel_proto::ClipKind;

#[derive(Debug, Clone, PartialEq)]
pub struct TextClass {
    pub kind: ClipKind,
    /// `#RRGGBB` or `#RRGGBBAA`, for the swatch.
    pub color: Option<String>,
    /// URL host without `www.`.
    pub domain: Option<String>,
}

impl TextClass {
    fn plain(kind: ClipKind) -> Self {
        Self {
            kind,
            color: None,
            domain: None,
        }
    }
}

/// Window classes whose copies are code when they span lines (terminals and code editors).
const CODE_APPS: &[&str] = &[
    "com.mitchellh.ghostty",
    "alacritty",
    "kitty",
    "foot",
    "footclient",
    "org.wezfurlong.wezterm",
    "xterm",
    "konsole",
    "org.gnome.console",
    "org.gnome.ptyxis",
    "code",
    "code-oss",
    "codium",
    "vscodium",
    "cursor",
    "dev.zed.zed",
    "neovide",
    "sublime_text",
];

fn is_code_app(app: &str) -> bool {
    let app = app.to_ascii_lowercase();
    CODE_APPS.contains(&app.as_str()) || app.starts_with("jetbrains-")
}

pub fn classify(text: &str, source_app: Option<&str>) -> TextClass {
    let trimmed = text.trim();
    if let Some(domain) = link_domain(trimmed) {
        return TextClass {
            kind: ClipKind::Link,
            color: None,
            domain: Some(domain),
        };
    }
    if let Some(color) = parse_color(trimmed) {
        return TextClass {
            kind: ClipKind::Color,
            color: Some(color),
            domain: None,
        };
    }
    if looks_like_code(trimmed, source_app.is_some_and(is_code_app)) {
        return TextClass::plain(ClipKind::Code);
    }
    TextClass::plain(ClipKind::Text)
}

/// One URL and nothing else: `scheme://host…`, `mailto:`, or `www.host.tld…`. Returns its host.
pub fn link_domain(text: &str) -> Option<String> {
    if text.is_empty() || text.len() > 4096 || text.chars().any(char::is_whitespace) {
        return None;
    }
    let lower = text.to_ascii_lowercase();
    let rest = if let Some(rest) = lower.strip_prefix("mailto:") {
        return rest
            .split_once('@')
            .map(|(_, host)| host.split(['?', '#']).next().unwrap_or(host).to_string())
            .filter(|h| h.contains('.'));
    } else if let Some((scheme, rest)) = lower.split_once("://") {
        let mut chars = scheme.chars();
        let ok = chars.next().is_some_and(|c| c.is_ascii_alphabetic())
            && chars.all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c));
        if !ok || scheme == "file" {
            return None;
        }
        rest
    } else if lower.starts_with("www.") {
        &lower[..]
    } else {
        return None;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = authority.rsplit('@').next().unwrap_or("");
    let host = if host.starts_with('[') {
        host.split_once(']').map(|(h, _)| &h[1..]).unwrap_or("")
    } else {
        host.split(':').next().unwrap_or("")
    };
    if host.is_empty() {
        return None;
    }
    Some(host.strip_prefix("www.").unwrap_or(host).to_string())
}

/// `#rgb[a]`, `#rrggbb[aa]`, `rgb[a]()`, `hsl[a]()`, `oklch()` → `#RRGGBB[AA]`.
pub fn parse_color(text: &str) -> Option<String> {
    if let Some(hex) = text.strip_prefix('#') {
        if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let full: String = match hex.len() {
            3 | 4 => hex.chars().flat_map(|c| [c, c]).collect(),
            6 | 8 => hex.to_string(),
            _ => return None,
        };
        return Some(format!("#{}", full.to_ascii_uppercase()));
    }
    let lower = text.to_ascii_lowercase();
    let (name, args) = lower.strip_suffix(')')?.split_once('(')?;
    let parts: Vec<&str> = args
        .split([',', ' ', '/'])
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    if !(3..=4).contains(&parts.len()) {
        return None;
    }
    let alpha = match parts.get(3) {
        Some(a) => Some(unit(a, 1.0)?),
        None => None,
    };
    let (r, g, b) = match name.trim() {
        "rgb" | "rgba" => (
            unit(parts[0], 255.0)?,
            unit(parts[1], 255.0)?,
            unit(parts[2], 255.0)?,
        ),
        "hsl" | "hsla" => {
            let h = degrees(parts[0])?;
            hsl_to_rgb(h, unit(parts[1], 100.0)?, unit(parts[2], 100.0)?)
        }
        "oklch" => {
            let l = unit(parts[0], 1.0)?;
            let c = parts[1].strip_suffix('%').map_or_else(
                || parts[1].parse::<f64>().ok(),
                |p| p.parse::<f64>().ok().map(|v| v / 100.0 * 0.4),
            )?;
            oklch_to_rgb(l, c, degrees(parts[2])?)
        }
        _ => return None,
    };
    let byte = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    let mut out = format!("#{:02X}{:02X}{:02X}", byte(r), byte(g), byte(b));
    if let Some(a) = alpha.filter(|a| *a < 1.0) {
        out.push_str(&format!("{:02X}", byte(a)));
    }
    Some(out)
}

/// `50%` → 0.5; a bare number is divided by `scale` (255 for rgb, 1 for alpha and L).
fn unit(text: &str, scale: f64) -> Option<f64> {
    let v = if let Some(p) = text.strip_suffix('%') {
        p.parse::<f64>().ok()? / 100.0
    } else {
        text.parse::<f64>().ok()? / scale
    };
    (v.is_finite() && (0.0..=1.0).contains(&v)).then_some(v)
}

fn degrees(text: &str) -> Option<f64> {
    let v = text
        .strip_suffix("deg")
        .unwrap_or(text)
        .parse::<f64>()
        .ok()?;
    v.is_finite().then(|| v.rem_euclid(360.0))
}

fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (f64, f64, f64) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (r + m, g + m, b + m)
}

/// CSS Color 4: OKLCH → OKLab → linear sRGB → sRGB (out-of-gamut values are clamped).
fn oklch_to_rgb(l: f64, c: f64, h: f64) -> (f64, f64, f64) {
    let (a, b) = (c * h.to_radians().cos(), c * h.to_radians().sin());
    let l_ = l + 0.396_337_777_4 * a + 0.215_803_757_3 * b;
    let m_ = l - 0.105_561_345_8 * a - 0.063_854_172_8 * b;
    let s_ = l - 0.089_484_177_5 * a - 1.291_485_548 * b;
    let (l3, m3, s3) = (l_.powi(3), m_.powi(3), s_.powi(3));
    let r = 4.076_741_662_1 * l3 - 3.307_711_591_3 * m3 + 0.230_969_929_2 * s3;
    let g = -1.268_438_004_6 * l3 + 2.609_757_401_1 * m3 - 0.341_319_396_5 * s3;
    let b = -0.004_196_086_3 * l3 - 0.703_418_614_7 * m3 + 1.707_614_701 * s3;
    let gamma = |v: f64| {
        if v <= 0.003_130_8 {
            12.92 * v
        } else {
            1.055 * v.powf(1.0 / 2.4) - 0.055
        }
    };
    (gamma(r), gamma(g), gamma(b))
}

/// Code needs at least two non-empty lines and either a copy from a terminal or editor, or most
/// lines looking like code (statement endings, operators, keywords, indentation with symbols).
fn looks_like_code(text: &str, code_app: bool) -> bool {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.len() < 2 {
        return false;
    }
    if code_app {
        return true;
    }
    let codey = lines.iter().filter(|l| code_line(l)).count();
    let symbols = text
        .chars()
        .filter(|c| "{}[]()<>;=&|$*\\".contains(*c))
        .count();
    let visible = text.chars().filter(|c| !c.is_whitespace()).count().max(1);
    codey * 2 >= lines.len() && (codey >= 2 || symbols * 100 / visible >= 8)
}

fn code_line(line: &str) -> bool {
    const KEYWORDS: &[&str] = &[
        "fn ", "pub ", "let ", "const ", "var ", "def ", "class ", "import ", "from ", "return",
        "#include", "function", "if (", "if ", "for (", "while ", "else", "struct ", "impl ",
        "use ", "package ", "func ", "#!/", "$ ", "echo ", "sudo ", "export ",
    ];
    const OPERATORS: &[&str] = &["=>", "->", "::", "==", "!=", "&&", "||", ":=", "+=", "</"];
    let t = line.trim();
    let ends = t.ends_with([';', '{', '}', '(', '[', ']', ')'])
        || (t.ends_with(':') && t.contains('('))
        || t.ends_with(" \\");
    let indented_symbol = line.starts_with("  ") || line.starts_with('\t');
    ends || KEYWORDS.iter().any(|k| t.starts_with(k))
        || OPERATORS.iter().any(|o| t.contains(o))
        || (indented_symbol && t.chars().any(|c| "{}()[];=<>".contains(c)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(text: &str) -> ClipKind {
        classify(text, None).kind
    }

    #[test]
    fn classifier_table() {
        let table: &[(&str, ClipKind)] = &[
            (
                "https://github.com/thakurious/trowel/pull/7",
                ClipKind::Link,
            ),
            ("  https://example.com  \n", ClipKind::Link),
            ("www.example.org/a?b=c", ClipKind::Link),
            ("mailto:someone@example.com", ClipKind::Link),
            ("ftp://files.example.net/x", ClipKind::Link),
            ("https://a.com https://b.com", ClipKind::Text),
            ("see https://example.com", ClipKind::Text),
            ("file:///home/u/a.txt", ClipKind::Text),
            ("example.com", ClipKind::Text),
            ("#F2A22A", ClipKind::Color),
            ("#fff", ClipKind::Color),
            ("#11223344", ClipKind::Color),
            ("rgb(242, 162, 42)", ClipKind::Color),
            ("rgba(0 0 0 / 50%)", ClipKind::Color),
            ("hsl(36, 89%, 56%)", ClipKind::Color),
            ("oklch(0.7 0.15 70)", ClipKind::Color),
            ("#hashtag", ClipKind::Text),
            ("#12345", ClipKind::Text),
            ("rgb(300, 0, 0)", ClipKind::Text),
            ("fn main() {\n    println!(\"hello\");\n}\n", ClipKind::Code),
            ("def f(x):\n    return x + 1\n", ClipKind::Code),
            ("const a = 1;\nconst b = a + 1;", ClipKind::Code),
            ("<div>\n  <p>hi</p>\n</div>", ClipKind::Code),
            (
                "Dear team,\nthe meeting moved to 3pm.\nThanks!",
                ClipKind::Text,
            ),
            ("Shopping:\n- milk\n- eggs", ClipKind::Text),
            ("one line of prose", ClipKind::Text),
            ("x = 1;", ClipKind::Text),
            ("", ClipKind::Text),
        ];
        for (text, want) in table {
            assert_eq!(kind(text), *want, "{text:?}");
        }
    }

    #[test]
    fn terminal_copies_of_several_lines_are_code() {
        let text = "total 8\ndrwxr-xr-x 2 u u 4096 a\n";
        assert_eq!(kind(text), ClipKind::Text);
        assert_eq!(
            classify(text, Some("com.mitchellh.ghostty")).kind,
            ClipKind::Code
        );
        assert_eq!(classify("ls -la", Some("Alacritty")).kind, ClipKind::Text);
        assert_eq!(
            classify("a\nb", Some("jetbrains-idea")).kind,
            ClipKind::Code
        );
    }

    #[test]
    fn colors_normalize_to_hex() {
        let c = |t: &str| parse_color(t).unwrap();
        assert_eq!(c("#f2a22a"), "#F2A22A");
        assert_eq!(c("#abc"), "#AABBCC");
        assert_eq!(c("rgb(242, 162, 42)"), "#F2A22A");
        assert_eq!(c("rgba(0, 0, 0, 0.5)"), "#00000080");
        assert_eq!(c("rgba(0 0 0 / 100%)"), "#000000");
        assert_eq!(c("hsl(0, 100%, 50%)"), "#FF0000");
        assert_eq!(c("hsl(120deg 100% 25%)"), "#008000");
        assert_eq!(c("oklch(1 0 0)"), "#FFFFFF");
        assert_eq!(c("oklch(0% 0 0)"), "#000000");
        // CSS Color 4 example: oklch(62.8% 0.2577 29.23) is sRGB red.
        assert_eq!(c("oklch(62.8% 0.2577 29.23)"), "#FF0000");
    }

    #[test]
    fn link_domains() {
        let d = |t: &str| link_domain(t);
        assert_eq!(d("https://www.github.com/x").as_deref(), Some("github.com"));
        assert_eq!(d("https://user@host.io:8080/p").as_deref(), Some("host.io"));
        assert_eq!(d("http://[::1]:3000/").as_deref(), Some("::1"));
        assert_eq!(d("mailto:a@b.org?subject=x").as_deref(), Some("b.org"));
        assert_eq!(d("https://"), None);
        assert_eq!(d("1://x"), None);
    }
}
