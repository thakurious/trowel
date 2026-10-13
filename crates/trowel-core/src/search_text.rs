//! Text the way keyword search keeps it and matches it.
//! `upstream/Sources/DigUpKit/SearchText.swift`

use unicode_normalization::UnicodeNormalization;

/// `upstream/Sources/DigUpKit/SearchText.swift:4`
pub struct SearchText;

impl SearchText {
    /// NFKC, drop invisible formatting, Arabic the way people type it.
    /// `upstream/Sources/DigUpKit/SearchText.swift:8`
    pub fn normalized(text: &str) -> String {
        let compatible: String = text.nfkc().collect();
        let mut out = String::new();
        for ch in compatible.chars() {
            let v = ch as u32;
            match v {
                0x200B => out.push(' '),
                0x00AD
                | 0x061C
                | 0x200C..=0x200F
                | 0x202A..=0x202E
                | 0x2060..=0x2064
                | 0x2066..=0x2069
                | 0xFEFF => {}
                0x0640 | 0x064B..=0x065F | 0x0670 => {}
                0x0622 | 0x0623 | 0x0625 | 0x0671 => out.push('\u{0627}'),
                _ => out.push(ch),
            }
        }
        out
    }

    /// Words of scripts that attach endings: keyword search matches them as prefixes.
    /// `upstream/Sources/DigUpKit/SearchText.swift:29`
    pub fn inflects(word: &str) -> bool {
        word.chars()
            .any(|c| SCRIPT_INDIC.contains(&(c as u32)) || SCRIPT_ARABIC.contains(&(c as u32)))
    }

    /// At least a fifth of what isn't space is letters (Apple `letterCharacterSet` = L* + M*).
    /// `upstream/Sources/DigUpKit/SearchText.swift:37`
    pub fn has_words(text: &str) -> bool {
        let mut letters = 0;
        let mut others = 0;
        for ch in text.chars() {
            if ch.is_whitespace() {
                continue;
            }
            if is_apple_letter(ch) {
                letters += 1;
            } else {
                others += 1;
            }
        }
        letters > 0 && letters * 4 >= others
    }

    /// Where `word` is in `text` as keyword search would find it, as byte ranges of the text as written.
    /// `upstream/Sources/DigUpKit/SearchText.swift:49`
    pub fn matches(word: &str, text: &str) -> Vec<std::ops::Range<usize>> {
        let needle = Self::normalized(word).to_lowercase();
        if needle.is_empty() {
            return Vec::new();
        }
        let folded = Folded::new(text);
        let prefix = Self::inflects(word);
        let haystack = folded.text.to_lowercase();
        let mut found = Vec::new();
        let mut search_from = 0;
        while found.len() < 500 {
            let rest = &haystack[search_from..];
            let Some(rel) = find_ignore_case(rest, &needle) else {
                break;
            };
            let start = search_from + rel;
            let end = start + needle.len();
            search_from = end;
            let Some(mut range) = folded.original(start..end) else {
                continue;
            };
            if range.start > 0 {
                if let Some(prev) = prev_char(text, range.start) {
                    if is_word_character(prev) {
                        continue;
                    }
                }
            }
            if range.end < text.len() {
                if let Some(next) = text[range.end..].chars().next() {
                    if is_word_character(next) {
                        if !prefix {
                            continue;
                        }
                        let mut end = range.end;
                        for ch in text[end..].chars() {
                            if !is_word_character(ch) {
                                break;
                            }
                            end += ch.len_utf8();
                        }
                        range.end = end;
                    }
                }
            }
            if found.last().is_some_and(|last: &std::ops::Range<usize>| {
                last.start < range.end && range.start < last.end
            }) {
                continue;
            }
            found.push(range);
        }
        found
    }

