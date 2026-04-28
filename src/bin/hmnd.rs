use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use hypomnema::api::{self, ApiState, VaultEntry};
use hypomnema::config::Config;
use hypomnema::embedding::{Embedder, EmbeddingClient, embed_health_probe};
use hypomnema::indexer::Scanner;
use hypomnema::legacy_state_migration;
use hypomnema::logging::{self, BinaryKind};
use hypomnema::outbox::Outbox;
use hypomnema::shutdown;
use hypomnema::store::Store;
use hypomnema::vault_registry::{VaultRegistry, VaultRow, VaultStatus, vault_data_dir};
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
        Some(Command::Scan) => do_scan(&config).await,
        Some(Command::ConfigValidate) => Ok(()),
    }
}

/// Per-vault constructed runtime state. Lives for the daemon's lifetime; the
/// HTTP API holds a snapshot of these via `ApiState::vaults` and the watcher
/// task lives in `consumer_handle`.
struct VaultRuntime {
    entry: VaultEntry,
    consumer_handle: tokio::task::JoinHandle<()>,
    watcher_handle: watcher::Watcher,
}

async fn run_daemon(config: Config) -> Result<()> {
    let pid = std::process::id();
    tracing::info!(
        data_dir = %config.storage.data_dir.0.display(),
        http_bind = %config.http.bind,
        debounce_ms = %config.watcher.debounce_ms,
        pid,
        "hmnd: starting daemon"
    );
    tracing::debug!(?config, "hmnd: full configuration");

    if config.mcp.transport != "stdio" {
        tracing::warn!(
            configured = %config.mcp.transport,
            socket = %config.mcp.socket.0.display(),
            "mcp.transport = {:?} is not implemented in v0; only stdio via the `hmn mcp` \
             subcommand on the CLI binary is shipped. The socket file is NOT bound. \
             To use MCP, invoke `hmn mcp` from the agent host.",
            config.mcp.transport,
        );
    }

    let registry = VaultRegistry::open(&config.storage.data_dir.0)
        .await
        .context("opening vault_registry")?;

    legacy_state_migration::run_if_needed(&config, &registry)
        .await
        .context("running legacy-state migration")?;

    let active_rows = reconcile(&config, &registry).await?;

    if active_rows.is_empty() {
        tracing::warn!(
            data_dir = %config.storage.data_dir.0.display(),
            "no vaults registered. The daemon is idle; populate vaults.sqlite or restore the \
             legacy [vault] config and restart."
        );
    }

    let client =
        EmbeddingClient::new(&config.embedding).context("constructing embedding client")?;
    embed_health_probe(&client, &config.embedding).await;
    let embedder: Arc<dyn Embedder> = Arc::new(client);

    let mut shutdown_rx = shutdown::install();

    let mut runtimes: Vec<VaultRuntime> = Vec::new();
    for row in &active_rows {
        let runtime = spawn_vault_runtime(&config, row, embedder.clone(), shutdown_rx.clone())
            .await
            .with_context(|| format!("spawning per-vault runtime for {}", row.id))?;
        runtimes.push(runtime);
    }

    let entries: Vec<VaultEntry> = runtimes.iter().map(|r| r.entry.clone()).collect();
    let api_state = ApiState {
        vaults: Arc::new(entries),
        embedder: embedder.clone(),
        embedding_dimension: config.embedding.dimension,
    };
    let app = api::router(api_state);

    let listener = tokio::net::TcpListener::bind(&config.http.bind)
        .await
        .with_context(|| format!("binding HTTP server to {}", config.http.bind))?;
    tracing::info!(bind = %config.http.bind, vault_count = runtimes.len(), "hmnd: http server listening");

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

    // Wait for each per-vault consumer to finish its drain window before
    // tearing watchers down so any in-flight events are still applied.
    for runtime in runtimes.iter_mut() {
        let take_handle = std::mem::replace(&mut runtime.consumer_handle, tokio::spawn(async {}));
        let _ = take_handle.await;
    }
    for runtime in runtimes {
        // Drop ordering: keep `watcher_handle` alive until after the consumer
        // drained (the loop above awaited each consumer); now drop them.
        drop(runtime.watcher_handle);
    }

    let _ = http_handle.await;

    tracing::info!("hmnd: drain complete, exiting cleanly");
    Ok(())
}

