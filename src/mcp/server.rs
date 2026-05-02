use std::sync::Arc;

use rmcp::ServerHandler;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_handler, tool_router};
use serde_json::{Value, json};

use crate::api::types::{
    ContentGetRequest, ContentQueryJson, CreateVaultRequest, FilesystemQueryJson, SemanticQueryJson,
    VaultCreateInput, VaultPauseInput, VaultRenameInput, VaultRescanInput, VaultResetInput,
    VaultResumeInput, VaultStatusInput, VaultTerminateInput,
};
use crate::mcp::backend::HypomnemaBackend;

#[derive(Clone)]
pub struct HypomnemaMcpServer {
    pub backend: Arc<dyn HypomnemaBackend + Send + Sync>,
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
        match self.backend.search_filesystem(&input).await {
            Ok(resp) => CallToolResult::structured(
                serde_json::to_value(resp).expect("response is JSON-serializable"),
            ),
            Err(err) => {
                CallToolResult::structured_error(envelope_from_anyhow(&*self.backend, &err))
            }
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
        match self.backend.search_content(&input).await {
            Ok(resp) => CallToolResult::structured(
                serde_json::to_value(resp).expect("response is JSON-serializable"),
            ),
            Err(err) => {
                CallToolResult::structured_error(envelope_from_anyhow(&*self.backend, &err))
            }
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
        match self.backend.search_semantic(&input).await {
            Ok(resp) => CallToolResult::structured(
                serde_json::to_value(resp).expect("response is JSON-serializable"),
            ),
            Err(err) => {
                CallToolResult::structured_error(envelope_from_anyhow(&*self.backend, &err))
            }
        }
    }

    #[tool(description = "Fetch full indexed file content by vault-relative path. Returns content \
                          from the search index — never reads from the vault filesystem at query \
                          time. Supports batching multiple paths and cross-vault fan-out.")]
    async fn content_get(
        &self,
        Parameters(input): Parameters<ContentGetRequest>,
    ) -> CallToolResult {
        match self.backend.content_get(&input).await {
            Ok(resp) => CallToolResult::structured(
                serde_json::to_value(resp).expect("response is JSON-serializable"),
            ),
            Err(err) => {
                CallToolResult::structured_error(envelope_from_anyhow(&*self.backend, &err))
            }
        }
    }

    #[tool(
        description = "List all registered vaults with their status, path, and creation time. \
                       See docs/specs/vault-management.md § Operations."
    )]
    async fn vault_list(&self) -> CallToolResult {
        match self.backend.list_vaults().await {
            Ok(resp) => CallToolResult::structured(
                serde_json::to_value(resp).expect("response is JSON-serializable"),
            ),
            Err(err) => {
                CallToolResult::structured_error(envelope_from_anyhow(&*self.backend, &err))
            }
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
        match self.backend.get_vault(target).await {
            Ok(resp) => CallToolResult::structured(
                serde_json::to_value(resp).expect("response is JSON-serializable"),
            ),
            Err(err) => {
                CallToolResult::structured_error(envelope_from_anyhow(&*self.backend, &err))
            }
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
        match self.backend.create_vault(&req).await {
            Ok(resp) => CallToolResult::structured(
                serde_json::to_value(resp).expect("response is JSON-serializable"),
            ),
            Err(err) => {
                CallToolResult::structured_error(envelope_from_anyhow(&*self.backend, &err))
            }
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
        match self.backend.terminate_vault(&input.target).await {
            Ok(resp) => CallToolResult::structured(
                serde_json::to_value(resp).expect("response is JSON-serializable"),
            ),
            Err(err) => {
                CallToolResult::structured_error(envelope_from_anyhow(&*self.backend, &err))
            }
        }
    }

    #[tool(
        description = "Pause a vault: stop its watcher and indexer; index preserved; vault \
                       silently skipped from default search scope. Disabled when \
                       [mcp] enable_write_tools = false. \
                       See docs/specs/vault-management.md § Operations § pause."
    )]
    async fn vault_pause(&self, Parameters(input): Parameters<VaultPauseInput>) -> CallToolResult {
        if !self.enable_write_tools {
            return CallToolResult::structured_error(write_tools_disabled_envelope("vault_pause"));
        }
        match self.backend.pause_vault(&input.target).await {
            Ok(resp) => CallToolResult::structured(
                serde_json::to_value(resp).expect("response is JSON-serializable"),
            ),
            Err(err) => {
                CallToolResult::structured_error(envelope_from_anyhow(&*self.backend, &err))
            }
        }
    }

    #[tool(
        description = "Resume a paused or errored vault: restart watcher and indexer; clears \
                       last_error if the vault was errored. Disabled when \
                       [mcp] enable_write_tools = false. \
                       See docs/specs/vault-management.md § Operations § resume."
    )]
    async fn vault_resume(
        &self,
        Parameters(input): Parameters<VaultResumeInput>,
    ) -> CallToolResult {
        if !self.enable_write_tools {
            return CallToolResult::structured_error(write_tools_disabled_envelope("vault_resume"));
        }
        match self.backend.resume_vault(&input.target).await {
            Ok(resp) => CallToolResult::structured(
                serde_json::to_value(resp).expect("response is JSON-serializable"),
            ),
            Err(err) => {
                CallToolResult::structured_error(envelope_from_anyhow(&*self.backend, &err))
            }
        }
    }

    #[tool(
        description = "Reset a vault: clear last_error and restart watcher + indexer. With \
                       rebuild=true, also drop and rebuild chunks + chunks_vec (preserves files). \
                       Disabled when [mcp] enable_write_tools = false. \
                       See docs/specs/vault-management.md § Operations § reset."
    )]
    async fn vault_reset(&self, Parameters(input): Parameters<VaultResetInput>) -> CallToolResult {
        if !self.enable_write_tools {
            return CallToolResult::structured_error(write_tools_disabled_envelope("vault_reset"));
        }
        match self.backend.reset_vault(&input.target, input.rebuild).await {
            Ok(resp) => CallToolResult::structured(
                serde_json::to_value(resp).expect("response is JSON-serializable"),
            ),
            Err(err) => {
                CallToolResult::structured_error(envelope_from_anyhow(&*self.backend, &err))
            }
        }
    }

    #[tool(
        description = "Rename a vault. Updates the registry row's name and the per-vault \
                       meta.toml; the vault's surrogate id and on-disk path are unchanged. \
                       Disabled when [mcp] enable_write_tools = false. \
                       See docs/specs/vault-management.md § Operations § rename."
    )]
    async fn vault_rename(
        &self,
        Parameters(input): Parameters<VaultRenameInput>,
    ) -> CallToolResult {
        if !self.enable_write_tools {
            return CallToolResult::structured_error(write_tools_disabled_envelope("vault_rename"));
        }
        match self
            .backend
            .rename_vault(&input.target, &input.new_name)
            .await
        {
            Ok(resp) => CallToolResult::structured(
                serde_json::to_value(resp).expect("response is JSON-serializable"),
            ),
            Err(err) => {
                CallToolResult::structured_error(envelope_from_anyhow(&*self.backend, &err))
            }
        }
    }

    #[tool(
        description = "Trigger a one-shot rescan of a vault: walks the vault directory and emits \
                       modified events for files whose stat or content_hash changed. Disabled \
                       when [mcp] enable_write_tools = false. \
                       See docs/specs/vault-management.md § Operations § rescan."
    )]
    async fn vault_rescan(
        &self,
        Parameters(input): Parameters<VaultRescanInput>,
    ) -> CallToolResult {
        if !self.enable_write_tools {
            return CallToolResult::structured_error(write_tools_disabled_envelope("vault_rescan"));
        }
        match self.backend.rescan_vault(&input.target).await {
            Ok(resp) => CallToolResult::structured(
                serde_json::to_value(resp).expect("response is JSON-serializable"),
            ),
            Err(err) => {
                CallToolResult::structured_error(envelope_from_anyhow(&*self.backend, &err))
            }
        }
    }
}

