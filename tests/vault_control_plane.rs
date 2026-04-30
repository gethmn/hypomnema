//! Step 10 vault control-plane integration tests.
//!
//! Composes Tasks 10.2–10.6 (`VaultManager`, HTTP control plane, CLI vault
//! subcommands, cross-vault search refinements, MCP tool wiring) against a
//! real two-vault setup. Two fixture shapes:
//!
//! - `LiveControlPlaneDaemon` — spins up a `VaultManager::open` against a
//!   real `VaultRegistry` so the HTTP `POST /vaults` and `DELETE
//!   /vaults/:name_or_id` routes are end-to-end (registry insert + per-vault
//!   subdir create + watcher + indexer + tear-down). Used by the CRUD,
//!   conflict, idempotency, and concurrency tests.
//! - `MultiVaultDaemon` — uses `VaultManager::for_tests_full` with seeded
//!   SQLite pools (and optional inactive row stubs for paused/errored
//!   diagnostics). Avoids spinning up real watchers per vault for the
//!   pure-search-shape assertions, mirroring the unit-level harness in
//!   `src/api/tests.rs` while exercising the live HTTP surface.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use hypomnema::api::{self, ApiState, VaultEntry};
use hypomnema::config::{
    Config, ConfigPath, EmbeddingConfig, HttpConfig, LoggingConfig, McpConfig, StorageConfig,
    WatcherConfig,
};
use hypomnema::control_plane::VaultManager;
use hypomnema::embedding::{EmbedFuture, Embedder, StubEmbedder};
use hypomnema::store::{SqlitePool, Store};
use hypomnema::vault_registry::{VaultId, VaultRegistry, VaultRow, VaultStatus, vault_data_dir};
use rusqlite::params;
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio::sync::watch;
use tokio::task::{self, JoinHandle};

const DIM: usize = 768;

// ===== LiveControlPlaneDaemon — real VaultManager::open over a TCP port =====

struct LiveControlPlaneDaemon {
    base_url: String,
    root: TempDir,
    data_dir: PathBuf,
    shutdown: watch::Sender<bool>,
    handle: Option<JoinHandle<()>>,
}

impl LiveControlPlaneDaemon {
    async fn shutdown(mut self) {
        let _ = self.shutdown.send(true);
        if let Some(h) = self.handle.take() {
            let _ = h.await;
        }
    }

    fn fresh_vault_dir(&self, name: &str) -> PathBuf {
        let p = self.root.path().join(name);
        fs::create_dir_all(&p).expect("create vault subdir");
        p
    }
}

async fn spawn_live_daemon() -> LiveControlPlaneDaemon {
    let root = TempDir::new().expect("tempdir");
    let data_dir = root.path().join("data");
    fs::create_dir_all(&data_dir).expect("create data_dir");

    let config = make_config(data_dir.clone());
    let config = Arc::new(config);

    let registry = Arc::new(VaultRegistry::open(&data_dir).await.expect("open registry"));

    let embedder: Arc<dyn Embedder> = Arc::new(StubEmbedder::new(DIM));
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let manager = VaultManager::open(
        registry,
        config.clone(),
        embedder,
        DIM as u32,
        shutdown_rx.clone(),
    )
    .await
    .expect("open VaultManager");
    let manager = Arc::new(manager);
    let state = ApiState {
        vault_manager: manager.clone(),
        event_bus: manager.event_bus(),
    };
    let app = api::router(state);

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind 127.0.0.1:0");
    let addr = listener.local_addr().expect("local_addr");
    let mut server_shutdown_rx = shutdown_rx;
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = server_shutdown_rx.wait_for(|v| *v).await;
            })
            .await;
    });

    LiveControlPlaneDaemon {
        base_url: format!("http://{addr}"),
        data_dir,
        root,
        shutdown: shutdown_tx,
        handle: Some(handle),
    }
}

fn make_config(data_dir: PathBuf) -> Config {
    Config {
        vault: None,
        http: HttpConfig::default(),
        mcp: McpConfig::default(),
        embedding: EmbeddingConfig::default(),
        watcher: WatcherConfig::default(),
        storage: StorageConfig {
            data_dir: ConfigPath(data_dir),
            index_file: "index.sqlite".to_string(),
            outbox_file: "outbox.jsonl".to_string(),
        },
        logging: LoggingConfig::default(),
        default_vault_name: "default".to_string(),
    }
}

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("reqwest client")
}

// ===== MultiVaultDaemon — for_tests_full + seeded pools over a TCP port =====
//
// Mirrors the in-process `multi_vault_harness` in `src/api/tests.rs` but
// publishes the router over a real TCP listener so the full reqwest-driven
// HTTP path is exercised (header parsing, content negotiation, partial
// response aggregation).

struct MultiVaultDaemon {
    base_url: String,
    pools: Vec<SqlitePool>,
    #[allow(dead_code)]
    ids: Vec<VaultId>,
    #[allow(dead_code)]
    names: Vec<String>,
    _root: TempDir,
    _vault_dirs: Vec<TempDir>,
    shutdown: watch::Sender<bool>,
    handle: Option<JoinHandle<()>>,
}

impl MultiVaultDaemon {
    async fn shutdown(mut self) {
        let _ = self.shutdown.send(true);
        if let Some(h) = self.handle.take() {
            let _ = h.await;
        }
    }
}

#[derive(Debug, Clone)]
struct InactiveStub {
    name: String,
    status: VaultStatus,
    last_error: Option<String>,
}

async fn spawn_multi_vault_daemon(active_names: Vec<&'static str>) -> MultiVaultDaemon {
    spawn_multi_vault_daemon_with(active_names, vec![], Arc::new(StubEmbedder::new(DIM))).await
}

async fn spawn_multi_vault_daemon_with(
    active_names: Vec<&'static str>,
    inactive: Vec<InactiveStub>,
    embedder: Arc<dyn Embedder>,
) -> MultiVaultDaemon {
    let root = TempDir::new().expect("tempdir");
    let mut pools: Vec<SqlitePool> = Vec::new();
    let mut ids: Vec<VaultId> = Vec::new();
    let mut names: Vec<String> = Vec::new();
    let mut entries: Vec<VaultEntry> = Vec::new();
    let mut vault_dirs: Vec<TempDir> = Vec::new();

    for name in &active_names {
        let vault_dir = TempDir::new().expect("vault tempdir");
        let vault_id = VaultId::new();
        let store = Store::open(
            &vault_id,
            root.path(),
            "index.sqlite",
            &EmbeddingConfig::default(),
        )
        .await
        .expect("open store");
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
            path: PathBuf::from("/dev/null"),
            status: stub.status,
            created_at: Utc::now(),
            last_error: stub.last_error,
        })
        .collect();

    let manager = VaultManager::for_tests_full(entries, inactive_rows, embedder, DIM as u32);
    let manager = Arc::new(manager);
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

    MultiVaultDaemon {
        base_url: format!("http://{addr}"),
        pools,
        ids,
        names,
        _root: root,
        _vault_dirs: vault_dirs,
        shutdown: shutdown_tx,
        handle: Some(handle),
    }
}

