# MCP Streamable HTTP Transport Specification

**Version**: 0.1.0
**Date**: 2026-04-27
**Status**: Draft

---

## Overview

The MCP Streamable HTTP transport exposes Hypomnema's MCP tool surface (the same `search_filesystem` / `search_content` / `search_semantic` operations served by the stdio transport today) over a single HTTP endpoint on `hmnd`'s existing Axum router. Agent hosts that cannot spawn subprocesses (browser-hosted hosts) or that connect to a non-co-located daemon talk to Hypomnema over this transport instead of the stdio shim served by `hmn mcp`.

The transport implements MCP's "Streamable HTTP" specification (the third standard MCP transport, post-2024-11-05): one endpoint that accepts JSON-RPC requests over POST and optionally streams server-to-client messages over SSE on GET.

This spec is deliberately small. The MCP HTTP transport is a control plane that offers strictly less than the existing `/search/*` HTTP API: same operations, behind an MCP framing, with no additional behavior, no additional tool surface, and no additional trust posture. Authentication, TLS, bind-address policy, and rate limiting are concerns of `hmnd`'s HTTP listener as a whole (today: loopback-only, no auth, no TLS — extending ADR-0005's local-everything trust boundary). The MCP HTTP transport inherits whatever posture the listener is configured with; it does not introduce its own.

**Related Documents**:
- [ADR-0012: MCP transport — stdio in v0; socket deferred](../../docs/decisions/0012-mcp-transport-stdio-v0.md) — present scope this spec extends
- [ADR-0005: Local Everything](../../docs/decisions/0005-local-everything.md) — trust boundary the HTTP listener inherits
- [ADR-0008: Two binaries (hmnd + hmn) in one crate](../../docs/decisions/0008-two-binary-daemon-plus-cli.md) — binary-placement principle (long-lived listener → `hmnd`)
- [ADR-0004: Three search modes as peers](../../docs/decisions/0004-three-search-modes-as-peers.md) — canonical tool names and the "transports are peers, not forks" claim
- [Architecture: Search API](../../docs/architecture/overview.md#search-api) — current two-transport surface (HTTP + stdio-MCP) extended here to three
- [filesystem-search](../../docs/specs/filesystem-search.md), [content-search](../../docs/specs/content-search.md), [semantic-search](../../docs/specs/semantic-search.md) — wire shapes the new transport reuses unchanged
- [vault-management](../../docs/specs/vault-management.md) — pre-commits to "same handlers, three transports" for vault-management MCP tools (round 3+); HTTP-MCP becomes one of those three

> Cross-reference paths above use `../../docs/...` because this draft lives in `notes/proposals/`. On promotion to `docs/specs/mcp-streamable-http.md`, rewrite to `../decisions/...`, `../architecture/...`, `./filesystem-search.md`, etc., per `_template.md` conventions.

---

## Behavior

### Normal Flow

1. `hmnd` starts and binds its HTTP listener at `config.http.bind` (default `127.0.0.1:7777`).
2. The Axum router mounts the MCP Streamable HTTP route at `/mcp`, alongside the existing `/search/*`, `/health`, and `/status` routes.
3. An MCP-capable client opens a connection to `http://<host>:<port>/mcp`.
4. The client sends a JSON-RPC `initialize` over POST. The server responds with `serverInfo.name = "hypomnema"` (per ADR-0012 § Resolution 4) and the tool list.
5. For each tool invocation, the client POSTs a JSON-RPC `tools/call`. The server executes the tool against the daemon's in-process search handlers and returns the result.
6. If the server has streamed messages to deliver (progress notifications, future server-initiated requests), the client opens an SSE connection via GET to the same endpoint. The server emits SSE-framed JSON-RPC messages until either side closes.
7. When the client disconnects (or the daemon shuts down), session state is discarded.

### State Machine

**State Machine**: N/A — the transport is stateless; tool invocations are independent and idempotent. (See Open Questions for the session-id question.)

---

## Data Schema

The MCP Streamable HTTP transport carries the same JSON-RPC envelope MCP defines for any transport. The wire schema is the MCP protocol's responsibility; this spec defines only what is Hypomnema-specific.

### MCP serverInfo

```yaml
serverInfo:
  name: "hypomnema"
  version: "<CARGO_PKG_VERSION>"
```

| Field | Type | Required | Default | Description |
|---|---|---|---|---|
| `name` | string | yes | (override) | Always `"hypomnema"`. Override of rmcp's auto-derived `Implementation::from_build_env()` per ADR-0012 § Resolution 4. |
| `version` | string | yes | `env!("CARGO_PKG_VERSION")` | Hypomnema crate version, supplied by rmcp 1.5's macro behavior when `name` is provided without an explicit `version`. |

### Tool surface

Identical to the stdio transport. Tools: `search_filesystem`, `search_content`, `search_semantic`. Request/response shapes are the `*QueryJson` / `*SearchResponse` types defined in the search specs. When `vault-management.md` ships its MCP tools (round 3+), they extend the same surface served by this transport.

### Endpoint configuration

```yaml
mcp:
  http:
    enabled: true
    path: "/mcp"
```

| Field | Type | Required | Default | Description |
|---|---|---|---|---|
| `mcp.http.enabled` | bool | no | `true` | Whether to mount `/mcp` on the daemon's HTTP listener. Default-on because it shares the listener's existing trust posture; setting to `false` disables HTTP-MCP without disabling stdio MCP. |
| `mcp.http.path` | string | no | `"/mcp"` | Route prefix. Reserved-for-forward-compat: rejected with a clear startup error if set to anything other than `"/mcp"` in v1. |

### Validation Rules

- `mcp.http.path` must equal `"/mcp"` in v1; any other value produces a startup error.
- The transport does not bind its own address — it uses the address the HTTP listener is configured with.
- The transport does not enforce auth or TLS — it inherits whatever the HTTP listener does. Today the listener is loopback-only with no auth and no TLS; if `hmnd` later gains those, MCP-over-HTTP rides on them with no spec change.

---

## Examples

### Example 1: agent host issues `search_filesystem` over HTTP-MCP

**Input** (HTTP POST `http://127.0.0.1:7777/mcp`):
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "search_filesystem",
    "arguments": {"vault_id": "default", "prefix": "notes/"}
  }
}
```

**Behavior**: `hmnd` routes the request to the in-process MCP server. The MCP server's `search_filesystem` tool handler invokes the same backend code that backs `POST /search/filesystem`. Result is wrapped in an MCP `CallToolResult`.

**Result**:
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "content": [{"type": "text", "text": "{\"files\":[…],\"truncated\":false}"}],
    "isError": false,
    "structuredContent": {"files": [...], "truncated": false}
  }
}
```

