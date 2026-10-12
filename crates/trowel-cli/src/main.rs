mod parity;

use clap::{Parser, Subcommand, ValueEnum};

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
        worker: Option<std::path::PathBuf>,
        #[arg(long)]
        reference: Option<std::path::PathBuf>,
    },
    /// Classify files in a folder (Phase 2: counts by kind, no Spotlight).
    Estimate {
        #[arg(default_value = ".")]
        path: std::path::PathBuf,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        code: bool,
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
        Commands::Estimate { path, json, code } => {
            if let Err(err) = estimate::run(&path, json, code) {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
    }
}

mod estimate {
    use std::collections::BTreeMap;
    use std::path::Path;
    use trowel_core::{Crawler, IndexOptions};

    pub fn run(path: &Path, json: bool, code: bool) -> Result<(), String> {
        let options = IndexOptions {
            code,
            ..IndexOptions::default()
        };
        let report = Crawler::crawl(&[path.to_path_buf()], &options);
        let mut by_kind: BTreeMap<&str, u64> = BTreeMap::new();
        for c in &report.candidates {
            *by_kind.entry(c.kind.as_str()).or_default() += 1;
        }
        if json {
            let body = serde_json::json!({
                "path": path.display().to_string(),
                "files": report.candidates.len(),
                "by_kind": by_kind,
                "skipped": report.skipped,
                "skipped_folders": report.skipped_folders,
                "datasets": report.datasets,
            });
            println!("{body}");
        } else {
            println!("{} files", report.candidates.len());
            for (kind, n) in &by_kind {
                println!("  {kind}: {n}");
            }
            for (reason, n) in &report.skipped {
                println!("skipped {reason}: {n}");
            }
        }
        Ok(())
    }
}
