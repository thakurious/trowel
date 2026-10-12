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
    }
}