The `structuredContent` payload is the same `FilesystemSearchResponse` JSON the HTTP `/search/filesystem` endpoint would return for an equivalent request.

### Example 2: server-streamed messages over GET

**Input**: After a successful initialize, the client opens GET `http://127.0.0.1:7777/mcp` with `Accept: text/event-stream`.

**Behavior**: The server holds the connection open and emits SSE events as JSON-RPC notifications. In v1 the server has no notifications to send for the read-only search tools — the SSE stream opens, stays open, closes cleanly with no events. Future MCP features (progress notifications during long semantic searches; server-initiated tool calls) flow through this channel.

**Result**: A successful keep-alive stream that emits no events and closes gracefully when the client or server disconnects.

---

## Edge Cases

### Client disconnects mid-tool-call

**Scenario**: Client POSTs a `tools/call`, then closes the TCP connection before the server returns the response (e.g., agent host process crashes).

**Behavior**: The tool handler may continue executing to completion (search operations are read-only and short-lived; no rollback needed). Response is dropped at the HTTP layer. No session state is leaked because the transport is stateless.

**Rationale**: Search tools have no side effects. Spending bookkeeping to abort an in-flight search saves nothing meaningful; matches the existing `/search/*` behavior.

### Multiple concurrent clients

**Scenario**: Two agent hosts connect to `/mcp` and issue tool calls in parallel.

