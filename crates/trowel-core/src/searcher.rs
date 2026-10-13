//! Hybrid meaning + words search.
//! `upstream/Sources/DigUpKit/Searcher.swift`

use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use crate::database::{DbError, SqlValue};
use crate::extract::{excerpt, CodeLookup};
use crate::index_store::IndexStore;
use crate::model::{FileKind, IndexOptions, Modality, SegmentKind};
use crate::search_text::{is_word_character, SearchText};
use crate::vectors::{VectorStorage, DIMENSION};

/// One ranked file. `upstream/Sources/DigUpKit/Searcher.swift:4`
#[derive(Debug, Clone)]
pub struct SearchHit {
    pub path: String,
    pub kind: FileKind,
    /// Fused rank score, for ordering only.
    pub score: f64,
    /// How far the best segment stands out for this query (`meaning`).
    pub z: Option<f64>,
    pub cosine: Option<f64>,
    /// 1-based rank among keyword matches.
    pub keyword_rank: Option<i32>,
    pub segment: SegmentKind,
    pub segment_id: i64,
    pub loc: Option<f64>,
    pub loc_end: Option<f64>,
    pub excerpt: Option<String>,
    pub modified: f64,
    pub size: i64,
    pub info: HashMap<String, String>,
    /// Code: the line to open at in the stretch.
    pub focus: Option<f64>,
}

/// Hybrid search: meaning (EmbeddingGemma's vectors) + words (FTS5).
/// `upstream/Sources/DigUpKit/Searcher.swift:32`
pub struct Searcher {
    pub store: IndexStore,
    matrix: Option<VectorStorage>,
    row_file: Vec<i64>,
    row_segment: Vec<i64>,
    row_modality: Vec<Modality>,
    /// False for later-pass vectors (`Indexer.finishLongFiles`): scored, but they don't set the scale.
    /// `upstream/Sources/DigUpKit/Searcher.swift:38`
    row_sets_scale: Vec<bool>,
    file_kind: HashMap<i64, FileKind>,
    pub vector_source: Option<String>,
    /// False after `release_vectors` (or `load_vectors: false`).
    pub vectors_loaded: bool,
}

impl Searcher {
    /// How much finding the query's words counts against meaning. Code's words count for less.
    /// `upstream/Sources/DigUpKit/Searcher.swift:113`
    pub const WORD_WEIGHT: f64 = 2.0;
    pub const CODE_WORD_WEIGHT: f64 = 1.0;
    pub const Z_GATE: f64 = 2.0;

    pub fn new(store: IndexStore, load_vectors: bool) -> Result<Self, DbError> {
        let mut searcher = Self {
            store,
            matrix: None,
            row_file: Vec::new(),
            row_segment: Vec::new(),
            row_modality: Vec::new(),
            row_sets_scale: Vec::new(),
            file_kind: HashMap::new(),
            vector_source: None,
            vectors_loaded: false,
        };
        if load_vectors {
            searcher.reload()?;
        } else {
            searcher.reload_files()?;
        }
        Ok(searcher)
    }

    pub fn vector_count(&self) -> usize {
        self.row_file.len()
    }

    /// This is the code index's searcher (`IndexStore::CODE_FILE`).
    pub fn is_code(&self) -> bool {
        self.store.file.file_name().and_then(|n| n.to_str()) == Some(IndexStore::CODE_FILE)
    }

    /// Reads the files and all vectors again (after indexing added some).
    /// `upstream/Sources/DigUpKit/Searcher.swift:59`
    pub fn reload(&mut self) -> Result<(), DbError> {
        self.release_vectors();
        self.reload_files()?;
        let mut rows = 0i64;
        self.store.db.query(
            "SELECT COUNT(*) FROM segments WHERE vec IS NOT NULL",
            &[],
            |row| {
                rows = row.int(0);
                Ok(())
            },
        )?;
        let rows = rows as usize;
        let Some(mut matrix) = VectorStorage::new(rows) else {
            self.vectors_loaded = true;
            return Ok(());
        };
        let row_bytes = DIMENSION * 2;
        let steps = IndexOptions::default();
        let mut written = 0usize;
        self.store.db.query(
            "SELECT s.id, s.file, s.modality, s.vec, s.kind, s.loc, f.kind, \
             CAST(json_extract(f.info, '$.pages') AS INTEGER) \
             FROM segments s JOIN files f ON f.id = s.file WHERE s.vec IS NOT NULL ORDER BY s.id",
            &[],
            |row| {
                let Some(blob) = row.blob(3) else {
                    return Ok(());
                };
                if blob.len() != row_bytes || written >= rows {
                    return Ok(());
                }
                matrix.write_row(written, &blob);
                self.row_segment.push(row.int(0));
                self.row_file.push(row.int(1));
                self.row_modality.push(
                    row.text(2)
                        .as_deref()
                        .and_then(|s| Modality::from_str(s).ok())
                        .unwrap_or(Modality::None),
                );
                // Past the first step of a long PDF (its pages) or document (its chunks).
                // `upstream/Sources/DigUpKit/Searcher.swift:80`
                let kind = row.text(4).unwrap_or_default();
                let place = if row.is_null(5) {
                    0
                } else {
                    row.double(5) as i32
                };
                let later = if row.text(6).as_deref() == Some("pdf") {
                    kind != SegmentKind::Chunk.as_str()
                        && row.int(7) > i64::from(steps.big_pdf_pages)
                        && place > steps.pdf_step
                } else {
                    kind == SegmentKind::Chunk.as_str() && place > steps.doc_step
                };
                self.row_sets_scale.push(!later);
                written += 1;
                Ok(())
            },
        )?;
        self.matrix = Some(matrix);
        self.vectors_loaded = true;
        Ok(())
    }

