use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use hypomnema::config::Config;
use hypomnema::indexer::{ScanReport, Scanner};
use hypomnema::logging::{self, BinaryKind};
use hypomnema::shutdown;
use hypomnema::store::Store;

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
        Some(Command::Scan) => {
            do_scan(&config).await?;
            Ok(())
        }
        Some(Command::ConfigValidate) => Ok(()),
    }
}

async fn run_daemon(config: Config) -> Result<()> {
    let pid = std::process::id();
    tracing::info!(
        vault = %config.vault.0.display(),
        data_dir = %config.storage.data_dir.0.display(),
        http_bind = %config.http.bind,
        pid,
        "hmnd: starting daemon"
    );
    tracing::debug!(?config, "hmnd: full configuration");

    do_scan(&config).await?;

    let mut shutdown_rx = shutdown::install();
    let _ = shutdown_rx.wait_for(|v| *v).await;

    tracing::info!("hmnd: drain complete, exiting cleanly");
    Ok(())
}

async fn do_scan(config: &Config) -> Result<ScanReport> {
    let store = Store::open(&config.storage.data_dir.0, &config.storage.index_file)
        .await
        .context("opening store")?;
    let scanner = Scanner::new(config, &store).context("constructing scanner")?;
    let report = scanner.run().await.context("running scan")?;
    tracing::info!(
        "hmnd: scan complete: inserted={} updated={} hash_unchanged={} deleted={} in {:.2}s",
        report.inserted,
        report.updated,
        report.hash_unchanged,
        report.deleted,
        report.duration.as_secs_f64()
    );
    Ok(report)
}
