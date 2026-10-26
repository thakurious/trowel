//! Keep the index in step, then extract and embed. `upstream/Sources/DigUpKit/Indexer.swift`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread; // prepare/commit overlap

use thiserror::Error;

use crate::crawler::Crawler;
use crate::database::DbError;
use crate::embedder::{Embedder, Paced};
use crate::extract::{
    excerpt, AudioExtractor, CodeExtractor, CodeFile, DocExtractor, ExtractError, ImageExtractor,
    MediaPiece, Ocr, PdfExtractor, PdfPage, PreparedImage, VideoExtractor,
};
use crate::index_store::{FileRecord, IndexStore};
use crate::model::{FileKind, IndexOptions, Modality, SegmentKind};
use crate::prompts::Prompts;
use crate::search_text::SearchText;

#[derive(Debug, Error)]
pub enum IndexerError {
    #[error(
        "this index was built with different settings or model:\n  index: {index}\n  now:   {now}\n\
         re-run with --reindex (vectors from different settings can't be compared)"
    )]
    FingerprintMismatch { index: String, now: String },
    #[error("worker error: {0}")]
    Worker(String),
    #[error(transparent)]
    Db(#[from] DbError),
    #[error("{0}")]
    Other(String),
}

#[derive(Debug, Clone, Default)]
pub struct SyncSummary {
    pub added: usize,
    pub changed: usize,
    pub moved: usize,
    pub removed: usize,
    pub unchanged: usize,
    pub crawl: crate::crawler::CrawlReport,
    pub queued: Vec<i64>,
}

#[derive(Debug, Clone)]
pub struct IndexEvent {
    pub done: usize,
    pub total: usize,
    pub path: String,
    pub kind: FileKind,
    pub state: String,
    pub seconds: f64,
    pub note: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Step {
    pub done: i32,
    pub total: i32,
    pub files: i32,
    pub path: String,
    pub kind: FileKind,
    pub state: String,
    pub seconds: f64,
    pub note: Option<String>,
}

pub struct Indexer {
    pub store: IndexStore,
    options: IndexOptions,
    make_embedder: Box<dyn Fn() -> Result<Box<dyn Embedder>, String> + Send>,
    worker: Option<Box<dyn Embedder>>,
    scratch: PathBuf,
    pub reset_on_model_change: Option<Box<dyn Fn(String, String) + Send>>,
    pub embedder_source: Option<String>,
    jobs_prepared: u64,
    /// Files whose attempt row is live in this run (so overlapping later steps
    /// of the same long file do not trip the F5c poison journal).
    in_flight: HashSet<i64>,
    /// D28 index speed: the worker is wrapped in a [`Paced`] embedder that rests `pace` × each
    /// request's GPU time (0 = unwrapped). Not part of the fingerprint.
    pace: f64,
}

#[derive(Clone)]
struct Job {
    files: Vec<FileRecord>,
    step: Option<Range<i32>>,
}

struct Outcome {
    state: String,
    detail: Option<String>,
}

struct ReadyImage {
    slot: usize,
    image: PreparedImage,
    ocr: String,
}

enum Payload {
    Nothing,
    Images {
        screenshot: bool,
        ready: Vec<ReadyImage>,
    },
    Pdf {
        pages: Vec<PdfPage>,
        read: Range<i32>,
        total: i32,
        title: Option<String>,
    },
    Doc {
        chunks: Vec<String>,
        read: Range<i32>,
        total: i32,
        characters: i32,
    },
    Audio {
        pieces: Vec<MediaPiece>,
        duration: f64,
    },
    Video {
        frames: Vec<MediaPiece>,
        sound: Vec<MediaPiece>,
        duration: f64,
    },
    Code(CodeFile),
}

struct Prepared {
    batch: Vec<FileRecord>,
    step: Option<Range<i32>>,
    folder: PathBuf,
    outcomes: HashMap<usize, Outcome>,
    payload: Payload,
}

impl Indexer {
    pub fn new(
        store: IndexStore,
        options: IndexOptions,
        make_embedder: Box<dyn Fn() -> Result<Box<dyn Embedder>, String> + Send>,
    ) -> Result<Self, IndexerError> {
        let scratch = std::env::temp_dir()
            .join(format!("trowel-{}", std::process::id()))
            .join(uuid_like());
        std::fs::create_dir_all(&scratch)
            .map_err(|e| IndexerError::Other(format!("scratch: {e}")))?;
        Ok(Self {
            store,
            options,
            make_embedder,
            worker: None,
            scratch,
            reset_on_model_change: None,
            embedder_source: None,
            jobs_prepared: 0,
            in_flight: HashSet::new(),
            pace: 0.0,
        })
    }

    pub fn close_worker(&mut self) {
        if let Some(mut w) = self.worker.take() {
            w.close();
        }
    }

    pub fn finish(&mut self) {
        self.close_worker();
        let _ = std::fs::remove_dir_all(&self.scratch);
    }

    pub fn worker_is_running(&self) -> bool {
        self.worker.is_some()
    }

