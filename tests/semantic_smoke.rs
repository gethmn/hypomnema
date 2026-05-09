//! Step 17 · Task 17.8 — Semantic smoke gate.
//!
//! Real in-process TCP daemon seeded with twelve 3 000-byte chunks, exercising
//! every mode the spec defines plus one HTTP/MCP-HTTP transport parity check.
//!
//! Modes:
//!   1. Default preview  — limit 10, text_kind "preview", text.len() ≤ 600
//!   2. Full text        — include_text "full", limit 3, full 3 000-byte content
//!   3. Metadata-only   — include_text "none", text fields absent, metadata present
//!   4. Bad include_text — 400 invalid_request
//!   5. preview_bytes=0  — 400 invalid_request
//!   6. Clamped preview  — preview_bytes 100 000 → text.len() ≤ 2 000, text_truncated=true
//!   +  Transport parity — HTTP and MCP-HTTP agree on the same query

use std::sync::Arc;
use std::time::Duration;

use hypomnema::api::mcp_http::{self, McpHttpState};
use hypomnema::api::{self, ApiState, VaultEntry};
use hypomnema::config::EmbeddingConfig;
use hypomnema::control_plane::VaultManager;
use hypomnema::embedding::{EmbedFuture, Embedder};
use hypomnema::mcp::{HypomnemaBackend, InProcessBackend};
use hypomnema::store::{SqlitePool, Store};
use hypomnema::vault_registry::{VaultId, VaultStatus};
use rusqlite::params;
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio::sync::watch;
use tokio::task::{self, JoinHandle};

const DIM: usize = 768;
const CHUNK_SIZE: usize = 3_000; // > 600 (Mode-1 truncation) and > 2 000 (Mode-6 clamp)
const CHUNK_COUNT: usize = 12;
const PROTOCOL_VERSION: &str = "2025-06-18";

// ===== Embedder that always returns the same unit vector =====

struct FixedEmbedder {
    vector: Vec<f32>,
}

impl FixedEmbedder {
    fn unit() -> Arc<Self> {
        let mut v = vec![0.0f32; DIM];
        v[0] = 1.0;
        Arc::new(Self { vector: v })
    }
}

impl Embedder for FixedEmbedder {
    fn embed_text<'a>(&'a self, _text: &'a str) -> EmbedFuture<'a> {
        let v = self.vector.clone();
        Box::pin(async move { Ok(v) })
    }
}

fn unit_vec() -> Vec<f32> {
    let mut v = vec![0.0f32; DIM];
    v[0] = 1.0;
    v
}

// ===== Daemon fixture =====

struct SmokeDaemon {
    base_url: String,
    shutdown: watch::Sender<bool>,
    handle: Option<JoinHandle<()>>,
    _root: TempDir,
}

impl SmokeDaemon {
    async fn shutdown(mut self) {
        let _ = self.shutdown.send(true);
        if let Some(h) = self.handle.take() {
            let _ = tokio::time::timeout(Duration::from_secs(5), h).await;
        }
    }
}

