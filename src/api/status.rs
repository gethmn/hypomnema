use anyhow::{Context, Result};
use axum::{Json, extract::State};
use tokio::task;

use super::ApiState;
use super::error::ApiError;
use super::types::{OutboxStatus, StatusResponse};

pub(crate) async fn status(
    State(s): State<ApiState>,
) -> std::result::Result<Json<StatusResponse>, ApiError> {
    let pool = s.pool.clone();
    let (count, last_indexed) = task::spawn_blocking(move || -> Result<(i64, Option<String>)> {
        let conn = pool
            .get()
            .context("acquiring connection from pool for /status")?;
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))
            .context("counting rows in files for /status")?;
        // MAX(indexed_at) always returns one row; the value is NULL when the
        // table is empty, so the column must deserialize as Option<String>.
        let last: Option<String> = conn
            .query_row("SELECT MAX(indexed_at) FROM files", [], |r| r.get(0))
            .context("reading MAX(indexed_at) for /status")?;
        Ok((count, last))
    })
    .await
    .context("spawn_blocking join error in /status handler")??;

    let outbox_size = std::fs::metadata(&s.outbox_path)
        .map(|m| m.len())
        .unwrap_or(0);

    Ok(Json(StatusResponse {
        vault: s.vault.display().to_string(),
        indexed_file_count: count as u64,
        last_indexed_at: last_indexed,
        outbox: OutboxStatus {
            path: s.outbox_path.display().to_string(),
            size_bytes: outbox_size,
        },
    }))
}