async fn seed_files(pool: SqlitePool, rows: Vec<(&'static str, &'static str, &'static str)>) {
    task::spawn_blocking(move || {
        let conn = pool.get().expect("get conn");
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
            .expect("seed insert");
        }
    })
    .await
    .expect("seed_files join");
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
        let mut conn = pool.get().expect("get conn");
        let tx = conn.transaction().expect("begin tx");
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
        .expect("seed chunk");
        let chunk_id = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO chunks_vec (chunk_id, embedding) VALUES (?1, ?2)",
            params![chunk_id, bytemuck::cast_slice::<f32, u8>(&embedding)],
        )
        .expect("seed chunk vec");
        tx.commit().expect("commit chunk seed");
    })
    .await
    .expect("seed_chunk join");
}

fn unit_vec(positions: &[(usize, f32)]) -> Vec<f32> {
    let mut v = vec![0.0f32; DIM];
    for (i, x) in positions {
        v[*i] = *x;
    }
    v
}

/// Test embedder that returns a fixed unit vector regardless of input. Used
/// by the cross-vault semantic test where each per-vault `search_semantic`
/// call re-invokes the embedder; a one-shot embedder would error on the
/// second call and mask the per-vault iteration shape.
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

// ===== Test 1 — POST /vaults then GET /vaults =====

#[tokio::test]
async fn http_create_vault_succeeds_and_appears_in_list() {
    let daemon = spawn_live_daemon().await;
    let vault_path = daemon.fresh_vault_dir("v1");

    let create: Value = http_client()
        .post(format!("{}/vaults", daemon.base_url))
        .json(&json!({ "name": "alpha", "path": vault_path.to_str().unwrap() }))
        .send()
        .await
        .expect("POST /vaults")
        .error_for_status()
        .expect("POST /vaults 2xx")
        .json()
        .await
        .expect("create JSON");
    assert_eq!(create["name"], "alpha");
    assert_eq!(create["status"], "active");
    let id = create["id"].as_str().expect("create.id").to_string();
    assert!(!id.is_empty());

    let listed: Value = http_client()
        .get(format!("{}/vaults", daemon.base_url))
        .send()
        .await
        .expect("GET /vaults")
        .error_for_status()
        .expect("GET /vaults 2xx")
        .json()
        .await
        .expect("list JSON");
    let vaults = listed["vaults"].as_array().expect("vaults array");
    assert_eq!(vaults.len(), 1);
    assert_eq!(vaults[0]["id"], id);
    assert_eq!(vaults[0]["name"], "alpha");

    daemon.shutdown().await;
}

// ===== Test 2 — duplicate path → 409 vault_path_conflict =====

#[tokio::test]
async fn http_create_vault_path_conflict_returns_409() {
    let daemon = spawn_live_daemon().await;
    let vault_path = daemon.fresh_vault_dir("v");

    let _ = http_client()
        .post(format!("{}/vaults", daemon.base_url))
        .json(&json!({ "name": "first", "path": vault_path.to_str().unwrap() }))
        .send()
        .await
        .expect("first POST")
        .error_for_status()
        .expect("first 2xx");

    let resp = http_client()
        .post(format!("{}/vaults", daemon.base_url))
        .json(&json!({ "name": "second", "path": vault_path.to_str().unwrap() }))
        .send()
        .await
        .expect("second POST");
    assert_eq!(resp.status().as_u16(), 409);
    let body: Value = resp.json().await.expect("conflict JSON");
    assert_eq!(body["error"]["code"], "vault_path_conflict");

    daemon.shutdown().await;
}

// ===== Test 3 — duplicate name → 409 vault_name_conflict =====

#[tokio::test]
async fn http_create_vault_name_conflict_returns_409() {
    let daemon = spawn_live_daemon().await;
    let path_a = daemon.fresh_vault_dir("a");
    let path_b = daemon.fresh_vault_dir("b");

    let _ = http_client()
        .post(format!("{}/vaults", daemon.base_url))
        .json(&json!({ "name": "shared", "path": path_a.to_str().unwrap() }))
        .send()
        .await
        .expect("first POST")
        .error_for_status()
        .expect("first 2xx");

    let resp = http_client()
        .post(format!("{}/vaults", daemon.base_url))
        .json(&json!({ "name": "shared", "path": path_b.to_str().unwrap() }))
        .send()
        .await
        .expect("second POST");
    assert_eq!(resp.status().as_u16(), 409);
    let body: Value = resp.json().await.expect("conflict JSON");
    assert_eq!(body["error"]["code"], "vault_name_conflict");

    daemon.shutdown().await;
}

// ===== Test 4 — non-existent path → 422 vault_path_invalid =====

#[tokio::test]
async fn http_create_vault_invalid_path_returns_422() {
    let daemon = spawn_live_daemon().await;
    let bogus = daemon.root.path().join("does-not-exist");

    let resp = http_client()
        .post(format!("{}/vaults", daemon.base_url))
        .json(&json!({ "name": "ghost", "path": bogus.to_str().unwrap() }))
        .send()
        .await
        .expect("POST /vaults");
    assert_eq!(resp.status().as_u16(), 422);
    let body: Value = resp.json().await.expect("invalid JSON");
    assert_eq!(body["error"]["code"], "vault_path_invalid");

    daemon.shutdown().await;
}

// ===== Test 5 — GET on near-miss → 404 with hint =====