    /// `upstream/Sources/DigUpKit/Indexer.swift:86`
    pub fn sync(&self, roots: &[PathBuf]) -> Result<SyncSummary, IndexerError> {
        let crawl = Crawler::crawl(roots, &self.options);
        let root_strings: Vec<String> = roots
            .iter()
            .map(|r| r.to_string_lossy().into_owned())
            .collect();
        let existing = self.store.files_under(&root_strings)?;
        let mut by_path: HashMap<String, FileRecord> = HashMap::new();
        let mut by_inode: HashMap<String, FileRecord> = HashMap::new();
        for file in &existing {
            by_path.insert(file.path.clone(), file.clone());
            by_inode.insert(format!("{}:{}", file.device, file.inode), file.clone());
        }
        let seen: HashSet<String> = crawl.candidates.iter().map(|c| c.path.clone()).collect();
        let mut moved_from = HashSet::new();
        let mut summary = SyncSummary {
            crawl,
            ..SyncSummary::default()
        };
        self.store.db.transaction(|| {
            for file in &summary.crawl.candidates {
                if let Some(row) = by_path.get(&file.path) {
                    if row.size != file.size
                        || (row.mtime - file.mtime).abs() > 0.001
                        || row.kind != file.kind
                    {
                        self.store.requeue_as(row.id, file)?;
                        summary.changed += 1;
                        summary.queued.push(row.id);
                    } else {
                        summary.unchanged += 1;
                    }
                } else if let Some(row) = by_inode.get(&format!("{}:{}", file.device, file.inode)) {
                    if !seen.contains(&row.path)
                        && !moved_from.contains(&row.path)
                        && row.size == file.size
                        && (row.mtime - file.mtime).abs() < 0.001
                    {
                        self.store.move_to(row.id, &file.path)?;
                        moved_from.insert(row.path.clone());
                        summary.moved += 1;
                    } else {
                        summary.queued.push(self.store.insert(file)?);
                        summary.added += 1;
                    }
                } else {
                    summary.queued.push(self.store.insert(file)?);
                    summary.added += 1;
                }
            }
            let vanished: Vec<String> = roots
                .iter()
                .filter(|r| !r.exists())
                .map(|r| {
                    let s = r.to_string_lossy();
                    if s.ends_with('/') {
                        s.into_owned()
                    } else {
                        format!("{s}/")
                    }
                })
                .collect();
            for row in &existing {
                if !seen.contains(&row.path)
                    && !moved_from.contains(&row.path)
                    && !vanished.iter().any(|p| row.path.starts_with(p))
                {
                    self.store.delete(row.id)?;
                    summary.removed += 1;
                }
            }
            Ok(())
        })?;
        Ok(summary)
    }

    pub fn run(
        &mut self,
        limit: Option<i64>,
        only: Option<&HashSet<i64>>,
        should_stop: &dyn Fn() -> bool,
        mut on_event: impl FnMut(IndexEvent),
    ) -> Result<BTreeMap<String, i32>, IndexerError> {
        let queue = self.store.pending(limit, only)?;
        let mut states: BTreeMap<String, i32> = BTreeMap::new();
        let mut done = 0usize;
        let total = queue.len();
        let jobs = Self::batches(&queue)
            .into_iter()
            .map(|files| Job { files, step: None })
            .collect();
        self.process(
            jobs,
            should_stop,
            &mut |_, job, results, seconds| {
                for (offset, file) in job.files.iter().enumerate() {
                    let (state, note) = &results[offset];
                    *states.entry(state.clone()).or_default() += 1;
                    on_event(IndexEvent {
                        done: done + offset + 1,
                        total,
                        path: file.path.clone(),
                        kind: file.kind,
                        state: state.clone(),
                        seconds,
                        note: note.clone(),
                    });
                }
                done += job.files.len();
            },
            &|_| false,
        )?;
        Ok(states)
    }

    pub fn finish_long_files(
        &mut self,
        should_stop: &dyn Fn() -> bool,
        mut on_step: impl FnMut(Step),
    ) -> Result<BTreeMap<String, i32>, IndexerError> {
        let mut jobs = Vec::new();
        for file in self.store.unfinished()? {
            let info = self.store.info(file.id)?;
            let size = if file.kind == FileKind::Pdf {
                self.options.pdf_step
            } else {
                self.options.doc_step
            };
            let mut start = info
                .get("read_to")
                .and_then(|s| s.parse().ok())
                .or_else(|| self.store.last_place(file.id).ok().flatten())
                .unwrap_or(0);
            let end = info
                .get("read_end")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0)
                .max(start + 1);
            while start < end {
                jobs.push(Job {
                    files: vec![file.clone()],
                    step: Some(start..start + size.min(end - start)),
                });
                start += size;
            }
        }
        let mut through = Vec::new();
        for job in &jobs {
            let prev = through.last().copied().unwrap_or(0);
            through.push(prev + job.step.as_ref().map(|s| s.end - s.start).unwrap_or(0));
        }
        let total_left = through.last().copied().unwrap_or(0);
        let mut files_left = jobs
            .iter()
            .map(|j| j.files[0].id)
            .collect::<HashSet<_>>()
            .len() as i32;
        let mut states: BTreeMap<String, i32> = BTreeMap::new();
        let finished = std::cell::RefCell::new(HashSet::new());
        self.process(
            jobs,
            should_stop,
            &mut |index, job, results, seconds| {
                let file = &job.files[0];
                let (state, note) = &results[0];
                if state != "partial" && finished.borrow_mut().insert(file.id) {
                    *states.entry(state.clone()).or_default() += 1;
                    files_left -= 1;
                }
                on_step(Step {
                    done: through[index],
                    total: total_left,
                    files: files_left,
                    path: file.path.clone(),
                    kind: file.kind,
                    state: state.clone(),
                    seconds,
                    note: note.clone(),
                });
            },
            &|job| finished.borrow().contains(&job.files[0].id),
        )?;
        Ok(states)
    }

