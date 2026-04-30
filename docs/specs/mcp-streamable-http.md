# MCP Streamable HTTP Transport Specification

**Version**: 1.1.0
**Date**: 2026-04-30
**Status**: Approved

---

## Overview

The MCP Streamable HTTP transport exposes Hypomnema's MCP tool surface (the same `search_filesystem` / `search_content` / `search_semantic` operations served by the stdio transport today, plus the vault-management tools and the live `vault_watch` subscription) over a single HTTP endpoint on `hmnd`'s existing Axum router. Agent hosts that cannot spawn subprocesses (browser-hosted hosts) or that connect to a non-co-located daemon talk to Hypomnema over this transport instead of the stdio shim served by `hmn mcp`.

The transport implements MCP's "Streamable HTTP" specification (the third standard MCP transport, post-2024-11-05): one endpoint that accepts JSON-RPC requests over POST and optionally streams server-to-client messages over SSE on GET.

This spec is deliberately small. The MCP HTTP transport is a control plane behind MCP framing, with no additional trust posture. Authentication, TLS, bind-address policy, and rate limiting are concerns of `hmnd`'s HTTP listener as a whole (today: loopback-only, no auth, no TLS — extending ADR-0005's local-everything trust boundary). The MCP HTTP transport inherits whatever posture the listener is configured with; it does not introduce its own.