    /// One line from `text` around the first of `terms`, terms marked « ».
    /// `upstream/Sources/DigUpKit/SearchText.swift:79`
    pub fn snippet(text: &str, terms: &[&str], words: usize) -> Option<String> {
        let mut marks: Vec<std::ops::Range<usize>> =
            terms.iter().flat_map(|t| Self::matches(t, text)).collect();
        marks.sort_by_key(|r| r.start);
        let first = marks.first()?.clone();
        let mut start = first.start;
        let mut before = 0;
        while start > 0 && before < words / 3 {
            let prev = prev_char_start(text, start);
            start = prev;
            if text[start..]
                .chars()
                .next()
                .is_some_and(|c| c.is_whitespace())
                && start < first.start
            {
                let after_ws = start + text[start..].chars().next().unwrap().len_utf8();
                if after_ws <= text.len()
                    && text[after_ws..]
                        .chars()
                        .next()
                        .is_some_and(|c| !c.is_whitespace())
                {
                    before += 1;
                }
            }
        }
        if start > 0
            || text[start..]
                .chars()
                .next()
                .is_some_and(|c| c.is_whitespace())
        {
            if let Some(ch) = text[start..].chars().next() {
                start += ch.len_utf8();
            }
        }
        let mut end = first.end;
        let mut after = 0;
        while end < text.len() && after < words.saturating_sub(before) {
            let ch = text[end..].chars().next().unwrap();
            if ch.is_whitespace() {
                after += 1;
            }
            end += ch.len_utf8();
        }
        let mut out = String::new();
        if start > 0 {
            out.push('…');
        }
        let mut cursor = start;
        for mark in &marks {
            if mark.start >= start && mark.end <= end && mark.start >= cursor {
                out.push_str(&text[cursor..mark.start]);
                out.push('«');
                out.push_str(&text[mark.start..mark.end]);
                out.push('»');
                cursor = mark.end;
            }
        }
        out.push_str(&text[cursor..end]);
        if end < text.len() {
            out.push('…');
        }
        Some(collapse_ws(&out).trim().to_string())
    }
}

fn is_apple_letter(ch: char) -> bool {
    // NSCharacterSet.letterCharacterSet: Unicode L* and M*.
    ch.is_alphabetic() || is_mark(ch)
}

fn is_mark(ch: char) -> bool {
    matches!(
        ch,
        '\u{0300}'..='\u{036F}'
            | '\u{0483}'..='\u{0489}'
            | '\u{0591}'..='\u{05BD}'
            | '\u{0610}'..='\u{061A}'
            | '\u{064B}'..='\u{065F}'
            | '\u{0670}'
            | '\u{06D6}'..='\u{06ED}'
            | '\u{0900}'..='\u{0903}'
            | '\u{093A}'..='\u{094F}'
            | '\u{0951}'..='\u{0957}'
            | '\u{0962}'..='\u{0963}'
            | '\u{0981}'..='\u{0983}'
            | '\u{09BE}'..='\u{09C4}'
            | '\u{09C7}'..='\u{09C8}'
            | '\u{09CB}'..='\u{09CC}'
            | '\u{09D7}'
            | '\u{09E2}'..='\u{09E3}'
    )
}

/// Apple `CharacterSet.alphanumerics`: Unicode L*, M*, N*.
pub(crate) fn is_word_character(ch: char) -> bool {
    ch.is_alphanumeric() || is_mark(ch)
}

fn prev_char(text: &str, byte: usize) -> Option<char> {
    text[..byte].chars().next_back()
}

fn prev_char_start(text: &str, byte: usize) -> usize {
    let ch = text[..byte].chars().next_back().unwrap();
    byte - ch.len_utf8()
}

fn find_ignore_case(haystack: &str, needle: &str) -> Option<usize> {
    haystack.find(needle)
}

fn collapse_ws(text: &str) -> String {
    let mut out = String::new();
    let mut prev_ws = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !prev_ws {
                out.push(' ');
            }
            prev_ws = true;
        } else {
            prev_ws = false;
            out.push(ch);
        }
    }
    out
}

struct Folded {
    text: String,
    /// For each UTF-8 byte of `text`, the start byte of the source character it came from.
    origin: Vec<usize>,
    source: String,
}

impl Folded {
    fn new(source: &str) -> Self {
        let mut text = String::new();
        let mut origin = Vec::new();
        let mut index = 0;
        for ch in source.chars() {
            let piece = SearchText::normalized(&ch.to_string()).to_lowercase();
            for _ in piece.bytes() {
                origin.push(index);
            }
            text.push_str(&piece);
            index += ch.len_utf8();
        }
        Self {
            text,
            origin,
            source: source.to_string(),
        }
    }

    fn original(&self, range: std::ops::Range<usize>) -> Option<std::ops::Range<usize>> {
        if range.is_empty() || range.end > self.origin.len() {
            return None;
        }
        let first = self.origin[range.start];
        let last = self.origin[range.end - 1];
        let last_ch = self.source[last..].chars().next()?;
        Some(first..last + last_ch.len_utf8())
    }
}

const SCRIPT_INDIC: std::ops::RangeInclusive<u32> = 0x0900..=0x0DFF;
const SCRIPT_ARABIC: std::ops::RangeInclusive<u32> = 0x0600..=0x06FF;