    fn batches(queue: &[FileRecord]) -> Vec<Vec<FileRecord>> {
        let mut batches = Vec::new();
        let mut index = 0;
        while index < queue.len() {
            let mut batch = vec![queue[index].clone()];
            if matches!(batch[0].kind, FileKind::Image | FileKind::Screenshot) {
                while index + batch.len() < queue.len()
                    && batch.len() < 8
                    && queue[index + batch.len()].kind == batch[0].kind
                {
                    batch.push(queue[index + batch.len()].clone());
                }
            }
            batches.push(batch);
            index += batches.last().map(Vec::len).unwrap_or(1);
        }
        batches
    }

    pub fn check_model(&self) -> Result<(), IndexerError> {
        let Some(source) = &self.embedder_source else {
            return Ok(());
        };
        self.adopt(&format!("{} | {}", source, self.options.fingerprint()))
    }

    fn adopt(&self, fingerprint: &str) -> Result<(), IndexerError> {
        let Some(indexed) = self.store.meta("fingerprint")? else {
            return Ok(());
        };
        if indexed == fingerprint {
            return Ok(());
        }
        let Some(reset) = &self.reset_on_model_change else {
            return Err(IndexerError::FingerprintMismatch {
                index: indexed,
                now: fingerprint.to_string(),
            });
        };
        self.store.reset_all()?;
        reset(indexed, fingerprint.to_string());
        Ok(())
    }

    /// D28: the worker started from now on rests `pace` × each request's time (clamped to 0…4).
    pub fn set_pace(&mut self, pace: f64) {
        self.pace = if pace.is_finite() {
            pace.clamp(0.0, 4.0)
        } else {
            0.0
        };
    }

    fn full_worker(&mut self) -> Result<&mut dyn Embedder, IndexerError> {
        if self.worker.is_none() {
            let mut started = (self.make_embedder)().map_err(IndexerError::Worker)?;
            let fingerprint = format!(
                "{} | {}",
                started.info().vector_source(),
                self.options.fingerprint()
            );
            if let Err(err) = self.adopt(&fingerprint) {
                started.close();
                return Err(err);
            }
            self.store.set_meta("fingerprint", Some(&fingerprint))?;
            self.store
                .set_meta("vector_source", Some(&started.info().vector_source()))?;
            self.worker = Some(if self.pace > 0.0 {
                Box::new(Paced::new(started, self.pace))
            } else {
                started
            });
        }
        Ok(self.worker.as_mut().unwrap().as_mut())
    }

    #[allow(clippy::type_complexity)]
    fn process(
        &mut self,
        jobs: Vec<Job>,
        should_stop: &dyn Fn() -> bool,
        committed: &mut dyn FnMut(usize, &Job, &[(String, Option<String>)], f64),
        skip: &dyn Fn(&Job) -> bool,
    ) -> Result<(), IndexerError> {
        let mut ahead: Option<mpsc::Receiver<Prepared>> = None;
        for (index, job) in jobs.iter().enumerate() {
            if should_stop() {
                break;
            }
            let started = std::time::Instant::now();
            let prepared = if let Some(rx) = ahead.take() {
                rx.recv()
                    .map_err(|_| IndexerError::Other("prepare thread died".into()))?
            } else {
                self.prepare_job(job)?
            };
            if index + 1 < jobs.len() {
                ahead = Some(self.prepare_ahead(&jobs[index + 1])?);
            }
            let folder = prepared.folder.clone();
            if skip(job) {
                let _ = std::fs::remove_dir_all(&folder);
                if job.files.len() == 1 {
                    let _ = self.store.clear_attempts(job.files[0].id);
                }
                continue;
            }
            let results = match self.commit(prepared) {
                Ok(r) => r,
                Err(err) => {
                    let _ = std::fs::remove_dir_all(&folder);
                    self.close_worker();
                    return Err(err);
                }
            };
            let _ = std::fs::remove_dir_all(&folder);
            let seconds = started.elapsed().as_secs_f64() / job.files.len().max(1) as f64;
            committed(index, job, &results, seconds);
        }
        if let Some(rx) = ahead {
            if let Ok(prepared) = rx.recv() {
                let _ = std::fs::remove_dir_all(prepared.folder);
            }
        }
        Ok(())
    }

    fn prepare_ahead(&mut self, job: &Job) -> Result<mpsc::Receiver<Prepared>, IndexerError> {
        let skip_slots = self.stamp_attempts(job)?;
        let (tx, rx) = mpsc::channel();
        let job = job.clone();
        let options = self.options.clone();
        self.jobs_prepared += 1;
        let folder = self.scratch.join(format!("batch-{}", self.jobs_prepared));
        thread::spawn(move || {
            let _ = tx.send(prepare(&job, &options, &folder, &skip_slots));
        });
        Ok(rx)
    }

    fn prepare_job(&mut self, job: &Job) -> Result<Prepared, IndexerError> {
        let skip_slots = self.stamp_attempts(job)?;
        self.jobs_prepared += 1;
        let folder = self.scratch.join(format!("batch-{}", self.jobs_prepared));
        let options = self.options.clone();
        Ok(prepare(job, &options, &folder, &skip_slots))
    }

    /// F5c: journal the attempt and commit before extract. n ≥ 2 → skip the file.
    /// Later steps of the same long file share the in-flight attempt so pipelining
    /// does not look like a crash-retry.
    fn stamp_attempts(&mut self, job: &Job) -> Result<HashSet<usize>, IndexerError> {
        let mut skip_slots = HashSet::new();
        for (slot, file) in job.files.iter().enumerate() {
            if self.in_flight.contains(&file.id) {
                continue;
            }
            let n = self.store.begin_attempt(file.id)?;
            if n >= 2 {
                self.store.finish(
                    file.id,
                    "failed",
                    Some("Couldn't be read safely"),
                    &BTreeMap::new(),
                )?;
                skip_slots.insert(slot);
            } else {
                self.in_flight.insert(file.id);
            }
        }
        Ok(skip_slots)
    }

