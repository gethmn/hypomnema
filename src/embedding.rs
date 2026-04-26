//! HTTP client for the OpenAI-compatible embedding service.
//!
//! Pure-async network module: lives on the runtime, never inside
//! `spawn_blocking` (per the sqlite-vec-extension skill smell — embedding is
//! a network call). Returns a typed [`EmbeddingError`] so the indexer can
//! distinguish "service unavailable, skip-and-log" from "JSON parse failure,
//! bug in our code or the service".

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::config::EmbeddingConfig;

/// Failure modes from a single `embed` call.
///
/// `Transport` and `Status { code: 5xx }` are the retryable, skip-and-log
/// classes. `Status { code: 4xx }`, `BodyParse`, and `DimensionMismatch` all
/// indicate a bug in the daemon, the service, or the configuration — they
/// should not be silently retried.
#[derive(Debug)]
pub enum EmbeddingError {
    Transport(reqwest::Error),
    Status { code: u16, body: String },
    BodyParse(serde_json::Error),
    DimensionMismatch { expected: u32, actual: u32 },
}

impl std::fmt::Display for EmbeddingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transport(e) => write!(f, "embedding transport error: {e}"),
            Self::Status { code, body } => {
                write!(f, "embedding service returned HTTP {code}: {body}")
            }
            Self::BodyParse(e) => write!(f, "embedding response body parse error: {e}"),
            Self::DimensionMismatch { expected, actual } => write!(
                f,
                "embedding dimension mismatch: expected {expected}, got {actual}"
            ),
        }
    }
}

impl std::error::Error for EmbeddingError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Transport(e) => Some(e),
            Self::BodyParse(e) => Some(e),
            _ => None,
        }
    }
}

const RETRY_BACKOFF: Duration = Duration::from_millis(250);

/// HTTP client to the local OpenAI-compatible embedding service. Built once
/// per daemon start and shared across the indexer; cloning is cheap because
/// `reqwest::Client` is `Arc`-internal.
#[derive(Debug, Clone)]
pub struct EmbeddingClient {
    http: reqwest::Client,
    endpoint: String,
    model: String,
    api_key: String,
    dimension: u32,
    max_retries: u8,
}

impl EmbeddingClient {
    pub fn new(cfg: &EmbeddingConfig) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_millis(cfg.timeout_ms))
            .build()
            .context("building embedding HTTP client")?;
        Ok(Self {
            http,
            endpoint: cfg.endpoint.clone(),
            model: cfg.model.clone(),
            api_key: cfg.api_key.clone(),
            dimension: cfg.dimension,
            max_retries: cfg.max_retries,
        })
    }

    /// Embed a single chunk of text. v0 always sends a one-element `input`
    /// array; `embed_batch` is deferred until `batch_size > 1` matters.
    ///
    /// Retries once (configurable via `embedding.max_retries`) on transport
    /// errors and HTTP 5xx, with a 250ms backoff. 4xx responses are returned
    /// unmodified — they signal a bug, not a transient condition.
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>, EmbeddingError> {
        let max_attempts = self.max_retries.saturating_add(1);
        let mut attempt: u8 = 0;
        loop {
            attempt += 1;
            match self.send_one(text).await {
                Ok(v) => return Ok(v),
                Err(e) => {
                    if attempt >= max_attempts || !is_retryable(&e) {
                        return Err(e);
                    }
                    tokio::time::sleep(RETRY_BACKOFF).await;
                }
            }
        }
    }

    async fn send_one(&self, text: &str) -> Result<Vec<f32>, EmbeddingError> {
        let body = serde_json::json!({
            "model": self.model,
            "input": [text],
        });
        let mut req = self.http.post(&self.endpoint).json(&body);
        if !self.api_key.is_empty() {
            req = req.bearer_auth(&self.api_key);
        }
        let resp = req.send().await.map_err(EmbeddingError::Transport)?;
        let status = resp.status();
        if !status.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            return Err(EmbeddingError::Status {
                code: status.as_u16(),
                body: body_text,
            });
        }
        let bytes = resp.bytes().await.map_err(EmbeddingError::Transport)?;
        let parsed: ApiResponse =
            serde_json::from_slice(&bytes).map_err(EmbeddingError::BodyParse)?;
        let embedding = parsed
            .data
            .into_iter()
            .next()
            .map(|d| d.embedding)
            .unwrap_or_default();
        if embedding.len() != self.dimension as usize {
            return Err(EmbeddingError::DimensionMismatch {
                expected: self.dimension,
                actual: embedding.len() as u32,
            });
        }
        Ok(embedding)
    }
}

fn is_retryable(e: &EmbeddingError) -> bool {
    match e {
        EmbeddingError::Transport(_) => true,
        EmbeddingError::Status { code, .. } => *code >= 500,
        EmbeddingError::BodyParse(_) | EmbeddingError::DimensionMismatch { .. } => false,
    }
}

