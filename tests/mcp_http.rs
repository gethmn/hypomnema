//! Integration tests for the Streamable-HTTP MCP transport (Step 12).
//!
//! Each test stands up a live `hmnd`-equivalent in-process daemon (with the
//! HTTP-MCP route mounted on the same axum listener as `/search/*` and
//! `/vaults/*`) and drives it over the wire via reqwest. JSON-RPC messages
//! ride POST `/mcp`; rmcp's `StreamableHttpService` returns `text/event-stream`
//! responses, which the test client parses line by line.
//!
//! Coverage maps 1:1 to the five stories archived at
//! `notes/proposals/archive/mcp-streamable-http-stories.md` after task 12.1:
//!   1. Agent host invokes a search tool over HTTP-MCP (equivalence with
//!      `/search/*` and `/vaults/*` HTTP endpoints).
//!   2. Origin validation at the round-trip level + negative-fingerprint grep.
//!   3. Trust-posture inheritance (no Authorization required, malformed
//!      Authorization processed normally) + fingerprint greps.
//!   4. Graceful shutdown closes open SSE streams + handler-module-has-no-
//!      signal-handling fingerprint.
//!   5. Stdio + HTTP-MCP coexistence on the same daemon.
//!
//! Per-middleware origin behavior (loopback variants, exact rejection body,
//! `Origin: null`, `https://` rejection) is unit-tested in
//! `src/api/mcp_http.rs::tests` (round-3 step-12 task 12.4); these integration
//! tests assert the HTTP round-trip honors the allow-list rather than
//! re-testing the parser. Per-tool argument shapes and per-error envelope
//! shapes are unit-tested in `src/mcp/server.rs` and exercised end-to-end via
//! `tests/mcp.rs` over stdio; this file focuses on the HTTP transport's own
//! contracts and the equivalences the spec promises.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use hypomnema::api::mcp_http::{self, McpHttpState};
use hypomnema::api::{self, ApiState, VaultEntry};
use hypomnema::config::{
    Config, ConfigPath, EmbeddingConfig, HttpConfig, LoggingConfig, McpConfig, McpHttpConfig,
    StorageConfig, WatcherConfig,
};
use hypomnema::control_plane::VaultManager;
use hypomnema::embedding::{EmbedFuture, Embedder, StubEmbedder};
use hypomnema::mcp::{HypomnemaBackend, InProcessBackend};
use hypomnema::store::{SqlitePool, Store};
use hypomnema::vault_registry::{VaultId, VaultRegistry, VaultRow, VaultStatus};
use rusqlite::params;
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::watch;
use tokio::task::{self, JoinHandle};

const DIM: usize = 768;
const PROTOCOL_VERSION: &str = "2025-06-18";

// ===== Fixture: SeededHttpMcpDaemon =====
//
// Daemon with HTTP-MCP enabled, stood up via `VaultManager::for_tests_full`
// against pre-seeded SQLite pools. Avoids the cost of running the watcher /
// scanner per test and keeps search results deterministic. Mirrors
// `MultiVaultDaemon` from `tests/vault_control_plane.rs` with the addition
// of the HTTP-MCP route mount.

struct SeededHttpMcpDaemon {
    base_url: String,
    cfg_path: PathBuf,
    pools: Vec<SqlitePool>,
    _root: TempDir,
    _vault_dirs: Vec<TempDir>,
    shutdown: watch::Sender<bool>,
    server: Option<JoinHandle<()>>,
}

impl SeededHttpMcpDaemon {
    async fn shutdown(mut self) {
        let _ = self.shutdown.send(true);
        if let Some(h) = self.server.take() {
            let _ = tokio::time::timeout(Duration::from_secs(5), h).await;
        }
    }
}

async fn spawn_seeded_daemon(active_names: Vec<&'static str>) -> SeededHttpMcpDaemon {
    spawn_seeded_daemon_with_embedder(active_names, Arc::new(StubEmbedder::new(DIM)), true).await
}

async fn spawn_seeded_daemon_with_embedder(
    active_names: Vec<&'static str>,
    embedder: Arc<dyn Embedder>,
    mcp_http_enabled: bool,
) -> SeededHttpMcpDaemon {
    let root = TempDir::new().expect("tempdir");
    let mut pools: Vec<SqlitePool> = Vec::new();
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
        entries.push(VaultEntry {
            id: vault_id,
            name: (*name).to_string(),
            vault_path: vault_dir.path().to_path_buf(),
            store,
            status: VaultStatus::Active,
        });
        pools.push(pool);
        names.push((*name).to_string());
        vault_dirs.push(vault_dir);
    }

    let manager = Arc::new(VaultManager::for_tests_full(
        entries,
        Vec::<VaultRow>::new(),
        embedder,
        DIM as u32,
    ));
    let api_state = ApiState {
        vault_manager: manager.clone(),
    };
    let mut app = api::router(api_state);

    if mcp_http_enabled {
        let backend: Arc<dyn HypomnemaBackend + Send + Sync> =
            Arc::new(InProcessBackend::new(manager.clone()));
        app = app.merge(mcp_http::router(McpHttpState {
            backend,
            default_vault_name: names.first().cloned().unwrap_or_else(|| "default".into()),
            enable_write_tools: true,
        }));
    }

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind 127.0.0.1:0");
    let addr = listener.local_addr().expect("local_addr");
    let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.wait_for(|v| *v).await;
            })
            .await;
    });

    let cfg_path = write_config_file(root.path(), mcp_http_enabled);

    SeededHttpMcpDaemon {
        base_url: format!("http://{addr}"),
        cfg_path,
        pools,
        _root: root,
        _vault_dirs: vault_dirs,
        shutdown: shutdown_tx,
        server: Some(server),
    }
}