    /// Frees the vectors' memory; keyword search keeps working.
    pub fn release_vectors(&mut self) {
        self.matrix = None;
        self.row_file.clear();
        self.row_segment.clear();
        self.row_modality.clear();
        self.row_sets_scale.clear();
        self.vectors_loaded = false;
    }

    fn reload_files(&mut self) -> Result<(), DbError> {
        self.file_kind.clear();
        self.store
            .db
            .query("SELECT id, kind FROM files", &[], |row| {
                if let Some(kind) = row
                    .text(1)
                    .as_deref()
                    .and_then(|s| FileKind::from_str(s).ok())
                {
                    self.file_kind.insert(row.int(0), kind);
                }
                Ok(())
            })?;
        self.vector_source = self.store.meta("vector_source")?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn search(
        &mut self,
        text: &str,
        query_vector: Option<&[f32]>,
        kinds: Option<&HashSet<FileKind>>,
        limit: usize,
        z_gate: f64,
        word_weight: Option<f64>,
        prefix_last: bool,
    ) -> Result<Vec<SearchHit>, DbError> {
        let word_weight = word_weight.unwrap_or(if self.is_code() {
            Self::CODE_WORD_WEIGHT
        } else {
            Self::WORD_WEIGHT
        });
        if query_vector.is_some() && !self.vectors_loaded {
            self.reload()?;
        }
        let allowed = |file: i64| {
            kinds.is_none_or(|set| self.file_kind.get(&file).is_some_and(|k| set.contains(k)))
        };

        let mut best: HashMap<i64, (f64, f64, usize)> = HashMap::new();
        if let Some(query) = query_vector {
            if query.len() == DIMENSION && !self.row_file.is_empty() {
                let scores = self.score(query);
                let meaning =
                    Self::meaning(&scores, &self.row_modality, Some(&self.row_sets_scale));
                for row in 0..scores.len() {
                    if !allowed(self.row_file[row]) {
                        continue;
                    }
                    let z = meaning[row];
                    let better = best
                        .get(&self.row_file[row])
                        .is_none_or(|(old, _, _)| z > *old);
                    if better {
                        best.insert(self.row_file[row], (z, f64::from(scores[row]), row));
                    }
                }
            }
        }

        let terms = Self::terms(text);
        let identifier = if self.is_code() {
            CodeLookup::identifier(text)
        } else {
            None
        };
        #[derive(Clone)]
        struct WordHit {
            file: i64,
            segment: i64,
            snippet: Option<String>,
        }
        let mut words: Vec<WordHit> = Vec::new();
        let mut coverage: HashMap<i64, f64> = HashMap::new();
        let patterns = identifier
            .as_ref()
            .map(|id| vec![CodeLookup::phrase(id)])
            .unwrap_or_else(|| Self::patterns(&terms, prefix_last));
        if !patterns.is_empty() {
            let mut seen = HashSet::new();
            let match_sql = patterns.join(" OR ");
            self.store.db.query(
                "SELECT s.file, s.id, snippet(fts, 1, '«', '»', '…', 12), fts.original FROM fts \
                 JOIN segments s ON s.id = fts.rowid \
                 WHERE fts MATCH ? ORDER BY bm25(fts, 3.0, 1.0) LIMIT 1000",
                &[SqlValue::Text(match_sql)],
                |row| {
                    let file = row.int(0);
                    if words.len() >= 100 || !allowed(file) || !seen.insert(file) {
                        return Ok(());
                    }
                    let snippet = row
                        .text(3)
                        .as_deref()
                        .and_then(|original| {
                            let refs: Vec<&str> = terms.iter().map(String::as_str).collect();
                            SearchText::snippet(original, &refs, 12)
                        })
                        .or_else(|| row.text(2));
                    words.push(WordHit {
                        file,
                        segment: row.int(1),
                        snippet,
                    });
                    Ok(())
                },
            )?;
            if !words.is_empty() {
                let files = words
                    .iter()
                    .map(|w| w.file.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                let n = patterns.len() as f64;
                for pattern in &patterns {
                    self.store.db.query(
                        &format!(
                            "SELECT DISTINCT s.file FROM fts JOIN segments s ON s.id = fts.rowid \
                             WHERE fts MATCH ? AND s.file IN ({files})"
                        ),
                        &[SqlValue::Text(pattern.clone())],
                        |row| {
                            *coverage.entry(row.int(0)).or_insert(0.0) += 1.0 / n;
                            Ok(())
                        },
                    )?;
                }
            }
        }

        // A file qualifies by standing out in meaning, or by covering ≥ half the query's words.
        // Fusion beat RRF (2026-10-07): R@1 0.88 vs 0.73 synthetic, 0.86 vs 0.82 real.
        // `upstream/Sources/DigUpKit/Searcher.swift:165`
        let lookup = Self::is_lookup(&terms) || identifier.is_some();
        let (meaning_weight, words_weight) = if lookup {
            (0.5, word_weight * 2.0)
        } else {
            (1.0, word_weight)
        };
        let hinted = Self::kind_hints(text);
        let mut candidates: HashSet<i64> = best
            .iter()
            .filter(|(_, (z, _, _))| *z >= z_gate)
            .map(|(file, _)| *file)
            .collect();
        for (file, share) in &coverage {
            if *share >= 0.5 {
                candidates.insert(*file);
            }
        }
        let mut scored: HashMap<i64, f64> = HashMap::new();
        for file in candidates {
            let mut score = meaning_weight
                * best.get(&file).map(|(z, _, _)| z.max(0.0)).unwrap_or(0.0)
                + words_weight * coverage.get(&file).copied().unwrap_or(0.0).powf(1.5);
            if let Some(kind) = self.file_kind.get(&file) {
                if hinted.contains(kind) {
                    score += 1.0;
                }
            }
            scored.insert(file, score);
        }

        let mut word_rank: HashMap<i64, (i32, WordHit)> = HashMap::new();
        for (i, w) in words.into_iter().enumerate() {
            word_rank.entry(w.file).or_insert((i as i32 + 1, w));
        }

        let mut ordered: Vec<(i64, f64)> = scored.into_iter().collect();
        ordered.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });

        let mut hits = Vec::new();
        for (file, score) in ordered.into_iter().take(limit) {
            let rank = hits.len();
            let semantic = best.get(&file).copied();
            let keyword = word_rank.get(&file).cloned();
            let use_meaning = semantic
                .map(|(z, _, _)| z >= z_gate || keyword.is_none())
                .unwrap_or(false);
            let Some(mut sid) = (if use_meaning {
                semantic.map(|(_, _, row)| self.row_segment[row])
            } else {
                keyword.as_ref().map(|k| k.1.segment)
            }) else {
                continue;
            };
            let mut focus = None;
            if self.is_code() {
                let words_here =
                    keyword.as_ref().is_some_and(|k| k.1.segment == sid) || identifier.is_some();
                let placed = self.code_place(
                    file,
                    sid,
                    identifier.as_deref(),
                    text,
                    &terms,
                    words_here,
                    rank < 10,
                )?;
                sid = placed.0;
                focus = placed.1;
            }
            let mut path = String::new();
            let mut kind = FileKind::Doc;
            let mut modified = 0.0;
            let mut size = 0i64;
            let mut info = HashMap::new();
            self.store.db.query(
                "SELECT path, kind, mtime, size, info FROM files WHERE id = ?",
                &[SqlValue::Int(file)],
                |row| {
                    path = row.text(0).unwrap_or_default();
                    kind = row
                        .text(1)
                        .as_deref()
                        .and_then(|s| FileKind::from_str(s).ok())
                        .unwrap_or(FileKind::Doc);
                    modified = row.double(2);
                    size = row.int(3);
                    if let Some(json) = row.text(4) {
                        if let Ok(map) = serde_json::from_str::<HashMap<String, String>>(&json) {
                            info = map;
                        }
                    }
                    Ok(())
                },
            )?;
            let mut segment = SegmentKind::Name;
            let mut loc = None;
            let mut loc_end = None;
            let mut stored_excerpt = None;
            self.store.db.query(
                "SELECT kind, loc, loc_end, excerpt FROM segments WHERE id = ?",
                &[SqlValue::Int(sid)],
                |row| {
                    segment = row
                        .text(0)
                        .as_deref()
                        .and_then(|s| SegmentKind::from_str(s).ok())
                        .unwrap_or(SegmentKind::Name);
                    loc = if row.is_null(1) {
                        None
                    } else {
                        Some(row.double(1))
                    };
                    loc_end = if row.is_null(2) {
                        None
                    } else {
                        Some(row.double(2))
                    };
                    stored_excerpt = row.text(3);
                    Ok(())
                },
            )?;
            // Quote matched words only when they come from the segment we point at.
            // `upstream/Sources/DigUpKit/Searcher.swift:218`
            let mut snippet = keyword
                .as_ref()
                .filter(|k| k.1.segment == sid)
                .and_then(|k| k.1.snippet.clone())
                .filter(|s| s.contains('«'))
                .map(|s| excerpt(&s, 400));
            if snippet.is_none() && stored_excerpt.is_none() && loc.is_none() {
                if let Some(match_kw) = keyword.as_ref() {
                    if match_kw
                        .1
                        .snippet
                        .as_deref()
                        .is_some_and(|t| t.contains('«'))
                    {
                        let mut matched_kind = None;
                        self.store.db.query(
                            "SELECT kind FROM segments WHERE id = ?",
                            &[SqlValue::Int(match_kw.1.segment)],
                            |row| {
                                matched_kind = row.text(0);
                                Ok(())
                            },
                        )?;
                        if matched_kind.as_deref() != Some(SegmentKind::Name.as_str()) {
                            snippet = match_kw.1.snippet.as_deref().map(|t| excerpt(t, 400));
                        }
                    }
                }
            }
            hits.push(SearchHit {
                path,
                kind,
                score,
                z: semantic.map(|s| s.0),
                cosine: semantic.map(|s| s.1),
                keyword_rank: keyword.map(|k| k.0),
                segment,
                segment_id: sid,
                loc,
                loc_end,
                excerpt: snippet.or(stored_excerpt),
                modified,
                size,
                info,
                focus,
            });
        }
        Ok(hits)
    }

