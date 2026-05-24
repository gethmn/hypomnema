//! CLI ↔ MCP parity (#330).
//!
//! Proves that `hmn --json <cmd>` stdout is byte-for-byte equal (as parsed
//! JSON) to the `structuredContent` returned by the stdio MCP server
//! (`hmn mcp`) for the *same logical request*, across every non-streaming tool
//! surface: filesystem search, content search (default + opt-in snippets),
//! semantic search (chunk + document), `content_get`, `vault_list`, and
//! `vault_status`.
//!
//! ## Why compare CLI against *stdio* MCP (one transport)
//!
//! HTTP-vs-MCP-HTTP `structuredContent` parity is already covered elsewhere —
//! `tests/mcp_http.rs` (`*_parity` tests for filesystem/content/semantic/
//! vault_list/vault_status), `tests/semantic_smoke.rs`
//! (`smoke_transport_parity_http_and_mcp`), and `tests/content_get.rs`
//! (`content_get_transport_parity_http_vs_mcp_backend`). Those prove the HTTP
//! API body, the HTTP-MCP `structuredContent`, and the in-process MCP backend
//! all agree, because every surface serializes the same `src/api/types.rs`
//! response types.
//!
//! The gap this file closes is the **CLI** surface. `hmn --json` and `hmn mcp`
//! are the same binary talking to the same daemon over HTTP, so comparing them
//! catches CLI-side *request-construction* drift that response-shape tests
//! cannot — specifically the Finding-1 regression where the CLI hardcoded
//! `include_matches: true` while a default MCP `search_content` leaves it
//! `false`. Comparing against stdio MCP also exercises the stdio transport's
//! `structuredContent`, which the HTTP parity tests above do not touch.
//! Together, CLI↔stdio-MCP here + HTTP↔MCP-HTTP elsewhere cover all four
//! surfaces, so one transport is enough in this file.

use std::path::{Path, PathBuf};
use std::process::{Command as StdCommand, Stdio};
use std::sync::Arc;
use std::time::Duration;

use hypomnema::api::{self, ApiState, VaultEntry};
use hypomnema::config::{Config, SemanticSearchConfig};
use hypomnema::control_plane::VaultManager;
use hypomnema::embedding::{EmbedFuture, Embedder, StubEmbedder};
use hypomnema::indexer::Scanner;
use hypomnema::store::Store;
use hypomnema::vault_registry::{VaultId, VaultRegistry, VaultStatus};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::process::{Child, ChildStdin, ChildStdout, Command as TokioCommand};
use tokio::sync::watch;
use tokio::task::JoinHandle;

const READ_TIMEOUT: Duration = Duration::from_secs(10);
const EXIT_TIMEOUT: Duration = Duration::from_secs(5);
const PROTOCOL_VERSION: &str = "2025-06-18";

// ===== Embedder =====
//
// `StubEmbedder` returns all-zero vectors, which are degenerate for cosine
// kNN. Use a fixed non-zero unit vector instead (mirrors
// `tests/semantic_smoke.rs::FixedEmbedder`): every chunk and the query embed
// to the same vector, so semantic results are non-empty and deterministic —
// which is all parity needs.
struct FixedEmbedder {
    vector: Vec<f32>,
}

