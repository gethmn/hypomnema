use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;

use hypomnema::api::{self, ApiState, VaultEntry};
use hypomnema::config::Config;
use hypomnema::control_plane::VaultManager;
use hypomnema::embedding::{Embedder, StubEmbedder};
use hypomnema::indexer::Scanner;
use hypomnema::store::Store;
use hypomnema::vault_registry::VaultStatus;
use hypomnema::vault_registry::{VaultId, VaultRegistry, VaultRow};
use serde_json::Value;
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio::sync::watch;
use tokio::task::JoinHandle;

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
    fs::create_dir_all(&vault).expect("create vault dir");

    let cfg_path = root.path().join("config.toml");
    fs::write(
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
        fs::create_dir_all(parent).expect("create parent dirs");
    }
    fs::write(&path, body).expect("write fixture file");
}

struct LiveDaemon {
    base_url: String,
    cfg_path: PathBuf,
    vault: PathBuf,
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
    let embedder: Arc<dyn Embedder> = Arc::new(StubEmbedder::new(768));
    let scanner =
        Scanner::new(&fx.vault, &fx.config, &store, embedder.clone()).expect("construct scanner");
    let _ = scanner.run().await.expect("initial scan");

    let entry = VaultEntry {
        id: fx.vault_id.clone(),
        name: "test".to_string(),
        vault_path: fx.vault.clone(),
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
        vault: fx.vault.clone(),
        shutdown: tx,
        handle: Some(handle),
        _fx: fx,
    }
}

async fn run_hmn(cfg_path: &Path, daemon_url: &str, args: &[&str]) -> std::process::Output {
    // The hmn binary is a sync subprocess, but the in-process daemon under
    // test runs on this same tokio runtime. Block in spawn_blocking so the
    // runtime's reactor stays free to drive the daemon's tasks.
    let cfg_path = cfg_path.to_path_buf();
    let daemon_url = daemon_url.to_string();
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    tokio::task::spawn_blocking(move || {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_hmn"));
        cmd.arg("--config")
            .arg(&cfg_path)
            .arg("--daemon-url")
            .arg(&daemon_url);
        cmd.args(&args);
        cmd.output().expect("run hmn")
    })
    .await
    .expect("spawn_blocking join")
}

fn seed_default_vault(fx: &Fixture) {
    write_file(fx, "alpha.md", b"alpha\n");
    write_file(fx, "beta.md", b"bravo\n");
    write_file(fx, "notes/gamma.md", b"gamma\n");
    write_file(fx, "notes/delta.md", b"delta\n");
    write_file(
        fx,
        "notes/sub/multi.md",
        b"line one\nline two contains pgvector here\nline three\n",
    );
}

#[tokio::test]
async fn hmn_search_filesystem_text_mode() {
    let fx = fixture();
    seed_default_vault(&fx);
    let daemon = spawn_live_daemon(fx).await;

    let out = run_hmn(
        &daemon.cfg_path,
        &daemon.base_url,
        &["search", "filesystem", "**/*.md"],
    )
    .await;
    assert!(
        out.status.success(),
        "hmn exit={:?} stderr={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    for expected in [
        "alpha.md",
        "beta.md",
        "notes/gamma.md",
        "notes/sub/multi.md",
    ] {
        assert!(
            stdout.contains(expected),
            "stdout missing {expected:?}: {stdout}"
        );
    }

    daemon.shutdown().await;
}

