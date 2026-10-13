mod index_service;
mod parity;
mod worker;

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use clap::{Parser, Subcommand, ValueEnum};
use trowel_core::{
    CodeQuery, Costs, Crawler, Embedder, Estimator, FakeEmbedder, FileKind, IndexOptions,
    IndexStore, Indexer, ModelFiles, Prompts, SearchHit, Searcher, SegmentKind, SharedEmbedder,
};

use crate::index_service::{IndexService, Part};
use crate::worker::{default_backend, WorkerEmbedder};

#[derive(Parser)]
#[command(name = "trowel", about = "Trowel CLI (Linux port of DigUp)")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Drive trowel-worker against the reference vectors (cosine ≥ 0.999).
    Parity {
        #[arg(long, value_enum, default_value_t = Backend::Vulkan)]
        backend: Backend,
        #[arg(long)]
        worker: Option<PathBuf>,
        #[arg(long)]
        reference: Option<PathBuf>,
    },
    /// Classify files in a folder (counts by kind, no Spotlight).
    Estimate {
        path: Vec<PathBuf>,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        code: bool,
        #[arg(long)]
        include_code: bool,
        #[arg(long)]
        include_datasets: bool,
        #[arg(long)]
        exclude: Vec<PathBuf>,
        #[arg(long)]
        exclude_ext: Vec<String>,
    },
    /// Sync and embed files under these folders.
    Index {
        #[arg(required = true)]
        path: Vec<PathBuf>,
        #[arg(long)]
        index: Option<PathBuf>,
        #[arg(long)]
        code: bool,
        #[arg(long)]
        reindex: bool,
        #[arg(long)]
        fake: bool,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        limit: Option<i64>,
        #[arg(long)]
        include_code: bool,
        #[arg(long)]
        include_datasets: bool,
        #[arg(long)]
        exclude: Vec<PathBuf>,
        #[arg(long)]
        exclude_ext: Vec<String>,
    },
    /// Hybrid search (meaning + words).
    Search {
        #[arg(required = true)]
        query: Vec<String>,
        #[arg(long)]
        index: Option<PathBuf>,
        #[arg(long)]
        json: bool,
        #[arg(short = 'n', long, default_value_t = 10)]
        limit: usize,
        #[arg(long)]
        kind: Option<String>,
        #[arg(long)]
        code: bool,
        #[arg(long)]
        fake: bool,
        #[arg(long, default_value_t = 2.0)]
        zgate: f64,
        #[arg(long)]
        word_weight: Option<f64>,
    },
    /// What's in the index.
    Status {
        #[arg(long)]
        index: Option<PathBuf>,
        #[arg(long)]
        code: bool,
        #[arg(long)]
        json: bool,
    },
    /// Recall of known answers (R@1/5/10, MRR, tags, @range, weaker-fold).
    Eval {
        file: PathBuf,
        #[arg(long)]
        index: Option<PathBuf>,
        #[arg(long)]
        json: bool,
        #[arg(short = 'n', long, default_value_t = 10)]
        limit: usize,
        #[arg(long)]
        fake: bool,
        #[arg(long, default_value_t = 2.0)]
        zgate: f64,
        #[arg(long)]
        word_weight: Option<f64>,
        #[arg(long)]
        bench: bool,
        /// Override the eval file's `base` (Linux testbed lives in the repo, not ~/DigUpTestbed).
        #[arg(long)]
        base: Option<PathBuf>,
    },
    /// The app's indexing helper (JSON lines on stdout).
    IndexService {
        #[arg(required = true)]
        path: Vec<PathBuf>,
        #[arg(long)]
        index: Option<PathBuf>,
        #[arg(long)]
        fake: bool,
        #[arg(long)]
        backfill: bool,
        #[arg(long)]
        prune: bool,
        #[arg(long)]
        code_root: Vec<PathBuf>,
        #[arg(long)]
        code_exclude: Vec<PathBuf>,
        #[arg(long, default_value_t = 50)]
        new_limit: usize,
        #[arg(long, default_value_t = 60.0)]
        linger: f64,
        #[arg(long)]
        include_code: bool,
        #[arg(long)]
        include_datasets: bool,
        #[arg(long)]
        exclude: Vec<PathBuf>,
        #[arg(long)]
        exclude_ext: Vec<String>,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum Backend {
    Vulkan,
    Cpu,
}

impl Backend {
    fn as_str(self) -> &'static str {
        match self {
            Backend::Vulkan => "vulkan",
            Backend::Cpu => "cpu",
        }
    }
}

