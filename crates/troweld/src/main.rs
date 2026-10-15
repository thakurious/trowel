use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use troweld::{engine, mock};

#[derive(Parser, Debug)]
#[command(name = "troweld", about = "Trowel daemon")]
struct Args {
    /// Serve canned PROTOCOL responses from fixtures (UI track).
    #[arg(long)]
    mock: bool,
    /// Unix socket path. Default: `$XDG_RUNTIME_DIR/trowel/troweld.sock`.
    #[arg(long)]
    socket: Option<PathBuf>,
    /// Fixture JSON with canned searches and a scripted status stream.
    #[arg(long)]
    fixture: Option<PathBuf>,
    /// Artificial search latency, for superseded/cancel goldens.
    #[arg(long, default_value_t = 0)]
    latency_ms: u64,
    /// Index directory (`index.sqlite`). Default from config.toml.
    #[arg(long)]
    index: Option<PathBuf>,
    /// Path to `config.toml`.
    #[arg(long)]
    config: Option<PathBuf>,
    /// Use FakeEmbedder (tests). Never crawls `$HOME`.
    #[arg(long)]
    fake: bool,
    /// Directory for `state.sqlite` (launcher/clipboard state). Default `$XDG_DATA_HOME/trowel`.
    #[arg(long)]
    state_dir: Option<PathBuf>,
}

fn main() -> ExitCode {
    let args = Args::parse();
    if args.mock {
        let opts = mock::MockOpts {
            socket: args.socket,
            fixture: args.fixture,
            latency_ms: args.latency_ms,
        };
        if let Err(err) = mock::run(opts) {
            eprintln!("troweld --mock: {err}");
            return ExitCode::from(1);
        }
        return ExitCode::SUCCESS;
    }
    let opts = engine::EngineOpts {
        socket: args.socket,
        index: args.index,
        config: args.config,
        fake: args.fake,
        state_dir: args.state_dir,
    };
    if let Err(err) = engine::run(opts) {
        eprintln!("troweld: {err}");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}