    #[allow(clippy::too_many_arguments)]
    fn code_place(
        &self,
        file: i64,
        segment: i64,
        identifier: Option<&[String]>,
        query: &str,
        terms: &[String],
        words_here: bool,
        look_up_name: bool,
    ) -> Result<(i64, Option<f64>), DbError> {
        let name = query.trim().replace("()", "");
        if look_up_name {
            if let Some(identifier) = identifier {
                let mut best: Option<(i64, i32, i32)> = None;
                self.store.db.query(
                    "SELECT s.id, s.loc, coalesce(fts.original, fts.body) FROM fts \
                     JOIN segments s ON s.id = fts.rowid \
                     WHERE fts MATCH ? AND s.file = ? ORDER BY s.loc LIMIT 300",
                    &[
                        SqlValue::Text(CodeLookup::phrase(identifier)),
                        SqlValue::Int(file),
                    ],
                    |row| {
                        if best.is_some_and(|b| b.2 >= 2) {
                            return Ok(());
                        }
                        let Some(text) = row.text(2) else {
                            return Ok(());
                        };
                        let Some((line, strength)) =
                            CodeLookup::definition(&name, &text, row.double(1) as i32)
                        else {
                            return Ok(());
                        };
                        if strength > best.map(|b| b.2).unwrap_or(0) {
                            best = Some((row.int(0), line, strength));
                        }
                        Ok(())
                    },
                )?;
                if let Some(best) = best {
                    return Ok((best.0, Some(f64::from(best.1))));
                }
            }
        }
        if !words_here && identifier.is_none() {
            return Ok((segment, None));
        }
        let Some(text) = self.text_of_segment(segment)? else {
            return Ok((segment, None));
        };
        let mut first = 1i32;
        self.store.db.query(
            "SELECT loc FROM segments WHERE id = ?",
            &[SqlValue::Int(segment)],
            |row| {
                first = row.double(0) as i32;
                Ok(())
            },
        )?;
        let lowered;
        let words: Vec<&str> = if identifier.is_none() {
            terms.iter().map(String::as_str).collect()
        } else {
            lowered = name.to_lowercase();
            vec![lowered.as_str()]
        };
        Ok((
            segment,
            CodeLookup::first_line(&words, &text, first).map(f64::from),
        ))
    }

