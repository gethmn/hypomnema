//! Integration tests for the chunk → embed → store pipeline against a live
//! `hmnd`-style stack and a stub embedding service. Per workplan § Task 6.6.
//!
//! Each test:
//! - Stands up a `TcpListener::bind("127.0.0.1:0")` stub embedding service
//!   with a configurable response mode (200 / 503 / wrong-dim).
//! - Builds a tempdir vault + data_dir + Config wired to the stub URL.
//! - Composes Store + Scanner + Watcher + axum HTTP server (the live
//!   `hmnd` shape from `tests/http.rs` and `tests/watch.rs`).
//! - Drives a vault file change and asserts on the SQLite index.
//!
//! Anti-flake: per workplan § Test strategy, no polling-loop helpers — a
//! fixed `SETTLE` window. Flakes on a non-deterministic boundary are
//! signal, not noise. The 3× flake-check budget at task close is the
//! safety net.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use hypomnema::api::{self, ApiState, VaultEntry};
use hypomnema::chunk::chunk_file;
use hypomnema::config::{Config, EmbeddingConfig};
use hypomnema::control_plane::VaultManager;
use hypomnema::embedding::{EmbedFuture, Embedder, EmbeddingClient};
use hypomnema::indexer::Scanner;
use hypomnema::outbox::Outbox;
use hypomnema::store::Store;
use hypomnema::vault_registry::VaultStatus;
use hypomnema::vault_registry::{VaultId, vault_data_dir};
use hypomnema::watcher::{self, Watcher};
use rusqlite::{Connection, OpenFlags, params};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio::task::JoinHandle;

const DEBOUNCE_MS: u64 = 50;
const SETTLE: Duration = Duration::from_millis(4 * DEBOUNCE_MS);
const SCHEMA_DIM: usize = 768;

// ===== Stub embedding service =====

#[derive(Clone)]
enum StubMode {
    Ok,
    Err503,
    WrongDim(usize),
}

struct StubServer {
    url: String,
    mode: Arc<Mutex<StubMode>>,
    shutdown_tx: watch::Sender<bool>,
    handle: Option<JoinHandle<()>>,
}

impl StubServer {
    async fn spawn(mode: StubMode) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind stub");
        let addr = listener.local_addr().expect("stub local_addr");
        let (tx, mut rx) = watch::channel(false);
        let mode = Arc::new(Mutex::new(mode));
        let mode_for_loop = mode.clone();
        let handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = rx.wait_for(|v| *v) => break,
                    accepted = listener.accept() => {
                        let stream = match accepted {
                            Ok((s, _)) => s,
                            Err(_) => continue,
                        };
                        // Lock-clone-release: never hold the Mutex across an
                        // .await in the per-connection handler (footgun with
                        // std::sync::Mutex on tokio).
                        let snapshot = mode_for_loop.lock().expect("stub mode lock").clone();
                        tokio::spawn(handle_request(stream, snapshot));
                    }
                }
            }
        });
        Self {
            url: format!("http://{addr}/v1/embeddings"),
            mode,
            shutdown_tx: tx,
            handle: Some(handle),
        }
    }

    /// Atomically swap the stub's response mode. The next request to land in
    /// `accept()` will pick up the new mode. Tests must serialize their
    /// mode-change-then-request call so an in-flight request doesn't race
    /// (the workplan calls this out — `set_mode` is intentionally not async
    /// and not coordinated with in-flight handlers).
    fn set_mode(&self, m: StubMode) {
        *self.mode.lock().expect("stub mode lock") = m;
    }

    async fn shutdown(mut self) {
        let _ = self.shutdown_tx.send(true);
        if let Some(h) = self.handle.take() {
            let _ = h.await;
        }
    }
}