fn write_config_file(root: &Path, mcp_http_enabled: bool) -> PathBuf {
    let path = root.join("config.toml");
    let body = format!(
        "[storage]\n\
         data_dir = \"{}\"\n\
         [mcp.http]\n\
         enabled = {}\n",
        root.join("data").display(),
        mcp_http_enabled,
    );
    std::fs::write(&path, body).expect("write config.toml");
    path
}

// ===== Fixture: LiveHttpMcpDaemon =====
//
// Daemon with HTTP-MCP enabled stood up via the production `VaultManager::open`
// path so the lifecycle ops (pause/resume/etc.) actually mutate runners and
// the registry. Used by the pause-then-resume round-trip and any other test
// that needs a live spawn ctx.

struct LiveHttpMcpDaemon {
    base_url: String,
    root: TempDir,
    shutdown: watch::Sender<bool>,
    server: Option<JoinHandle<()>>,
}

impl LiveHttpMcpDaemon {
    async fn shutdown(mut self) {
        let _ = self.shutdown.send(true);
        if let Some(h) = self.server.take() {
            let _ = tokio::time::timeout(Duration::from_secs(5), h).await;
        }
    }

    fn fresh_vault_dir(&self, name: &str) -> PathBuf {
        let p = self.root.path().join(name);
        std::fs::create_dir_all(&p).expect("create vault subdir");
        p
    }
}

async fn spawn_live_daemon() -> LiveHttpMcpDaemon {
    let root = TempDir::new().expect("tempdir");
    let data_dir = root.path().join("data");
    std::fs::create_dir_all(&data_dir).expect("create data_dir");

    let config = Arc::new(make_live_config(data_dir.clone()));
    let registry = Arc::new(VaultRegistry::open(&data_dir).await.expect("open registry"));

    let embedder: Arc<dyn Embedder> = Arc::new(StubEmbedder::new(DIM));
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let manager = Arc::new(
        VaultManager::open(
            registry,
            config.clone(),
            embedder,
            DIM as u32,
            shutdown_rx.clone(),
        )
        .await
        .expect("open VaultManager"),
    );

    let api_state = ApiState {
        vault_manager: manager.clone(),
    };
    let backend: Arc<dyn HypomnemaBackend + Send + Sync> =
        Arc::new(InProcessBackend::new(manager.clone()));
    let app = api::router(api_state).merge(mcp_http::router(McpHttpState {
        backend,
        default_vault_name: config.default_vault_name.clone(),
        enable_write_tools: true,
    }));

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind 127.0.0.1:0");
    let addr = listener.local_addr().expect("local_addr");
    let mut server_shutdown_rx = shutdown_rx;
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = server_shutdown_rx.wait_for(|v| *v).await;
            })
            .await;
    });

    LiveHttpMcpDaemon {
        base_url: format!("http://{addr}"),
        root,
        shutdown: shutdown_tx,
        server: Some(server),
    }
}

