//! Integration tests for the MCP wire path: live in-process `hmnd` + `hmn mcp`
//! subprocess speaking newline-delimited JSON-RPC over stdio. Per workplan
//! § Task 8.4.
//!
//! Each test:
//! - Stands up a `LiveDaemon` (in-process axum server + StubEmbedder, mirroring
//!   the `tests/cli.rs` pattern).
//! - Spawns `hmn mcp --daemon-url <url>` as a subprocess via
//!   `env!("CARGO_BIN_EXE_hmn")`.
//! - Drives the MCP handshake (`initialize` + `notifications/initialized`) and
//!   the test-specific request over the child's stdio.
//! - Asserts on the JSON-RPC response shape and tears down the child by
//!   dropping its stdin (per Task 8.3's verification that
//!   `rmcp::transport::stdio` exits cleanly on EOF).
//!
//! Anti-flake: 3× consecutive `cargo test --test mcp` clean is required at
//! task close (workplan § Task 8.4). Subprocess teardown closes stdin and
//! `wait_with_output`-style timeout safety nets the child's exit.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use hypomnema::api::{self, ApiState};
use hypomnema::config::{Config, EmbeddingConfig};
use hypomnema::embedding::{Embedder, StubEmbedder};
use hypomnema::indexer::Scanner;
use hypomnema::outbox::Outbox;
use hypomnema::store::Store;
use hypomnema::watcher::{self, Watcher};
use rusqlite::Connection;
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::watch;
use tokio::task::JoinHandle;

const DEBOUNCE_MS: u64 = 50;
const SCHEMA_DIM: usize = 768;
const READ_TIMEOUT: Duration = Duration::from_secs(10);
const EXIT_TIMEOUT: Duration = Duration::from_secs(5);
const PROTOCOL_VERSION: &str = "2025-06-18";

// ===== Fixture =====

struct Fixture {
    _root: TempDir,
    vault: PathBuf,
    data_dir: PathBuf,
    cfg_path: PathBuf,
    config: Config,
}

fn fixture() -> Fixture {
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
             debounce_ms = {}\n",
            vault.display(),
            data_dir.display(),
            DEBOUNCE_MS,
        ),
    )
    .expect("write config.toml");
    let config = Config::load(Some(&cfg_path)).expect("load config");
    let vault = config.vault.0.clone();
    let data_dir = config.storage.data_dir.0.clone();
    Fixture {
        _root: root,
        vault,
        data_dir,
        cfg_path,
        config,
    }
}

fn write_file(vault: &Path, rel: &str, body: &[u8]) {
    let path = vault.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent dirs");
    }
    fs::write(&path, body).expect("write fixture file");
}

// ===== Live in-process daemon =====

struct LiveDaemon {
    base_url: String,
    cfg_path: PathBuf,
    data_dir: PathBuf,
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
    let embedder: Arc<dyn Embedder> = Arc::new(StubEmbedder::new(SCHEMA_DIM));
    let store = Store::open(
        &fx.data_dir,
        &fx.config.storage.index_file,
        &fx.config.embedding,
    )
    .await
    .expect("open store");
    let scanner = Scanner::new(&fx.config, &store, embedder.clone()).expect("construct scanner");
    let _ = scanner.run().await.expect("initial scan");

    let ignores = fx
        .config
        .watcher
        .compiled_ignores()
        .expect("compile ignores");
    let (watcher, rx) = watcher::spawn_watcher(
        &fx.vault,
        ignores,
        Duration::from_millis(fx.config.watcher.debounce_ms),
        256,
    )
    .expect("spawn watcher");

    let outbox_path = fx.data_dir.join(&fx.config.storage.outbox_file);
    let outbox = Outbox::open(outbox_path.clone())
        .await
        .expect("open outbox");
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let consumer = tokio::spawn(watcher::run_consumer(
        rx,
        scanner,
        outbox,
        shutdown_rx.clone(),
    ));

    let state = ApiState {
        pool: store.pool(),
        vault: fx.vault.clone(),
        outbox_path,
        embedder,
        embedding_dimension: fx.config.embedding.dimension,
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
        data_dir: fx.data_dir.clone(),
        watcher: Some(watcher),
        consumer: Some(consumer),
        server: Some(server),
        shutdown_tx,
        _fx: fx,
    }
}

