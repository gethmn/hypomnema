//! Integration tests for the `content_get` operation (Step 19).
//!
//! Tests 8.2–8.7 from the workplan § Tier 8. Uses `VaultManager::for_tests`
//! and `for_tests_full` with pre-seeded SQLite pools to keep tests
//! deterministic without spinning up real watchers.
//!
//! Test 8.7 (transport parity) drives the same request via HTTP and via the
//! in-process MCP backend and verifies the response shapes match.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use hypomnema::api::{self, ApiState, VaultEntry};
use hypomnema::config::EmbeddingConfig;
use hypomnema::control_plane::VaultManager;
use hypomnema::embedding::{Embedder, StubEmbedder};
use hypomnema::mcp::{HypomnemaBackend, InProcessBackend};
use hypomnema::store::{SqlitePool, Store};
use hypomnema::vault_registry::{VaultId, VaultRow, VaultStatus};
use rusqlite::params;
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio::sync::watch;
use tokio::task::{self, JoinHandle};

const DIM: u32 = 768;

// ===== Helpers =====

async fn open_store(vault_id: &VaultId, root: &TempDir) -> Store {
    Store::open(
        vault_id,
        root.path(),
        "index.sqlite",
        &EmbeddingConfig::default(),
    )
    .await
    .expect("open store")
}

async fn seed_files(pool: SqlitePool, rows: Vec<(&'static str, &'static str)>) {
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
                    "sha256:00",
                    "2026-01-01T00:00:00Z",
                    content,
                ],
            )
            .expect("seed insert");
        }
    })
    .await
    .expect("seed_files join");
}

struct TestDaemon {
    base_url: String,
    _root: TempDir,
    _vault_dirs: Vec<TempDir>,
    shutdown: watch::Sender<bool>,
    handle: Option<JoinHandle<()>>,
}

impl TestDaemon {
    async fn shutdown(mut self) {
        let _ = self.shutdown.send(true);
        if let Some(h) = self.handle.take() {
            let _ = h.await;
        }
    }
}

async fn spawn_daemon_with_entries(
    entries: Vec<VaultEntry>,
    root: TempDir,
    vault_dirs: Vec<TempDir>,
) -> TestDaemon {
    let embedder: Arc<dyn Embedder> = Arc::new(StubEmbedder::new(DIM as usize));
    let manager = Arc::new(VaultManager::for_tests(entries, embedder, DIM));
    let state = ApiState {
        vault_manager: manager.clone(),
        event_bus: manager.event_bus(),
    };
    let app = api::router(state);
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
    TestDaemon {
        base_url: format!("http://{addr}"),
        _root: root,
        _vault_dirs: vault_dirs,
        shutdown: shutdown_tx,
        handle: Some(handle),
    }
}

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("reqwest client")
}

// ===== Test 8.2 — Single-file retrieval =====

#[tokio::test]
async fn content_get_single_file_retrieval() {
    let root = TempDir::new().expect("tempdir");
    let vault_dir = TempDir::new().expect("vault tempdir");
    let vault_id = VaultId::new();
    let store = open_store(&vault_id, &root).await;
    let pool = store.pool();

    seed_files(
        pool.clone(),
        vec![("notes/a.md", "# File A\nSome content.\n")],
    )
    .await;

    let entry = VaultEntry {
        id: vault_id.clone(),
        name: "test".to_string(),
        vault_path: vault_dir.path().to_path_buf(),
        store: Arc::new(store),
        status: VaultStatus::Active,
    };

    let daemon = spawn_daemon_with_entries(vec![entry], root, vec![vault_dir]).await;

    let resp: Value = http_client()
        .post(format!("{}/content/get", daemon.base_url))
        .json(&json!({ "paths": ["notes/a.md"] }))
        .send()
        .await
        .expect("POST /content/get")
        .error_for_status()
        .expect("200")
        .json()
        .await
        .expect("JSON");

    let results = resp["results"].as_array().expect("results array");
    assert_eq!(results.len(), 1, "expected 1 result");

    let item = &results[0];
    assert_eq!(item["path"].as_str(), Some("notes/a.md"));
    assert_eq!(item["content"].as_str(), Some("# File A\nSome content.\n"));
    assert!(
        !item["content_hash"].as_str().unwrap_or("").is_empty(),
        "content_hash should be non-empty"
    );
    assert!(item["size"].as_i64().unwrap_or(0) > 0, "size should be > 0");
    assert!(
        !item["mtime"].as_str().unwrap_or("").is_empty(),
        "mtime should be non-empty"
    );
    assert!(
        !item["vault"].as_str().unwrap_or("").is_empty(),
        "vault should be non-empty"
    );
    assert_eq!(item["vault_name"].as_str(), Some("test"));
    assert!(item["error"].is_null(), "no error expected");

    daemon.shutdown().await;
}

