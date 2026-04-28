//! HTTP-MCP transport: mounts rmcp's Streamable HTTP server at `/mcp` on the
//! daemon's existing axum listener. See ADR-0013 and
//! `docs/specs/mcp-streamable-http.md`. The MCP route is gated by an Origin-
//! validation middleware that rejects non-loopback origins per the spec's
//! browser-DNS-rebinding mitigation guidance.

use std::sync::Arc;

use axum::Router;
use axum::extract::Request;
use axum::http::{HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};

use crate::mcp::{HypomnemaBackend, HypomnemaMcpServer};

/// Construction state for the HTTP-MCP sub-router. Mirrors the fields the
/// `HypomnemaMcpServer` needs; `hmnd` populates these from its already-built
/// `InProcessBackend` plus the parsed `[mcp]` config.
#[derive(Clone)]
pub struct McpHttpState {
    pub backend: Arc<dyn HypomnemaBackend + Send + Sync>,
    pub default_vault_name: String,
    pub enable_write_tools: bool,
}

/// Build the axum sub-router that mounts the Streamable HTTP MCP service at
/// `/mcp`, gated by the Origin-validation middleware. The caller (`hmnd`)
/// merges this router into the main API router only when
/// `config.mcp.http.enabled = true`; when disabled, `/mcp` is unrouted and
/// axum returns 404 by default.
///
/// Mount shape: `Router::nest_service("/mcp", StreamableHttpService::new(...))`
/// per ADR-0013 § Resolution A. Verified against rmcp 1.5's own integration
/// tests (`tests/test_streamable_http_json_response.rs`) which use the same
/// shape on axum 0.7.
pub fn router(state: McpHttpState) -> Router {
    let server = HypomnemaMcpServer {
        backend: state.backend,
        default_vault_name: state.default_vault_name,
        enable_write_tools: state.enable_write_tools,
    };
    let service: StreamableHttpService<HypomnemaMcpServer, LocalSessionManager> =
        StreamableHttpService::new(
            move || Ok(server.clone()),
            Arc::new(LocalSessionManager::default()),
            StreamableHttpServerConfig::default(),
        );
    Router::new()
        .nest_service("/mcp", service)
        .layer(middleware::from_fn(origin_validation))
}

/// Reject requests whose `Origin` header names a non-loopback host. Absent
/// `Origin` is accepted (curl, MCP CLI clients, server-to-server callers).
/// `Origin: null` is accepted per the MCP spec's allowance for sandboxed
/// browser frames and `file://` documents.
async fn origin_validation(req: Request, next: Next) -> Response {
    let Some(origin) = req.headers().get(axum::http::header::ORIGIN) else {
        return next.run(req).await;
    };
    let origin_str = match origin.to_str() {
        Ok(s) => s,
        Err(_) => return reject_origin(origin),
    };
    if origin_is_allowed(origin_str) {
        return next.run(req).await;
    }
    (
        StatusCode::FORBIDDEN,
        format!("Origin not allowed: {origin_str}"),
    )
        .into_response()
}

fn reject_origin(value: &HeaderValue) -> Response {
    let display = value.to_str().unwrap_or("<non-utf8>");
    (
        StatusCode::FORBIDDEN,
        format!("Origin not allowed: {display}"),
    )
        .into_response()
}

fn origin_is_allowed(origin: &str) -> bool {
    if origin == "null" {
        return true;
    }
    let uri: axum::http::Uri = match origin.parse() {
        Ok(u) => u,
        Err(_) => return false,
    };
    if uri.scheme_str() != Some("http") {
        return false;
    }
    let Some(host) = uri.host() else {
        return false;
    };
    let normalized = host
        .trim_matches('[')
        .trim_matches(']')
        .to_ascii_lowercase();
    matches!(normalized.as_str(), "localhost" | "127.0.0.1" | "::1")
}

/// Test-only helper: build an axum router that exposes a stub `/mcp` route
/// behind the same Origin-validation middleware used in production. Used by
/// the unit tests below to exercise the middleware in isolation without
/// constructing a real `StreamableHttpService` or `InProcessBackend`.
#[cfg(test)]
fn test_router_with_origin_middleware() -> Router {
    use axum::routing::any;
    Router::new()
        .route("/mcp", any(|| async { StatusCode::OK }))
        .layer(middleware::from_fn(origin_validation))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{Body, to_bytes};
    use axum::http::{Method, Request as HttpRequest};
    use tower::ServiceExt;

    async fn post_with_origin(origin: Option<&str>) -> (StatusCode, String) {
        let app = test_router_with_origin_middleware();
        let mut builder = HttpRequest::builder().method(Method::POST).uri("/mcp");
        if let Some(o) = origin {
            builder = builder.header("Origin", o);
        }
        let req = builder.body(Body::empty()).unwrap();
        let res = app.oneshot(req).await.unwrap();
        let status = res.status();
        let body = to_bytes(res.into_body(), 1024).await.unwrap();
        (status, String::from_utf8_lossy(&body).to_string())
    }

    #[tokio::test]
    async fn mcp_http_origin_loopback_v4_accepted() {
        let (status, _) = post_with_origin(Some("http://127.0.0.1:7777")).await;
        assert_ne!(status, StatusCode::FORBIDDEN);
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn mcp_http_origin_loopback_v6_accepted() {
        let (status, _) = post_with_origin(Some("http://[::1]:7777")).await;
        assert_ne!(status, StatusCode::FORBIDDEN);
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn mcp_http_origin_localhost_accepted() {
        let (status, _) = post_with_origin(Some("http://localhost")).await;
        assert_ne!(status, StatusCode::FORBIDDEN);
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn mcp_http_origin_localhost_with_port_accepted() {
        let (status, _) = post_with_origin(Some("http://localhost:7777")).await;
        assert_ne!(status, StatusCode::FORBIDDEN);
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn mcp_http_origin_null_accepted() {
        let (status, _) = post_with_origin(Some("null")).await;
        assert_ne!(status, StatusCode::FORBIDDEN);
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn mcp_http_origin_missing_accepted() {
        let (status, _) = post_with_origin(None).await;
        assert_ne!(status, StatusCode::FORBIDDEN);
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn mcp_http_origin_remote_rejected() {
        let (status, body) = post_with_origin(Some("http://example.com")).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert!(
            body.contains("Origin not allowed: http://example.com"),
            "expected exact rejection body, got {body:?}"
        );
    }

    #[tokio::test]
    async fn mcp_http_origin_https_loopback_rejected() {
        // Per the spec, only `http://` loopback origins are allowed (browser
        // contexts that hit a local server use http). https against loopback
        // is unusual and not in the allow-list.
        let (status, body) = post_with_origin(Some("https://127.0.0.1:7777")).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert!(body.contains("Origin not allowed: https://127.0.0.1:7777"));
    }

    #[tokio::test]
    async fn mcp_http_disabled_returns_404() {
        // When `mcp.http.enabled = false`, hmnd does not merge the
        // mcp_http::router into the main app router; `/mcp` is therefore
        // unrouted and axum's default fallback returns 404. This test models
        // that absence-of-mount with a router that only has the API surface.
        use axum::routing::post;
        let app = Router::new().route(
            "/search/filesystem",
            post(|| async { axum::Json(serde_json::json!({"results": []})) }),
        );

        let res = app
            .clone()
            .oneshot(
                HttpRequest::builder()
                    .method(Method::POST)
                    .uri("/mcp")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);

        let res = app
            .oneshot(
                HttpRequest::builder()
                    .method(Method::POST)
                    .uri("/search/filesystem")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }
}
