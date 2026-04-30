use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use hypomnema::api::{self, ApiState, VaultEntry};
use hypomnema::config::Config;
use hypomnema::control_plane::VaultManager;
use hypomnema::embedding::{Embedder, StubEmbedder};
use hypomnema::indexer::Scanner;
use hypomnema::store::Store;
use hypomnema::vault_registry::VaultId;
use hypomnema::vault_registry::VaultStatus;
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio::sync::watch;
use tokio::task::JoinHandle;

struct Fixture {
    _root: TempDir,
    vault: PathBuf,
    data_dir: PathBuf,
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
        shutdown: tx,
        handle: Some(handle),
        _fx: fx,
    }
}

fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("reqwest client")
}

fn seed_default_vault(fx: &Fixture) {
    write_file(fx, "alpha.md", b"alpha\n");
    write_file(fx, "beta.md", b"bravo\n");
    write_file(fx, "notes/gamma.md", b"gamma\n");
    write_file(fx, "notes/delta.md", b"delta\n");
    write_file(
        fx,
        "notes/sub/multi.md",
        b"line one\nline two contains Pgvector here\nline three\n",
    );
}

#[tokio::test]
async fn health_endpoint_reachable() {
    let fx = fixture();
    seed_default_vault(&fx);
    let daemon = spawn_live_daemon(fx).await;

    let body: Value = http()
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
}

#[tokio::test]
async fn status_reports_vault_count_last_indexed_outbox() {
    let fx = fixture();
    let expected_vault = fx.vault.display().to_string();
    seed_default_vault(&fx);
    let daemon = spawn_live_daemon(fx).await;

    let resp = http()
        .get(format!("{}/status", daemon.base_url))
        .send()
        .await
        .expect("GET /status");
    assert!(resp.status().is_success(), "/status not 2xx: {resp:?}");
    let body: Value = resp.json().await.expect("/status JSON");

    assert_eq!(body["vault"], expected_vault);
    assert_eq!(body["indexed_file_count"], 5);
    let last = body["last_indexed_at"]
        .as_str()
        .expect("last_indexed_at should be a string after a scan");
    chrono::DateTime::parse_from_rfc3339(last)
        .unwrap_or_else(|e| panic!("last_indexed_at must be RFC3339, got {last}: {e}"));
    assert_eq!(body["outbox"]["path"], "");
    assert_eq!(body["outbox"]["size_bytes"], 0);

    daemon.shutdown().await;
}