#[tokio::test]
async fn http_get_vault_unknown_returns_404_with_hint() {
    let daemon = spawn_live_daemon().await;
    let path = daemon.fresh_vault_dir("v");
    let _ = http_client()
        .post(format!("{}/vaults", daemon.base_url))
        .json(&json!({ "name": "personal", "path": path.to_str().unwrap() }))
        .send()
        .await
        .expect("create")
        .error_for_status()
        .expect("create 2xx");

    let resp = http_client()
        .get(format!("{}/vaults/personel", daemon.base_url))
        .send()
        .await
        .expect("GET near-miss");
    assert_eq!(resp.status().as_u16(), 404);
    let body: Value = resp.json().await.expect("404 JSON");
    assert_eq!(body["error"]["code"], "vault_not_found");
    let msg = body["error"]["message"]
        .as_str()
        .expect("error.message string");
    assert!(
        msg.contains("personal"),
        "404 message should suggest 'personal'; got {msg:?}"
    );

    daemon.shutdown().await;
}

// ===== Test 6 — DELETE removes from /vaults list =====

#[tokio::test]
async fn http_delete_vault_succeeds_and_removes_from_list() {
    let daemon = spawn_live_daemon().await;
    let path = daemon.fresh_vault_dir("v");

    let create: Value = http_client()
        .post(format!("{}/vaults", daemon.base_url))
        .json(&json!({ "name": "doomed", "path": path.to_str().unwrap() }))
        .send()
        .await
        .expect("POST")
        .error_for_status()
        .expect("create 2xx")
        .json()
        .await
        .expect("create JSON");
    let id = create["id"].as_str().unwrap().to_string();

    let term: Value = http_client()
        .delete(format!("{}/vaults/doomed", daemon.base_url))
        .send()
        .await
        .expect("DELETE")
        .error_for_status()
        .expect("DELETE 2xx")
        .json()
        .await
        .expect("term JSON");
    assert_eq!(term["terminated"], true);
    assert_eq!(term["id"], id);

    let listed: Value = http_client()
        .get(format!("{}/vaults", daemon.base_url))
        .send()
        .await
        .expect("GET")
        .error_for_status()
        .expect("list 2xx")
        .json()
        .await
        .expect("list JSON");
    assert!(listed["vaults"].as_array().unwrap().is_empty());

    daemon.shutdown().await;
}

// ===== Test 7 — DELETE unknown → 404 =====

#[tokio::test]
async fn http_delete_vault_unknown_returns_404() {
    let daemon = spawn_live_daemon().await;

    let resp = http_client()
        .delete(format!("{}/vaults/never-was", daemon.base_url))
        .send()
        .await
        .expect("DELETE");
    assert_eq!(resp.status().as_u16(), 404);
    let body: Value = resp.json().await.expect("404 JSON");
    assert_eq!(body["error"]["code"], "vault_not_found");

    daemon.shutdown().await;
}

// ===== Test 8 — terminate then re-create with same name (ADR-0010 idempotency) =====

#[tokio::test]
async fn http_terminate_then_create_with_same_name_succeeds() {
    let daemon = spawn_live_daemon().await;
    let path_a = daemon.fresh_vault_dir("a");
    let path_b = daemon.fresh_vault_dir("b");

    let first: Value = http_client()
        .post(format!("{}/vaults", daemon.base_url))
        .json(&json!({ "name": "reborn", "path": path_a.to_str().unwrap() }))
        .send()
        .await
        .expect("first POST")
        .error_for_status()
        .expect("first 2xx")
        .json()
        .await
        .expect("first JSON");
    let first_id = first["id"].as_str().unwrap().to_string();

    let _ = http_client()
        .delete(format!("{}/vaults/reborn", daemon.base_url))
        .send()
        .await
        .expect("DELETE")
        .error_for_status()
        .expect("DELETE 2xx");

    let second: Value = http_client()
        .post(format!("{}/vaults", daemon.base_url))
        .json(&json!({ "name": "reborn", "path": path_b.to_str().unwrap() }))
        .send()
        .await
        .expect("second POST")
        .error_for_status()
        .expect("second 2xx")
        .json()
        .await
        .expect("second JSON");
    let second_id = second["id"].as_str().unwrap().to_string();

    assert_ne!(
        first_id, second_id,
        "terminate-then-recreate must mint a fresh UUID; reusing the prior id would violate ADR-0010"
    );
    assert_eq!(second["name"], "reborn");

    daemon.shutdown().await;
}

// ===== Test 9 — terminate removes per-vault subdir =====

#[tokio::test]
async fn http_terminate_removes_per_vault_subdir() {
    let daemon = spawn_live_daemon().await;
    let path = daemon.fresh_vault_dir("v");

    let create: Value = http_client()
        .post(format!("{}/vaults", daemon.base_url))
        .json(&json!({ "name": "scratch", "path": path.to_str().unwrap() }))
        .send()
        .await
        .expect("POST")
        .error_for_status()
        .expect("create 2xx")
        .json()
        .await
        .expect("create JSON");
    let id_str = create["id"].as_str().unwrap();
    let id = VaultId::from_string(id_str.to_string());
    let subdir = vault_data_dir(&daemon.data_dir, &id);
    assert!(subdir.is_dir(), "per-vault subdir created on POST");

    let _ = http_client()
        .delete(format!("{}/vaults/scratch", daemon.base_url))
        .send()
        .await
        .expect("DELETE")
        .error_for_status()
        .expect("DELETE 2xx");

    assert!(
        !subdir.exists(),
        "per-vault subdir {} should be removed by terminate",
        subdir.display()
    );

    daemon.shutdown().await;
}

// ===== Test 10 — concurrent creates on different names succeed in parallel =====

#[tokio::test]
async fn concurrent_creates_on_different_names_succeed_in_parallel() {
    let daemon = spawn_live_daemon().await;
    let path_a = daemon.fresh_vault_dir("a");
    let path_b = daemon.fresh_vault_dir("b");
    let url = daemon.base_url.clone();
    let client = http_client();

    let req_a = client.post(format!("{url}/vaults")).json(&json!({
        "name": "vault-a",
        "path": path_a.to_str().unwrap(),
    }));
    let req_b = client.post(format!("{url}/vaults")).json(&json!({
        "name": "vault-b",
        "path": path_b.to_str().unwrap(),
    }));

    let (resp_a, resp_b) = tokio::join!(req_a.send(), req_b.send());
    let resp_a = resp_a.expect("send A");
    let resp_b = resp_b.expect("send B");
    assert_eq!(resp_a.status().as_u16(), 200, "concurrent A status");
    assert_eq!(resp_b.status().as_u16(), 200, "concurrent B status");

    let listed: Value = http_client()
        .get(format!("{}/vaults", daemon.base_url))
        .send()
        .await
        .expect("list")
        .error_for_status()
        .expect("list 2xx")
        .json()
        .await
        .expect("list JSON");
    let mut names: Vec<&str> = listed["vaults"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["name"].as_str().unwrap())
        .collect();
    names.sort();
    assert_eq!(names, vec!["vault-a", "vault-b"]);

    daemon.shutdown().await;
}

