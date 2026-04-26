use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use serde::de::DeserializeOwned;

pub use crate::api::types::{
    ContentMatchJson, ContentQueryJson, ContentResultJson, ContentSearchResponse, ErrorEnvelope,
    FilesystemQueryJson, FilesystemResultJson, FilesystemSearchResponse, HealthResponse,
    StatusResponse,
};
use crate::config::Config;

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
}

fn normalize_base_url(url: &str) -> String {
    url.trim_end_matches('/').to_string()
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
    Err(anyhow!("daemon returned HTTP {}: {}", status, body))
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

    use super::*;
    use crate::api::{ApiState, router};
    use crate::config::EmbeddingConfig;
    use crate::store::Store;

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
        let store = Store::open(dir.path(), "index.sqlite", &EmbeddingConfig::default())
            .await
            .unwrap();
        let state = ApiState {
            pool: store.pool(),
            vault: vault.path().to_path_buf(),
            outbox_path: dir.path().join("outbox.jsonl"),
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
            base_url: format!("http://{}", addr),
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
            })
            .await
            .expect("content search succeeds");
        assert!(resp.results.is_empty());
        assert!(!resp.truncated);
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