impl FixedEmbedder {
    fn unit(dim: usize) -> Arc<Self> {
        let mut v = vec![0.0f32; dim];
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

// ===== Fixture + daemon (adapted from tests/cli.rs) =====

struct Fixture {
    _root: TempDir,
    vault: PathBuf,
    data_dir: PathBuf,
    cfg_path: PathBuf,
    config: Config,
    vault_id: VaultId,
}

fn fixture() -> Fixture {
    let root = tempfile::tempdir().expect("create root tempdir");
    let vault = root.path().join("vault");
    let data_dir = root.path().join("data");
    std::fs::create_dir_all(&vault).expect("create vault dir");

    let cfg_path = root.path().join("config.toml");
    std::fs::write(
        &cfg_path,
        format!(
            "vault = \"{}\"\n[storage]\ndata_dir = \"{}\"\n",
            vault.display(),
            data_dir.display(),
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

fn write_file(fx: &Fixture, rel: &str, body: &[u8]) {
    let path = fx.vault.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create parent dirs");
    }
    std::fs::write(&path, body).expect("write fixture file");
}

/// Seed a vault that exercises every parity surface: globbable paths
/// (filesystem), a file with a repeated substring across lines (content +
/// snippets), and chunked content for semantic search.
fn seed_vault(fx: &Fixture) {
    write_file(
        fx,
        "alpha.md",
        b"# Alpha\nThe quick brown fox jumps over the lazy dog.\n",
    );
    write_file(
        fx,
        "notes/beta.md",
        b"# Beta\nvector databases enable semantic search\nvector embeddings power retrieval\n",
    );
    write_file(
        fx,
        "notes/gamma.md",
        b"# Gamma\nDebouncing filesystem events avoids reindex storms.\n",
    );
}

struct LiveDaemon {
    base_url: String,
    cfg_path: PathBuf,
    shutdown: watch::Sender<bool>,
    handle: Option<JoinHandle<()>>,
    _fx: Fixture,
}

impl LiveDaemon {
    async fn shutdown(mut self) {
        let _ = self.shutdown.send(true);
        if let Some(h) = self.handle.take() {
            let _ = h.await;
        }
    }
}

async fn spawn_live_daemon(fx: Fixture) -> LiveDaemon {
    let store = Store::open(
        &fx.vault_id,
        &fx.data_dir,
        &fx.config.storage.index_file,
        &fx.config.embedding,
    )
    .await
    .expect("open store");
    let embedder: Arc<dyn Embedder> = FixedEmbedder::unit(fx.config.embedding.dimension as usize);
    let scanner =
        Scanner::new(&fx.vault, &fx.config, &store, embedder.clone()).expect("construct scanner");
    let _ = scanner.run().await.expect("initial scan");

    let entry = VaultEntry {
        id: fx.vault_id.clone(),
        name: "test".to_string(),
        vault_path: fx.vault.clone(),
        store: Arc::new(store),
        status: VaultStatus::Active,
        bootstrap_state: hypomnema::api::BootstrapState::ready_state(),
    };
    let manager = Arc::new(VaultManager::for_tests(
        vec![entry],
        embedder,
        fx.config.embedding.dimension,
    ));
    let state = ApiState {
        vault_manager: manager.clone(),
        event_bus: manager.event_bus(),
        started_at: std::time::Instant::now(),
        embedding_endpoint: None,
        semantic_config: SemanticSearchConfig::default(),
    };
    let app = api::router(state);

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind 127.0.0.1:0");
    let addr = listener.local_addr().expect("local_addr");
    let (tx, mut rx) = watch::channel(false);
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = rx.wait_for(|v| *v).await;
            })
            .await;
    });

    LiveDaemon {
        base_url: format!("http://{addr}"),
        cfg_path: fx.cfg_path.clone(),
        shutdown: tx,
        handle: Some(handle),
        _fx: fx,
    }
}

// ===== CLI invocation =====

/// Run `hmn --config <cfg> --daemon-url <url> --json <args...>`, assert
/// success, and parse stdout as JSON. The synchronous `hmn` subprocess runs in
/// `spawn_blocking` so the in-process daemon's reactor keeps driving.
async fn run_hmn_json(cfg_path: &Path, daemon_url: &str, args: &[&str]) -> Value {
    let cfg_path = cfg_path.to_path_buf();
    let daemon_url = daemon_url.to_string();
    let owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let out = tokio::task::spawn_blocking(move || {
        let mut cmd = StdCommand::new(env!("CARGO_BIN_EXE_hmn"));
        cmd.arg("--config")
            .arg(&cfg_path)
            .arg("--daemon-url")
            .arg(&daemon_url)
            .arg("--json");
        cmd.args(&owned);
        cmd.output().expect("run hmn")
    })
    .await
    .expect("spawn_blocking join");

    assert!(
        out.status.success(),
        "hmn --json {args:?} exit={:?} stderr={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "hmn --json {args:?} stdout is not JSON: {e}; raw={}",
            String::from_utf8_lossy(&out.stdout)
        )
    })
}