// ===== Test 11 — concurrent terminate on same vault: one wins, other 404s =====

#[tokio::test]
async fn concurrent_terminate_on_same_vault_one_404s() {
    let daemon = spawn_live_daemon().await;
    let path = daemon.fresh_vault_dir("v");
    let _ = http_client()
        .post(format!("{}/vaults", daemon.base_url))
        .json(&json!({ "name": "twice-terminated", "path": path.to_str().unwrap() }))
        .send()
        .await
        .expect("create")
        .error_for_status()
        .expect("create 2xx");

    let url = daemon.base_url.clone();
    let client = http_client();
    let req_a = client.delete(format!("{url}/vaults/twice-terminated"));
    let req_b = client.delete(format!("{url}/vaults/twice-terminated"));

    let (resp_a, resp_b) = tokio::join!(req_a.send(), req_b.send());
    let resp_a = resp_a.expect("send A");
    let resp_b = resp_b.expect("send B");
    let mut codes = vec![resp_a.status().as_u16(), resp_b.status().as_u16()];
    codes.sort();
    assert_eq!(
        codes,
        vec![200, 404],
        "exactly one terminate should win; got {codes:?}"
    );

    let listed: Value = http_client()
        .get(format!("{}/vaults", daemon.base_url))
        .send()
        .await
        .expect("list")
        .error_for_status()
        .expect("list 2xx")
        .json()
        .await
        .expect("list JSON");
    assert!(listed["vaults"].as_array().unwrap().is_empty());

    daemon.shutdown().await;
}

// ===== Test 12 — cross-vault filesystem search merges global path-asc =====

#[tokio::test]
async fn cross_vault_filesystem_search_returns_intermingled_global_path_sorted() {
    let daemon = spawn_multi_vault_daemon(vec!["alpha", "bravo"]).await;
    seed_files(
        daemon.pools[0].clone(),
        vec![
            ("notes/a.md", "alpha-a", "2026-04-01T00:00:00Z"),
            ("notes/m.md", "alpha-m", "2026-04-01T00:00:00Z"),
        ],
    )
    .await;
    seed_files(
        daemon.pools[1].clone(),
        vec![
            ("notes/b.md", "bravo-b", "2026-04-01T00:00:00Z"),
            ("notes/z.md", "bravo-z", "2026-04-01T00:00:00Z"),
        ],
    )
    .await;

    let body: Value = http_client()
        .post(format!("{}/search/filesystem", daemon.base_url))
        .json(&json!({ "glob": "**/*.md" }))
        .send()
        .await
        .expect("POST /search/filesystem")
        .error_for_status()
        .expect("2xx")
        .json()
        .await
        .expect("response JSON");
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

    daemon.shutdown().await;
}

// ===== Test 13 — cross-vault content search merges global path-asc =====

#[tokio::test]
async fn cross_vault_content_search_returns_intermingled_global_path_sorted() {
    let daemon = spawn_multi_vault_daemon(vec!["alpha", "bravo"]).await;
    seed_files(
        daemon.pools[0].clone(),
        vec![
            ("docs/a.md", "needle alpha", "2026-04-01T00:00:00Z"),
            ("docs/m.md", "needle alpha-m", "2026-04-01T00:00:00Z"),
        ],
    )
    .await;
    seed_files(
        daemon.pools[1].clone(),
        vec![
            ("docs/b.md", "needle bravo", "2026-04-01T00:00:00Z"),
            ("docs/z.md", "needle bravo-z", "2026-04-01T00:00:00Z"),
        ],
    )
    .await;

    let body: Value = http_client()
        .post(format!("{}/search/content", daemon.base_url))
        .json(&json!({ "query": "needle" }))
        .send()
        .await
        .expect("POST /search/content")
        .error_for_status()
        .expect("2xx")
        .json()
        .await
        .expect("response JSON");
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

    daemon.shutdown().await;
}

// ===== Test 14 — cross-vault semantic search merges score-desc =====

#[tokio::test]
async fn cross_vault_semantic_search_returns_intermingled_score_desc_sorted() {
    let embedder = FixedEmbedder::new(&[(0, 1.0)]);
    let daemon = spawn_multi_vault_daemon_with(vec!["alpha", "bravo"], vec![], embedder).await;

    seed_chunk(
        daemon.pools[0].clone(),
        "alpha.md",
        0,
        "Intro",
        "alpha body",
        unit_vec(&[(1, 1.0)]),
    )
    .await;
    seed_chunk(
        daemon.pools[1].clone(),
        "bravo.md",
        0,
        "Intro",
        "bravo body",
        unit_vec(&[(0, 1.0)]),
    )
    .await;

    let body: Value = http_client()
        .post(format!("{}/search/semantic", daemon.base_url))
        .json(&json!({ "query": "any" }))
        .send()
        .await
        .expect("POST /search/semantic")
        .error_for_status()
        .expect("2xx")
        .json()
        .await
        .expect("response JSON");

    let results = body["results"].as_array().unwrap();
    assert_eq!(results.len(), 2);
    let s0 = results[0]["score"].as_f64().unwrap();
    let s1 = results[1]["score"].as_f64().unwrap();
    assert!(s0 >= s1, "expected score-desc but got {s0} then {s1}");
    assert_eq!(results[0]["file_path"], "bravo.md");
    assert_eq!(results[1]["file_path"], "alpha.md");

    daemon.shutdown().await;
}

// ===== Test 15 — vaults filter narrows subset =====