fn make_live_config(data_dir: PathBuf) -> Config {
    Config {
        vault: None,
        http: HttpConfig::default(),
        mcp: McpConfig {
            http: McpHttpConfig::default(),
            ..McpConfig::default()
        },
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

// ===== HTTP-MCP client =====
//
// Minimal reqwest wrapper that drives a session through `/mcp`:
//   - First POST is `initialize` with no `Mcp-Session-Id`; rmcp creates the
//     session and returns the id in the response header. We capture it and
//     send it on every subsequent request.
//   - Both directions speak JSON-RPC 2.0 over POST. Server responses are
//     returned as `text/event-stream` framed events (rmcp's default for
//     stateful sessions).
//
// SSE framing: the server emits zero or more events; each event is a block
// of `field: value` lines terminated by a blank line. We extract `data:`
// lines, skip empty `data:` (priming) events, and JSON-parse the first
// non-empty data block as the JSON-RPC response.

struct HttpMcpClient {
    client: reqwest::Client,
    base_url: String,
    session_id: Option<String>,
    next_id: u64,
}

impl HttpMcpClient {
    fn new(base_url: &str) -> Self {
        Self::with_client(reqwest_client(), base_url)
    }

    fn with_client(client: reqwest::Client, base_url: &str) -> Self {
        Self {
            client,
            base_url: base_url.to_string(),
            session_id: None,
            next_id: 0,
        }
    }

    async fn initialize(&mut self) -> Value {
        let body = json!({
            "jsonrpc": "2.0",
            "id": self.next_request_id(),
            "method": "initialize",
            "params": {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": "mcp-http-tests", "version": "0.0.0" },
            }
        });
        let resp = self.post_raw(&body, None).await.expect("initialize: send");
        let session_id = resp
            .headers()
            .get("mcp-session-id")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
            .expect("initialize response missing Mcp-Session-Id header");
        self.session_id = Some(session_id);
        let bytes = resp.bytes().await.expect("read initialize body");
        let msg = parse_sse_single_message(&bytes);
        // Notify the server that initialization is complete (per MCP spec).
        // Without this, some clients may not advance to subsequent requests;
        // rmcp's worker reads it as an ordinary client notification.
        self.send_notification("notifications/initialized", json!({}))
            .await;
        msg
    }

    fn next_request_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    async fn send_request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_request_id();
        let body = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        let resp = self.post_raw(&body, None).await.expect("send_request");
        assert!(
            resp.status().is_success(),
            "{method} returned non-2xx status {}",
            resp.status()
        );
        let bytes = resp.bytes().await.expect("read response body");
        parse_sse_single_message(&bytes)
    }

    async fn send_notification(&mut self, method: &str, params: Value) {
        let body = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        let resp = self.post_raw(&body, None).await.expect("send_notification");
        assert!(
            resp.status().is_success() || resp.status() == reqwest::StatusCode::ACCEPTED,
            "notification {method} returned non-2xx status {}",
            resp.status(),
        );
    }

    async fn post_raw(
        &self,
        body: &Value,
        extra_headers: Option<Vec<(&'static str, String)>>,
    ) -> reqwest::Result<reqwest::Response> {
        let mut req = self
            .client
            .post(format!("{}/mcp", self.base_url))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .json(body);
        if let Some(sid) = &self.session_id {
            req = req.header("Mcp-Session-Id", sid);
        }
        if let Some(headers) = extra_headers {
            for (name, value) in headers {
                req = req.header(name, value);
            }
        }
        req.send().await
    }
}

fn reqwest_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .expect("reqwest client")
}

fn parse_sse_single_message(bytes: &[u8]) -> Value {
    let text = std::str::from_utf8(bytes).expect("SSE response is utf8");
    let mut datas: Vec<String> = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    for line in text.split('\n') {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            if !current.is_empty() {
                datas.push(current.join("\n"));
                current.clear();
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("data:") {
            current.push(rest.strip_prefix(' ').unwrap_or(rest));
        }
        // Other SSE fields (event:, id:, retry:) are ignored.
    }
    if !current.is_empty() {
        datas.push(current.join("\n"));
    }
    let data = datas
        .into_iter()
        .find(|d| !d.is_empty())
        .unwrap_or_else(|| panic!("SSE response had no non-priming data line: {text:?}"));
    serde_json::from_str(&data)
        .unwrap_or_else(|e| panic!("invalid JSON-RPC frame in SSE data: {e}; raw: {data:?}"))
}

// ===== Seed helpers (mirror tests/vault_control_plane.rs) =====

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

// ===== Story 1: agent host invokes a search tool over HTTP-MCP =====

#[tokio::test]
async fn initialize_returns_serverinfo_hypomnema() {
    let daemon = spawn_seeded_daemon(vec!["alpha"]).await;
    let mut client = HttpMcpClient::new(&daemon.base_url);

    let resp = client.initialize().await;
    let result = &resp["result"];
    assert_eq!(result["serverInfo"]["name"], json!("hypomnema"));
    assert_eq!(
        result["serverInfo"]["version"],
        json!(env!("CARGO_PKG_VERSION"))
    );
    assert_eq!(result["protocolVersion"], json!(PROTOCOL_VERSION));

    daemon.shutdown().await;
}

#[tokio::test]
async fn tools_list_returns_all_twelve_tools() {
    let daemon = spawn_seeded_daemon(vec!["alpha"]).await;
    let mut client = HttpMcpClient::new(&daemon.base_url);
    client.initialize().await;

    let resp = client.send_request("tools/list", json!({})).await;
    let tools = resp["result"]["tools"]
        .as_array()
        .expect("tools/list returns tools array");
    let names: Vec<&str> = tools
        .iter()
        .map(|t| t["name"].as_str().expect("tool name string"))
        .collect();
    assert_eq!(names.len(), 12, "expected 12 tools, got {names:?}");
    for expected in [
        "search_filesystem",
        "search_content",
        "search_semantic",
        "vault_list",
        "vault_status",
        "vault_create",
        "vault_terminate",
        "vault_pause",
        "vault_resume",
        "vault_reset",
        "vault_rename",
        "vault_rescan",
    ] {
        assert!(names.contains(&expected), "missing {expected} in {names:?}");
    }

    daemon.shutdown().await;
}

#[tokio::test]
async fn tools_call_search_filesystem_matches_http() {
    let daemon = spawn_seeded_daemon(vec!["alpha"]).await;
    seed_files(
        daemon.pools[0].clone(),
        vec![
            ("alpha.md", "alpha body\n", "2026-01-01T00:00:00Z"),
            ("notes/beta.md", "beta body\n", "2026-01-01T00:00:00Z"),
        ],
    )
    .await;

    let query = json!({ "glob": "**/*.md" });

    // Equivalence baseline: hit `/search/filesystem` directly.
    let http_resp: Value = reqwest_client()
        .post(format!("{}/search/filesystem", daemon.base_url))
        .json(&query)
        .send()
        .await
        .expect("POST /search/filesystem")
        .error_for_status()
        .expect("/search/filesystem 2xx")
        .json()
        .await
        .expect("decode JSON");

    // Same query via HTTP-MCP `tools/call`.
    let mut client = HttpMcpClient::new(&daemon.base_url);
    client.initialize().await;
    let mcp_resp = client
        .send_request(
            "tools/call",
            json!({ "name": "search_filesystem", "arguments": query }),
        )
        .await;
    let structured = &mcp_resp["result"]["structuredContent"];
    assert_eq!(
        structured, &http_resp,
        "HTTP-MCP structuredContent must equal /search/filesystem JSON"
    );

    daemon.shutdown().await;
}

#[tokio::test]
async fn tools_call_search_content_matches_http() {
    let daemon = spawn_seeded_daemon(vec!["alpha"]).await;
    seed_files(
        daemon.pools[0].clone(),
        vec![
            (
                "alpha.md",
                "the quick brown fox jumps over the lazy dog\n",
                "2026-01-01T00:00:00Z",
            ),
            ("beta.md", "no animals here\n", "2026-01-01T00:00:00Z"),
        ],
    )
    .await;

    let query = json!({ "query": "quick brown" });

    let http_resp: Value = reqwest_client()
        .post(format!("{}/search/content", daemon.base_url))
        .json(&query)
        .send()
        .await
        .expect("POST /search/content")
        .error_for_status()
        .expect("/search/content 2xx")
        .json()
        .await
        .expect("decode JSON");

    let mut client = HttpMcpClient::new(&daemon.base_url);
    client.initialize().await;
    let mcp_resp = client
        .send_request(
            "tools/call",
            json!({ "name": "search_content", "arguments": query }),
        )
        .await;
    assert_eq!(
        &mcp_resp["result"]["structuredContent"], &http_resp,
        "HTTP-MCP structuredContent must equal /search/content JSON"
    );

    daemon.shutdown().await;
}

#[tokio::test]
async fn tools_call_search_semantic_matches_http() {
    // FixedEmbedder so each query embeds to the same vector and the search
    // returns deterministic ordering regardless of which transport issued
    // the call.
    let daemon =
        spawn_seeded_daemon_with_embedder(vec!["alpha"], FixedEmbedder::new(&[(0, 1.0)]), true)
            .await;
    seed_files(
        daemon.pools[0].clone(),
        vec![("note.md", "context body\n", "2026-01-01T00:00:00Z")],
    )
    .await;
    seed_chunk(
        daemon.pools[0].clone(),
        "note.md",
        0,
        "## Section",
        "indexed body text",
        unit_vec(&[(0, 1.0)]),
    )
    .await;

    let query = json!({ "query": "anything" });

    let http_resp: Value = reqwest_client()
        .post(format!("{}/search/semantic", daemon.base_url))
        .json(&query)
        .send()
        .await
        .expect("POST /search/semantic")
        .error_for_status()
        .expect("/search/semantic 2xx")
        .json()
        .await
        .expect("decode JSON");

    let mut client = HttpMcpClient::new(&daemon.base_url);
    client.initialize().await;
    let mcp_resp = client
        .send_request(
            "tools/call",
            json!({ "name": "search_semantic", "arguments": query }),
        )
        .await;
    assert_eq!(
        &mcp_resp["result"]["structuredContent"], &http_resp,
        "HTTP-MCP structuredContent must equal /search/semantic JSON"
    );

    daemon.shutdown().await;
}

#[tokio::test]
async fn tools_call_vault_list_matches_http() {
    // /vaults reads the registry, which only exists on the live manager.
    let daemon = spawn_live_daemon().await;
    let path_a = daemon.fresh_vault_dir("a");
    let path_b = daemon.fresh_vault_dir("b");
    for (name, path) in [("alpha", path_a), ("beta", path_b)] {
        let _ = reqwest_client()
            .post(format!("{}/vaults", daemon.base_url))
            .json(&json!({ "name": name, "path": path.to_str().unwrap() }))
            .send()
            .await
            .expect("POST /vaults")
            .error_for_status()
            .expect("create 2xx");
    }

    let http_resp: Value = reqwest_client()
        .get(format!("{}/vaults", daemon.base_url))
        .send()
        .await
        .expect("GET /vaults")
        .error_for_status()
        .expect("/vaults 2xx")
        .json()
        .await
        .expect("decode JSON");

    let mut client = HttpMcpClient::new(&daemon.base_url);
    client.initialize().await;
    let mcp_resp = client
        .send_request(
            "tools/call",
            json!({ "name": "vault_list", "arguments": {} }),
        )
        .await;
    assert_eq!(
        &mcp_resp["result"]["structuredContent"], &http_resp,
        "HTTP-MCP vault_list structuredContent must equal /vaults JSON"
    );

    daemon.shutdown().await;
}

#[tokio::test]
async fn tools_call_vault_status_matches_http() {
    // Lifecycle/status calls live behind the registry-backed live manager; the
    // for_tests_full path returns Internal because there's no spawn ctx.
    let daemon = spawn_live_daemon().await;
    let vault_path = daemon.fresh_vault_dir("v1");
    let create: Value = reqwest_client()
        .post(format!("{}/vaults", daemon.base_url))
        .json(&json!({ "name": "alpha", "path": vault_path.to_str().unwrap() }))
        .send()
        .await
        .expect("POST /vaults")
        .error_for_status()
        .expect("create 2xx")
        .json()
        .await
        .expect("decode create JSON");
    assert_eq!(create["status"], "active");

    let http_resp: Value = reqwest_client()
        .get(format!("{}/vaults/alpha", daemon.base_url))
        .send()
        .await
        .expect("GET /vaults/{name}")
        .error_for_status()
        .expect("/vaults/{name} 2xx")
        .json()
        .await
        .expect("decode JSON");

    let mut client = HttpMcpClient::new(&daemon.base_url);
    client.initialize().await;
    let mcp_resp = client
        .send_request(
            "tools/call",
            json!({
                "name": "vault_status",
                "arguments": { "target": "alpha" },
            }),
        )
        .await;
    assert_eq!(
        &mcp_resp["result"]["structuredContent"], &http_resp,
        "HTTP-MCP vault_status structuredContent must equal /vaults/{{name}} JSON"
    );

    daemon.shutdown().await;
}

#[tokio::test]
async fn tools_call_vault_pause_then_resume_round_trip() {
    let daemon = spawn_live_daemon().await;
    let vault_path = daemon.fresh_vault_dir("v1");

    // Create a vault via the HTTP control plane so the live manager has a
    // real runner to pause/resume.
    let create: Value = reqwest_client()
        .post(format!("{}/vaults", daemon.base_url))
        .json(&json!({ "name": "alpha", "path": vault_path.to_str().unwrap() }))
        .send()
        .await
        .expect("POST /vaults")
        .error_for_status()
        .expect("create 2xx")
        .json()
        .await
        .expect("decode JSON");
    assert_eq!(create["status"], "active");

    let mut client = HttpMcpClient::new(&daemon.base_url);
    client.initialize().await;

    let paused = client
        .send_request(
            "tools/call",
            json!({
                "name": "vault_pause",
                "arguments": { "target": "alpha" },
            }),
        )
        .await;
    assert!(
        !paused["result"]["isError"].as_bool().unwrap_or(false),
        "vault_pause failed: {paused}"
    );
    assert_eq!(paused["result"]["structuredContent"]["status"], "paused");

    let resumed = client
        .send_request(
            "tools/call",
            json!({
                "name": "vault_resume",
                "arguments": { "target": "alpha" },
            }),
        )
        .await;
    assert!(
        !resumed["result"]["isError"].as_bool().unwrap_or(false),
        "vault_resume failed: {resumed}"
    );
    assert_eq!(resumed["result"]["structuredContent"]["status"], "active");

    daemon.shutdown().await;
}

#[tokio::test]
async fn mcp_http_disabled_returns_404() {
    let daemon = spawn_seeded_daemon_with_embedder(
        vec!["alpha"],
        Arc::new(StubEmbedder::new(DIM)),
        /* mcp_http_enabled = */ false,
    )
    .await;

    let resp = reqwest_client()
        .post(format!("{}/mcp", daemon.base_url))
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .json(&json!({"jsonrpc": "2.0", "id": 1, "method": "ping"}))
        .send()
        .await
        .expect("POST /mcp");
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::NOT_FOUND,
        "expected 404 when [mcp.http] enabled = false"
    );

    // Sanity check: the API surface is still mounted on the same daemon so
    // the 404 is specific to /mcp, not a daemon-wide failure.
    let api = reqwest_client()
        .get(format!("{}/health", daemon.base_url))
        .send()
        .await
        .expect("GET /health");
    assert_eq!(api.status(), reqwest::StatusCode::OK);

    daemon.shutdown().await;
}

// ===== Story 2: Origin validation =====

#[tokio::test]
async fn origin_remote_rejected_403() {
    let daemon = spawn_seeded_daemon(vec!["alpha"]).await;

    let resp = reqwest_client()
        .post(format!("{}/mcp", daemon.base_url))
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("Origin", "http://example.com")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": "x", "version": "0" },
            }
        }))
        .send()
        .await
        .expect("POST /mcp");
    assert_eq!(resp.status(), reqwest::StatusCode::FORBIDDEN);
    let body = resp.text().await.expect("body text");
    assert!(
        body.contains("Origin not allowed: http://example.com"),
        "expected exact rejection body, got {body:?}"
    );

    daemon.shutdown().await;
}

