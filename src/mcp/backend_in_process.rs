use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

use crate::api::search::{
    run_content_get, run_content_search, run_filesystem_search, run_semantic_search,
};
use crate::api::types::{
    ContentGetRequest, ContentGetResponse, ContentQueryJson, ContentSearchResponse,
    CreateVaultRequest, FilesystemQueryJson, FilesystemSearchResponse, RescanResponseJson,
    SemanticQueryJson, SemanticSearchResponse, TerminateVaultResponse, VaultListResponse,
    VaultRowJson,
};
use crate::config::SemanticSearchConfig;
use crate::control_plane::{CreateVaultRequest as ControlCreateRequest, VaultManager};
use crate::mcp::backend::HypomnemaBackend;

/// In-process backend: drives the MCP tool surface by calling the daemon's
/// `VaultManager` and search free functions directly, without an HTTP hop.
/// Constructed at `hmnd` startup from the same `Arc<VaultManager>` that
/// `ApiState` holds, so HTTP-MCP, in-process MCP, and HTTP API all observe
/// the same vault registry and runner map.
pub struct InProcessBackend {
    pub vault_manager: Arc<VaultManager>,
    pub semantic_config: SemanticSearchConfig,
}

impl InProcessBackend {
    pub fn new(vault_manager: Arc<VaultManager>) -> Self {
        Self {
            vault_manager,
            semantic_config: SemanticSearchConfig::default(),
        }
    }

    pub fn with_semantic_config(
        vault_manager: Arc<VaultManager>,
        semantic_config: SemanticSearchConfig,
    ) -> Self {
        Self {
            vault_manager,
            semantic_config,
        }
    }
}

#[async_trait]
impl HypomnemaBackend for InProcessBackend {
    async fn search_filesystem(&self, q: &FilesystemQueryJson) -> Result<FilesystemSearchResponse> {
        run_filesystem_search(&self.vault_manager, q)
            .await
            .map_err(Into::into)
    }

    async fn search_content(&self, q: &ContentQueryJson) -> Result<ContentSearchResponse> {
        run_content_search(&self.vault_manager, q)
            .await
            .map_err(Into::into)
    }

    async fn search_semantic(&self, q: &SemanticQueryJson) -> Result<SemanticSearchResponse> {
        run_semantic_search(&self.vault_manager, q, &self.semantic_config)
            .await
            .map_err(Into::into)
    }

    async fn content_get(&self, req: &ContentGetRequest) -> Result<ContentGetResponse> {
        run_content_get(&self.vault_manager, req)
            .await
            .map_err(Into::into)
    }

    async fn list_vaults(&self) -> Result<VaultListResponse> {
        let rows = self.vault_manager.list().await.map_err(anyhow_from_cp)?;
        Ok(VaultListResponse {
            vaults: rows.into_iter().map(VaultRowJson::from).collect(),
        })
    }

    async fn get_vault(&self, name_or_id: &str) -> Result<VaultRowJson> {
        let row = self
            .vault_manager
            .get(name_or_id)
            .await
            .map_err(anyhow_from_cp)?;
        Ok(VaultRowJson::from(row))
    }

    async fn create_vault(&self, req: &CreateVaultRequest) -> Result<VaultRowJson> {
        let row = self
            .vault_manager
            .create(ControlCreateRequest {
                name: req.name.clone(),
                path: PathBuf::from(req.path.clone()),
            })
            .await
            .map_err(anyhow_from_cp)?;
        Ok(VaultRowJson::from(row))
    }

    async fn pause_vault(&self, name_or_id: &str) -> Result<VaultRowJson> {
        let row = self
            .vault_manager
            .pause(name_or_id)
            .await
            .map_err(anyhow_from_cp)?;
        Ok(VaultRowJson::from(row))
    }

    async fn resume_vault(&self, name_or_id: &str) -> Result<VaultRowJson> {
        let row = self
            .vault_manager
            .resume(name_or_id)
            .await
            .map_err(anyhow_from_cp)?;
        Ok(VaultRowJson::from(row))
    }