async fn handle_request(mut stream: TcpStream, mode: StubMode) {
    drain_request(&mut stream).await;
    let (status, status_text, body) = match mode {
        StubMode::Ok => (200u16, "OK", vector_response(SCHEMA_DIM)),
        StubMode::Err503 => (503u16, "Service Unavailable", "service down".to_string()),
        StubMode::WrongDim(n) => (200u16, "OK", vector_response(n)),
    };
    let resp = format!(
        "HTTP/1.1 {status} {status_text}\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\
         \r\n{body}",
        body.len()
    );
    let _ = stream.write_all(resp.as_bytes()).await;
    let _ = stream.flush().await;
    let _ = stream.shutdown().await;
}

async fn drain_request(stream: &mut TcpStream) {
    let mut buf = [0u8; 4096];
    let mut accum: Vec<u8> = Vec::new();
    let header_end = loop {
        match stream.read(&mut buf).await {
            Ok(0) => return,
            Ok(n) => {
                accum.extend_from_slice(&buf[..n]);
                if let Some(idx) = accum.windows(4).position(|w| w == b"\r\n\r\n") {
                    break idx;
                }
            }
            Err(_) => return,
        }
    };
    let header_str = String::from_utf8_lossy(&accum[..header_end]).to_string();
    let cl = parse_content_length(&header_str).unwrap_or(0);
    let body_so_far = accum.len().saturating_sub(header_end + 4);
    let mut remaining = cl.saturating_sub(body_so_far);
    while remaining > 0 {
        match stream.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => remaining = remaining.saturating_sub(n),
            Err(_) => break,
        }
    }
}

fn parse_content_length(headers: &str) -> Option<usize> {
    for line in headers.lines() {
        if let Some(idx) = line.find(':') {
            let (name, value) = line.split_at(idx);
            if name.eq_ignore_ascii_case("content-length") {
                return value[1..].trim().parse().ok();
            }
        }
    }
    None
}

fn vector_response(dim: usize) -> String {
    let v: Vec<f32> = (0..dim).map(|i| (i as f32) * 0.001).collect();
    json!({ "data": [{ "embedding": v }] }).to_string()
}

// ===== Test fixture =====

struct Fixture {
    _root: TempDir,
    vault: PathBuf,
    data_dir: PathBuf,
    cfg_path: PathBuf,
    config: Config,
    vault_id: VaultId,
}

fn fixture(stub_url: &str) -> Fixture {
    let root = tempfile::tempdir().expect("create root tempdir");
    let vault = root.path().join("vault");
    let data_dir = root.path().join("data");
    fs::create_dir_all(&vault).expect("create vault dir");

    let cfg_path = root.path().join("config.toml");
    fs::write(
        &cfg_path,
        format!(
            "vault = \"{}\"\n\
             [storage]\n\
             data_dir = \"{}\"\n\
             [watcher]\n\
             debounce_ms = {}\n\
             [embedding]\n\
             endpoint = \"{}\"\n\
             max_retries = 0\n",
            vault.display(),
            data_dir.display(),
            DEBOUNCE_MS,
            stub_url,
        ),
    )
    .expect("write config.toml");
    let config = Config::load(Some(&cfg_path)).expect("load config");
    let vault = config
        .vault
        .as_ref()
        .expect("test config must define [vault] block")
        .0
        .clone();
    let data_dir = config.storage.data_dir.0.clone();
    Fixture {
        _root: root,
        vault,
        data_dir,
        cfg_path,
        config,
        vault_id: VaultId::new(),
    }
}

// ===== Live daemon composition =====

struct LiveDaemon {
    base_url: String,
    cfg_path: PathBuf,
    data_dir: PathBuf,
    vault: PathBuf,
    watcher: Option<Watcher>,
    consumer: Option<JoinHandle<()>>,
    server: Option<JoinHandle<()>>,
    shutdown_tx: watch::Sender<bool>,
    _fx: Fixture,
}

impl LiveDaemon {
    async fn shutdown(mut self) {
        let _ = self.shutdown_tx.send(true);
        drop(self.watcher.take());
        if let Some(h) = self.consumer.take() {
            let _ = h.await;
        }
        if let Some(h) = self.server.take() {
            let _ = h.await;
        }
    }
}