    /// Codes and IDs and long numbers weigh words double and meaning half.
    /// `upstream/Sources/DigUpKit/Searcher.swift:269`
    pub fn is_lookup(terms: &[String]) -> bool {
        !terms.is_empty()
            && terms.iter().all(|t| {
                t.chars().any(|c| c.is_numeric())
                    && (t.chars().any(|c| c.is_alphabetic()) || t.chars().count() >= 4)
            })
    }

    /// How many of `hits` (best first) are strong: scoring at least `ratio` × the top hit.
    /// `upstream/Sources/DigUpKit/Searcher.swift:278`
    pub fn strong_count(hits: &[SearchHit], ratio: f64) -> usize {
        let Some(top) = hits.first().map(|h| h.score) else {
            return 0;
        };
        if top <= 0.0 {
            return hits.len();
        }
        hits.iter().take_while(|h| h.score >= top * ratio).count()
    }

    fn score(&self, query: &[f32]) -> Vec<f32> {
        self.matrix
            .as_ref()
            .map(|m| m.scores(query))
            .unwrap_or_else(|| vec![0.0; self.row_file.len()])
    }

    /// Per-modality z-score on one shared deviation. Later-pass vectors can be left out of the scale.
    /// `upstream/Sources/DigUpKit/Searcher.swift:316`
    pub fn meaning(scores: &[f32], modality: &[Modality], sets_scale: Option<&[bool]>) -> Vec<f64> {
        let means = Self::modality_means(scores, modality, sets_scale);
        let centered: Vec<f64> = scores
            .iter()
            .zip(modality)
            .map(|(s, m)| f64::from(*s) - means.get(m).copied().unwrap_or(0.0))
            .collect();
        let mut count: f64 = 0.0;
        let mut sum = 0.0;
        for (row, value) in centered.iter().enumerate() {
            if sets_scale.map(|s| s[row]).unwrap_or(true) {
                count += 1.0;
                sum += value;
            }
        }
        let mean = sum / count.max(1.0);
        let mut squares = 0.0;
        for (row, value) in centered.iter().enumerate() {
            if sets_scale.map(|s| s[row]).unwrap_or(true) {
                squares += (value - mean) * (value - mean);
            }
        }
        let deviation = (squares / count.max(1.0)).sqrt().max(0.02);
        centered.iter().map(|v| (v - mean) / deviation).collect()
    }