/// Boxed-future shape returned by [`Embedder::embed_text`]. Manual `Pin<Box<…>>`
/// (rather than `async fn` in the trait) so the trait stays object-safe and
/// `Scanner` can hold an `Arc<dyn Embedder>` without a generic parameter.
pub type EmbedFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Vec<f32>, EmbeddingError>> + Send + 'a>>;

/// Abstraction over the embedding backend. Production wires
/// [`EmbeddingClient`]; tests wire [`StubEmbedder`] (or a custom impl) so the
/// indexer's chunk-and-embed pipeline can be exercised without a live HTTP
/// service.
pub trait Embedder: Send + Sync + 'static {
    fn embed_text<'a>(&'a self, text: &'a str) -> EmbedFuture<'a>;
}

impl Embedder for EmbeddingClient {
    fn embed_text<'a>(&'a self, text: &'a str) -> EmbedFuture<'a> {
        Box::pin(self.embed(text))
    }
}

/// Test embedder that returns a deterministic zero-filled vector of the
/// configured dimension for any input. Public so that integration tests in
/// `tests/` can wire it through `Scanner::new` without standing up a stub
/// HTTP service.
#[derive(Debug, Clone)]
pub struct StubEmbedder {
    pub dimension: usize,
}

impl StubEmbedder {
    pub fn new(dimension: usize) -> Self {
        Self { dimension }
    }
}

impl Embedder for StubEmbedder {
    fn embed_text<'a>(&'a self, _text: &'a str) -> EmbedFuture<'a> {
        let dim = self.dimension;
        Box::pin(async move { Ok(vec![0.0_f32; dim]) })
    }
}

#[derive(Deserialize)]
struct ApiResponse {
    data: Vec<EmbeddingItem>,
}

