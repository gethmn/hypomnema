//! HTTP client for the OpenAI-compatible embedding service.
//!
//! Pure-async network module: lives on the runtime, never inside
//! `spawn_blocking` (per the sqlite-vec-extension skill smell — embedding is
//! a network call). Returns a typed [`EmbeddingError`] so the indexer can
//! distinguish "service unavailable, skip-and-log" from "JSON parse failure,
//! bug in our code or the service".

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
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
    Status {
        code: u16,
        body: String,
    },
    BodyParse(serde_json::Error),
    DimensionMismatch {
        expected: u32,
        actual: u32,
    },
    /// The service returned a different number of embeddings than inputs we
    /// sent in a batch request. A contract violation, not a transient
    /// condition — treated like `BodyParse` (non-retryable, hard error).
    CountMismatch {
        expected: usize,
        actual: usize,
    },
    /// The batch response carried `index` values that are not a contiguous
    /// `0..len` permutation of the inputs, so vectors cannot be reliably
    /// aligned to the chunks we sent. Non-retryable contract violation.
    ResponseShape(String),
}

impl std::fmt::Display for EmbeddingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transport(e) => write!(f, "embedding transport error: {e}"),
            Self::Status { code, body } => {
                write!(
                    f,
                    "embedding service returned HTTP {code}: {}",
                    truncate_for_display(body)
                )
            }
            Self::BodyParse(e) => write!(f, "embedding response body parse error: {e}"),
            Self::DimensionMismatch { expected, actual } => write!(
                f,
                "embedding dimension mismatch: expected {expected}, got {actual}"
            ),
            Self::CountMismatch { expected, actual } => write!(
                f,
                "embedding count mismatch: sent {expected} inputs, got {actual} vectors"
            ),
            Self::ResponseShape(detail) => {
                write!(f, "embedding response shape error: {detail}")
            }
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
    /// Current per-request batch ceiling. Starts at `embedding.batch_size` and
    /// shrinks (halving, floor 1) the first time the service rejects a
    /// multi-input request — see [`EmbeddingClient::embed_many`]. Shared via
    /// `Arc` so the learned ceiling is sticky across clones and across files
    /// for the daemon's lifetime, rather than re-probed per request.
    max_batch: Arc<AtomicUsize>,
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
            max_batch: Arc::new(AtomicUsize::new((cfg.batch_size as usize).max(1))),
        })
    }

    /// Embed a single chunk of text via a one-element `input` array. Used by
    /// the startup health probe; the indexer uses [`EmbeddingClient::embed_many`]
    /// for real work so multiple chunks share a request.
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

    /// Embed many texts, splitting into requests of at most the current batch
    /// ceiling and concatenating the results in input order.
    ///
    /// Backend-agnostic by design: it depends only on the OpenAI-compatible
    /// `/v1/embeddings` contract and on the service rejecting an over-large
    /// batch with a 4xx. It does **not** probe any vendor-specific capability
    /// endpoint (e.g. TEI's `/info`), so it works equally behind a LiteLLM
    /// proxy or against any other compatible service.
    ///
    /// Adaptive shrink: if a multi-input request is rejected with a
    /// shrink-eligible status (see [`should_shrink`]), the ceiling is halved
    /// (floor 1) and the same span retried. The learned ceiling is sticky, so
    /// the cost is at most a few wasted requests once per daemon lifetime
    /// rather than per file. A rejection of a single-input request is a real
    /// error (e.g. a chunk exceeds the model's max input) and propagates.
    pub async fn embed_many(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        let mut out: Vec<Vec<f32>> = Vec::with_capacity(texts.len());
        let mut i = 0;
        // `size` is local to this call. Shrink decisions stay local until a
        // *multi-input* request actually succeeds at the reduced size — only
        // then do we lower the shared, daemon-wide ceiling. This prevents a
        // single pathological chunk (one input that fails at every batch size)
        // from permanently disabling batching for all other files: it would
        // shrink `size` to 1 here and hard-error, but the shared ceiling is
        // never lowered without positive batch-size evidence. `fetch_min` only
        // ever lowers the ceiling — once a server reveals a limit we stay
        // conservative for the daemon's lifetime.
        let mut size = self.max_batch.load(Ordering::Relaxed).max(1);
        while i < texts.len() {
            let end = (i + size).min(texts.len());
            let span = end - i;
            match self.send_batch_with_retry(&texts[i..end]).await {
                Ok(vecs) => {
                    if span > 1 {
                        self.max_batch.fetch_min(size, Ordering::Relaxed);
                    }
                    out.extend(vecs);
                    i = end;
                }
                Err(e) if span > 1 && should_shrink(&e) => {
                    let new = (size / 2).max(1);
                    size = new;
                    // Log only the structured status code, never the response
                    // body: it can be large and may echo input content.
                    let status_code = match &e {
                        EmbeddingError::Status { code, .. } => *code,
                        _ => 0,
                    };
                    tracing::warn!(
                        new_batch_size = new,
                        status_code,
                        "embedding service rejected a batch request; shrinking client \
                         batch size and retrying. Set `embedding.batch_size` at or below \
                         the service's limit to avoid this probe."
                    );
                    // Do not advance `i`; the loop retries this span at `new`.
                }
                Err(e) => return Err(e),
            }
        }
        Ok(out)
    }

    /// One batch request wrapped in the same transport/5xx retry policy as
    /// [`EmbeddingClient::embed`].
    async fn send_batch_with_retry(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        let max_attempts = self.max_retries.saturating_add(1);
        let mut attempt: u8 = 0;
        loop {
            attempt += 1;
            match self.send_batch(texts).await {
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

    async fn send_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        let body = serde_json::json!({
            "model": self.model,
            "input": texts,
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
        let mut parsed: ApiResponse =
            serde_json::from_slice(&bytes).map_err(EmbeddingError::BodyParse)?;
        if parsed.data.len() != texts.len() {
            return Err(EmbeddingError::CountMismatch {
                expected: texts.len(),
                actual: parsed.data.len(),
            });
        }
        // The OpenAI contract returns each item with its input `index`; reorder
        // by it so vectors line up with chunks even if the service returns them
        // out of order. `index` is `Option<usize>`, so a service that omits the
        // field gives `None` and is distinguishable from one that explicitly
        // sends `0`. If every item omits `index` we trust array order (also the
        // contract). If any item carries an index we require them all to and to
        // form a contiguous `0..len` permutation — gaps, duplicates, mixed
        // presence, or out-of-range values would silently misalign vectors, so
        // fail fast instead.
        let any_index = parsed.data.iter().any(|d| d.index.is_some());
        if any_index {
            if parsed.data.iter().any(|d| d.index.is_none()) {
                return Err(EmbeddingError::ResponseShape(
                    "response mixes items with and without `index`".to_string(),
                ));
            }
            parsed.data.sort_by_key(|d| d.index);
            for (pos, item) in parsed.data.iter().enumerate() {
                if item.index != Some(pos) {
                    return Err(EmbeddingError::ResponseShape(format!(
                        "response indices are not a contiguous permutation of \
                         0..={} (position {pos} has index {:?})",
                        texts.len().saturating_sub(1),
                        item.index
                    )));
                }
            }
        }
        let mut out = Vec::with_capacity(parsed.data.len());
        for item in parsed.data {
            if item.embedding.len() != self.dimension as usize {
                return Err(EmbeddingError::DimensionMismatch {
                    expected: self.dimension,
                    actual: item.embedding.len() as u32,
                });
            }
            out.push(item.embedding);
        }
        Ok(out)
    }
}

/// Cap an embedding-service response body when rendering it in an error
/// message. Bodies can be large and some services echo part of the input in
/// 4xx responses, so error `Display` (which flows into logs via `anyhow`
/// chains) shows only a bounded prefix. The structured `body` field on
/// [`EmbeddingError::Status`] remains available for explicit inspection.
fn truncate_for_display(body: &str) -> std::borrow::Cow<'_, str> {
    const MAX: usize = 256;
    if body.len() <= MAX {
        return std::borrow::Cow::Borrowed(body);
    }
    let mut end = MAX;
    while !body.is_char_boundary(end) {
        end -= 1;
    }
    std::borrow::Cow::Owned(format!("{}… ({} bytes total)", &body[..end], body.len()))
}

fn is_retryable(e: &EmbeddingError) -> bool {
    match e {
        EmbeddingError::Transport(_) => true,
        EmbeddingError::Status { code, .. } => *code >= 500,
        EmbeddingError::BodyParse(_)
        | EmbeddingError::DimensionMismatch { .. }
        | EmbeddingError::CountMismatch { .. }
        | EmbeddingError::ResponseShape(_) => false,
    }
}

/// Whether a failed batch request should trigger an adaptive shrink-and-retry.
///
/// We treat the 4xx codes a service uses to reject an over-large batch
/// (`400 Bad Request`, `413 Payload Too Large`, `422 Unprocessable Entity`) as
/// "maybe too big — try smaller". This is deliberately status-based rather than
/// message-matched so it stays backend-agnostic: TEI answers `413`,
/// OpenAI-style services answer `400`. Auth (`401`/`403`), not-found (`404`),
/// and rate-limit (`429`) are excluded — shrinking would not help. If the cause
/// was not in fact batch size, the span shrinks to a single input and then
/// surfaces the underlying error as a hard failure (the v0 behavior).
fn should_shrink(e: &EmbeddingError) -> bool {
    matches!(
        e,
        EmbeddingError::Status {
            code: 400 | 413 | 422,
            ..
        }
    )
}

/// Boxed-future shape returned by [`Embedder::embed_text`]. Manual `Pin<Box<…>>`
/// (rather than `async fn` in the trait) so the trait stays object-safe and
/// `Scanner` can hold an `Arc<dyn Embedder>` without a generic parameter.
pub type EmbedFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Vec<f32>, EmbeddingError>> + Send + 'a>>;

/// Boxed-future shape returned by [`Embedder::embed_batch`].
pub type BatchFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Vec<Vec<f32>>, EmbeddingError>> + Send + 'a>>;

/// Abstraction over the embedding backend. Production wires
/// [`EmbeddingClient`]; tests wire [`StubEmbedder`] (or a custom impl) so the
/// indexer's chunk-and-embed pipeline can be exercised without a live HTTP
/// service.
pub trait Embedder: Send + Sync + 'static {
    fn embed_text<'a>(&'a self, text: &'a str) -> EmbedFuture<'a>;

    /// Embed many texts at once, returning one vector per input in order.
    ///
    /// The default implementation embeds sequentially via [`Self::embed_text`],
    /// so test doubles get a working batch path for free. [`EmbeddingClient`]
    /// overrides it to send real multi-input requests with adaptive shrink.
    fn embed_batch<'a>(&'a self, texts: &'a [&'a str]) -> BatchFuture<'a> {
        Box::pin(async move {
            let mut out = Vec::with_capacity(texts.len());
            for &text in texts {
                out.push(self.embed_text(text).await?);
            }
            Ok(out)
        })
    }
}