    /// Average score of each modality. Small groups (< 20) take the average of all.
    /// `upstream/Sources/DigUpKit/Searcher.swift:334`
    pub fn modality_means(
        scores: &[f32],
        modality: &[Modality],
        sets_scale: Option<&[bool]>,
    ) -> HashMap<Modality, f64> {
        let mut sum: HashMap<Modality, f64> = HashMap::new();
        let mut count: HashMap<Modality, i32> = HashMap::new();
        for (row, score) in scores.iter().enumerate() {
            if sets_scale.map(|s| s[row]).unwrap_or(true) {
                *sum.entry(modality[row]).or_insert(0.0) += f64::from(*score);
                *count.entry(modality[row]).or_insert(0) += 1;
            }
        }
        let all_n = count.values().sum::<i32>().max(1);
        let all = sum.values().sum::<f64>() / f64::from(all_n);
        let mut out = HashMap::new();
        for modality in Modality::ALL {
            if let Some(n) = count.get(&modality) {
                if *n >= 20 {
                    out.insert(modality, sum[&modality] / f64::from(*n));
                    continue;
                }
            }
            out.insert(modality, all);
        }
        out
    }

    pub fn text_of_segment(&self, id: i64) -> Result<Option<String>, DbError> {
        let mut text = None;
        self.store.db.query(
            "SELECT coalesce(original, body) FROM fts WHERE rowid = ?",
            &[SqlValue::Int(id)],
            |row| {
                text = row.text(0);
                Ok(())
            },
        )?;
        Ok(text)
    }

    /// Content words of a query. Kind words and stopwords are dropped.
    /// `upstream/Sources/DigUpKit/Searcher.swift:375`
    pub fn terms(text: &str) -> Vec<String> {
        let mut seen = HashSet::new();
        SearchText::normalized(text)
            .to_lowercase()
            .split(|c: char| !is_word_character(c))
            .filter(|w| {
                w.chars().count() >= 2 && !STOPWORDS.contains(w) && seen.insert((*w).to_string())
            })
            .map(str::to_string)
            .collect()
    }

    fn patterns(terms: &[String], prefix_last: bool) -> Vec<String> {
        terms
            .iter()
            .enumerate()
            .map(|(index, term)| {
                let prefix =
                    (prefix_last && index == terms.len() - 1) || SearchText::inflects(term);
                format!("\"{term}\"{}", if prefix { "*" } else { "" })
            })
            .collect()
    }

    /// The query's words as an FTS5 OR-query.
    pub fn fts_query(text: &str, prefix_last: bool) -> Option<String> {
        let words = Self::terms(text);
        if words.is_empty() {
            return None;
        }
        Some(Self::patterns(&words, prefix_last).join(" OR "))
    }

    /// Kinds the query asks for: a soft boost, not a filter.
    /// `upstream/Sources/DigUpKit/Searcher.swift:412`
    pub fn kind_hints(text: &str) -> HashSet<FileKind> {
        SearchText::normalized(text)
            .to_lowercase()
            .split(|c: char| !is_word_character(c))
            .filter_map(|word| {
                if word.is_empty() {
                    return None;
                }
                KIND_WORDS
                    .iter()
                    .find(|(key, _)| *key == word)
                    .or_else(|| {
                        KIND_WORDS.iter().find(|(key, _)| {
                            SearchText::inflects(key)
                                && key.chars().count() >= 3
                                && word.starts_with(key)
                        })
                    })
                    .map(|(_, kind)| *kind)
            })
            .collect()
    }
}