async fn spawn_live_daemon(fx: Fixture) -> LiveDaemon {
    let client = EmbeddingClient::new(&fx.config.embedding).expect("build embedding client");
    let embedder: Arc<dyn Embedder> = Arc::new(client);
    spawn_live_daemon_with_embedder(fx, embedder).await
}

async fn spawn_live_daemon_with_embedder(fx: Fixture, embedder: Arc<dyn Embedder>) -> LiveDaemon {
    let store = Store::open(
        &fx.vault_id,
        &fx.data_dir,
        &fx.config.storage.index_file,
        &fx.config.embedding,
    )
    .await
    .expect("open store");
    let scanner =
        Scanner::new(&fx.vault, &fx.config, &store, embedder.clone()).expect("construct scanner");
    let _ = scanner.run().await.expect("initial scan");

    let ignores = fx
        .config
        .watcher
        .compiled_ignores()
        .expect("compile ignores");
    let (watcher, rx) = watcher::spawn_watcher(
        &fx.vault_id,
        &fx.vault,
        ignores,
        Duration::from_millis(fx.config.watcher.debounce_ms),
        256,
    )
    .expect("spawn watcher");

    let outbox_path =
        vault_data_dir(&fx.data_dir, &fx.vault_id).join(&fx.config.storage.outbox_file);
    let outbox = Outbox::open(fx.vault_id.clone(), outbox_path.clone())
        .await
        .expect("open outbox");
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let consumer = tokio::spawn(watcher::run_consumer(
        rx,
        scanner,
        outbox,
        shutdown_rx.clone(),
    ));

    let entry = VaultEntry {
        id: fx.vault_id.clone(),
        name: "test".to_string(),
        vault_path: fx.vault.clone(),
        outbox_path,
        store: Arc::new(store),
        status: VaultStatus::Active,
    };
    let state = ApiState {
        vault_manager: Arc::new(VaultManager::for_tests(
            vec![entry],
            embedder,
            fx.config.embedding.dimension,
        )),
    };
    let app = api::router(state);

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    let mut server_shutdown_rx = shutdown_rx;
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = server_shutdown_rx.wait_for(|v| *v).await;
            })
            .await;
    });

    LiveDaemon {
        base_url: format!("http://{addr}"),
        cfg_path: fx.cfg_path.clone(),
        data_dir: vault_data_dir(&fx.data_dir, &fx.vault_id),
        vault: fx.vault.clone(),
        watcher: Some(watcher),
        consumer: Some(consumer),
        server: Some(server),
        shutdown_tx,
        _fx: fx,
    }
}

// ===== Read-only query helpers =====

fn open_index(data_dir: &Path) -> Connection {
    let db_path = data_dir.join("index.sqlite");
    let conn = Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .expect("open index.sqlite read-only");
    let ext = EmbeddingConfig::default().resolved_extension_path();
    unsafe {
        conn.load_extension_enable()
            .expect("enable load_extension on read-only conn");
        conn.load_extension(&ext, Some("sqlite3_vec_init"))
            .expect("load sqlite-vec on read-only conn");
        conn.load_extension_disable()
            .expect("disable load_extension on read-only conn");
    }
    conn
}

fn count_chunks_for(data_dir: &Path, rel: &str) -> i64 {
    let conn = open_index(data_dir);
    conn.query_row(
        "SELECT count(*) FROM chunks WHERE file_path = ?1",
        params![rel],
        |r| r.get(0),
    )
    .expect("count chunks for path")
}

fn count_chunks_total(data_dir: &Path) -> i64 {
    let conn = open_index(data_dir);
    conn.query_row("SELECT count(*) FROM chunks", [], |r| r.get(0))
        .expect("count chunks total")
}

fn count_chunks_vec_total(data_dir: &Path) -> i64 {
    let conn = open_index(data_dir);
    conn.query_row("SELECT count(*) FROM chunks_vec", [], |r| r.get(0))
        .expect("count chunks_vec total")
}

