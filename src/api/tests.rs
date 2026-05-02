use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use rusqlite::params;
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::task;
use tower::ServiceExt;

use std::sync::{Arc, Mutex};

use super::{ApiState, VaultEntry, router};
use crate::config::{
    Config, ConfigPath, EmbeddingConfig, HttpConfig, LoggingConfig, McpConfig, StorageConfig,
    WatcherConfig,
};
use crate::control_plane::VaultManager;
use crate::embedding::{EmbedFuture, Embedder, EmbeddingError, StubEmbedder};
use crate::store::{SqlitePool, Store};
use crate::vault_registry::{VaultId, VaultRegistry, VaultRow, VaultStatus};

// 4 MB is plenty for these test bodies; we set a finite cap to satisfy
// `to_bytes` without inviting unbounded reads.
const BODY_LIMIT: usize = 4 * 1024 * 1024;

struct Harness {
    _dir: TempDir,
    _vault: TempDir,
    state: ApiState,
    pool: SqlitePool,
    vault_path: std::path::PathBuf,
    vault_id: VaultId,
    vault_name: String,
}

impl Harness {
    fn pool(&self) -> SqlitePool {
        self.pool.clone()
    }
}

async fn harness() -> Harness {
    harness_with_embedder(Arc::new(StubEmbedder::new(768))).await
}

async fn harness_with_embedder(embedder: Arc<dyn Embedder>) -> Harness {
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
    let store = Arc::new(store);
    let pool = store.pool();
    let entry = VaultEntry {
        id: vault_id.clone(),
        name: "test".to_string(),
        vault_path: vault.path().to_path_buf(),
        store,
        status: VaultStatus::Active,
    };
    let manager = Arc::new(VaultManager::for_tests(vec![entry], embedder, 768));
    let state = ApiState {
        vault_manager: manager.clone(),
        event_bus: manager.event_bus(),
    };
    Harness {
        _dir: dir,
        vault_path: vault.path().to_path_buf(),
        _vault: vault,
        pool,
        vault_id,
        vault_name: "test".to_string(),
        state,
    }
}

async fn seed_files(pool: SqlitePool, rows: Vec<(&'static str, &'static str, &'static str)>) {
    task::spawn_blocking(move || {
        let conn = pool.get().unwrap();
        for (path, content, indexed_at) in rows {
            conn.execute(
                "INSERT INTO files (path, size, mtime, content_hash, indexed_at, content) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    path,
                    content.len() as i64,
                    "2026-01-01T00:00:00Z",
                    "sha256:00",
                    indexed_at,
                    content,
                ],
            )
            .unwrap();
        }
    })
    .await
    .unwrap();
}

async fn json_request(method: &str, uri: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn body_json(resp: axum::http::Response<Body>) -> (StatusCode, Value) {
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), BODY_LIMIT).await.unwrap();
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    (status, value)
}

#[tokio::test]
async fn health_returns_200_with_status_ok() {
    let h = harness().await;
    let app = router(h.state.clone());
    let req = Request::builder()
        .method("GET")
        .uri("/health")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "status": "ok" }));
}

#[tokio::test]
async fn status_reports_zero_files_when_index_empty() {
    let h = harness().await;
    let app = router(h.state.clone());
    let req = Request::builder()
        .method("GET")
        .uri("/status")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["indexed_file_count"], 0);
    assert!(body["last_indexed_at"].is_null());
    assert_eq!(body["vault"], h.vault_path.display().to_string());
}

#[tokio::test]
async fn status_reports_count_and_last_indexed_after_seeding() {
    let h = harness().await;
    seed_files(
        h.pool(),
        vec![
            ("a.md", "alpha", "2026-04-01T00:00:00Z"),
            ("b.md", "bravo", "2026-04-22T14:31:08.123456Z"),
        ],
    )
    .await;
    let app = router(h.state.clone());
    let req = Request::builder()
        .method("GET")
        .uri("/status")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["indexed_file_count"], 2);
    assert_eq!(body["last_indexed_at"], "2026-04-22T14:31:08.123456Z");
}

#[tokio::test]
async fn search_filesystem_returns_results_for_glob() {
    let h = harness().await;
    seed_files(
        h.pool(),
        vec![
            ("notes/a.md", "alpha", "2026-04-01T00:00:00Z"),
            ("notes/b.txt", "bravo", "2026-04-01T00:00:00Z"),
            ("notes/sub/c.md", "charlie", "2026-04-01T00:00:00Z"),
        ],
    )
    .await;
    let app = router(h.state.clone());
    let req = json_request("POST", "/search/filesystem", json!({ "glob": "**/*.md" })).await;
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    let paths: Vec<&str> = body["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, vec!["notes/a.md", "notes/sub/c.md"]);
    assert_eq!(body["truncated"], false);
}

#[tokio::test]
async fn search_filesystem_invalid_glob_returns_400_with_code() {
    let h = harness().await;
    let app = router(h.state.clone());
    let req = json_request(
        "POST",
        "/search/filesystem",
        json!({ "glob": "[unterminated" }),
    )
    .await;
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_glob");
}

#[tokio::test]
async fn search_content_returns_results_with_matches() {
    let h = harness().await;
    seed_files(
        h.pool(),
        vec![
            (
                "a.md",
                "first line\nsecond pgvector line",
                "2026-04-01T00:00:00Z",
            ),
            ("b.md", "no relevant content", "2026-04-01T00:00:00Z"),
        ],
    )
    .await;
    let app = router(h.state.clone());
    let req = json_request("POST", "/search/content", json!({ "query": "pgvector" })).await;
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    let results = body["results"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["path"], "a.md");
    assert_eq!(results[0]["match_count"], 1);
    let matches = results[0]["matches"].as_array().unwrap();
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0]["line"], 2);
    assert_eq!(matches[0]["text"], "second pgvector line");
}

#[tokio::test]
async fn search_content_invalid_regex_returns_400_with_code() {
    let h = harness().await;
    let app = router(h.state.clone());
    let req = json_request(
        "POST",
        "/search/content",
        json!({ "query": "[unterminated", "regex": true }),
    )
    .await;
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_regex");
}

#[tokio::test]
async fn search_response_populates_vault_and_vault_name() {
    // Step 9 onward: every search result carries `vault` (id) and
    // `vault_name`. Spec text amendment for the four search specs lands
    // in step 10's workplan-write per Resolution F.
    let h = harness().await;
    seed_files(
        h.pool(),
        vec![("a.md", "alpha pgvector", "2026-04-01T00:00:00Z")],
    )
    .await;
    let expected_id = h.vault_id.to_string();
    let expected_name = h.vault_name.clone();

    // Filesystem result entries carry vault + vault_name.
    let app = router(h.state.clone());
    let req = json_request("POST", "/search/filesystem", json!({})).await;
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    let entry = &body["results"][0];
    assert_eq!(entry["vault"].as_str(), Some(expected_id.as_str()));
    assert_eq!(entry["vault_name"].as_str(), Some(expected_name.as_str()));

    // Content result entries carry vault + vault_name.
    let app = router(h.state.clone());
    let req = json_request("POST", "/search/content", json!({ "query": "pgvector" })).await;
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    let entry = &body["results"][0];
    assert_eq!(entry["vault"].as_str(), Some(expected_id.as_str()));
    assert_eq!(entry["vault_name"].as_str(), Some(expected_name.as_str()));
}

// ===== Semantic-search test helpers =====

const DIM: usize = 768;

/// Test embedder yielding a single result on the first call. Mirrors the
/// `OneShotEmbedder` shape from `src/search/semantic.rs::tests` so handler
/// tests can inject either a specific non-zero vector or a chosen
/// `EmbeddingError`.
struct OneShotEmbedder {
    slot: Mutex<Option<Result<Vec<f32>, EmbeddingError>>>,
}