impl Embedder for EmbeddingClient {
    fn embed_text<'a>(&'a self, text: &'a str) -> EmbedFuture<'a> {
        Box::pin(self.embed(text))
    }

    fn embed_batch<'a>(&'a self, texts: &'a [&'a str]) -> BatchFuture<'a> {
        Box::pin(self.embed_many(texts))
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

/// Startup health probe for the embedding service. Sends a one-token
/// `embed("ping")` request and logs the outcome.
///
/// **Never fails the daemon.** Per Resolution 4 + the v0 skip-and-log
/// policy, embedding outages are tolerated at runtime — the indexer
/// classifies per-file. The probe is purely diagnostic so an operator
/// sees the service state at startup without grepping later indexer
/// logs.
///
/// Three log shapes:
/// - `INFO`: 200 + correct-dimension vector → service reachable.
/// - `WARN` (dimension mismatch): service responded with a wrong-length
///   vector. Names both numbers, the configured endpoint and model, and
///   the suggested resolution path. Indexing will continue per-file but
///   every file will fail to index until the model is fixed.
/// - `WARN` (other failure): transport / 5xx / 4xx / parse error.
///   Daemon stays up; chunking will skip-and-log per file.
pub async fn embed_health_probe(client: &EmbeddingClient, cfg: &EmbeddingConfig) {
    match client.embed("ping").await {
        Ok(v) => {
            tracing::info!(
                endpoint = %cfg.endpoint,
                model = %cfg.model,
                vector_len = v.len(),
                "embedding service reachable, vector length matches dimension"
            );
        }
        Err(EmbeddingError::DimensionMismatch { expected, actual }) => {
            tracing::warn!(
                endpoint = %cfg.endpoint,
                model = %cfg.model,
                expected_dimension = expected,
                observed_dimension = actual,
                "embedding service returned a vector with the wrong dimension; \
                 service may be configured to a different model — check `embedding.model` \
                 and the endpoint's loaded model. Daemon stays up; chunking will skip-and-log per file."
            );
        }
        Err(e) => {
            tracing::warn!(
                endpoint = %cfg.endpoint,
                model = %cfg.model,
                error = %e,
                "embedding service not reachable at startup; chunking will skip-and-log per file"
            );
        }
    }
}

