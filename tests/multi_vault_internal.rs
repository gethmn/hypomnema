//! Step 9 multi-vault integration tests.
//!
//! These exercise per-vault internals via direct registry manipulation, since
//! step 10's `hmn vault` user surface does not exist yet. Three of the five
//! tests have unit-level counterparts in `src/legacy_state_migration.rs`; the
//! integration versions add cross-cutting daemon-spawn + HTTP-probe validation
//! that the unit tests cannot reach (real Store/Scanner/Watcher/Outbox/Axum
//! stack instead of mocked file mechanics).
//!
//! **Step-9-ahead-of-spec gap**: this file pins the wire shape with
//! populated `vault` + `vault_name` fields on every search response, while the
//! four search/event specs (`docs/specs/{filesystem-search,content-search,
//! semantic-search,change-events}.md`) still say "always absent in v0" through
//! step 9's ship date. Step 10's workplan-write phase (Solo todo 64) closes
//! the gap by amending the specs. Until then, these tests are the canonical
//! source of truth for the populated wire shape.

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
use hypomnema::embedding::{Embedder, StubEmbedder};
use hypomnema::events::{EventBus, StreamEvent};
use hypomnema::indexer::Scanner;
use hypomnema::legacy_state_migration;
use hypomnema::store::Store;
use hypomnema::vault_registry::{VaultId, VaultRegistry, VaultRow, VaultStatus, vault_data_dir};
use hypomnema::watcher::{self, Watcher};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, watch};
use tokio::task::JoinHandle;

const DEBOUNCE_MS: u64 = 50;
const SETTLE: Duration = Duration::from_millis(4 * DEBOUNCE_MS);

fn make_config(data_dir: PathBuf) -> Config {
    Config {
        vault: None,
        http: HttpConfig::default(),
        mcp: McpConfig::default(),
        embedding: EmbeddingConfig::default(),
        watcher: WatcherConfig {
            debounce_ms: DEBOUNCE_MS,
            ..WatcherConfig::default()
        },
        storage: StorageConfig {
            data_dir: ConfigPath(data_dir),
            index_file: "index.sqlite".to_string(),
            outbox_file: "outbox.jsonl".to_string(),
        },
        logging: LoggingConfig::default(),
        default_vault_name: "default".to_string(),
    }
}

fn make_legacy_config(vault: PathBuf, data_dir: PathBuf) -> Config {
    let mut cfg = make_config(data_dir);
    cfg.vault = Some(ConfigPath(vault));
    cfg
}

async fn open_store(vault_id: &VaultId, config: &Config) -> Store {
    Store::open(
        vault_id,
        &config.storage.data_dir.0,
        &config.storage.index_file,
        &config.embedding,
    )
    .await
    .expect("open store")
}

async fn build_vault_entry(
    row: &VaultRow,
    config: &Config,
    embedder: Arc<dyn Embedder>,
) -> VaultEntry {
    let store = open_store(&row.id, config).await;
    let scanner =
        Scanner::new(&row.path, config, &store, embedder.clone()).expect("construct scanner");
    let _ = scanner.run().await.expect("initial scan");

    VaultEntry {
        id: row.id.clone(),
        name: row.name.clone(),
        vault_path: row.path.clone(),
        store: Arc::new(store),
        status: row.status,
    }
}

fn build_test_state(
    entries: Vec<VaultEntry>,
    embedder: Arc<dyn Embedder>,
    embedding_dimension: u32,
) -> ApiState {
    ApiState {
        vault_manager: Arc::new(VaultManager::for_tests(
            entries,
            embedder,
            embedding_dimension,
        )),
    }
}

struct LiveDaemon {
    base_url: String,
    shutdown: watch::Sender<bool>,
    handle: Option<JoinHandle<()>>,
}

impl LiveDaemon {
    async fn shutdown(mut self) {
        let _ = self.shutdown.send(true);
        if let Some(h) = self.handle.take() {
            let _ = h.await;
        }
    }
}

async fn spawn_daemon(state: ApiState) -> LiveDaemon {
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
    }
}

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("reqwest client")
}

struct WatcherRuntime {
    _watcher: Watcher,
    shutdown_tx: watch::Sender<bool>,
    _rescan_tx: watch::Sender<u64>,
    consumer: JoinHandle<()>,
    event_bus: Arc<EventBus>,
}

impl WatcherRuntime {
    fn subscribe(&self) -> broadcast::Receiver<StreamEvent> {
        self.event_bus.subscribe()
    }

