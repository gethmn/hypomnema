// Unit tests for the control_plane module. Twelve cases per workplan
// § Task 10.2 task body.

use std::path::Path;
use std::sync::Arc;

use chrono::Utc;
use tempfile::TempDir;
use tokio::sync::watch;

use crate::config::{
    Config, ConfigPath, EmbeddingConfig, HttpConfig, LoggingConfig, McpConfig, StorageConfig,
    WatcherConfig,
};
use crate::embedding::{Embedder, StubEmbedder};
use crate::vault_registry::{VaultId, VaultRegistry, VaultRow, VaultStatus, vault_data_dir};

use super::manager::{ControlPlaneError, CreateVaultRequest, VaultManager};

const DIM: u32 = 768;

fn make_config_at(data_dir: &Path) -> Config {
    Config {
        vault: None,
        http: HttpConfig::default(),
        mcp: McpConfig::default(),
        embedding: EmbeddingConfig::default(),
        watcher: WatcherConfig::default(),
        storage: StorageConfig {
            data_dir: ConfigPath(data_dir.to_path_buf()),
            index_file: "index.sqlite".to_string(),
            outbox_file: "outbox.jsonl".to_string(),
        },
        logging: LoggingConfig::default(),
        default_vault_name: "default".to_string(),
    }
}

async fn setup() -> (
    TempDir,
    Arc<Config>,
    Arc<VaultRegistry>,
    Arc<dyn Embedder>,
    watch::Sender<bool>,
    watch::Receiver<bool>,
) {
    let root = TempDir::new().expect("tempdir");
    let data_dir = root.path().join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let config = Arc::new(make_config_at(&data_dir));
    let registry = Arc::new(VaultRegistry::open(&data_dir).await.expect("open registry"));
    let embedder: Arc<dyn Embedder> = Arc::new(StubEmbedder::new(DIM as usize));
    let (tx, rx) = watch::channel(false);
    (root, config, registry, embedder, tx, rx)
}

async fn open_manager(
    config: Arc<Config>,
    registry: Arc<VaultRegistry>,
    embedder: Arc<dyn Embedder>,
    rx: watch::Receiver<bool>,
) -> VaultManager {
    VaultManager::open(registry, config, embedder, DIM, rx)
        .await
        .expect("open manager")
}

fn fresh_vault_dir(root: &Path, name: &str) -> std::path::PathBuf {
    let p = root.join(name);
    std::fs::create_dir_all(&p).unwrap();
    p
}

#[tokio::test]
async fn vault_manager_open_loads_active_runners() {
    let (root, config, registry, embedder, _tx, rx) = setup().await;

    let path_a = fresh_vault_dir(root.path(), "a");
    let path_b = fresh_vault_dir(root.path(), "b");
    let path_c = fresh_vault_dir(root.path(), "c");
    for (name, path, status) in [
        ("alpha", path_a, VaultStatus::Active),
        ("bravo", path_b, VaultStatus::Active),
        ("charlie", path_c, VaultStatus::Paused),
    ] {
        registry
            .insert(VaultRow {
                id: VaultId::new(),
                name: name.to_string(),
                path,
                status,
                created_at: Utc::now(),
                last_error: None,
            })
            .await
            .unwrap();
    }

    let manager = open_manager(config, registry, embedder, rx).await;
    let active = manager.active_vaults();
    assert_eq!(active.len(), 2);
    let mut names: Vec<&str> = active.iter().map(|e| e.name.as_str()).collect();
    names.sort();
    assert_eq!(names, vec!["alpha", "bravo"]);
}

#[tokio::test]
async fn create_inserts_row_subdir_and_runner() {
    let (root, config, registry, embedder, _tx, rx) = setup().await;
    let manager = open_manager(config.clone(), registry.clone(), embedder, rx).await;

    let path = fresh_vault_dir(root.path(), "v");
    let row = manager
        .create(CreateVaultRequest {
            name: Some("first".to_string()),
            path: path.clone(),
        })
        .await
        .expect("create succeeds");
    assert_eq!(row.name, "first");
    assert_eq!(row.status, VaultStatus::Active);

    // Registry row present.
    let registry_rows = registry.list().await.unwrap();
    assert_eq!(registry_rows.len(), 1);
    assert_eq!(registry_rows[0].id, row.id);

    // Per-vault subdir + meta.toml present.
    let subdir = vault_data_dir(&config.storage.data_dir.0, &row.id);
    assert!(subdir.is_dir(), "per-vault subdir created");
    assert!(subdir.join("meta.toml").is_file(), "meta.toml written");

    // Runner-map entry visible via active_vaults().
    let active = manager.active_vaults();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].id, row.id);
}