    fn commit(
        &mut self,
        mut prepared: Prepared,
    ) -> Result<Vec<(String, Option<String>)>, IndexerError> {
        let batch = prepared.batch.clone();
        let later = prepared.step.is_some();
        if let Some(step) = prepared.step.clone() {
            if let Some(outcome) = prepared.outcomes.remove(&0) {
                return Ok(vec![self.stop_reading(
                    &batch[0],
                    step.start,
                    &outcome.detail.unwrap_or(outcome.state),
                )?]);
            }
        }
        let mut results = vec![("failed".into(), None); batch.len()];
        for (slot, outcome) in &prepared.outcomes {
            self.store.finish(
                batch[*slot].id,
                &outcome.state,
                outcome.detail.as_deref(),
                &BTreeMap::new(),
            )?;
            results[*slot] = (outcome.state.clone(), outcome.detail.clone());
        }
        let payload = std::mem::replace(&mut prepared.payload, Payload::Nothing);
        match payload {
            Payload::Nothing => {}
            Payload::Images { screenshot, ready } => {
                self.images(&batch, screenshot, ready, &mut results)?;
            }
            Payload::Pdf {
                pages,
                read,
                total,
                title,
            } => {
                results[0] = self.pdf(&batch[0], pages, read, total, title, later)?;
            }
            Payload::Doc {
                chunks,
                read,
                total,
                characters,
            } => {
                results[0] = self.doc(&batch[0], chunks, read, total, characters, later)?;
            }
            Payload::Audio { pieces, duration } => {
                results[0] = self.audio(&batch[0], pieces, duration)?;
            }
            Payload::Video {
                frames,
                sound,
                duration,
            } => {
                results[0] = self.video(&batch[0], frames, sound, duration)?;
            }
            Payload::Code(code) => {
                results[0] = self.code(&batch[0], code)?;
            }
        }
        for (slot, file) in batch.iter().enumerate() {
            if results[slot].0 != "failed" && results[slot].0 != "pending" {
                let _ = self.store.clear_attempts(file.id);
            }
            if results[slot].0 != "partial" {
                self.in_flight.remove(&file.id);
            }
        }
        Ok(results)
    }

    fn stop_reading(
        &self,
        file: &FileRecord,
        place: i32,
        problem: &str,
    ) -> Result<(String, Option<String>), IndexerError> {
        let mut info = self.store.info(file.id)?;
        info.remove("read_to");
        info.remove("read_end");
        let unit = if file.kind == FileKind::Pdf {
            "page"
        } else {
            "chunk"
        };
        self.store.finish(
            file.id,
            "done",
            Some(&format!("read to {unit} {place}, not after: {problem}")),
            &info,
        )?;
        Ok((
            "done".into(),
            Some(format!("stopped before {unit} {}: {problem}", place + 1)),
        ))
    }

    fn images(
        &mut self,
        batch: &[FileRecord],
        screenshot: bool,
        ready: Vec<ReadyImage>,
        results: &mut [(String, Option<String>)],
    ) -> Result<(), IndexerError> {
        if ready.is_empty() {
            return Ok(());
        }
        let budget = if screenshot {
            self.options.screenshot_budget
        } else {
            self.options.photo_budget
        };
        let paths: Vec<PathBuf> = ready.iter().map(|r| r.image.path.clone()).collect();
        let vectors = {
            let worker = self.full_worker()?;
            worker
                .embed_images(&paths, budget)
                .map_err(IndexerError::Worker)?
        };
        self.store.db.transaction(|| {
            for (position, item) in ready.iter().enumerate() {
                let file = &batch[item.slot];
                let Some(vector) = vectors.get(position).and_then(|e| e.vector.as_ref()) else {
                    let err = vectors
                        .get(position)
                        .and_then(|e| e.error.clone())
                        .unwrap_or_else(|| "embed failed".into());
                    self.store
                        .finish(file.id, "failed", Some(&err), &BTreeMap::new())?;
                    results[item.slot] = ("failed".into(), Some(err));
                    continue;
                };
                self.store.add_segment(
                    file.id,
                    SegmentKind::Image,
                    Modality::Image,
                    None,
                    None,
                    None,
                    None,
                    Some(vector),
                )?;
                if !item.ocr.is_empty() {
                    let ex = excerpt(&item.ocr, 160);
                    self.store.add_segment(
                        file.id,
                        SegmentKind::Ocr,
                        Modality::Text,
                        None,
                        None,
                        Some(&ex),
                        Some(&item.ocr),
                        None,
                    )?;
                }
                let mut info = BTreeMap::new();
                info.insert("width".into(), item.image.width.to_string());
                info.insert("height".into(), item.image.height.to_string());
                if let Some(taken) = &item.image.taken {
                    info.insert("taken".into(), taken.clone());
                }
                if screenshot {
                    info.insert("ocr_chars".into(), item.ocr.chars().count().to_string());
                }
                self.store.finish(file.id, "done", None, &info)?;
                results[item.slot] = (
                    "done".into(),
                    screenshot.then(|| format!("ocr {} chars", item.ocr.chars().count())),
                );
            }
            Ok(())
        })?;
        Ok(())
    }