fn read_chunk_created_ats(data_dir: &Path, rel: &str) -> Vec<String> {
    let conn = open_index(data_dir);
    let mut stmt = conn
        .prepare("SELECT created_at FROM chunks WHERE file_path = ?1 ORDER BY chunk_index")
        .expect("prepare created_at");
    let rows = stmt
        .query_map(params![rel], |r| r.get::<_, String>(0))
        .expect("query created_at");
    rows.map(|r| r.expect("created_at row")).collect()
}

fn read_chunk_hashes(data_dir: &Path, rel: &str) -> Vec<String> {
    let conn = open_index(data_dir);
    let mut stmt = conn
        .prepare("SELECT content_hash FROM chunks WHERE file_path = ?1 ORDER BY chunk_index")
        .expect("prepare content_hash");
    let rows = stmt
        .query_map(params![rel], |r| r.get::<_, String>(0))
        .expect("query content_hash");
    rows.map(|r| r.expect("content_hash row")).collect()
}

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("reqwest client")
}

// ===== Tests =====

#[tokio::test]
async fn editing_a_watched_file_writes_chunks_to_db() {
    let stub = StubServer::spawn(StubMode::Ok).await;
    let fx = fixture(&stub.url);
    let daemon = spawn_live_daemon(fx).await;

    fs::write(
        daemon.vault.join("note.md"),
        b"# Title\n\nA paragraph of body text.\n",
    )
    .expect("write note.md");
    tokio::time::sleep(SETTLE).await;

    let n = count_chunks_for(&daemon.data_dir, "note.md");
    assert!(n >= 1, "expected ≥1 chunks row for note.md, got {n}");

    daemon.shutdown().await;
    stub.shutdown().await;
}

#[tokio::test]
async fn chunk_count_matches_chunker_for_known_fixture() {
    let stub = StubServer::spawn(StubMode::Ok).await;
    let fx = fixture(&stub.url);
    let daemon = spawn_live_daemon(fx).await;

    // Three H2-separated sections — chunker emits one chunk per H2 boundary.
    let body = "## Section A\n\nBody A.\n\n## Section B\n\nBody B.\n\n## Section C\n\nBody C.\n";
    fs::write(daemon.vault.join("three.md"), body).expect("write three.md");
    tokio::time::sleep(SETTLE).await;

    let direct = chunk_file(body);
    assert_eq!(
        direct.len(),
        3,
        "fixture must produce exactly 3 chunks via chunk_file()"
    );
    let stored = count_chunks_for(&daemon.data_dir, "three.md");
    assert_eq!(
        stored, 3,
        "stored chunks count must equal direct chunk_file() output"
    );

    daemon.shutdown().await;
    stub.shutdown().await;
}

#[tokio::test]
async fn chunks_vec_row_per_chunks_row() {
    let stub = StubServer::spawn(StubMode::Ok).await;
    let fx = fixture(&stub.url);
    let daemon = spawn_live_daemon(fx).await;

    fs::write(
        daemon.vault.join("a.md"),
        b"## Alpha\n\nFirst.\n\n## Beta\n\nSecond.\n",
    )
    .expect("write a.md");
    fs::write(daemon.vault.join("b.md"), b"# Title\n\nBody.\n").expect("write b.md");
    tokio::time::sleep(SETTLE).await;

    let total = count_chunks_total(&daemon.data_dir);
    let vec_total = count_chunks_vec_total(&daemon.data_dir);
    assert!(
        total >= 3,
        "expected ≥3 chunks across both files, got {total}"
    );
    assert_eq!(
        total, vec_total,
        "chunks_vec must have one row per chunks row"
    );

    daemon.shutdown().await;
    stub.shutdown().await;
}

#[tokio::test]
async fn embedding_service_unavailable_skips_file_and_keeps_daemon_responsive() {
    let stub = StubServer::spawn(StubMode::Err503).await;
    let fx = fixture(&stub.url);
    let daemon = spawn_live_daemon(fx).await;

    fs::write(
        daemon.vault.join("doomed.md"),
        b"## Section\n\nThis file's embedding will fail.\n",
    )
    .expect("write doomed.md");
    tokio::time::sleep(SETTLE).await;

    assert_eq!(
        count_chunks_for(&daemon.data_dir, "doomed.md"),
        0,
        "embedding-service-down must skip file: chunks rows must be absent"
    );

    let body: Value = http_client()
        .get(format!("{}/health", daemon.base_url))
        .send()
        .await
        .expect("GET /health")
        .error_for_status()
        .expect("/health 2xx")
        .json()
        .await
        .expect("/health JSON");
    assert_eq!(body, json!({ "status": "ok" }));

    daemon.shutdown().await;
    stub.shutdown().await;
}