// ===== `hmn mcp` subprocess + JSON-RPC framing =====
//
// rmcp's `transport::stdio` speaks newline-delimited JSON-RPC over stdin and
// stdout: one JSON object per `\n`-terminated line, no Content-Length headers.
// Verified during Task 8.3 manual smoke (see todo 78 results comment for the
// transcripts that pinned this). We hand-roll the framing here rather than
// pull in a full MCP client crate — the framing is small, the protocol surface
// we exercise is small, and rmcp's client-side feature flags would otherwise
// need their own verification gate.

struct McpClient {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl McpClient {
    async fn spawn(cfg_path: &Path, daemon_url: &str) -> Self {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_hmn"));
        cmd.arg("--config")
            .arg(cfg_path)
            .arg("--daemon-url")
            .arg(daemon_url)
            .arg("mcp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // Stderr is dropped — rmcp + tracing under `BinaryKind::HmnMcp`
            // route logs there, but the tests assert only on stdout's JSON-RPC
            // frames. If a test fails surprisingly, swap to `Stdio::inherit()`
            // locally to see the child's logs.
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

    async fn send_request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        let req = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        self.write_message(&req).await;
        loop {
            let msg = self.read_message().await;
            // Skip server-initiated notifications / unrelated responses; only
            // return when the response's id matches our request.
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
        serde_json::from_str(line.trim()).unwrap_or_else(|e| {
            panic!("invalid JSON-RPC frame from child stdout: {e}; raw line: {line:?}")
        })
    }

    async fn handshake(&mut self) -> Value {
        let resp = self
            .send_request(
                "initialize",
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": { "name": "hmn-mcp-tests", "version": "0.0.0" }
                }),
            )
            .await;
        // Per MCP spec, the client must send the `initialized` notification
        // after a successful initialize before issuing further requests.
        self.send_notification("notifications/initialized", json!({}))
            .await;
        resp
    }