/// Validate registry rows against the filesystem and return the active
/// subset. Rows whose `path` is no longer accessible transition to `errored`
/// (Resolution E Case 1). Paused rows are skipped without modification.
async fn reconcile(config: &Config, registry: &VaultRegistry) -> Result<Vec<VaultRow>> {
    let rows = registry.list().await.context("listing registry rows")?;
    let mut active: Vec<VaultRow> = Vec::new();
    for row in rows {
        match row.status {
            VaultStatus::Paused => {
                tracing::info!(vault_id = %row.id, vault_name = %row.name, "reconcile: vault paused; skipping");
                continue;
            }
            VaultStatus::Errored => {
                tracing::warn!(
                    vault_id = %row.id,
                    vault_name = %row.name,
                    last_error = %row.last_error.as_deref().unwrap_or(""),
                    "reconcile: vault errored; skipping"
                );
                continue;
            }
            VaultStatus::Active => {}
        }

        // Verify accessibility; transition to errored if the path went away.
        match std::fs::metadata(&row.path) {
            Ok(meta) if meta.is_dir() => {}
            Ok(_) => {
                let err = format!("vault path {} is not a directory", row.path.display());
                tracing::warn!(vault_id = %row.id, vault = %row.path.display(), "reconcile: marking errored: not a directory");
                registry
                    .update_status(&row.id, VaultStatus::Errored, Some(&err))
                    .await
                    .with_context(|| format!("updating status to errored for {}", row.id))?;
                continue;
            }
            Err(e) => {
                let err = format!("vault path {} not accessible: {}", row.path.display(), e);
                tracing::warn!(vault_id = %row.id, vault = %row.path.display(), error = %e, "reconcile: marking errored: not accessible");
                registry
                    .update_status(&row.id, VaultStatus::Errored, Some(&err))
                    .await
                    .with_context(|| format!("updating status to errored for {}", row.id))?;
                continue;
            }
        }

        // Ensure the per-vault data directory exists. Missing-but-otherwise-
        // active is a recoverable state — the directory is created and the
        // store will populate from scan.
        let target = vault_data_dir(&config.storage.data_dir.0, &row.id);
        std::fs::create_dir_all(&target).with_context(|| {
            format!(
                "creating per-vault directory {} during reconcile",
                target.display()
            )
        })?;

        active.push(row);
    }
    Ok(active)
}

async fn spawn_vault_runtime(
    config: &Config,
    row: &VaultRow,
    embedder: Arc<dyn Embedder>,
    shutdown_rx: tokio::sync::watch::Receiver<bool>,
) -> Result<VaultRuntime> {
    let store = Store::open(
        &row.id,
        &config.storage.data_dir.0,
        &config.storage.index_file,
        &config.embedding,
    )
    .await
    .with_context(|| format!("opening store for {}", row.id))?;
    let store = Arc::new(store);

    let scanner = Scanner::new(&row.path, config, &store, embedder.clone())
        .with_context(|| format!("constructing scanner for {}", row.id))?;
    let report = scanner
        .run()
        .await
        .with_context(|| format!("running initial scan for {}", row.id))?;
    tracing::info!(
        vault_id = %row.id,
        vault_name = %row.name,
        "hmnd: scan complete: inserted={} updated={} hash_unchanged={} deleted={} in {:.2}s",
        report.inserted,
        report.updated,
        report.hash_unchanged,
        report.deleted,
        report.duration.as_secs_f64()
    );

    let outbox_path =
        vault_data_dir(&config.storage.data_dir.0, &row.id).join(&config.storage.outbox_file);
    let outbox = Outbox::open(row.id.clone(), outbox_path.clone())
        .await
        .with_context(|| format!("opening outbox for {}", row.id))?;

    let ignores = config
        .watcher
        .compiled_ignores()
        .context("compiling watcher.ignore_patterns for daemon watcher")?;
    let (watcher_handle, rx) = watcher::spawn_watcher(
        &row.id,
        &row.path,
        ignores,
        Duration::from_millis(config.watcher.debounce_ms),
        256,
    )
    .with_context(|| format!("spawning watcher for {}", row.id))?;

    let scanner_for_consumer = Scanner::new(&row.path, config, &store, embedder)
        .with_context(|| format!("constructing scanner (consumer) for {}", row.id))?;
    let consumer_handle = tokio::spawn(watcher::run_consumer(
        rx,
        scanner_for_consumer,
        outbox,
        shutdown_rx,
    ));

    Ok(VaultRuntime {
        entry: VaultEntry {
            id: row.id.clone(),
            name: row.name.clone(),
            vault_path: row.path.clone(),
            outbox_path,
            store,
        },
        consumer_handle,
        watcher_handle,
    })
}

async fn do_scan(config: &Config) -> Result<()> {
    let registry = VaultRegistry::open(&config.storage.data_dir.0)
        .await
        .context("opening vault_registry")?;
    legacy_state_migration::run_if_needed(config, &registry)
        .await
        .context("running legacy-state migration")?;
    let active_rows = reconcile(config, &registry).await?;

    if active_rows.is_empty() {
        tracing::warn!("hmn scan: no active vaults — nothing to scan");
        return Ok(());
    }

    let client =
        EmbeddingClient::new(&config.embedding).context("constructing embedding client")?;
    embed_health_probe(&client, &config.embedding).await;
    let embedder: Arc<dyn Embedder> = Arc::new(client);

    for row in &active_rows {
        let store = Store::open(
            &row.id,
            &config.storage.data_dir.0,
            &config.storage.index_file,
            &config.embedding,
        )
        .await
        .with_context(|| format!("opening store for {}", row.id))?;
        let scanner = Scanner::new(&row.path, config, &store, embedder.clone())
            .with_context(|| format!("constructing scanner for {}", row.id))?;
        let report = scanner
            .run()
            .await
            .with_context(|| format!("running scan for {}", row.id))?;
        tracing::info!(
            vault_id = %row.id,
            vault_name = %row.name,
            "hmnd: scan complete: inserted={} updated={} hash_unchanged={} deleted={} in {:.2}s",
            report.inserted,
            report.updated,
            report.hash_unchanged,
            report.deleted,
            report.duration.as_secs_f64()
        );
    }
    Ok(())
}