#[tokio::test]
async fn vaults_filter_narrows_subset() {
    let daemon = spawn_multi_vault_daemon(vec!["personal", "work"]).await;
    seed_files(
        daemon.pools[0].clone(),
        vec![("p.md", "p", "2026-04-01T00:00:00Z")],
    )
    .await;
    seed_files(
        daemon.pools[1].clone(),
        vec![("w.md", "w", "2026-04-01T00:00:00Z")],
    )
    .await;

    let body: Value = http_client()
        .post(format!("{}/search/filesystem", daemon.base_url))
        .json(&json!({ "glob": "**/*.md", "vaults": ["personal"] }))
        .send()
        .await
        .expect("POST /search/filesystem")
        .error_for_status()
        .expect("2xx")
        .json()
        .await
        .expect("response JSON");

    let paths: Vec<&str> = body["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, vec!["p.md"]);
    let vault_names: Vec<&str> = body["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["vault_name"].as_str().unwrap())
        .collect();
    assert_eq!(vault_names, vec!["personal"]);

    daemon.shutdown().await;
}

// ===== Test 16 — paused vault skipped with partial_results diagnostic =====

#[tokio::test]
async fn paused_vault_skipped_in_default_scope_with_partial_results_diagnostic() {
    let daemon = spawn_multi_vault_daemon_with(
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
        daemon.pools[0].clone(),
        vec![("a.md", "x", "2026-04-01T00:00:00Z")],
    )
    .await;

    let body: Value = http_client()
        .post(format!("{}/search/filesystem", daemon.base_url))
        .json(&json!({ "glob": "**/*.md" }))
        .send()
        .await
        .expect("POST /search/filesystem")
        .error_for_status()
        .expect("2xx")
        .json()
        .await
        .expect("response JSON");

    assert_eq!(body["results"].as_array().unwrap().len(), 1);
    let skipped = body["partial_results"]["skipped"].as_array().unwrap();
    assert_eq!(skipped.len(), 1);
    assert_eq!(skipped[0]["status"], "paused");
    assert_eq!(skipped[0]["vault_name"], "bravo");
    assert_eq!(skipped[0]["reason"], "vault is paused");

    daemon.shutdown().await;
}

// ===== Test 17 — errored vault skipped with last_error propagated =====

#[tokio::test]
async fn errored_vault_skipped_with_last_error_propagated() {
    let daemon = spawn_multi_vault_daemon_with(
        vec!["alpha"],
        vec![InactiveStub {
            name: "bravo".to_string(),
            status: VaultStatus::Errored,
            last_error: Some("simulated path-vanished".to_string()),
        }],
        Arc::new(StubEmbedder::new(DIM)),
    )
    .await;
    seed_files(
        daemon.pools[0].clone(),
        vec![("a.md", "x", "2026-04-01T00:00:00Z")],
    )
    .await;

    let body: Value = http_client()
        .post(format!("{}/search/filesystem", daemon.base_url))
        .json(&json!({ "glob": "**/*.md" }))
        .send()
        .await
        .expect("POST /search/filesystem")
        .error_for_status()
        .expect("2xx")
        .json()
        .await
        .expect("response JSON");

    assert_eq!(body["results"].as_array().unwrap().len(), 1);
    let skipped = body["partial_results"]["skipped"].as_array().unwrap();
    assert_eq!(skipped.len(), 1);
    assert_eq!(skipped[0]["status"], "errored");
    assert_eq!(skipped[0]["vault_name"], "bravo");
    let reason = skipped[0]["reason"].as_str().unwrap();
    assert!(
        reason.contains("simulated path-vanished"),
        "errored skip reason should propagate last_error; got {reason:?}"
    );

    daemon.shutdown().await;
}

// ===== Step 11 · Task 11.7: lifecycle-op integration tests over HTTP =====
//
// Round-3 shipping gate. Each test exercises a single lifecycle op (or a
// combination) end-to-end through the live HTTP TCP transport using
// `LiveControlPlaneDaemon` (real `VaultManager::open`, real per-vault store +
// outbox + watcher + indexer). The unit-level handler tests in
// `src/api/tests.rs` cover happy-path shapes; these add: real HTTP transport,
// outbox-file inspection, post-rebuild rescan emission, multi-op composition,
// and concurrency edge cases.

/// Like `spawn_live_daemon`, but pre-inserts an `Errored` row into the
/// registry before opening the manager. Mirrors `errored_vault_harness` from
/// `src/api/tests.rs`. Returns the daemon plus the bogus path the errored row
/// was registered against (the caller can `fs::create_dir_all(&bogus)` to
/// flip the path-accessibility precondition before calling `/reset`).
async fn spawn_live_daemon_with_errored_row(
    errored_name: &str,
    last_error: &str,
) -> (LiveControlPlaneDaemon, PathBuf) {
    let root = TempDir::new().expect("tempdir");
    let data_dir = root.path().join("data");
    fs::create_dir_all(&data_dir).expect("create data_dir");

    let bogus = root.path().join("not-there");
    let id = VaultId::new();
    let registry = Arc::new(VaultRegistry::open(&data_dir).await.expect("open registry"));
    registry
        .insert(VaultRow {
            id,
            name: errored_name.to_string(),
            path: bogus.clone(),
            status: VaultStatus::Errored,
            created_at: Utc::now(),
            last_error: Some(last_error.to_string()),
        })
        .await
        .expect("seed errored row");

    let config = Arc::new(make_config(data_dir.clone()));
    let embedder: Arc<dyn Embedder> = Arc::new(StubEmbedder::new(DIM));
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let manager = VaultManager::open(
        registry,
        config.clone(),
        embedder,
        DIM as u32,
        shutdown_rx.clone(),
    )
    .await
    .expect("open VaultManager");
    let manager = Arc::new(manager);
    let state = ApiState {
        vault_manager: manager.clone(),
        event_bus: manager.event_bus(),
    };
    let app = api::router(state);

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind 127.0.0.1:0");
    let addr = listener.local_addr().expect("local_addr");
    let mut server_shutdown_rx = shutdown_rx;
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = server_shutdown_rx.wait_for(|v| *v).await;
            })
            .await;
    });

    (
        LiveControlPlaneDaemon {
            base_url: format!("http://{addr}"),
            data_dir,
            root,
            shutdown: shutdown_tx,
            handle: Some(handle),
        },
        bogus,
    )
}

async fn count_chunks_for_vault(daemon: &LiveControlPlaneDaemon, vault_name: &str) -> i64 {
    // Drive the SQL through the public HTTP semantic-search endpoint? No —
    // we want a direct chunk count. Re-open the per-vault store read-only
    // via the on-disk index file. Avoids reaching into the manager's
    // internals from outside the crate.
    let index_path = daemon
        .data_dir
        .join("vaults")
        .join(resolve_vault_id(daemon, vault_name).await)
        .join("index.sqlite");
    let index_path = index_path.to_path_buf();
    task::spawn_blocking(move || {
        let conn = rusqlite::Connection::open(&index_path).expect("open index.sqlite");
        conn.query_row("SELECT COUNT(*) FROM chunks", [], |r| r.get::<_, i64>(0))
            .expect("count chunks")
    })
    .await
    .expect("count_chunks join")
}