#[tokio::test]
async fn create_rejects_path_already_registered() {
    let (root, config, registry, embedder, _tx, rx) = setup().await;
    let manager = open_manager(config, registry, embedder, rx).await;

    let path = fresh_vault_dir(root.path(), "shared");
    manager
        .create(CreateVaultRequest {
            name: Some("first".to_string()),
            path: path.clone(),
        })
        .await
        .unwrap();

    let err = manager
        .create(CreateVaultRequest {
            name: Some("second".to_string()),
            path: path.clone(),
        })
        .await
        .expect_err("second create against same path should reject");
    match err {
        ControlPlaneError::VaultPathConflict { existing_name, .. } => {
            assert_eq!(existing_name, "first");
        }
        other => panic!("expected VaultPathConflict, got {other:?}"),
    }
}

#[tokio::test]
async fn create_rejects_name_already_in_use() {
    let (root, config, registry, embedder, _tx, rx) = setup().await;
    let manager = open_manager(config, registry, embedder, rx).await;

    let path_a = fresh_vault_dir(root.path(), "a");
    let path_b = fresh_vault_dir(root.path(), "b");
    manager
        .create(CreateVaultRequest {
            name: Some("shared".to_string()),
            path: path_a.clone(),
        })
        .await
        .unwrap();

    let err = manager
        .create(CreateVaultRequest {
            name: Some("shared".to_string()),
            path: path_b.clone(),
        })
        .await
        .expect_err("second create against same name should reject");
    match err {
        ControlPlaneError::VaultNameConflict { name, .. } => {
            assert_eq!(name, "shared");
        }
        other => panic!("expected VaultNameConflict, got {other:?}"),
    }
}

#[tokio::test]
async fn create_rejects_data_dir_under_vault_path() {
    let root = TempDir::new().expect("tempdir");
    let outer = root.path().join("outer");
    let data_dir = outer.join("data");
    std::fs::create_dir_all(&data_dir).unwrap();

    let mut cfg = make_config_at(&data_dir);
    cfg.storage.data_dir = ConfigPath(data_dir.clone());
    let config = Arc::new(cfg);
    let registry = Arc::new(VaultRegistry::open(&data_dir).await.expect("open registry"));
    let embedder: Arc<dyn Embedder> = Arc::new(StubEmbedder::new(DIM as usize));
    let (_tx, rx) = watch::channel(false);
    let manager = open_manager(config.clone(), registry, embedder, rx).await;

    // The vault path is `outer`, data_dir = `outer/data` — data_dir is under
    // vault path → reject with VaultPathInvalid.
    let err = manager
        .create(CreateVaultRequest {
            name: Some("v".to_string()),
            path: outer.clone(),
        })
        .await
        .expect_err("create should reject data_dir under vault path");
    match err {
        ControlPlaneError::VaultPathInvalid { detail } => {
            assert!(
                detail.contains("data_dir") && detail.contains("under"),
                "expected detail to mention data_dir under vault path: {detail}"
            );
        }
        other => panic!("expected VaultPathInvalid, got {other:?}"),
    }
}

#[tokio::test]
async fn create_resolves_default_name_when_omitted() {
    let (root, config, registry, embedder, _tx, rx) = setup().await;
    let manager = open_manager(config.clone(), registry, embedder, rx).await;

    let path = fresh_vault_dir(root.path(), "v");
    let row = manager
        .create(CreateVaultRequest {
            name: None,
            path: path.clone(),
        })
        .await
        .expect("create succeeds with default name");
    assert_eq!(row.name, config.default_vault_name);
}

#[tokio::test]
async fn create_rejects_when_default_name_empty_and_no_explicit_name() {
    let root = TempDir::new().expect("tempdir");
    let data_dir = root.path().join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let mut cfg = make_config_at(&data_dir);
    cfg.default_vault_name = String::new();
    let config = Arc::new(cfg);
    let registry = Arc::new(VaultRegistry::open(&data_dir).await.expect("open registry"));
    let embedder: Arc<dyn Embedder> = Arc::new(StubEmbedder::new(DIM as usize));
    let (_tx, rx) = watch::channel(false);
    let manager = open_manager(config, registry, embedder, rx).await;

    let path = fresh_vault_dir(root.path(), "v");
    let err = manager
        .create(CreateVaultRequest { name: None, path })
        .await
        .expect_err("create with no name + empty default should reject");
    match err {
        ControlPlaneError::VaultPathInvalid { detail } => {
            assert!(
                detail.contains("name is required"),
                "expected name-required text, got {detail}"
            );
        }
        other => panic!("expected VaultPathInvalid, got {other:?}"),
    }
}

