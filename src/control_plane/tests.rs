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
