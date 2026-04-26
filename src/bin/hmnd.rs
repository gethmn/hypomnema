use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use hypomnema::api;
use hypomnema::config::Config;
use hypomnema::embedding::{Embedder, EmbeddingClient, embed_health_probe};
use hypomnema::indexer::{ScanReport, Scanner};
use hypomnema::logging::{self, BinaryKind};
use hypomnema::outbox::Outbox;
use hypomnema::shutdown;
use hypomnema::store::Store;
use hypomnema::watcher;

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
    let outbox_path = config.storage.data_dir.0.join(&config.storage.outbox_file);
    tracing::info!(
        vault = %config.vault.0.display(),
        data_dir = %config.storage.data_dir.0.display(),
        outbox = %outbox_path.display(),
        http_bind = %config.http.bind,
        debounce_ms = %config.watcher.debounce_ms,
        pid,
        "hmnd: starting daemon"
    );
    tracing::debug!(?config, "hmnd: full configuration");

    let store = Store::open(
        &config.storage.data_dir.0,
        &config.storage.index_file,
        &config.embedding,
    )
    .await
    .context("opening store")?;
    let client =
        EmbeddingClient::new(&config.embedding).context("constructing embedding client")?;
    embed_health_probe(&client, &config.embedding).await;
    let embedder: Arc<dyn Embedder> = Arc::new(client);
    let scanner =
        Scanner::new(&config, &store, embedder.clone()).context("constructing scanner")?;
    let report = scanner.run().await.context("running initial scan")?;
    tracing::info!(
        "hmnd: scan complete: inserted={} updated={} hash_unchanged={} deleted={} in {:.2}s",
        report.inserted,
        report.updated,
        report.hash_unchanged,
        report.deleted,
        report.duration.as_secs_f64()
    );

    let outbox = Outbox::open(outbox_path.clone())
        .await
        .context("opening outbox")?;

    let ignores = config
        .watcher
        .compiled_ignores()
        .context("compiling watcher.ignore_patterns for daemon watcher")?;
    let (watcher_handle, rx) = watcher::spawn_watcher(
        &config.vault.0,
        ignores,
        Duration::from_millis(config.watcher.debounce_ms),
        256,
    )
    .context("spawning watcher")?;

    let mut shutdown_rx = shutdown::install();
    let consumer = tokio::spawn(watcher::run_consumer(
        rx,
        scanner,
        outbox,
        shutdown_rx.clone(),
    ));

    let api_state = api::ApiState {
        pool: store.pool(),
        vault: config.vault.0.clone(),
        outbox_path: outbox_path.clone(),
        embedder: embedder.clone(),
        embedding_dimension: config.embedding.dimension,
    };
    let app = api::router(api_state);

    let listener = tokio::net::TcpListener::bind(&config.http.bind)
        .await
        .with_context(|| format!("binding HTTP server to {}", config.http.bind))?;
    tracing::info!(bind = %config.http.bind, "hmnd: http server listening");

    let mut http_shutdown = shutdown_rx.clone();
    let http_handle = tokio::spawn(async move {
        let server = axum::serve(listener, app).with_graceful_shutdown(async move {
            let _ = http_shutdown.wait_for(|v| *v).await;
        });
        if let Err(e) = server.await {
            tracing::warn!(error = ?e, "hmnd: http server task ended with error");
        }
    });

    let _ = shutdown_rx.wait_for(|v| *v).await;
    // Wait for the consumer to finish its drain window before tearing the
    // watcher down so any in-flight events sitting in the channel are still
    // applied to the index.
    let _ = consumer.await;
    // Drop ordering matters: keep `watcher_handle` alive until after the
    // consumer drains. Dropping the debouncer earlier would stop the notify
    // thread mid-drain and leave queued events unprocessed.
    drop(watcher_handle);

    let _ = http_handle.await;

    tracing::info!("hmnd: drain complete, exiting cleanly");
    Ok(())
}

async fn do_scan(config: &Config) -> Result<ScanReport> {
    let store = Store::open(
        &config.storage.data_dir.0,
        &config.storage.index_file,
        &config.embedding,
    )
    .await
    .context("opening store")?;
    let client =
        EmbeddingClient::new(&config.embedding).context("constructing embedding client")?;
    embed_health_probe(&client, &config.embedding).await;
    let embedder: Arc<dyn Embedder> = Arc::new(client);
    let scanner = Scanner::new(config, &store, embedder).context("constructing scanner")?;
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