#[tokio::test]
async fn terminate_removes_runner_row_and_subdir() {
    let (root, config, registry, embedder, _tx, rx) = setup().await;
    let manager = open_manager(config.clone(), registry.clone(), embedder, rx).await;

    let path = fresh_vault_dir(root.path(), "v");
    let row = manager
        .create(CreateVaultRequest {
            name: Some("doomed".to_string()),
            path,
        })
        .await
        .unwrap();
    let subdir = vault_data_dir(&config.storage.data_dir.0, &row.id);
    assert!(subdir.is_dir(), "subdir present pre-terminate");

    manager
        .terminate("doomed")
        .await
        .expect("terminate succeeds");

    assert!(manager.active_vaults().is_empty());
    assert!(registry.list().await.unwrap().is_empty());
    assert!(!subdir.exists(), "subdir removed by terminate");
}

#[tokio::test]
async fn terminate_returns_vault_not_found_for_unknown() {
    let (_root, config, registry, embedder, _tx, rx) = setup().await;
    let manager = open_manager(config, registry, embedder, rx).await;

    let err = manager
        .terminate("does-not-exist")
        .await
        .expect_err("terminate against unknown vault must error");
    match err {
        ControlPlaneError::VaultNotFound { name_or_id, .. } => {
            assert_eq!(name_or_id, "does-not-exist");
        }
        other => panic!("expected VaultNotFound, got {other:?}"),
    }
}

#[tokio::test]
async fn terminate_then_create_with_same_name_succeeds() {
    let (root, config, registry, embedder, _tx, rx) = setup().await;
    let manager = open_manager(config, registry, embedder, rx).await;

    let path = fresh_vault_dir(root.path(), "v");
    let first = manager
        .create(CreateVaultRequest {
            name: Some("recyclable".to_string()),
            path: path.clone(),
        })
        .await
        .unwrap();
    manager.terminate("recyclable").await.unwrap();
    let second = manager
        .create(CreateVaultRequest {
            name: Some("recyclable".to_string()),
            path: path.clone(),
        })
        .await
        .expect("re-create after terminate succeeds");

    assert_ne!(first.id, second.id, "fresh UUIDv7 minted on re-create");
    assert_eq!(second.name, "recyclable");
}

#[tokio::test]
async fn concurrent_creates_on_different_names_dont_block() {
    let (root, config, registry, embedder, _tx, rx) = setup().await;
    let manager = Arc::new(open_manager(config, registry.clone(), embedder, rx).await);

    let path_a = fresh_vault_dir(root.path(), "a");
    let path_b = fresh_vault_dir(root.path(), "b");

    let m1 = manager.clone();
    let h1 = tokio::spawn(async move {
        m1.create(CreateVaultRequest {
            name: Some("first".to_string()),
            path: path_a,
        })
        .await
    });
    let m2 = manager.clone();
    let h2 = tokio::spawn(async move {
        m2.create(CreateVaultRequest {
            name: Some("second".to_string()),
            path: path_b,
        })
        .await
    });

    h1.await.unwrap().expect("first create succeeds");
    h2.await.unwrap().expect("second create succeeds");

    let rows = registry.list().await.unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(manager.active_vaults().len(), 2);
}

#[tokio::test]
async fn concurrent_terminate_on_same_vault_serializes() {
    let (root, config, registry, embedder, _tx, rx) = setup().await;
    let manager = Arc::new(open_manager(config, registry.clone(), embedder, rx).await);

    let path = fresh_vault_dir(root.path(), "v");
    manager
        .create(CreateVaultRequest {
            name: Some("doomed".to_string()),
            path,
        })
        .await
        .unwrap();

    let m1 = manager.clone();
    let h1 = tokio::spawn(async move { m1.terminate("doomed").await });
    let m2 = manager.clone();
    let h2 = tokio::spawn(async move { m2.terminate("doomed").await });

    let r1 = h1.await.unwrap();
    let r2 = h2.await.unwrap();

    let success_count = [&r1, &r2].iter().filter(|r| r.is_ok()).count();
    let not_found_count = [&r1, &r2]
        .iter()
        .filter(|r| matches!(r, Err(ControlPlaneError::VaultNotFound { .. })))
        .count();
    assert_eq!(success_count, 1, "exactly one terminate succeeds");
    assert_eq!(
        not_found_count, 1,
        "the loser sees VaultNotFound (the winner removed the runner first)"
    );
}