/// `upstream/Sources/DigUpKit/SearchText.swift:143`
pub struct Landmark;

impl Landmark {
    pub fn find(page: i32, texts: &[&str], words: &[&str]) -> Option<String> {
        if page <= 1 || texts.len() < page as usize {
            return None;
        }
        let target = texts[(page as usize) - 1];
        let before: Vec<String> = texts[..(page as usize) - 1]
            .iter()
            .map(|t| SearchText::normalized(t).to_lowercase())
            .collect();
        let on_page: Vec<String> = words
            .iter()
            .filter(|w| w.chars().count() >= 3)
            .filter_map(|word| {
                SearchText::matches(word, target)
                    .first()
                    .map(|r| target[r.start..r.end].to_string())
            })
            .collect();
        on_page
            .iter()
            .find(|word| {
                let n = SearchText::normalized(word).to_lowercase();
                !before.iter().any(|b| b.contains(&n))
            })
            .cloned()
            .or_else(|| on_page.first().cloned())
    }
}

/// `upstream/Sources/DigUpKit/SearchText.swift:165`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageTextProblem {
    GlyphOrder,
    LostLetters,
}

impl PageTextProblem {
    pub fn check(text: &str) -> Option<Self> {
        let text: String = text.nfc().collect();
        let mut vowel_signs = 0;
        let mut misplaced = 0;
        let mut lost_letters = 0;
        let mut previous = ' ';
        let mut before_previous = ' ';
        for scalar in text.chars() {
            if is_indic_vowel_sign(scalar as u32) {
                vowel_signs += 1;
                if !is_indic_consonant_or_nukta(previous as u32) {
                    misplaced += 1;
                }
            }
            if before_previous == '\u{0627}'
                && "?.\u{FFFD}".chars().any(|c| c == previous)
                && SCRIPT_ARABIC.contains(&(scalar as u32))
                && scalar.is_alphabetic()
            {
                lost_letters += 1;
            }
            before_previous = previous;
            previous = scalar;
        }
        if vowel_signs >= 10 && misplaced * 20 >= vowel_signs {
            return Some(Self::GlyphOrder);
        }
        if lost_letters >= 2 {
            return Some(Self::LostLetters);
        }
        None
    }
}

fn is_indic_vowel_sign(value: u32) -> bool {
    matches!(
        value,
        0x09BE..=0x09C4
            | 0x09C7
            | 0x09C8
            | 0x09CB
            | 0x09CC
            | 0x09D7
            | 0x09E2
            | 0x09E3
            | 0x093A
            | 0x093B
            | 0x093E..=0x094C
            | 0x094E
            | 0x094F
            | 0x0955..=0x0957
            | 0x0962
            | 0x0963
    )
}