impl OneShotEmbedder {
    fn ok(v: Vec<f32>) -> Arc<Self> {
        Arc::new(Self {
            slot: Mutex::new(Some(Ok(v))),
        })
    }
    fn err(e: EmbeddingError) -> Arc<Self> {
        Arc::new(Self {
            slot: Mutex::new(Some(Err(e))),
        })
    }
}

impl Embedder for OneShotEmbedder {
    fn embed_text<'a>(&'a self, _text: &'a str) -> EmbedFuture<'a> {
        let r = self
            .slot
            .lock()
            .unwrap()
            .take()
            .expect("OneShotEmbedder called more than once");
        Box::pin(async move { r })
    }
}

fn unit_vec(positions: &[(usize, f32)]) -> Vec<f32> {
    let mut v = vec![0.0f32; DIM];
    for (i, x) in positions {
        v[*i] = *x;
    }
    v
}

async fn seed_chunk(
    pool: SqlitePool,
    file_path: &'static str,
    chunk_index: u32,
    heading_path: &'static str,
    content: &'static str,
    embedding: Vec<f32>,
) {
    task::spawn_blocking(move || {
        let mut conn = pool.get().unwrap();
        let tx = conn.transaction().unwrap();
        tx.execute(
            "INSERT INTO chunks (file_path, chunk_index, heading_path, content, content_hash, start_byte, end_byte, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                file_path,
                chunk_index,
                heading_path,
                content,
                "sha256:00",
                0i64,
                content.len() as i64,
                "2026-01-01T00:00:00Z"
            ],
        )
        .unwrap();
        let chunk_id = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO chunks_vec (chunk_id, embedding) VALUES (?1, ?2)",
            params![chunk_id, bytemuck::cast_slice::<f32, u8>(&embedding)],
        )
        .unwrap();
        tx.commit().unwrap();
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn semantic_handler_returns_200_with_results_for_seeded_chunks() {
    let h = harness_with_embedder(OneShotEmbedder::ok(unit_vec(&[(0, 1.0)]))).await;
    seed_files(
        h.pool(),
        vec![("a.md", "alpha body", "2026-04-01T00:00:00Z")],
    )
    .await;
    seed_chunk(
        h.pool(),
        "a.md",
        0,
        "Intro",
        "alpha body",
        unit_vec(&[(0, 1.0)]),
    )
    .await;

    let app = router(h.state.clone());
    let req = json_request("POST", "/search/semantic", json!({ "query": "alpha" })).await;
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    let results = body["results"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["file_path"], "a.md");
    assert_eq!(results[0]["chunk_index"], 0);
    assert_eq!(results[0]["heading_path"], json!(["Intro"]));
    assert_eq!(results[0]["text"], "alpha body");
    assert_eq!(results[0]["content_hash"], "sha256:00");
    assert!(body.get("hint").is_none() || body["hint"].is_null());
}

#[tokio::test]
async fn semantic_handler_returns_503_for_embedding_unavailable() {
    let embedder = OneShotEmbedder::err(EmbeddingError::Status {
        code: 503,
        body: "service unavailable".to_string(),
    });
    let h = harness_with_embedder(embedder).await;
    let app = router(h.state.clone());
    let req = json_request("POST", "/search/semantic", json!({ "query": "alpha" })).await;
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"]["code"], "embedding_unavailable");
}

#[tokio::test]
async fn semantic_handler_returns_400_for_invalid_prefix() {
    let h = harness().await;
    let app = router(h.state.clone());
    let req = json_request(
        "POST",
        "/search/semantic",
        json!({ "query": "alpha", "prefix": "/abs" }),
    )
    .await;
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_prefix");
}

#[tokio::test]
async fn semantic_handler_populates_vault_and_vault_name() {
    let h = harness_with_embedder(OneShotEmbedder::ok(unit_vec(&[(0, 1.0)]))).await;
    seed_files(
        h.pool(),
        vec![("a.md", "alpha body", "2026-04-01T00:00:00Z")],
    )
    .await;
    seed_chunk(
        h.pool(),
        "a.md",
        0,
        "Intro",
        "alpha body",
        unit_vec(&[(0, 1.0)]),
    )
    .await;
    let expected_id = h.vault_id.to_string();
    let expected_name = h.vault_name.clone();

    let app = router(h.state.clone());
    let req = json_request("POST", "/search/semantic", json!({ "query": "alpha" })).await;
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    let entry = &body["results"][0];
    assert_eq!(entry["vault"].as_str(), Some(expected_id.as_str()));
    assert_eq!(entry["vault_name"].as_str(), Some(expected_name.as_str()));
}

#[tokio::test]
async fn semantic_handler_returns_hint_when_index_empty_and_files_present() {
    let h = harness_with_embedder(OneShotEmbedder::ok(unit_vec(&[(0, 1.0)]))).await;
    seed_files(h.pool(), vec![("a.md", "alpha", "2026-04-01T00:00:00Z")]).await;
    let app = router(h.state.clone());
    let req = json_request("POST", "/search/semantic", json!({ "query": "alpha" })).await;
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["results"].as_array().unwrap().is_empty());
    assert_eq!(body["hint"], "semantic index is building");
}

#[tokio::test]
async fn semantic_handler_clamps_min_similarity_to_unit_range() {
    // Out-of-range min_similarity must not error; it clamps to [0.0, 1.0].
    // 1.5 clamps to 1.0, which filters out an orthogonal seeded chunk
    // (orthogonal cosine score is 0.5).
    let h = harness_with_embedder(OneShotEmbedder::ok(unit_vec(&[(0, 1.0)]))).await;
    seed_files(
        h.pool(),
        vec![("a.md", "orthogonal", "2026-04-01T00:00:00Z")],
    )
    .await;
    seed_chunk(h.pool(), "a.md", 0, "", "orthogonal", unit_vec(&[(1, 1.0)])).await;
    let app = router(h.state.clone());
    let req = json_request(
        "POST",
        "/search/semantic",
        json!({ "query": "alpha", "min_similarity": 1.5 }),
    )
    .await;
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["results"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn semantic_handler_default_limit_caps_at_10() {
    // Semantic default is DEFAULT_SEMANTIC_LIMIT == 10; filesystem and content
    // stay at 100. Seed 15 identical-vector chunks, omit `limit`, expect
    // exactly 10 results.
    let h = harness_with_embedder(OneShotEmbedder::ok(unit_vec(&[(0, 1.0)]))).await;
    let pool = h.pool();
    task::spawn_blocking(move || {
        let mut conn = pool.get().unwrap();
        let tx = conn.transaction().unwrap();
        for i in 0u32..15 {
            let path = format!("f{i:03}.md");
            tx.execute(
                "INSERT INTO files (path, size, mtime, content_hash, indexed_at, content) \
                 VALUES (?1, 1, '2026-01-01T00:00:00Z', 'sha256:00', '2026-01-01T00:00:00Z', '')",
                params![path],
            )
            .unwrap();
            tx.execute(
                "INSERT INTO chunks (file_path, chunk_index, heading_path, content, content_hash, start_byte, end_byte, created_at) \
                 VALUES (?1, 0, '', 'x', 'sha256:00', 0, 1, '2026-01-01T00:00:00Z')",
                params![path],
            )
            .unwrap();
            let chunk_id = tx.last_insert_rowid();
            let v = {
                let mut v = vec![0.0f32; DIM];
                v[0] = 1.0;
                v
            };
            tx.execute(
                "INSERT INTO chunks_vec (chunk_id, embedding) VALUES (?1, ?2)",
                params![chunk_id, bytemuck::cast_slice::<f32, u8>(&v)],
            )
            .unwrap();
        }
        tx.commit().unwrap();
    })
    .await
    .unwrap();

    let app = router(h.state.clone());
    let req = json_request("POST", "/search/semantic", json!({ "query": "alpha" })).await;
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["results"].as_array().unwrap().len(), 10);
}

#[tokio::test]
async fn semantic_explicit_limit_returns_up_to_limit() {
    // Explicit limit overrides the semantic default. Seed 20 chunks, request
    // limit=17, expect exactly 17 results.
    let h = harness_with_embedder(OneShotEmbedder::ok(unit_vec(&[(0, 1.0)]))).await;
    let pool = h.pool();
    task::spawn_blocking(move || {
        let mut conn = pool.get().unwrap();
        let tx = conn.transaction().unwrap();
        for i in 0u32..20 {
            let path = format!("g{i:03}.md");
            tx.execute(
                "INSERT INTO files (path, size, mtime, content_hash, indexed_at, content) \
                 VALUES (?1, 1, '2026-01-01T00:00:00Z', 'sha256:00', '2026-01-01T00:00:00Z', '')",
                params![path],
            )
            .unwrap();
            tx.execute(
                "INSERT INTO chunks (file_path, chunk_index, heading_path, content, content_hash, start_byte, end_byte, created_at) \
                 VALUES (?1, 0, '', 'x', 'sha256:00', 0, 1, '2026-01-01T00:00:00Z')",
                params![path],
            )
            .unwrap();
            let chunk_id = tx.last_insert_rowid();
            let v = {
                let mut v = vec![0.0f32; DIM];
                v[0] = 1.0;
                v
            };
            tx.execute(
                "INSERT INTO chunks_vec (chunk_id, embedding) VALUES (?1, ?2)",
                params![chunk_id, bytemuck::cast_slice::<f32, u8>(&v)],
            )
            .unwrap();
        }
        tx.commit().unwrap();
    })
    .await
    .unwrap();

    let app = router(h.state.clone());
    let req = json_request(
        "POST",
        "/search/semantic",
        json!({ "query": "alpha", "limit": 17 }),
    )
    .await;
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["results"].as_array().unwrap().len(), 17);
}