// ===== stdio MCP client (adapted from tests/mcp.rs) =====

struct McpClient {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl McpClient {
    async fn spawn(cfg_path: &Path, daemon_url: &str) -> Self {
        let mut cmd = TokioCommand::new(env!("CARGO_BIN_EXE_hmn"));
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
        let mut client = Self {
            child,
            stdin: Some(stdin),
            stdout,
            next_id: 0,
        };
        client.handshake().await;
        client
    }

    async fn send_request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        let req = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        self.write_message(&req).await;
        loop {
            let msg = self.read_message().await;
            if msg.get("id").and_then(Value::as_u64) == Some(id) {
                return msg;
            }
        }
    }

    async fn send_notification(&mut self, method: &str, params: Value) {
        let req = json!({ "jsonrpc": "2.0", "method": method, "params": params });
        self.write_message(&req).await;
    }

    async fn write_message(&mut self, msg: &Value) {
        let stdin = self.stdin.as_mut().expect("stdin still open");
        let line = serde_json::to_string(msg).expect("serialize JSON-RPC message");
        stdin
            .write_all(line.as_bytes())
            .await
            .expect("write JSON-RPC line");
        stdin.write_all(b"\n").await.expect("write framing newline");
        stdin.flush().await.expect("flush stdin");
    }

    async fn read_message(&mut self) -> Value {
        let mut line = String::new();
        let n = tokio::time::timeout(READ_TIMEOUT, self.stdout.read_line(&mut line))
            .await
            .expect("read_line timed out waiting for MCP response")
            .expect("read_line IO error");
        assert!(n > 0, "child stdout closed before sending response");
        serde_json::from_str(line.trim())
            .unwrap_or_else(|e| panic!("invalid JSON-RPC frame from child: {e}; raw: {line:?}"))
    }

    async fn handshake(&mut self) {
        self.send_request(
            "initialize",
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": "json-parity-tests", "version": "0.0.0" }
            }),
        )
        .await;
        self.send_notification("notifications/initialized", json!({}))
            .await;
    }

    /// Call a tool and return its `structuredContent`, asserting non-error.
    async fn structured(&mut self, name: &str, arguments: Value) -> Value {
        let resp = self
            .send_request(
                "tools/call",
                json!({ "name": name, "arguments": arguments }),
            )
            .await;
        let result = &resp["result"];
        assert!(
            !result["isError"].as_bool().unwrap_or(false),
            "MCP tool `{name}` returned an error: {result}"
        );
        result["structuredContent"].clone()
    }

    async fn shutdown(mut self) {
        drop(self.stdin.take());
        let status = tokio::time::timeout(EXIT_TIMEOUT, self.child.wait())
            .await
            .expect("hmn mcp child did not exit within EXIT_TIMEOUT")
            .expect("hmn mcp child wait failed");
        assert!(
            status.success(),
            "hmn mcp child exited non-zero: {status:?}"
        );
    }
}

// ===== Parity helper =====

/// Run the same logical request over `hmn --json` and the MCP tool, assert the
/// two JSON payloads are equal, and return the (shared) value for further
/// schema assertions.
async fn assert_parity(
    cfg_path: &Path,
    base_url: &str,
    client: &mut McpClient,
    cli_args: &[&str],
    tool: &str,
    mcp_args: Value,
) -> Value {
    let cli = run_hmn_json(cfg_path, base_url, cli_args).await;
    let mcp = client.structured(tool, mcp_args).await;
    assert_eq!(
        cli, mcp,
        "CLI `--json {cli_args:?}` stdout must equal MCP `{tool}` structuredContent\n  \
         CLI: {cli:#}\n  MCP: {mcp:#}"
    );
    cli
}

