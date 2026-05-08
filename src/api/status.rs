use std::sync::atomic::Ordering;

use anyhow::{Context, Result};
use axum::{Json, extract::State};
use chrono::SecondsFormat;
use tokio::task;

use super::ApiState;
use super::error::ApiError;
use super::types::{BootstrapBlock, StatusResponse, VaultStatusEntry};
use crate::api::BootstrapState;

// Step-9 multi-vault behavior:
//
// - With **N=1** active vaults the response is byte-identical to v0.1.0
//   (single vault path + indexed_file_count) at the top level. The new
//   `vaults` array (Step 24) carries per-vault detail additively.
// - With **N=0** active vaults (Resolution E Case 2 — fresh install / zero
//   registered vaults) we return an empty representative: empty vault path,
//   zero counts, empty `vaults` array.
// - With **N≥2** we sum `indexed_file_count` across vaults and pick the first
//   vault's path as the representative (back-compat with the legacy v0
//   shape). Per-vault detail — including the in-memory bootstrap block — is
//   on `vaults[]`.
//
// Step 24 additionally exposes a per-vault `bootstrap` block: in-memory state
// pulled from the manager at query time. v0 clients ignoring unknown fields
// continue to work; `vaults[]` is omitted from the wire when no active
// vaults exist (skip_serializing_if = "Vec::is_empty").
pub(crate) async fn status(
    State(s): State<ApiState>,
) -> std::result::Result<Json<StatusResponse>, ApiError> {
    let vaults = s.vault_manager.active_vaults();
    if vaults.is_empty() {
        return Ok(Json(StatusResponse {
            vault: String::new(),
            indexed_file_count: 0,
            last_indexed_at: None,
            vaults: Vec::new(),
        }));
    }

    let mut total_count: i64 = 0;
    let mut max_indexed: Option<String> = None;
    let mut entries: Vec<VaultStatusEntry> = Vec::with_capacity(vaults.len());

    for vault in vaults.iter() {
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
        if let Some(li) = last_indexed.clone() {
            max_indexed = match max_indexed.take() {
                Some(prev) if prev >= li => Some(prev),
                _ => Some(li),
            };
        }

        let bootstrap = read_bootstrap_block(&vault.bootstrap_state);

        entries.push(VaultStatusEntry {
            id: vault.id.to_string(),
            name: vault.name.clone(),
            path: vault.vault_path.display().to_string(),
            indexed_file_count: count.max(0) as u64,
            last_indexed_at: last_indexed,
            bootstrap,
        });
    }

    Ok(Json(StatusResponse {
        vault: vaults[0].vault_path.display().to_string(),
        indexed_file_count: total_count.max(0) as u64,
        last_indexed_at: max_indexed,
        vaults: entries,
    }))
}

/// Snapshot the per-vault bootstrap state into the wire shape. Reads atomic
/// counters with `Relaxed` ordering — the counters are observation-only and
/// monotonic; no synchronization is implied across them.
fn read_bootstrap_block(state: &std::sync::RwLock<BootstrapState>) -> BootstrapBlock {
    match state.read() {
        Ok(guard) => match &*guard {
            BootstrapState::Indexing {
                started_at,
                files_seen,
                files_indexed,
            } => BootstrapBlock {
                state: "indexing".to_string(),
                started_at: Some(started_at.to_rfc3339_opts(SecondsFormat::Micros, true)),
                files_seen: files_seen.load(Ordering::Relaxed),
                files_indexed: files_indexed.load(Ordering::Relaxed),
                message: None,
            },
            BootstrapState::Ready => BootstrapBlock {
                state: "ready".to_string(),
                started_at: None,
                files_seen: 0,
                files_indexed: 0,
                message: None,
            },
            BootstrapState::Errored(msg) => BootstrapBlock {
                state: "errored".to_string(),
                started_at: None,
                files_seen: 0,
                files_indexed: 0,
                message: Some(msg.clone()),
            },
        },
        // Lock poisoned: report a synthetic errored block rather than 5xx.
        // This is exceptional and the daemon is shutting down or already wedged.
        Err(_) => BootstrapBlock {
            state: "errored".to_string(),
            started_at: None,
            files_seen: 0,
            files_indexed: 0,
            message: Some("bootstrap state lock poisoned".to_string()),
        },
    }
}