fn is_indic_consonant_or_nukta(value: u32) -> bool {
    matches!(
        value,
        0x0995..=0x09B9
            | 0x09BC
            | 0x09CE
            | 0x09DC
            | 0x09DD
            | 0x09DF
            | 0x09F0
            | 0x09F1
            | 0x0915..=0x0939
            | 0x093C
            | 0x0958..=0x095F
            | 0x0979..=0x097F
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_folds_what_is_typed_differently() {
        assert_eq!(SearchText::normalized("الْمَكْتَبَةَ"), "المكتبة");
        assert_eq!(SearchText::normalized("إلى الأردن"), "الى الاردن");
        assert_eq!(SearchText::normalized("مـــرحبا"), "مرحبا");
        assert_eq!(SearchText::normalized("\u{FEFB}"), "\u{0644}\u{0627}");
        assert_eq!(SearchText::normalized("\u{FB01}le"), "file");
        assert_eq!(SearchText::normalized("র\u{200D}্যাব"), "র্যাব");
        assert_eq!(SearchText::normalized("\u{09C7}\u{09BE}"), "\u{09CB}");
        assert_eq!(SearchText::normalized("a\u{200B}b"), "a b");
    }

    #[test]
    fn garbled_page_text_is_noticed() {
        let glyph_order =
            "বািড় ভাড়ার চিপ ধানমি ৭ নর রােডর িতন বডরেমর াট মািসক ২৫,০০০ টাকা ভাড়ায় দওয়া হেলা। ভাড়ায়া দুই মােসর";
        let bengali =
            "বাড়ি ভাড়ার চুক্তিপত্র। ধানমন্ডি ৭ নম্বর রোডের তিন বেডরুমের ফ্ল্যাটটি মাসিক ২৫,০০০ টাকা ভাড়ায় দেওয়া হলো।";
        let lost = "ارتفعت ا.بيعات بنسبة خمسة عشر با.ئة في الربع الثالث. زرنا مدينة البتراء ا?ثرية في جنوب ا?ردن.";
        let arabic = "ارتفعت المبيعات بنسبة خمسة عشر بالمئة في الربع الثالث. زرنا مدينة البتراء الأثرية في جنوب الأردن.";
        assert_eq!(
            PageTextProblem::check(glyph_order),
            Some(PageTextProblem::GlyphOrder)
        );
        assert_eq!(PageTextProblem::check(bengali), None);
        assert_eq!(
            PageTextProblem::check(lost),
            Some(PageTextProblem::LostLetters)
        );
        assert_eq!(PageTextProblem::check(arabic), None);
        assert_eq!(
            PageTextProblem::check("Revenue grew 20%. See p.4?No. Done."),
            None
        );
    }

    #[test]
    fn landmark_finds_the_page() {
        let pages = [
            "Quarterly business review\nRevenue grew in Japan and Europe.",
            "Hiking trip to Mount Fuji\nWe took the Yoshida trail from the fifth station in Japan.",
        ];
        assert_eq!(
            Landmark::find(2, &pages, &["japan", "Yoshida"]),
            Some("Yoshida".into())
        );
        assert_eq!(
            Landmark::find(2, &pages, &["japan", "bread"]),
            Some("Japan".into())
        );
        assert_eq!(Landmark::find(2, &pages, &["bread"]), None);
        assert_eq!(Landmark::find(1, &pages, &["japan"]), None);
        let arabic = [
            "التقرير الربعي",
            "رحلة إلى البتراء\nزرنا مدينة البتراء الأثرية في جنوب الأردن",
        ];
        assert_eq!(Landmark::find(2, &arabic, &["الاردن"]), Some("الأردن".into()));
        let garbled = ["التقرير الربعي", "زرنا مدينة البتراء ا?ثرية في جنوب ا?ردن"];
        assert_eq!(Landmark::find(2, &garbled, &["الاردن"]), None);
    }

    #[test]
    fn matches_are_shown_as_written() {
        let arabic = "زرنا مدينة البتراء الأثرية في جنوب الأردن.";
        let found: Vec<_> = SearchText::matches("الاردن", arabic)
            .into_iter()
            .map(|r| &arabic[r])
            .collect();
        assert_eq!(found, ["الأردن"]);
        let voweled = "إِنَّ الْمَكْتَبَةَ تُغْلَقُ";
        let found: Vec<_> = SearchText::matches("المكتبة", voweled)
            .into_iter()
            .map(|r| &voweled[r])
            .collect();
        assert_eq!(found, ["الْمَكْتَبَةَ"]);
        let bengali = "একটি বড় কুমিরের ছবি";
        let found: Vec<_> = SearchText::matches("কুমির", bengali)
            .into_iter()
            .map(|r| &bengali[r])
            .collect();
        assert_eq!(found, ["কুমিরের"]);
        let english = "Sunday trip: day two";
        let found: Vec<_> = SearchText::matches("day", english)
            .into_iter()
            .map(|r| &english[r])
            .collect();
        assert_eq!(found, ["day"]);
        let turkish = "İ".repeat(60) + " son kelime";
        let found: Vec<_> = SearchText::matches("kelime", &turkish)
            .into_iter()
            .map(|r| &turkish[r])
            .collect();
        assert_eq!(found, ["kelime"]);
    }

    #[test]
    fn snippets_quote_the_text_as_written() {
        let text = "قَالَ مُحَمَّدٌ إِنَّ الْمَكْتَبَةَ الْعَامَّةَ تُغْلَقُ فِي السَّاعَةِ الثَّامِنَةِ مَسَاءً يَوْمَ الْخَمِيسِ";
        let snippet = SearchText::snippet(text, &["المكتبة"], 12);
        assert!(
            snippet.as_deref().is_some_and(|s| s.contains("«الْمَكْتَبَةَ»")),
            "snippet={snippet:?}"
        );
        assert_eq!(SearchText::snippet(text, &["كلمة"], 12), None);
    }

    #[test]
    fn real_arabic_abbreviations_are_not_lost_letters() {
        assert_eq!(
            PageTextProblem::check(
                "أ.د.محمد علي، أستاذ في الجامعة. د.خالد و د.سارة من شركة ذ.م.م للتجارة"
            ),
            None
        );
        let bullets = "\u{F0B7} Item one of the slide\n".repeat(6);
        assert_eq!(PageTextProblem::check(&bullets), None);
    }
}
