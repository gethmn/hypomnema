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
    let state = ApiState {
        vault_manager: Arc::new(manager),
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
        let outbox_path = root.path().join(format!("outbox-{name}.jsonl"));
        let store = Arc::new(store);
        let pool = store.pool();
        let entry = VaultEntry {
            id: vault_id.clone(),
            name: (*name).to_string(),
            vault_path: vault_dir.path().to_path_buf(),
            outbox_path,
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
    let state = ApiState {
        vault_manager: Arc::new(manager),
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