// ===== Test 8.3 — Multi-file batch with hits and misses =====

#[tokio::test]
async fn content_get_multi_file_with_missing() {
    let root = TempDir::new().expect("tempdir");
    let vault_dir = TempDir::new().expect("vault tempdir");
    let vault_id = VaultId::new();
    let store = open_store(&vault_id, &root).await;
    let pool = store.pool();

    seed_files(
        pool.clone(),
        vec![
            ("notes/a.md", "content of a"),
            ("notes/b.md", "content of b"),
        ],
    )
    .await;

    let entry = VaultEntry {
        id: vault_id.clone(),
        name: "test".to_string(),
        vault_path: vault_dir.path().to_path_buf(),
        store: Arc::new(store),
        status: VaultStatus::Active,
    };

    let daemon = spawn_daemon_with_entries(vec![entry], root, vec![vault_dir]).await;

    let resp: Value = http_client()
        .post(format!("{}/content/get", daemon.base_url))
        .json(&json!({ "paths": ["notes/a.md", "notes/missing.md", "notes/b.md"] }))
        .send()
        .await
        .expect("POST /content/get")
        .error_for_status()
        .expect("200")
        .json()
        .await
        .expect("JSON");

    let results = resp["results"].as_array().expect("results array");
    assert_eq!(results.len(), 3, "expected 3 result items");

    // Results are ordered by (path ASC, vault_id ASC) — alphabetical
    let paths: Vec<&str> = results
        .iter()
        .map(|r| r["path"].as_str().expect("path"))
        .collect();
    assert_eq!(paths, vec!["notes/a.md", "notes/b.md", "notes/missing.md"]);

    // a.md and b.md are successes
    assert_eq!(
        results[0]["content"].as_str(),
        Some("content of a"),
        "a.md content"
    );
    assert_eq!(
        results[1]["content"].as_str(),
        Some("content of b"),
        "b.md content"
    );

    // missing.md is an error with path_not_found
    let missing = &results[2];
    assert_eq!(missing["path"].as_str(), Some("notes/missing.md"));
    assert_eq!(
        missing["error"]["code"].as_str(),
        Some("path_not_found"),
        "missing file should have path_not_found error code"
    );
    assert!(missing["content"].is_null(), "error item has no content");

    daemon.shutdown().await;
}

// ===== Test 8.4 — Multi-vault fan-out =====