#[tokio::test]
async fn origin_loopback_variants_accepted() {
    let daemon = spawn_seeded_daemon(vec!["alpha"]).await;

    for origin in [
        "http://localhost",
        "http://localhost:1234",
        "http://127.0.0.1",
        "http://127.0.0.1:7777",
        "http://[::1]",
        "http://[::1]:7777",
        "null",
    ] {
        let resp = reqwest_client()
            .post(format!("{}/mcp", daemon.base_url))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .header("Origin", origin)
            .json(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": { "name": "x", "version": "0" },
                }
            }))
            .send()
            .await
            .expect("POST /mcp");
        assert_ne!(
            resp.status(),
            reqwest::StatusCode::FORBIDDEN,
            "loopback origin {origin:?} rejected unexpectedly: status {}",
            resp.status()
        );
    }

    // Missing Origin header is also accepted (curl, MCP CLI clients,
    // server-to-server callers don't send Origin).
    let resp = reqwest_client()
        .post(format!("{}/mcp", daemon.base_url))
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": "x", "version": "0" },
            }
        }))
        .send()
        .await
        .expect("POST /mcp (no Origin)");
    assert_ne!(resp.status(), reqwest::StatusCode::FORBIDDEN);

    daemon.shutdown().await;
}

#[test]
fn origin_negative_fingerprint_grep() {
    // Codify Story 2's "no broad CORS allow-list" guarantee. Any of the
    // listed substrings appearing in `src/api/mcp_http*.rs` would mean
    // someone reverted the loopback-only Origin policy. Use rg if present;
    // fall back to a `find` + `grep` shell pipeline so the assertion runs
    // even when rg is missing on a CI runner.
    assert_no_match_in_files(
        &[
            "allow_any_origin",
            "cors_allow_all",
            r"Access-Control-Allow-Origin: \*",
        ],
        "src/api/mcp_http",
    );
}