    fn pdf(
        &mut self,
        file: &FileRecord,
        pages: Vec<PdfPage>,
        read: Range<i32>,
        total: i32,
        title: Option<String>,
        later: bool,
    ) -> Result<(String, Option<String>), IndexerError> {
        let end = total.min(self.options.max_pages);
        let unread = read.end < end;
        if pages.is_empty() && !later && !unread {
            self.store
                .finish(file.id, "skipped", Some("no pages"), &BTreeMap::new())?;
            return Ok(("skipped".into(), Some("no pages".into())));
        }
        let title = title.unwrap_or_else(|| stem(&file.path));
        let text_pages: Vec<&PdfPage> = pages.iter().filter(|p| p.image_path.is_none()).collect();
        let word_pages: Vec<&PdfPage> = text_pages
            .iter()
            .copied()
            .filter(|p| SearchText::has_words(&p.text))
            .collect();
        let image_pages: Vec<&PdfPage> = pages.iter().filter(|p| p.image_path.is_some()).collect();
        let garbled_pages: Vec<&PdfPage> = pages.iter().filter(|p| p.garbled.is_some()).collect();
        let mut text_vectors: HashMap<i32, Vec<f32>> = HashMap::new();
        let mut image_vectors = Vec::new();
        let mut garbled_vectors = Vec::new();
        if !word_pages.is_empty() || !image_pages.is_empty() {
            let prompts: Vec<String> = word_pages
                .iter()
                .map(|p| {
                    Prompts::document(Some(&title), &p.text.chars().take(4000).collect::<String>())
                })
                .collect();
            let img_paths: Vec<PathBuf> = image_pages
                .iter()
                .filter_map(|p| p.image_path.clone())
                .collect();
            let garbled_prompts: Vec<String> = garbled_pages
                .iter()
                .map(|p| {
                    Prompts::document(
                        Some(&title),
                        &p.garbled
                            .as_ref()
                            .unwrap()
                            .chars()
                            .take(4000)
                            .collect::<String>(),
                    )
                })
                .collect();
            let budget = self.options.page_budget;
            let (tv, iv, gv) = {
                let worker = self.full_worker()?;
                let tv = if prompts.is_empty() {
                    Vec::new()
                } else {
                    worker.embed_texts(&prompts).map_err(IndexerError::Worker)?
                };
                let iv = if img_paths.is_empty() {
                    Vec::new()
                } else {
                    worker
                        .embed_images(&img_paths, budget)
                        .map_err(IndexerError::Worker)?
                };
                let gv = if garbled_prompts.is_empty() {
                    Vec::new()
                } else {
                    worker
                        .embed_texts(&garbled_prompts)
                        .map_err(IndexerError::Worker)?
                };
                (tv, iv, gv)
            };
            text_vectors = word_pages.iter().map(|p| p.number).zip(tv).collect();
            image_vectors = iv;
            garbled_vectors = gv;
        }
        self.store.db.transaction(|| {
            for (page, vector) in garbled_pages.iter().zip(&garbled_vectors) {
                self.store.add_segment(
                    file.id,
                    SegmentKind::Page,
                    Modality::Text,
                    Some(f64::from(page.number)),
                    None,
                    None,
                    None,
                    Some(vector),
                )?;
            }
            for page in &text_pages {
                let ex = excerpt(&page.text, 160);
                self.store.add_segment(
                    file.id,
                    SegmentKind::Page,
                    Modality::Text,
                    Some(f64::from(page.number)),
                    None,
                    Some(&ex),
                    Some(&page.text),
                    text_vectors.get(&page.number).map(Vec::as_slice),
                )?;
            }
            for (page, embedding) in image_pages.iter().zip(&image_vectors) {
                let ex = if page.text.is_empty() {
                    None
                } else {
                    Some(excerpt(&page.text, 160))
                };
                self.store.add_segment(
                    file.id,
                    SegmentKind::PageImage,
                    Modality::Image,
                    Some(f64::from(page.number)),
                    None,
                    ex.as_deref(),
                    Some(&page.text),
                    embedding.vector.as_deref(),
                )?;
            }
            let mut info = if later {
                self.store.info(file.id)?
            } else {
                BTreeMap::new()
            };
            info.insert("pages".into(), total.to_string());
            let scanned = info
                .get("scanned_pages")
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(0)
                + image_pages.len() as i32;
            info.insert("scanned_pages".into(), scanned.to_string());
            if unread {
                info.insert("read_to".into(), read.end.to_string());
                info.insert("read_end".into(), end.to_string());
            } else {
                info.remove("read_to");
                info.remove("read_end");
            }
            let detail = if unread {
                Some(format!("read to page {} of {total}", read.end))
            } else if end < total {
                Some(format!("the first {end} of {total} pages"))
            } else {
                None
            };
            self.store.finish(
                file.id,
                if unread { "partial" } else { "done" },
                detail.as_deref(),
                &info,
            )?;
            Ok(())
        })?;
        let ocr = pages.iter().filter(|p| p.ocr).count();
        let span = if later {
            format!("pages {}–{} of {total}", read.start + 1, read.end)
        } else {
            format!("{}/{} pages", pages.len(), total)
        };
        let mut note = format!("{span}, {} as images", image_pages.len());
        if ocr > 0 {
            note.push_str(&format!(", {ocr} read by OCR"));
        }
        Ok((if unread { "partial" } else { "done" }.into(), Some(note)))
    }

