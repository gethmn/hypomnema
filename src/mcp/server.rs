use rmcp::ServerHandler;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_handler, tool_router};
use serde_json::{Value, json};

use crate::api::types::{ContentQueryJson, FilesystemQueryJson, SemanticQueryJson};
use crate::client::{DaemonClient, is_connect_error};

#[derive(Clone)]
pub struct HypomnemaMcpServer {
    pub client: DaemonClient,
}

// Brand-identity override: surface "hypomnema" as the MCP serverInfo.name to MCP hosts
// (Claude Code, Iris) instead of the auto-derived "rmcp". Version is auto-filled by
// the tool_handler macro from `env!("CARGO_PKG_VERSION")` when `name` is provided
// without an explicit `version`. See ADR-0012.
#[tool_handler(name = "hypomnema")]
impl ServerHandler for HypomnemaMcpServer {}

#[tool_router]
impl HypomnemaMcpServer {
    #[tool(description = "List vault files matching a path prefix and/or glob. \
                       Cheapest of the three search modes; the typical first step \
                       when exploring an unfamiliar vault. See docs/specs/filesystem-search.md.")]
    async fn search_filesystem(
        &self,
        Parameters(input): Parameters<FilesystemQueryJson>,
    ) -> CallToolResult {
        match self.client.search_filesystem(&input).await {
            Ok(resp) => CallToolResult::structured(
                serde_json::to_value(resp).expect("response is JSON-serializable"),
            ),
            Err(err) => CallToolResult::structured_error(envelope_from_anyhow(&self.client, &err)),
        }
    }

    #[tool(
        description = "Search file contents by substring or regex. Grep-shaped — \
                       answers \"which files contain this exact phrase?\". \
                       See docs/specs/content-search.md."
    )]
    async fn search_content(
        &self,
        Parameters(input): Parameters<ContentQueryJson>,
    ) -> CallToolResult {
        match self.client.search_content(&input).await {
            Ok(resp) => CallToolResult::structured(
                serde_json::to_value(resp).expect("response is JSON-serializable"),
            ),
            Err(err) => CallToolResult::structured_error(envelope_from_anyhow(&self.client, &err)),
        }
    }

    #[tool(
        description = "Semantic search via cosine similarity over indexed chunk \
                       embeddings. Answers \"what in this vault is conceptually \
                       similar to this idea?\". See docs/specs/semantic-search.md."
    )]
    async fn search_semantic(
        &self,
        Parameters(input): Parameters<SemanticQueryJson>,
    ) -> CallToolResult {
        match self.client.search_semantic(&input).await {
            Ok(resp) => CallToolResult::structured(
                serde_json::to_value(resp).expect("response is JSON-serializable"),
            ),
            Err(err) => CallToolResult::structured_error(envelope_from_anyhow(&self.client, &err)),
        }
    }
}

fn envelope_from_anyhow(client: &DaemonClient, err: &anyhow::Error) -> Value {
    if is_connect_error(err) {
        return daemon_unreachable_envelope(client.base_url(), err);
    }
    let display = format!("{err:#}");
    let (code, message) = match display.split_once(": ") {
        Some((c, m)) => (c.to_string(), m.to_string()),
        None => ("internal".to_string(), display),
    };
    json!({ "error": { "code": code, "message": message } })
}

