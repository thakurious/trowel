mod mock;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "troweld", about = "Trowel daemon")]
struct Args {
    /// Serve canned PROTOCOL v1 responses (Phase 1 UI track).
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
}

fn main() -> ExitCode {
    let args = Args::parse();
    if !args.mock {
        eprintln!("troweld is not implemented yet (Phase 5)");
        return ExitCode::from(2);
    }
    let opts = mock::MockOpts {
        socket: args.socket,
        fixture: args.fixture,
        latency_ms: args.latency_ms,
    };
    if let Err(err) = mock::run(opts) {
        eprintln!("troweld --mock: {err}");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}