// -- step-11 task 11.1 ----------------------------------------------------

fn read_meta_toml_name(vault_dir: &Path) -> String {
    let s = std::fs::read_to_string(vault_dir.join("meta.toml")).expect("read meta.toml");
    for line in s.lines() {
        if let Some(rest) = line.strip_prefix("name = \"") {
            if let Some(end) = rest.rfind('"') {
                return rest[..end].to_string();
            }
        }
    }
    panic!("meta.toml has no name line: {s}");
}

#[tokio::test]
async fn pause_drains_runner_and_updates_status() {
    let (root, config, registry, embedder, _tx, rx) = setup().await;
    let manager = open_manager(config, registry.clone(), embedder, rx).await;

    let path = fresh_vault_dir(root.path(), "v");
    let row = manager
        .create(CreateVaultRequest {
            name: Some("alpha".to_string()),
            path,
        })
        .await
        .unwrap();

    let updated = manager.pause("alpha").await.expect("pause succeeds");
    assert_eq!(updated.id, row.id);
    assert_eq!(updated.status, VaultStatus::Paused);
    assert!(updated.last_error.is_none());

    let on_disk = registry.get_by_id(&row.id).await.unwrap().unwrap();
    assert_eq!(on_disk.status, VaultStatus::Paused);

    // Active snapshot drops the paused vault; cross-vault scope shows it as
    // a non-active row (entry None).
    assert!(manager.active_vaults().is_empty());
    let scope = manager.search_scope().await.unwrap();
    assert_eq!(scope.len(), 1);
    assert_eq!(scope[0].status, VaultStatus::Paused);
    assert!(scope[0].entry.is_none());
}

#[tokio::test]
async fn pause_idempotent_on_already_paused() {
    let (root, config, registry, embedder, _tx, rx) = setup().await;
    let manager = open_manager(config, registry, embedder, rx).await;

    let path = fresh_vault_dir(root.path(), "v");
    manager
        .create(CreateVaultRequest {
            name: Some("alpha".to_string()),
            path,
        })
        .await
        .unwrap();

    manager.pause("alpha").await.expect("first pause succeeds");
    let again = manager
        .pause("alpha")
        .await
        .expect("second pause is a no-op");
    assert_eq!(again.status, VaultStatus::Paused);
}

#[tokio::test]
async fn pause_returns_vault_not_found_for_unknown() {
    let (_root, config, registry, embedder, _tx, rx) = setup().await;
    let manager = open_manager(config, registry, embedder, rx).await;

    let err = manager
        .pause("does-not-exist")
        .await
        .expect_err("pause against unknown vault must error");
    match err {
        ControlPlaneError::VaultNotFound { name_or_id, .. } => {
            assert_eq!(name_or_id, "does-not-exist");
        }
        other => panic!("expected VaultNotFound, got {other:?}"),
    }
}

#[tokio::test]
async fn resume_from_paused_restores_active() {
    let (root, config, registry, embedder, _tx, rx) = setup().await;
    let manager = open_manager(config, registry.clone(), embedder, rx).await;

    let path = fresh_vault_dir(root.path(), "v");
    let row = manager
        .create(CreateVaultRequest {
            name: Some("alpha".to_string()),
            path,
        })
        .await
        .unwrap();

    manager.pause("alpha").await.unwrap();
    let resumed = manager.resume("alpha").await.expect("resume succeeds");
    assert_eq!(resumed.id, row.id);
    assert_eq!(resumed.status, VaultStatus::Active);
    assert!(resumed.last_error.is_none());

    let on_disk = registry.get_by_id(&row.id).await.unwrap().unwrap();
    assert_eq!(on_disk.status, VaultStatus::Active);
    assert_eq!(manager.active_vaults().len(), 1);
}