async fn spawn_smoke_daemon() -> SmokeDaemon {
    let root = TempDir::new().expect("tempdir");
    let vault_id = VaultId::new();
    let vault_dir = root.path().join("vault");
    std::fs::create_dir_all(&vault_dir).expect("create vault dir");

    let store = Store::open(
        &vault_id,
        root.path(),
        "index.sqlite",
        &EmbeddingConfig::default(),
    )
    .await
    .expect("open store");
    let pool = store.pool();

    let content = "x".repeat(CHUNK_SIZE);
    let file_rows: Vec<(String, String)> = (0..CHUNK_COUNT)
        .map(|i| (format!("file_{i:02}.md"), content.clone()))
        .collect();
    seed_files(pool.clone(), file_rows).await;

    let emb = unit_vec();
    for i in 0..CHUNK_COUNT {
        seed_chunk(
            pool.clone(),
            format!("file_{i:02}.md"),
            0,
            content.clone(),
            emb.clone(),
        )
        .await;
    }

    let embedder: Arc<dyn Embedder> = FixedEmbedder::unit();
    let vault_name = "smoke".to_string();
    let entry = VaultEntry {
        id: vault_id,
        name: vault_name.clone(),
        vault_path: vault_dir,
        store: Arc::new(store),
        status: VaultStatus::Active,
    };
    let manager = Arc::new(VaultManager::for_tests(vec![entry], embedder, DIM as u32));
    let api_state = ApiState {
        vault_manager: manager.clone(),
        event_bus: manager.event_bus(),
        started_at: std::time::Instant::now(),
        embedding_endpoint: None,

        semantic_config: hypomnema::config::SemanticSearchConfig::default(),
    };
    let backend: Arc<dyn HypomnemaBackend + Send + Sync> =
        Arc::new(InProcessBackend::new(manager.clone()));
    let app = api::router(api_state).merge(mcp_http::router(McpHttpState {
        backend,
        default_vault_name: vault_name,
        enable_write_tools: true,
    }));

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind 127.0.0.1:0");
    let addr = listener.local_addr().expect("local_addr");
    let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.wait_for(|v| *v).await;
            })
            .await;
    });

    SmokeDaemon {
        base_url: format!("http://{addr}"),
        shutdown: shutdown_tx,
        handle: Some(handle),
        _root: root,
    }
}

// ===== Seed helpers =====

async fn seed_files(pool: SqlitePool, rows: Vec<(String, String)>) {
    task::spawn_blocking(move || {
        let conn = pool.get().expect("get conn");
        for (path, content) in rows {
            conn.execute(
                "INSERT INTO files (path, size, mtime, content_hash, indexed_at, content) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    path,
                    content.len() as i64,
                    "2026-01-01T00:00:00Z",
                    "sha256:smoke00",
                    "2026-01-01T00:00:00Z",
                    content,
                ],
            )
            .expect("seed file insert");
        }
    })
    .await
    .expect("seed_files join");
}

async fn seed_chunk(
    pool: SqlitePool,
    file_path: String,
    chunk_index: u32,
    content: String,
    embedding: Vec<f32>,
) {
    task::spawn_blocking(move || {
        let mut conn = pool.get().expect("get conn");
        let tx = conn.transaction().expect("begin tx");
        tx.execute(
            "INSERT INTO chunks (file_path, chunk_index, heading_path, content, content_hash, \
             start_byte, end_byte, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                file_path,
                chunk_index,
                "",
                content,
                "sha256:smoke00",
                0i64,
                content.len() as i64,
                "2026-01-01T00:00:00Z"
            ],
        )
        .expect("seed chunk insert");
        let chunk_id = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO chunks_vec (chunk_id, embedding) VALUES (?1, ?2)",
            params![chunk_id, bytemuck::cast_slice::<f32, u8>(&embedding)],
        )
        .expect("seed chunk_vec insert");
        tx.commit().expect("commit chunk seed");
    })
    .await
    .expect("seed_chunk join");
}

// ===== HTTP helper =====

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("reqwest client")
}

async fn post_semantic(base_url: &str, body: Value) -> (u16, Value) {
    let resp = http_client()
        .post(format!("{base_url}/search/semantic"))
        .json(&body)
        .send()
        .await
        .expect("send POST /search/semantic");
    let status = resp.status().as_u16();
    let json: Value = resp.json().await.expect("parse response json");
    (status, json)
}

// ===== Minimal MCP-HTTP client (for transport-parity test only) =====

struct McpClient {
    client: reqwest::Client,
    base_url: String,
    session_id: Option<String>,
    next_id: u64,
}

impl McpClient {
    fn new(base_url: &str) -> Self {
        Self {
            client: http_client(),
            base_url: base_url.to_string(),
            session_id: None,
            next_id: 0,
        }
    }

