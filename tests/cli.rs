use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use hypomnema::api::{self, ApiState};
use hypomnema::config::Config;
use hypomnema::embedding::{Embedder, StubEmbedder};
use hypomnema::indexer::Scanner;
use hypomnema::store::Store;
use hypomnema::vault_registry::VaultId;
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
    let vault = config.vault.0.clone();
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

    let outbox_path = fx.data_dir.join(&fx.config.storage.outbox_file);
    let state = ApiState {
        pool: store.pool(),
        vault: fx.vault.clone(),
        outbox_path,
        embedder,
        embedding_dimension: fx.config.embedding.dimension,
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