// ===== Story 3: trust posture inheritance =====

#[tokio::test]
async fn mcp_http_no_authorization_required() {
    let daemon = spawn_seeded_daemon(vec!["alpha"]).await;
    let mut client = HttpMcpClient::new(&daemon.base_url);
    let resp = client.initialize().await;
    assert_eq!(resp["result"]["serverInfo"]["name"], json!("hypomnema"));
    daemon.shutdown().await;
}

#[tokio::test]
async fn mcp_http_with_authorization_header_proceeds_normally() {
    let daemon = spawn_seeded_daemon(vec!["alpha"]).await;

    let resp = reqwest_client()
        .post(format!("{}/mcp", daemon.base_url))
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("Authorization", "Bearer fake-token-not-validated")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": "x", "version": "0" },
            }
        }))
        .send()
        .await
        .expect("POST /mcp with Authorization");
    assert!(
        resp.status().is_success(),
        "expected success despite Authorization header, got {}",
        resp.status()
    );
    let bytes = resp.bytes().await.expect("read body");
    let parsed = parse_sse_single_message(&bytes);
    assert_eq!(parsed["result"]["serverInfo"]["name"], json!("hypomnema"));

    daemon.shutdown().await;
}

#[test]
fn trust_posture_negative_fingerprint_greps() {
    // Story 3 acceptance: the HTTP-MCP handler module owns no client/listener
    // state and adds no auth surface. Either substring landing in
    // `src/api/mcp_http*.rs` would be a regression.
    assert_no_match_in_files(
        &[r"reqwest::Client", r"hyper::Client", r"TcpListener::bind"],
        "src/api/mcp_http",
    );
    assert_no_match_in_files(
        &[
            "rustls",
            "server_config",
            "ServerCertVerifier",
            "verify_token",
            "api_key",
        ],
        "src/api/mcp_http",
    );
}