/// `upstream/Sources/DigUpKit/Searcher.swift:360`
const STOPWORDS: &[&str] = &[
    "a",
    "an",
    "the",
    "of",
    "in",
    "on",
    "at",
    "to",
    "for",
    "with",
    "and",
    "or",
    "my",
    "me",
    "that",
    "this",
    "these",
    "those",
    "from",
    "about",
    "where",
    "what",
    "which",
    "who",
    "when",
    "is",
    "was",
    "are",
    "were",
    "be",
    "it",
    "its",
    "some",
    "any",
    "find",
    "show",
    "file",
    "files",
    "photo",
    "photos",
    "picture",
    "image",
    "images",
    "screenshot",
    "screenshots",
    "video",
    "videos",
    "pdf",
    "document",
    "documents",
    "recording",
    "এবং",
    "ও",
    "এই",
    "সেই",
    "একটি",
    "আমার",
    "থেকে",
    "জন্য",
    "সাথে",
    "মধ্যে",
    "কোথায়",
    "কোন",
    "কী",
    "কি",
    "যে",
    "ছবি",
    "স্ক্রিনশট",
    "ভিডিও",
    "ফাইল",
    "পিডিএফ",
    "في",
    "من",
    "على",
    "الى",
    "عن",
    "مع",
    "هذا",
    "هذه",
    "ذلك",
    "التي",
    "الذي",
    "اين",
    "ما",
    "صورة",
    "صور",
    "لقطة",
    "شاشة",
    "فيديو",
    "ملف",
    "مستند",
];