    async fn shutdown(self) {
        let _ = self.shutdown_tx.send(true);
        let _ = self.consumer.await;
        drop(self._watcher);
    }
}

async fn spawn_watcher_runtime(
    row: &VaultRow,
    config: &Config,
    embedder: Arc<dyn Embedder>,
) -> WatcherRuntime {
    let store = open_store(&row.id, config).await;
    let scanner =
        Scanner::new(&row.path, config, &store, embedder.clone()).expect("construct scanner");
    let _ = scanner.run().await.expect("initial scan");

    let ignores = config.watcher.compiled_ignores().expect("compile ignores");
    let (watcher_handle, rx) = watcher::spawn_watcher(
        &row.id,
        &row.path,
        ignores,
        Duration::from_millis(config.watcher.debounce_ms),
        256,
    )
    .expect("spawn watcher");

    let scanner_for_consumer =
        Scanner::new(&row.path, config, &store, embedder).expect("construct scanner (consumer)");
    let event_bus = Arc::new(EventBus::new());
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let (rescan_tx, rescan_rx) = watch::channel(0u64);
    let consumer = tokio::spawn(watcher::run_consumer(
        rx,
        scanner_for_consumer,
        row.id.clone(),
        event_bus.clone(),
        shutdown_rx,
        rescan_rx,
    ));

    WatcherRuntime {
        _watcher: watcher_handle,
        shutdown_tx,
        _rescan_tx: rescan_tx,
        consumer,
        event_bus,
    }
}

fn drain_events(rx: &mut broadcast::Receiver<StreamEvent>) -> Vec<StreamEvent> {
    let mut events = Vec::new();
    loop {
        match rx.try_recv() {
            Ok(ev) => events.push(ev),
            Err(broadcast::error::TryRecvError::Empty) => break,
            Err(e) => panic!("event receive failed: {e:?}"),
        }
    }
    events
}

async fn insert_active_vault(registry: &VaultRegistry, name: &str, path: PathBuf) -> VaultRow {
    let row = VaultRow {
        id: VaultId::new(),
        name: name.to_string(),
        path,
        status: VaultStatus::Active,
        created_at: Utc::now(),
        last_error: None,
    };
    registry
        .insert(row.clone())
        .await
        .expect("insert vault row");
    row
}

// ===== Test 1 — two vaults index in isolation =====

#[tokio::test]
async fn two_vaults_index_in_isolation() {
    let root = TempDir::new().expect("tempdir");
    let vault_a = root.path().join("vault_a");
    let vault_b = root.path().join("vault_b");
    fs::create_dir_all(&vault_a).unwrap();
    fs::create_dir_all(&vault_b).unwrap();
    fs::write(vault_a.join("a.md"), b"# a-initial\n").unwrap();
    fs::write(vault_b.join("b.md"), b"# b-initial\n").unwrap();

    let data_dir = root.path().join("data");
    let config = make_config(data_dir.clone());
    let registry = VaultRegistry::open(&data_dir).await.expect("open registry");

    let row_a = insert_active_vault(&registry, "vault-a", vault_a.clone()).await;
    let row_b = insert_active_vault(&registry, "vault-b", vault_b.clone()).await;

    let embedder: Arc<dyn Embedder> = Arc::new(StubEmbedder::new(768));
    let rt_a = spawn_watcher_runtime(&row_a, &config, embedder.clone()).await;
    let rt_b = spawn_watcher_runtime(&row_b, &config, embedder.clone()).await;
    let mut rx_a = rt_a.subscribe();
    let mut rx_b = rt_b.subscribe();

    // Initial scans must not publish live watcher events.
    assert!(drain_events(&mut rx_a).is_empty());
    assert!(drain_events(&mut rx_b).is_empty());

    // Modify a file only in vault A.
    fs::write(vault_a.join("a.md"), b"# a-modified-bytes\n").unwrap();
    tokio::time::sleep(SETTLE).await;

    let events_a = drain_events(&mut rx_a);
    let events_b = drain_events(&mut rx_b);
    assert!(
        !events_a.is_empty(),
        "vault A's bus should publish at least one event"
    );
    assert_eq!(
        events_b.len(),
        0,
        "vault B's bus must not publish events when only vault A changed"
    );

    // Every event in A's stream carries A's vault id.
    assert!(
        events_a.iter().all(|ev| match ev {
            StreamEvent::FileChanged(ev) => ev.vault == row_a.id,
            StreamEvent::StreamLagged(_) => false,
        }),
        "all vault A live events must carry vault A's id; got {events_a:?}"
    );

    rt_a.shutdown().await;
    rt_b.shutdown().await;
}