fn results_array(v: &Value) -> &Vec<Value> {
    v["results"]
        .as_array()
        .unwrap_or_else(|| panic!("expected `results` array, got: {v:#}"))
}

// ===== Registry-backed daemon (for vault_list / vault_status) =====
//
// `vault_list` / `vault_status` read the vault registry, so they need the
// production `VaultManager::open` path rather than the `for_tests` manager the
// search surfaces use. Adapted from `tests/cli.rs::spawn_vault_cli_daemon`.

struct VaultDaemon {
    base_url: String,
    cfg_path: PathBuf,
    root: TempDir,
    shutdown: watch::Sender<bool>,
    handle: Option<JoinHandle<()>>,
}

impl VaultDaemon {
    async fn shutdown(mut self) {
        let _ = self.shutdown.send(true);
        if let Some(h) = self.handle.take() {
            let _ = h.await;
        }
    }
}

async fn spawn_vault_daemon() -> VaultDaemon {
    let root = tempfile::tempdir().expect("create root tempdir");
    let data_dir = root.path().join("data");
    std::fs::create_dir_all(&data_dir).expect("create data_dir");
    let cfg_path = root.path().join("config.toml");
    std::fs::write(
        &cfg_path,
        format!(
            "default_vault_name = \"default\"\n[storage]\ndata_dir = \"{}\"\n",
            data_dir.display(),
        ),
    )
    .expect("write config.toml");
    let config = Arc::new(Config::load(Some(&cfg_path)).expect("load config"));
    let registry = Arc::new(VaultRegistry::open(&data_dir).await.expect("open registry"));
    let embedder: Arc<dyn Embedder> = Arc::new(StubEmbedder::new(768));
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let manager = Arc::new(
        VaultManager::open(registry, config, embedder, 768, shutdown_rx.clone())
            .await
            .expect("open VaultManager"),
    );
    let state = ApiState {
        vault_manager: manager.clone(),
        event_bus: manager.event_bus(),
        started_at: std::time::Instant::now(),
        embedding_endpoint: None,
        semantic_config: SemanticSearchConfig::default(),
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

    VaultDaemon {
        base_url: format!("http://{addr}"),
        cfg_path,
        root,
        shutdown: shutdown_tx,
        handle: Some(handle),
    }
}

/// Poll `hmn --json vault status <name>` until the vault reports `active`, so
/// the parity snapshots are taken against a settled (not mid-bootstrap) state.
async fn wait_for_active(daemon: &VaultDaemon, name: &str) {
    for _ in 0..100 {
        let status = run_hmn_json(
            &daemon.cfg_path,
            &daemon.base_url,
            &["vault", "status", name],
        )
        .await;
        if status["status"].as_str() == Some("active") {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("vault `{name}` did not reach `active` within timeout");
}

// ===== Tests =====

#[tokio::test]
async fn cli_mcp_parity_search_filesystem() {
    let fx = fixture();
    seed_vault(&fx);
    let daemon = spawn_live_daemon(fx).await;
    let mut client = McpClient::spawn(&daemon.cfg_path, &daemon.base_url).await;

    let body = assert_parity(
        &daemon.cfg_path,
        &daemon.base_url,
        &mut client,
        &["search", "filesystem", "**/*.md"],
        "search_filesystem",
        json!({ "glob": "**/*.md" }),
    )
    .await;

    let results = results_array(&body);
    assert_eq!(results.len(), 3, "expected 3 markdown files: {body:#}");
    // Canonical path-field contract (#328): `path`, never `file_path`.
    for r in results {
        assert!(r.get("path").is_some(), "result missing `path`: {r}");
        assert!(
            r.get("file_path").is_none(),
            "result must not use `file_path`: {r}"
        );
    }

    client.shutdown().await;
    daemon.shutdown().await;
}

#[tokio::test]
async fn cli_mcp_parity_content_search_default_omits_matches() {
    // Finding-1 regression: a default content search must leave
    // `include_matches` at the wire default (false) on BOTH surfaces, so the
    // CLI cannot reintroduce its old hardcoded `include_matches: true`. If it
    // did, CLI `matches` would be populated while MCP's stays empty and the
    // parity assertion below would fail.
    let fx = fixture();
    seed_vault(&fx);
    let daemon = spawn_live_daemon(fx).await;
    let mut client = McpClient::spawn(&daemon.cfg_path, &daemon.base_url).await;

    let body = assert_parity(
        &daemon.cfg_path,
        &daemon.base_url,
        &mut client,
        &["search", "content", "vector"],
        "search_content",
        json!({ "query": "vector" }),
    )
    .await;

    let beta = results_array(&body)
        .iter()
        .find(|r| r["path"].as_str() == Some("notes/beta.md"))
        .unwrap_or_else(|| panic!("notes/beta.md missing from content results: {body:#}"));
    assert!(
        beta["match_count"].as_u64().unwrap_or(0) >= 1,
        "expected match_count >= 1 on beta.md: {beta}"
    );
    let matches = beta["matches"]
        .as_array()
        .unwrap_or_else(|| panic!("`matches` must serialize as an array: {beta}"));
    assert!(
        matches.is_empty(),
        "default content search must omit snippets until include_matches is set: {beta}"
    );

    client.shutdown().await;
    daemon.shutdown().await;
}

#[tokio::test]
async fn cli_mcp_parity_content_search_with_snippets() {
    let fx = fixture();
    seed_vault(&fx);
    let daemon = spawn_live_daemon(fx).await;
    let mut client = McpClient::spawn(&daemon.cfg_path, &daemon.base_url).await;

    let body = assert_parity(
        &daemon.cfg_path,
        &daemon.base_url,
        &mut client,
        &["search", "content", "vector", "--include-matches"],
        "search_content",
        json!({ "query": "vector", "include_matches": true }),
    )
    .await;

    let beta = results_array(&body)
        .iter()
        .find(|r| r["path"].as_str() == Some("notes/beta.md"))
        .unwrap_or_else(|| panic!("notes/beta.md missing: {body:#}"));
    let matches = beta["matches"]
        .as_array()
        .unwrap_or_else(|| panic!("`matches` must be an array: {beta}"));
    assert!(
        !matches.is_empty(),
        "opting in with --include-matches must populate snippets: {beta}"
    );

    client.shutdown().await;
    daemon.shutdown().await;
}

#[tokio::test]
async fn cli_mcp_parity_search_semantic_chunk() {
    let fx = fixture();
    seed_vault(&fx);
    let daemon = spawn_live_daemon(fx).await;
    let mut client = McpClient::spawn(&daemon.cfg_path, &daemon.base_url).await;

    let body = assert_parity(
        &daemon.cfg_path,
        &daemon.base_url,
        &mut client,
        &[
            "search",
            "semantic",
            "vector search",
            "--granularity",
            "chunk",
        ],
        "search_semantic",
        json!({ "query": "vector search", "granularity": "chunk" }),
    )
    .await;

    let results = results_array(&body);
    assert!(!results.is_empty(), "expected ≥1 chunk result: {body:#}");
    for r in results {
        assert!(r.get("path").is_some(), "chunk result missing `path`: {r}");
        assert!(
            r.get("file_path").is_none(),
            "chunk result must not use `file_path`: {r}"
        );
        assert!(
            r.get("content_hash").is_some(),
            "chunk result missing `content_hash`: {r}"
        );
    }

    client.shutdown().await;
    daemon.shutdown().await;
}

#[tokio::test]
async fn cli_mcp_parity_search_semantic_document() {
    let fx = fixture();
    seed_vault(&fx);
    let daemon = spawn_live_daemon(fx).await;
    let mut client = McpClient::spawn(&daemon.cfg_path, &daemon.base_url).await;

    let body = assert_parity(
        &daemon.cfg_path,
        &daemon.base_url,
        &mut client,
        &[
            "search",
            "semantic",
            "vector search",
            "--granularity",
            "document",
        ],
        "search_semantic",
        json!({ "query": "vector search", "granularity": "document" }),
    )
    .await;

    let results = results_array(&body);
    assert!(!results.is_empty(), "expected ≥1 document result: {body:#}");
    for doc in results {
        // Parent carries the canonical source reference.
        assert!(doc.get("path").is_some(), "document missing `path`: {doc}");
        assert!(
            doc.get("file_path").is_none(),
            "document must not use `file_path`: {doc}"
        );
        assert!(
            doc.get("content_hash").is_some(),
            "document missing `content_hash`: {doc}"
        );
        // Evidence chunks deliberately omit the source reference (#329).
        let chunks = doc["chunks"]
            .as_array()
            .unwrap_or_else(|| panic!("document missing nested `chunks`: {doc}"));
        assert!(
            !chunks.is_empty(),
            "document must have ≥1 evidence chunk: {doc}"
        );
        for chunk in chunks {
            assert!(
                chunk.get("path").is_none(),
                "evidence chunk must omit `path`: {chunk}"
            );
            assert!(
                chunk.get("file_path").is_none(),
                "evidence chunk must omit `file_path`: {chunk}"
            );
            assert!(
                chunk.get("content_hash").is_none(),
                "evidence chunk must omit `content_hash`: {chunk}"
            );
        }
    }

    client.shutdown().await;
    daemon.shutdown().await;
}

#[tokio::test]
async fn cli_mcp_parity_content_get() {
    let fx = fixture();
    seed_vault(&fx);
    let daemon = spawn_live_daemon(fx).await;
    let mut client = McpClient::spawn(&daemon.cfg_path, &daemon.base_url).await;

    let body = assert_parity(
        &daemon.cfg_path,
        &daemon.base_url,
        &mut client,
        &["content", "get", "notes/beta.md"],
        "content_get",
        json!({ "paths": ["notes/beta.md"] }),
    )
    .await;

    let item = &results_array(&body)[0];
    assert_eq!(
        item["path"].as_str(),
        Some("notes/beta.md"),
        "content_get path: {item}"
    );
    assert!(
        item.get("file_path").is_none(),
        "content_get must not use `file_path`: {item}"
    );

    client.shutdown().await;
    daemon.shutdown().await;
}

#[tokio::test]
async fn cli_mcp_parity_vault_list_and_status() {
    let daemon = spawn_vault_daemon().await;

    // Register one (empty) vault via the CLI so list/status have a subject;
    // an empty vault settles immediately, keeping the parity snapshots stable.
    let vault_dir = daemon.root.path().join("parity-vault");
    std::fs::create_dir_all(&vault_dir).expect("create vault dir");
    let created = run_hmn_json(
        &daemon.cfg_path,
        &daemon.base_url,
        &[
            "vault",
            "create",
            "--name",
            "parity",
            vault_dir.to_str().unwrap(),
        ],
    )
    .await;
    assert_eq!(
        created["status"].as_str(),
        Some("active"),
        "create: {created:#}"
    );
    wait_for_active(&daemon, "parity").await;

    // The registry-backed daemon mounts only the HTTP api router, so reach MCP
    // through `hmn mcp` (stdio), which proxies to the same daemon over HTTP.
    let mut client = McpClient::spawn(&daemon.cfg_path, &daemon.base_url).await;

    let list = assert_parity(
        &daemon.cfg_path,
        &daemon.base_url,
        &mut client,
        &["vault", "list"],
        "vault_list",
        json!({}),
    )
    .await;
    let vaults = list["vaults"]
        .as_array()
        .unwrap_or_else(|| panic!("expected `vaults` array: {list:#}"));
    assert_eq!(
        vaults.len(),
        1,
        "expected the single created vault: {list:#}"
    );

    let status = assert_parity(
        &daemon.cfg_path,
        &daemon.base_url,
        &mut client,
        &["vault", "status", "parity"],
        "vault_status",
        json!({ "target": "parity" }),
    )
    .await;
    assert_eq!(
        status["name"].as_str(),
        Some("parity"),
        "vault_status name: {status:#}"
    );

    client.shutdown().await;
    daemon.shutdown().await;
}
