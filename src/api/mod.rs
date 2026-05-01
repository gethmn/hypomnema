pub(crate) mod error;
mod health;
pub mod mcp_http;
pub(crate) mod search;
mod status;
pub mod types;
pub(crate) mod vaults;
pub(crate) mod watch;

use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use axum::routing::{get, post};

pub use types::*;

use crate::control_plane::VaultManager;
use crate::events::EventBus;
use crate::store::Store;
use crate::vault_registry::{VaultId, VaultStatus};

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
}

pub fn router(state: ApiState) -> Router {
    Router::new()
        .route("/health", get(health::health))
        .route("/status", get(status::status))
        .route("/search/filesystem", post(search::filesystem))
        .route("/search/content", post(search::content))
        .route("/search/semantic", post(search::semantic))
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