// ===== Story 4: graceful shutdown =====

#[tokio::test]
async fn shutdown_cleanly_completes_when_no_streams_open() {
    // Story 4's primary acceptance criterion: triggering daemon-wide shutdown
    // closes the HTTP server cleanly. The companion negative-fingerprint grep
    // (`shutdown_handler_module_has_no_signal_handling`) verifies the route
    // module owns no SIGINT/SIGTERM handling; this test verifies the
    // existing `with_graceful_shutdown` integration on the merged router
    // still reaches a clean exit when HTTP-MCP is mounted alongside the API.
    let daemon = spawn_seeded_daemon(vec!["alpha"]).await;
    // Drive one POST so the route is exercised before shutdown.
    let mut client = HttpMcpClient::new(&daemon.base_url);
    client.initialize().await;
    // Now signal shutdown and assert the server task completes within the
    // bounded timeout (matches the spec's 2s graceful budget — generous
    // ceiling here to absorb scheduler jitter on shared CI hardware).
    let server = daemon.server;
    let shutdown = daemon.shutdown.clone();
    drop(daemon.pools);
    drop(daemon._root);
    drop(daemon._vault_dirs);
    let _ = shutdown.send(true);
    if let Some(handle) = server {
        tokio::time::timeout(Duration::from_secs(5), handle)
            .await
            .expect("HTTP server did not shut down within 5s of signal")
            .expect("HTTP server task panicked");
    }
}