async fn resolve_vault_id(daemon: &LiveControlPlaneDaemon, vault_name: &str) -> String {
    let vault: Value = http_client()
        .get(format!("{}/vaults/{vault_name}", daemon.base_url))
        .send()
        .await
        .expect("GET /vaults/:name")
        .error_for_status()
        .expect("GET vault 2xx")
        .json()
        .await
        .expect("vault JSON");
    vault["id"].as_str().expect("vault.id").to_string()
}

// ===== Test 18 — pause then resume round-trip via HTTP =====

#[tokio::test]
async fn http_pause_then_resume_round_trip() {
    let daemon = spawn_live_daemon().await;
    let path = daemon.fresh_vault_dir("v");
    fs::write(path.join("note.md"), b"# Title\n\nneedle keyword body.\n").expect("write file");

    let create: Value = http_client()
        .post(format!("{}/vaults", daemon.base_url))
        .json(&json!({ "name": "round-trip", "path": path.to_str().unwrap() }))
        .send()
        .await
        .expect("create")
        .error_for_status()
        .expect("create 2xx")
        .json()
        .await
        .expect("create JSON");
    assert_eq!(create["status"], "active");

    // Pause — assert returned row carries status=paused.
    let paused: Value = http_client()
        .post(format!("{}/vaults/round-trip/pause", daemon.base_url))
        .send()
        .await
        .expect("pause")
        .error_for_status()
        .expect("pause 2xx")
        .json()
        .await
        .expect("pause JSON");
    assert_eq!(paused["status"], "paused");
    assert_eq!(paused["name"], "round-trip");

    // Search/filesystem should report the vault in partial_results.skipped
    // with status=paused.
    let body: Value = http_client()
        .post(format!("{}/search/filesystem", daemon.base_url))
        .json(&json!({ "glob": "**/*.md" }))
        .send()
        .await
        .expect("search")
        .error_for_status()
        .expect("search 2xx")
        .json()
        .await
        .expect("search JSON");
    assert_eq!(
        body["results"].as_array().unwrap().len(),
        0,
        "paused vault contributes no results: {body}"
    );
    let skipped = body["partial_results"]["skipped"].as_array().unwrap();
    assert_eq!(skipped.len(), 1);
    assert_eq!(skipped[0]["status"], "paused");
    assert_eq!(skipped[0]["vault_name"], "round-trip");

    // Resume — assert row flips back to active.
    let resumed: Value = http_client()
        .post(format!("{}/vaults/round-trip/resume", daemon.base_url))
        .send()
        .await
        .expect("resume")
        .error_for_status()
        .expect("resume 2xx")
        .json()
        .await
        .expect("resume JSON");
    assert_eq!(resumed["status"], "active");

    // Subsequent search returns vault's results normally; partial_results
    // is absent (or null).
    let body: Value = http_client()
        .post(format!("{}/search/filesystem", daemon.base_url))
        .json(&json!({ "glob": "**/*.md" }))
        .send()
        .await
        .expect("search post-resume")
        .error_for_status()
        .expect("search 2xx")
        .json()
        .await
        .expect("search JSON");
    let paths: Vec<&str> = body["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, vec!["note.md"]);
    assert!(body.get("partial_results").is_none() || body["partial_results"].is_null());

    daemon.shutdown().await;
}

// ===== Test 19 — reset clears errored state via HTTP =====

#[tokio::test]
async fn http_reset_clears_errored_state() {
    let (daemon, bogus) = spawn_live_daemon_with_errored_row("errd", "path missing").await;
    // Flip the precondition: the errored row's path now exists, so reset
    // can rebuild lifecycle and update status to Active.
    fs::create_dir_all(&bogus).expect("create previously-bogus path");

    let resp = http_client()
        .post(format!("{}/vaults/errd/reset", daemon.base_url))
        .json(&json!({}))
        .send()
        .await
        .expect("POST /reset");
    assert_eq!(resp.status().as_u16(), 200, "reset should succeed");
    let body: Value = resp.json().await.expect("reset JSON");
    assert_eq!(body["status"], "active");
    assert_eq!(body["name"], "errd");

    // Subsequent GET /vaults/errd reflects the active state — the row is
    // canonically active and the runner is wired up.
    let listed: Value = http_client()
        .get(format!("{}/vaults/errd", daemon.base_url))
        .send()
        .await
        .expect("GET")
        .error_for_status()
        .expect("GET 2xx")
        .json()
        .await
        .expect("GET JSON");
    assert_eq!(listed["status"], "active");

    daemon.shutdown().await;
}

// ===== Test 20 — reset --rebuild clears chunks/chunks_vec/content_hash, preserves outbox =====

#[tokio::test]
async fn http_reset_with_rebuild_clears_chunks_chunks_vec_and_content_hash() {
    let daemon = spawn_live_daemon().await;
    let path = daemon.fresh_vault_dir("v");
    fs::write(path.join("a.md"), b"# Alpha\n\nFirst body.\n").expect("a.md");
    fs::write(path.join("b.md"), b"# Bravo\n\nSecond body.\n").expect("b.md");

    let _ = http_client()
        .post(format!("{}/vaults", daemon.base_url))
        .json(&json!({ "name": "rebuild-target", "path": path.to_str().unwrap() }))
        .send()
        .await
        .expect("create")
        .error_for_status()
        .expect("create 2xx");

    let chunks_before = count_chunks_for_vault(&daemon, "rebuild-target").await;
    assert!(
        chunks_before > 0,
        "initial scan should populate chunks; got {chunks_before}"
    );

    // Reset with rebuild=true — drains the lifecycle, runs the rebuild SQL,
    // and re-spawns with skip_initial_scan=true so content_hash stays empty.
    let body: Value = http_client()
        .post(format!("{}/vaults/rebuild-target/reset", daemon.base_url))
        .json(&json!({ "rebuild": true }))
        .send()
        .await
        .expect("reset")
        .error_for_status()
        .expect("reset 2xx")
        .json()
        .await
        .expect("reset JSON");
    assert_eq!(body["status"], "active");

    let chunks_after = count_chunks_for_vault(&daemon, "rebuild-target").await;
    assert_eq!(chunks_after, 0, "rebuild should clear chunks");

    // Verify content_hash was wiped on every files row.
    let vault_id = resolve_vault_id(&daemon, "rebuild-target").await;
    let index_path = daemon
        .data_dir
        .join("vaults")
        .join(&vault_id)
        .join("index.sqlite");
    let (file_count, empty_hash_count) = task::spawn_blocking(move || -> (i64, i64) {
        let conn = rusqlite::Connection::open(&index_path).expect("open index.sqlite");
        let total: i64 = conn
            .query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))
            .expect("count files");
        let empty: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM files WHERE content_hash = ''",
                [],
                |r| r.get(0),
            )
            .expect("count empty hash");
        (total, empty)
    })
    .await
    .expect("join");
    assert_eq!(file_count, 2, "files rows preserved across rebuild");
    assert_eq!(
        empty_hash_count, 2,
        "rebuild should wipe content_hash on every file"
    );

    daemon.shutdown().await;
}