fn write_tools_disabled_envelope(tool_name: &str) -> Value {
    json!({
        "error": {
            "code": "write_tools_disabled",
            "message": format!(
                "{tool_name} is disabled by config; set [mcp] enable_write_tools = true to enable vault management write tools"
            ),
        }
    })
}

fn envelope_from_anyhow(backend: &dyn HypomnemaBackend, err: &anyhow::Error) -> Value {
    if backend.is_connect_error(err) {
        return backend.daemon_unreachable_envelope(err);
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
        FilesystemSearchResponse, RescanResponseJson, SemanticResultJson, SemanticSearchResponse,
        TerminateVaultResponse, VaultListResponse, VaultRowJson,
    };
    use crate::client::DaemonClient;
    use crate::config::Config;
    use std::sync::{Arc, Mutex};

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
            backend: Arc::new(client),
            default_vault_name: cfg.default_vault_name.clone(),
            enable_write_tools: true,
        }
    }

    fn server_against_with_writes(url: &str, enable_write_tools: bool) -> HypomnemaMcpServer {
        let cfg = smoke_config();
        let client = DaemonClient::from_config(&cfg, Some(url)).unwrap();
        HypomnemaMcpServer {
            backend: Arc::new(client),
            default_vault_name: cfg.default_vault_name.clone(),
            enable_write_tools,
        }
    }

    #[test]
    fn hypomnema_mcp_server_holds_arc_dyn_backend() {
        // Type-check that the field is `Arc<dyn HypomnemaBackend + Send + Sync>`.
        // Catches accidental concrete-typing regression of the field.
        fn assert_field_type(_: &Arc<dyn HypomnemaBackend + Send + Sync>) {}
        let cfg = smoke_config();
        let client = DaemonClient::from_config(&cfg, None).unwrap();
        let server = HypomnemaMcpServer {
            backend: Arc::new(client),
            default_vault_name: cfg.default_vault_name.clone(),
            enable_write_tools: true,
        };
        assert_field_type(&server.backend);
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
                        text: Some("some chunk text".into()),
                        text_kind: Some("preview".into()),
                        text_truncated: Some(false),
                        content_hash: "sha256:baz".into(),
                        vault: None,
                        vault_name: None,
                    }],
                    truncated: false,
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
                "/vaults/{name_or_id}",
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
                "/vaults/{name_or_id}",
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
            "/vaults/{name_or_id}",
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

    fn unbound_url() -> String {
        // Used by the gated tests: short-circuit fires before any HTTP call,
        // so binding then dropping a port produces a usable URL where a stray
        // request would obviously error — but no request is expected.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn mcp_vault_pause_succeeds_when_write_tools_enabled() {
        let app = Router::new().route(
            "/vaults/{name_or_id}/pause",
            post(|AxumPath(name): AxumPath<String>| async move {
                let mut row = sample_vault_row(&name);
                row.status = "paused".into();
                axum::Json(row)
            }),
        );
        let mock = MockDaemon::spawn(app).await;
        let server = server_against_with_writes(&mock.base_url, true);

        let result = server
            .vault_pause(Parameters(VaultPauseInput {
                target: "personal".into(),
            }))
            .await;

        assert!(!result.is_error.unwrap_or(false));
        let value = result
            .structured_content
            .expect("structured content present");
        assert_eq!(value["name"], json!("personal"));
        assert_eq!(value["status"], json!("paused"));
        mock.shutdown().await;
    }

    #[tokio::test]
    async fn mcp_vault_pause_returns_write_tools_disabled_when_gated() {
        let url = unbound_url();
        let server = server_against_with_writes(&url, false);

        let result = server
            .vault_pause(Parameters(VaultPauseInput {
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
            message.contains("vault_pause"),
            "message should reference the gated tool, got {message:?}"
        );
    }

    #[tokio::test]
    async fn mcp_vault_resume_succeeds_when_write_tools_enabled() {
        let app =
            Router::new().route(
                "/vaults/{name_or_id}/resume",
                post(|AxumPath(name): AxumPath<String>| async move {
                    axum::Json(sample_vault_row(&name))
                }),
            );
        let mock = MockDaemon::spawn(app).await;
        let server = server_against_with_writes(&mock.base_url, true);

        let result = server
            .vault_resume(Parameters(VaultResumeInput {
                target: "personal".into(),
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
    async fn mcp_vault_resume_returns_write_tools_disabled_when_gated() {
        let url = unbound_url();
        let server = server_against_with_writes(&url, false);

        let result = server
            .vault_resume(Parameters(VaultResumeInput {
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
            message.contains("vault_resume"),
            "message should reference the gated tool, got {message:?}"
        );
    }

    #[tokio::test]
    async fn mcp_vault_reset_succeeds_when_write_tools_enabled() {
        let app =
            Router::new().route(
                "/vaults/{name_or_id}/reset",
                post(|AxumPath(name): AxumPath<String>| async move {
                    axum::Json(sample_vault_row(&name))
                }),
            );
        let mock = MockDaemon::spawn(app).await;
        let server = server_against_with_writes(&mock.base_url, true);

        let result = server
            .vault_reset(Parameters(VaultResetInput {
                target: "personal".into(),
                rebuild: false,
            }))
            .await;

        assert!(!result.is_error.unwrap_or(false));
        let value = result
            .structured_content
            .expect("structured content present");
        assert_eq!(value["name"], json!("personal"));
        mock.shutdown().await;
    }

    #[tokio::test]
    async fn mcp_vault_reset_returns_write_tools_disabled_when_gated() {
        let url = unbound_url();
        let server = server_against_with_writes(&url, false);

        let result = server
            .vault_reset(Parameters(VaultResetInput {
                target: "personal".into(),
                rebuild: true,
            }))
            .await;

        assert!(result.is_error.unwrap_or(false), "expected gated error");
        let value = result
            .structured_content
            .expect("structured content present");
        assert_eq!(value["error"]["code"], json!("write_tools_disabled"));
        let message = value["error"]["message"].as_str().unwrap_or("");
        assert!(
            message.contains("vault_reset"),
            "message should reference the gated tool, got {message:?}"
        );
    }

    #[tokio::test]
    async fn mcp_vault_reset_with_rebuild_passes_rebuild_through_to_daemon() {
        // Defends the rebuild round-trip: the server hands `input.rebuild` to
        // `client.reset_vault(target, rebuild)`, which serializes
        // `{"rebuild": <bool>}` on the wire. Capture the body server-side and
        // assert it parses to `rebuild = true`.
        let captured: Arc<Mutex<Option<Value>>> = Arc::new(Mutex::new(None));
        let captured_for_handler = captured.clone();
        let app = Router::new().route(
            "/vaults/{name_or_id}/reset",
            post(
                |AxumPath(name): AxumPath<String>, body: axum::body::Bytes| async move {
                    let parsed: Value = serde_json::from_slice(&body).expect("body is JSON");
                    *captured_for_handler.lock().unwrap() = Some(parsed);
                    axum::Json(sample_vault_row(&name))
                },
            ),
        );
        let mock = MockDaemon::spawn(app).await;
        let server = server_against_with_writes(&mock.base_url, true);

        let result = server
            .vault_reset(Parameters(VaultResetInput {
                target: "personal".into(),
                rebuild: true,
            }))
            .await;

        assert!(!result.is_error.unwrap_or(false));
        let body = captured
            .lock()
            .unwrap()
            .clone()
            .expect("handler should have captured a request body");
        assert_eq!(
            body,
            json!({ "rebuild": true }),
            "rebuild=true should round-trip through the client to the wire"
        );
        mock.shutdown().await;
    }

    #[tokio::test]
    async fn mcp_vault_rename_succeeds_when_write_tools_enabled() {
        let app = Router::new().route(
            "/vaults/{name_or_id}/rename",
            post(|body: axum::body::Bytes| async move {
                let parsed: Value = serde_json::from_slice(&body).expect("body is JSON");
                let new_name = parsed["new_name"]
                    .as_str()
                    .expect("new_name is a string")
                    .to_string();
                axum::Json(sample_vault_row(&new_name))
            }),
        );
        let mock = MockDaemon::spawn(app).await;
        let server = server_against_with_writes(&mock.base_url, true);

        let result = server
            .vault_rename(Parameters(VaultRenameInput {
                target: "personal".into(),
                new_name: "renamed".into(),
            }))
            .await;

        assert!(!result.is_error.unwrap_or(false));
        let value = result
            .structured_content
            .expect("structured content present");
        assert_eq!(value["name"], json!("renamed"));
        mock.shutdown().await;
    }

    #[tokio::test]
    async fn mcp_vault_rename_returns_write_tools_disabled_when_gated() {
        let url = unbound_url();
        let server = server_against_with_writes(&url, false);

        let result = server
            .vault_rename(Parameters(VaultRenameInput {
                target: "personal".into(),
                new_name: "renamed".into(),
            }))
            .await;

        assert!(result.is_error.unwrap_or(false), "expected gated error");
        let value = result
            .structured_content
            .expect("structured content present");
        assert_eq!(value["error"]["code"], json!("write_tools_disabled"));
        let message = value["error"]["message"].as_str().unwrap_or("");
        assert!(
            message.contains("vault_rename"),
            "message should reference the gated tool, got {message:?}"
        );
    }

    #[tokio::test]
    async fn mcp_vault_rescan_succeeds_when_write_tools_enabled() {
        let app = Router::new().route(
            "/vaults/{name_or_id}/rescan",
            post(|AxumPath(name): AxumPath<String>| async move {
                axum::Json(RescanResponseJson {
                    row: sample_vault_row(&name),
                    rescan_initiated_at: "2026-04-28T09:31:27.123456Z".into(),
                })
            }),
        );
        let mock = MockDaemon::spawn(app).await;
        let server = server_against_with_writes(&mock.base_url, true);

        let result = server
            .vault_rescan(Parameters(VaultRescanInput {
                target: "personal".into(),
            }))
            .await;

        assert!(!result.is_error.unwrap_or(false));
        let value = result
            .structured_content
            .expect("structured content present");
        assert_eq!(value["name"], json!("personal"));
        assert_eq!(
            value["rescan_initiated_at"],
            json!("2026-04-28T09:31:27.123456Z")
        );
        mock.shutdown().await;
    }

    #[tokio::test]
    async fn mcp_vault_rescan_returns_write_tools_disabled_when_gated() {
        let url = unbound_url();
        let server = server_against_with_writes(&url, false);

        let result = server
            .vault_rescan(Parameters(VaultRescanInput {
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
            message.contains("vault_rescan"),
            "message should reference the gated tool, got {message:?}"
        );
    }
}