#[tokio::test]
async fn content_get_multi_vault_fanout() {
    let root = TempDir::new().expect("tempdir");
    let vault_dir_a = TempDir::new().expect("vault tempdir a");
    let vault_dir_b = TempDir::new().expect("vault tempdir b");

    // Two separate stores: open them using unique sub-dirs inside root
    let vault_id_a = VaultId::new();
    let vault_id_b = VaultId::new();

    // Each vault needs its own storage subdir so they don't share a pool
    let store_dir_a = {
        let d = root.path().join("vault-a");
        std::fs::create_dir_all(&d).expect("create vault-a dir");
        d
    };
    let store_dir_b = {
        let d = root.path().join("vault-b");
        std::fs::create_dir_all(&d).expect("create vault-b dir");
        d
    };

    let store_a = Store::open(
        &vault_id_a,
        &store_dir_a,
        "index.sqlite",
        &EmbeddingConfig::default(),
    )
    .await
    .expect("open store a");

    let store_b = Store::open(
        &vault_id_b,
        &store_dir_b,
        "index.sqlite",
        &EmbeddingConfig::default(),
    )
    .await
    .expect("open store b");

    seed_files(
        store_a.pool(),
        vec![("shared/file.md", "content from vault a")],
    )
    .await;
    seed_files(
        store_b.pool(),
        vec![("shared/file.md", "content from vault b")],
    )
    .await;

    let entry_a = VaultEntry {
        id: vault_id_a.clone(),
        name: "vault-a".to_string(),
        vault_path: vault_dir_a.path().to_path_buf(),
        store: Arc::new(store_a),
        status: VaultStatus::Active,
    };
    let entry_b = VaultEntry {
        id: vault_id_b.clone(),
        name: "vault-b".to_string(),
        vault_path: vault_dir_b.path().to_path_buf(),
        store: Arc::new(store_b),
        status: VaultStatus::Active,
    };

    let embedder: Arc<dyn Embedder> = Arc::new(StubEmbedder::new(DIM as usize));
    let manager = Arc::new(VaultManager::for_tests(
        vec![entry_a, entry_b],
        embedder,
        DIM,
    ));
    let state = ApiState {
        vault_manager: manager.clone(),
        event_bus: manager.event_bus(),
    };
    let app = api::router(state);
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
    let base_url = format!("http://{addr}");

    let resp: Value = http_client()
        .post(format!("{base_url}/content/get"))
        .json(&json!({ "paths": ["shared/file.md"] }))
        .send()
        .await
        .expect("POST /content/get fanout")
        .error_for_status()
        .expect("200")
        .json()
        .await
        .expect("JSON");

    let results = resp["results"].as_array().expect("results array");
    assert_eq!(results.len(), 2, "expected 2 results (one per vault)");

    // Both items are successes
    for item in results {
        assert!(
            item["content"].as_str().is_some(),
            "each item should be a success with content"
        );
        assert!(item["error"].is_null(), "no error expected");
        assert_eq!(item["path"].as_str(), Some("shared/file.md"));
    }

    // Ordered by (path ASC, vault_id ASC) — vault_id order is alphabetic UUID
    let vault_ids: Vec<&str> = results
        .iter()
        .map(|r| r["vault"].as_str().expect("vault id"))
        .collect();
    let mut sorted = vault_ids.clone();
    sorted.sort();
    assert_eq!(vault_ids, sorted, "results must be ordered by vault_id ASC");

    let _ = shutdown_tx.send(true);
    let _ = handle.await;
}

// ===== Test 8.5 — Explicit vault scoping =====

#[tokio::test]
async fn content_get_explicit_vault_scoping() {
    let root = TempDir::new().expect("tempdir");
    let vault_dir_1 = TempDir::new().expect("vault tempdir 1");
    let vault_dir_2 = TempDir::new().expect("vault tempdir 2");

    let vault_id_1 = VaultId::new();
    let vault_id_2 = VaultId::new();

    let store_dir_1 = {
        let d = root.path().join("v1");
        std::fs::create_dir_all(&d).expect("create v1 dir");
        d
    };
    let store_dir_2 = {
        let d = root.path().join("v2");
        std::fs::create_dir_all(&d).expect("create v2 dir");
        d
    };

    let store_1 = Store::open(
        &vault_id_1,
        &store_dir_1,
        "index.sqlite",
        &EmbeddingConfig::default(),
    )
    .await
    .expect("open store 1");

    let store_2 = Store::open(
        &vault_id_2,
        &store_dir_2,
        "index.sqlite",
        &EmbeddingConfig::default(),
    )
    .await
    .expect("open store 2");

    seed_files(store_1.pool(), vec![("file.md", "vault1 content")]).await;
    seed_files(store_2.pool(), vec![("file.md", "vault2 content")]).await;

    let entry_1 = VaultEntry {
        id: vault_id_1.clone(),
        name: "vault1".to_string(),
        vault_path: vault_dir_1.path().to_path_buf(),
        store: Arc::new(store_1),
        status: VaultStatus::Active,
    };
    let entry_2 = VaultEntry {
        id: vault_id_2.clone(),
        name: "vault2".to_string(),
        vault_path: vault_dir_2.path().to_path_buf(),
        store: Arc::new(store_2),
        status: VaultStatus::Active,
    };

    let embedder: Arc<dyn Embedder> = Arc::new(StubEmbedder::new(DIM as usize));
    let manager = Arc::new(VaultManager::for_tests(
        vec![entry_1, entry_2],
        embedder,
        DIM,
    ));
    let state = ApiState {
        vault_manager: manager.clone(),
        event_bus: manager.event_bus(),
    };
    let app = api::router(state);
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
    let base_url = format!("http://{addr}");

    // Only request from vault1 by name
    let resp: Value = http_client()
        .post(format!("{base_url}/content/get"))
        .json(&json!({ "paths": ["file.md"], "vaults": ["vault1"] }))
        .send()
        .await
        .expect("POST /content/get scoped")
        .error_for_status()
        .expect("200")
        .json()
        .await
        .expect("JSON");

    let results = resp["results"].as_array().expect("results array");
    assert_eq!(results.len(), 1, "expected only vault1 item");
    assert_eq!(results[0]["vault_name"].as_str(), Some("vault1"));
    assert_eq!(results[0]["content"].as_str(), Some("vault1 content"));

    let _ = shutdown_tx.send(true);
    let _ = handle.await;
}