pub fn daemon_unreachable_envelope(url: &str, err: &anyhow::Error) -> Value {
    json!({
        "error": {
            "code": "daemon_unreachable",
            "message": format!("{} did not respond: {:#}", url, err),
        }
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use anyhow::anyhow;
    use axum::Router;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    use axum::routing::post;
    use serde_json::json;
    use tempfile::TempDir;
    use tokio::net::TcpListener;
    use tokio::sync::watch;
    use tokio::task::JoinHandle;

    use super::*;
    use crate::api::types::{
        ContentMatchJson, ContentResultJson, ContentSearchResponse, FilesystemResultJson,
        FilesystemSearchResponse, SemanticResultJson, SemanticSearchResponse,
    };
    use crate::config::Config;

    struct MockDaemon {
        base_url: String,
        shutdown: watch::Sender<bool>,
        handle: Option<JoinHandle<()>>,
        _vault: TempDir,
    }

    impl MockDaemon {
        async fn spawn(app: Router) -> Self {
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
            Self {
                base_url: format!("http://{addr}"),
                shutdown: tx,
                handle: Some(handle),
                _vault: TempDir::new().unwrap(),
            }
        }

        async fn shutdown(mut self) {
            let _ = self.shutdown.send(true);
            if let Some(h) = self.handle.take() {
                let _ = h.await;
            }
        }
    }

    fn smoke_config() -> Config {
        Config::default_for_smoke_test(PathBuf::from("/tmp/hypomnema-mcp-tests"))
    }

    fn server_against(url: &str) -> HypomnemaMcpServer {
        let cfg = smoke_config();
        let client = DaemonClient::from_config(&cfg, Some(url)).unwrap();
        HypomnemaMcpServer { client }
    }

    #[tokio::test]
    async fn mcp_search_filesystem_round_trips_success() {
        let app = Router::new().route(
            "/search/filesystem",
            post(|| async {
                axum::Json(FilesystemSearchResponse {
                    results: vec![FilesystemResultJson {
                        path: "notes/foo.md".into(),
                        size: 42,
                        mtime: "2026-04-27T00:00:00Z".into(),
                        content_hash: "abc".into(),
                        vault: None,
                        vault_name: None,
                    }],
                    truncated: false,
                    partial_results: None,
                })
            }),
        );
        let mock = MockDaemon::spawn(app).await;
        let server = server_against(&mock.base_url);

        let result = server
            .search_filesystem(Parameters(FilesystemQueryJson {
                glob: Some("**/*.md".into()),
                ..Default::default()
            }))
            .await;

        assert!(!result.is_error.unwrap_or(false), "expected success");
        let value = result
            .structured_content
            .expect("structured content present");
        assert_eq!(value["truncated"], json!(false));
        assert_eq!(value["results"][0]["path"], json!("notes/foo.md"));
        assert_eq!(value["results"][0]["size"], json!(42));
        mock.shutdown().await;
    }

    #[tokio::test]
    async fn mcp_search_content_round_trips_success() {
        let app = Router::new().route(
            "/search/content",
            post(|| async {
                axum::Json(ContentSearchResponse {
                    results: vec![ContentResultJson {
                        path: "notes/bar.md".into(),
                        match_count: 1,
                        matches: vec![ContentMatchJson {
                            line: 7,
                            text: "the quick brown fox".into(),
                        }],
                        vault: None,
                        vault_name: None,
                    }],
                    truncated: false,
                    partial_results: None,
                })
            }),
        );
        let mock = MockDaemon::spawn(app).await;
        let server = server_against(&mock.base_url);

        let result = server
            .search_content(Parameters(ContentQueryJson {
                query: "fox".into(),
                regex: false,
                case_sensitive: false,
                prefix: None,
                include_matches: true,
                max_matches_per_file: None,
                limit: None,
                vaults: None,
            }))
            .await;

        assert!(!result.is_error.unwrap_or(false));
        let value = result
            .structured_content
            .expect("structured content present");
        assert_eq!(value["results"][0]["path"], json!("notes/bar.md"));
        assert_eq!(value["results"][0]["matches"][0]["line"], json!(7));
        mock.shutdown().await;
    }

    #[tokio::test]
    async fn mcp_search_semantic_round_trips_success() {
        let app = Router::new().route(
            "/search/semantic",
            post(|| async {
                axum::Json(SemanticSearchResponse {
                    results: vec![SemanticResultJson {
                        score: 0.75,
                        file_path: "notes/baz.md".into(),
                        chunk_index: 2,
                        heading_path: vec!["H1".into(), "H2".into()],
                        text: "some chunk text".into(),
                        vault: None,
                        vault_name: None,
                    }],
                    hint: None,
                    partial_results: None,
                })
            }),
        );
        let mock = MockDaemon::spawn(app).await;
        let server = server_against(&mock.base_url);

        let result = server
            .search_semantic(Parameters(SemanticQueryJson {
                query: "anything".into(),
                ..Default::default()
            }))
            .await;

        assert!(!result.is_error.unwrap_or(false));
        let value = result
            .structured_content
            .expect("structured content present");
        assert_eq!(value["results"][0]["file_path"], json!("notes/baz.md"));
        assert_eq!(value["results"][0]["chunk_index"], json!(2));
        mock.shutdown().await;
    }

    #[tokio::test]
    async fn mcp_search_semantic_propagates_embedding_unavailable_envelope() {
        let app = Router::new().route(
            "/search/semantic",
            post(|| async {
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    axum::Json(json!({
                        "error": {
                            "code": "embedding_unavailable",
                            "message": "stub: embedding service down"
                        }
                    })),
                )
                    .into_response()
            }),
        );
        let mock = MockDaemon::spawn(app).await;
        let server = server_against(&mock.base_url);

        let result = server
            .search_semantic(Parameters(SemanticQueryJson {
                query: "anything".into(),
                ..Default::default()
            }))
            .await;

        assert!(result.is_error.unwrap_or(false), "expected error result");
        let value = result
            .structured_content
            .expect("structured content present");
        assert_eq!(value["error"]["code"], json!("embedding_unavailable"));
        mock.shutdown().await;
    }

    #[tokio::test]
    async fn mcp_search_filesystem_propagates_invalid_glob_envelope() {
        let app = Router::new().route(
            "/search/filesystem",
            post(|| async {
                (
                    StatusCode::BAD_REQUEST,
                    axum::Json(json!({
                        "error": {
                            "code": "invalid_glob",
                            "message": "unterminated character class"
                        }
                    })),
                )
                    .into_response()
            }),
        );
        let mock = MockDaemon::spawn(app).await;
        let server = server_against(&mock.base_url);

        let result = server
            .search_filesystem(Parameters(FilesystemQueryJson {
                glob: Some("[unterminated".into()),
                ..Default::default()
            }))
            .await;

        assert!(result.is_error.unwrap_or(false), "expected error result");
        let value = result
            .structured_content
            .expect("structured content present");
        assert_eq!(value["error"]["code"], json!("invalid_glob"));
        mock.shutdown().await;
    }

    #[tokio::test]
    async fn mcp_search_filesystem_synthesizes_daemon_unreachable_for_connect_error() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let url = format!("http://{addr}");
        let server = server_against(&url);

        let result = server
            .search_filesystem(Parameters(FilesystemQueryJson::default()))
            .await;

        assert!(result.is_error.unwrap_or(false), "expected error result");
        let value = result
            .structured_content
            .expect("structured content present");
        assert_eq!(value["error"]["code"], json!("daemon_unreachable"));
        let message = value["error"]["message"].as_str().unwrap_or("");
        assert!(
            message.contains(&url),
            "message should contain configured URL, got {message:?}"
        );
    }

    #[test]
    fn daemon_unreachable_envelope_shape() {
        let err = anyhow!("connection refused");
        let env = daemon_unreachable_envelope("http://x.invalid", &err);
        assert_eq!(env["error"]["code"], json!("daemon_unreachable"));
        let message = env["error"]["message"].as_str().unwrap_or("");
        assert!(
            message.contains("http://x.invalid"),
            "message should embed URL, got {message:?}"
        );
        assert!(
            message.contains("connection refused"),
            "message should embed cause, got {message:?}"
        );
    }
}