// ===== Test 21 — rename updates search response vault_name =====

#[tokio::test]
async fn http_rename_updates_search_response_vault_name() {
    let daemon = spawn_live_daemon().await;
    let path = daemon.fresh_vault_dir("v");
    fs::write(path.join("renamed.md"), b"# Topic\n\nrenamed body.\n").expect("write");

    let create: Value = http_client()
        .post(format!("{}/vaults", daemon.base_url))
        .json(&json!({ "name": "oldname", "path": path.to_str().unwrap() }))
        .send()
        .await
        .expect("create")
        .error_for_status()
        .expect("create 2xx")
        .json()
        .await
        .expect("create JSON");
    let vault_id_before = create["id"].as_str().unwrap().to_string();

    // Pre-rename search carries vault_name=oldname.
    let body: Value = http_client()
        .post(format!("{}/search/filesystem", daemon.base_url))
        .json(&json!({ "glob": "**/*.md" }))
        .send()
        .await
        .expect("search")
        .error_for_status()
        .expect("search 2xx")
        .json()
        .await
        .expect("search JSON");
    assert_eq!(body["results"][0]["vault_name"], "oldname");

    // Rename.
    let renamed: Value = http_client()
        .post(format!("{}/vaults/oldname/rename", daemon.base_url))
        .json(&json!({ "new_name": "newname" }))
        .send()
        .await
        .expect("rename")
        .error_for_status()
        .expect("rename 2xx")
        .json()
        .await
        .expect("rename JSON");
    assert_eq!(renamed["name"], "newname");
    assert_eq!(
        renamed["id"].as_str().unwrap(),
        vault_id_before,
        "surrogate id is stable across rename"
    );

    // Post-rename search response carries vault_name=newname.
    let body: Value = http_client()
        .post(format!("{}/search/filesystem", daemon.base_url))
        .json(&json!({ "glob": "**/*.md" }))
        .send()
        .await
        .expect("search post-rename")
        .error_for_status()
        .expect("search 2xx")
        .json()
        .await
        .expect("search JSON");
    assert_eq!(body["results"][0]["vault_name"], "newname");

    daemon.shutdown().await;
}

// ===== Test 22 — rescan emits outbox events for existing files (post-rebuild) =====