    fn doc(
        &mut self,
        file: &FileRecord,
        chunks: Vec<String>,
        read: Range<i32>,
        total: i32,
        characters: i32,
        later: bool,
    ) -> Result<(String, Option<String>), IndexerError> {
        let _ = later;
        if total <= 0 {
            self.store
                .finish(file.id, "skipped", Some("no text"), &BTreeMap::new())?;
            return Ok(("skipped".into(), Some("no text".into())));
        }
        let title = stem(&file.path);
        let worded: Vec<usize> = chunks
            .iter()
            .enumerate()
            .filter(|(_, c)| SearchText::has_words(c))
            .map(|(i, _)| i)
            .collect();
        let prompts: Vec<String> = worded
            .iter()
            .map(|&i| Prompts::document(Some(&title), &chunks[i]))
            .collect();
        let vectors = if prompts.is_empty() {
            Vec::new()
        } else {
            self.full_worker()?
                .embed_texts(&prompts)
                .map_err(IndexerError::Worker)?
        };
        let vector: HashMap<usize, Vec<f32>> = worded.into_iter().zip(vectors).collect();
        let end = total.min(self.options.max_chunks);
        let unread = read.end < end;
        self.store.db.transaction(|| {
            for (offset, chunk) in chunks.iter().enumerate() {
                let ex = excerpt(chunk, 160);
                self.store.add_segment(
                    file.id,
                    SegmentKind::Chunk,
                    Modality::Text,
                    Some(f64::from(read.start + offset as i32 + 1)),
                    None,
                    Some(&ex),
                    Some(chunk),
                    vector.get(&offset).map(Vec::as_slice),
                )?;
            }
            let mut info = BTreeMap::new();
            info.insert("chunks".into(), end.to_string());
            info.insert("chars".into(), characters.to_string());
            if unread {
                info.insert("read_to".into(), read.end.to_string());
                info.insert("read_end".into(), end.to_string());
            }
            let detail = if unread {
                Some(format!("read to chunk {} of {end}", read.end))
            } else if end < total {
                Some(format!("the first {end} chunks"))
            } else {
                None
            };
            self.store.finish(
                file.id,
                if unread { "partial" } else { "done" },
                detail.as_deref(),
                &info,
            )?;
            Ok(())
        })?;
        let span = if later {
            format!("chunks {}–{} of {end}", read.start + 1, read.end)
        } else if unread {
            format!("{} of {end} chunks", chunks.len())
        } else {
            format!("{} chunks", chunks.len())
        };
        Ok((if unread { "partial" } else { "done" }.into(), Some(span)))
    }

    fn code(
        &mut self,
        file: &FileRecord,
        code: CodeFile,
    ) -> Result<(String, Option<String>), IndexerError> {
        if code.pieces.is_empty() {
            self.store
                .finish(file.id, "skipped", Some("no code"), &BTreeMap::new())?;
            return Ok(("skipped".into(), Some("no code".into())));
        }
        let worded: Vec<usize> = code
            .pieces
            .iter()
            .enumerate()
            .filter(|(_, p)| SearchText::has_words(&p.text))
            .map(|(i, _)| i)
            .collect();
        let prompts: Vec<String> = worded
            .iter()
            .map(|&i| Prompts::document(Some(&code.title), &code.pieces[i].text))
            .collect();
        let vectors = if prompts.is_empty() {
            Vec::new()
        } else {
            self.full_worker()?
                .embed_texts(&prompts)
                .map_err(IndexerError::Worker)?
        };
        let vector: HashMap<usize, Vec<f32>> = worded.into_iter().zip(vectors).collect();
        self.store.db.transaction(|| {
            let kind = if code.cells.is_none() {
                SegmentKind::Lines
            } else {
                SegmentKind::Cells
            };
            for (offset, piece) in code.pieces.iter().enumerate() {
                let summary = CodeExtractor::summary(&piece.text);
                self.store.add_segment(
                    file.id,
                    kind,
                    Modality::Text,
                    Some(f64::from(piece.first)),
                    Some(f64::from(piece.last)),
                    Some(&summary),
                    Some(&piece.text),
                    vector.get(&offset).map(Vec::as_slice),
                )?;
            }
            let mut info = BTreeMap::new();
            info.insert("lines".into(), code.lines.to_string());
            if let Some(cells) = code.cells {
                info.insert("cells".into(), cells.to_string());
            }
            if let Some(repo) = &code.repo {
                info.insert("repo".into(), repo.clone());
            }
            self.store.finish(file.id, "done", None, &info)?;
            Ok(())
        })?;
        Ok((
            "done".into(),
            Some(format!("{} stretches", code.pieces.len())),
        ))
    }

    fn audio(
        &mut self,
        file: &FileRecord,
        pieces: Vec<MediaPiece>,
        duration: f64,
    ) -> Result<(String, Option<String>), IndexerError> {
        if pieces.is_empty() {
            self.store.finish(
                file.id,
                "skipped",
                Some("silent or shorter than 2 s"),
                &BTreeMap::new(),
            )?;
            return Ok(("skipped".into(), Some("silent or too short".into())));
        }
        let paths: Vec<PathBuf> = pieces.iter().map(|p| p.path.clone()).collect();
        let vectors = self
            .full_worker()?
            .embed_audio(&paths)
            .map_err(IndexerError::Worker)?;
        self.store.db.transaction(|| {
            for (piece, embedding) in pieces.iter().zip(&vectors) {
                self.store.add_segment(
                    file.id,
                    SegmentKind::Audio,
                    Modality::Audio,
                    Some(piece.start),
                    Some(piece.end),
                    None,
                    None,
                    embedding.vector.as_deref(),
                )?;
            }
            let mut info = BTreeMap::new();
            info.insert("duration".into(), format!("{duration:.1}"));
            self.store.finish(file.id, "done", None, &info)?;
            Ok(())
        })?;
        Ok((
            "done".into(),
            Some(format!("{} windows, {} s", pieces.len(), duration as i32)),
        ))
    }