**Behavior**: Each request handled independently. Tool implementations are reentrant (they read from the SQLite store via the daemon's connection pool, which is concurrency-safe per the `rusqlite-in-async` skill).

**Rationale**: Stateless transport; concurrency is bounded by the daemon's existing connection-pool limits, which apply identically to `/search/*` requests.

### Origin header validation (DNS rebinding defense)

**Scenario**: A request arrives with an `Origin` header set to an unexpected value — the standard DNS-rebinding attack vector against a loopback-bound daemon.

**Behavior**: If `Origin` is set, it must be `null`, or a loopback origin (`http://localhost[:port]`, `http://127.0.0.1[:port]`, `http://[::1][:port]`). Mismatches return HTTP 403. Requests with no `Origin` header are accepted (curl, non-browser clients).

**Rationale**: DNS rebinding lets a malicious page in the user's browser issue requests to `127.0.0.1:7777`. `Origin` validation is the standard defense and costs nothing. This is an MCP-spec recommendation, not a Hypomnema invention.

### Daemon shutdown with open SSE stream

**Scenario**: An SSE stream is open when `hmnd` receives SIGTERM.

**Behavior**: The SSE stream closes cleanly via the existing `with_graceful_shutdown` integration on `hmnd`'s Axum server (`src/bin/hmnd.rs:165`).

**Rationale**: Reuses the daemon's existing shutdown machinery. No new shutdown path needed.

---

## Error Handling

The MCP error envelopes already established by ADR-0012 (`daemon_unreachable`, `invalid_glob`, `invalid_regex`, `invalid_prefix`, `invalid_request`, `embedding_unavailable`, `internal`) apply unchanged. The HTTP transport adds two transport-layer error conditions:

| Error Condition | Error Code/Type | Message | Recovery |
|---|---|---|---|
| Origin header rejected | HTTP 403 | `Origin not allowed: <value>` | Client must use a permitted Origin (`null`, loopback). For non-loopback bind targets, listener-level config governs. |
| `mcp.http.path` invalid at startup | startup error (no HTTP response) | `mcp.http.path must be "/mcp" in this version of Hypomnema` | Operator removes or corrects `mcp.http.path`. |

Note: `daemon_unreachable` (synthesized in `hmn mcp`'s HTTP shim per ADR-0012) does **not** apply on the HTTP transport — there is no shim, the tools execute in-process. If `hmnd` is not running, the client gets a connection error from its own HTTP stack, not an MCP error envelope.

---

## Integration Points

### With `hmnd`'s HTTP listener

The MCP route mounts onto the same Axum router that serves `/health`, `/status`, and `/search/*`. Bind address, TLS, auth, and graceful shutdown are inherited from the existing listener — no parallel infrastructure.

**Data Flow**:
```
agent host
   │
   ▼ POST/GET /mcp
hmnd Axum router  ───►  /mcp route
                            │
                            ▼
                       rmcp service
                            │
                            ▼
                       HypomnemaMcpServer
                            │
                            ▼ (in-process; no DaemonClient HTTP shim)
                       search backend
                            │
                            ▼
                       SQLite + sqlite-vec
```

Contrast with `hmn mcp` (ADR-0012 § Resolution 1): there the `HypomnemaMcpServer` lives in the `hmn` process and forwards each tool call to `hmnd` over loopback HTTP via `DaemonClient`. The HTTP-MCP transport eliminates that hop.

### With the existing stdio transport (`hmn mcp`)

The two transports run independently and serve the same tool surface. An operator can use either or both at the same time. There is no shared session state between them.

### With the deferred Unix-socket transport (ADR-0012 § Decision 2)

If/when socket transport ships, it lives in `hmnd` (per ADR-0012). HTTP-MCP and socket-MCP would share the same `HypomnemaMcpServer` factory in `hmnd` but bind to different transports.

### With the rmcp crate

This spec assumes rmcp 1.5 ships a server-side Streamable HTTP transport feature that integrates with axum. Implementation must verify and either enable that feature or scope scaffolding over rmcp's core types. (See Open Questions and Implementation Notes.)

---

## Implementation Notes

- **Binary placement**: HTTP-MCP lives in `hmnd`, not `hmn` — long-lived listener attached to the daemon's HTTP server, parallel to the deferred Unix-socket transport. Continues the principle from ADR-0008 § Amendments / ADR-0012: each transport's binary matches its lifetime.
- **In-process tool implementations**: Tool handlers in `hmnd` call the search backend directly, not through the `DaemonClient` HTTP shim that `hmn mcp` uses. The shared abstraction is a `SearchBackend` trait (or equivalent) with two implementations: in-process (used by `hmnd`'s router) and HTTP-shim (used by `hmn mcp`). Workplan-time decision on the trait shape.
- **Stdout reservation does NOT apply to HTTP-MCP**: ADR-0012's stdout invariant applies only to the stdio transport. HTTP-MCP carries JSON-RPC over HTTP and SSE; logging on `hmnd` continues to use its existing tracing setup.
- **Brand-identity macro pin**: The `#[tool_handler(name = "hypomnema")]` attribute on `HypomnemaMcpServer` continues to apply for HTTP-MCP. ADR-0012 § Negative consequences flags that this syntax is rmcp-1.5.x-specific; the rmcp upgrade workplan revisits both transports.
- **rmcp transport feature verification**: `Cargo.toml:31` currently enables `["server", "transport-io", "macros", "schemars"]`. HTTP-MCP requires adding the rmcp Streamable HTTP server feature (verify exact name against rmcp 1.5 docs at workplan time). If rmcp 1.5 does not ship one, the workplan must scope custom transport scaffolding (axum handlers + rmcp service trait), and that scope expansion gets re-validated before commitment.
- **Origin-validation positive grep**: After this spec ships, `rg 'Origin' src/api/` returns at least one match in the MCP handler module — the validator is required, not optional.
- **No new authentication or TLS code**: The spec is a strict no-op on auth and TLS. If the implementer finds themselves writing token verification, bearer parsing, or `rustls` server config in the `/mcp` handler, they have drifted from the spec — those concerns belong on `hmnd`'s HTTP listener as a whole, not on the MCP route.
- **Stories file**: see [`mcp-streamable-http-stories.md`](./mcp-streamable-http-stories.md) (peer artifact in the same directory).

---

## Open Questions

- [x] **rmcp 1.5 Streamable HTTP server transport availability** — **Resolved 2026-04-28 (round-4 pre-round prep)**: `rmcp = "1.5.0"` ships the `transport-streamable-http-server` feature, which exposes `StreamableHttpService` (a tower service) and `StreamableHttpServerConfig` under `rmcp::transport::streamable_http_server`. Session management is via the `SessionManager` trait with an in-memory `session::local::LocalSessionManager` default. Mounting on an axum router is one line: `Router::new().nest_service("/mcp", StreamableHttpService::new(factory, manager.into(), config))` — see the canonical [`counter_streamhttp.rs`](https://github.com/modelcontextprotocol/rust-sdk/blob/main/examples/servers/src/counter_streamhttp.rs) example. No custom scaffolding needed; scope is unchanged. **Caveat**: rmcp's example uses axum 0.8; Hypomnema is on axum 0.7. `StreamableHttpService` is generic over standard `http`/`http-body`/`tower` types (not axum-version-specific), and `Router::nest_service` exists in both 0.7 and 0.8 — axum 0.7 should work without an upgrade, but verify with a `cargo build` at workplan-time before committing. Add `transport-streamable-http-server` to the existing `rmcp` features list in `Cargo.toml`; no new top-level crates required (`tower-http` `cors` is optional and only relevant if/when CORS lands per OQ #4 below).
- [ ] **Session management** — MCP Streamable HTTP supports `Mcp-Session-Id` for stateful sessions. v1 deliberately ships stateless. If a future round-3 vault-management MCP tool needs sessions, this spec amends to add them — fits the TBD rule (workplan-time resolution, 1-3 paragraphs).
- [ ] **Resumable SSE streams** (`Last-Event-ID`) — not needed for v1 read-only search tools (each call is single request-response). Revisits when long-running tools or server-pushed notifications land.
- [ ] **CORS** — v1 sets no CORS headers. Browser-hosted hosts may need them; the first real consumer's requirements decide what to allow. Workplan-time decision when a concrete browser host is in scope.
- [ ] **`mcp.http.path` configurability** — v1 rejects any value other than `"/mcp"` to keep the routing story trivial. Future versions may allow path customization (e.g., for operators reverse-proxying multiple Hypomnema daemons under different prefixes). Out of scope for v1.

---

## Revision History

| Version | Date | Changes |
|---|---|---|
| 0.1.0 | 2026-04-27 | Initial draft. |
| 0.1.1 | 2026-04-28 | Round-4 pre-round prep: resolved Open Question 1 (rmcp 1.5 ships `transport-streamable-http-server`; `StreamableHttpService` mounts on an axum `Router` via `nest_service`; no custom scaffolding needed). |