/// `upstream/Sources/DigUpKit/Searcher.swift:398`
const KIND_WORDS: &[(&str, FileKind)] = &[
    ("screenshot", FileKind::Screenshot),
    ("screenshots", FileKind::Screenshot),
    ("photo", FileKind::Image),
    ("photos", FileKind::Image),
    ("picture", FileKind::Image),
    ("pictures", FileKind::Image),
    ("image", FileKind::Image),
    ("images", FileKind::Image),
    ("pdf", FileKind::Pdf),
    ("pdfs", FileKind::Pdf),
    ("paper", FileKind::Pdf),
    ("papers", FileKind::Pdf),
    ("document", FileKind::Doc),
    ("documents", FileKind::Doc),
    ("note", FileKind::Doc),
    ("notes", FileKind::Doc),
    ("video", FileKind::Video),
    ("videos", FileKind::Video),
    ("clip", FileKind::Video),
    ("movie", FileKind::Video),
    ("audio", FileKind::Audio),
    ("song", FileKind::Audio),
    ("podcast", FileKind::Audio),
    ("voice", FileKind::Audio),
    ("sound", FileKind::Audio),
    ("স্ক্রিনশট", FileKind::Screenshot),
    ("ছবি", FileKind::Image),
    ("পিডিএফ", FileKind::Pdf),
    ("নোট", FileKind::Doc),
    ("ভিডিও", FileKind::Video),
    ("অডিও", FileKind::Audio),
    ("গান", FileKind::Audio),
    ("রেকর্ডিং", FileKind::Audio),
    ("لقطة", FileKind::Screenshot),
    ("صورة", FileKind::Image),
    ("صور", FileKind::Image),
    ("مستند", FileKind::Doc),
    ("ملاحظات", FileKind::Doc),
    ("فيديو", FileKind::Video),
    ("مقطع", FileKind::Video),
    ("صوت", FileKind::Audio),
    ("تسجيل", FileKind::Audio),
    ("اغنية", FileKind::Audio),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embedder::FakeEmbedder;
    use crate::model::Candidate;
    use crate::vectors::{VectorStorage, Vectors};

    fn temp() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "trowel-searcher-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn candidate(path: &str, kind: FileKind, inode: u64) -> Candidate {
        Candidate {
            path: path.into(),
            kind,
            size: 1,
            mtime: 1.0,
            inode,
            device: 1,
        }
    }

    fn hit(score: f64) -> SearchHit {
        SearchHit {
            path: format!("/{score}"),
            kind: FileKind::Image,
            score,
            z: None,
            cosine: None,
            keyword_rank: None,
            segment: SegmentKind::Image,
            segment_id: 1,
            loc: None,
            loc_end: None,
            excerpt: None,
            modified: 0.0,
            size: 0,
            info: HashMap::new(),
            focus: None,
        }
    }

    #[test]
    fn fts_query_drops_stopwords_and_kind_words() {
        assert_eq!(
            Searcher::fts_query("the screenshot of my payment error", false).as_deref(),
            Some("\"payment\" OR \"error\"")
        );
        assert_eq!(
            Searcher::fts_query("zebr", true).as_deref(),
            Some("\"zebr\"*")
        );
        assert_eq!(Searcher::fts_query("a screenshot", false), None);
    }

    #[test]
    fn query_words_of_languages_with_endings_are_prefixes() {
        assert_eq!(
            Searcher::fts_query("কুমির", false).as_deref(),
            Some("\"কুমির\"*")
        );
        assert_eq!(
            Searcher::fts_query("مدرسة الأطفال", false).as_deref(),
            Some("\"مدرسة\"* OR \"الاطفال\"*")
        );
        assert_eq!(Searcher::fts_query("মা", false).as_deref(), Some("\"মা\"*"));
        assert_eq!(
            Searcher::fts_query("payment error", false).as_deref(),
            Some("\"payment\" OR \"error\"")
        );
        assert_eq!(
            Searcher::kind_hints("জেব্রার ভিডিওতে"),
            HashSet::from([FileKind::Video])
        );
        assert_eq!(
            Searcher::kind_hints("صورة حمار وحشي"),
            HashSet::from([FileKind::Image])
        );
    }

    #[test]
    fn bengali_and_arabic_words_stay_whole() {
        let dir = temp();
        let store = IndexStore::open(&dir).unwrap();
        let id = store
            .insert(&candidate("/notes/trip.txt", FileKind::Doc, 1))
            .unwrap();
        store
            .add_segment(
                id,
                SegmentKind::Chunk,
                Modality::Text,
                Some(1.0),
                None,
                None,
                Some("সুন্দরবনে আমরা কুমিরও দেখেছি। قَالَ إِنَّ الْمَكْتَبَةَ تُغْلَقُ"),
                None,
            )
            .unwrap();
        let mut searcher = Searcher::new(store, false).unwrap();
        let bengali = searcher
            .search("কুমির", None, None, 20, 2.0, None, false)
            .unwrap();
        assert_eq!(
            bengali.iter().map(|h| h.path.as_str()).collect::<Vec<_>>(),
            ["/notes/trip.txt"]
        );
        assert!(
            bengali
                .first()
                .and_then(|h| h.excerpt.as_deref())
                .is_some_and(|e| e.contains("«কুমিরও»")),
            "excerpt={:?}",
            bengali.first().and_then(|h| h.excerpt.as_ref())
        );
        assert_eq!(
            searcher
                .search("المكتبة", None, None, 20, 2.0, None, false)
                .unwrap()
                .iter()
                .map(|h| h.path.as_str())
                .collect::<Vec<_>>(),
            ["/notes/trip.txt"]
        );
        assert!(searcher
            .search("কলকাতা", None, None, 20, 2.0, None, false)
            .unwrap()
            .is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn meaning_takes_out_each_modalitys_level_on_one_scale() {
        let text: Vec<f32> = (0..30).map(|i| 0.70 + (i % 3) as f32 * 0.01).collect();
        let image: Vec<f32> = (0..30).map(|i| 0.55 + (i % 3) as f32 * 0.002).collect();
        let scores: Vec<f32> = text.into_iter().chain(image).collect();
        let modality: Vec<Modality> = std::iter::repeat_n(Modality::Text, 30)
            .chain(std::iter::repeat_n(Modality::Image, 30))
            .collect();
        let z = Searcher::meaning(&scores, &modality, None);
        assert!(z[1].abs() < 1e-4 && z[31].abs() < 1e-4);
        assert!((z[2] / z[32] - 5.0).abs() < 0.01);
        let means = Searcher::modality_means(&scores, &modality, None);
        assert!((means[&Modality::Text] - 0.71).abs() < 0.001);
        assert!((means[&Modality::Image] - 0.552).abs() < 0.001);
        assert!((means[&Modality::Audio] - 0.631).abs() < 0.001);
    }

    #[test]
    fn later_pages_are_scored_without_moving_the_scale() {
        let scores: Vec<f32> = (0..30)
            .map(|i| 0.70 + (i % 3) as f32 * 0.01)
            .chain((0..30).map(|i| 0.55 + (i % 3) as f32 * 0.002))
            .collect();
        let modality: Vec<Modality> = std::iter::repeat_n(Modality::Text, 30)
            .chain(std::iter::repeat_n(Modality::Image, 30))
            .collect();
        let book: Vec<f32> = (0..400)
            .map(|i| 0.60 + (i % 5) as f32 * 0.004)
            .chain(std::iter::once(0.80))
            .collect();
        let z = Searcher::meaning(&scores, &modality, None);
        let mut with_scores = scores.clone();
        with_scores.extend_from_slice(&book);
        let mut with_mod = modality.clone();
        with_mod.extend(std::iter::repeat_n(Modality::Text, book.len()));
        let mut sets = vec![true; 60];
        sets.extend(std::iter::repeat_n(false, 401));
        let with_book = Searcher::meaning(&with_scores, &with_mod, Some(&sets));
        for row in 0..60 {
            assert!((z[row] - with_book[row]).abs() < 1e-9);
        }
        assert!(
            with_book.last().copied().unwrap()
                > z.iter().copied().fold(f64::NEG_INFINITY, f64::max)
        );
    }

    #[test]
    fn codes_and_long_numbers_are_lookups() {
        assert!(Searcher::is_lookup(&["k7q2lm".into()]));
        assert!(Searcher::is_lookup(&["trx50".into()]));
        assert!(Searcher::is_lookup(&["4815162342".into()]));
        assert!(Searcher::is_lookup(&["2041".into()]));
        assert!(!Searcher::is_lookup(&["417".into()]));
        assert!(!Searcher::is_lookup(&["inv".into(), "2041".into()]));
        assert!(!Searcher::is_lookup(&[]));
    }

    #[test]
    fn weaker_matches_fold_below_half_the_top_score() {
        assert_eq!(
            Searcher::strong_count(&[hit(6.0), hit(4.1), hit(3.0), hit(2.9), hit(0.7)], 0.5),
            3
        );
        assert_eq!(Searcher::strong_count(&[hit(0.71)], 0.5), 1);
        assert_eq!(Searcher::strong_count(&[], 0.5), 0);
    }

    #[test]
    fn lookup_query_finds_the_code() {
        let dir = temp();
        let store = IndexStore::open(&dir).unwrap();
        let id = store
            .insert(&candidate("/d/shot.png", FileKind::Screenshot, 1))
            .unwrap();
        store
            .add_segment(
                id,
                SegmentKind::Ocr,
                Modality::Text,
                None,
                None,
                Some("Booking K7Q2LM"),
                Some("Booking K7Q2LM"),
                None,
            )
            .unwrap();
        let mut searcher = Searcher::new(store, false).unwrap();
        let hits = searcher
            .search("K7Q2LM", None, None, 20, 2.0, None, false)
            .unwrap();
        assert_eq!(
            hits.iter().map(|h| h.path.as_str()).collect::<Vec<_>>(),
            ["/d/shot.png"]
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn half_vectors_convert_back_exactly() {
        let vector: Vec<f32> = (0..DIMENSION)
            .map(|i| (i as f32 * 0.37).sin() * 0.05)
            .collect();
        let mut storage = VectorStorage::new(2).unwrap();
        storage.write_row(1, &Vectors::half(&vector));
        let scores_block_unused = storage.halves();
        assert_eq!(scores_block_unused.len(), 2 * DIMENSION * 2);
        let mut back = vec![0f32; DIMENSION];
        Vectors::floats_from_half(&storage.halves()[DIMENSION * 2..], &mut back);
        let expected: Vec<f32> = vector
            .iter()
            .map(|x| half::f16::from_f32(*x).to_f32())
            .collect();
        assert_eq!(back, expected);
        let mut first = vec![0f32; DIMENSION];
        Vectors::floats_from_half(&storage.halves()[..DIMENSION * 2], &mut first);
        assert!(first.iter().all(|x| *x == 0.0));
    }

    #[test]
    fn meaning_match_ranks_the_right_vector() {
        let dir = temp();
        let store = IndexStore::open(&dir).unwrap();
        let a = store
            .insert(&candidate("/a.png", FileKind::Image, 1))
            .unwrap();
        let b = store
            .insert(&candidate("/b.png", FileKind::Image, 2))
            .unwrap();
        let target = FakeEmbedder::vector_for("needle");
        store
            .add_segment(
                a,
                SegmentKind::Image,
                Modality::Image,
                None,
                None,
                None,
                None,
                Some(&FakeEmbedder::vector_for("other")),
            )
            .unwrap();
        store
            .add_segment(
                b,
                SegmentKind::Image,
                Modality::Image,
                None,
                None,
                None,
                None,
                Some(&target),
            )
            .unwrap();
        let mut searcher = Searcher::new(store, true).unwrap();
        // Two vectors: z of a perfect match is 1.0 (below the usual 2.0 gate). Rank, not the gate.
        let hits = searcher
            .search("", Some(&target), None, 3, 0.0, None, false)
            .unwrap();
        assert_eq!(hits.first().map(|h| h.path.as_str()), Some("/b.png"));
        assert!((hits.first().and_then(|h| h.cosine).unwrap_or(0.0) - 1.0).abs() < 0.002);
        let _ = std::fs::remove_dir_all(dir);
    }
}