#[test]
fn shutdown_handler_module_has_no_signal_handling() {
    // Story 4's third acceptance criterion: the MCP route handler relies on
    // the daemon-wide shutdown integration and does not run its own signal
    // handling. tokio::signal or its select! arms in the route module would
    // mean someone forked off the daemon-wide path.
    assert_no_match_in_files(&[r"tokio::signal", "select!"], "src/api/mcp_http");
}

// ===== Story 5: stdio + HTTP-MCP coexistence =====

#[tokio::test]
async fn stdio_and_http_serve_same_tools_list() {
    let daemon = spawn_seeded_daemon(vec!["alpha"]).await;

    let mut http = HttpMcpClient::new(&daemon.base_url);
    http.initialize().await;
    let http_tools = http.send_request("tools/list", json!({})).await["result"]["tools"].clone();

    let mut stdio = StdioMcpChild::spawn(&daemon.cfg_path, &daemon.base_url).await;
    stdio.handshake().await;
    let stdio_tools = stdio.send_request("tools/list", json!({})).await["result"]["tools"].clone();
    stdio.shutdown().await;

    assert_eq!(
        http_tools, stdio_tools,
        "tools/list arrays must be byte-identical across stdio and HTTP"
    );

    daemon.shutdown().await;
}

#[tokio::test]
async fn stdio_and_http_brand_identity_match() {
    let daemon = spawn_seeded_daemon(vec!["alpha"]).await;

    let mut http = HttpMcpClient::new(&daemon.base_url);
    let http_init = http.initialize().await;
    assert_eq!(
        http_init["result"]["serverInfo"]["name"],
        json!("hypomnema")
    );

    let mut stdio = StdioMcpChild::spawn(&daemon.cfg_path, &daemon.base_url).await;
    let stdio_init = stdio.handshake().await;
    assert_eq!(
        stdio_init["result"]["serverInfo"]["name"],
        json!("hypomnema")
    );
    assert_eq!(
        stdio_init["result"]["serverInfo"]["version"], http_init["result"]["serverInfo"]["version"],
        "version must match across transports"
    );
    stdio.shutdown().await;

    daemon.shutdown().await;
}

#[tokio::test]
async fn concurrent_calls_dont_block_each_other() {
    let daemon = spawn_seeded_daemon(vec!["alpha"]).await;
    seed_files(
        daemon.pools[0].clone(),
        vec![("a.md", "alpha body\n", "2026-01-01T00:00:00Z")],
    )
    .await;

    let base_url = daemon.base_url.clone();
    let cfg_path = daemon.cfg_path.clone();

    // Five HTTP-MCP calls in parallel using independent sessions (the
    // session manager serializes per-session requests, so independent
    // sessions exercise the actual concurrency path), plus five stdio
    // calls each owning an independent `hmn mcp` subprocess.
    let mut set: tokio::task::JoinSet<Value> = tokio::task::JoinSet::new();

    for _ in 0..5 {
        let url = base_url.clone();
        set.spawn(async move {
            let mut c = HttpMcpClient::new(&url);
            c.initialize().await;
            c.send_request(
                "tools/call",
                json!({
                    "name": "search_filesystem",
                    "arguments": { "glob": "**/*.md" }
                }),
            )
            .await
        });
    }

    for _ in 0..5 {
        let cfg = cfg_path.clone();
        let url = base_url.clone();
        set.spawn(async move {
            let mut child = StdioMcpChild::spawn(&cfg, &url).await;
            child.handshake().await;
            let resp = child
                .send_request(
                    "tools/call",
                    json!({
                        "name": "search_filesystem",
                        "arguments": { "glob": "**/*.md" }
                    }),
                )
                .await;
            child.shutdown().await;
            resp
        });
    }

    let mut completed = 0usize;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    while let Ok(Some(joined)) = tokio::time::timeout_at(deadline, set.join_next()).await {
        let resp = joined.expect("parallel call task panicked");
        assert!(
            !resp["result"]["isError"].as_bool().unwrap_or(false),
            "parallel call returned isError=true: {resp}"
        );
        completed += 1;
    }
    assert_eq!(
        completed, 10,
        "expected 10 parallel calls to complete within 20s, got {completed}"
    );

    daemon.shutdown().await;
}