    fn video(
        &mut self,
        file: &FileRecord,
        frames: Vec<MediaPiece>,
        sound: Vec<MediaPiece>,
        duration: f64,
    ) -> Result<(String, Option<String>), IndexerError> {
        if frames.is_empty() && sound.is_empty() {
            self.store.finish(
                file.id,
                "skipped",
                Some("no frames or sound"),
                &BTreeMap::new(),
            )?;
            return Ok(("skipped".into(), Some("no frames or sound".into())));
        }
        let frame_paths: Vec<PathBuf> = frames.iter().map(|p| p.path.clone()).collect();
        let sound_paths: Vec<PathBuf> = sound.iter().map(|p| p.path.clone()).collect();
        let budget = self.options.frame_budget;
        let (frame_vectors, sound_vectors) = {
            let worker = self.full_worker()?;
            let fv = if frame_paths.is_empty() {
                Vec::new()
            } else {
                worker
                    .embed_images(&frame_paths, budget)
                    .map_err(IndexerError::Worker)?
            };
            let sv = if sound_paths.is_empty() {
                Vec::new()
            } else {
                worker
                    .embed_audio(&sound_paths)
                    .map_err(IndexerError::Worker)?
            };
            (fv, sv)
        };
        self.store.db.transaction(|| {
            for (frame, embedding) in frames.iter().zip(&frame_vectors) {
                self.store.add_segment(
                    file.id,
                    SegmentKind::Frame,
                    Modality::Image,
                    Some(frame.start),
                    Some(frame.end),
                    None,
                    None,
                    embedding.vector.as_deref(),
                )?;
            }
            for (piece, embedding) in sound.iter().zip(&sound_vectors) {
                self.store.add_segment(
                    file.id,
                    SegmentKind::Audio,
                    Modality::Audio,
                    Some(piece.start),
                    Some(piece.end),
                    None,
                    None,
                    embedding.vector.as_deref(),
                )?;
            }
            let mut info = BTreeMap::new();
            info.insert("duration".into(), format!("{duration:.1}"));
            info.insert("frames".into(), frames.len().to_string());
            info.insert("sound_windows".into(), sound.len().to_string());
            self.store.finish(file.id, "done", None, &info)?;
            Ok(())
        })?;
        Ok((
            "done".into(),
            Some(format!(
                "{} frames, {} sound windows, {} s",
                frames.len(),
                sound.len(),
                duration as i32
            )),
        ))
    }
}

impl Drop for Indexer {
    fn drop(&mut self) {
        self.finish();
    }
}

fn stem(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn uuid_like() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{t:x}")
}

fn clamp(range: Range<i32>, max: i32) -> Range<i32> {
    let start = range.start.max(0).min(max);
    let end = range.end.max(start).min(max);
    start..end
}

fn prepare(
    job: &Job,
    options: &IndexOptions,
    folder: &Path,
    skip_slots: &HashSet<usize>,
) -> Prepared {
    let batch = job.files.clone();
    let mut prepared = Prepared {
        batch: batch.clone(),
        step: job.step.clone(),
        folder: folder.to_path_buf(),
        outcomes: HashMap::new(),
        payload: Payload::Nothing,
    };
    for &slot in skip_slots {
        prepared.outcomes.insert(
            slot,
            Outcome {
                state: "failed".into(),
                detail: Some("Couldn't be read safely".into()),
            },
        );
    }
    if skip_slots.len() == batch.len() {
        return prepared;
    }
    let url = PathBuf::from(&batch[0].path);
    if let Err(err) = std::fs::create_dir_all(folder) {
        prepared.outcomes.insert(
            0,
            Outcome {
                state: "failed".into(),
                detail: Some(err.to_string()),
            },
        );
        return prepared;
    }
    let result = match batch[0].kind {
        FileKind::Image | FileKind::Screenshot => {
            let screenshot = batch[0].kind == FileKind::Screenshot;
            let mut ready = Vec::new();
            for (slot, file) in batch.iter().enumerate() {
                if skip_slots.contains(&slot) {
                    continue;
                }
                let path = Path::new(&file.path);
                match ImageExtractor::prepare(
                    path,
                    if screenshot { 1536 } else { 1024 },
                    options.min_image_side,
                    folder,
                    screenshot,
                ) {
                    Ok(image) => {
                        let ocr = if screenshot {
                            Ocr::text(path).unwrap_or_default()
                        } else {
                            String::new()
                        };
                        ready.push(ReadyImage { slot, image, ocr });
                    }
                    Err(err @ ExtractError::TooSmall { .. }) => {
                        prepared.outcomes.insert(
                            slot,
                            Outcome {
                                state: "skipped".into(),
                                detail: Some(err.to_string()),
                            },
                        );
                    }
                    Err(err) => {
                        prepared.outcomes.insert(
                            slot,
                            Outcome {
                                state: "failed".into(),
                                detail: Some(err.to_string()),
                            },
                        );
                    }
                }
            }
            prepared.payload = Payload::Images { screenshot, ready };
            Ok(())
        }
        FileKind::Pdf => {
            let mut wanted = 0..0;
            PdfExtractor::pages(
                &url,
                |total| {
                    wanted = clamp(
                        job.step
                            .clone()
                            .unwrap_or(if total > options.big_pdf_pages {
                                0..options.pdf_step
                            } else {
                                0..options.max_pages
                            }),
                        total.min(options.max_pages),
                    );
                    wanted.clone()
                },
                options.min_page_text,
                1400,
                folder,
            )
            .map(|(pages, total, title)| {
                prepared.payload = Payload::Pdf {
                    pages,
                    read: wanted,
                    total,
                    title,
                };
            })
        }
        FileKind::Doc => DocExtractor::text_of(&url).map(|text| {
            let chunks = DocExtractor::chunks(&text, 1800, 200, (options.max_chunks + 1) as usize);
            let read = clamp(
                job.step.clone().unwrap_or(0..options.doc_step),
                (chunks.len() as i32).min(options.max_chunks),
            );
            let slice = if read.start as usize >= chunks.len() {
                Vec::new()
            } else {
                chunks[read.start as usize..read.end as usize].to_vec()
            };
            prepared.payload = Payload::Doc {
                chunks: slice,
                read,
                total: chunks.len() as i32,
                characters: text.chars().count() as i32,
            };
        }),
        FileKind::Audio => {
            AudioExtractor::windows(&url, options.audio_window, options.audio_hop, 2.0, folder).map(
                |(pieces, duration)| {
                    prepared.payload = Payload::Audio { pieces, duration };
                },
            )
        }
        FileKind::Video => VideoExtractor::keyframes(
            &url,
            options.frame_interval,
            1024,
            200,
            folder,
        )
        .and_then(|(frames, duration, has_audio)| {
            let sound = if has_audio {
                AudioExtractor::windows(&url, options.audio_window, options.audio_hop, 2.0, folder)?
                    .0
            } else {
                Vec::new()
            };
            prepared.payload = Payload::Video {
                frames,
                sound,
                duration,
            };
            Ok(())
        }),
        FileKind::Code => CodeExtractor::read(&url, options).map(|code| {
            prepared.payload = Payload::Code(code);
        }),
    };
    if let Err(err) = result {
        match err {
            ExtractError::LeftOut(why) => {
                prepared.outcomes.insert(
                    0,
                    Outcome {
                        state: "skipped".into(),
                        detail: Some(why),
                    },
                );
            }
            other => {
                for slot in 0..batch.len() {
                    prepared.outcomes.entry(slot).or_insert(Outcome {
                        state: "failed".into(),
                        detail: Some(other.to_string()),
                    });
                }
            }
        }
        prepared.payload = Payload::Nothing;
    }
    prepared
}

