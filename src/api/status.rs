use anyhow::{Context, Result};
use axum::{Json, extract::State};
use tokio::task;

use super::ApiState;
use super::error::ApiError;
use super::types::{OutboxStatus, StatusResponse};

// Step-9 multi-vault behavior:
//
// - With **N=1** active vaults the response is byte-identical to v0.1.0
//   (single vault path + outbox path + indexed_file_count).
// - With **N=0** active vaults (Resolution E Case 2 — fresh install / zero
//   registered vaults) we return an empty representative: empty vault path,
//   zero counts, empty outbox path. v0 consumers see a "fresh, empty"
//   snapshot rather than an error.
// - With **N≥2** (only reachable via direct registry insertion in step 9 —
//   step 10's `hmn vault create` is the user surface) we sum
//   `indexed_file_count` across vaults and pick the first vault's path +
//   outbox as the representative. The cross-vault wire shape lands in
//   step 10's `/status` amendment per Resolution F's "ahead of spec"
//   footnote.
pub(crate) async fn status(
    State(s): State<ApiState>,
) -> std::result::Result<Json<StatusResponse>, ApiError> {
    if s.vaults.is_empty() {
        return Ok(Json(StatusResponse {
            vault: String::new(),
            indexed_file_count: 0,
            last_indexed_at: None,
            outbox: OutboxStatus {
                path: String::new(),
                size_bytes: 0,
            },
        }));
    }

    let mut total_count: i64 = 0;
    let mut max_indexed: Option<String> = None;
    for vault in s.vaults.iter() {
        let pool = vault.store.pool();
        let (count, last_indexed) =
            task::spawn_blocking(move || -> Result<(i64, Option<String>)> {
                let conn = pool
                    .get()
                    .context("acquiring connection from pool for /status")?;
                let count: i64 = conn
                    .query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))
                    .context("counting rows in files for /status")?;
                let last: Option<String> = conn
                    .query_row("SELECT MAX(indexed_at) FROM files", [], |r| r.get(0))
                    .context("reading MAX(indexed_at) for /status")?;
                Ok((count, last))
            })
            .await
            .context("spawn_blocking join error in /status handler")??;
        total_count += count;
        if let Some(li) = last_indexed {
            max_indexed = match max_indexed.take() {
                Some(prev) if prev >= li => Some(prev),
                _ => Some(li),
            };
        }
    }

    let representative = &s.vaults[0];
    let outbox_size = std::fs::metadata(&representative.outbox_path)
        .map(|m| m.len())
        .unwrap_or(0);

    Ok(Json(StatusResponse {
        vault: representative.vault_path.display().to_string(),
        indexed_file_count: total_count as u64,
        last_indexed_at: max_indexed,
        outbox: OutboxStatus {
            path: representative.outbox_path.display().to_string(),
            size_bytes: outbox_size,
        },
    }))
}