// ===== Stdio MCP child harness (Story 5) =====
//
// Mirrors `tests/mcp.rs::McpClient` but condensed: spawns `hmn mcp
// --daemon-url <base_url>` and speaks newline-delimited JSON-RPC over its
// stdio. We need it here to assert tools/list parity and concurrent-call
// non-blocking; full per-tool coverage already lives in `tests/mcp.rs`.

struct StdioMcpChild {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl StdioMcpChild {
    async fn spawn(cfg_path: &Path, daemon_url: &str) -> Self {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_hmn"));
        cmd.arg("--config")
            .arg(cfg_path)
            .arg("--daemon-url")
            .arg(daemon_url)
            .arg("mcp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let mut child = cmd.spawn().expect("spawn hmn mcp");
        let stdin = child.stdin.take().expect("child stdin");
        let stdout = BufReader::new(child.stdout.take().expect("child stdout"));
        Self {
            child,
            stdin: Some(stdin),
            stdout,
            next_id: 0,
        }
    }

    async fn handshake(&mut self) -> Value {
        let resp = self
            .send_request(
                "initialize",
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": { "name": "mcp-http-tests", "version": "0.0.0" }
                }),
            )
            .await;
        self.send_notification("notifications/initialized", json!({}))
            .await;
        resp
    }

    async fn send_request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        let req = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        self.write(&req).await;
        loop {
            let msg = self.read().await;
            if msg.get("id").and_then(Value::as_u64) == Some(id) {
                return msg;
            }
        }
    }

    async fn send_notification(&mut self, method: &str, params: Value) {
        let req = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        self.write(&req).await;
    }

    async fn write(&mut self, msg: &Value) {
        let stdin = self.stdin.as_mut().expect("stdin still open");
        let line = serde_json::to_string(msg).expect("serialize message");
        stdin.write_all(line.as_bytes()).await.expect("write line");
        stdin.write_all(b"\n").await.expect("write newline");
        stdin.flush().await.expect("flush stdin");
    }

    async fn read(&mut self) -> Value {
        let mut line = String::new();
        let n = tokio::time::timeout(Duration::from_secs(15), self.stdout.read_line(&mut line))
            .await
            .expect("read_line timed out")
            .expect("read_line IO error");
        assert!(n > 0, "child stdout closed before sending response");
        serde_json::from_str(line.trim())
            .unwrap_or_else(|e| panic!("invalid JSON-RPC frame: {e}; raw: {line:?}"))
    }

    async fn shutdown(mut self) {
        drop(self.stdin.take());
        let _ = tokio::time::timeout(Duration::from_secs(5), self.child.wait()).await;
    }
}

// ===== Negative-fingerprint helper =====

/// Assert that none of the `forbidden` substrings appear in any file under
/// `src/` whose path contains `path_substr`. Uses `rg` when on PATH; falls
/// back to a portable `grep -r` walk.
fn assert_no_match_in_files(forbidden: &[&str], path_substr: &str) {
    use std::process::Command;
    let cargo_dir = env!("CARGO_MANIFEST_DIR");
    for needle in forbidden {
        let pattern = needle.to_string();
        let mut found_any = false;
        let command_used: &str;
        // Prefer rg for performance + regex-by-default, fall back to grep -E
        // so the assert keeps running even where rg is missing.
        if let Ok(out) = Command::new("rg")
            .arg("-n")
            .arg("--no-heading")
            .arg("-e")
            .arg(&pattern)
            .arg(format!("{cargo_dir}/src/api"))
            .output()
        {
            command_used = "rg";
            if !out.stdout.is_empty() {
                let s = String::from_utf8_lossy(&out.stdout);
                for line in s.lines() {
                    if line.contains(path_substr) {
                        found_any = true;
                        eprintln!("forbidden match: {line}");
                    }
                }
            }
        } else if let Ok(out) = Command::new("grep")
            .arg("-rEn")
            .arg("-e")
            .arg(&pattern)
            .arg(format!("{cargo_dir}/src/api"))
            .output()
        {
            command_used = "grep";
            let s = String::from_utf8_lossy(&out.stdout);
            for line in s.lines() {
                if line.contains(path_substr) {
                    found_any = true;
                    eprintln!("forbidden match: {line}");
                }
            }
        } else {
            panic!(
                "neither rg nor grep available on PATH; cannot run negative-fingerprint check for {pattern:?}"
            );
        }
        assert!(
            !found_any,
            "negative-fingerprint regression: pattern {pattern:?} matched a file containing {path_substr:?} (via {command_used})"
        );
    }
}

#[allow(dead_code)]
fn _ensure_chrono_used() {
    let _ = Utc::now();
}