#[tokio::test]
async fn hmn_search_filesystem_json_mode() {
    let fx = fixture();
    seed_default_vault(&fx);
    let daemon = spawn_live_daemon(fx).await;

    let out = run_hmn(
        &daemon.cfg_path,
        &daemon.base_url,
        &["--json", "search", "filesystem", "**/*.md"],
    )
    .await;
    assert!(
        out.status.success(),
        "hmn exit={:?} stderr={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let body: Value = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("stdout is not JSON: {e}; raw={:?}", out.stdout));
    assert!(
        body.get("results").is_some(),
        "missing `results` key in {body}"
    );
    assert!(
        body.get("truncated").is_some(),
        "missing `truncated` key in {body}"
    );
    assert_eq!(body["results"].as_array().unwrap().len(), 5);

    daemon.shutdown().await;
}

#[tokio::test]
async fn hmn_search_content_text_mode() {
    let fx = fixture();
    seed_default_vault(&fx);
    let daemon = spawn_live_daemon(fx).await;

    let out = run_hmn(
        &daemon.cfg_path,
        &daemon.base_url,
        &["search", "content", "pgvector"],
    )
    .await;
    assert!(
        out.status.success(),
        "hmn exit={:?} stderr={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("notes/sub/multi.md (1 matches)"),
        "expected `<path> (<count> matches)` block, got {stdout}"
    );

    daemon.shutdown().await;
}

#[tokio::test]
async fn hmn_status_text_mode() {
    let fx = fixture();
    seed_default_vault(&fx);
    let daemon = spawn_live_daemon(fx).await;

    let out = run_hmn(&daemon.cfg_path, &daemon.base_url, &["status"]).await;
    assert!(
        out.status.success(),
        "hmn status exit={:?} stderr={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let vault_str = daemon.vault.display().to_string();
    assert!(
        stdout.contains(&vault_str),
        "stdout must contain the vault path {vault_str:?}; got {stdout}"
    );

    daemon.shutdown().await;
}

// ===== Vault control-plane CLI tests =====
//
// These spin up a daemon backed by a real `VaultManager::open`, so the
// HTTP `POST /vaults` and `DELETE /vaults/:id_or_name` routes are
// genuinely callable. The fixtures above use `VaultManager::for_tests`,
// which short-circuits create/list/get/terminate.

struct VaultCliDaemon {
    base_url: String,
    cfg_path: PathBuf,
    root: TempDir,
    shutdown: watch::Sender<bool>,
    handle: Option<JoinHandle<()>>,
}

impl VaultCliDaemon {
    async fn shutdown(mut self) {
        // Single shutdown signal: the watch::Sender feeds both the
        // VaultManager's per-runner mirror task and axum's graceful-
        // shutdown future. Test cleanup needs both to wind down so the
        // tempdir drops cleanly.
        let _ = self.shutdown.send(true);
        if let Some(h) = self.handle.take() {
            let _ = h.await;
        }
    }
}

async fn spawn_vault_cli_daemon() -> VaultCliDaemon {
    let root = tempfile::tempdir().expect("create root tempdir");
    let data_dir = root.path().join("data");
    fs::create_dir_all(&data_dir).expect("create data_dir");
    let cfg_path = root.path().join("config.toml");
    fs::write(
        &cfg_path,
        format!(
            "default_vault_name = \"default\"\n[storage]\ndata_dir = \"{}\"\n",
            data_dir.display(),
        ),
    )
    .expect("write config.toml");
    let config = Config::load(Some(&cfg_path)).expect("load config");
    let config = Arc::new(config);
    let registry = Arc::new(
        VaultRegistry::open(&data_dir)
            .await
            .expect("open VaultRegistry"),
    );
    let embedder: Arc<dyn Embedder> = Arc::new(StubEmbedder::new(768));
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let manager = VaultManager::open(registry, config, embedder, 768, shutdown_rx.clone())
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

    VaultCliDaemon {
        base_url: format!("http://{addr}"),
        cfg_path,
        root,
        shutdown: shutdown_tx,
        handle: Some(handle),
    }
}

fn fresh_vault_dir(parent: &Path, name: &str) -> PathBuf {
    let p = parent.join(name);
    fs::create_dir_all(&p).expect("create vault subdir");
    p
}

#[tokio::test]
async fn hmn_vault_create_then_list_returns_the_new_vault() {
    let daemon = spawn_vault_cli_daemon().await;
    let vault_path = fresh_vault_dir(daemon.root.path(), "v1");

    let create = run_hmn(
        &daemon.cfg_path,
        &daemon.base_url,
        &[
            "--json",
            "vault",
            "create",
            "--name",
            "alpha",
            vault_path.to_str().unwrap(),
        ],
    )
    .await;
    assert!(
        create.status.success(),
        "create exit={:?} stderr={}",
        create.status.code(),
        String::from_utf8_lossy(&create.stderr),
    );
    let created: Value = serde_json::from_slice(&create.stdout)
        .unwrap_or_else(|e| panic!("create stdout not JSON: {e}; raw={:?}", create.stdout));
    assert_eq!(created["name"], "alpha");
    assert_eq!(created["status"], "active");
    let id = created["id"]
        .as_str()
        .expect("id is a string in create response");
    assert!(!id.is_empty());

    let list = run_hmn(
        &daemon.cfg_path,
        &daemon.base_url,
        &["--json", "vault", "list"],
    )
    .await;
    assert!(
        list.status.success(),
        "list exit={:?} stderr={}",
        list.status.code(),
        String::from_utf8_lossy(&list.stderr),
    );
    let listed: Value = serde_json::from_slice(&list.stdout)
        .unwrap_or_else(|e| panic!("list stdout not JSON: {e}; raw={:?}", list.stdout));
    let vaults = listed["vaults"]
        .as_array()
        .expect("`vaults` array in list response");
    assert_eq!(vaults.len(), 1);
    assert_eq!(vaults[0]["id"].as_str(), Some(id));
    assert_eq!(vaults[0]["name"], "alpha");

    daemon.shutdown().await;
}

#[tokio::test]
async fn hmn_vault_terminate_with_yes_succeeds() {
    let daemon = spawn_vault_cli_daemon().await;
    let vault_path = fresh_vault_dir(daemon.root.path(), "v");

    let create = run_hmn(
        &daemon.cfg_path,
        &daemon.base_url,
        &[
            "--json",
            "vault",
            "create",
            "--name",
            "doomed",
            vault_path.to_str().unwrap(),
        ],
    )
    .await;
    assert!(
        create.status.success(),
        "create stderr={}",
        String::from_utf8_lossy(&create.stderr),
    );

    let term = run_hmn(
        &daemon.cfg_path,
        &daemon.base_url,
        &["--json", "vault", "terminate", "doomed", "--yes"],
    )
    .await;
    assert!(
        term.status.success(),
        "terminate --yes exit={:?} stderr={}",
        term.status.code(),
        String::from_utf8_lossy(&term.stderr),
    );
    let terminated: Value = serde_json::from_slice(&term.stdout)
        .unwrap_or_else(|e| panic!("terminate stdout not JSON: {e}; raw={:?}", term.stdout));
    assert_eq!(terminated["terminated"], true);
    assert!(
        terminated["id"].as_str().is_some_and(|s| !s.is_empty()),
        "terminate response carries canonical id: {terminated}"
    );

    let list = run_hmn(
        &daemon.cfg_path,
        &daemon.base_url,
        &["--json", "vault", "list"],
    )
    .await;
    let listed: Value = serde_json::from_slice(&list.stdout).expect("list JSON");
    assert!(
        listed["vaults"].as_array().unwrap().is_empty(),
        "vault should be gone after terminate; got {listed}"
    );

    daemon.shutdown().await;
}

#[tokio::test]
async fn hmn_vault_terminate_without_yes_aborts_on_no() {
    let daemon = spawn_vault_cli_daemon().await;
    let vault_path = fresh_vault_dir(daemon.root.path(), "v");

    let create = run_hmn(
        &daemon.cfg_path,
        &daemon.base_url,
        &[
            "--json",
            "vault",
            "create",
            "--name",
            "kept",
            vault_path.to_str().unwrap(),
        ],
    )
    .await;
    assert!(create.status.success());

    // Pipe `n\n` to stdin; no --yes flag → confirmation prompt fires; the
    // process must exit 0 without hitting the daemon's terminate route.
    let cfg_path = daemon.cfg_path.clone();
    let base_url = daemon.base_url.clone();
    let term = tokio::task::spawn_blocking(move || {
        let mut child = Command::new(env!("CARGO_BIN_EXE_hmn"))
            .arg("--config")
            .arg(&cfg_path)
            .arg("--daemon-url")
            .arg(&base_url)
            .args(["vault", "terminate", "kept"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn hmn vault terminate");
        {
            let mut stdin = child.stdin.take().expect("hmn stdin");
            stdin.write_all(b"n\n").expect("write stdin");
        }
        child.wait_with_output().expect("hmn wait")
    })
    .await
    .expect("spawn_blocking join");

    assert!(
        term.status.success(),
        "no-on-prompt should exit 0; got {:?} stderr={}",
        term.status.code(),
        String::from_utf8_lossy(&term.stderr),
    );
    let stdout = String::from_utf8_lossy(&term.stdout);
    let stderr = String::from_utf8_lossy(&term.stderr);
    assert!(
        stdout.contains("aborted"),
        "stdout should announce abort; got stdout={stdout:?} stderr={stderr:?}",
    );
    assert!(
        stderr.contains("Terminate vault 'kept'? (y/N) "),
        "prompt should appear on stderr; got {stderr:?}",
    );

    // Confirm the vault is still present.
    let list = run_hmn(
        &daemon.cfg_path,
        &daemon.base_url,
        &["--json", "vault", "list"],
    )
    .await;
    let listed: Value = serde_json::from_slice(&list.stdout).expect("list JSON");
    let vaults = listed["vaults"].as_array().unwrap();
    assert_eq!(vaults.len(), 1, "vault should not have been terminated");
    assert_eq!(vaults[0]["name"], "kept");

    daemon.shutdown().await;
}

#[tokio::test]
async fn hmn_vault_status_without_target_falls_back_to_default_vault_name() {
    let daemon = spawn_vault_cli_daemon().await;
    let vault_path = fresh_vault_dir(daemon.root.path(), "v");

    // Create a vault with the config's default name; `status` with no target
    // must resolve to it.
    let create = run_hmn(
        &daemon.cfg_path,
        &daemon.base_url,
        &[
            "--json",
            "vault",
            "create",
            "--name",
            "default",
            vault_path.to_str().unwrap(),
        ],
    )
    .await;
    assert!(create.status.success());

    let status = run_hmn(
        &daemon.cfg_path,
        &daemon.base_url,
        &["--json", "vault", "status"],
    )
    .await;
    assert!(
        status.status.success(),
        "status exit={:?} stderr={}",
        status.status.code(),
        String::from_utf8_lossy(&status.stderr),
    );
    let row: Value = serde_json::from_slice(&status.stdout).expect("status JSON");
    assert_eq!(row["name"], "default");
    assert_eq!(row["status"], "active");

    daemon.shutdown().await;
}

// ===== Lifecycle-op CLI tests (Task 11.4) =====
//
// These reuse the `spawn_vault_cli_daemon` fixture so the daemon is backed
// by a real `VaultManager::open` — the lifecycle ops walk through the
// genuine pause/resume/reset/rename/rescan paths.

async fn spawn_vault_cli_daemon_with_errored_row(
    errored_name: &str,
    last_error: &str,
) -> (VaultCliDaemon, VaultId) {
    let root = tempfile::tempdir().expect("create root tempdir");
    let data_dir = root.path().join("data");
    fs::create_dir_all(&data_dir).expect("create data_dir");
    let cfg_path = root.path().join("config.toml");
    fs::write(
        &cfg_path,
        format!(
            "default_vault_name = \"default\"\n[storage]\ndata_dir = \"{}\"\n",
            data_dir.display(),
        ),
    )
    .expect("write config.toml");
    let config = Config::load(Some(&cfg_path)).expect("load config");
    let config = Arc::new(config);
    let registry = Arc::new(
        VaultRegistry::open(&data_dir)
            .await
            .expect("open VaultRegistry"),
    );

    // Path must be accessible — reset's errored-row branch validates that
    // before flipping status back to active.
    let vault_path = root.path().join("errd-vault");
    fs::create_dir_all(&vault_path).expect("create errored vault dir");
    let id = VaultId::new();
    registry
        .insert(VaultRow {
            id: id.clone(),
            name: errored_name.to_string(),
            path: vault_path,
            status: VaultStatus::Errored,
            created_at: chrono::Utc::now(),
            last_error: Some(last_error.to_string()),
        })
        .await
        .expect("insert errored row");

    let embedder: Arc<dyn Embedder> = Arc::new(StubEmbedder::new(768));
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let manager = VaultManager::open(registry, config, embedder, 768, shutdown_rx.clone())
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

    (
        VaultCliDaemon {
            base_url: format!("http://{addr}"),
            cfg_path,
            root,
            shutdown: shutdown_tx,
            handle: Some(handle),
        },
        id,
    )
}

#[tokio::test]
async fn hmn_vault_pause_then_resume_round_trip() {
    let daemon = spawn_vault_cli_daemon().await;
    let vault_path = fresh_vault_dir(daemon.root.path(), "v");

    let create = run_hmn(
        &daemon.cfg_path,
        &daemon.base_url,
        &[
            "--json",
            "vault",
            "create",
            "--name",
            "rt",
            vault_path.to_str().unwrap(),
        ],
    )
    .await;
    assert!(
        create.status.success(),
        "create stderr={}",
        String::from_utf8_lossy(&create.stderr)
    );

    let pause = run_hmn(
        &daemon.cfg_path,
        &daemon.base_url,
        &["--json", "vault", "pause", "rt"],
    )
    .await;
    assert!(
        pause.status.success(),
        "pause exit={:?} stderr={}",
        pause.status.code(),
        String::from_utf8_lossy(&pause.stderr)
    );
    let paused: Value = serde_json::from_slice(&pause.stdout).expect("pause JSON");
    assert_eq!(paused["status"], "paused");

    let resume = run_hmn(
        &daemon.cfg_path,
        &daemon.base_url,
        &["--json", "vault", "resume", "rt"],
    )
    .await;
    assert!(
        resume.status.success(),
        "resume exit={:?} stderr={}",
        resume.status.code(),
        String::from_utf8_lossy(&resume.stderr)
    );
    let resumed: Value = serde_json::from_slice(&resume.stdout).expect("resume JSON");
    assert_eq!(resumed["status"], "active");

    daemon.shutdown().await;
}

#[tokio::test]
async fn hmn_vault_rename_updates_vault_list() {
    let daemon = spawn_vault_cli_daemon().await;
    let vault_path = fresh_vault_dir(daemon.root.path(), "v");

    let create = run_hmn(
        &daemon.cfg_path,
        &daemon.base_url,
        &[
            "--json",
            "vault",
            "create",
            "--name",
            "before",
            vault_path.to_str().unwrap(),
        ],
    )
    .await;
    assert!(create.status.success());

    let rename = run_hmn(
        &daemon.cfg_path,
        &daemon.base_url,
        &["--json", "vault", "rename", "before", "--new-name", "after"],
    )
    .await;
    assert!(
        rename.status.success(),
        "rename exit={:?} stderr={}",
        rename.status.code(),
        String::from_utf8_lossy(&rename.stderr)
    );
    let renamed: Value = serde_json::from_slice(&rename.stdout).expect("rename JSON");
    assert_eq!(renamed["name"], "after");

    let list = run_hmn(
        &daemon.cfg_path,
        &daemon.base_url,
        &["--json", "vault", "list"],
    )
    .await;
    let listed: Value = serde_json::from_slice(&list.stdout).expect("list JSON");
    let vaults = listed["vaults"].as_array().unwrap();
    assert_eq!(vaults.len(), 1);
    assert_eq!(vaults[0]["name"], "after");

    daemon.shutdown().await;
}

#[tokio::test]
async fn hmn_vault_reset_without_rebuild_clears_errored_state() {
    let (daemon, _id) =
        spawn_vault_cli_daemon_with_errored_row("errd", "synthetic startup error").await;

    let reset = run_hmn(
        &daemon.cfg_path,
        &daemon.base_url,
        &["--json", "vault", "reset", "errd"],
    )
    .await;
    assert!(
        reset.status.success(),
        "reset exit={:?} stderr={}",
        reset.status.code(),
        String::from_utf8_lossy(&reset.stderr)
    );
    let row: Value = serde_json::from_slice(&reset.stdout).expect("reset JSON");
    assert_eq!(row["status"], "active");
    assert!(
        row.get("last_error").is_none() || row["last_error"].is_null(),
        "last_error should be cleared after reset; got {row}"
    );

    daemon.shutdown().await;
}

#[tokio::test]
async fn hmn_vault_reset_with_rebuild_yes_succeeds() {
    let daemon = spawn_vault_cli_daemon().await;
    let vault_path = fresh_vault_dir(daemon.root.path(), "v");

    let create = run_hmn(
        &daemon.cfg_path,
        &daemon.base_url,
        &[
            "--json",
            "vault",
            "create",
            "--name",
            "rb",
            vault_path.to_str().unwrap(),
        ],
    )
    .await;
    assert!(create.status.success());

    let reset = run_hmn(
        &daemon.cfg_path,
        &daemon.base_url,
        &["--json", "vault", "reset", "rb", "--rebuild", "--yes"],
    )
    .await;
    assert!(
        reset.status.success(),
        "reset --rebuild --yes exit={:?} stderr={}",
        reset.status.code(),
        String::from_utf8_lossy(&reset.stderr)
    );
    let row: Value = serde_json::from_slice(&reset.stdout).expect("reset JSON");
    assert_eq!(row["status"], "active");
    assert_eq!(row["name"], "rb");

    daemon.shutdown().await;
}

#[tokio::test]
async fn hmn_vault_rescan_with_yes_returns_rescan_initiated_at() {
    let daemon = spawn_vault_cli_daemon().await;
    let vault_path = fresh_vault_dir(daemon.root.path(), "v");

    let create = run_hmn(
        &daemon.cfg_path,
        &daemon.base_url,
        &[
            "--json",
            "vault",
            "create",
            "--name",
            "rsc",
            vault_path.to_str().unwrap(),
        ],
    )
    .await;
    assert!(create.status.success());

    let rescan = run_hmn(
        &daemon.cfg_path,
        &daemon.base_url,
        &["--json", "vault", "rescan", "rsc", "--yes"],
    )
    .await;
    assert!(
        rescan.status.success(),
        "rescan exit={:?} stderr={}",
        rescan.status.code(),
        String::from_utf8_lossy(&rescan.stderr)
    );
    let body: Value = serde_json::from_slice(&rescan.stdout).expect("rescan JSON");
    let rescan_initiated_at = body["rescan_initiated_at"]
        .as_str()
        .expect("rescan_initiated_at is a string");
    assert!(
        !rescan_initiated_at.is_empty(),
        "rescan_initiated_at must be non-empty; got {body}"
    );
    // Spec wire shape is ISO-8601 micros UTC; sanity-check the trailing Z.
    assert!(
        rescan_initiated_at.ends_with('Z'),
        "expected ISO-8601 UTC timestamp ending in Z; got {rescan_initiated_at}"
    );
    assert_eq!(body["name"], "rsc");

    daemon.shutdown().await;
}

#[tokio::test]
async fn hmn_vault_rescan_without_yes_prompts_and_aborts_on_no() {
    let daemon = spawn_vault_cli_daemon().await;
    let vault_path = fresh_vault_dir(daemon.root.path(), "v");

    let create = run_hmn(
        &daemon.cfg_path,
        &daemon.base_url,
        &[
            "--json",
            "vault",
            "create",
            "--name",
            "rscno",
            vault_path.to_str().unwrap(),
        ],
    )
    .await;
    assert!(create.status.success());

    // Pipe `n\n` to stdin; no --yes flag → confirmation prompt fires; the
    // process must exit 0 without hitting the daemon's rescan route.
    let cfg_path = daemon.cfg_path.clone();
    let base_url = daemon.base_url.clone();
    let resp = tokio::task::spawn_blocking(move || {
        let mut child = Command::new(env!("CARGO_BIN_EXE_hmn"))
            .arg("--config")
            .arg(&cfg_path)
            .arg("--daemon-url")
            .arg(&base_url)
            .args(["vault", "rescan", "rscno"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn hmn vault rescan");
        {
            let mut stdin = child.stdin.take().expect("hmn stdin");
            stdin.write_all(b"n\n").expect("write stdin");
        }
        child.wait_with_output().expect("hmn wait")
    })
    .await
    .expect("spawn_blocking join");

    assert!(
        resp.status.success(),
        "no-on-prompt should exit 0; got {:?} stderr={}",
        resp.status.code(),
        String::from_utf8_lossy(&resp.stderr)
    );
    let stdout = String::from_utf8_lossy(&resp.stdout);
    let stderr = String::from_utf8_lossy(&resp.stderr);
    assert!(
        stdout.contains("aborted"),
        "stdout should announce abort; got stdout={stdout:?} stderr={stderr:?}",
    );
    assert!(
        stderr.contains("Rescan vault 'rscno'? This will re-emit outbox events. (y/N) "),
        "prompt should appear on stderr; got {stderr:?}",
    );

    daemon.shutdown().await;
}

#[tokio::test]
async fn hmn_status_when_daemon_unreachable_exits_4() {
    // Bind once to grab a kernel-assigned free port, then drop the listener so
    // nothing is listening when `hmn` connects. Using a deliberately unbound
    // port (rather than a started-then-stopped daemon) avoids the race where
    // the daemon hasn't bound yet on a slow CI host.
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    drop(listener);

    let fx = fixture();
    let url = format!("http://{addr}");
    let out = run_hmn(&fx.cfg_path, &url, &["status"]).await;
    assert_eq!(
        out.status.code(),
        Some(4),
        "expected exit 4 (daemon unreachable); got {:?}; stderr={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("daemon not reachable"),
        "stderr must mention `daemon not reachable`; got {stderr}"
    );
}