#[tokio::test]
async fn semantic_truncated_true_when_results_exceed_limit() {
    // Seed 15 chunks, request limit=10 (the default). The per-vault kNN
    // returns 10 rows (k=10), hitting the cap → truncated: true.
    let h = harness_with_embedder(OneShotEmbedder::ok(unit_vec(&[(0, 1.0)]))).await;
    let pool = h.pool();
    task::spawn_blocking(move || {
        let mut conn = pool.get().unwrap();
        let tx = conn.transaction().unwrap();
        for i in 0u32..15 {
            let path = format!("t{i:03}.md");
            tx.execute(
                "INSERT INTO files (path, size, mtime, content_hash, indexed_at, content) \
                 VALUES (?1, 1, '2026-01-01T00:00:00Z', 'sha256:00', '2026-01-01T00:00:00Z', '')",
                params![path],
            )
            .unwrap();
            tx.execute(
                "INSERT INTO chunks (file_path, chunk_index, heading_path, content, content_hash, start_byte, end_byte, created_at) \
                 VALUES (?1, 0, '', 'x', 'sha256:00', 0, 1, '2026-01-01T00:00:00Z')",
                params![path],
            )
            .unwrap();
            let chunk_id = tx.last_insert_rowid();
            let v = {
                let mut v = vec![0.0f32; DIM];
                v[0] = 1.0;
                v
            };
            tx.execute(
                "INSERT INTO chunks_vec (chunk_id, embedding) VALUES (?1, ?2)",
                params![chunk_id, bytemuck::cast_slice::<f32, u8>(&v)],
            )
            .unwrap();
        }
        tx.commit().unwrap();
    })
    .await
    .unwrap();

    let app = router(h.state.clone());
    let req = json_request("POST", "/search/semantic", json!({ "query": "alpha" })).await;
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["results"].as_array().unwrap().len(), 10);
    assert_eq!(body["truncated"], true);
}

#[tokio::test]
async fn semantic_truncated_false_when_results_within_limit() {
    // Seed 3 chunks, request limit=10 (the default). The per-vault kNN
    // returns 3 rows (k=10, fewer available) → truncated: false.
    let h = harness_with_embedder(OneShotEmbedder::ok(unit_vec(&[(0, 1.0)]))).await;
    seed_files(
        h.pool(),
        vec![
            ("u1.md", "x", "2026-04-01T00:00:00Z"),
            ("u2.md", "x", "2026-04-01T00:00:00Z"),
            ("u3.md", "x", "2026-04-01T00:00:00Z"),
        ],
    )
    .await;
    seed_chunk(h.pool(), "u1.md", 0, "", "x", unit_vec(&[(0, 1.0)])).await;
    seed_chunk(h.pool(), "u2.md", 0, "", "x", unit_vec(&[(0, 1.0)])).await;
    seed_chunk(h.pool(), "u3.md", 0, "", "x", unit_vec(&[(0, 1.0)])).await;

    let app = router(h.state.clone());
    let req = json_request("POST", "/search/semantic", json!({ "query": "alpha" })).await;
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["results"].as_array().unwrap().len(), 3);
    assert_eq!(body["truncated"], false);
}

// ===== Vault control-plane handler tests =====

struct VaultHarness {
    _root: TempDir,
    state: ApiState,
    _shutdown_tx: tokio::sync::watch::Sender<bool>,
}

fn vault_test_config(data_dir: &std::path::Path) -> Config {
    Config {
        vault: None,
        http: HttpConfig::default(),
        mcp: McpConfig::default(),
        embedding: EmbeddingConfig::default(),
        watcher: WatcherConfig::default(),
        storage: StorageConfig {
            data_dir: ConfigPath(data_dir.to_path_buf()),
            index_file: "index.sqlite".to_string(),
        },
        logging: LoggingConfig::default(),
        default_vault_name: "default".to_string(),
    }
}

async fn vault_harness() -> VaultHarness {
    let root = TempDir::new().unwrap();
    let data_dir = root.path().join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let config = Arc::new(vault_test_config(&data_dir));
    let registry = Arc::new(VaultRegistry::open(&data_dir).await.unwrap());
    let embedder: Arc<dyn Embedder> = Arc::new(StubEmbedder::new(768));
    let (tx, rx) = tokio::sync::watch::channel(false);
    let manager = VaultManager::open(registry, config, embedder, 768, rx)
        .await
        .unwrap();
    let manager = Arc::new(manager);
    let state = ApiState {
        vault_manager: manager.clone(),
        event_bus: manager.event_bus(),
    };
    VaultHarness {
        _root: root,
        state,
        _shutdown_tx: tx,
    }
}

fn fresh_dir(parent: &std::path::Path, name: &str) -> std::path::PathBuf {
    let p = parent.join(name);
    std::fs::create_dir_all(&p).unwrap();
    p
}

async fn create_vault_via_api(
    state: &ApiState,
    name: Option<&str>,
    path: &std::path::Path,
) -> Value {
    let mut body = json!({ "path": path.display().to_string() });
    if let Some(n) = name {
        body["name"] = json!(n);
    }
    let app = router(state.clone());
    let req = json_request("POST", "/vaults", body).await;
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "create returned {body}");
    body
}

#[tokio::test]
async fn post_vaults_returns_200_on_create() {
    let h = vault_harness().await;
    let path = fresh_dir(h._root.path(), "v");
    let app = router(h.state.clone());
    let req = json_request(
        "POST",
        "/vaults",
        json!({ "name": "first", "path": path.display().to_string() }),
    )
    .await;
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], "first");
    assert_eq!(body["status"], "active");
    assert!(
        body["id"].as_str().is_some_and(|s| !s.is_empty()),
        "id present"
    );
    assert!(body["created_at"].as_str().is_some(), "created_at present");
}

#[tokio::test]
async fn post_vaults_returns_409_on_path_conflict() {
    let h = vault_harness().await;
    let path = fresh_dir(h._root.path(), "shared");
    create_vault_via_api(&h.state, Some("first"), &path).await;

    let app = router(h.state.clone());
    let req = json_request(
        "POST",
        "/vaults",
        json!({ "name": "second", "path": path.display().to_string() }),
    )
    .await;
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "vault_path_conflict");
}