#[tokio::test]
async fn editing_existing_file_replaces_chunks() {
    let stub = StubServer::spawn(StubMode::Ok).await;
    let fx = fixture(&stub.url);
    let daemon = spawn_live_daemon(fx).await;

    let path = daemon.vault.join("note.md");
    fs::write(
        &path,
        b"## Alpha\n\nFirst section.\n\n## Beta\n\nSecond section.\n",
    )
    .expect("write v1");
    tokio::time::sleep(SETTLE).await;
    assert_eq!(count_chunks_for(&daemon.data_dir, "note.md"), 2);
    let original_hashes = read_chunk_hashes(&daemon.data_dir, "note.md");
    let original_created_ats = read_chunk_created_ats(&daemon.data_dir, "note.md");
    assert_eq!(original_hashes.len(), 2);
    assert_eq!(original_created_ats.len(), 2);

    fs::write(
        &path,
        b"## Alpha\n\nFirst section, expanded.\n\n\
          ## Beta\n\nSecond section, also expanded.\n\n\
          ## Gamma\n\nThird section, freshly added.\n",
    )
    .expect("write v2");
    tokio::time::sleep(SETTLE * 2).await;
    assert_eq!(count_chunks_for(&daemon.data_dir, "note.md"), 3);

    let new_hashes = read_chunk_hashes(&daemon.data_dir, "note.md");
    assert_eq!(new_hashes.len(), 3);
    for h in &original_hashes {
        assert!(
            !new_hashes.contains(h),
            "old chunk hash {h} must be gone after edit"
        );
    }
    let new_created_ats = read_chunk_created_ats(&daemon.data_dir, "note.md");
    assert_eq!(new_created_ats.len(), 3);
    for ts in &new_created_ats {
        assert!(
            !original_created_ats.contains(ts),
            "new chunk created_at {ts} must be fresh, not carried from v1"
        );
    }

    daemon.shutdown().await;
    stub.shutdown().await;
}

#[tokio::test]
async fn dimension_mismatch_at_startup_fails_loudly() {
    // No stub or daemon needed — `Store::open` is the surface under test.
    let root = tempfile::tempdir().expect("tempdir");
    let data_dir = root.path().join("data");

    // Default extension path so the dylib check passes; only `dimension`
    // diverges from what migration 0003 bakes (768).
    let cfg = EmbeddingConfig {
        dimension: 512,
        ..EmbeddingConfig::default()
    };

    let vault_id = VaultId::new();
    let err = match Store::open(&vault_id, &data_dir, "index.sqlite", &cfg).await {
        Ok(_) => panic!("Store::open must error on dimension mismatch"),
        Err(e) => e,
    };
    let msg = format!("{err:#}");
    assert!(
        msg.contains("512"),
        "error must mention config dim 512: {msg}"
    );
    assert!(
        msg.contains("768"),
        "error must mention schema dim 768: {msg}"
    );
    assert!(
        msg.to_lowercase().contains("adr-0007")
            || msg.to_lowercase().contains("re-index")
            || msg.to_lowercase().contains("delete"),
        "error must point at a resolution path: {msg}"
    );
}