// ===== Test 8.6 — Paused vault behavior =====

#[tokio::test]
async fn content_get_paused_vault_behavior() {
    let root = TempDir::new().expect("tempdir");
    let vault_dir_active = TempDir::new().expect("vault tempdir active");

    let vault_id_active = VaultId::new();
    let vault_id_paused = VaultId::new();

    let store_dir_active = {
        let d = root.path().join("active");
        std::fs::create_dir_all(&d).expect("create active dir");
        d
    };

    let store_active = Store::open(
        &vault_id_active,
        &store_dir_active,
        "index.sqlite",
        &EmbeddingConfig::default(),
    )
    .await
    .expect("open store active");

    seed_files(
        store_active.pool(),
        vec![("file.md", "active vault content")],
    )
    .await;

    let active_entry = VaultEntry {
        id: vault_id_active.clone(),
        name: "vault1".to_string(),
        vault_path: vault_dir_active.path().to_path_buf(),
        store: Arc::new(store_active),
        status: VaultStatus::Active,
    };

    // Paused vault row — no runner, no store
    let paused_row = VaultRow {
        id: vault_id_paused.clone(),
        name: "vault2".to_string(),
        path: PathBuf::from("/dev/null"),
        status: VaultStatus::Paused,
        created_at: Utc::now(),
        last_error: None,
    };

    let embedder: Arc<dyn Embedder> = Arc::new(StubEmbedder::new(DIM as usize));
    let manager = Arc::new(VaultManager::for_tests_full(
        vec![active_entry],
        vec![paused_row],
        embedder,
        DIM,
    ));
    let state = ApiState {
        vault_manager: manager.clone(),
        event_bus: manager.event_bus(),
    };
    let app = api::router(state);
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
    let base_url = format!("http://{addr}");

    // Request 1: default scope — active vault returns result, paused is skipped
    let resp1: Value = http_client()
        .post(format!("{base_url}/content/get"))
        .json(&json!({ "paths": ["file.md"] }))
        .send()
        .await
        .expect("POST /content/get request 1")
        .error_for_status()
        .expect("200")
        .json()
        .await
        .expect("JSON");

    let results1 = resp1["results"].as_array().expect("results array");
    assert_eq!(
        results1.len(),
        1,
        "default scope: only active vault item returned"
    );
    assert_eq!(results1[0]["vault_name"].as_str(), Some("vault1"));

    // vault2 should be in partial_results.skipped
    let skipped1 = resp1["partial_results"]["skipped"]
        .as_array()
        .expect("skipped array");
    assert_eq!(skipped1.len(), 1, "vault2 must appear in skipped");
    assert_eq!(skipped1[0]["vault_name"].as_str(), Some("vault2"));
    assert_eq!(skipped1[0]["status"].as_str(), Some("paused"));

    // Request 2: explicitly request vault2 (paused) — entry returned from index
    // (this vault has no store, so will end up in failed — this tests that the
    // paused vault with no live runner correctly surfaces the vault in
    // partial_results.skipped, not as an item result, because there's no entry)
    let resp2: Value = http_client()
        .post(format!("{base_url}/content/get"))
        .json(&json!({ "paths": ["file.md"], "vaults": ["vault2"] }))
        .send()
        .await
        .expect("POST /content/get request 2")
        .error_for_status()
        .expect("200")
        .json()
        .await
        .expect("JSON");

    // vault2 is Paused: no entry/runner, so skipped
    let skipped2 = resp2["partial_results"]["skipped"]
        .as_array()
        .expect("skipped array for request 2");
    assert_eq!(
        skipped2.len(),
        1,
        "vault2 in skipped even when explicitly requested"
    );
    assert_eq!(skipped2[0]["vault_name"].as_str(), Some("vault2"));
    assert_eq!(skipped2[0]["status"].as_str(), Some("paused"));

    let _ = shutdown_tx.send(true);
    let _ = handle.await;
}