#[tokio::test]
async fn post_vaults_returns_409_on_name_conflict() {
    let h = vault_harness().await;
    let path_a = fresh_dir(h._root.path(), "a");
    let path_b = fresh_dir(h._root.path(), "b");
    create_vault_via_api(&h.state, Some("shared"), &path_a).await;

    let app = router(h.state.clone());
    let req = json_request(
        "POST",
        "/vaults",
        json!({ "name": "shared", "path": path_b.display().to_string() }),
    )
    .await;
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "vault_name_conflict");
}

#[tokio::test]
async fn post_vaults_returns_422_on_invalid_path() {
    let h = vault_harness().await;
    let nonexistent = h._root.path().join("does-not-exist");
    let app = router(h.state.clone());
    let req = json_request(
        "POST",
        "/vaults",
        json!({ "name": "v", "path": nonexistent.display().to_string() }),
    )
    .await;
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "vault_path_invalid");
}

#[tokio::test]
async fn get_vaults_returns_list() {
    let h = vault_harness().await;
    let path_a = fresh_dir(h._root.path(), "a");
    let path_b = fresh_dir(h._root.path(), "b");
    create_vault_via_api(&h.state, Some("alpha"), &path_a).await;
    create_vault_via_api(&h.state, Some("bravo"), &path_b).await;

    let app = router(h.state.clone());
    let req = Request::builder()
        .method("GET")
        .uri("/vaults")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    let vaults = body["vaults"].as_array().unwrap();
    assert_eq!(vaults.len(), 2);
    let mut names: Vec<&str> = vaults.iter().map(|v| v["name"].as_str().unwrap()).collect();
    names.sort();
    assert_eq!(names, vec!["alpha", "bravo"]);
}

#[tokio::test]
async fn get_vaults_id_returns_single() {
    let h = vault_harness().await;
    let path = fresh_dir(h._root.path(), "v");
    let created = create_vault_via_api(&h.state, Some("solo"), &path).await;
    let id = created["id"].as_str().unwrap();

    // By id.
    let app = router(h.state.clone());
    let req = Request::builder()
        .method("GET")
        .uri(format!("/vaults/{id}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], id);
    assert_eq!(body["name"], "solo");

    // By name.
    let app = router(h.state.clone());
    let req = Request::builder()
        .method("GET")
        .uri("/vaults/solo")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], id);
    assert_eq!(body["name"], "solo");
}

#[tokio::test]
async fn get_vaults_unknown_returns_404_with_hint() {
    let h = vault_harness().await;
    let path = fresh_dir(h._root.path(), "v");
    create_vault_via_api(&h.state, Some("personal"), &path).await;

    let app = router(h.state.clone());
    let req = Request::builder()
        .method("GET")
        .uri("/vaults/personol")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "vault_not_found");
    let msg = body["error"]["message"].as_str().unwrap();
    assert!(
        msg.contains("personal"),
        "expected closest-name hint 'personal' in message: {msg}"
    );
}

#[tokio::test]
async fn delete_vaults_returns_200_on_terminate() {
    let h = vault_harness().await;
    let path = fresh_dir(h._root.path(), "v");
    let created = create_vault_via_api(&h.state, Some("doomed"), &path).await;
    let id = created["id"].as_str().unwrap().to_string();

    let app = router(h.state.clone());
    let req = Request::builder()
        .method("DELETE")
        .uri("/vaults/doomed")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["terminated"], true);
    assert_eq!(body["id"], id);

    // Confirm gone via GET.
    let app = router(h.state.clone());
    let req = Request::builder()
        .method("GET")
        .uri("/vaults/doomed")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let (status, _) = body_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_vaults_unknown_returns_404() {
    let h = vault_harness().await;
    let app = router(h.state.clone());
    let req = Request::builder()
        .method("DELETE")
        .uri("/vaults/missing")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "vault_not_found");
}

#[tokio::test]
async fn search_filesystem_invalid_request_body_returns_400() {
    let h = harness().await;
    let app = router(h.state.clone());
    let req = Request::builder()
        .method("POST")
        .uri("/search/filesystem")
        .header("content-type", "application/json")
        .body(Body::from("{not valid json"))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_request");
}

// ===== Cross-vault search tests =====
//
// Pinned to `docs/specs/vault-management.md § Cross-Vault Search Semantics`
// (workplan § A's eight resolutions). Fixtures use `for_tests_full` to set
// up multiple active vaults plus optional paused/errored row stubs without
// spinning up a live `VaultManager::open` (which would need a registry +
// embedder + watcher per vault).

struct MultiVaultHarness {
    _root: TempDir,
    _vault_dirs: Vec<TempDir>,
    state: ApiState,
    pools: Vec<SqlitePool>,
    ids: Vec<VaultId>,
    #[allow(dead_code)]
    names: Vec<String>,
}

#[derive(Debug, Clone)]
struct InactiveStub {
    name: String,
    status: VaultStatus,
    last_error: Option<String>,
}

async fn multi_vault_harness(active_names: Vec<&'static str>) -> MultiVaultHarness {
    multi_vault_harness_with(active_names, vec![], Arc::new(StubEmbedder::new(DIM))).await
}

async fn multi_vault_harness_with(
    active_names: Vec<&'static str>,
    inactive: Vec<InactiveStub>,
    embedder: Arc<dyn Embedder>,
) -> MultiVaultHarness {
    let root = TempDir::new().unwrap();
    let mut pools: Vec<SqlitePool> = Vec::new();
    let mut ids: Vec<VaultId> = Vec::new();
    let mut names: Vec<String> = Vec::new();
    let mut entries: Vec<VaultEntry> = Vec::new();
    let mut vault_dirs: Vec<TempDir> = Vec::new();
    for name in &active_names {
        let vault_dir = TempDir::new().unwrap();
        let vault_id = VaultId::new();
        let store = Store::open(
            &vault_id,
            root.path(),
            "index.sqlite",
            &EmbeddingConfig::default(),
        )
        .await
        .unwrap();
        let store = Arc::new(store);
        let pool = store.pool();
        let entry = VaultEntry {
            id: vault_id.clone(),
            name: (*name).to_string(),
            vault_path: vault_dir.path().to_path_buf(),
            store,
            status: VaultStatus::Active,
        };
        pools.push(pool);
        ids.push(vault_id);
        names.push((*name).to_string());
        entries.push(entry);
        vault_dirs.push(vault_dir);
    }
    let inactive_rows: Vec<VaultRow> = inactive
        .into_iter()
        .map(|stub| VaultRow {
            id: VaultId::new(),
            name: stub.name,
            path: std::path::PathBuf::from("/dev/null"),
            status: stub.status,
            created_at: chrono::Utc::now(),
            last_error: stub.last_error,
        })
        .collect();
    let manager = Arc::new(VaultManager::for_tests_full(
        entries,
        inactive_rows,
        embedder,
        DIM as u32,
    ));
    let state = ApiState {
        vault_manager: manager.clone(),
        event_bus: manager.event_bus(),
    };
    MultiVaultHarness {
        _root: root,
        _vault_dirs: vault_dirs,
        state,
        pools,
        ids,
        names,
    }
}

/// Test embedder that returns a fixed unit vector on every call. Sufficient
/// for cross-vault semantic tests where each per-vault call to
/// `search_semantic` re-invokes the embedder; `OneShotEmbedder` errors on
/// the second call which would mask the per-vault iteration shape.
struct FixedEmbedder {
    vector: Vec<f32>,
}

impl FixedEmbedder {
    fn new(positions: &[(usize, f32)]) -> Arc<Self> {
        Arc::new(Self {
            vector: unit_vec(positions),
        })
    }
}

impl Embedder for FixedEmbedder {
    fn embed_text<'a>(&'a self, _text: &'a str) -> EmbedFuture<'a> {
        let v = self.vector.clone();
        Box::pin(async move { Ok(v) })
    }
}