#[derive(Deserialize)]
struct EmbeddingItem {
    embedding: Vec<f32>,
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};
    use tokio::sync::watch;
    use tokio::task::JoinHandle;

    use super::*;

    fn response_body_with_dim(dim: usize) -> String {
        let vec: Vec<f32> = (0..dim).map(|i| (i as f32) * 0.001).collect();
        serde_json::json!({ "data": [{ "embedding": vec }] }).to_string()
    }

    fn test_config(endpoint: &str) -> EmbeddingConfig {
        EmbeddingConfig {
            endpoint: endpoint.to_string(),
            dimension: 4,
            timeout_ms: 5_000,
            max_retries: 1,
            ..EmbeddingConfig::default()
        }
    }

    #[derive(Clone)]
    enum StubAction {
        Respond { status: u16, body: String },
        AcceptThenClose,
        Hang,
    }

    struct StubServer {
        url: String,
        accept_count: Arc<AtomicUsize>,
        shutdown_tx: watch::Sender<bool>,
        handle: Option<JoinHandle<()>>,
    }

    impl StubServer {
        async fn spawn(actions: Vec<StubAction>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            let accept_count = Arc::new(AtomicUsize::new(0));
            let counter = accept_count.clone();
            let (tx, mut rx) = watch::channel(false);
            let actions = Arc::new(actions);
            let handle = tokio::spawn(async move {
                loop {
                    tokio::select! {
                        _ = rx.wait_for(|v| *v) => break,
                        accepted = listener.accept() => {
                            let stream = match accepted {
                                Ok((s, _)) => s,
                                Err(_) => continue,
                            };
                            let idx = counter.fetch_add(1, Ordering::SeqCst);
                            let action = actions
                                .get(idx)
                                .cloned()
                                .unwrap_or(StubAction::AcceptThenClose);
                            tokio::spawn(handle_request(stream, action));
                        }
                    }
                }
            });
            Self {
                url: format!("http://{addr}/v1/embeddings"),
                accept_count,
                shutdown_tx: tx,
                handle: Some(handle),
            }
        }

        fn count(&self) -> usize {
            self.accept_count.load(Ordering::SeqCst)
        }

        async fn shutdown(mut self) {
            let _ = self.shutdown_tx.send(true);
            if let Some(h) = self.handle.take() {
                let _ = h.await;
            }
        }
    }

    async fn handle_request(mut stream: TcpStream, action: StubAction) {
        match action {
            StubAction::AcceptThenClose => {
                drain_request(&mut stream).await;
                let _ = stream.shutdown().await;
            }
            StubAction::Hang => {
                drain_request(&mut stream).await;
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
            StubAction::Respond { status, body } => {
                drain_request(&mut stream).await;
                let status_text = match status {
                    200 => "OK",
                    400 => "Bad Request",
                    503 => "Service Unavailable",
                    _ => "Status",
                };
                let resp = format!(
                    "HTTP/1.1 {status} {status_text}\r\n\
                     Content-Type: application/json\r\n\
                     Content-Length: {}\r\n\
                     Connection: close\r\n\
                     \r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(resp.as_bytes()).await;
                let _ = stream.flush().await;
                let _ = stream.shutdown().await;
            }
        }
    }

    async fn drain_request(stream: &mut TcpStream) {
        let mut buf = [0u8; 4096];
        let mut accum: Vec<u8> = Vec::new();
        let header_end = loop {
            match stream.read(&mut buf).await {
                Ok(0) => return,
                Ok(n) => {
                    accum.extend_from_slice(&buf[..n]);
                    if let Some(idx) = find_subseq(&accum, b"\r\n\r\n") {
                        break idx;
                    }
                }
                Err(_) => return,
            }
        };
        let header_str = String::from_utf8_lossy(&accum[..header_end]).to_string();
        let cl = parse_content_length(&header_str).unwrap_or(0);
        let body_so_far = accum.len().saturating_sub(header_end + 4);
        let mut remaining = cl.saturating_sub(body_so_far);
        while remaining > 0 {
            match stream.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => remaining = remaining.saturating_sub(n),
                Err(_) => break,
            }
        }
    }

    fn find_subseq(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        haystack.windows(needle.len()).position(|w| w == needle)
    }

    fn parse_content_length(headers: &str) -> Option<usize> {
        for line in headers.lines() {
            if let Some(idx) = line.find(':') {
                let (name, value) = line.split_at(idx);
                if name.eq_ignore_ascii_case("content-length") {
                    return value[1..].trim().parse().ok();
                }
            }
        }
        None
    }

    #[tokio::test]
    async fn embed_returns_vector_for_200() {
        let stub = StubServer::spawn(vec![StubAction::Respond {
            status: 200,
            body: response_body_with_dim(4),
        }])
        .await;
        let client = EmbeddingClient::new(&test_config(&stub.url)).unwrap();
        let v = client.embed("hello").await.expect("embed succeeds");
        assert_eq!(v.len(), 4);
        assert_eq!(stub.count(), 1);
        stub.shutdown().await;
    }

    #[tokio::test]
    async fn embed_retries_once_on_503() {
        let stub = StubServer::spawn(vec![
            StubAction::Respond {
                status: 503,
                body: "service down".to_string(),
            },
            StubAction::Respond {
                status: 200,
                body: response_body_with_dim(4),
            },
        ])
        .await;
        let client = EmbeddingClient::new(&test_config(&stub.url)).unwrap();
        let v = client.embed("hello").await.expect("retry then success");
        assert_eq!(v.len(), 4);
        assert_eq!(stub.count(), 2, "exactly one retry");
        stub.shutdown().await;
    }

    #[tokio::test]
    async fn embed_retries_once_on_connection_refused() {
        let stub = StubServer::spawn(vec![
            StubAction::AcceptThenClose,
            StubAction::AcceptThenClose,
        ])
        .await;
        let client = EmbeddingClient::new(&test_config(&stub.url)).unwrap();
        let err = client
            .embed("hello")
            .await
            .expect_err("both attempts fail with transport");
        assert!(
            matches!(err, EmbeddingError::Transport(_)),
            "expected Transport, got {err:?}"
        );
        assert_eq!(stub.count(), 2, "exactly one retry, total two attempts");
        stub.shutdown().await;
    }

    #[tokio::test]
    async fn embed_does_not_retry_on_4xx() {
        let stub = StubServer::spawn(vec![StubAction::Respond {
            status: 400,
            body: "bad input".to_string(),
        }])
        .await;
        let client = EmbeddingClient::new(&test_config(&stub.url)).unwrap();
        let err = client.embed("hello").await.expect_err("400 returned");
        match err {
            EmbeddingError::Status { code, body } => {
                assert_eq!(code, 400);
                assert_eq!(body, "bad input");
            }
            other => panic!("expected Status; got {other:?}"),
        }
        assert_eq!(stub.count(), 1, "no retry on 4xx");
        stub.shutdown().await;
    }

    #[tokio::test]
    async fn embed_dimension_mismatch_classified() {
        let stub = StubServer::spawn(vec![StubAction::Respond {
            status: 200,
            body: response_body_with_dim(8), // config expects 4
        }])
        .await;
        let client = EmbeddingClient::new(&test_config(&stub.url)).unwrap();
        let err = client.embed("hello").await.expect_err("dimension mismatch");
        match err {
            EmbeddingError::DimensionMismatch { expected, actual } => {
                assert_eq!(expected, 4);
                assert_eq!(actual, 8);
            }
            other => panic!("expected DimensionMismatch; got {other:?}"),
        }
        stub.shutdown().await;
    }

    #[tokio::test]
    async fn embed_honors_timeout() {
        let stub = StubServer::spawn(vec![StubAction::Hang, StubAction::Hang]).await;
        let cfg = EmbeddingConfig {
            timeout_ms: 200,
            ..test_config(&stub.url)
        };
        let client = EmbeddingClient::new(&cfg).unwrap();
        let err = client.embed("hello").await.expect_err("timeout");
        match err {
            EmbeddingError::Transport(e) => {
                assert!(e.is_timeout(), "expected is_timeout; got {e}");
            }
            other => panic!("expected Transport(timeout); got {other:?}"),
        }
        stub.shutdown().await;
    }
}