    async fn reset_vault(&self, name_or_id: &str, rebuild: bool) -> Result<VaultRowJson> {
        let row = self
            .vault_manager
            .reset(name_or_id, rebuild)
            .await
            .map_err(anyhow_from_cp)?;
        Ok(VaultRowJson::from(row))
    }

    async fn rename_vault(&self, name_or_id: &str, new_name: &str) -> Result<VaultRowJson> {
        let row = self
            .vault_manager
            .rename(name_or_id, new_name)
            .await
            .map_err(anyhow_from_cp)?;
        Ok(VaultRowJson::from(row))
    }

    async fn rescan_vault(&self, name_or_id: &str) -> Result<RescanResponseJson> {
        let resp = self
            .vault_manager
            .rescan(name_or_id)
            .await
            .map_err(anyhow_from_cp)?;
        Ok(RescanResponseJson::from(resp))
    }

    async fn terminate_vault(&self, name_or_id: &str) -> Result<TerminateVaultResponse> {
        // Mirror the HTTP handler: pre-resolve so the response carries the
        // canonical id even when called by name. A concurrent terminate that
        // wins between resolve and terminate turns ours into a 404 via the
        // inner `terminate`'s own resolve.
        let id = self
            .vault_manager
            .resolve(name_or_id)
            .map_err(anyhow_from_cp)?;
        self.vault_manager
            .terminate(name_or_id)
            .await
            .map_err(anyhow_from_cp)?;
        Ok(TerminateVaultResponse {
            terminated: true,
            id: id.to_string(),
        })
    }

    fn is_connect_error(&self, _err: &anyhow::Error) -> bool {
        // No transport: the in-process backend cannot fail a connect because
        // there is no connection. `envelope_from_anyhow` therefore never
        // routes through `daemon_unreachable_envelope` for this backend.
        false
    }

    fn daemon_unreachable_envelope(&self, _err: &anyhow::Error) -> Value {
        // Per the trait contract, only called when `is_connect_error` returns
        // `true`. This backend's `is_connect_error` always returns `false`,
        // so this is an invariant violation if ever reached.
        unreachable!("InProcessBackend has no transport-level unreachability")
    }
}