**Related Documents**:
- [ADR-0013: MCP transport — Streamable HTTP on `hmnd`](../decisions/0013-mcp-transport-streamable-http.md) — present scope this spec records
- [ADR-0012: MCP transport — stdio in v0; socket deferred](../decisions/0012-mcp-transport-stdio-v0.md) — sibling MCP transport (stdio); this spec extends ADR-0012's transport landscape
- [ADR-0005: Local Everything](../decisions/0005-local-everything.md) — trust boundary the HTTP listener inherits
- [ADR-0008: Two binaries (hmnd + hmn) in one crate](../decisions/0008-two-binary-daemon-plus-cli.md) — binary-placement principle (long-lived listener → `hmnd`)
- [ADR-0004: Three search modes as peers](../decisions/0004-three-search-modes-as-peers.md) — canonical tool names and the "transports are peers, not forks" claim
- [Architecture: Search API](../architecture/overview.md#search-api) — current three-transport surface (HTTP + stdio-MCP + HTTP-MCP)
- [filesystem-search](./filesystem-search.md), [content-search](./content-search.md), [semantic-search](./semantic-search.md) — wire shapes the new transport reuses unchanged
- [vault-management](./vault-management.md) — pre-commits to "same handlers, three transports" for vault-management MCP tools; HTTP-MCP is one of those three
- [change-events](./change-events.md) — live `vault_watch` semantics and future durable-stream notes

---

## Behavior

### Normal Flow

1. `hmnd` starts and binds its HTTP listener at `config.http.bind` (default `127.0.0.1:7777`).
2. The Axum router mounts the MCP Streamable HTTP route at `/mcp`, alongside the existing `/search/*`, `/health`, and `/status` routes.
3. An MCP-capable client opens a connection to `http://<host>:<port>/mcp`.
4. The client sends a JSON-RPC `initialize` over POST. The server responds with `serverInfo.name = "hypomnema"` (per ADR-0012 § Resolution 4) and the tool list.
5. For each tool invocation, the client POSTs a JSON-RPC `tools/call`. The server executes the tool against the daemon's in-process search handlers and returns the result.
6. If the server has streamed messages to deliver (`vault_watch`, progress notifications, future server-initiated requests), the client opens an SSE connection via GET to the same endpoint. The server emits SSE-framed JSON-RPC messages until either side closes.
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

Identical to the stdio transport. Request/response tools include `search_filesystem`, `search_content`, `search_semantic`, and the vault-management request/response tools. `vault_watch` adds a read-only long-lived live-event subscription; its semantics are defined in [change-events.md](./change-events.md#mcp-subscription). Search request/response shapes are the `*QueryJson` / `*SearchResponse` types defined in the search specs.

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

**Behavior**: The server holds the connection open and emits SSE events as JSON-RPC notifications. Read-only search tools do not emit notifications. `vault_watch` uses the live-stream framing pinned by [change-events.md](./change-events.md#mcp-subscription). Future MCP features (progress notifications during long semantic searches; server-initiated tool calls) flow through this channel.

**Result**: A successful keep-alive stream that emits no events unless a long-lived operation such as `vault_watch` is active, and closes gracefully when the client or server disconnects.

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
- **In-process tool implementations**: Tool handlers in `hmnd` call the search backend directly, not through the `DaemonClient` HTTP shim that `hmn mcp` uses. The shared abstraction is a `HypomnemaBackend` trait with two implementations: in-process (used by `hmnd`'s router) and HTTP-shim (used by `hmn mcp`). The trait's exact shape was pinned at workplan-time.
- **Stdout reservation does NOT apply to HTTP-MCP**: ADR-0012's stdout invariant applies only to the stdio transport. HTTP-MCP carries JSON-RPC over HTTP and SSE; logging on `hmnd` continues to use its existing tracing setup.
- **Brand-identity macro pin**: The `#[tool_handler(name = "hypomnema")]` attribute on `HypomnemaMcpServer` continues to apply for HTTP-MCP. ADR-0012 § Negative consequences flags that this syntax is rmcp-1.5.x-specific; the rmcp upgrade workplan revisits both transports.
- **rmcp transport feature**: `Cargo.toml`'s `rmcp` features list adds `transport-streamable-http-server` to the existing `["server", "transport-io", "macros", "schemars"]` set. Pre-round-prep verified the feature exists at rmcp 1.5.0; no version bump.
- **Origin-validation positive grep**: After this spec ships, `rg 'Origin' src/api/` returns at least one match in the MCP handler module — the validator is required, not optional.
- **No new authentication or TLS code**: The spec is a strict no-op on auth and TLS. If the implementer finds themselves writing token verification, bearer parsing, or `rustls` server config in the `/mcp` handler, they have drifted from the spec — those concerns belong on `hmnd`'s HTTP listener as a whole, not on the MCP route.
- **Stories file**: see [`mcp-streamable-http-stories.md`](../../notes/proposals/archive/mcp-streamable-http-stories.md) (archived alongside the original proposal at promotion time).
- **Workplan-time resolutions**: Open Questions 1–5 below were resolved at the round-4 step-12 workplan; see [`notes/roadmap/archive/step-12-workplan.md` § Deferred-decision resolutions](../../notes/roadmap/archive/step-12-workplan.md) for the full A–E rationale.

---

## Open Questions

- [ ] **Session management** — MCP Streamable HTTP supports `Mcp-Session-Id` for stateful sessions. v1 ships stateless per Resolution B in the round-4 step-12 workplan: `LocalSessionManager::default()` satisfies rmcp's type bound without persisting cross-call state, and Hypomnema's request/response tool surface has no session-scoped state to track. Future-amendment trigger: a tool that needs cursor-in-session or multi-step transactional state.
- [ ] **Resumable SSE streams** (`Last-Event-ID`) — deferred per Resolution C in the round-4 step-12 workplan. `vault_watch` is live-only and deliberately has no replay / no `since` semantics in v0, so `Last-Event-ID` remains out of scope until a durable event store exists. Future-amendment trigger: durable replay, progress notifications during long semantic searches, or server-initiated tool calls.
- [ ] **CORS** — v1 sets no CORS headers per Resolution D in the round-4 step-12 workplan. The first real browser-hosted host consumer's requirements decide what to allow. Hand-rolled `tower-http::cors` is the natural future shape; deferred until a concrete consumer surfaces. Note: the Origin-validation middleware (DNS-rebinding defense) is a separate concern and is shipped in v1 — Origin validation rejects unauthorized cross-origin POSTs at the request-entry boundary; CORS preflight is the browser's mechanism for the client to know which cross-origin requests to issue.
- [ ] **`mcp.http.path` configurability** — v1 rejects any value other than `"/mcp"` to keep the routing story trivial per Resolution E in the round-4 step-12 workplan. Future versions may allow path customization (e.g., for operators reverse-proxying multiple Hypomnema daemons under different prefixes). Out of scope for v1.

---

## Revision History

| Version | Date | Changes |
|---|---|---|
| 0.1.0 | 2026-04-27 | Initial draft. |
| 0.1.1 | 2026-04-28 | Round-4 pre-round prep: resolved Open Question 1 (rmcp 1.5 ships `transport-streamable-http-server`; `StreamableHttpService` mounts on an axum `Router` via `nest_service`; no custom scaffolding needed). |
| 1.0.0 | 2026-04-28 | Promoted from `notes/proposals/mcp-streamable-http.md` (was 0.1.1). Round-4 step 12 workplan resolutions: Open Question 1 → resolved at pre-round prep (rmcp ships transport feature; mount via `nest_service` on axum 0.7 with hand-rolled-handler fallback); Open Question 2 → v1 stateless; Open Question 3 → defer; Open Question 4 → v1 sets no CORS headers; Open Question 5 → v1 rejects `mcp.http.path != "/mcp"` with startup error. |
| 1.1.0 | 2026-04-30 | Amended for live `vault_watch`: MCP HTTP remains stateless and non-resumable, but can carry live server-to-client event notifications for active watch subscribers. |
