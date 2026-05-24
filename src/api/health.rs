use std::time::Duration;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;

use crate::vault_registry::VaultStatus;

use super::ApiState;
use super::types::{EmbeddingHealth, HealthResponse};

pub(crate) async fn health(State(state): State<ApiState>) -> impl IntoResponse {
    let snapshot = collect_health(&state).await;
    let status_code = match snapshot.status.as_str() {
        "healthy" => StatusCode::OK,
        _ => StatusCode::SERVICE_UNAVAILABLE,
    };
    (status_code, Json(snapshot))
}

async fn collect_health(state: &ApiState) -> HealthResponse {
    let uptime_seconds = state.started_at.elapsed().as_secs();

    // Signal: vault counts from search_scope.
    let scope = match state.vault_manager.search_scope().await {
        Ok(rows) => rows,
        Err(_) => {
            // Registry unreachable — can't determine vault state.
            return HealthResponse {
                status: "unhealthy".to_string(),
                vaults_active: 0,
                vaults_errored: 0,
                uptime_seconds,
                embedding: None,
            };
        }
    };
    let vaults_active = scope
        .iter()
        .filter(|r| matches!(r.status, VaultStatus::Active))
        .count() as u64;
    let vaults_errored = scope
        .iter()
        .filter(|r| matches!(r.status, VaultStatus::Errored))
        .count() as u64;

    // Signal: watcher liveness.
    let watcher_dead = state.vault_manager.any_watcher_dead().await;

    // Signal: DB probe — SELECT 1 on each active vault's store.
    let active_vaults = state.vault_manager.active_vaults();
    let mut db_ok = true;
    for entry in &active_vaults {
        let pool = entry.store.pool();
        let ok = tokio::task::spawn_blocking(move || -> bool {
            let conn = match pool.get() {
                Ok(c) => c,
                Err(_) => return false,
            };
            conn.query_row("SELECT 1", [], |r| r.get::<_, i64>(0))
                .is_ok()
        })
        .await
        .unwrap_or(false);
        if !ok {
            db_ok = false;
            break;
        }
    }

    // Signal: embedding probe — only when an endpoint is configured.
    let mut embedding: Option<EmbeddingHealth> = None;
    if let Some(ref endpoint) = state.embedding_endpoint {
        let client = reqwest::Client::new();
        let reachable = client
            .head(endpoint)
            .timeout(Duration::from_millis(500))
            .send()
            .await
            .is_ok();
        embedding = Some(EmbeddingHealth {
            status: if reachable {
                "healthy".to_string()
            } else {
                "degraded".to_string()
            },
            endpoint: endpoint.clone(),
        });
    }

    // Determine overall status per the signal ladder (highest precedence wins).
    let status = if watcher_dead || !db_ok {
        "unhealthy"
    } else if vaults_errored > 0 || embedding.as_ref().is_some_and(|e| e.status == "degraded") {
        "degraded"
    } else {
        "healthy"
    };

    HealthResponse {
        status: status.to_string(),
        vaults_active,
        vaults_errored,
        uptime_seconds,
        embedding,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Instant;

    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use chrono::Utc;
    use tempfile::TempDir;
    use tower::ServiceExt;

    use crate::api::{ApiState, VaultEntry, router};
    use crate::config::EmbeddingConfig;
    use crate::control_plane::VaultManager;
    use crate::embedding::StubEmbedder;
    use crate::store::Store;
    use crate::vault_registry::{VaultId, VaultRow, VaultStatus};

    const BODY_LIMIT: usize = 4 * 1024 * 1024;

    async fn health_state_one_active() -> (ApiState, TempDir, TempDir) {
        let dir = TempDir::new().unwrap();
        let vault = TempDir::new().unwrap();
        let vault_id = VaultId::new();
        let store = Store::open(
            &vault_id,
            dir.path(),
            "index.sqlite",
            &EmbeddingConfig::default(),
        )
        .await
        .unwrap();
        let entry = VaultEntry {
            id: vault_id,
            name: "test".to_string(),
            vault_path: vault.path().to_path_buf(),
            store: Arc::new(store),
            status: VaultStatus::Active,
            bootstrap_state: crate::api::BootstrapState::ready_state(),
        };
        let manager = Arc::new(VaultManager::for_tests(
            vec![entry],
            Arc::new(StubEmbedder::new(768)),
            768,
        ));
        let state = ApiState {
            vault_manager: manager.clone(),
            event_bus: manager.event_bus(),
            started_at: Instant::now(),
            embedding_endpoint: None,
            semantic_config: crate::config::SemanticSearchConfig::default(),
        };
        (state, dir, vault)
    }

    async fn get_health(state: ApiState) -> (StatusCode, serde_json::Value) {
        let app = router(state);
        let req = Request::builder()
            .method("GET")
            .uri("/health")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let status = resp.status();
        let bytes = to_bytes(resp.into_body(), BODY_LIMIT).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        (status, value)
    }

    #[tokio::test]
    async fn healthy_path_returns_200_with_correct_shape() {
        let (state, _dir, _vault) = health_state_one_active().await;
        let (status, body) = get_health(state).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "healthy");
        assert_eq!(body["vaults_active"], 1);
        assert_eq!(body["vaults_errored"], 0);
        assert!(body["uptime_seconds"].is_number());
        assert!(body.get("embedding").is_none() || body["embedding"].is_null());
    }

    #[tokio::test]
    async fn degraded_path_errored_vault_returns_503() {
        let dir = TempDir::new().unwrap();
        let vault = TempDir::new().unwrap();
        let vault_id = VaultId::new();
        let store = Store::open(
            &vault_id,
            dir.path(),
            "index.sqlite",
            &EmbeddingConfig::default(),
        )
        .await
        .unwrap();
        let active_entry = VaultEntry {
            id: vault_id,
            name: "active".to_string(),
            vault_path: vault.path().to_path_buf(),
            store: Arc::new(store),
            status: VaultStatus::Active,
            bootstrap_state: crate::api::BootstrapState::ready_state(),
        };
        let errored_row = VaultRow {
            id: VaultId::new(),
            name: "errored".to_string(),
            path: std::path::PathBuf::from("/nonexistent"),
            status: VaultStatus::Errored,
            created_at: Utc::now(),
            last_error: Some("path not accessible".to_string()),
        };
        let manager = Arc::new(VaultManager::for_tests_full(
            vec![active_entry],
            vec![errored_row],
            Arc::new(StubEmbedder::new(768)),
            768,
        ));
        let state = ApiState {
            vault_manager: manager.clone(),
            event_bus: manager.event_bus(),
            started_at: Instant::now(),
            embedding_endpoint: None,
            semantic_config: crate::config::SemanticSearchConfig::default(),
        };
        let (status, body) = get_health(state).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body["status"], "degraded");
        assert_eq!(body["vaults_active"], 1);
        assert_eq!(body["vaults_errored"], 1);
    }

    #[tokio::test]
    async fn degraded_path_embedding_unreachable_returns_503() {
        let (mut state, _dir, _vault) = health_state_one_active().await;
        // Port 1 is always closed; connection will be refused immediately.
        state.embedding_endpoint = Some("http://127.0.0.1:1".to_string());
        let (status, body) = get_health(state).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body["status"], "degraded");
        assert_eq!(body["embedding"]["status"], "degraded");
        assert_eq!(body["embedding"]["endpoint"], "http://127.0.0.1:1");
    }

    #[tokio::test]
    async fn uptime_seconds_is_present_and_numeric() {
        let (state, _dir, _vault) = health_state_one_active().await;
        let (status, body) = get_health(state).await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            body["uptime_seconds"].is_number(),
            "uptime_seconds must be a number; got {:?}",
            body["uptime_seconds"]
        );
    }
}
