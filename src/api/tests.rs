use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use rusqlite::params;
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::task;
use tower::ServiceExt;

use std::sync::{Arc, Mutex};

use super::{ApiState, VaultEntry, router};
use crate::config::EmbeddingConfig;
use crate::control_plane::VaultManager;
use crate::embedding::{EmbedFuture, Embedder, EmbeddingError, StubEmbedder};
use crate::store::{SqlitePool, Store};
use crate::vault_registry::{VaultId, VaultStatus};

// 4 MB is plenty for these test bodies; we set a finite cap to satisfy
// `to_bytes` without inviting unbounded reads.
const BODY_LIMIT: usize = 4 * 1024 * 1024;

struct Harness {
    _dir: TempDir,
    _vault: TempDir,
    state: ApiState,
    pool: SqlitePool,
    vault_path: std::path::PathBuf,
    outbox_path: std::path::PathBuf,
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
    let outbox_path = dir.path().join("outbox.jsonl");
    let store = Arc::new(store);
    let pool = store.pool();
    let entry = VaultEntry {
        id: vault_id.clone(),
        name: "test".to_string(),
        vault_path: vault.path().to_path_buf(),
        outbox_path: outbox_path.clone(),
        store,
        status: VaultStatus::Active,
    };
    let manager = Arc::new(VaultManager::for_tests(vec![entry], embedder, 768));
    let state = ApiState {
        vault_manager: manager,
    };
    Harness {
        _dir: dir,
        vault_path: vault.path().to_path_buf(),
        _vault: vault,
        outbox_path,
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
    assert_eq!(body["outbox"]["path"], h.outbox_path.display().to_string());
    assert_eq!(body["outbox"]["size_bytes"], 0);
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
    std::fs::write(&h.outbox_path, b"line1\n").unwrap();
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
    assert_eq!(body["outbox"]["size_bytes"], 6);
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
async fn semantic_handler_default_limit_caps_at_default() {
    // Default limit (DEFAULT_LIMIT == 100) caps the result count even when
    // more chunks are present. Seed 105 identical-vector chunks, omit
    // `limit`, expect exactly 100 results.
    let h = harness_with_embedder(OneShotEmbedder::ok(unit_vec(&[(0, 1.0)]))).await;
    let pool = h.pool();
    task::spawn_blocking(move || {
        let mut conn = pool.get().unwrap();
        let tx = conn.transaction().unwrap();
        for i in 0u32..105 {
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
    assert_eq!(body["results"].as_array().unwrap().len(), 100);
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