#[tokio::test]
async fn filesystem_search_with_glob_returns_matches() {
    let fx = fixture();
    seed_default_vault(&fx);
    let daemon = spawn_live_daemon(fx).await;

    let body: Value = http()
        .post(format!("{}/search/filesystem", daemon.base_url))
        .json(&json!({ "glob": "**/*.md" }))
        .send()
        .await
        .expect("POST /search/filesystem")
        .error_for_status()
        .expect("filesystem 2xx")
        .json()
        .await
        .expect("filesystem JSON");
    let paths: Vec<&str> = body["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["path"].as_str().unwrap())
        .collect();
    assert_eq!(
        paths,
        vec![
            "alpha.md",
            "beta.md",
            "notes/delta.md",
            "notes/gamma.md",
            "notes/sub/multi.md",
        ]
    );

    let body: Value = http()
        .post(format!("{}/search/filesystem", daemon.base_url))
        .json(&json!({ "glob": "notes/*.md" }))
        .send()
        .await
        .expect("POST /search/filesystem nested")
        .error_for_status()
        .expect("filesystem nested 2xx")
        .json()
        .await
        .expect("filesystem nested JSON");
    let paths: Vec<&str> = body["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["path"].as_str().unwrap())
        .collect();
    // globset's default `*` matches across `/`, so `notes/*.md` matches every
    // .md beneath `notes/`, including the doubly-nested `notes/sub/multi.md`.
    assert_eq!(
        paths,
        vec!["notes/delta.md", "notes/gamma.md", "notes/sub/multi.md"]
    );

    daemon.shutdown().await;
}

#[tokio::test]
async fn filesystem_search_with_prefix_narrows_results() {
    let fx = fixture();
    seed_default_vault(&fx);
    let daemon = spawn_live_daemon(fx).await;

    let body: Value = http()
        .post(format!("{}/search/filesystem", daemon.base_url))
        .json(&json!({ "prefix": "notes/" }))
        .send()
        .await
        .expect("POST /search/filesystem prefix")
        .error_for_status()
        .expect("prefix 2xx")
        .json()
        .await
        .expect("prefix JSON");
    let paths: Vec<&str> = body["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["path"].as_str().unwrap())
        .collect();
    assert_eq!(
        paths,
        vec!["notes/delta.md", "notes/gamma.md", "notes/sub/multi.md"]
    );

    daemon.shutdown().await;
}

#[tokio::test]
async fn filesystem_search_invalid_glob_returns_400_with_code() {
    let fx = fixture();
    seed_default_vault(&fx);
    let daemon = spawn_live_daemon(fx).await;

    let resp = http()
        .post(format!("{}/search/filesystem", daemon.base_url))
        .json(&json!({ "glob": "[" }))
        .send()
        .await
        .expect("POST /search/filesystem invalid");
    assert_eq!(resp.status().as_u16(), 400);
    let body: Value = resp.json().await.expect("400 JSON");
    assert_eq!(body["error"]["code"], "invalid_glob");

    daemon.shutdown().await;
}

#[tokio::test]
async fn content_search_substring_is_case_insensitive_by_default() {
    let fx = fixture();
    seed_default_vault(&fx);
    let daemon = spawn_live_daemon(fx).await;

    let body: Value = http()
        .post(format!("{}/search/content", daemon.base_url))
        .json(&json!({ "query": "pgvector" }))
        .send()
        .await
        .expect("POST /search/content default")
        .error_for_status()
        .expect("content 2xx")
        .json()
        .await
        .expect("content JSON");
    let paths: Vec<&str> = body["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, vec!["notes/sub/multi.md"]);

    let body: Value = http()
        .post(format!("{}/search/content", daemon.base_url))
        .json(&json!({ "query": "pgvector", "case_sensitive": true }))
        .send()
        .await
        .expect("POST /search/content sensitive")
        .error_for_status()
        .expect("content sensitive 2xx")
        .json()
        .await
        .expect("content sensitive JSON");
    assert!(body["results"].as_array().unwrap().is_empty());

    daemon.shutdown().await;
}

#[tokio::test]
async fn content_search_regex_matches_alternation() {
    let fx = fixture();
    write_file(&fx, "a.md", b"the quick foo jumps\n");
    write_file(&fx, "b.md", b"a wandering bar appears\n");
    write_file(&fx, "c.md", b"neither word here\n");
    let daemon = spawn_live_daemon(fx).await;

    let body: Value = http()
        .post(format!("{}/search/content", daemon.base_url))
        .json(&json!({ "query": "foo|bar", "regex": true }))
        .send()
        .await
        .expect("POST /search/content regex")
        .error_for_status()
        .expect("regex 2xx")
        .json()
        .await
        .expect("regex JSON");
    let paths: Vec<&str> = body["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, vec!["a.md", "b.md"]);

    daemon.shutdown().await;
}

#[tokio::test]
async fn content_search_returns_line_and_text_per_match() {
    let fx = fixture();
    seed_default_vault(&fx);
    let daemon = spawn_live_daemon(fx).await;

    let body: Value = http()
        .post(format!("{}/search/content", daemon.base_url))
        .json(&json!({ "query": "Pgvector", "case_sensitive": true }))
        .send()
        .await
        .expect("POST /search/content match")
        .error_for_status()
        .expect("match 2xx")
        .json()
        .await
        .expect("match JSON");
    let results = body["results"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    let matches = results[0]["matches"].as_array().unwrap();
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0]["line"], 2);
    assert_eq!(matches[0]["text"], "line two contains Pgvector here");

    daemon.shutdown().await;
}

#[tokio::test]
async fn content_search_invalid_regex_returns_400_with_code() {
    let fx = fixture();
    seed_default_vault(&fx);
    let daemon = spawn_live_daemon(fx).await;

    let resp = http()
        .post(format!("{}/search/content", daemon.base_url))
        .json(&json!({ "query": "(", "regex": true }))
        .send()
        .await
        .expect("POST /search/content bad regex");
    assert_eq!(resp.status().as_u16(), 400);
    let body: Value = resp.json().await.expect("400 JSON");
    assert_eq!(body["error"]["code"], "invalid_regex");

    daemon.shutdown().await;
}

#[tokio::test]
async fn content_search_phrase_across_line_boundary_matches() {
    let fx = fixture();
    write_file(&fx, "split.md", b"foo\nbar\n");
    let daemon = spawn_live_daemon(fx).await;

    let body: Value = http()
        .post(format!("{}/search/content", daemon.base_url))
        .json(&json!({ "query": "foo\\sbar", "regex": true }))
        .send()
        .await
        .expect("POST /search/content cross-line")
        .error_for_status()
        .expect("cross-line 2xx")
        .json()
        .await
        .expect("cross-line JSON");
    let results = body["results"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["path"], "split.md");
    assert_eq!(results[0]["match_count"], 1);

    daemon.shutdown().await;
}

#[tokio::test]
async fn filesystem_search_truncated_when_limit_below_total() {
    let fx = fixture();
    write_file(&fx, "a.md", b"a\n");
    write_file(&fx, "b.md", b"b\n");
    write_file(&fx, "c.md", b"c\n");
    let daemon = spawn_live_daemon(fx).await;

    let body: Value = http()
        .post(format!("{}/search/filesystem", daemon.base_url))
        .json(&json!({ "glob": "**/*.md", "limit": 2 }))
        .send()
        .await
        .expect("POST /search/filesystem truncated")
        .error_for_status()
        .expect("truncated 2xx")
        .json()
        .await
        .expect("truncated JSON");
    assert_eq!(body["results"].as_array().unwrap().len(), 2);
    assert_eq!(body["truncated"], true);

    daemon.shutdown().await;
}

#[tokio::test]
async fn search_responses_populate_vault_and_vault_name() {
    // Step 9: every search result carries `vault` (id) + `vault_name`.
    let fx = fixture();
    seed_default_vault(&fx);
    let expected_id = fx.vault_id.to_string();
    let daemon = spawn_live_daemon(fx).await;

    let body: Value = http()
        .post(format!("{}/search/filesystem", daemon.base_url))
        .json(&json!({}))
        .send()
        .await
        .expect("POST /search/filesystem")
        .error_for_status()
        .expect("2xx")
        .json()
        .await
        .expect("JSON");
    for entry in body["results"].as_array().unwrap() {
        assert_eq!(entry["vault"].as_str(), Some(expected_id.as_str()));
        assert_eq!(entry["vault_name"].as_str(), Some("test"));
    }

    let body: Value = http()
        .post(format!("{}/search/content", daemon.base_url))
        .json(&json!({ "query": "pgvector" }))
        .send()
        .await
        .expect("POST /search/content")
        .error_for_status()
        .expect("2xx")
        .json()
        .await
        .expect("JSON");
    for entry in body["results"].as_array().unwrap() {
        assert_eq!(entry["vault"].as_str(), Some(expected_id.as_str()));
        assert_eq!(entry["vault_name"].as_str(), Some("test"));
    }

    daemon.shutdown().await;
}

#[tokio::test]
async fn graceful_shutdown_closes_listener() {
    let fx = fixture();
    seed_default_vault(&fx);
    let daemon = spawn_live_daemon(fx).await;
    let url = format!("{}/health", daemon.base_url);

    // Sanity: the daemon answers before shutdown.
    let resp = http().get(&url).send().await.expect("pre-shutdown GET");
    assert!(resp.status().is_success(), "pre-shutdown not 2xx: {resp:?}");

    // Send shutdown and await the server task to exit.
    let LiveDaemon {
        shutdown,
        mut handle,
        ..
    } = daemon;
    let _ = shutdown.send(true);
    if let Some(h) = handle.take() {
        let _ = h.await;
    }

    // Use a fresh client per attempt so reqwest doesn't surface a cached
    // keep-alive connection from the pre-shutdown request as a success.
    let mut last_err: Option<reqwest::Error> = None;
    let mut closed = false;
    for _ in 0..20 {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .pool_max_idle_per_host(0)
            .build()
            .expect("reqwest client");
        match client.get(&url).send().await {
            Ok(r) => {
                // Server might still be tearing down on the very first attempt
                // — keep trying until the listener is gone.
                drop(r);
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            Err(e) => {
                last_err = Some(e);
                closed = true;
                break;
            }
        }
    }
    assert!(
        closed,
        "expected post-shutdown GET to fail; last error: {last_err:?}"
    );
}
