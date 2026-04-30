use axum::body::{Body, Bytes};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::Response;
use futures_util::stream;
use serde::Deserialize;
use tokio::sync::broadcast;

use super::ApiState;
use super::error::ApiError;
use crate::events::{StreamEvent, StreamLagAction, StreamLaggedEvent};
use crate::vault_registry::VaultId;

// ---------------------------------------------------------------------------
// Single-vault watch: GET /vaults/{name_or_id}/watch
// ---------------------------------------------------------------------------

/// Returns an NDJSON streaming response of `StreamEvent` values for the named
/// vault. Events from other vaults are filtered out at stream time.
///
/// Error responses:
/// - 404 `vault_not_found` — no vault matches `name_or_id`.
/// - 409 `vault_not_active` — vault exists but is not currently active
///   (paused or errored); caller should use the vault API to check status.
pub(crate) async fn watch_vault(
    State(s): State<ApiState>,
    Path(name_or_id): Path<String>,
) -> Result<Response<Body>, ApiError> {
    // Resolve name/id → VaultId (covers runners regardless of status).
    let vault_id = s.vault_manager.resolve(&name_or_id)?;

    // Verify the vault is currently active so we don't silently stream nothing.
    let is_active = s
        .vault_manager
        .active_vaults()
        .iter()
        .any(|e| e.id == vault_id);
    if !is_active {
        return Err(ApiError::vault_not_active(format!(
            "vault {name_or_id} is not active (paused or errored)"
        )));
    }

    let rx = s.event_bus.subscribe();
    let body = ndjson_stream_single(rx, vault_id);
    Ok(ndjson_response(body))
}

// ---------------------------------------------------------------------------
// All-active-vaults watch: GET /events/watch
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub(crate) struct WatchAllQuery {
    /// When provided, accepted as a no-op parameter for ergonomic parity with
    /// the spec's `?all=true` hint. The route always streams all active vaults.
    #[allow(dead_code)]
    pub all: Option<bool>,
}

/// Returns an NDJSON streaming response of `StreamEvent` values from all
/// vaults that are active at subscription time. Vaults started or created
/// after the subscription begins are not included (v0 pin).
pub(crate) async fn watch_all(
    State(s): State<ApiState>,
    Query(_q): Query<WatchAllQuery>,
) -> Response<Body> {
    // Pin the active vault IDs at subscription time (v0 spec: no dynamic
    // addition of vaults after subscription starts).
    let active_ids: Vec<VaultId> = s
        .vault_manager
        .active_vaults()
        .iter()
        .map(|e| e.id.clone())
        .collect();

    let rx = s.event_bus.subscribe();
    let body = ndjson_stream_multi(rx, active_ids);
    ndjson_response(body)
}

// ---------------------------------------------------------------------------
// Shared streaming helpers
// ---------------------------------------------------------------------------

/// State threaded through `stream::unfold` for the single-vault stream.
struct SingleVaultState {
    rx: broadcast::Receiver<StreamEvent>,
    vault_id: VaultId,
}

/// Build a `Body` that reads from a broadcast receiver and filters to a single
/// vault. `RecvError::Lagged(n)` is converted to a `stream_lagged` NDJSON
/// line and the stream continues. `RecvError::Closed` terminates the stream.
fn ndjson_stream_single(rx: broadcast::Receiver<StreamEvent>, vault_id: VaultId) -> Body {
    Body::from_stream(stream::unfold(
        SingleVaultState { rx, vault_id },
        |mut state| async move {
            loop {
                match state.rx.recv().await {
                    Ok(event) => {
                        if !event_vault_matches(&event, &state.vault_id) {
                            continue;
                        }
                        match serialize_ndjson(&event) {
                            Ok(line) => return Some((Ok::<_, std::io::Error>(line), state)),
                            Err(_) => continue,
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        let lagged =
                            StreamEvent::stream_lagged_for_vault(state.vault_id.clone(), Some(n));
                        match serialize_ndjson(&lagged) {
                            Ok(line) => return Some((Ok::<_, std::io::Error>(line), state)),
                            Err(_) => continue,
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => return None,
                }
            }
        },
    ))
}

/// State threaded through `stream::unfold` for the all-active-vaults stream.
struct MultiVaultState {
    rx: broadcast::Receiver<StreamEvent>,
    active_ids: Vec<VaultId>,
}

/// Build a `Body` that reads from a broadcast receiver and filters to the
/// pinned set of active vault IDs. Events from vaults not in `active_ids`
/// are silently dropped.
fn ndjson_stream_multi(rx: broadcast::Receiver<StreamEvent>, active_ids: Vec<VaultId>) -> Body {
    Body::from_stream(stream::unfold(
        MultiVaultState { rx, active_ids },
        |mut state| async move {
            loop {
                match state.rx.recv().await {
                    Ok(event) => {
                        if !event_in_set(&event, &state.active_ids) {
                            continue;
                        }
                        match serialize_ndjson(&event) {
                            Ok(line) => return Some((Ok::<_, std::io::Error>(line), state)),
                            Err(_) => continue,
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        // Emit a daemon-wide lag event since we can't know which
                        // vault's events were dropped without replaying the buffer.
                        let lagged = StreamEvent::StreamLagged(StreamLaggedEvent {
                            vault: None,
                            missed: Some(n),
                            action: StreamLagAction::ResyncRequired,
                            detected_at: chrono::Utc::now()
                                .to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
                        });
                        match serialize_ndjson(&lagged) {
                            Ok(line) => return Some((Ok::<_, std::io::Error>(line), state)),
                            Err(_) => continue,
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => return None,
                }
            }
        },
    ))
}

fn event_vault_matches(event: &StreamEvent, id: &VaultId) -> bool {
    match event {
        StreamEvent::FileChanged(e) => &e.vault == id,
        // Pass `stream_lagged` only if it targets this vault or is undirected.
        StreamEvent::StreamLagged(e) => e.vault.as_ref().is_none_or(|v| v == id),
    }
}

fn event_in_set(event: &StreamEvent, ids: &[VaultId]) -> bool {
    match event {
        StreamEvent::FileChanged(e) => ids.contains(&e.vault),
        StreamEvent::StreamLagged(e) => {
            // Undirected lag events pass through; vault-specific lag only if
            // the vault is in the subscription set.
            e.vault.as_ref().is_none_or(|v| ids.contains(v))
        }
    }
}

fn serialize_ndjson(event: &StreamEvent) -> Result<Bytes, serde_json::Error> {
    let mut s = serde_json::to_string(event)?;
    s.push('\n');
    Ok(Bytes::from(s.into_bytes()))
}

fn ndjson_response(body: Body) -> Response<Body> {
    Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/x-ndjson"),
        )
        .body(body)
        .expect("static response construction cannot fail")
}
