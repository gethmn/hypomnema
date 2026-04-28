use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use hypomnema::api::mcp_http::{self, McpHttpState};
use hypomnema::api::{self, ApiState};
use hypomnema::config::Config;
use hypomnema::control_plane::VaultManager;
use hypomnema::embedding::{Embedder, EmbeddingClient, embed_health_probe};
use hypomnema::legacy_state_migration;
use hypomnema::logging::{self, BinaryKind};
use hypomnema::mcp::{HypomnemaBackend, InProcessBackend};
use hypomnema::shutdown;
use hypomnema::vault_registry::VaultRegistry;

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
        Some(Command::ConfigValidate) => Ok(()),
    }
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
    let registry = Arc::new(registry);

    legacy_state_migration::run_if_needed(&config, &registry)
        .await
        .context("running legacy-state migration")?;

    let client =
        EmbeddingClient::new(&config.embedding).context("constructing embedding client")?;
    embed_health_probe(&client, &config.embedding).await;
    let embedder: Arc<dyn Embedder> = Arc::new(client);

    let shutdown_rx = shutdown::install();

    let embedding_dimension = config.embedding.dimension;
    let config = Arc::new(config);
    let manager = VaultManager::open(
        registry.clone(),
        config.clone(),
        embedder.clone(),
        embedding_dimension,
        shutdown_rx.clone(),
    )
    .await
    .context("opening vault manager")?;
    let vault_manager = Arc::new(manager);

    let active_count = vault_manager.active_vaults().len();
    if active_count == 0 {
        tracing::warn!(
            data_dir = %config.storage.data_dir.0.display(),
            "no vaults registered. The daemon is idle; populate vaults.sqlite or restore the \
             legacy [vault] config and restart."
        );
    }

    let api_state = ApiState {
        vault_manager: vault_manager.clone(),
    };
    let mut app = api::router(api_state);

    if config.mcp.http.enabled {
        let backend: Arc<dyn HypomnemaBackend + Send + Sync> =
            Arc::new(InProcessBackend::new(vault_manager.clone()));
        let mcp_state = McpHttpState {
            backend,
            default_vault_name: config.default_vault_name.clone(),
            enable_write_tools: config.mcp.enable_write_tools,
        };
        app = app.merge(mcp_http::router(mcp_state));
        tracing::info!(
            path = %config.mcp.http.path,
            enabled = config.mcp.http.enabled,
            "hmnd: mcp http transport mounted"
        );
    } else {
        tracing::info!(
            enabled = config.mcp.http.enabled,
            "hmnd: mcp http transport disabled"
        );
    }

    let listener = tokio::net::TcpListener::bind(&config.http.bind)
        .await
        .with_context(|| format!("binding HTTP server to {}", config.http.bind))?;
    tracing::info!(
        bind = %config.http.bind,
        vault_count = active_count,
        "hmnd: http server listening"
    );

    let mut http_shutdown = shutdown_rx.clone();
    let http_handle = tokio::spawn(async move {
        let server = axum::serve(listener, app).with_graceful_shutdown(async move {
            let _ = http_shutdown.wait_for(|v| *v).await;
        });
        if let Err(e) = server.await {
            tracing::warn!(error = ?e, "hmnd: http server task ended with error");
        }
    });

    let mut shutdown_rx = shutdown_rx;
    let _ = shutdown_rx.wait_for(|v| *v).await;

    let _ = http_handle.await;

    // Each VaultRunner's consumer task observes the daemon-wide shutdown via
    // its per-vault watch mirror (set up in spawn_runner_for_row); the
    // signal has already fired by the time we get here, so the consumers
    // are draining. Dropping the manager (after the Arc reference count
    // hits zero, via dropping vault_manager + the API's clone) lets each
    // runner's watcher drop in turn.
    drop(vault_manager);

    tracing::info!("hmnd: drain complete, exiting cleanly");
    Ok(())
}