#[derive(Deserialize)]
struct ApiResponse {
    data: Vec<EmbeddingItem>,
}

#[derive(Deserialize)]
struct EmbeddingItem {
    embedding: Vec<f32>,
    /// Position of this vector's input in the request. Always present in the
    /// OpenAI contract; `Option` so a service that omits it deserializes to
    /// `None` (distinct from an explicit `0`) and the client can tell "index
    /// omitted" apart from "index present-but-wrong".
    index: Option<usize>,
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

    /// `n` embeddings of width `dim`, each tagged with its input `index`.
    fn batch_response_body(dim: usize, n: usize) -> String {
        let data: Vec<_> = (0..n)
            .map(|i| {
                let vec: Vec<f32> = (0..dim).map(|j| (j as f32) * 0.001).collect();
                serde_json::json!({ "embedding": vec, "index": i })
            })
            .collect();
        serde_json::json!({ "data": data }).to_string()
    }

    /// One item per entry in `indices`, in that array order, each carrying the
    /// given `index` and an embedding whose first element equals that index so
    /// a test can assert how the client reordered them.
    fn body_with_indices(dim: usize, indices: &[usize]) -> String {
        let data: Vec<_> = indices
            .iter()
            .map(|&idx| {
                let mut v = vec![0.0_f32; dim];
                v[0] = idx as f32;
                serde_json::json!({ "embedding": v, "index": idx })
            })
            .collect();
        serde_json::json!({ "data": data }).to_string()
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
    async fn embed_many_single_request_within_ceiling() {
        let stub = StubServer::spawn(vec![StubAction::Respond {
            status: 200,
            body: batch_response_body(4, 3),
        }])
        .await;
        let cfg = EmbeddingConfig {
            batch_size: 4,
            ..test_config(&stub.url)
        };
        let client = EmbeddingClient::new(&cfg).unwrap();
        let texts: Vec<&str> = vec!["a", "b", "c"];
        let out = client.embed_many(&texts).await.expect("batch succeeds");
        assert_eq!(out.len(), 3);
        assert_eq!(stub.count(), 1, "three inputs fit one request at ceiling 4");
        stub.shutdown().await;
    }

    #[tokio::test]
    async fn embed_many_splits_over_ceiling() {
        let stub = StubServer::spawn(vec![
            StubAction::Respond {
                status: 200,
                body: batch_response_body(4, 2),
            },
            StubAction::Respond {
                status: 200,
                body: batch_response_body(4, 1),
            },
        ])
        .await;
        let cfg = EmbeddingConfig {
            batch_size: 2,
            ..test_config(&stub.url)
        };
        let client = EmbeddingClient::new(&cfg).unwrap();
        let texts: Vec<&str> = vec!["a", "b", "c"];
        let out = client
            .embed_many(&texts)
            .await
            .expect("split batch succeeds");
        assert_eq!(out.len(), 3);
        assert_eq!(stub.count(), 2, "ceiling 2 over 3 inputs => two requests");
        stub.shutdown().await;
    }

    #[tokio::test]
    async fn embed_many_shrinks_on_batch_rejection() {
        // First request (4 inputs) is rejected as too large; the client halves
        // to 2 and re-requests both spans. Three requests total.
        let stub = StubServer::spawn(vec![
            StubAction::Respond {
                status: 413,
                body: "batch size 4 > maximum allowed batch size 2".into(),
            },
            StubAction::Respond {
                status: 200,
                body: batch_response_body(4, 2),
            },
            StubAction::Respond {
                status: 200,
                body: batch_response_body(4, 2),
            },
        ])
        .await;
        let cfg = EmbeddingConfig {
            batch_size: 4,
            ..test_config(&stub.url)
        };
        let client = EmbeddingClient::new(&cfg).unwrap();
        let texts: Vec<&str> = vec!["a", "b", "c", "d"];
        let out = client
            .embed_many(&texts)
            .await
            .expect("shrink then succeed");
        assert_eq!(out.len(), 4);
        assert_eq!(stub.count(), 3, "one rejected + two halved-span requests");
        // A size-2 multi-input request succeeded, so the shared ceiling is
        // lowered to the working size and persists for later files.
        assert_eq!(client.max_batch.load(Ordering::Relaxed), 2);
        stub.shutdown().await;
    }

    #[tokio::test]
    async fn embed_many_single_input_rejection_does_not_lower_ceiling() {
        // One input rejected (size 1, no shrink possible) is a hard error and
        // must NOT lower the shared ceiling — a single bad chunk should not
        // disable batching daemon-wide for every other file.
        let stub = StubServer::spawn(vec![StubAction::Respond {
            status: 413,
            body: "input too long".into(),
        }])
        .await;
        let cfg = EmbeddingConfig {
            batch_size: 8,
            ..test_config(&stub.url)
        };
        let client = EmbeddingClient::new(&cfg).unwrap();
        let texts: Vec<&str> = vec!["a"];
        let err = client
            .embed_many(&texts)
            .await
            .expect_err("single-input rejection");
        assert!(matches!(err, EmbeddingError::Status { code: 413, .. }));
        assert_eq!(
            client.max_batch.load(Ordering::Relaxed),
            8,
            "ceiling unchanged by a single-input rejection"
        );
        stub.shutdown().await;
    }

    #[tokio::test]
    async fn embed_many_single_input_rejection_is_hard_error() {
        // A rejection that cannot be shrunk past size 1 surfaces as an error.
        let stub = StubServer::spawn(vec![StubAction::Respond {
            status: 413,
            body: "input too long".into(),
        }])
        .await;
        let cfg = EmbeddingConfig {
            batch_size: 1,
            ..test_config(&stub.url)
        };
        let client = EmbeddingClient::new(&cfg).unwrap();
        let texts: Vec<&str> = vec!["a"];
        let err = client
            .embed_many(&texts)
            .await
            .expect_err("413 on a single input is a hard error");
        assert!(
            matches!(err, EmbeddingError::Status { code: 413, .. }),
            "expected Status 413, got {err:?}"
        );
        assert_eq!(stub.count(), 1, "no shrink retry possible at size 1");
        stub.shutdown().await;
    }

    #[tokio::test]
    async fn embed_many_count_mismatch_classified() {
        // Service returns fewer vectors than inputs sent.
        let stub = StubServer::spawn(vec![StubAction::Respond {
            status: 200,
            body: batch_response_body(4, 1),
        }])
        .await;
        let cfg = EmbeddingConfig {
            batch_size: 4,
            ..test_config(&stub.url)
        };
        let client = EmbeddingClient::new(&cfg).unwrap();
        let texts: Vec<&str> = vec!["a", "b"];
        let err = client.embed_many(&texts).await.expect_err("count mismatch");
        match err {
            EmbeddingError::CountMismatch { expected, actual } => {
                assert_eq!(expected, 2);
                assert_eq!(actual, 1);
            }
            other => panic!("expected CountMismatch; got {other:?}"),
        }
        stub.shutdown().await;
    }

    #[tokio::test]
    async fn embed_many_reorders_by_response_index() {
        // Service returns items out of order; the client must realign by `index`.
        let stub = StubServer::spawn(vec![StubAction::Respond {
            status: 200,
            body: body_with_indices(4, &[2, 0, 1]),
        }])
        .await;
        let cfg = EmbeddingConfig {
            batch_size: 4,
            ..test_config(&stub.url)
        };
        let client = EmbeddingClient::new(&cfg).unwrap();
        let texts: Vec<&str> = vec!["a", "b", "c"];
        let out = client.embed_many(&texts).await.expect("reordered batch");
        // body_with_indices encodes the index in element 0, so a correct
        // realignment yields ascending first elements.
        assert_eq!(out[0][0], 0.0);
        assert_eq!(out[1][0], 1.0);
        assert_eq!(out[2][0], 2.0);
        stub.shutdown().await;
    }

    #[tokio::test]
    async fn embed_many_rejects_noncontiguous_indices() {
        // Indices {0, 2} for two inputs: not all-zero (so not the omitted case),
        // and not a 0..len permutation -> hard ResponseShape error.
        let stub = StubServer::spawn(vec![StubAction::Respond {
            status: 200,
            body: body_with_indices(4, &[0, 2]),
        }])
        .await;
        let cfg = EmbeddingConfig {
            batch_size: 4,
            ..test_config(&stub.url)
        };
        let client = EmbeddingClient::new(&cfg).unwrap();
        let texts: Vec<&str> = vec!["a", "b"];
        let err = client
            .embed_many(&texts)
            .await
            .expect_err("noncontiguous indices");
        assert!(
            matches!(err, EmbeddingError::ResponseShape(_)),
            "expected ResponseShape, got {err:?}"
        );
        stub.shutdown().await;
    }

    #[tokio::test]
    async fn embed_many_rejects_explicit_all_zero_indices() {
        // Every item explicitly carries `index: 0`. This is now distinguishable
        // from an omitted index (Option::None) and must fail validation rather
        // than be mistaken for "trust array order".
        let stub = StubServer::spawn(vec![StubAction::Respond {
            status: 200,
            body: body_with_indices(4, &[0, 0]),
        }])
        .await;
        let cfg = EmbeddingConfig {
            batch_size: 4,
            ..test_config(&stub.url)
        };
        let client = EmbeddingClient::new(&cfg).unwrap();
        let texts: Vec<&str> = vec!["a", "b"];
        let err = client
            .embed_many(&texts)
            .await
            .expect_err("explicit duplicate index 0");
        assert!(
            matches!(err, EmbeddingError::ResponseShape(_)),
            "expected ResponseShape, got {err:?}"
        );
        stub.shutdown().await;
    }

    #[tokio::test]
    async fn embed_many_trusts_array_order_when_index_omitted() {
        // A response that omits `index` deserializes all-zero; the client must
        // fall back to array order rather than erroring on the "duplicate" 0s.
        let body = serde_json::json!({
            "data": [
                { "embedding": vec![1.0_f32, 0.0, 0.0, 0.0] },
                { "embedding": vec![2.0_f32, 0.0, 0.0, 0.0] },
            ]
        })
        .to_string();
        let stub = StubServer::spawn(vec![StubAction::Respond { status: 200, body }]).await;
        let cfg = EmbeddingConfig {
            batch_size: 4,
            ..test_config(&stub.url)
        };
        let client = EmbeddingClient::new(&cfg).unwrap();
        let texts: Vec<&str> = vec!["a", "b"];
        let out = client
            .embed_many(&texts)
            .await
            .expect("array order trusted");
        assert_eq!(out[0][0], 1.0);
        assert_eq!(out[1][0], 2.0);
        stub.shutdown().await;
    }

    #[test]
    fn truncate_for_display_bounds_long_bodies() {
        let short = "small error";
        assert_eq!(truncate_for_display(short), short);

        let long = "x".repeat(1000);
        let rendered = truncate_for_display(&long);
        assert!(rendered.len() < long.len());
        assert!(rendered.contains("1000 bytes total"));
        assert!(rendered.starts_with("xxx"));
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