    async fn shutdown(mut self) {
        // Closing stdin is the documented signal for `rmcp::transport::stdio`
        // to wind down (verified in Task 8.3 smoke; see scratchpad 8 § 8.3
        // forward note). Wait with a timeout as the safety net.
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

// ===== Helpers for the semantic-hint test =====

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

/// Wipe `chunks_vec` and `chunks` while leaving `files` populated. Mirrors
/// `tests/embedding.rs::truncate_chunks_only` — the corrected hint-state
/// recipe per step-7 § Build-time amendments item 1.
fn truncate_chunks_only(data_dir: &Path) {
    let conn = open_index_rw(data_dir);
    conn.execute("DELETE FROM chunks_vec", [])
        .expect("delete chunks_vec");
    conn.execute("DELETE FROM chunks", [])
        .expect("delete chunks");
}

// ===== Tests =====

#[tokio::test]
async fn mcp_initialize_returns_server_info() {
    let daemon = spawn_live_daemon(fixture()).await;
    let mut client = McpClient::spawn(&daemon.cfg_path, &daemon.base_url).await;

    let resp = client.handshake().await;
    let result = &resp["result"];
    // Auto-derived from the rmcp crate via `Implementation::from_build_env()`.
    // Task 8.3 smoke pinned this to (`rmcp`, `1.5.0`); revisit if Task 8.5
    // adds a `#[tool_handler(name=…, version=…)]` override on the impl block.
    assert_eq!(
        result["serverInfo"]["name"],
        json!("rmcp"),
        "unexpected serverInfo: {result}"
    );
    assert_eq!(
        result["serverInfo"]["version"],
        json!("1.5.0"),
        "unexpected serverInfo: {result}"
    );
    assert_eq!(
        result["protocolVersion"],
        json!(PROTOCOL_VERSION),
        "unexpected protocolVersion: {result}"
    );
    assert!(
        result["capabilities"]["tools"].is_object(),
        "tools capability missing: {result}"
    );

    client.shutdown().await;
    daemon.shutdown().await;
}

#[tokio::test]
async fn mcp_tools_list_advertises_three_tools() {
    let daemon = spawn_live_daemon(fixture()).await;
    let mut client = McpClient::spawn(&daemon.cfg_path, &daemon.base_url).await;
    client.handshake().await;

    let resp = client.send_request("tools/list", json!({})).await;
    let tools = resp["result"]["tools"]
        .as_array()
        .unwrap_or_else(|| panic!("tools/list missing tools array: {resp}"));
    let names: Vec<&str> = tools
        .iter()
        .map(|t| t["name"].as_str().expect("tool name is a string"))
        .collect();
    assert_eq!(names.len(), 3, "expected 3 tools, got {names:?}");
    assert!(
        names.contains(&"search_filesystem"),
        "missing search_filesystem in {names:?}"
    );
    assert!(
        names.contains(&"search_content"),
        "missing search_content in {names:?}"
    );
    assert!(
        names.contains(&"search_semantic"),
        "missing search_semantic in {names:?}"
    );

    client.shutdown().await;
    daemon.shutdown().await;
}

#[tokio::test]
async fn mcp_tools_list_includes_input_schemas_with_descriptions() {
    let daemon = spawn_live_daemon(fixture()).await;
    let mut client = McpClient::spawn(&daemon.cfg_path, &daemon.base_url).await;
    client.handshake().await;

    let resp = client.send_request("tools/list", json!({})).await;
    let tools = resp["result"]["tools"]
        .as_array()
        .unwrap_or_else(|| panic!("tools/list missing tools array: {resp}"));

    let fs_tool = tools
        .iter()
        .find(|t| t["name"] == "search_filesystem")
        .unwrap_or_else(|| panic!("search_filesystem missing from tools/list: {resp}"));

    // Sanity-check Task 8.1's per-field `#[schemars(description = …)]`
    // annotation made it through derive → JSON Schema → tools/list.
    let prefix_desc = fs_tool["inputSchema"]["properties"]["prefix"]["description"]
        .as_str()
        .unwrap_or_else(|| {
            panic!(
                "missing inputSchema.properties.prefix.description on search_filesystem: {fs_tool}"
            )
        });
    assert!(
        !prefix_desc.is_empty(),
        "search_filesystem.inputSchema.properties.prefix.description must be non-empty"
    );

    // The tool-level description from `#[tool(description = …)]` must also
    // round-trip — independent gate from the per-field schema description.
    let tool_desc = fs_tool["description"].as_str().unwrap_or_else(|| {
        panic!("search_filesystem.description missing or non-string: {fs_tool}")
    });
    assert!(
        !tool_desc.is_empty(),
        "search_filesystem.description must be non-empty"
    );

    client.shutdown().await;
    daemon.shutdown().await;
}

#[tokio::test]
async fn mcp_call_search_filesystem_returns_structured_content() {
    let fx = fixture();
    // Seed before spawning the daemon so the synchronous initial scan picks
    // them up — sidesteps the watcher-debounce timing axis for this test.
    write_file(&fx.vault, "alpha.md", b"alpha body\n");
    write_file(&fx.vault, "notes/beta.md", b"beta body\n");
    let daemon = spawn_live_daemon(fx).await;
    let mut client = McpClient::spawn(&daemon.cfg_path, &daemon.base_url).await;
    client.handshake().await;

    let resp = client
        .send_request(
            "tools/call",
            json!({
                "name": "search_filesystem",
                "arguments": { "glob": "**/*.md" }
            }),
        )
        .await;
    let result = &resp["result"];
    assert!(
        !result["isError"].as_bool().unwrap_or(false),
        "expected success, got {result}"
    );
    let structured = &result["structuredContent"];
    let results = structured["results"]
        .as_array()
        .unwrap_or_else(|| panic!("missing structuredContent.results array: {structured}"));
    assert_eq!(
        results.len(),
        2,
        "expected 2 filesystem results, got {results:?}"
    );
    let paths: Vec<&str> = results
        .iter()
        .map(|r| r["path"].as_str().expect("path is a string"))
        .collect();
    assert!(
        paths.contains(&"alpha.md"),
        "expected alpha.md in {paths:?}"
    );
    assert!(
        paths.contains(&"notes/beta.md"),
        "expected notes/beta.md in {paths:?}"
    );

    client.shutdown().await;
    daemon.shutdown().await;
}

#[tokio::test]
async fn mcp_call_search_content_returns_structured_content() {
    let fx = fixture();
    write_file(
        &fx.vault,
        "needle.md",
        b"the quick brown fox jumps over the lazy dog\n",
    );
    let daemon = spawn_live_daemon(fx).await;
    let mut client = McpClient::spawn(&daemon.cfg_path, &daemon.base_url).await;
    client.handshake().await;

    let resp = client
        .send_request(
            "tools/call",
            json!({
                "name": "search_content",
                "arguments": { "query": "quick brown" }
            }),
        )
        .await;
    let result = &resp["result"];
    assert!(
        !result["isError"].as_bool().unwrap_or(false),
        "expected success, got {result}"
    );
    let structured = &result["structuredContent"];
    let results = structured["results"]
        .as_array()
        .unwrap_or_else(|| panic!("missing structuredContent.results array: {structured}"));
    let needle = results
        .iter()
        .find(|r| r["path"].as_str() == Some("needle.md"))
        .unwrap_or_else(|| panic!("needle.md missing from results: {structured}"));
    let match_count = needle["match_count"]
        .as_u64()
        .unwrap_or_else(|| panic!("match_count missing or non-numeric on needle.md: {needle}"));
    assert!(
        match_count >= 1,
        "expected match_count >= 1 on needle.md, got {match_count} ({needle})"
    );

    client.shutdown().await;
    daemon.shutdown().await;
}

#[tokio::test]
async fn mcp_call_search_semantic_returns_structured_content_with_hint() {
    let fx = fixture();
    // Index normally first so `files` has a row, then truncate `chunks_vec`
    // and `chunks` to reproduce the "vault seen, semantic index empty" state
    // that triggers the hint. Per step-7 § Build-time amendments item 1 and
    // tests/embedding.rs::semantic_search_returns_hint_when_index_empty…
    write_file(
        &fx.vault,
        "seeded.md",
        b"## Section\n\nIndexed body text.\n",
    );
    let daemon = spawn_live_daemon(fx).await;
    truncate_chunks_only(&daemon.data_dir);

    let mut client = McpClient::spawn(&daemon.cfg_path, &daemon.base_url).await;
    client.handshake().await;

    let resp = client
        .send_request(
            "tools/call",
            json!({
                "name": "search_semantic",
                "arguments": { "query": "section" }
            }),
        )
        .await;
    let result = &resp["result"];
    assert!(
        !result["isError"].as_bool().unwrap_or(false),
        "expected success, got {result}"
    );
    let structured = &result["structuredContent"];
    assert_eq!(
        structured["hint"].as_str(),
        Some("semantic index is building"),
        "expected hint, got {structured}"
    );

    client.shutdown().await;
    daemon.shutdown().await;
}

#[tokio::test]
async fn mcp_call_with_invalid_glob_returns_structured_error() {
    let daemon = spawn_live_daemon(fixture()).await;
    let mut client = McpClient::spawn(&daemon.cfg_path, &daemon.base_url).await;
    client.handshake().await;

    let resp = client
        .send_request(
            "tools/call",
            json!({
                "name": "search_filesystem",
                "arguments": { "glob": "[unterminated" }
            }),
        )
        .await;
    let result = &resp["result"];
    assert_eq!(
        result["isError"],
        json!(true),
        "expected isError=true, got {result}"
    );
    let structured = &result["structuredContent"];
    assert_eq!(
        structured["error"]["code"],
        json!("invalid_glob"),
        "expected error.code = invalid_glob, got {structured}"
    );

    client.shutdown().await;
    daemon.shutdown().await;
}

#[tokio::test]
async fn mcp_call_against_dead_daemon_returns_daemon_unreachable() {
    // Bind+drop pattern: grab a free port, then drop the listener so connects
    // are refused. Same pattern as `client::tests::client_returns_connect_error_when_daemon_down`.
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    drop(listener);
    let dead_url = format!("http://{addr}");

    // The `hmn` binary still needs a valid config to load; provide one whose
    // vault is real but whose daemon URL we override on the command line.
    let fx = fixture();
    let cfg_path = fx.cfg_path.clone();
    let mut client = McpClient::spawn(&cfg_path, &dead_url).await;
    client.handshake().await;

    let resp = client
        .send_request(
            "tools/call",
            json!({
                "name": "search_filesystem",
                "arguments": {}
            }),
        )
        .await;
    let result = &resp["result"];
    assert_eq!(
        result["isError"],
        json!(true),
        "expected isError=true, got {result}"
    );
    let structured = &result["structuredContent"];
    assert_eq!(
        structured["error"]["code"],
        json!("daemon_unreachable"),
        "expected error.code = daemon_unreachable, got {structured}"
    );
    let message = structured["error"]["message"].as_str().unwrap_or("");
    assert!(
        message.contains(&dead_url),
        "error message should embed the configured URL ({dead_url}), got {message:?}"
    );

    client.shutdown().await;
    drop(fx);
}