fn default_index_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("TROWEL_INDEX") {
        return PathBuf::from(dir);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".local/share/trowel/index")
}

fn options_from(
    code: bool,
    include_code: bool,
    include_datasets: bool,
    exclude: &[PathBuf],
    exclude_ext: &[String],
) -> IndexOptions {
    let mut options = IndexOptions {
        code,
        include_code,
        include_datasets,
        ..IndexOptions::default()
    };
    options.excluded_folders = exclude
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    options.excluded_extensions = IndexOptions::extensions(&exclude_ext.join(","));
    options
}

fn make_embedder(fake: bool) -> Result<Box<dyn Embedder>, String> {
    if fake {
        return Ok(Box::new(FakeEmbedder::new()));
    }
    WorkerEmbedder::spawn(default_backend(), false).map(|w| Box::new(w) as Box<dyn Embedder>)
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Parity {
            backend,
            worker,
            reference,
        } => {
            if let Err(err) = parity::run(backend.as_str(), worker, reference) {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
        Commands::Estimate {
            path,
            json,
            code,
            include_code,
            include_datasets,
            exclude,
            exclude_ext,
        } => {
            let path = if path.is_empty() {
                vec![PathBuf::from(".")]
            } else {
                path
            };
            if let Err(err) = estimate::run(
                &path,
                json,
                &options_from(code, include_code, include_datasets, &exclude, &exclude_ext),
            ) {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
        Commands::Index {
            path,
            index,
            code,
            reindex,
            fake,
            json,
            limit,
            include_code,
            include_datasets,
            exclude,
            exclude_ext,
        } => {
            if let Err(err) = cmd_index(
                &path,
                index.unwrap_or_else(default_index_dir),
                options_from(code, include_code, include_datasets, &exclude, &exclude_ext),
                reindex,
                fake,
                json,
                limit,
            ) {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
        Commands::Search {
            query,
            index,
            json,
            limit,
            kind,
            code,
            fake,
            zgate,
            word_weight,
        } => {
            if let Err(err) = cmd_search(
                &query.join(" "),
                index.unwrap_or_else(default_index_dir),
                json,
                limit,
                kind.as_deref(),
                code,
                fake,
                zgate,
                word_weight,
            ) {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
        Commands::Status { index, code, json } => {
            if let Err(err) = cmd_status(index.unwrap_or_else(default_index_dir), code, json) {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
        Commands::Eval {
            file,
            index,
            json,
            limit,
            fake,
            zgate,
            word_weight,
            bench,
            base,
        } => {
            if let Err(err) = cmd_eval(
                &file,
                index.unwrap_or_else(default_index_dir),
                json,
                limit,
                fake,
                zgate,
                word_weight,
                bench,
                base,
            ) {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
        Commands::IndexService {
            path,
            index,
            fake,
            backfill,
            prune,
            code_root,
            code_exclude,
            new_limit,
            linger,
            include_code,
            include_datasets,
            exclude,
            exclude_ext,
        } => {
            let code = cmd_index_service(
                path,
                index.unwrap_or_else(default_index_dir),
                options_from(
                    false,
                    include_code,
                    include_datasets,
                    &exclude,
                    &exclude_ext,
                ),
                fake,
                backfill,
                prune,
                code_root,
                code_exclude,
                new_limit,
                linger,
            );
            std::process::exit(code);
        }
    }
}

mod estimate {
    use super::*;

    pub fn run(paths: &[PathBuf], json: bool, options: &IndexOptions) -> Result<(), String> {
        for path in paths {
            if !path.is_dir() {
                if json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "folder": path.display().to_string(),
                            "readable": false,
                            "seconds": 0,
                            "files": {},
                        })
                    );
                } else {
                    println!("{}    can't read it", path.display());
                }
                continue;
            }
            let report = Crawler::crawl(std::slice::from_ref(path), options);
            let estimate = Estimator::estimate(
                &report.candidates,
                options,
                &Costs::default(),
                report.directories,
            );
            let mut by_kind: BTreeMap<&str, u64> = BTreeMap::new();
            for c in &report.candidates {
                *by_kind.entry(c.kind.as_str()).or_default() += 1;
            }
            if json {
                let body = serde_json::json!({
                    "folder": path.display().to_string(),
                    "path": path.display().to_string(),
                    "readable": true,
                    "files": report.candidates.len(),
                    "by_kind": by_kind,
                    "skipped": report.skipped,
                    "skipped_folders": report.skipped_folders,
                    "datasets": report.datasets,
                    "pdf_pages": estimate.pdf_pages,
                    "pdf_pages_total": estimate.pdf_pages_total,
                    "audio_seconds": estimate.audio_seconds,
                    "video_seconds": estimate.video_seconds,
                    "doc_chunks": estimate.doc_chunks,
                    "seconds": estimate.seconds,
                    "long_files": estimate.long_files,
                    "later_seconds": estimate.later_seconds,
                    "code_stretches": estimate.code_stretches,
                    "bytes": estimate.bytes,
                    "directories": estimate.directories,
                });
                println!("{body}");
            } else {
                println!("{} files", report.candidates.len());
                for (kind, n) in &by_kind {
                    println!("  {kind}: {n}");
                }
                println!(
                    "first pass ~{:.0}s ({} pdf pages, {:.0}s audio, {:.0}s video, {} doc chunks, {} dirs)",
                    estimate.seconds,
                    estimate.pdf_pages,
                    estimate.audio_seconds,
                    estimate.video_seconds,
                    estimate.doc_chunks,
                    estimate.directories
                );
                for (reason, n) in &report.skipped {
                    println!("skipped {reason}: {n}");
                }
            }
        }
        Ok(())
    }
}

fn cmd_index(
    paths: &[PathBuf],
    index: PathBuf,
    options: IndexOptions,
    reindex: bool,
    fake: bool,
    json: bool,
    limit: Option<i64>,
) -> Result<(), String> {
    let file = if options.code {
        IndexStore::CODE_FILE
    } else {
        IndexStore::MAIN_FILE
    };
    let store = IndexStore::open_named(&index, file).map_err(|e| e.to_string())?;
    if reindex {
        store.reset_all().map_err(|e| e.to_string())?;
    }
    let mut indexer = Indexer::new(store, options, Box::new(move || make_embedder(fake)))
        .map_err(|e| e.to_string())?;
    // Fingerprint is written from the live worker in Indexer::full_worker. Do not
    // pre-check a guessed `worker_source()` string — it does not match the
    // worker's `google/embeddinggemma-2@… llama.cpp-vectors1 …` line, so a
    // second `trowel index` would refuse to resume.
    let summary = indexer.sync(paths).map_err(|e| e.to_string())?;
    if json {
        println!(
            "{}",
            serde_json::json!({
                "added": summary.added,
                "changed": summary.changed,
                "moved": summary.moved,
                "removed": summary.removed,
                "unchanged": summary.unchanged,
                "queued": summary.queued.len(),
            })
        );
    } else {
        println!(
            "sync: {} new · {} changed · {} moved · {} removed · {} unchanged",
            summary.added, summary.changed, summary.moved, summary.removed, summary.unchanged
        );
    }
    let states = indexer
        .run(limit, None, &|| false, |event| {
            if !json {
                let name = Path::new(&event.path)
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or(event.path.clone());
                let note = event.note.as_deref().unwrap_or("");
                println!(
                    "[{:>4}/{}] {:<10} {:<7} {:5.2}s  {name} {note}",
                    event.done,
                    event.total,
                    event.kind.as_str(),
                    event.state,
                    event.seconds
                );
            }
        })
        .map_err(|e| e.to_string())?;
    if json {
        println!("{}", serde_json::json!({"states": states}));
    } else if states.is_empty() {
        println!("nothing pending");
    }
    if limit.is_none() && indexer.store.unfinished_count().map(|c| c.0).unwrap_or(0) > 0 {
        let finished = indexer
            .finish_long_files(&|| false, |step| {
                if !json {
                    let name = Path::new(&step.path)
                        .file_name()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or(step.path.clone());
                    println!(
                        "[rest {:>5}/{}] {:<4} {:<7} {:5.2}s  {name}",
                        step.done,
                        step.total,
                        step.kind.as_str(),
                        step.state,
                        step.seconds
                    );
                }
            })
            .map_err(|e| e.to_string())?;
        if json {
            println!("{}", serde_json::json!({"rest": finished}));
        }
    }
    Ok(())
}

fn parse_kinds(kind: Option<&str>) -> Result<Option<HashSet<FileKind>>, String> {
    let Some(list) = kind else {
        return Ok(None);
    };
    let mut set = HashSet::new();
    for part in list.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        set.insert(part.parse().map_err(|_| {
            format!("unknown kind {part}; use screenshot, image, pdf, doc, audio, video")
        })?);
    }
    Ok(Some(set).filter(|s| !s.is_empty()))
}

fn query_vectors(
    queries: &[String],
    searcher: &Searcher,
    prompt: fn(&str) -> String,
    fake: bool,
) -> Result<Option<Vec<Vec<f32>>>, String> {
    if searcher.vector_count() == 0 || queries.is_empty() {
        return Ok(None);
    }
    let prompted: Vec<String> = queries.iter().map(|q| prompt(q)).collect();
    let use_fake = fake
        || searcher
            .vector_source
            .as_deref()
            .is_some_and(|s| s.starts_with("fake@"));
    if use_fake {
        let mut embedder = FakeEmbedder::new();
        return Ok(Some(
            embedder.embed_texts(&prompted).map_err(|e| e.to_string())?,
        ));
    }
    let mut worker = WorkerEmbedder::spawn(default_backend(), true)?;
    if let Some(source) = &searcher.vector_source {
        let now = worker.info().vector_source();
        if source != &now {
            worker.close();
            return Err(format!(
                "index vectors come from {source} but the query encoder is {now}"
            ));
        }
    }
    let vectors = worker.embed_texts(&prompted).map_err(|e| e.to_string())?;
    worker.close();
    Ok(Some(vectors))
}

#[allow(clippy::too_many_arguments)]
fn cmd_search(
    query: &str,
    index: PathBuf,
    json: bool,
    limit: usize,
    kind: Option<&str>,
    code: bool,
    fake: bool,
    zgate: f64,
    word_weight: Option<f64>,
) -> Result<(), String> {
    let mut q = query.to_string();
    let code_q = CodeQuery::text_of(&q);
    if let Some(rest) = &code_q {
        q = rest.clone();
    }
    if q.is_empty() {
        return Err("search for what?".into());
    }
    let file = if code || code_q.is_some() {
        IndexStore::CODE_FILE
    } else {
        IndexStore::MAIN_FILE
    };
    let store = IndexStore::open_named(&index, file).map_err(|e| e.to_string())?;
    let mut searcher = Searcher::new(store, true).map_err(|e| e.to_string())?;
    let started = Instant::now();
    let prompt = if code_q.is_some() {
        Prompts::code_query
    } else {
        Prompts::query
    };
    let vector =
        query_vectors(&[q.clone()], &searcher, prompt, fake)?.and_then(|v| v.into_iter().next());
    let encoded = started.elapsed();
    let kinds = parse_kinds(kind)?;
    let search_started = Instant::now();
    let hits = searcher
        .search(
            &q,
            vector.as_deref(),
            kinds.as_ref(),
            limit,
            zgate,
            word_weight,
            false,
        )
        .map_err(|e| e.to_string())?;
    let searched = search_started.elapsed();
    if json {
        let rows: Vec<serde_json::Value> = hits.iter().map(hit_json).collect();
        println!("{}", serde_json::to_string_pretty(&rows).unwrap());
        return Ok(());
    }
    if hits.is_empty() {
        println!("no good matches");
    }
    for (rank, hit) in hits.iter().enumerate() {
        let why = [
            hit.z.map(|z| format!("z {z:.1}")),
            hit.keyword_rank.map(|r| format!("words #{r}")),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(", ");
        let place = location(hit);
        let name = Path::new(&hit.path)
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| hit.path.clone());
        println!(
            "{:>2}. {:<10} {name}{}   ({why})",
            rank + 1,
            hit.kind.as_str(),
            if place.is_empty() {
                String::new()
            } else {
                format!("  [{place}]")
            }
        );
        let parent = Path::new(&hit.path)
            .parent()
            .map(|p| tilde(&p.display().to_string()))
            .unwrap_or_default();
        println!("      {parent}");
        if let Some(excerpt) = &hit.excerpt {
            if !excerpt.is_empty() {
                let shown: String = excerpt.chars().take(160).collect();
                println!("      “{shown}”");
            }
        }
    }
    println!(
        "({} vectors · query encoder {:.2} s incl. start-up · search {:.0} ms)",
        searcher.vector_count(),
        encoded.as_secs_f64(),
        searched.as_secs_f64() * 1000.0
    );
    Ok(())
}

fn hit_json(hit: &SearchHit) -> serde_json::Value {
    let mut row = serde_json::json!({
        "path": hit.path,
        "kind": hit.kind.as_str(),
        "segment": hit.segment.as_str(),
        "score": hit.score,
    });
    if let Some(z) = hit.z {
        row["z"] = serde_json::json!(z);
    }
    if let Some(cosine) = hit.cosine {
        row["cosine"] = serde_json::json!(cosine);
    }
    if let Some(rank) = hit.keyword_rank {
        row["keyword_rank"] = serde_json::json!(rank);
    }
    if let Some(loc) = hit.loc {
        row["loc"] = serde_json::json!(loc);
    }
    if let Some(end) = hit.loc_end {
        row["loc_end"] = serde_json::json!(end);
    }
    if let Some(focus) = hit.focus {
        row["focus"] = serde_json::json!(focus);
    }
    if let Some(excerpt) = &hit.excerpt {
        row["excerpt"] = serde_json::json!(excerpt);
    }
    row
}

fn tilde(path: &str) -> String {
    if let Ok(home) = std::env::var("HOME") {
        if let Some(rest) = path.strip_prefix(&home) {
            return format!("~{rest}");
        }
    }
    path.to_string()
}

fn clock(seconds: f64) -> String {
    let total = seconds as i64;
    if total >= 3600 {
        format!(
            "{}:{:02}:{:02}",
            total / 3600,
            (total / 60) % 60,
            total % 60
        )
    } else {
        format!("{}:{:02}", total / 60, total % 60)
    }
}

fn location(hit: &SearchHit) -> String {
    match hit.segment {
        SegmentKind::Page | SegmentKind::PageImage => hit
            .loc
            .map(|l| format!("p. {}", l as i64))
            .unwrap_or_default(),
        SegmentKind::Audio | SegmentKind::Frame => hit
            .loc
            .map(|l| {
                format!(
                    "at {}{}",
                    clock(l),
                    if hit.segment == SegmentKind::Audio {
                        " (sound)"
                    } else {
                        ""
                    }
                )
            })
            .unwrap_or_default(),
        SegmentKind::Chunk => hit
            .loc
            .map(|l| format!("part {}", l as i64))
            .unwrap_or_default(),
        SegmentKind::Lines => hit
            .loc
            .map(|l| {
                format!(
                    "lines {}–{}{}",
                    l as i64,
                    hit.loc_end.unwrap_or(l) as i64,
                    hit.focus
                        .map(|f| format!(", opens at {}", f as i64))
                        .unwrap_or_default()
                )
            })
            .unwrap_or_default(),
        SegmentKind::Cells => hit
            .loc
            .map(|l| {
                if Some(l) == hit.loc_end {
                    format!("cell {}", l as i64)
                } else {
                    format!("cells {}–{}", l as i64, hit.loc_end.unwrap_or(l) as i64)
                }
            })
            .unwrap_or_default(),
        SegmentKind::Ocr => "text in image".into(),
        _ => String::new(),
    }
}

fn cmd_status(index: PathBuf, code: bool, json: bool) -> Result<(), String> {
    let file = if code {
        IndexStore::CODE_FILE
    } else {
        IndexStore::MAIN_FILE
    };
    let store = IndexStore::open_named(&index, file).map_err(|e| e.to_string())?;
    let counts = store.counts().map_err(|e| e.to_string())?;
    if json {
        println!(
            "{}",
            serde_json::json!({
                "index": store.file.display().to_string(),
                "vectors": counts.vectors,
                "segments": counts.segments,
                "unfinished": counts.unfinished,
                "left": counts.left,
                "by_kind_and_state": counts.by_kind_and_state,
                "fingerprint": store.meta("fingerprint").ok().flatten(),
            })
        );
        return Ok(());
    }
    println!("index: {}", store.file.display());
    println!(
        "vectors: {} in {} segments",
        counts.vectors, counts.segments
    );
    for kind in FileKind::ALL {
        if let Some(states) = counts.by_kind_and_state.get(kind.as_str()) {
            let total: usize = states.values().sum();
            let line: String = states
                .iter()
                .map(|(s, n)| format!("{n} {s}"))
                .collect::<Vec<_>>()
                .join(" · ");
            println!("  {:<11} {:>6}   {line}", kind.as_str(), total);
        }
    }
    if counts.unfinished > 0 {
        println!(
            "long files still being read: {} ({} pages and chunks left)",
            counts.unfinished, counts.left
        );
    }
    if let Some(fp) = store.meta("fingerprint").ok().flatten() {
        println!("built with: {fp}");
    }
    println!(
        "this trowel: llama.cpp models rev {}",
        &ModelFiles::REVISION[..7]
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn cmd_eval(
    file: &Path,
    index: PathBuf,
    json: bool,
    limit: usize,
    fake: bool,
    zgate: f64,
    word_weight: Option<f64>,
    bench: bool,
    base_override: Option<PathBuf>,
) -> Result<(), String> {
    #[derive(serde::Deserialize)]
    struct Query {
        q: String,
        expect: Vec<String>,
        tag: Option<String>,
        kind: Option<String>,
    }
    #[derive(serde::Deserialize)]
    struct EvalFile {
        #[serde(default)]
        base: String,
        queries: Vec<Query>,
    }
    let raw = std::fs::read_to_string(file).map_err(|e| e.to_string())?;
    let spec: EvalFile = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    let base = base_override
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| expand_tilde(&spec.base));
    let is_code: Vec<bool> = spec
        .queries
        .iter()
        .map(|q| CodeQuery::text_of(&q.q).is_some())
        .collect();
    let texts: Vec<String> = spec
        .queries
        .iter()
        .map(|q| CodeQuery::text_of(&q.q).unwrap_or_else(|| q.q.clone()))
        .collect();
    let mut searchers: std::collections::HashMap<bool, Searcher> = std::collections::HashMap::new();
    let mut vectors: Vec<Option<Vec<f32>>> = vec![None; texts.len()];
    let encode_started = Instant::now();
    for code in [false, true] {
        if !is_code.contains(&code) {
            continue;
        }
        let file_name = if code {
            IndexStore::CODE_FILE
        } else {
            IndexStore::MAIN_FILE
        };
        let store = IndexStore::open_named(&index, file_name).map_err(|e| e.to_string())?;
        let searcher = Searcher::new(store, true).map_err(|e| e.to_string())?;
        let indices: Vec<usize> = texts
            .iter()
            .enumerate()
            .filter(|(i, _)| is_code[*i] == code)
            .map(|(i, _)| i)
            .collect();
        let batch: Vec<String> = indices.iter().map(|i| texts[*i].clone()).collect();
        let prompt = if code {
            Prompts::code_query
        } else {
            Prompts::query
        };
        let embedded = query_vectors(&batch, &searcher, prompt, fake)?;
        if let Some(embedded) = embedded {
            for (index, vector) in indices.into_iter().zip(embedded) {
                vectors[index] = Some(vector);
            }
        }
        searchers.insert(code, searcher);
    }
    let encode_secs = encode_started.elapsed().as_secs_f64();

    #[derive(Clone, Copy, Default)]
    struct Row {
        n: i32,
        at1: i32,
        at5: i32,
        at10: i32,
        mrr: f64,
    }
    let mut by_tag: BTreeMap<String, Row> = BTreeMap::new();
    let mut shown = 0;
    let mut listed = 0;
    let mut folded = 0;
    let mut search_secs = 0.0;
    let mut lines = Vec::new();
    for (index, query) in spec.queries.iter().enumerate() {
        let kinds = query
            .kind
            .as_deref()
            .and_then(|k| k.parse::<FileKind>().ok())
            .map(|k| HashSet::from([k]));
        let searcher = searchers
            .get_mut(&is_code[index])
            .ok_or_else(|| "no searcher".to_string())?;
        let started = Instant::now();
        let hits = searcher
            .search(
                &texts[index],
                vectors[index].as_deref(),
                kinds.as_ref(),
                limit,
                zgate,
                word_weight,
                false,
            )
            .map_err(|e| e.to_string())?;
        search_secs += started.elapsed().as_secs_f64();
        let strong = Searcher::strong_count(&hits, 0.5);
        shown += strong;
        listed += hits.len();
        let targets: Vec<(String, Option<(f64, f64)>)> = query
            .expect
            .iter()
            .map(|entry| {
                let mut parts = entry.splitn(2, '@');
                let rel = parts.next().unwrap_or(entry);
                let range = parts.next().and_then(|span| {
                    let mut nums = span.split('-').filter_map(|s| s.parse::<f64>().ok());
                    Some((nums.next()?, nums.next()?))
                });
                let path = if base.is_empty() {
                    rel.to_string()
                } else {
                    format!("{base}/{rel}")
                };
                (path, range)
            })
            .collect();
        let at = |hit: &SearchHit, lo: f64, hi: f64| -> bool {
            let Some(loc) = hit.loc else {
                return false;
            };
            if hit.segment == SegmentKind::Lines || hit.segment == SegmentKind::Cells {
                loc <= hi && hit.loc_end.unwrap_or(loc) >= lo
            } else {
                loc >= lo && loc <= hi
            }
        };
        let rank = hits.iter().position(|hit| {
            targets.iter().any(|(path, range)| {
                hit.path == *path && range.map(|(lo, hi)| at(hit, lo, hi)).unwrap_or(true)
            })
        });
        if let Some(rank) = rank {
            if rank + 1 > strong {
                folded += 1;
            }
        }
        let tag = query.tag.as_deref().unwrap_or("all");
        for key in [tag, "ALL"] {
            let row = by_tag.entry(key.to_string()).or_default();
            row.n += 1;
            if let Some(rank) = rank {
                let r = rank + 1;
                if r <= 1 {
                    row.at1 += 1;
                }
                if r <= 5 {
                    row.at5 += 1;
                }
                if r <= 10 {
                    row.at10 += 1;
                }
                row.mrr += 1.0 / r as f64;
            }
        }
        let top = hits
            .first()
            .map(|h| {
                let name = Path::new(&h.path)
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| h.path.clone());
                let place = location(h);
                if place.is_empty() {
                    name
                } else {
                    format!("{name} [{place}]")
                }
            })
            .unwrap_or_else(|| "-".into());
        let rank_s = rank
            .map(|r| format!("#{:<3}", r + 1))
            .unwrap_or_else(|| "miss".into());
        lines.push(format!("{rank_s}  {:<11} “{}” top: {top}", tag, query.q));
    }
    let nq = spec.queries.len().max(1) as f64;
    if json {
        let mut tags = serde_json::Map::new();
        for (tag, row) in &by_tag {
            let n = f64::from(row.n);
            tags.insert(
                tag.clone(),
                serde_json::json!({
                    "n": row.n,
                    "r@1": f64::from(row.at1) / n,
                    "r@5": f64::from(row.at5) / n,
                    "r@10": f64::from(row.at10) / n,
                    "mrr": row.mrr / n,
                }),
            );
        }
        println!(
            "{}",
            serde_json::json!({
                "n": spec.queries.len(),
                "tags": tags,
                "fold": {
                    "shown_per_query": shown as f64 / nq,
                    "listed_per_query": listed as f64 / nq,
                    "expected_folded": folded,
                },
                "bench": {
                    "encode_s": encode_secs,
                    "search_s": search_secs,
                    "search_ms_per_query": search_secs * 1000.0 / nq,
                }
            })
        );
    } else {
        for line in lines {
            println!("{line}");
        }
        println!();
        println!(
            "{:<12} {:>3}  {:>5}  {:>5}  {:>5}  {:>5}",
            "tag", "n", "R@1", "R@5", "R@10", "MRR"
        );
        let mut tags: Vec<_> = by_tag.into_iter().collect();
        tags.sort_by(|a, b| {
            let key = |k: &str| {
                if k == "ALL" {
                    "~".to_string()
                } else {
                    k.to_string()
                }
            };
            key(&a.0).cmp(&key(&b.0))
        });
        for (tag, row) in tags {
            let n = f64::from(row.n);
            println!(
                "{:<12} {:>3}  {:5.2}  {:5.2}  {:5.2}  {:5.2}",
                tag,
                row.n,
                f64::from(row.at1) / n,
                f64::from(row.at5) / n,
                f64::from(row.at10) / n,
                row.mrr / n
            );
        }
        println!(
            "weaker-matches fold (below 0.5× the top score): {:.1} of {:.1} results shown per query; {} expected {} folded",
            shown as f64 / nq,
            listed as f64 / nq,
            folded,
            if folded == 1 { "answer" } else { "answers" }
        );
        if bench {
            println!(
                "bench: encode {:.2}s · search {:.0} ms/query ({:.2}s total)",
                encode_secs,
                search_secs * 1000.0 / nq,
                search_secs
            );
        }
    }
    Ok(())
}

fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{home}/{rest}");
        }
    }
    if path == "~" {
        return std::env::var("HOME").unwrap_or_else(|_| path.to_string());
    }
    path.to_string()
}

#[allow(clippy::too_many_arguments)]
fn cmd_index_service(
    path: Vec<PathBuf>,
    index: PathBuf,
    options: IndexOptions,
    fake: bool,
    backfill: bool,
    prune: bool,
    code_root: Vec<PathBuf>,
    code_exclude: Vec<PathBuf>,
    new_limit: usize,
    linger: f64,
) -> i32 {
    let model = SharedEmbedder::new(move || make_embedder(fake));
    let ready = if fake {
        Box::new(|| true) as Box<dyn Fn() -> bool + Send>
    } else {
        Box::new(index_service::model_is_ready) as Box<dyn Fn() -> bool + Send>
    };
    let make_part =
        |file: &str, options: IndexOptions, roots: Vec<PathBuf>| -> Result<Part, String> {
            let store = IndexStore::open_named(&index, file).map_err(|e| e.to_string())?;
            let shared = model.clone();
            let mut indexer = Indexer::new(
                store,
                options.clone(),
                Box::new(move || Ok(Box::new(shared.clone()) as Box<dyn Embedder>)),
            )
            .map_err(|e| e.to_string())?;
            if !fake {
                indexer.embedder_source = crate::worker::worker_source(default_backend());
            }
            let code_flag = options.code;
            indexer.reset_on_model_change = Some(Box::new(move |from, to| {
                index_service::emit(&serde_json::json!({
                    "event": "reindex",
                    "from": from,
                    "to": to,
                    "code": code_flag,
                }));
            }));
            Ok(Part {
                indexer,
                keep_only: prune.then_some(roots.clone()),
                roots,
                code: options.code,
            })
        };
    let main = match make_part(IndexStore::MAIN_FILE, options.clone(), path) {
        Ok(p) => p,
        Err(err) => {
            eprintln!("{err}");
            return 1;
        }
    };
    let code = if code_root.is_empty() {
        None
    } else {
        let mut code_options = options;
        code_options.code = true;
        code_options.excluded_folders = code_exclude
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        match make_part(IndexStore::CODE_FILE, code_options, code_root) {
            Ok(p) => Some(p),
            Err(err) => {
                eprintln!("{err}");
                return 1;
            }
        }
    };
    let service = IndexService::new(main, code, model, ready, backfill, new_limit, linger);
    service.serve()
}