#[tokio::test]
async fn embedding_service_returns_wrong_dimension_skips_file_and_keeps_daemon_responsive() {
    // Stub returns a 4-element vector when schema/config expects 768. Per
    // Task 6.4r1's contract (workplan directive 3), the indexer must
    // skip-and-log on `EmbeddingError::DimensionMismatch` and the daemon
    // must stay up.
    let stub = StubServer::spawn(StubMode::WrongDim(4)).await;
    let fx = fixture(&stub.url);
    let daemon = spawn_live_daemon(fx).await;

    fs::write(
        daemon.vault.join("wrong.md"),
        b"## Section\n\nServer returns wrong-dim vectors.\n",
    )
    .expect("write wrong.md");
    tokio::time::sleep(SETTLE).await;

    assert_eq!(
        count_chunks_for(&daemon.data_dir, "wrong.md"),
        0,
        "wrong-dim service response must skip file: chunks rows must be absent"
    );

    let body: Value = http_client()
        .get(format!("{}/health", daemon.base_url))
        .send()
        .await
        .expect("GET /health")
        .error_for_status()
        .expect("/health 2xx")
        .json()
        .await
        .expect("/health JSON");
    assert_eq!(body, json!({ "status": "ok" }));

    daemon.shutdown().await;
    stub.shutdown().await;
}

// ===== Semantic search integration tests (Task 7.5) =====

/// Test-only embedder that maps each input string to a deterministic, non-zero
/// unit vector via per-slot FNV-1a hashing. Used in lieu of the all-zero
/// `StubEmbedder` and the same-vector-for-every-input HTTP stub so kNN
/// ordering against varied chunk text is well-conditioned (cosine distance is
/// undefined for zero vectors and degenerate when every chunk shares the
/// query's vector).
struct DeterministicHashEmbedder {
    dimension: usize,
}

impl DeterministicHashEmbedder {
    fn new(dimension: usize) -> Self {
        Self { dimension }
    }
}

impl Embedder for DeterministicHashEmbedder {
    fn embed_text<'a>(&'a self, text: &'a str) -> EmbedFuture<'a> {
        let dim = self.dimension;
        let owned = text.to_string();
        Box::pin(async move {
            let bytes = owned.as_bytes();
            let mut v = vec![0.0_f32; dim];
            for (i, slot) in v.iter_mut().enumerate() {
                let mut h: u64 = 0xcbf29ce484222325 ^ (i as u64);
                for b in bytes {
                    h ^= *b as u64;
                    h = h.wrapping_mul(0x100000001b3);
                }
                *slot = ((h as f32) / (u64::MAX as f32)) - 0.5;
            }
            let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
            if norm > 0.0 {
                for x in &mut v {
                    *x /= norm;
                }
            }
            Ok(v)
        })
    }
}

fn open_index_rw(data_dir: &Path) -> Connection {
    let db_path = data_dir.join("index.sqlite");
    let conn = Connection::open(&db_path).expect("open index.sqlite read-write");
    let ext = EmbeddingConfig::default().resolved_extension_path();
    unsafe {
        conn.load_extension_enable()
            .expect("enable load_extension on rw conn");
        conn.load_extension(&ext, Some("sqlite3_vec_init"))
            .expect("load sqlite-vec on rw conn");
        conn.load_extension_disable()
            .expect("disable load_extension on rw conn");
    }
    conn
}

/// Wipe `chunks_vec` and `chunks` while leaving `files` populated. Reaches the
/// "empty index but vault has been seen" state needed for resolution B's hint
/// path. The workplan-literal recipe (configure stub to fail at index time)
/// does not produce this state — embedding failures during the initial scan
/// cause the indexer to skip the file row entirely, leaving `files` empty as
/// well. Forwarded from Task 7.3's smoke verification.
fn truncate_chunks_only(data_dir: &Path) {
    let conn = open_index_rw(data_dir);
    conn.execute("DELETE FROM chunks_vec", [])
        .expect("delete chunks_vec");
    conn.execute("DELETE FROM chunks", [])
        .expect("delete chunks");
}

fn count_files_total(data_dir: &Path) -> i64 {
    let conn = open_index(data_dir);
    conn.query_row("SELECT count(*) FROM files", [], |r| r.get(0))
        .expect("count files total")
}