#[cfg(test)]
mod fingerprint_tests {
    use super::*;
    use crate::embedder::{Embedder, FakeEmbedder, WorkerInfo};
    use crate::index_store::IndexStore;
    use crate::model::Candidate;
    use crate::model_files::ModelFiles;
    use crate::vectors::DIMENSION;
    use std::sync::atomic::{AtomicU64, Ordering};

    static N: AtomicU64 = AtomicU64::new(0);

    fn temp() -> PathBuf {
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("trowel-fp-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn live_source() -> String {
        WorkerInfo {
            model: "google/embeddinggemma-2".into(),
            revision: ModelFiles::REVISION.into(),
            runtime: "llama.cpp-vectors1".into(),
            dtype: "text-q8_0+mmproj-q8_0".into(),
            dim: DIMENSION,
            text_only: false,
            load_seconds: None,
        }
        .vector_source()
    }

    fn seeded() -> (PathBuf, Indexer) {
        let dir = temp();
        let store = IndexStore::open(&dir).unwrap();
        let id = store
            .insert(&Candidate {
                path: "/a/x.png".into(),
                kind: FileKind::Image,
                size: 10,
                mtime: 1.0,
                inode: 1,
                device: 1,
            })
            .unwrap();
        let vector = FakeEmbedder::vector_for("x");
        store
            .add_segment(
                id,
                SegmentKind::Image,
                Modality::Image,
                None,
                None,
                None,
                None,
                Some(&vector),
            )
            .unwrap();
        store.finish(id, "done", None, &BTreeMap::new()).unwrap();
        let fingerprint = format!(
            "{} | {}",
            live_source(),
            IndexOptions::default().fingerprint()
        );
        store.set_meta("fingerprint", Some(&fingerprint)).unwrap();
        let indexer = Indexer::new(
            store,
            IndexOptions::default(),
            Box::new(|| Ok(Box::new(FakeEmbedder::new()) as Box<dyn Embedder>)),
        )
        .unwrap();
        (dir, indexer)
    }

    #[test]
    fn matching_embedder_source_keeps_vectors() {
        let (dir, mut indexer) = seeded();
        indexer.embedder_source = Some(live_source());
        indexer.reset_on_model_change = Some(Box::new(|_, _| {
            panic!("check_model must not reset a matching fingerprint");
        }));
        indexer.check_model().unwrap();
        assert_eq!(indexer.store.counts().unwrap().vectors, 1);
        indexer.finish();
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn guessed_backend_source_resets_when_callback_is_set() {
        let (dir, mut indexer) = seeded();
        indexer.embedder_source = Some(format!(
            "embeddinggemma-2@{} llama.cpp-vulkan q8_0",
            &ModelFiles::REVISION[..7]
        ));
        let reset = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = reset.clone();
        indexer.reset_on_model_change = Some(Box::new(move |_, _| {
            flag.store(true, Ordering::SeqCst);
        }));
        indexer.check_model().unwrap();
        assert!(reset.load(Ordering::SeqCst));
        assert_eq!(indexer.store.counts().unwrap().vectors, 0);
        assert!(indexer.store.meta("fingerprint").unwrap().is_none());
        indexer.finish();
        let _ = std::fs::remove_dir_all(dir);
    }
}