// ===== Test 8.7 — Transport parity: HTTP vs in-process MCP backend =====

#[tokio::test]
async fn content_get_transport_parity_http_vs_mcp_backend() {
    let root = TempDir::new().expect("tempdir");
    let vault_dir = TempDir::new().expect("vault tempdir");
    let vault_id = VaultId::new();
    let store = open_store(&vault_id, &root).await;

    seed_files(
        store.pool(),
        vec![
            ("notes/a.md", "# Note A\nBody of A."),
            ("notes/b.md", "# Note B\nBody of B."),
        ],
    )
    .await;

    let store = Arc::new(store);
    let entry = VaultEntry {
        id: vault_id.clone(),
        name: "test".to_string(),
        vault_path: vault_dir.path().to_path_buf(),
        store: store.clone(),
        status: VaultStatus::Active,
    };

    let embedder: Arc<dyn Embedder> = Arc::new(StubEmbedder::new(DIM as usize));
    let manager = Arc::new(VaultManager::for_tests(vec![entry], embedder, DIM));

    // HTTP daemon
    let state = ApiState {
        vault_manager: manager.clone(),
        event_bus: manager.event_bus(),
    };
    let app = api::router(state);
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
    let base_url = format!("http://{addr}");

    // HTTP response
    let http_resp: Value = http_client()
        .post(format!("{base_url}/content/get"))
        .json(&json!({ "paths": ["notes/a.md", "notes/b.md"] }))
        .send()
        .await
        .expect("POST /content/get HTTP")
        .error_for_status()
        .expect("200")
        .json()
        .await
        .expect("JSON");

    // In-process MCP backend response
    let backend = InProcessBackend::new(manager);
    let req = hypomnema::api::types::ContentGetRequest {
        paths: vec!["notes/a.md".to_string(), "notes/b.md".to_string()],
        vaults: None,
    };
    let mcp_resp = backend
        .content_get(&req)
        .await
        .expect("in-process content_get");
    let mcp_resp_value = serde_json::to_value(&mcp_resp).expect("MCP response to JSON");

    // Both responses must have the same number of results
    let http_results = http_resp["results"].as_array().expect("HTTP results");
    let mcp_results = mcp_resp_value["results"].as_array().expect("MCP results");
    assert_eq!(
        http_results.len(),
        mcp_results.len(),
        "HTTP and MCP must return same number of results"
    );

    // Each result item's path and content must match (field-by-field comparison)
    for (http_item, mcp_item) in http_results.iter().zip(mcp_results.iter()) {
        assert_eq!(
            http_item["path"], mcp_item["path"],
            "path must match between HTTP and MCP"
        );
        assert_eq!(
            http_item["content"], mcp_item["content"],
            "content must match between HTTP and MCP"
        );
        assert_eq!(
            http_item["content_hash"], mcp_item["content_hash"],
            "content_hash must match between HTTP and MCP"
        );
        assert_eq!(
            http_item["size"], mcp_item["size"],
            "size must match between HTTP and MCP"
        );
    }

    let _ = shutdown_tx.send(true);
    let _ = handle.await;
}