    async fn initialize(&mut self) {
        let id = self.next_id();
        let body = json!({
            "jsonrpc": "2.0", "id": id, "method": "initialize",
            "params": {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": "smoke-parity", "version": "0.0.0" },
            }
        });
        let resp = self.post(&body).await;
        let session_id = resp
            .headers()
            .get("mcp-session-id")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
            .expect("initialize missing Mcp-Session-Id header");
        self.session_id = Some(session_id);
        let bytes = resp.bytes().await.expect("read initialize body");
        parse_sse(&bytes); // discard but validate parseable

        let notif = json!({
            "jsonrpc": "2.0", "method": "notifications/initialized", "params": {}
        });
        let _ = self.post(&notif).await;
    }

    async fn tools_call(&mut self, name: &str, arguments: Value) -> Value {
        let id = self.next_id();
        let body = json!({
            "jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": { "name": name, "arguments": arguments }
        });
        let resp = self.post(&body).await;
        assert!(
            resp.status().is_success(),
            "tools/call returned {}",
            resp.status()
        );
        let bytes = resp.bytes().await.expect("read tools/call body");
        parse_sse(&bytes)
    }

    async fn post(&self, body: &Value) -> reqwest::Response {
        let mut req = self
            .client
            .post(format!("{}/mcp", self.base_url))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .json(body);
        if let Some(sid) = &self.session_id {
            req = req.header("Mcp-Session-Id", sid);
        }
        req.send().await.expect("MCP HTTP send")
    }

    fn next_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }
}

