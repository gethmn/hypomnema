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
use crate::store::SqlitePool;

#[derive(Clone)]
pub struct ApiState {
    pub pool: SqlitePool,
    pub vault: PathBuf,
    pub outbox_path: PathBuf,
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
