use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use rusqlite::params;
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::task;
use tower::ServiceExt;

use super::{ApiState, router};
use crate::config::EmbeddingConfig;
use crate::store::Store;

// 4 MB is plenty for these test bodies; we set a finite cap to satisfy
// `to_bytes` without inviting unbounded reads.
const BODY_LIMIT: usize = 4 * 1024 * 1024;

struct Harness {
    _dir: TempDir,
    _vault: TempDir,
    state: ApiState,
}

async fn harness() -> Harness {
    let dir = TempDir::new().unwrap();
    let vault = TempDir::new().unwrap();
    let store = Store::open(dir.path(), "index.sqlite", &EmbeddingConfig::default())
        .await
        .unwrap();
    let state = ApiState {
        pool: store.pool(),
        vault: vault.path().to_path_buf(),
        outbox_path: dir.path().join("outbox.jsonl"),
    };
    Harness {
        _dir: dir,
        _vault: vault,
        state,
    }
}

async fn seed_files(state: &ApiState, rows: Vec<(&'static str, &'static str, &'static str)>) {
    let pool = state.pool.clone();
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
    assert_eq!(body["vault"], h.state.vault.display().to_string());
    assert_eq!(
        body["outbox"]["path"],
        h.state.outbox_path.display().to_string()
    );
    assert_eq!(body["outbox"]["size_bytes"], 0);
}

#[tokio::test]
async fn status_reports_count_and_last_indexed_after_seeding() {
    let h = harness().await;
    seed_files(
        &h.state,
        vec![
            ("a.md", "alpha", "2026-04-01T00:00:00Z"),
            ("b.md", "bravo", "2026-04-22T14:31:08.123456Z"),
        ],
    )
    .await;
    std::fs::write(&h.state.outbox_path, b"line1\n").unwrap();
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
        &h.state,
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
        &h.state,
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
async fn search_response_omits_vault_field_in_v0() {
    let h = harness().await;
    seed_files(
        &h.state,
        vec![("a.md", "alpha pgvector", "2026-04-01T00:00:00Z")],
    )
    .await;

    // Filesystem result entries omit vault.
    let app = router(h.state.clone());
    let req = json_request("POST", "/search/filesystem", json!({})).await;
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    let entry = &body["results"][0];
    assert!(
        entry.get("vault").is_none(),
        "filesystem entry should omit `vault`; got {entry}"
    );

    // Content result entries omit vault.
    let app = router(h.state.clone());
    let req = json_request("POST", "/search/content", json!({ "query": "pgvector" })).await;
    let resp = app.oneshot(req).await.unwrap();
    let (status, body) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    let entry = &body["results"][0];
    assert!(
        entry.get("vault").is_none(),
        "content entry should omit `vault`; got {entry}"
    );
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