// ===== Test 2 — cross-vault search returns intermingled results =====

#[tokio::test]
async fn cross_vault_search_returns_intermingled_results_with_vault_id() {
    let root = TempDir::new().expect("tempdir");
    let vault_a = root.path().join("vault_a");
    let vault_b = root.path().join("vault_b");
    fs::create_dir_all(&vault_a).unwrap();
    fs::create_dir_all(&vault_b).unwrap();
    fs::write(vault_a.join("alpha.md"), b"alpha\n").unwrap();
    fs::write(vault_a.join("apricot.md"), b"apricot\n").unwrap();
    fs::write(vault_b.join("bravo.md"), b"bravo\n").unwrap();
    fs::write(vault_b.join("blueberry.md"), b"blueberry\n").unwrap();

    let data_dir = root.path().join("data");
    let config = make_config(data_dir.clone());
    let registry = VaultRegistry::open(&data_dir).await.expect("open registry");

    let row_a = insert_active_vault(&registry, "alpha-vault", vault_a.clone()).await;
    let row_b = insert_active_vault(&registry, "bravo-vault", vault_b.clone()).await;

    let embedder: Arc<dyn Embedder> = Arc::new(StubEmbedder::new(768));
    let entry_a = build_vault_entry(&row_a, &config, embedder.clone()).await;
    let entry_b = build_vault_entry(&row_b, &config, embedder.clone()).await;

    let state = build_test_state(vec![entry_a, entry_b], embedder, config.embedding.dimension);
    let daemon = spawn_daemon(state).await;

    let body: Value = http_client()
        .post(format!("{}/search/filesystem", daemon.base_url))
        .json(&json!({}))
        .send()
        .await
        .expect("POST /search/filesystem")
        .error_for_status()
        .expect("/search/filesystem 2xx")
        .json()
        .await
        .expect("/search/filesystem JSON");

    let expected_a_id = row_a.id.to_string();
    let expected_b_id = row_b.id.to_string();
    let mut a_seen: Vec<&str> = Vec::new();
    let mut b_seen: Vec<&str> = Vec::new();
    for entry in body["results"].as_array().expect("results array") {
        let path = entry["path"].as_str().expect("result.path");
        let vault = entry["vault"].as_str().expect("result.vault");
        let vault_name = entry["vault_name"].as_str().expect("result.vault_name");
        if vault == expected_a_id {
            assert_eq!(vault_name, "alpha-vault");
            a_seen.push(path);
        } else if vault == expected_b_id {
            assert_eq!(vault_name, "bravo-vault");
            b_seen.push(path);
        } else {
            panic!("unexpected vault id {vault} on result {path}");
        }
    }
    a_seen.sort();
    b_seen.sort();
    assert_eq!(a_seen, vec!["alpha.md", "apricot.md"]);
    assert_eq!(b_seen, vec!["blueberry.md", "bravo.md"]);

    daemon.shutdown().await;
}

// ===== Test 3 — legacy state migration preserves index =====

