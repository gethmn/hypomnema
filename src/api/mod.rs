pub(crate) mod error;
mod health;
pub mod mcp_http;
pub(crate) mod search;
mod status;
pub mod types;
pub(crate) mod vaults;
pub(crate) mod watch;

use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, RwLock};
use std::time::Instant;

use axum::Router;
use axum::routing::{get, post};
use chrono::{DateTime, Utc};

pub use types::*;

use crate::control_plane::VaultManager;
use crate::events::EventBus;
use crate::store::Store;
use crate::vault_registry::{VaultId, VaultStatus};

/// In-memory per-vault bootstrap (initial-scan) state. Orthogonal to the
/// persisted `VaultStatus` enum: this is the lifecycle the daemon reports
/// while building the index for the first time after a process start (or
/// after a resume / reset that re-runs the initial scan). Surfaced via the
/// `/status` `bootstrap` block.
///
/// Counter fields are atomic so the scan loop can update them without
/// taking the outer `RwLock` write-lock on every file.
#[derive(Debug)]
pub enum BootstrapState {
    Indexing {
        started_at: DateTime<Utc>,
        files_seen: Arc<AtomicU64>,
        files_indexed: Arc<AtomicU64>,
    },
    Ready,
    Errored(String),
}

impl BootstrapState {
    /// Default state for fixtures and test-only `VaultEntry` construction —
    /// signals "no bootstrap in flight; queryable now".
    pub fn ready_state() -> Arc<RwLock<BootstrapState>> {
        Arc::new(RwLock::new(BootstrapState::Ready))
    }

    /// Build a fresh `Indexing` state with zeroed atomics. Returns the state
    /// plus a clone of each counter so the scanner can update them without
    /// re-reading the lock.
    pub fn indexing(
        started_at: DateTime<Utc>,
    ) -> (Arc<RwLock<BootstrapState>>, Arc<AtomicU64>, Arc<AtomicU64>) {
        let files_seen = Arc::new(AtomicU64::new(0));
        let files_indexed = Arc::new(AtomicU64::new(0));
        let state = Arc::new(RwLock::new(BootstrapState::Indexing {
            started_at,
            files_seen: files_seen.clone(),
            files_indexed: files_indexed.clone(),
        }));
        (state, files_seen, files_indexed)
    }
}

/// One active-vault entry exposed to the HTTP API. Constructed by the
/// `control_plane::VaultManager` (one per active runner) and surfaced to
/// search/status handlers via `VaultManager::active_vaults()`. The
/// `status` field carries the runner's view of the vault — step 10 only
/// inserts `Active` entries into the runners map, but step 11 will mutate
/// it for pause/resume without tearing the runner down.
#[derive(Clone)]
pub struct VaultEntry {
    pub id: VaultId,
    pub name: String,
    pub vault_path: PathBuf,
    pub store: Arc<Store>,
    pub status: VaultStatus,
    /// In-memory bootstrap state. Shared between the runner, the background
    /// bootstrap task that updates it, and `/status` readers. See
    /// `BootstrapState`.
    pub bootstrap_state: Arc<RwLock<BootstrapState>>,
}

impl VaultEntry {
    pub fn is_active(&self) -> bool {
        matches!(self.status, VaultStatus::Active)
    }
}

#[derive(Clone)]
pub struct ApiState {
    pub vault_manager: Arc<VaultManager>,
    pub event_bus: Arc<EventBus>,
    /// Monotonic instant captured at daemon start; used to derive uptime_seconds.
    pub started_at: Instant,
    /// Embedding service endpoint to probe on /health. `None` disables the
    /// embedding signal (used in test fixtures that don't run an embedding
    /// service).
    pub embedding_endpoint: Option<String>,
}

pub fn router(state: ApiState) -> Router {
    Router::new()
        .route("/health", get(health::health))
        .route("/status", get(status::status))
        .route("/search/filesystem", post(search::filesystem))
        .route("/search/content", post(search::content))
        .route("/search/semantic", post(search::semantic))
        .route("/content/get", post(search::content_get))
        .route("/vaults", post(vaults::create).get(vaults::list))
        .route(
            "/vaults/{name_or_id}",
            get(vaults::get).delete(vaults::terminate),
        )
        .route("/vaults/{name_or_id}/pause", post(vaults::pause))
        .route("/vaults/{name_or_id}/resume", post(vaults::resume))
        .route("/vaults/{name_or_id}/reset", post(vaults::reset))
        .route("/vaults/{name_or_id}/rename", post(vaults::rename))
        .route("/vaults/{name_or_id}/rescan", post(vaults::rescan))
        .route("/vaults/{name_or_id}/watch", get(watch::watch_vault))
        .route("/events/watch", get(watch::watch_all))
        .with_state(state)
}

#[cfg(test)]
mod tests;