fn parse_sse(bytes: &[u8]) -> Value {
    let text = std::str::from_utf8(bytes).expect("SSE utf-8");
    let mut parts: Vec<String> = Vec::new();
    let mut cur: Vec<&str> = Vec::new();
    for line in text.split('\n') {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            if !cur.is_empty() {
                parts.push(cur.join("\n"));
                cur.clear();
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("data:") {
            cur.push(rest.strip_prefix(' ').unwrap_or(rest));
        }
    }
    if !cur.is_empty() {
        parts.push(cur.join("\n"));
    }
    let data = parts
        .into_iter()
        .find(|d| !d.is_empty())
        .unwrap_or_else(|| panic!("SSE had no non-priming data: {text:?}"));
    serde_json::from_str(&data)
        .unwrap_or_else(|e| panic!("invalid JSON in SSE data: {e}; raw: {data:?}"))
}

// ===== Mode 1: default preview =====
// Exactly 10 results, text_kind="preview", text_truncated=true for 3 000-byte chunks,
// text.len() ≤ 600, content_hash populated, response truncated=true (12 > 10).

#[tokio::test]
async fn smoke_mode1_default_preview() {
    let daemon = spawn_smoke_daemon().await;
    let (status, body) = post_semantic(
        &daemon.base_url,
        json!({ "query": "test", "granularity": "chunk" }),
    )
    .await;
    assert_eq!(status, 200, "body: {body}");

    let results = body["results"].as_array().expect("results array");
    assert_eq!(
        results.len(),
        10,
        "default limit must return 10 results, got {}",
        results.len()
    );
    for result in results {
        assert_eq!(result["text_kind"], "preview", "text_kind must be preview");
        assert_eq!(
            result["text_truncated"], true,
            "3 000-byte chunk must be truncated at 600"
        );
        let text = result["text"].as_str().expect("text field present");
        assert!(
            text.len() <= 600,
            "preview text must be ≤ 600 bytes, got {}",
            text.len()
        );
        assert!(
            result["content_hash"].as_str().is_some(),
            "content_hash must be populated"
        );
    }
    assert_eq!(
        body["truncated"], true,
        "12 chunks > limit 10 → response truncated must be true"
    );

    daemon.shutdown().await;
}

// ===== Mode 2: full text, limited results =====
// 3 results, text_kind="full", full 3 000-byte content, text_truncated=false.

#[tokio::test]
async fn smoke_mode2_full_text_limited() {
    let daemon = spawn_smoke_daemon().await;
    let (status, body) = post_semantic(
        &daemon.base_url,
        json!({ "query": "test", "include_text": "full", "limit": 3, "granularity": "chunk" }),
    )
    .await;
    assert_eq!(status, 200, "body: {body}");

    let results = body["results"].as_array().expect("results array");
    assert_eq!(
        results.len(),
        3,
        "explicit limit=3 must return 3 results, got {}",
        results.len()
    );
    for result in results {
        assert_eq!(result["text_kind"], "full");
        assert_eq!(result["text_truncated"], false);
        let text = result["text"].as_str().expect("text present");
        assert_eq!(
            text.len(),
            CHUNK_SIZE,
            "full text must be the complete {CHUNK_SIZE}-byte chunk"
        );
    }

    daemon.shutdown().await;
}

// ===== Mode 3: metadata-only =====
// text, text_kind, text_truncated absent; score, file_path, content_hash, chunk_index present.

#[tokio::test]
async fn smoke_mode3_metadata_only() {
    let daemon = spawn_smoke_daemon().await;
    let (status, body) = post_semantic(
        &daemon.base_url,
        json!({ "query": "test", "include_text": "none", "limit": 20, "granularity": "chunk" }),
    )
    .await;
    assert_eq!(status, 200, "body: {body}");

    let results = body["results"].as_array().expect("results array");
    assert!(
        !results.is_empty(),
        "metadata-only search must return results"
    );
    for result in results {
        assert!(
            result.get("text").is_none(),
            "text must be absent for include_text=none"
        );
        assert!(
            result.get("text_kind").is_none(),
            "text_kind must be absent for include_text=none"
        );
        assert!(
            result.get("text_truncated").is_none(),
            "text_truncated must be absent for include_text=none"
        );
        assert!(result["score"].as_f64().is_some(), "score must be present");
        assert!(
            result["file_path"].as_str().is_some(),
            "file_path must be present"
        );
        assert!(
            result["content_hash"].as_str().is_some(),
            "content_hash must be present"
        );
        assert!(
            result["chunk_index"].as_u64().is_some(),
            "chunk_index must be present"
        );
    }

    daemon.shutdown().await;
}

// ===== Mode 4: invalid include_text =====
// 400, code="invalid_request", message mentions "include_text".

#[tokio::test]
async fn smoke_mode4_invalid_include_text() {
    let daemon = spawn_smoke_daemon().await;
    let (status, body) = post_semantic(
        &daemon.base_url,
        json!({ "query": "test", "include_text": "bogus" }),
    )
    .await;
    assert_eq!(status, 400, "body: {body}");
    assert_eq!(body["error"]["code"], "invalid_request");
    let msg = body["error"]["message"]
        .as_str()
        .expect("error message present");
    assert!(
        msg.contains("include_text"),
        "error message must mention include_text, got: {msg:?}"
    );

    daemon.shutdown().await;
}

// ===== Mode 5: preview_bytes=0 =====
// 400, code="invalid_request".

#[tokio::test]
async fn smoke_mode5_preview_bytes_zero() {
    let daemon = spawn_smoke_daemon().await;
    let (status, body) = post_semantic(
        &daemon.base_url,
        json!({ "query": "test", "preview_bytes": 0 }),
    )
    .await;
    assert_eq!(status, 400, "body: {body}");
    assert_eq!(body["error"]["code"], "invalid_request");

    daemon.shutdown().await;
}

// ===== Mode 6: clamped preview_bytes =====
// preview_bytes=100000 clamped to 2000; 3 000-byte chunks → text.len() ≤ 2000, text_truncated=true.

#[tokio::test]
async fn smoke_mode6_clamped_preview_bytes() {
    let daemon = spawn_smoke_daemon().await;
    let (status, body) = post_semantic(
        &daemon.base_url,
        json!({ "query": "test", "preview_bytes": 100_000, "granularity": "chunk" }),
    )
    .await;
    assert_eq!(status, 200, "body: {body}");

    let results = body["results"].as_array().expect("results array");
    assert!(!results.is_empty(), "clamped preview must return results");
    for result in results {
        let text = result["text"].as_str().expect("text present");
        assert!(
            text.len() <= 2_000,
            "clamped preview must be ≤ 2 000 bytes, got {}",
            text.len()
        );
        assert_eq!(
            result["text_truncated"], true,
            "3 000-byte chunk must still be truncated after clamping to 2 000"
        );
    }

    daemon.shutdown().await;
}

// ===== Mode 7: document mode diversity =====
// Default granularity="document" (no explicit field). 12 unique files, 1 chunk
// each → 10 document results, each from a distinct file, truncated=true.
// This is the document-diversity regression smoke: one file with many chunks
// cannot crowd out distinct documents under the default config.

#[tokio::test]
async fn smoke_mode7_document_mode_diversity() {
    let daemon = spawn_smoke_daemon().await;
    // No granularity field → daemon default ("document").
    let (status, body) = post_semantic(&daemon.base_url, json!({ "query": "test" })).await;
    assert_eq!(status, 200, "body: {body}");

    let results = body["results"].as_array().expect("results array");
    assert_eq!(
        results.len(),
        10,
        "default limit must return 10 document results, got {}",
        results.len()
    );
    // Every result must be a document (has nested chunks, no top-level chunk_index).
    for result in results {
        assert!(
            result.get("chunks").is_some(),
            "document mode results must have nested chunks"
        );
        assert!(
            result.get("chunk_index").is_none(),
            "document mode results must not have top-level chunk_index"
        );
    }
    // All 10 results must be from distinct files.
    let paths: std::collections::HashSet<&str> = results
        .iter()
        .map(|r| r["file_path"].as_str().expect("file_path"))
        .collect();
    assert_eq!(
        paths.len(),
        10,
        "all 10 results must be from distinct files"
    );
    assert_eq!(
        body["truncated"], true,
        "12 files > limit 10 → truncated must be true"
    );

    daemon.shutdown().await;
}

// ===== Mode 8: document mode with include_text=full =====
// Explicit granularity="document" + include_text="full": nested chunks must
// carry the full 3 000-byte chunk text.

#[tokio::test]
async fn smoke_mode8_document_mode_full_text_nested() {
    let daemon = spawn_smoke_daemon().await;
    let (status, body) = post_semantic(
        &daemon.base_url,
        json!({ "query": "test", "granularity": "document", "include_text": "full", "limit": 3 }),
    )
    .await;
    assert_eq!(status, 200, "body: {body}");

    let results = body["results"].as_array().expect("results array");
    assert_eq!(results.len(), 3, "limit=3 must return 3 documents");
    for result in results {
        let chunks = result["chunks"].as_array().expect("nested chunks");
        assert!(
            !chunks.is_empty(),
            "each document must have at least one nested chunk"
        );
        for chunk in chunks {
            assert_eq!(chunk["text_kind"], "full");
            assert_eq!(chunk["text_truncated"], false);
            let text = chunk["text"].as_str().expect("nested chunk text");
            assert_eq!(
                text.len(),
                CHUNK_SIZE,
                "full text must be the complete {CHUNK_SIZE}-byte chunk"
            );
        }
    }

    daemon.shutdown().await;
}

// ===== Transport parity: HTTP and MCP-HTTP agree =====
// One semantic query over both transports; structuredContent must equal HTTP response.

#[tokio::test]
async fn smoke_transport_parity_http_and_mcp() {
    let daemon = spawn_smoke_daemon().await;

    let query = json!({ "query": "test", "limit": 3 });

    let http_resp: Value = http_client()
        .post(format!("{}/search/semantic", daemon.base_url))
        .json(&query)
        .send()
        .await
        .expect("POST /search/semantic")
        .error_for_status()
        .expect("/search/semantic 2xx")
        .json()
        .await
        .expect("decode HTTP JSON");

    let mut mcp = McpClient::new(&daemon.base_url);
    mcp.initialize().await;
    let mcp_resp = mcp.tools_call("search_semantic", query).await;
    assert_eq!(
        &mcp_resp["result"]["structuredContent"], &http_resp,
        "MCP-HTTP structuredContent must equal /search/semantic response"
    );

    daemon.shutdown().await;
}
