mod error;
mod health;
mod search;
mod status;
pub mod types;

use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use axum::routing::{get, post};

pub use types::*;

use crate::embedding::Embedder;
use crate::store::Store;
use crate::vault_registry::VaultId;

/// One active-vault entry in the daemon's API surface. Step-9 introduces
/// per-vault routing via `Vec<VaultEntry>`; the search handlers iterate over
/// the entries and merge results (passthrough for N=1 per Resolution F).
#[derive(Clone)]
pub struct VaultEntry {
    pub id: VaultId,
    pub name: String,
    pub vault_path: PathBuf,
    pub outbox_path: PathBuf,
    pub store: Arc<Store>,
}

#[derive(Clone)]
pub struct ApiState {
    pub vaults: Arc<Vec<VaultEntry>>,
    pub embedder: Arc<dyn Embedder>,
    pub embedding_dimension: u32,
}

pub fn router(state: ApiState) -> Router {
    Router::new()
        .route("/health", get(health::health))
        .route("/status", get(status::status))
        .route("/search/filesystem", post(search::filesystem))
        .route("/search/content", post(search::content))
        .route("/search/semantic", post(search::semantic))
        .with_state(state)
}

#[cfg(test)]
mod tests;
