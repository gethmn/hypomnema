use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use serde::de::DeserializeOwned;

pub use crate::api::types::{
    ContentMatchJson, ContentQueryJson, ContentResultJson, ContentSearchResponse,
    CreateVaultRequest, ErrorEnvelope, FilesystemQueryJson, FilesystemResultJson,
    FilesystemSearchResponse, HealthResponse, RenameRequest, RescanResponseJson, ResetRequest,
    SemanticQueryJson, SemanticResultJson, SemanticSearchResponse, StatusResponse,
    TerminateVaultResponse, VaultListResponse, VaultRowJson,
};
use crate::config::Config;

#[derive(Clone)]
pub struct DaemonClient {
    base_url: String,
    http: reqwest::Client,
}

impl DaemonClient {
    pub fn from_config(config: &Config, override_url: Option<&str>) -> Result<Self> {
        let base_url = match override_url {
            Some(u) => normalize_base_url(u),
            None => format!("http://{}", config.http.bind),
        };
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .context("building reqwest client")?;
        Ok(Self { base_url, http })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub async fn health(&self) -> Result<HealthResponse> {
        let url = format!("{}/health", self.base_url);
        let resp = self.http.get(&url).send().await?;
        decode_response(resp).await
    }

    pub async fn status(&self) -> Result<StatusResponse> {
        let url = format!("{}/status", self.base_url);
        let resp = self.http.get(&url).send().await?;
        decode_response(resp).await
    }

    pub async fn search_filesystem(
        &self,
        q: &FilesystemQueryJson,
    ) -> Result<FilesystemSearchResponse> {
        let url = format!("{}/search/filesystem", self.base_url);
        let resp = self.http.post(&url).json(q).send().await?;
        decode_response(resp).await
    }

    pub async fn search_content(&self, q: &ContentQueryJson) -> Result<ContentSearchResponse> {
        let url = format!("{}/search/content", self.base_url);
        let resp = self.http.post(&url).json(q).send().await?;
        decode_response(resp).await
    }

    pub async fn search_semantic(&self, q: &SemanticQueryJson) -> Result<SemanticSearchResponse> {
        let url = format!("{}/search/semantic", self.base_url);
        let resp = self.http.post(&url).json(q).send().await?;
        decode_response(resp).await
    }

    pub async fn create_vault(&self, req: &CreateVaultRequest) -> Result<VaultRowJson> {
        let url = format!("{}/vaults", self.base_url);
        let resp = self.http.post(&url).json(req).send().await?;
        decode_response(resp).await
    }

    pub async fn list_vaults(&self) -> Result<VaultListResponse> {
        let url = format!("{}/vaults", self.base_url);
        let resp = self.http.get(&url).send().await?;
        decode_response(resp).await
    }

    pub async fn get_vault(&self, name_or_id: &str) -> Result<VaultRowJson> {
        let url = vault_url(&self.base_url, name_or_id)?;
        let resp = self.http.get(url).send().await?;
        decode_response(resp).await
    }

    pub async fn terminate_vault(&self, name_or_id: &str) -> Result<TerminateVaultResponse> {
        let url = vault_url(&self.base_url, name_or_id)?;
        let resp = self.http.delete(url).send().await?;
        decode_response(resp).await
    }

    pub async fn pause_vault(&self, name_or_id: &str) -> Result<VaultRowJson> {
        let url = vault_op_url(&self.base_url, name_or_id, "pause")?;
        let resp = self.http.post(url).send().await?;
        decode_response(resp).await
    }

    pub async fn resume_vault(&self, name_or_id: &str) -> Result<VaultRowJson> {
        let url = vault_op_url(&self.base_url, name_or_id, "resume")?;
        let resp = self.http.post(url).send().await?;
        decode_response(resp).await
    }

    pub async fn reset_vault(&self, name_or_id: &str, rebuild: bool) -> Result<VaultRowJson> {
        let url = vault_op_url(&self.base_url, name_or_id, "reset")?;
        let resp = self
            .http
            .post(url)
            .json(&ResetRequest { rebuild })
            .send()
            .await?;
        decode_response(resp).await
    }

    pub async fn rename_vault(&self, name_or_id: &str, new_name: &str) -> Result<VaultRowJson> {
        let url = vault_op_url(&self.base_url, name_or_id, "rename")?;
        let resp = self
            .http
            .post(url)
            .json(&RenameRequest {
                new_name: new_name.to_string(),
            })
            .send()
            .await?;
        decode_response(resp).await
    }

    pub async fn rescan_vault(&self, name_or_id: &str) -> Result<RescanResponseJson> {
        let url = vault_op_url(&self.base_url, name_or_id, "rescan")?;
        let resp = self.http.post(url).send().await?;
        decode_response(resp).await
    }
}

fn normalize_base_url(url: &str) -> String {
    url.trim_end_matches('/').to_string()
}

/// Build a `/vaults/<segment>` URL with `name_or_id` percent-encoded as a
/// single path segment. Vault names are otherwise unconstrained at the
/// CLI surface; without this, names containing `/` or `?` would land on
/// the wrong handler.
fn vault_url(base_url: &str, name_or_id: &str) -> Result<reqwest::Url> {
    let mut url = reqwest::Url::parse(base_url).context("parsing daemon base URL")?;
    url.path_segments_mut()
        .map_err(|_| anyhow!("daemon URL cannot be a base"))?
        .extend(["vaults", name_or_id]);
    Ok(url)
}

fn vault_op_url(base_url: &str, name_or_id: &str, op: &str) -> Result<reqwest::Url> {
    let mut url = reqwest::Url::parse(base_url).context("parsing daemon base URL")?;
    url.path_segments_mut()
        .map_err(|_| anyhow!("daemon URL cannot be a base"))?
        .extend(["vaults", name_or_id, op]);
    Ok(url)
}

async fn decode_response<T: DeserializeOwned>(resp: reqwest::Response) -> Result<T> {
    let status = resp.status();
    if status.is_success() {
        return resp
            .json::<T>()
            .await
            .context("decoding daemon response body");
    }
    let bytes = resp
        .bytes()
        .await
        .context("reading daemon error response body")?;
    if let Ok(env) = serde_json::from_slice::<ErrorEnvelope>(&bytes) {
        return Err(anyhow!("{}: {}", env.error.code, env.error.message));
    }
    let body = String::from_utf8_lossy(&bytes);
    Err(anyhow!("daemon returned HTTP {status}: {body}"))
}

pub fn is_connect_error(err: &anyhow::Error) -> bool {
    err.chain().any(|e| {
        e.downcast_ref::<reqwest::Error>()
            .map(|r| r.is_connect())
            .unwrap_or(false)
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tempfile::TempDir;
    use tokio::net::TcpListener;
    use tokio::sync::watch;
    use tokio::task::JoinHandle;

    use std::sync::Arc;

    use super::*;
    use crate::api::{ApiState, VaultEntry, router};
    use crate::config::EmbeddingConfig;
    use crate::control_plane::VaultManager;
    use crate::embedding::{Embedder, StubEmbedder};
    use crate::store::Store;
    use crate::vault_registry::VaultStatus;

    struct TestDaemon {
        base_url: String,
        shutdown: watch::Sender<bool>,
        handle: Option<JoinHandle<()>>,
        _dir: TempDir,
        _vault: TempDir,
    }

    impl TestDaemon {
        async fn shutdown(mut self) {
            let _ = self.shutdown.send(true);
            if let Some(h) = self.handle.take() {
                let _ = h.await;
            }
        }
    }

    async fn spawn_test_daemon() -> TestDaemon {
        let dir = TempDir::new().unwrap();
        let vault = TempDir::new().unwrap();
        let store = Store::open(
            &crate::vault_registry::VaultId::new(),
            dir.path(),
            "index.sqlite",
            &EmbeddingConfig::default(),
        )
        .await
        .unwrap();
        let embedder: Arc<dyn Embedder> = Arc::new(StubEmbedder::new(768));
        let entry = VaultEntry {
            id: crate::vault_registry::VaultId::new(),
            name: "test".to_string(),
            vault_path: vault.path().to_path_buf(),
            store: Arc::new(store),
            status: VaultStatus::Active,
        };
        let manager = Arc::new(VaultManager::for_tests(vec![entry], embedder, 768));
        let state = ApiState {
            vault_manager: manager.clone(),
            event_bus: manager.event_bus(),
        };
        let app = router(state);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, mut rx) = watch::channel(false);
        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = rx.wait_for(|v| *v).await;
                })
                .await;
        });
        TestDaemon {
            base_url: format!("http://{addr}"),
            shutdown: tx,
            handle: Some(handle),
            _dir: dir,
            _vault: vault,
        }
    }

    fn smoke_config(bind: &str) -> Config {
        let mut cfg = Config::default_for_smoke_test(PathBuf::from("/tmp/hypomnema-client-tests"));
        cfg.http.bind = bind.to_string();
        cfg
    }

    #[tokio::test]
    async fn client_default_url_builds_from_config_bind() {
        let cfg = smoke_config("127.0.0.1:7777");
        let client = DaemonClient::from_config(&cfg, None).unwrap();
        assert_eq!(client.base_url(), "http://127.0.0.1:7777");
    }

    #[tokio::test]
    async fn client_override_url_takes_precedence() {
        let cfg = smoke_config("127.0.0.1:7777");
        let client = DaemonClient::from_config(&cfg, Some("http://example.invalid:9999/")).unwrap();
        assert_eq!(client.base_url(), "http://example.invalid:9999");
    }

    #[tokio::test]
    async fn client_health_parses_response() {
        let daemon = spawn_test_daemon().await;
        let cfg = smoke_config("127.0.0.1:7777");
        let client = DaemonClient::from_config(&cfg, Some(&daemon.base_url)).unwrap();
        let health = client.health().await.expect("health succeeds");
        assert_eq!(health.status, "ok");
        daemon.shutdown().await;
    }

    #[tokio::test]
    async fn client_search_filesystem_round_trips() {
        let daemon = spawn_test_daemon().await;
        let cfg = smoke_config("127.0.0.1:7777");
        let client = DaemonClient::from_config(&cfg, Some(&daemon.base_url)).unwrap();
        let resp = client
            .search_filesystem(&FilesystemQueryJson {
                glob: Some("**/*.md".to_string()),
                ..Default::default()
            })
            .await
            .expect("filesystem search succeeds");
        assert!(resp.results.is_empty());
        assert!(!resp.truncated);
        daemon.shutdown().await;
    }

    #[tokio::test]
    async fn client_search_content_round_trips() {
        let daemon = spawn_test_daemon().await;
        let cfg = smoke_config("127.0.0.1:7777");
        let client = DaemonClient::from_config(&cfg, Some(&daemon.base_url)).unwrap();
        let resp = client
            .search_content(&ContentQueryJson {
                query: "anything".to_string(),
                regex: false,
                case_sensitive: false,
                prefix: None,
                include_matches: true,
                max_matches_per_file: None,
                limit: None,
                vaults: None,
            })
            .await
            .expect("content search succeeds");
        assert!(resp.results.is_empty());
        assert!(!resp.truncated);
        daemon.shutdown().await;
    }

    #[tokio::test]
    async fn client_search_semantic_round_trips() {
        let daemon = spawn_test_daemon().await;
        let cfg = smoke_config("127.0.0.1:7777");
        let client = DaemonClient::from_config(&cfg, Some(&daemon.base_url)).unwrap();
        let resp = client
            .search_semantic(&SemanticQueryJson {
                query: "anything".to_string(),
                ..Default::default()
            })
            .await
            .expect("semantic search succeeds");
        assert!(resp.results.is_empty());
        assert!(resp.hint.is_none());
        daemon.shutdown().await;
    }

    #[tokio::test]
    async fn client_translates_400_to_anyhow_with_code() {
        let daemon = spawn_test_daemon().await;
        let cfg = smoke_config("127.0.0.1:7777");
        let client = DaemonClient::from_config(&cfg, Some(&daemon.base_url)).unwrap();
        let err = client
            .search_filesystem(&FilesystemQueryJson {
                glob: Some("[unterminated".to_string()),
                ..Default::default()
            })
            .await
            .expect_err("invalid glob should yield Err");
        let msg = format!("{err:#}");
        assert!(
            msg.starts_with("invalid_glob: "),
            "expected leading code token, got {msg:?}"
        );
        daemon.shutdown().await;
    }

    #[test]
    fn vault_url_encodes_special_chars_in_segment() {
        let url = vault_url("http://localhost:9099", "needs/encoding?").unwrap();
        assert_eq!(
            url.as_str(),
            "http://localhost:9099/vaults/needs%2Fencoding%3F"
        );
    }

    #[test]
    fn vault_url_handles_normal_names() {
        let url = vault_url("http://localhost:9099", "personal").unwrap();
        assert_eq!(url.as_str(), "http://localhost:9099/vaults/personal");
    }

    #[test]
    fn vault_op_url_encodes_target_segment() {
        let url = vault_op_url("http://localhost:9099", "needs/encoding?", "pause").unwrap();
        assert_eq!(
            url.as_str(),
            "http://localhost:9099/vaults/needs%2Fencoding%3F/pause"
        );
    }

    #[test]
    fn vault_op_url_handles_normal_names_and_ops() {
        for op in ["pause", "resume", "reset", "rename", "rescan"] {
            let url = vault_op_url("http://localhost:9099", "personal", op).unwrap();
            assert_eq!(
                url.as_str(),
                format!("http://localhost:9099/vaults/personal/{op}")
            );
        }
    }

    #[tokio::test]
    async fn client_returns_connect_error_when_daemon_down() {
        // Bind to grab a free port, then drop the listener so nothing is
        // listening on it. A fresh connect attempt should be refused.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let url = format!("http://{addr}");
        let cfg = smoke_config("127.0.0.1:7777");
        let client = DaemonClient::from_config(&cfg, Some(&url)).unwrap();
        let err = client.health().await.expect_err("connect should fail");
        assert!(
            is_connect_error(&err),
            "expected connect error, got chain: {err:#}"
        );
    }
}