#[tokio::test]
async fn semantic_search_returns_results_after_indexing() {
    // Custom embedder bypasses the HTTP stub entirely so kNN ordering
    // distinguishes chunks rather than collapsing to the all-same-vector
    // tie. The HTTP stub URL in the fixture is unused on this path.
    let fx = fixture("http://127.0.0.1:1/v1/embeddings");
    let embedder: Arc<dyn Embedder> = Arc::new(DeterministicHashEmbedder::new(SCHEMA_DIM));
    let daemon = spawn_live_daemon_with_embedder(fx, embedder).await;

    fs::write(
        daemon.vault.join("note.md"),
        b"## Section A\n\nAlpha body content.\n\n\
          ## Section B\n\nBeta body content.\n\n\
          ## Section C\n\nGamma body content.\n",
    )
    .expect("write note.md");
    tokio::time::sleep(SETTLE).await;
    assert!(
        count_chunks_total(&daemon.data_dir) >= 3,
        "expected ≥3 chunks indexed before query, got {}",
        count_chunks_total(&daemon.data_dir)
    );

    let body: Value = http_client()
        .post(format!("{}/search/semantic", daemon.base_url))
        .json(&json!({ "query": "alpha body", "limit": 5 }))
        .send()
        .await
        .expect("POST /search/semantic")
        .error_for_status()
        .expect("/search/semantic 2xx")
        .json()
        .await
        .expect("/search/semantic JSON");
    let results = body["results"].as_array().expect("results array");
    assert!(
        !results.is_empty(),
        "expected non-empty results from indexed vault, got {body}"
    );
    assert!(
        body.get("hint").map(|h| h.is_null()).unwrap_or(true),
        "no hint when results are non-empty, got {body}"
    );

    daemon.shutdown().await;
}

#[tokio::test]
async fn semantic_search_returns_hint_when_index_empty_after_files_seeded() {
    // Reach the hint state via the corrected recipe (Task 7.3 forward note):
    // (a) index normally so `files` and `chunks` populate, (b) truncate
    // `chunks_vec` + `chunks` while leaving `files`, (c) query and assert
    // the hint. The workplan literal ("stub down → no chunks land") does
    // not produce this state — see truncate_chunks_only doc comment.
    let stub = StubServer::spawn(StubMode::Ok).await;
    let fx = fixture(&stub.url);
    let daemon = spawn_live_daemon(fx).await;

    fs::write(
        daemon.vault.join("seeded.md"),
        b"## Section\n\nIndexed body.\n",
    )
    .expect("write seeded.md");
    tokio::time::sleep(SETTLE).await;
    assert!(
        count_chunks_total(&daemon.data_dir) > 0,
        "indexer must populate chunks before truncate; got 0"
    );
    assert!(
        count_files_total(&daemon.data_dir) > 0,
        "indexer must populate files before truncate; got 0"
    );

    truncate_chunks_only(&daemon.data_dir);
    assert_eq!(count_chunks_total(&daemon.data_dir), 0);
    assert_eq!(count_chunks_vec_total(&daemon.data_dir), 0);
    assert!(
        count_files_total(&daemon.data_dir) > 0,
        "files row must remain so the hint condition triggers"
    );

    let body: Value = http_client()
        .post(format!("{}/search/semantic", daemon.base_url))
        .json(&json!({ "query": "section" }))
        .send()
        .await
        .expect("POST /search/semantic")
        .error_for_status()
        .expect("/search/semantic 2xx")
        .json()
        .await
        .expect("/search/semantic JSON");
    assert_eq!(body["results"].as_array().unwrap().len(), 0);
    assert_eq!(body["hint"].as_str(), Some("semantic index is building"));

    daemon.shutdown().await;
    stub.shutdown().await;
}