#[tokio::test]
async fn http_rescan_re_indexes_all_files_after_rebuild() {
    // The rescan-emission path documented in the spec + Task 11.2 forward
    // note: rescan walks the vault and Upserts each file; on stable mtime
    // the indexer's hash-gate short-circuits so a rescan of an up-to-date
    // vault is silent. Pair it with `reset --rebuild` (which clears
    // content_hash) to force per-file re-indexing, exactly the operator
    // workflow Smoke 5 + Smoke 7 verify. Verified via index state (non-empty
    // content_hash) rather than outbox which no longer exists.
    let daemon = spawn_live_daemon().await;
    let path = daemon.fresh_vault_dir("v");
    fs::write(path.join("one.md"), b"# One\n\nFirst.\n").expect("write");
    fs::write(path.join("two.md"), b"# Two\n\nSecond.\n").expect("write");

    let _ = http_client()
        .post(format!("{}/vaults", daemon.base_url))
        .json(&json!({ "name": "rescanned", "path": path.to_str().unwrap() }))
        .send()
        .await
        .expect("create")
        .error_for_status()
        .expect("create 2xx");

    let vault_id = resolve_vault_id(&daemon, "rescanned").await;

    // reset --rebuild to clear content_hash.
    let _ = http_client()
        .post(format!("{}/vaults/rescanned/reset", daemon.base_url))
        .json(&json!({ "rebuild": true }))
        .send()
        .await
        .expect("reset")
        .error_for_status()
        .expect("reset 2xx");

    // Verify content_hash is empty after rebuild.
    let index_path = daemon
        .data_dir
        .join("vaults")
        .join(&vault_id)
        .join("index.sqlite");
    let index_path_for_baseline = index_path.clone();
    let empty_after_rebuild = task::spawn_blocking(move || -> i64 {
        let conn = rusqlite::Connection::open(&index_path_for_baseline).expect("open index.sqlite");
        conn.query_row(
            "SELECT COUNT(*) FROM files WHERE content_hash = ''",
            [],
            |r| r.get(0),
        )
        .expect("count empty hash")
    })
    .await
    .expect("join");
    assert_eq!(
        empty_after_rebuild, 2,
        "rebuild must wipe content_hash on all files before rescan"
    );

    // rescan; expect all files to be re-indexed (content_hash repopulated).
    let resp: Value = http_client()
        .post(format!("{}/vaults/rescanned/rescan", daemon.base_url))
        .send()
        .await
        .expect("rescan")
        .error_for_status()
        .expect("rescan 2xx")
        .json()
        .await
        .expect("rescan JSON");
    assert!(
        resp.get("rescan_initiated_at").is_some(),
        "rescan response carries initiated timestamp: {resp}"
    );

    // Poll until content_hash is non-empty on all files (rescan is async).
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    loop {
        let index_path_poll = index_path.clone();
        let empty = task::spawn_blocking(move || -> i64 {
            let conn =
                rusqlite::Connection::open(&index_path_poll).expect("open index.sqlite for poll");
            conn.query_row(
                "SELECT COUNT(*) FROM files WHERE content_hash = ''",
                [],
                |r| r.get(0),
            )
            .expect("count empty hash in poll")
        })
        .await
        .expect("join poll");
        if empty == 0 {
            break;
        }
        if std::time::Instant::now() >= deadline {
            panic!(
                "rescan did not re-index all files within 15s; files with empty content_hash: {empty}"
            );
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    daemon.shutdown().await;
}

// ===== Test 23 — pause is idempotent (pause-on-paused returns 200 + paused row) =====

#[tokio::test]
async fn http_pause_idempotent() {
    let daemon = spawn_live_daemon().await;
    let path = daemon.fresh_vault_dir("v");
    let _ = http_client()
        .post(format!("{}/vaults", daemon.base_url))
        .json(&json!({ "name": "twice-paused", "path": path.to_str().unwrap() }))
        .send()
        .await
        .expect("create")
        .error_for_status()
        .expect("create 2xx");

    let first: Value = http_client()
        .post(format!("{}/vaults/twice-paused/pause", daemon.base_url))
        .send()
        .await
        .expect("pause #1")
        .error_for_status()
        .expect("pause #1 2xx")
        .json()
        .await
        .expect("pause #1 JSON");
    assert_eq!(first["status"], "paused");

    let second: Value = http_client()
        .post(format!("{}/vaults/twice-paused/pause", daemon.base_url))
        .send()
        .await
        .expect("pause #2")
        .error_for_status()
        .expect("pause #2 2xx")
        .json()
        .await
        .expect("pause #2 JSON");
    assert_eq!(second["status"], "paused");
    assert_eq!(second["id"], first["id"]);

    daemon.shutdown().await;
}

// ===== Test 24 — resume is idempotent (resume-on-active returns 200 + active row) =====

#[tokio::test]
async fn http_resume_idempotent() {
    let daemon = spawn_live_daemon().await;
    let path = daemon.fresh_vault_dir("v");
    let _ = http_client()
        .post(format!("{}/vaults", daemon.base_url))
        .json(&json!({ "name": "always-active", "path": path.to_str().unwrap() }))
        .send()
        .await
        .expect("create")
        .error_for_status()
        .expect("create 2xx");

    // Vault is already active; resume should be a no-op-shaped 200 with the
    // existing row (per manager-layer idempotency guarantee).
    let body: Value = http_client()
        .post(format!("{}/vaults/always-active/resume", daemon.base_url))
        .send()
        .await
        .expect("resume")
        .error_for_status()
        .expect("resume 2xx")
        .json()
        .await
        .expect("resume JSON");
    assert_eq!(body["status"], "active");
    assert_eq!(body["name"], "always-active");

    daemon.shutdown().await;
}

// ===== Test 25 — reset on unknown vault returns 404 =====

#[tokio::test]
async fn http_reset_returns_404_for_unknown() {
    let daemon = spawn_live_daemon().await;

    let resp = http_client()
        .post(format!("{}/vaults/never-was/reset", daemon.base_url))
        .json(&json!({}))
        .send()
        .await
        .expect("reset unknown");
    assert_eq!(resp.status().as_u16(), 404);
    let body: Value = resp.json().await.expect("404 JSON");
    assert_eq!(body["error"]["code"], "vault_not_found");

    daemon.shutdown().await;
}

// ===== Test 26 — rename to a colliding name returns 409 =====

#[tokio::test]
async fn http_rename_returns_409_on_collision() {
    let daemon = spawn_live_daemon().await;
    let path_a = daemon.fresh_vault_dir("a");
    let path_b = daemon.fresh_vault_dir("b");
    let _ = http_client()
        .post(format!("{}/vaults", daemon.base_url))
        .json(&json!({ "name": "alpha", "path": path_a.to_str().unwrap() }))
        .send()
        .await
        .expect("create alpha")
        .error_for_status()
        .expect("create alpha 2xx");
    let _ = http_client()
        .post(format!("{}/vaults", daemon.base_url))
        .json(&json!({ "name": "bravo", "path": path_b.to_str().unwrap() }))
        .send()
        .await
        .expect("create bravo")
        .error_for_status()
        .expect("create bravo 2xx");

    let resp = http_client()
        .post(format!("{}/vaults/alpha/rename", daemon.base_url))
        .json(&json!({ "new_name": "bravo" }))
        .send()
        .await
        .expect("rename");
    assert_eq!(resp.status().as_u16(), 409);
    let body: Value = resp.json().await.expect("409 JSON");
    assert_eq!(body["error"]["code"], "vault_name_conflict");

    daemon.shutdown().await;
}

// ===== Test 27 — concurrent pause + terminate on the same vault don't corrupt state =====

#[tokio::test]
async fn concurrent_pause_and_terminate_dont_corrupt_state() {
    let daemon = spawn_live_daemon().await;
    let path = daemon.fresh_vault_dir("v");
    let _ = http_client()
        .post(format!("{}/vaults", daemon.base_url))
        .json(&json!({ "name": "racy", "path": path.to_str().unwrap() }))
        .send()
        .await
        .expect("create")
        .error_for_status()
        .expect("create 2xx");

    let url = daemon.base_url.clone();
    let client = http_client();
    let pause = client.post(format!("{url}/vaults/racy/pause"));
    let terminate = client.delete(format!("{url}/vaults/racy"));
    let (pause_resp, term_resp) = tokio::join!(pause.send(), terminate.send());
    let pause_status = pause_resp.expect("pause send").status().as_u16();
    let term_status = term_resp.expect("terminate send").status().as_u16();

    // The serializing op_lock guarantees ordered application; the second to
    // run sees registry state that already-applied the first. Acceptable
    // outcomes:
    //   - pause first wins (200), terminate then succeeds (200): final list
    //     is empty.
    //   - terminate first wins (200), pause then 404s on resolve: final
    //     list is empty.
    //   - terminate first wins (200), pause races on the registry row read
    //     and 200s with the just-paused row before terminate's deletion
    //     lands; this is a narrow but legal interleaving — the post-test
    //     final-list assertion still holds.
    let pair = (pause_status, term_status);
    assert!(
        matches!(pair, (200, 200) | (404, 200) | (200, 404)),
        "unexpected status pair from concurrent pause + terminate: {pair:?}"
    );

    let listed: Value = http_client()
        .get(format!("{}/vaults", daemon.base_url))
        .send()
        .await
        .expect("list")
        .error_for_status()
        .expect("list 2xx")
        .json()
        .await
        .expect("list JSON");
    assert!(
        listed["vaults"].as_array().unwrap().is_empty(),
        "post-race vault list should be empty (terminate must have applied); got {listed}"
    );

    daemon.shutdown().await;
}
