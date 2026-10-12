mod index_service;
mod parity;
mod worker;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand, ValueEnum};
use trowel_core::{
    CodeQuery, Costs, Crawler, Embedder, Estimator, FakeEmbedder, FileKind, IndexOptions,
    IndexStore, Indexer, ModelFiles, SharedEmbedder, SqlValue,
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
    /// Keyword search (vector search is Phase 4).
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
    /// Keyword-only recall until Phase 4 wires Searcher.
    Eval {
        file: PathBuf,
        #[arg(long)]
        index: Option<PathBuf>,
        #[arg(long)]
        json: bool,
        #[arg(short = 'n', long, default_value_t = 10)]
        limit: usize,
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
        } => {
            if let Err(err) = cmd_search(
                &query.join(" "),
                index.unwrap_or_else(default_index_dir),
                json,
                limit,
                kind.as_deref(),
                code,
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
        } => {
            if let Err(err) = cmd_eval(&file, index.unwrap_or_else(default_index_dir), json, limit)
            {
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
    if !fake {
        indexer.embedder_source = crate::worker::worker_source(default_backend());
        indexer.check_model().map_err(|e| e.to_string())?;
    }
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

fn cmd_search(
    query: &str,
    index: PathBuf,
    json: bool,
    limit: usize,
    kind: Option<&str>,
    code: bool,
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
    let mut sql = String::from(
        "SELECT f.path, f.kind, s.kind, s.loc, s.excerpt, bm25(fts) \
         FROM fts JOIN segments s ON s.id = fts.rowid JOIN files f ON f.id = s.file \
         WHERE fts MATCH ?",
    );
    let mut values = vec![SqlValue::Text(fts_query(&q))];
    if let Some(kind) = kind {
        sql.push_str(" AND f.kind = ?");
        values.push(SqlValue::Text(kind.into()));
    }
    sql.push_str(" ORDER BY bm25(fts) LIMIT ?");
    values.push(SqlValue::Int(limit as i64));
    let mut rows = Vec::new();
    store
        .db
        .query(&sql, &values, |row| {
            rows.push(serde_json::json!({
                "path": row.text(0).unwrap_or_default(),
                "kind": row.text(1).unwrap_or_default(),
                "segment": row.text(2).unwrap_or_default(),
                "loc": row.double(3),
                "excerpt": row.text(4).unwrap_or_default(),
                "bm25": row.double(5),
            }));
            Ok(())
        })
        .map_err(|e| e.to_string())?;
    if json {
        println!("{}", serde_json::to_string_pretty(&rows).unwrap());
    } else if rows.is_empty() {
        println!("no good matches");
    } else {
        for (i, row) in rows.iter().enumerate() {
            println!(
                "{:>2}. {:<10} {}  [{}]",
                i + 1,
                row["kind"].as_str().unwrap_or(""),
                Path::new(row["path"].as_str().unwrap_or(""))
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                row["excerpt"].as_str().unwrap_or("")
            );
        }
        println!("(keyword search; vector search is Phase 4)");
    }
    Ok(())
}

fn fts_query(query: &str) -> String {
    query
        .split_whitespace()
        .map(|w| format!("\"{}\"", w.replace('"', "")))
        .collect::<Vec<_>>()
        .join(" ")
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

fn cmd_eval(file: &Path, index: PathBuf, json: bool, limit: usize) -> Result<(), String> {
    #[derive(serde::Deserialize)]
    struct Query {
        q: String,
        expect: Vec<String>,
        tag: Option<String>,
        kind: Option<String>,
    }
    #[derive(serde::Deserialize)]
    struct EvalFile {
        queries: Vec<Query>,
    }
    let raw = std::fs::read_to_string(file).map_err(|e| e.to_string())?;
    let spec: EvalFile = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    let store = IndexStore::open(&index).map_err(|e| e.to_string())?;
    let mut r1 = 0;
    let mut r5 = 0;
    let mut r10 = 0;
    let n = spec.queries.len().max(1);
    for q in &spec.queries {
        let mut hits = Vec::new();
        store
            .db
            .query(
                "SELECT f.path FROM fts JOIN segments s ON s.id = fts.rowid JOIN files f ON f.id = s.file \
                 WHERE fts MATCH ? ORDER BY bm25(fts) LIMIT ?",
                &[
                    SqlValue::Text(fts_query(&q.q)),
                    SqlValue::Int(limit as i64),
                ],
                |row| {
                    hits.push(row.text(0).unwrap_or_default());
                    Ok(())
                },
            )
            .ok();
        let found = hits.iter().position(|p| {
            q.expect.iter().any(|e| {
                let e = e.split('@').next().unwrap_or(e);
                p.ends_with(e) || p.contains(e)
            })
        });
        if found == Some(0) {
            r1 += 1;
        }
        if found.is_some_and(|i| i < 5) {
            r5 += 1;
        }
        if found.is_some_and(|i| i < 10) {
            r10 += 1;
        }
        let _ = &q.tag;
        let _ = &q.kind;
    }
    let body = serde_json::json!({
        "mode": "keyword",
        "note": "vector eval is Phase 4",
        "n": spec.queries.len(),
        "r@1": r1 as f64 / n as f64,
        "r@5": r5 as f64 / n as f64,
        "r@10": r10 as f64 / n as f64,
    });
    if json {
        println!("{body}");
    } else {
        println!(
            "keyword-only (Phase 4 does vectors): R@1={:.2} R@5={:.2} R@10={:.2} n={}",
            r1 as f64 / n as f64,
            r5 as f64 / n as f64,
            r10 as f64 / n as f64,
            spec.queries.len()
        );
    }
    Ok(())
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