#[tokio::test]
async fn legacy_state_migration_preserves_index() {
    let root = TempDir::new().expect("tempdir");
    let vault = root.path().join("vault");
    fs::create_dir_all(&vault).unwrap();
    fs::write(vault.join("note-one.md"), b"first legacy note\n").unwrap();
    fs::write(vault.join("note-two.md"), b"second legacy note\n").unwrap();

    let data_dir = root.path().join("data");
    let config = make_legacy_config(vault.clone(), data_dir.clone());

    // Phase 1: build a real index at a temp vault id, populate via scanner.
    let temp_id = VaultId::new();
    {
        let store = open_store(&temp_id, &config).await;
        let embedder: Arc<dyn Embedder> = Arc::new(StubEmbedder::new(768));
        let scanner = Scanner::new(&vault, &config, &store, embedder).expect("construct scanner");
        let _ = scanner.run().await.expect("initial scan");
    }

    // Phase 2: relocate the populated SQLite + outbox into the v0.1.0 layout
    // (`<data_dir>/index.sqlite`, etc.). The temp vault subdir + an unused
    // vaults.sqlite from the temp open are removed so legacy_state_migration
    // sees an empty registry.
    let temp_vault_dir = vault_data_dir(&data_dir, &temp_id);
    let temp_index = temp_vault_dir.join("index.sqlite");
    let legacy_index = data_dir.join("index.sqlite");
    fs::rename(&temp_index, &legacy_index).expect("relocate index.sqlite");
    for sidecar in &["index.sqlite-wal", "index.sqlite-shm"] {
        let src = temp_vault_dir.join(sidecar);
        if src.exists() {
            fs::rename(&src, data_dir.join(sidecar)).expect("relocate sqlite sidecar");
        }
    }
    fs::write(data_dir.join("outbox.jsonl"), b"").expect("write empty legacy outbox");
    fs::remove_dir_all(&temp_vault_dir).expect("remove temp vault dir");
    let _ = fs::remove_file(data_dir.join("vaults.sqlite"));
    let _ = fs::remove_file(data_dir.join("vaults.sqlite-wal"));
    let _ = fs::remove_file(data_dir.join("vaults.sqlite-shm"));

    assert!(
        legacy_index.is_file(),
        "legacy index.sqlite present pre-migration"
    );

    // Phase 3: open a fresh registry and run the migration.
    let registry = VaultRegistry::open(&data_dir).await.expect("open registry");
    legacy_state_migration::run_if_needed(&config, &registry)
        .await
        .expect("run legacy_state_migration");

    let rows = registry.list_active().await.expect("list active rows");
    assert_eq!(rows.len(), 1, "exactly one auto-migrated row");
    let row = rows.into_iter().next().unwrap();
    assert_eq!(row.name, config.default_vault_name);
    // legacy_state_migration::run_if_needed canonicalizes the vault path
    // (best-effort) before storing it on the row, so compare against the
    // canonical form rather than the configured path. On macOS this resolves
    // /var/... -> /private/var/... .
    let expected_path = fs::canonicalize(&vault).unwrap_or_else(|_| vault.clone());
    assert_eq!(row.path, expected_path);

    let migrated_dir = vault_data_dir(&data_dir, &row.id);
    assert!(
        migrated_dir.join("index.sqlite").is_file(),
        "index.sqlite renamed into per-vault subdir"
    );
    assert!(
        !legacy_index.exists(),
        "legacy <data_dir>/index.sqlite no longer present"
    );

    // Phase 4: spin up an HTTP daemon over the migrated row and confirm the
    // pre-migration files are searchable. This is the integration-level
    // counterpart to the file-mechanics-only unit test in
    // src/legacy_state_migration.rs.
    let embedder: Arc<dyn Embedder> = Arc::new(StubEmbedder::new(768));
    let entry = build_vault_entry(&row, &config, embedder.clone()).await;
    let state = build_test_state(vec![entry], embedder, config.embedding.dimension);
    let daemon = spawn_daemon(state).await;

    let body: Value = http_client()
        .post(format!("{}/search/filesystem", daemon.base_url))
        .json(&json!({}))
        .send()
        .await
        .expect("POST /search/filesystem")
        .error_for_status()
        .expect("/search/filesystem 2xx")
        .json()
        .await
        .expect("/search/filesystem JSON");

    let mut paths: Vec<&str> = body["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["path"].as_str().unwrap())
        .collect();
    paths.sort();
    assert_eq!(paths, vec!["note-one.md", "note-two.md"]);

    let expected_id = row.id.to_string();
    for entry in body["results"].as_array().unwrap() {
        assert_eq!(entry["vault"].as_str(), Some(expected_id.as_str()));
        assert_eq!(entry["vault_name"].as_str(), Some(row.name.as_str()));
    }

    daemon.shutdown().await;
}

// ===== Test 4 — migration idempotent under crash window =====

#[tokio::test]
async fn migration_idempotent_under_crash_window() {
    let root = TempDir::new().expect("tempdir");
    let vault = root.path().join("vault");
    fs::create_dir_all(&vault).unwrap();
    fs::write(vault.join("survivor.md"), b"survivor\n").unwrap();

    let data_dir = root.path().join("data");
    let config = make_legacy_config(vault.clone(), data_dir.clone());

    // Phase 1: pre-populate a real Store at a known vault_id_a (final target).
    let vault_id = VaultId::new();
    {
        let store = open_store(&vault_id, &config).await;
        let embedder: Arc<dyn Embedder> = Arc::new(StubEmbedder::new(768));
        let scanner = Scanner::new(&vault, &config, &store, embedder).expect("construct scanner");
        let _ = scanner.run().await.expect("initial scan");
    }

    // Phase 2: simulate the crash-mid-rename state per workplan § Task 9.7.
    // Registry row is already inserted; <data_dir>/vaults/<id>/index.sqlite
    // is in place (final destination); a stray legacy file is still at
    // <data_dir>/index.sqlite-wal awaiting its rename.
    let registry = VaultRegistry::open(&data_dir).await.expect("open registry");
    let row = VaultRow {
        id: vault_id.clone(),
        name: config.default_vault_name.clone(),
        path: vault.clone(),
        status: VaultStatus::Active,
        created_at: Utc::now(),
        last_error: None,
    };
    registry.insert(row.clone()).await.expect("insert row");

    fs::write(data_dir.join("index.sqlite-wal"), b"legacy-wal-bytes")
        .expect("write stray legacy wal");

    // Phase 3: re-run the migration. The rename pass should complete the
    // outstanding move without disturbing the already-relocated index file.
    legacy_state_migration::run_if_needed(&config, &registry)
        .await
        .expect("run legacy_state_migration after partial crash");

    let target_dir = vault_data_dir(&data_dir, &vault_id);
    assert!(
        target_dir.join("index.sqlite").is_file(),
        "index.sqlite intact"
    );
    assert!(
        target_dir.join("index.sqlite-wal").is_file(),
        "index.sqlite-wal completed rename into vault dir"
    );
    assert!(
        !data_dir.join("index.sqlite-wal").exists(),
        "legacy <data_dir>/index.sqlite-wal cleaned up"
    );

    // Phase 4: registry should still hold exactly one row (the pre-existing
    // one); the migration must NOT have inserted a second row when one was
    // already present.
    let rows = registry.list_active().await.expect("list active rows");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, vault_id);

    // Phase 5: spin up the daemon and confirm the index round-trips through
    // HTTP search after the recovered migration.
    let embedder: Arc<dyn Embedder> = Arc::new(StubEmbedder::new(768));
    let entry = build_vault_entry(&row, &config, embedder.clone()).await;
    let state = build_test_state(vec![entry], embedder, config.embedding.dimension);
    let daemon = spawn_daemon(state).await;

    let body: Value = http_client()
        .post(format!("{}/search/filesystem", daemon.base_url))
        .json(&json!({}))
        .send()
        .await
        .expect("POST /search/filesystem")
        .error_for_status()
        .expect("/search/filesystem 2xx")
        .json()
        .await
        .expect("/search/filesystem JSON");
    let paths: Vec<&str> = body["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, vec!["survivor.md"]);

    daemon.shutdown().await;
}

// ===== Test 5 — errored vault returns empty search results =====

#[tokio::test]
async fn errored_vault_returns_empty_search_results() {
    let root = TempDir::new().expect("tempdir");
    let data_dir = root.path().join("data");
    let bogus_path = root.path().join("does_not_exist");
    let config = make_config(data_dir.clone());

    let registry = VaultRegistry::open(&data_dir).await.expect("open registry");
    // Insert an `errored` row directly. The daemon's reconcile pass would
    // produce the same shape from an active row whose path went away.
    let row = VaultRow {
        id: VaultId::new(),
        name: "broken-vault".to_string(),
        path: bogus_path,
        status: VaultStatus::Errored,
        created_at: Utc::now(),
        last_error: Some("simulated path-vanished".to_string()),
    };
    registry.insert(row).await.expect("insert errored row");

    // The daemon constructs ApiState from `list_active()`, which excludes
    // errored rows — so the live API surface sees zero vaults.
    let active = registry.list_active().await.expect("list_active");
    assert!(
        active.is_empty(),
        "errored row must not appear in list_active()"
    );

    let embedder: Arc<dyn Embedder> = Arc::new(StubEmbedder::new(768));
    let state = build_test_state(
        Vec::<VaultEntry>::new(),
        embedder,
        config.embedding.dimension,
    );
    let daemon = spawn_daemon(state).await;

    let resp = http_client()
        .post(format!("{}/search/filesystem", daemon.base_url))
        .json(&json!({}))
        .send()
        .await
        .expect("POST /search/filesystem");
    assert_eq!(resp.status().as_u16(), 200, "search must not 5xx");
    let body: Value = resp.json().await.expect("/search/filesystem JSON");
    assert_eq!(body["results"].as_array().unwrap().len(), 0);
    assert_eq!(body["truncated"], false);

    // Sanity: content + semantic also degrade cleanly with zero vaults.
    let resp = http_client()
        .post(format!("{}/search/content", daemon.base_url))
        .json(&json!({ "query": "anything" }))
        .send()
        .await
        .expect("POST /search/content");
    assert_eq!(resp.status().as_u16(), 200);
    let body: Value = resp.json().await.expect("/search/content JSON");
    assert_eq!(body["results"].as_array().unwrap().len(), 0);

    daemon.shutdown().await;
}