#[tokio::test]
async fn semantic_search_returns_503_when_embedding_service_unavailable_at_query_time() {
    let stub = StubServer::spawn(StubMode::Ok).await;
    let fx = fixture(&stub.url);
    let daemon = spawn_live_daemon(fx).await;

    fs::write(
        daemon.vault.join("seeded.md"),
        b"## Section\n\nIndexed body.\n",
    )
    .expect("write seeded.md");
    tokio::time::sleep(SETTLE).await;
    assert!(
        count_chunks_total(&daemon.data_dir) > 0,
        "chunks must land before flipping the stub mode; got 0"
    );

    stub.set_mode(StubMode::Err503);

    let resp = http_client()
        .post(format!("{}/search/semantic", daemon.base_url))
        .json(&json!({ "query": "section" }))
        .send()
        .await
        .expect("POST /search/semantic");
    assert_eq!(
        resp.status(),
        503,
        "expected 503 when embedding service is down at query time"
    );
    let body: Value = resp.json().await.expect("error body JSON");
    assert_eq!(
        body["error"]["code"].as_str(),
        Some("embedding_unavailable"),
        "expected error.code = embedding_unavailable, got {body}"
    );

    daemon.shutdown().await;
    stub.shutdown().await;
}

#[tokio::test]
async fn semantic_search_against_wrong_dimension_at_query_time_returns_503() {
    let stub = StubServer::spawn(StubMode::Ok).await;
    let fx = fixture(&stub.url);
    let daemon = spawn_live_daemon(fx).await;

    fs::write(
        daemon.vault.join("seeded.md"),
        b"## Section\n\nIndexed body.\n",
    )
    .expect("write seeded.md");
    tokio::time::sleep(SETTLE).await;
    assert!(
        count_chunks_total(&daemon.data_dir) > 0,
        "chunks must land before flipping the stub mode; got 0"
    );

    stub.set_mode(StubMode::WrongDim(4));

    let resp = http_client()
        .post(format!("{}/search/semantic", daemon.base_url))
        .json(&json!({ "query": "section" }))
        .send()
        .await
        .expect("POST /search/semantic");
    assert_eq!(
        resp.status(),
        503,
        "expected 503 when embedding service returns wrong-dim at query time"
    );
    let body: Value = resp.json().await.expect("error body JSON");
    assert_eq!(
        body["error"]["code"].as_str(),
        Some("embedding_unavailable"),
        "expected error.code = embedding_unavailable, got {body}"
    );

    daemon.shutdown().await;
    stub.shutdown().await;
}

#[tokio::test]
async fn semantic_search_full_round_trip_via_hmn_binary() {
    // Mirrors `tests/cli.rs` precedent: drive the real `hmn` binary against a
    // live in-process daemon. Uses a custom embedder so the HTTP stub is not
    // a third moving part for the binary path; the HTTP-stub-mode-flip cases
    // already cover the network surface in tests above.
    let fx = fixture("http://127.0.0.1:1/v1/embeddings");
    let embedder: Arc<dyn Embedder> = Arc::new(DeterministicHashEmbedder::new(SCHEMA_DIM));
    let daemon = spawn_live_daemon_with_embedder(fx, embedder).await;

    fs::write(
        daemon.vault.join("note.md"),
        b"## Section A\n\nAlpha body content.\n\n\
          ## Section B\n\nBeta body content.\n",
    )
    .expect("write note.md");
    tokio::time::sleep(SETTLE).await;
    assert!(
        count_chunks_total(&daemon.data_dir) >= 2,
        "expected ≥2 chunks before binary invocation, got {}",
        count_chunks_total(&daemon.data_dir)
    );

    let cfg_path = daemon.cfg_path.clone();
    let base_url = daemon.base_url.clone();
    let out = tokio::task::spawn_blocking(move || {
        // The hmn binary is sync; run it on a blocking thread so the daemon's
        // tokio reactor remains free to serve the in-process HTTP request.
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_hmn"));
        cmd.arg("--config")
            .arg(&cfg_path)
            .arg("--daemon-url")
            .arg(&base_url)
            .arg("search")
            .arg("semantic")
            .arg("alpha");
        cmd.output().expect("run hmn search semantic")
    })
    .await
    .expect("spawn_blocking join");

    assert!(
        out.status.success(),
        "hmn exit={:?} stderr={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("note.md"),
        "stdout missing 'note.md': {stdout}"
    );
    assert!(
        stdout.contains("(score:"),
        "stdout missing '(score:' header: {stdout}"
    );

    daemon.shutdown().await;
}