#[tokio::test]
async fn cross_vault_filesystem_results_global_path_sorted() {
    let h = multi_vault_harness(vec!["alpha", "bravo"]).await;
    seed_files(
        h.pools[0].clone(),
        vec![
            ("notes/a.md", "alpha", "2026-04-01T00:00:00Z"),
            ("notes/m.md", "alpha-m", "2026-04-01T00:00:00Z"),
        ],
    )
    .await;
    seed_files(
        h.pools[1].clone(),
        vec![
            ("notes/b.md", "bravo-b", "2026-04-01T00:00:00Z"),
            ("notes/z.md", "bravo-z", "2026-04-01T00:00:00Z"),
        ],
    )
    .await;
    let app = router(h.state.clone());
    let req = json_request("POST", "/search/filesystem", json!({ "glob": "**/*.md" })).await;
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    let paths: Vec<&str> = body["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["path"].as_str().unwrap())
        .collect();
    assert_eq!(
        paths,
        vec!["notes/a.md", "notes/b.md", "notes/m.md", "notes/z.md"]
    );
    assert!(body.get("partial_results").is_none() || body["partial_results"].is_null());
}

#[tokio::test]
async fn cross_vault_content_results_global_path_sorted() {
    let h = multi_vault_harness(vec!["alpha", "bravo"]).await;
    seed_files(
        h.pools[0].clone(),
        vec![
            ("docs/a.md", "needle alpha", "2026-04-01T00:00:00Z"),
            ("docs/m.md", "needle alpha-m", "2026-04-01T00:00:00Z"),
        ],
    )
    .await;
    seed_files(
        h.pools[1].clone(),
        vec![
            ("docs/b.md", "needle bravo", "2026-04-01T00:00:00Z"),
            ("docs/z.md", "needle bravo-z", "2026-04-01T00:00:00Z"),
        ],
    )
    .await;
    let app = router(h.state.clone());
    let req = json_request("POST", "/search/content", json!({ "query": "needle" })).await;
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    let paths: Vec<&str> = body["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["path"].as_str().unwrap())
        .collect();
    assert_eq!(
        paths,
        vec!["docs/a.md", "docs/b.md", "docs/m.md", "docs/z.md"]
    );
    assert!(body.get("partial_results").is_none() || body["partial_results"].is_null());
}

#[tokio::test]
async fn cross_vault_semantic_results_score_desc_sorted() {
    // Two vaults, each with one chunk. Different cosine scores against the
    // query vector; merged response must be score-descending.
    let embedder = FixedEmbedder::new(&[(0, 1.0)]);
    let h = multi_vault_harness_with(vec!["alpha", "bravo"], vec![], embedder).await;
    // Vault 0 chunk: orthogonal to query → cosine 0.5 after [-1, 1] -> [0, 1]
    // mapping (i.e. small-but-nonzero score).
    seed_chunk(
        h.pools[0].clone(),
        "alpha.md",
        0,
        "Intro",
        "alpha body",
        unit_vec(&[(1, 1.0)]),
    )
    .await;
    // Vault 1 chunk: parallel to query → cosine 1.0 → score 1.0.
    seed_chunk(
        h.pools[1].clone(),
        "bravo.md",
        0,
        "Intro",
        "bravo body",
        unit_vec(&[(0, 1.0)]),
    )
    .await;

    let app = router(h.state.clone());
    let req = json_request("POST", "/search/semantic", json!({ "query": "any" })).await;
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    let results = body["results"].as_array().unwrap();
    assert_eq!(results.len(), 2);
    // Higher score first.
    let s0 = results[0]["score"].as_f64().unwrap();
    let s1 = results[1]["score"].as_f64().unwrap();
    assert!(s0 >= s1, "expected score-desc but got {s0} then {s1}");
    assert_eq!(results[0]["file_path"], "bravo.md");
    assert_eq!(results[1]["file_path"], "alpha.md");
}

#[tokio::test]
async fn vaults_filter_narrows_to_subset_by_name() {
    let h = multi_vault_harness(vec!["alpha", "bravo"]).await;
    seed_files(
        h.pools[0].clone(),
        vec![("a.md", "x", "2026-04-01T00:00:00Z")],
    )
    .await;
    seed_files(
        h.pools[1].clone(),
        vec![("b.md", "x", "2026-04-01T00:00:00Z")],
    )
    .await;
    let app = router(h.state.clone());
    let req = json_request(
        "POST",
        "/search/filesystem",
        json!({ "glob": "**/*.md", "vaults": ["alpha"] }),
    )
    .await;
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    let paths: Vec<&str> = body["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, vec!["a.md"]);
    let vault_names: Vec<&str> = body["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["vault_name"].as_str().unwrap())
        .collect();
    assert_eq!(vault_names, vec!["alpha"]);
}

#[tokio::test]
async fn vaults_filter_narrows_to_subset_by_id() {
    let h = multi_vault_harness(vec!["alpha", "bravo"]).await;
    seed_files(
        h.pools[0].clone(),
        vec![("a.md", "x", "2026-04-01T00:00:00Z")],
    )
    .await;
    seed_files(
        h.pools[1].clone(),
        vec![("b.md", "x", "2026-04-01T00:00:00Z")],
    )
    .await;
    let target_id = h.ids[1].to_string();
    let app = router(h.state.clone());
    let req = json_request(
        "POST",
        "/search/filesystem",
        json!({ "glob": "**/*.md", "vaults": [target_id.clone()] }),
    )
    .await;
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    let paths: Vec<&str> = body["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, vec!["b.md"]);
    assert_eq!(body["results"][0]["vault"], target_id);
}

#[tokio::test]
async fn vaults_filter_unknown_name_appears_in_partial_results_failed() {
    let h = multi_vault_harness(vec!["alpha"]).await;
    seed_files(
        h.pools[0].clone(),
        vec![("a.md", "x", "2026-04-01T00:00:00Z")],
    )
    .await;
    let app = router(h.state.clone());
    let req = json_request(
        "POST",
        "/search/filesystem",
        json!({ "glob": "**/*.md", "vaults": ["alpha", "ghost"] }),
    )
    .await;
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    let paths: Vec<&str> = body["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, vec!["a.md"]);
    let failed = body["partial_results"]["failed"].as_array().unwrap();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0]["code"], "vault_not_found");
    assert_eq!(failed[0]["vault"], "ghost");
}

#[tokio::test]
async fn vaults_filter_empty_array_returns_invalid_request() {
    let h = multi_vault_harness(vec!["alpha"]).await;
    let app = router(h.state.clone());
    let req = json_request(
        "POST",
        "/search/filesystem",
        json!({ "glob": "**/*.md", "vaults": [] }),
    )
    .await;
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_request");
}

#[tokio::test]
async fn paused_vault_skipped_with_partial_results_diagnostic() {
    let h = multi_vault_harness_with(
        vec!["alpha"],
        vec![InactiveStub {
            name: "bravo".to_string(),
            status: VaultStatus::Paused,
            last_error: None,
        }],
        Arc::new(StubEmbedder::new(DIM)),
    )
    .await;
    seed_files(
        h.pools[0].clone(),
        vec![("a.md", "x", "2026-04-01T00:00:00Z")],
    )
    .await;
    let app = router(h.state.clone());
    let req = json_request("POST", "/search/filesystem", json!({ "glob": "**/*.md" })).await;
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    // Active vault still returns results.
    assert_eq!(body["results"].as_array().unwrap().len(), 1);
    // Paused vault appears in skipped.
    let skipped = body["partial_results"]["skipped"].as_array().unwrap();
    assert_eq!(skipped.len(), 1);
    assert_eq!(skipped[0]["status"], "paused");
    assert_eq!(skipped[0]["vault_name"], "bravo");
    assert_eq!(skipped[0]["reason"], "vault is paused");
}

#[tokio::test]
async fn errored_vault_skipped_with_last_error_propagated() {
    let h = multi_vault_harness_with(
        vec!["alpha"],
        vec![InactiveStub {
            name: "broken".to_string(),
            status: VaultStatus::Errored,
            last_error: Some("vault path /home/foo no longer accessible".to_string()),
        }],
        Arc::new(StubEmbedder::new(DIM)),
    )
    .await;
    let app = router(h.state.clone());
    let req = json_request("POST", "/search/filesystem", json!({ "glob": "**/*.md" })).await;
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    let skipped = body["partial_results"]["skipped"].as_array().unwrap();
    assert_eq!(skipped.len(), 1);
    assert_eq!(skipped[0]["status"], "errored");
    let reason = skipped[0]["reason"].as_str().unwrap();
    assert!(
        reason.contains("vault path /home/foo no longer accessible"),
        "expected last_error propagated, got: {reason}"
    );
    assert!(reason.starts_with("vault is errored: "));
}

#[tokio::test]
async fn partial_results_omitted_when_all_active() {
    let h = multi_vault_harness(vec!["alpha", "bravo"]).await;
    seed_files(
        h.pools[0].clone(),
        vec![("a.md", "x", "2026-04-01T00:00:00Z")],
    )
    .await;
    seed_files(
        h.pools[1].clone(),
        vec![("b.md", "x", "2026-04-01T00:00:00Z")],
    )
    .await;
    let app = router(h.state.clone());
    let req = json_request("POST", "/search/filesystem", json!({ "glob": "**/*.md" })).await;
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    // The field is `skip_serializing_if = Option::is_none` → absent on the wire.
    assert!(
        body.get("partial_results").is_none(),
        "expected partial_results absent when all vaults active, got: {body}"
    );
}

#[tokio::test]
async fn truncated_true_when_global_limit_capped_after_merge() {
    let h = multi_vault_harness(vec!["alpha", "bravo"]).await;
    seed_files(
        h.pools[0].clone(),
        vec![
            ("a1.md", "x", "2026-04-01T00:00:00Z"),
            ("a2.md", "x", "2026-04-01T00:00:00Z"),
        ],
    )
    .await;
    seed_files(
        h.pools[1].clone(),
        vec![
            ("b1.md", "x", "2026-04-01T00:00:00Z"),
            ("b2.md", "x", "2026-04-01T00:00:00Z"),
        ],
    )
    .await;
    let app = router(h.state.clone());
    let req = json_request(
        "POST",
        "/search/filesystem",
        json!({ "glob": "**/*.md", "limit": 2 }),
    )
    .await;
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["results"].as_array().unwrap().len(), 2);
    assert_eq!(body["truncated"], true);
}

#[tokio::test]
async fn cross_vault_path_collision_breaks_tie_by_vault_id() {
    // Same path indexed in both vaults → identical sort key. Tie-break by
    // vault_id (UUIDv7 → creation-time-stable). The first-created vault has
    // the lexicographically-smaller id, so its result lands first.
    let h = multi_vault_harness(vec!["alpha", "bravo"]).await;
    let id_alpha = h.ids[0].to_string();
    let id_bravo = h.ids[1].to_string();
    assert!(
        id_alpha < id_bravo,
        "UUIDv7 invariant: first-created id is smaller (got alpha={id_alpha}, bravo={id_bravo})"
    );

    seed_files(
        h.pools[0].clone(),
        vec![("notes/shared.md", "x", "2026-04-01T00:00:00Z")],
    )
    .await;
    seed_files(
        h.pools[1].clone(),
        vec![("notes/shared.md", "x", "2026-04-01T00:00:00Z")],
    )
    .await;
    let app = router(h.state.clone());
    let req = json_request("POST", "/search/filesystem", json!({ "glob": "**/*.md" })).await;
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    let results = body["results"].as_array().unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0]["path"], "notes/shared.md");
    assert_eq!(results[1]["path"], "notes/shared.md");
    assert_eq!(results[0]["vault"], id_alpha);
    assert_eq!(results[1]["vault"], id_bravo);
}

#[tokio::test]
async fn merged_limit_applied_after_per_vault_search() {
    // Each vault returns up to `limit` rows; the merged set is then capped
    // at the same `limit`. Per-vault search reports `truncated` only if it
    // hits its own cap; with 2 rows per vault and limit=2, no per-vault
    // truncation, but the merge of 4 → 2 capped.
    let h = multi_vault_harness(vec!["alpha", "bravo"]).await;
    seed_files(
        h.pools[0].clone(),
        vec![
            ("aa.md", "x", "2026-04-01T00:00:00Z"),
            ("am.md", "x", "2026-04-01T00:00:00Z"),
        ],
    )
    .await;
    seed_files(
        h.pools[1].clone(),
        vec![
            ("ba.md", "x", "2026-04-01T00:00:00Z"),
            ("bm.md", "x", "2026-04-01T00:00:00Z"),
        ],
    )
    .await;
    let app = router(h.state.clone());
    let req = json_request(
        "POST",
        "/search/filesystem",
        json!({ "glob": "**/*.md", "limit": 2 }),
    )
    .await;
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    let paths: Vec<&str> = body["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["path"].as_str().unwrap())
        .collect();
    // Global path-sort: aa, am, ba, bm → first two are aa, am.
    assert_eq!(paths, vec!["aa.md", "am.md"]);
    assert_eq!(body["truncated"], true);
}

#[tokio::test]
async fn semantic_search_assumes_same_dimension_across_vaults() {
    // The daemon's embedding service is configured per-daemon; all vaults
    // share the same dimension. The defensive path: if a per-vault search
    // somehow errored on a storage-level dimension issue, the error would
    // appear in `partial_results.failed` (code `vault_search_failed`) and
    // the other vault still returns. We force this by dropping the
    // `chunks_vec` table on one vault — sqlite-vec then errors on MATCH.
    let embedder = FixedEmbedder::new(&[(0, 1.0)]);
    let h = multi_vault_harness_with(vec!["alpha", "bravo"], vec![], embedder).await;
    // Vault 0: real chunk that will succeed.
    seed_chunk(
        h.pools[0].clone(),
        "ok.md",
        0,
        "Intro",
        "alpha body",
        unit_vec(&[(0, 1.0)]),
    )
    .await;
    // Vault 1: drop chunks_vec so any MATCH errors at the SQL layer.
    let pool = h.pools[1].clone();
    task::spawn_blocking(move || {
        let conn = pool.get().unwrap();
        conn.execute("DROP TABLE chunks_vec", []).unwrap();
    })
    .await
    .unwrap();

    let app = router(h.state.clone());
    let req = json_request("POST", "/search/semantic", json!({ "query": "x" })).await;
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "should not crash; got {body}");
    // Vault 0's chunk survives in results.
    let results = body["results"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["file_path"], "ok.md");
    // Vault 1 lands in failed.
    let failed = body["partial_results"]["failed"].as_array().unwrap();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0]["code"], "vault_search_failed");
    assert_eq!(failed[0]["vault_name"], "bravo");
}

