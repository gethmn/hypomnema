use rmcp::ServerHandler;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_handler, tool_router};
use serde_json::{Value, json};

use crate::api::types::{
    ContentQueryJson, CreateVaultRequest, FilesystemQueryJson, SemanticQueryJson, VaultCreateInput,
    VaultStatusInput, VaultTerminateInput,
};
use crate::client::{DaemonClient, is_connect_error};

#[derive(Clone)]
pub struct HypomnemaMcpServer {
    pub client: DaemonClient,
    pub default_vault_name: String,
    pub enable_write_tools: bool,
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

    #[tool(
        description = "List all registered vaults with their status, path, and creation time. \
                       See docs/specs/vault-management.md § Operations."
    )]
    async fn vault_list(&self) -> CallToolResult {
        match self.client.list_vaults().await {
            Ok(resp) => CallToolResult::structured(
                serde_json::to_value(resp).expect("response is JSON-serializable"),
            ),
            Err(err) => CallToolResult::structured_error(envelope_from_anyhow(&self.client, &err)),
        }
    }

    #[tool(
        description = "Get detail for a single vault by name or ID. Defaults to the configured \
                       default vault when target is omitted. \
                       See docs/specs/vault-management.md § Operations."
    )]
    async fn vault_status(
        &self,
        Parameters(input): Parameters<VaultStatusInput>,
    ) -> CallToolResult {
        let target = input
            .target
            .as_deref()
            .unwrap_or(self.default_vault_name.as_str());
        match self.client.get_vault(target).await {
            Ok(resp) => CallToolResult::structured(
                serde_json::to_value(resp).expect("response is JSON-serializable"),
            ),
            Err(err) => CallToolResult::structured_error(envelope_from_anyhow(&self.client, &err)),
        }
    }

    #[tool(
        description = "Create a new vault. Path must be absolute and exist. Name defaults to the \
                       configured default name. Disabled when [mcp] enable_write_tools = false. \
                       See docs/specs/vault-management.md § Operations § create."
    )]
    async fn vault_create(
        &self,
        Parameters(input): Parameters<VaultCreateInput>,
    ) -> CallToolResult {
        if !self.enable_write_tools {
            return CallToolResult::structured_error(write_tools_disabled_envelope("vault_create"));
        }
        let req = CreateVaultRequest {
            name: input.name,
            path: input.path,
        };
        match self.client.create_vault(&req).await {
            Ok(resp) => CallToolResult::structured(
                serde_json::to_value(resp).expect("response is JSON-serializable"),
            ),
            Err(err) => CallToolResult::structured_error(envelope_from_anyhow(&self.client, &err)),
        }
    }

    #[tool(
        description = "Permanently remove a vault from the registry; deletes its index and event \
                       log; never touches the vault directory itself. Disabled when \
                       [mcp] enable_write_tools = false. \
                       See docs/specs/vault-management.md § Operations § terminate."
    )]
    async fn vault_terminate(
        &self,
        Parameters(input): Parameters<VaultTerminateInput>,
    ) -> CallToolResult {
        if !self.enable_write_tools {
            return CallToolResult::structured_error(write_tools_disabled_envelope(
                "vault_terminate",
            ));
        }
        match self.client.terminate_vault(&input.target).await {
            Ok(resp) => CallToolResult::structured(
                serde_json::to_value(resp).expect("response is JSON-serializable"),
            ),
            Err(err) => CallToolResult::structured_error(envelope_from_anyhow(&self.client, &err)),
        }
    }
}