#[tokio::test]
async fn resume_from_errored_with_path_accessible_succeeds() {
    let (root, config, registry, embedder, _tx, rx) = setup().await;

    let path = fresh_vault_dir(root.path(), "ev");
    let id = VaultId::new();
    registry
        .insert(VaultRow {
            id: id.clone(),
            name: "errd".to_string(),
            path: path.clone(),
            status: VaultStatus::Errored,
            created_at: Utc::now(),
            last_error: Some("prior reconcile failure".to_string()),
        })
        .await
        .unwrap();

    let manager = open_manager(config, registry.clone(), embedder, rx).await;
    // Reconcile skipped the errored row, so no runner is in the map yet.
    assert!(manager.active_vaults().is_empty());

    let resumed = manager.resume("errd").await.expect("resume errored vault");
    assert_eq!(resumed.id, id);
    assert_eq!(resumed.status, VaultStatus::Active);
    assert!(resumed.last_error.is_none());

    let on_disk = registry.get_by_id(&id).await.unwrap().unwrap();
    assert_eq!(on_disk.status, VaultStatus::Active);
    assert!(on_disk.last_error.is_none());
    assert_eq!(manager.active_vaults().len(), 1);
}

#[tokio::test]
async fn resume_from_errored_with_path_inaccessible_returns_503_vault_errored() {
    let (root, config, registry, embedder, _tx, rx) = setup().await;

    let bogus = root.path().join("not-there");
    let id = VaultId::new();
    registry
        .insert(VaultRow {
            id: id.clone(),
            name: "errd".to_string(),
            path: bogus,
            status: VaultStatus::Errored,
            created_at: Utc::now(),
            last_error: Some("path missing".to_string()),
        })
        .await
        .unwrap();

    let manager = open_manager(config, registry.clone(), embedder, rx).await;

    let err = manager
        .resume("errd")
        .await
        .expect_err("resume against inaccessible path must error");
    match err {
        ControlPlaneError::VaultErrored {
            name_or_id,
            last_error,
        } => {
            assert_eq!(name_or_id, "errd");
            assert_eq!(last_error.as_deref(), Some("path missing"));
        }
        other => panic!("expected VaultErrored, got {other:?}"),
    }

    let on_disk = registry.get_by_id(&id).await.unwrap().unwrap();
    assert_eq!(on_disk.status, VaultStatus::Errored);
}

#[tokio::test]
async fn resume_idempotent_on_already_active() {
    let (root, config, registry, embedder, _tx, rx) = setup().await;
    let manager = open_manager(config, registry, embedder, rx).await;

    let path = fresh_vault_dir(root.path(), "v");
    manager
        .create(CreateVaultRequest {
            name: Some("alpha".to_string()),
            path,
        })
        .await
        .unwrap();

    let again = manager
        .resume("alpha")
        .await
        .expect("resume on already-active is a no-op");
    assert_eq!(again.status, VaultStatus::Active);
    assert_eq!(manager.active_vaults().len(), 1);
}

#[tokio::test]
async fn rename_updates_registry_and_meta_toml() {
    let (root, config, registry, embedder, _tx, rx) = setup().await;
    let manager = open_manager(config.clone(), registry.clone(), embedder, rx).await;

    let path = fresh_vault_dir(root.path(), "v");
    let row = manager
        .create(CreateVaultRequest {
            name: Some("old".to_string()),
            path,
        })
        .await
        .unwrap();

    let renamed = manager
        .rename("old", "new-name")
        .await
        .expect("rename succeeds");
    assert_eq!(renamed.id, row.id, "surrogate id unchanged");
    assert_eq!(renamed.name, "new-name");

    let on_disk = registry.get_by_id(&row.id).await.unwrap().unwrap();
    assert_eq!(on_disk.name, "new-name");

    let vault_dir = vault_data_dir(&config.storage.data_dir.0, &row.id);
    assert_eq!(read_meta_toml_name(&vault_dir), "new-name");

    // Resolve via the new name; runner stayed in the map.
    let active = manager.active_vaults();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].name, "new-name");
}

#[tokio::test]
async fn rename_validates_new_name_regex() {
    let (root, config, registry, embedder, _tx, rx) = setup().await;
    let manager = open_manager(config, registry, embedder, rx).await;

    let path = fresh_vault_dir(root.path(), "v");
    manager
        .create(CreateVaultRequest {
            name: Some("alpha".to_string()),
            path,
        })
        .await
        .unwrap();

    for bad in ["has space", "with/slash", "", "dotted.name"] {
        let err = manager
            .rename("alpha", bad)
            .await
            .expect_err(&format!("rename to {bad:?} should fail"));
        match err {
            ControlPlaneError::VaultPathInvalid { detail } => {
                assert!(
                    detail.contains("new_name"),
                    "expected detail to mention new_name: {detail}"
                );
            }
            other => panic!("expected VaultPathInvalid for {bad:?}, got {other:?}"),
        }
    }
}