/// Round-trip a `ControlPlaneError` (the manager's error type) through the
/// API's `ApiError` mapping so the resulting anyhow display matches the
/// `code: message` shape `envelope_from_anyhow` expects. The DaemonClient
/// shapes its own anyhow that way via `decode_response`; this keeps both
/// backends symmetric so MCP hosts see identical envelopes regardless of
/// transport.
fn anyhow_from_cp(err: crate::control_plane::ControlPlaneError) -> anyhow::Error {
    crate::api::error::ApiError::from(err).into()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tempfile::TempDir;

    use super::*;
    use crate::api::VaultEntry;
    use crate::api::types::{ContentQueryJson, FilesystemQueryJson, SemanticQueryJson};
    use crate::config::{
        Config, ConfigPath, EmbeddingConfig, HttpConfig, LoggingConfig, McpConfig, StorageConfig,
        WatcherConfig,
    };
    use crate::control_plane::VaultManager;
    use crate::embedding::{Embedder, StubEmbedder};
    use crate::store::Store;
    use crate::vault_registry::{VaultId, VaultRegistry, VaultStatus};

    fn assert_impl<T: HypomnemaBackend>(_: &T) {}

    #[test]
    fn in_process_backend_implements_hypomnema_backend() {
        let manager = Arc::new(VaultManager::for_tests(
            Vec::new(),
            Arc::new(StubEmbedder::new(768)),
            768,
        ));
        let backend = InProcessBackend::new(manager);
        assert_impl(&backend);
    }

    async fn for_tests_manager_with_one_vault() -> (TempDir, TempDir, Arc<VaultManager>) {
        let dir = TempDir::new().unwrap();
        let vault = TempDir::new().unwrap();
        let store = Store::open(
            &VaultId::new(),
            dir.path(),
            "index.sqlite",
            &EmbeddingConfig::default(),
        )
        .await
        .unwrap();
        let entry = VaultEntry {
            id: VaultId::new(),
            name: "test".to_string(),
            vault_path: vault.path().to_path_buf(),
            store: Arc::new(store),
            status: VaultStatus::Active,
        };
        let embedder: Arc<dyn Embedder> = Arc::new(StubEmbedder::new(768));
        let manager = Arc::new(VaultManager::for_tests(vec![entry], embedder, 768));
        (dir, vault, manager)
    }

    #[tokio::test]
    async fn in_process_search_filesystem_returns_same_shape_as_http_handler() {
        let (_dir, _vault, manager) = for_tests_manager_with_one_vault().await;
        let backend = InProcessBackend::new(Arc::clone(&manager));
        let q = FilesystemQueryJson {
            glob: Some("**/*.md".into()),
            ..Default::default()
        };

        let via_backend = backend.search_filesystem(&q).await.expect("backend ok");
        let via_runner: FilesystemSearchResponse = run_filesystem_search(&manager, &q)
            .await
            .map_err(anyhow::Error::from)
            .expect("run_* ok");

        let bytes_a = serde_json::to_vec(&via_backend).unwrap();
        let bytes_b = serde_json::to_vec(&via_runner).unwrap();
        assert_eq!(bytes_a, bytes_b);
    }

    #[tokio::test]
    async fn in_process_search_content_returns_same_shape_as_http_handler() {
        let (_dir, _vault, manager) = for_tests_manager_with_one_vault().await;
        let backend = InProcessBackend::new(Arc::clone(&manager));
        let q = ContentQueryJson {
            query: "anything".into(),
            mode: None,
            regex: false,
            case_sensitive: false,
            prefix: None,
            include_matches: true,
            max_matches_per_file: None,
            limit: None,
            vaults: None,
        };

        let via_backend = backend.search_content(&q).await.expect("backend ok");
        let via_runner: ContentSearchResponse = run_content_search(&manager, &q)
            .await
            .map_err(anyhow::Error::from)
            .expect("run_* ok");

        let bytes_a = serde_json::to_vec(&via_backend).unwrap();
        let bytes_b = serde_json::to_vec(&via_runner).unwrap();
        assert_eq!(bytes_a, bytes_b);
    }

    #[tokio::test]
    async fn in_process_search_semantic_returns_same_shape_as_http_handler() {
        let (_dir, _vault, manager) = for_tests_manager_with_one_vault().await;
        let backend = InProcessBackend::new(Arc::clone(&manager));
        let q = SemanticQueryJson {
            query: "anything".into(),
            ..Default::default()
        };

        let via_backend = backend.search_semantic(&q).await.expect("backend ok");
        let via_runner: SemanticSearchResponse =
            run_semantic_search(&manager, &q, &SemanticSearchConfig::default())
                .await
                .map_err(anyhow::Error::from)
                .expect("run_* ok");

        let bytes_a = serde_json::to_vec(&via_backend).unwrap();
        let bytes_b = serde_json::to_vec(&via_runner).unwrap();
        assert_eq!(bytes_a, bytes_b);
    }

    fn vault_test_config(data_dir: &std::path::Path) -> Config {
        Config {
            vault: None,
            http: HttpConfig::default(),
            mcp: McpConfig::default(),
            embedding: EmbeddingConfig::default(),
            watcher: WatcherConfig::default(),
            storage: StorageConfig {
                data_dir: ConfigPath(data_dir.to_path_buf()),
                index_file: "index.sqlite".to_string(),
            },
            logging: LoggingConfig::default(),
            default_vault_name: "default".to_string(),
            search: crate::config::SearchConfig::default(),
        }
    }

    async fn vault_harness() -> (TempDir, Arc<VaultManager>, tokio::sync::watch::Sender<bool>) {
        let root = TempDir::new().unwrap();
        let data_dir = root.path().join("data");
        std::fs::create_dir_all(&data_dir).unwrap();
        let config = Arc::new(vault_test_config(&data_dir));
        let registry = Arc::new(VaultRegistry::open(&data_dir).await.unwrap());
        let embedder: Arc<dyn Embedder> = Arc::new(StubEmbedder::new(768));
        let (tx, rx) = tokio::sync::watch::channel(false);
        let manager = VaultManager::open(registry, config, embedder, 768, rx)
            .await
            .unwrap();
        (root, Arc::new(manager), tx)
    }

    fn fresh_dir(parent: &std::path::Path, name: &str) -> PathBuf {
        let p = parent.join(name);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[tokio::test]
    async fn in_process_list_vaults_returns_registry_rows() {
        let (root, manager, _tx) = vault_harness().await;
        let backend = InProcessBackend::new(Arc::clone(&manager));

        let path_a = fresh_dir(root.path(), "alpha");
        let path_b = fresh_dir(root.path(), "beta");
        backend
            .create_vault(&CreateVaultRequest {
                name: Some("alpha".into()),
                path: path_a.display().to_string(),
            })
            .await
            .expect("create alpha");
        backend
            .create_vault(&CreateVaultRequest {
                name: Some("beta".into()),
                path: path_b.display().to_string(),
            })
            .await
            .expect("create beta");

        let listed = backend.list_vaults().await.expect("list");
        let names: Vec<&str> = listed.vaults.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"alpha"), "names = {names:?}");
        assert!(names.contains(&"beta"), "names = {names:?}");
    }

    #[tokio::test]
    async fn in_process_get_vault_returns_404_envelope_for_unknown() {
        let (_root, manager, _tx) = vault_harness().await;
        let backend = InProcessBackend::new(manager);

        let err = backend
            .get_vault("nonexistent")
            .await
            .expect_err("expected vault_not_found");
        let display = format!("{err:#}");
        assert!(
            display.starts_with("vault_not_found:"),
            "expected leading code token, got {display:?}"
        );
    }

    #[tokio::test]
    async fn in_process_vault_lifecycle_round_trip() {
        let (root, manager, _tx) = vault_harness().await;
        let backend = InProcessBackend::new(Arc::clone(&manager));
        let path = fresh_dir(root.path(), "lifecycle");

        let created = backend
            .create_vault(&CreateVaultRequest {
                name: Some("lifecycle".into()),
                path: path.display().to_string(),
            })
            .await
            .expect("create");
        assert_eq!(created.name, "lifecycle");
        assert_eq!(created.status, "active");

        let paused = backend.pause_vault("lifecycle").await.expect("pause");
        assert_eq!(paused.status, "paused");

        let resumed = backend.resume_vault("lifecycle").await.expect("resume");
        assert_eq!(resumed.status, "active");

        let rescan = backend.rescan_vault("lifecycle").await.expect("rescan");
        assert_eq!(rescan.row.name, "lifecycle");
        assert!(!rescan.rescan_initiated_at.is_empty());

        let terminated = backend
            .terminate_vault("lifecycle")
            .await
            .expect("terminate");
        assert!(terminated.terminated);
        assert!(!terminated.id.is_empty());

        let err = backend
            .get_vault("lifecycle")
            .await
            .expect_err("get after terminate should fail");
        assert!(format!("{err:#}").starts_with("vault_not_found:"));
    }

    #[tokio::test]
    async fn in_process_is_connect_error_always_false() {
        let manager = Arc::new(VaultManager::for_tests(
            Vec::new(),
            Arc::new(StubEmbedder::new(768)),
            768,
        ));
        let backend = InProcessBackend::new(manager);

        let connect_shaped = anyhow::anyhow!("connection refused");
        assert!(!backend.is_connect_error(&connect_shaped));

        let arbitrary = anyhow::anyhow!("vault_not_found: vault foo not found");
        assert!(!backend.is_connect_error(&arbitrary));
    }
}