fn write_tools_disabled_envelope(tool_name: &str) -> Value {
    json!({
        "error": {
            "code": "write_tools_disabled",
            "message": format!(
                "{tool_name} is disabled by config; set [mcp] enable_write_tools = true to enable vault.create/terminate"
            ),
        }
    })
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
    use axum::extract::Path as AxumPath;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    use axum::routing::{delete, get, post};
    use serde_json::json;
    use tempfile::TempDir;
    use tokio::net::TcpListener;
    use tokio::sync::watch;
    use tokio::task::JoinHandle;

    use super::*;
    use crate::api::types::{
        ContentMatchJson, ContentResultJson, ContentSearchResponse, FilesystemResultJson,
        FilesystemSearchResponse, SemanticResultJson, SemanticSearchResponse,
        TerminateVaultResponse, VaultListResponse, VaultRowJson,
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
        HypomnemaMcpServer {
            client,
            default_vault_name: cfg.default_vault_name.clone(),
            enable_write_tools: true,
        }
    }

    fn server_against_with_writes(url: &str, enable_write_tools: bool) -> HypomnemaMcpServer {
        let cfg = smoke_config();
        let client = DaemonClient::from_config(&cfg, Some(url)).unwrap();
        HypomnemaMcpServer {
            client,
            default_vault_name: cfg.default_vault_name.clone(),
            enable_write_tools,
        }
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

    fn sample_vault_row(name: &str) -> VaultRowJson {
        VaultRowJson {
            id: "018f3a7c-9b4e-7d2a-95f1-c8a6e3b2d1f0".into(),
            name: name.into(),
            path: format!("/tmp/{name}"),
            status: "active".into(),
            created_at: "2026-04-28T00:00:00Z".into(),
            last_error: None,
        }
    }

    #[tokio::test]
    async fn mcp_vault_list_returns_list() {
        let app = Router::new().route(
            "/vaults",
            get(|| async {
                axum::Json(VaultListResponse {
                    vaults: vec![sample_vault_row("default"), sample_vault_row("personal")],
                })
            }),
        );
        let mock = MockDaemon::spawn(app).await;
        let server = server_against(&mock.base_url);

        let result = server.vault_list().await;

        assert!(!result.is_error.unwrap_or(false));
        let value = result
            .structured_content
            .expect("structured content present");
        assert_eq!(value["vaults"][0]["name"], json!("default"));
        assert_eq!(value["vaults"][1]["name"], json!("personal"));
        mock.shutdown().await;
    }

    #[tokio::test]
    async fn mcp_vault_status_returns_single() {
        let app =
            Router::new().route(
                "/vaults/:name_or_id",
                get(|AxumPath(name): AxumPath<String>| async move {
                    axum::Json(sample_vault_row(&name))
                }),
            );
        let mock = MockDaemon::spawn(app).await;
        let server = server_against(&mock.base_url);

        let result = server
            .vault_status(Parameters(VaultStatusInput {
                target: Some("personal".into()),
            }))
            .await;

        assert!(!result.is_error.unwrap_or(false));
        let value = result
            .structured_content
            .expect("structured content present");
        assert_eq!(value["name"], json!("personal"));
        assert_eq!(value["status"], json!("active"));
        mock.shutdown().await;
    }

    #[tokio::test]
    async fn mcp_vault_status_uses_default_vault_name_when_target_omitted() {
        // Defends the `default_vault_name` fallback: with `target = None`,
        // the tool addresses the daemon at the configured default name.
        let app =
            Router::new().route(
                "/vaults/:name_or_id",
                get(|AxumPath(name): AxumPath<String>| async move {
                    axum::Json(sample_vault_row(&name))
                }),
            );
        let mock = MockDaemon::spawn(app).await;
        let server = server_against(&mock.base_url);

        let result = server
            .vault_status(Parameters(VaultStatusInput { target: None }))
            .await;

        assert!(!result.is_error.unwrap_or(false));
        let value = result
            .structured_content
            .expect("structured content present");
        // server_against uses Config::default_for_smoke_test which sets
        // default_vault_name = "default".
        assert_eq!(value["name"], json!("default"));
        mock.shutdown().await;
    }

    #[tokio::test]
    async fn mcp_vault_create_succeeds_when_write_tools_enabled() {
        let app = Router::new().route(
            "/vaults",
            post(|| async { axum::Json(sample_vault_row("notes")) }),
        );
        let mock = MockDaemon::spawn(app).await;
        let server = server_against_with_writes(&mock.base_url, true);

        let result = server
            .vault_create(Parameters(VaultCreateInput {
                name: Some("notes".into()),
                path: "/tmp/notes".into(),
            }))
            .await;

        assert!(!result.is_error.unwrap_or(false));
        let value = result
            .structured_content
            .expect("structured content present");
        assert_eq!(value["name"], json!("notes"));
        assert_eq!(value["status"], json!("active"));
        mock.shutdown().await;
    }

    #[tokio::test]
    async fn mcp_vault_create_returns_write_tools_disabled_when_gated() {
        // No mock daemon needed — the short-circuit must fire before any HTTP
        // call happens. Bind a port to get a usable URL, drop the listener so
        // a stray request would error obviously, but expect no request.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let url = format!("http://{addr}");
        let server = server_against_with_writes(&url, false);

        let result = server
            .vault_create(Parameters(VaultCreateInput {
                name: None,
                path: "/tmp/x".into(),
            }))
            .await;

        assert!(result.is_error.unwrap_or(false), "expected gated error");
        let value = result
            .structured_content
            .expect("structured content present");
        assert_eq!(value["error"]["code"], json!("write_tools_disabled"));
        let message = value["error"]["message"].as_str().unwrap_or("");
        assert!(
            message.contains("vault_create"),
            "message should reference the gated tool, got {message:?}"
        );
        assert!(
            message.contains("enable_write_tools"),
            "message should reference the config knob, got {message:?}"
        );
    }

    #[tokio::test]
    async fn mcp_vault_terminate_succeeds_when_write_tools_enabled() {
        let app = Router::new().route(
            "/vaults/:name_or_id",
            delete(|AxumPath(_name): AxumPath<String>| async move {
                axum::Json(TerminateVaultResponse {
                    terminated: true,
                    id: "018f3a7c-9b4e-7d2a-95f1-c8a6e3b2d1f0".into(),
                })
            }),
        );
        let mock = MockDaemon::spawn(app).await;
        let server = server_against_with_writes(&mock.base_url, true);

        let result = server
            .vault_terminate(Parameters(VaultTerminateInput {
                target: "personal".into(),
            }))
            .await;

        assert!(!result.is_error.unwrap_or(false));
        let value = result
            .structured_content
            .expect("structured content present");
        assert_eq!(value["terminated"], json!(true));
        assert_eq!(value["id"], json!("018f3a7c-9b4e-7d2a-95f1-c8a6e3b2d1f0"));
        mock.shutdown().await;
    }

    #[tokio::test]
    async fn mcp_vault_terminate_returns_write_tools_disabled_when_gated() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let url = format!("http://{addr}");
        let server = server_against_with_writes(&url, false);

        let result = server
            .vault_terminate(Parameters(VaultTerminateInput {
                target: "personal".into(),
            }))
            .await;

        assert!(result.is_error.unwrap_or(false), "expected gated error");
        let value = result
            .structured_content
            .expect("structured content present");
        assert_eq!(value["error"]["code"], json!("write_tools_disabled"));
        let message = value["error"]["message"].as_str().unwrap_or("");
        assert!(
            message.contains("vault_terminate"),
            "message should reference the gated tool, got {message:?}"
        );
    }

    #[tokio::test]
    async fn mcp_vault_list_propagates_daemon_unreachable_envelope() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let url = format!("http://{addr}");
        let server = server_against(&url);

        let result = server.vault_list().await;

        assert!(result.is_error.unwrap_or(false));
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
}
