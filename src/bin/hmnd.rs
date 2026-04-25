use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};

use hypomnema::config::Config;
use hypomnema::logging::{self, BinaryKind};
use hypomnema::shutdown;

#[derive(Debug, Parser)]
#[command(name = "hmnd", version, about = "Hypomnema daemon")]
struct Cli {
    #[arg(short, long, value_name = "PATH", global = true)]
    config: Option<PathBuf>,

    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Walk the vault and reconcile the index without starting servers.
    Scan,
    /// Parse and validate the configuration file, then exit.
    ConfigValidate,
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    let config = match Config::load(cli.config.as_deref()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("hmnd: configuration error: {e:#}");
            return ExitCode::from(3);
        }
    };

    if let Err(e) = logging::init(&config.logging, cli.verbose, BinaryKind::Hmnd) {
        eprintln!("hmnd: error: {e:#}");
        return ExitCode::from(1);
    }

    match dispatch(cli.command, config).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("hmnd: error: {e:#}");
            ExitCode::from(1)
        }
    }
}

async fn dispatch(command: Option<Command>, config: Config) -> Result<()> {
    match command {
        None => run_daemon(config).await,
        Some(Command::Scan) => bail!("hmnd scan: not implemented yet (lands in step 2)"),
        Some(Command::ConfigValidate) => Ok(()),
    }
}

async fn run_daemon(config: Config) -> Result<()> {
    let pid = std::process::id();
    // Binary crate's default tracing target is `hmnd`, but the configured
    // EnvFilter only knows about `hypomnema=*`. Tag the binary's events so
    // they ride the same filter as lib events ("everything we write").
    tracing::info!(
        target: "hypomnema::hmnd",
        vault = %config.vault.0.display(),
        data_dir = %config.storage.data_dir.0.display(),
        http_bind = %config.http.bind,
        pid,
        "hmnd: starting daemon"
    );
    tracing::debug!(target: "hypomnema::hmnd", ?config, "hmnd: full configuration");

    let mut shutdown_rx = shutdown::install();
    let _ = shutdown_rx.wait_for(|v| *v).await;

    // shutdown::install logs signal receipt; this confirms the daemon body
    // finished draining and is about to return cleanly.
    tracing::info!(target: "hypomnema::hmnd", "hmnd: drain complete, exiting cleanly");
    Ok(())
}