// ===== Step 11 · Task 11.3: lifecycle handler tests =====
//
// Five new POST routes (`pause`, `resume`, `reset`, `rename`, `rescan`) on
// top of `vault_harness()` (full `VaultManager::open` so the spawn ctx is
// available). One dedicated `errored_vault_harness` covers the
// path-inaccessible resume case by pre-inserting an Errored row before
// `VaultManager::open` reconciles.

async fn errored_vault_harness(
    errored_name: &str,
    last_error: &str,
) -> (VaultHarness, std::path::PathBuf) {
    let root = TempDir::new().unwrap();
    let data_dir = root.path().join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let config = Arc::new(vault_test_config(&data_dir));
    let registry = Arc::new(VaultRegistry::open(&data_dir).await.unwrap());

    let bogus = root.path().join("not-there");
    let id = VaultId::new();
    registry
        .insert(VaultRow {
            id,
            name: errored_name.to_string(),
            path: bogus.clone(),
            status: VaultStatus::Errored,
            created_at: chrono::Utc::now(),
            last_error: Some(last_error.to_string()),
        })
        .await
        .unwrap();

    let embedder: Arc<dyn Embedder> = Arc::new(StubEmbedder::new(768));
    let (tx, rx) = tokio::sync::watch::channel(false);
    let manager = VaultManager::open(registry, config, embedder, 768, rx)
        .await
        .unwrap();
    let manager = Arc::new(manager);
    let state = ApiState {
        vault_manager: manager.clone(),
        event_bus: manager.event_bus(),
    };
    (
        VaultHarness {
            _root: root,
            state,
            _shutdown_tx: tx,
        },
        bogus,
    )
}