#[tokio::test]
async fn rename_rejects_name_already_in_use() {
    let (root, config, registry, embedder, _tx, rx) = setup().await;
    let manager = open_manager(config, registry, embedder, rx).await;

    let path_a = fresh_vault_dir(root.path(), "a");
    let path_b = fresh_vault_dir(root.path(), "b");
    manager
        .create(CreateVaultRequest {
            name: Some("alpha".to_string()),
            path: path_a,
        })
        .await
        .unwrap();
    manager
        .create(CreateVaultRequest {
            name: Some("bravo".to_string()),
            path: path_b,
        })
        .await
        .unwrap();

    let err = manager
        .rename("alpha", "bravo")
        .await
        .expect_err("rename to in-use name must error");
    match err {
        ControlPlaneError::VaultNameConflict { name, .. } => assert_eq!(name, "bravo"),
        other => panic!("expected VaultNameConflict, got {other:?}"),
    }
}

#[tokio::test]
async fn rename_to_same_name_is_noop() {
    let (root, config, registry, embedder, _tx, rx) = setup().await;
    let manager = open_manager(config, registry.clone(), embedder, rx).await;

    let path = fresh_vault_dir(root.path(), "v");
    manager
        .create(CreateVaultRequest {
            name: Some("alpha".to_string()),
            path,
        })
        .await
        .unwrap();

    let row = manager
        .rename("alpha", "alpha")
        .await
        .expect("rename to same name succeeds");
    assert_eq!(row.name, "alpha");
}

#[tokio::test]
async fn concurrent_renames_on_different_vaults_run_in_parallel() {
    let (root, config, registry, embedder, _tx, rx) = setup().await;
    let manager = Arc::new(open_manager(config, registry, embedder, rx).await);

    let path_a = fresh_vault_dir(root.path(), "a");
    let path_b = fresh_vault_dir(root.path(), "b");
    manager
        .create(CreateVaultRequest {
            name: Some("alpha".to_string()),
            path: path_a,
        })
        .await
        .unwrap();
    manager
        .create(CreateVaultRequest {
            name: Some("bravo".to_string()),
            path: path_b,
        })
        .await
        .unwrap();

    let m1 = manager.clone();
    let m2 = manager.clone();
    let h1 = tokio::spawn(async move { m1.rename("alpha", "alpha2").await });
    let h2 = tokio::spawn(async move { m2.rename("bravo", "bravo2").await });

    let r1 = h1.await.unwrap().expect("rename alpha succeeds");
    let r2 = h2.await.unwrap().expect("rename bravo succeeds");
    assert_eq!(r1.name, "alpha2");
    assert_eq!(r2.name, "bravo2");

    let mut names: Vec<String> = manager
        .active_vaults()
        .iter()
        .map(|e| e.name.clone())
        .collect();
    names.sort();
    assert_eq!(names, vec!["alpha2", "bravo2"]);
}

#[tokio::test]
async fn concurrent_pause_and_search_dont_deadlock() {
    let (root, config, registry, embedder, _tx, rx) = setup().await;
    let manager = Arc::new(open_manager(config, registry, embedder, rx).await);

    let path_a = fresh_vault_dir(root.path(), "a");
    let path_b = fresh_vault_dir(root.path(), "b");
    manager
        .create(CreateVaultRequest {
            name: Some("alpha".to_string()),
            path: path_a,
        })
        .await
        .unwrap();
    manager
        .create(CreateVaultRequest {
            name: Some("bravo".to_string()),
            path: path_b,
        })
        .await
        .unwrap();

    let m1 = manager.clone();
    let m2 = manager.clone();
    let pause_h = tokio::spawn(async move { m1.pause("alpha").await });
    let search_h = tokio::spawn(async move {
        for _ in 0..32 {
            let _ = m2.search_scope().await.expect("search_scope");
            tokio::task::yield_now().await;
        }
    });

    let pause_res = tokio::time::timeout(std::time::Duration::from_secs(5), pause_h)
        .await
        .expect("pause did not complete within 5s — possible deadlock")
        .unwrap()
        .expect("pause result");
    assert_eq!(pause_res.status, VaultStatus::Paused);

    tokio::time::timeout(std::time::Duration::from_secs(5), search_h)
        .await
        .expect("searches did not complete within 5s — possible deadlock")
        .unwrap();
}