async fn count_chunks_via_manager(state: &ApiState) -> i64 {
    let entries = state.vault_manager.active_vaults();
    let pool = entries[0].store.pool();
    task::spawn_blocking(move || -> i64 {
        let conn = pool.get().unwrap();
        conn.query_row("SELECT COUNT(*) FROM chunks", [], |r| r.get(0))
            .unwrap()
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn post_vaults_pause_returns_200_with_updated_row() {
    let h = vault_harness().await;
    let path = fresh_dir(h._root.path(), "v");
    create_vault_via_api(&h.state, Some("paused-target"), &path).await;

    let app = router(h.state.clone());
    let req = Request::builder()
        .method("POST")
        .uri("/vaults/paused-target/pause")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], "paused-target");
    assert_eq!(body["status"], "paused");
}

#[tokio::test]
async fn post_vaults_pause_unknown_returns_404() {
    let h = vault_harness().await;
    let app = router(h.state.clone());
    let req = Request::builder()
        .method("POST")
        .uri("/vaults/missing/pause")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "vault_not_found");
}

#[tokio::test]
async fn post_vaults_resume_returns_200_with_updated_row() {
    let h = vault_harness().await;
    let path = fresh_dir(h._root.path(), "v");
    create_vault_via_api(&h.state, Some("resume-target"), &path).await;

    // Pause first so resume has work to do.
    let app = router(h.state.clone());
    let req = Request::builder()
        .method("POST")
        .uri("/vaults/resume-target/pause")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let app = router(h.state.clone());
    let req = Request::builder()
        .method("POST")
        .uri("/vaults/resume-target/resume")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], "resume-target");
    assert_eq!(body["status"], "active");
}

#[tokio::test]
async fn post_vaults_resume_errored_path_inaccessible_returns_503_vault_errored() {
    let (h, _bogus) = errored_vault_harness("errd", "path missing").await;
    let app = router(h.state.clone());
    let req = Request::builder()
        .method("POST")
        .uri("/vaults/errd/resume")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"]["code"], "vault_errored");
    let msg = body["error"]["message"].as_str().unwrap();
    assert!(
        msg.contains("path missing"),
        "expected last_error in message: {msg}"
    );
}

#[tokio::test]
async fn post_vaults_reset_returns_200_with_updated_row() {
    let h = vault_harness().await;
    let path = fresh_dir(h._root.path(), "v");
    create_vault_via_api(&h.state, Some("reset-target"), &path).await;

    let app = router(h.state.clone());
    let req = json_request("POST", "/vaults/reset-target/reset", json!({})).await;
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], "reset-target");
    assert_eq!(body["status"], "active");
}

#[tokio::test]
async fn post_vaults_reset_with_rebuild_true_returns_200_and_clears_chunks() {
    let h = vault_harness().await;
    let path = fresh_dir(h._root.path(), "v");
    std::fs::write(path.join("a.md"), b"# Alpha\n\nBody one.\n").unwrap();
    std::fs::write(path.join("b.md"), b"# Bravo\n\nBody two.\n").unwrap();
    create_vault_via_api(&h.state, Some("rebuild-target"), &path).await;

    let chunks_before = count_chunks_via_manager(&h.state).await;
    assert!(
        chunks_before > 0,
        "initial scan should populate chunks; got {chunks_before}"
    );

    let app = router(h.state.clone());
    let req = json_request(
        "POST",
        "/vaults/rebuild-target/reset",
        json!({ "rebuild": true }),
    )
    .await;
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "active");

    let chunks_after = count_chunks_via_manager(&h.state).await;
    assert_eq!(
        chunks_after, 0,
        "rebuild should clear chunks; got {chunks_after}"
    );
}

#[tokio::test]
async fn post_vaults_rename_returns_200_with_new_name() {
    let h = vault_harness().await;
    let path = fresh_dir(h._root.path(), "v");
    create_vault_via_api(&h.state, Some("oldname"), &path).await;

    let app = router(h.state.clone());
    let req = json_request(
        "POST",
        "/vaults/oldname/rename",
        json!({ "new_name": "newname" }),
    )
    .await;
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], "newname");
}

#[tokio::test]
async fn post_vaults_rename_invalid_new_name_returns_422_vault_path_invalid() {
    let h = vault_harness().await;
    let path = fresh_dir(h._root.path(), "v");
    create_vault_via_api(&h.state, Some("renametarget"), &path).await;

    let app = router(h.state.clone());
    let req = json_request(
        "POST",
        "/vaults/renametarget/rename",
        json!({ "new_name": "has spaces" }),
    )
    .await;
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "vault_path_invalid");
}

#[tokio::test]
async fn post_vaults_rename_collision_returns_409_vault_name_conflict() {
    let h = vault_harness().await;
    let path_a = fresh_dir(h._root.path(), "a");
    let path_b = fresh_dir(h._root.path(), "b");
    create_vault_via_api(&h.state, Some("alpha"), &path_a).await;
    create_vault_via_api(&h.state, Some("bravo"), &path_b).await;

    let app = router(h.state.clone());
    let req = json_request(
        "POST",
        "/vaults/alpha/rename",
        json!({ "new_name": "bravo" }),
    )
    .await;
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "vault_name_conflict");
}

#[tokio::test]
async fn post_vaults_rescan_returns_200_with_rescan_initiated_at() {
    let h = vault_harness().await;
    let path = fresh_dir(h._root.path(), "v");
    create_vault_via_api(&h.state, Some("rescan-target"), &path).await;

    let app = router(h.state.clone());
    let req = Request::builder()
        .method("POST")
        .uri("/vaults/rescan-target/rescan")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], "rescan-target");
    assert_eq!(body["status"], "active");
    let initiated = body["rescan_initiated_at"]
        .as_str()
        .expect("rescan_initiated_at present");
    assert!(
        chrono::DateTime::parse_from_rfc3339(initiated).is_ok(),
        "rescan_initiated_at not RFC3339: {initiated}"
    );
}

#[tokio::test]
async fn post_vaults_unknown_op_path_returns_404() {
    let h = vault_harness().await;
    let path = fresh_dir(h._root.path(), "v");
    create_vault_via_api(&h.state, Some("sometarget"), &path).await;

    let app = router(h.state.clone());
    let req = Request::builder()
        .method("POST")
        .uri("/vaults/sometarget/bogus")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// ===== Step 16 · Task 16.4: HTTP watch endpoint tests =====
//
// Tests for GET /vaults/{name_or_id}/watch (single-vault NDJSON stream) and
// GET /events/watch (all-active-vaults NDJSON stream).

use crate::events::{EventType, StreamEvent};
use futures_util::StreamExt;

/// Minimal harness for watch endpoint tests: a VaultManager::for_tests with
/// one active vault, the event bus exposed for publishing, and the vault id.
struct WatchHarness {
    _dir: tempfile::TempDir,
    _vault: tempfile::TempDir,
    state: ApiState,
    vault_id: VaultId,
    vault_name: String,
}

async fn watch_harness() -> WatchHarness {
    let dir = tempfile::TempDir::new().unwrap();
    let vault = tempfile::TempDir::new().unwrap();
    let vault_id = VaultId::new();
    let store = crate::store::Store::open(
        &vault_id,
        dir.path(),
        "index.sqlite",
        &crate::config::EmbeddingConfig::default(),
    )
    .await
    .unwrap();
    let store = Arc::new(store);
    let entry = VaultEntry {
        id: vault_id.clone(),
        name: "watched".to_string(),
        vault_path: vault.path().to_path_buf(),
        store,
        status: VaultStatus::Active,
    };
    let manager = Arc::new(VaultManager::for_tests(
        vec![entry],
        Arc::new(StubEmbedder::new(768)),
        768,
    ));
    let state = ApiState {
        vault_manager: manager.clone(),
        event_bus: manager.event_bus(),
    };
    WatchHarness {
        _dir: dir,
        _vault: vault,
        state,
        vault_id,
        vault_name: "watched".to_string(),
    }
}

/// Read the first complete NDJSON line from a streaming response body, parse
/// it as a JSON `Value`, and return it. Panics if the stream ends without a
/// line or if the line is invalid JSON.
async fn first_ndjson_line(body: axum::body::Body) -> serde_json::Value {
    let mut stream = body.into_data_stream();
    let mut buf = String::new();
    loop {
        let chunk = stream
            .next()
            .await
            .expect("stream ended before a complete NDJSON line")
            .expect("stream error");
        buf.push_str(std::str::from_utf8(&chunk).expect("chunk is valid utf8"));
        if buf.contains('\n') {
            break;
        }
    }
    let line = buf.lines().next().expect("at least one line");
    serde_json::from_str(line).unwrap_or_else(|e| panic!("invalid JSON: {e}: {line}"))
}

#[tokio::test]
async fn watch_vault_unknown_returns_404() {
    let h = watch_harness().await;
    let app = router(h.state);
    let req = Request::builder()
        .method("GET")
        .uri("/vaults/nonexistent/watch")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let (_, body) = body_json(resp).await;
    assert_eq!(body["error"]["code"], "vault_not_found");
}

#[tokio::test]
async fn watch_vault_streams_file_changed_event() {
    let h = watch_harness().await;
    let app = router(h.state.clone());

    let req = Request::builder()
        .method("GET")
        .uri(format!("/vaults/{}/watch", h.vault_name))
        .body(Body::empty())
        .unwrap();

    // oneshot completes once the handler returns the streaming response.
    // At that point the broadcast receiver is subscribed. We then publish
    // the event and read the first NDJSON line.
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok()),
        Some("application/x-ndjson"),
    );

    // Publish one file_changed event targeting our vault.
    h.state.event_bus.publish(StreamEvent::file_changed(
        h.vault_id.clone(),
        EventType::Created,
        "notes/a.md".to_string(),
        None,
    ));

    // Read and parse the first NDJSON line.
    let event = first_ndjson_line(resp.into_body()).await;
    assert_eq!(event["type"], "file_changed");
    assert_eq!(event["event_type"], "created");
    assert_eq!(event["vault"], h.vault_id.to_string());
    assert_eq!(event["path"], "notes/a.md");
    assert!(event["detected_at"].is_string(), "detected_at present");
}

#[tokio::test]
async fn watch_vault_filters_out_events_for_other_vaults() {
    let h = watch_harness().await;
    let app = router(h.state.clone());

    let req = Request::builder()
        .method("GET")
        .uri(format!("/vaults/{}/watch", h.vault_name))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let other_vault = VaultId::new();
    let watched_vault = h.vault_id.clone();

    // First event: different vault → should be filtered.
    h.state.event_bus.publish(StreamEvent::file_changed(
        other_vault,
        EventType::Modified,
        "notes/b.md".to_string(),
        None,
    ));
    // Second event: our vault → should pass through.
    h.state.event_bus.publish(StreamEvent::file_changed(
        watched_vault.clone(),
        EventType::Deleted,
        "notes/a.md".to_string(),
        None,
    ));

    // The first NDJSON line we receive should be from our vault, not the other.
    let event = first_ndjson_line(resp.into_body()).await;
    assert_eq!(event["type"], "file_changed");
    assert_eq!(event["vault"], watched_vault.to_string());
    assert_eq!(event["path"], "notes/a.md");
}

#[tokio::test]
async fn watch_all_streams_events_from_all_active_vaults() {
    // Two-vault harness via for_tests_full.
    let root = tempfile::TempDir::new().unwrap();
    let mut pools: Vec<SqlitePool> = Vec::new();
    let mut ids: Vec<VaultId> = Vec::new();
    let mut entries: Vec<VaultEntry> = Vec::new();
    let mut _vault_dirs: Vec<tempfile::TempDir> = Vec::new();
    for name in &["alpha", "bravo"] {
        let vault_dir = tempfile::TempDir::new().unwrap();
        let vault_id = VaultId::new();
        let store = crate::store::Store::open(
            &vault_id,
            root.path(),
            "index.sqlite",
            &crate::config::EmbeddingConfig::default(),
        )
        .await
        .unwrap();
        let pool = store.pool();
        let store = Arc::new(store);
        entries.push(VaultEntry {
            id: vault_id.clone(),
            name: (*name).to_string(),
            vault_path: vault_dir.path().to_path_buf(),
            store,
            status: VaultStatus::Active,
        });
        pools.push(pool);
        ids.push(vault_id);
        _vault_dirs.push(vault_dir);
    }
    let manager = Arc::new(VaultManager::for_tests_full(
        entries,
        vec![],
        Arc::new(StubEmbedder::new(768)),
        768,
    ));
    let state = ApiState {
        vault_manager: manager.clone(),
        event_bus: manager.event_bus(),
    };

    let app = router(state.clone());
    let req = Request::builder()
        .method("GET")
        .uri("/events/watch")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok()),
        Some("application/x-ndjson"),
    );

    // Publish events from both vaults.
    state.event_bus.publish(StreamEvent::file_changed(
        ids[0].clone(),
        EventType::Created,
        "notes/from_alpha.md".to_string(),
        None,
    ));

    // First event should be from alpha.
    let event = first_ndjson_line(resp.into_body()).await;
    assert_eq!(event["type"], "file_changed");
    assert_eq!(event["vault"], ids[0].to_string());
}

#[tokio::test]
async fn watch_all_filters_out_inactive_vault_events() {
    // Active vault + one "extra" vault ID not in the pinned set.
    let root = tempfile::TempDir::new().unwrap();
    let vault_dir = tempfile::TempDir::new().unwrap();
    let vault_id = VaultId::new();
    let store = crate::store::Store::open(
        &vault_id,
        root.path(),
        "index.sqlite",
        &crate::config::EmbeddingConfig::default(),
    )
    .await
    .unwrap();
    let store = Arc::new(store);
    let entry = VaultEntry {
        id: vault_id.clone(),
        name: "active".to_string(),
        vault_path: vault_dir.path().to_path_buf(),
        store,
        status: VaultStatus::Active,
    };
    let manager = Arc::new(VaultManager::for_tests(
        vec![entry],
        Arc::new(StubEmbedder::new(768)),
        768,
    ));
    let state = ApiState {
        vault_manager: manager.clone(),
        event_bus: manager.event_bus(),
    };

    let app = router(state.clone());
    let req = Request::builder()
        .method("GET")
        .uri("/events/watch")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let ghost_vault = VaultId::new();

    // First publish to an ID not in the active set (should be filtered).
    state.event_bus.publish(StreamEvent::file_changed(
        ghost_vault,
        EventType::Modified,
        "ghost.md".to_string(),
        None,
    ));
    // Then publish to the active vault.
    state.event_bus.publish(StreamEvent::file_changed(
        vault_id.clone(),
        EventType::Created,
        "active.md".to_string(),
        None,
    ));

    let event = first_ndjson_line(resp.into_body()).await;
    assert_eq!(event["type"], "file_changed");
    assert_eq!(event["vault"], vault_id.to_string());
    assert_eq!(event["path"], "active.md");
}
