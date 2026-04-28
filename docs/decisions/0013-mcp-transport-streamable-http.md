# ADR-0013: MCP transport — Streamable HTTP on `hmnd`

**Status**: accepted
**Date**: 2026-04-28
**Decision-Makers**: Beau Simensen

---

## Context

[ADR-0012](./0012-mcp-transport-stdio-v0.md) shipped MCP-over-stdio in round 2 (the `hmn mcp` subcommand) and explicitly deferred the Unix-socket transport to a follow-on workplan, naming three transports as the v0 horizon: stdio (shipped), Unix socket (deferred), and "any future MCP transport" (open). Round 3 added nine vault-management MCP tools on top of the round-2 search tools, bringing the surface to twelve tools total served via stdio.

Round 4 adds the **third standard MCP transport**: Streamable HTTP (the post-2024-11-05 MCP-spec transport that pairs JSON-RPC over POST with optional SSE on GET). The drivers:

- **Browser-hosted MCP hosts** can't spawn subprocesses. Iris and any future browser-resident agent host need a network endpoint, not a stdio shim.
- **Remote MCP scenarios** (the agent host and the daemon are not co-located) need a network endpoint with origin defenses against DNS rebinding from a malicious page.
- **The work is small.** rmcp 1.5.0 ships a `transport-streamable-http-server` feature exposing `StreamableHttpService` (a tower service); it mounts on the existing Axum router with one new route. No new top-level crate; no parallel listener.

This ADR records the round-4 decision and amends [ADR-0012](./0012-mcp-transport-stdio-v0.md) to enumerate the three now-shipped-or-deferred MCP transports.

## Decision

**HTTP-MCP ships in round 4 as the third standard MCP transport, mounted on `hmnd`'s existing Axum router at `/mcp`.**

1. **Transport mount on `hmnd`.** The MCP-over-HTTP surface is a route on `hmnd`'s existing Axum router, alongside `/health`, `/status`, `/search/*`, and the round-3 `/vaults/*` control plane. Mounted via `rmcp::transport::streamable_http_server::StreamableHttpService` (a tower service) using `Router::nest_service("/mcp", service)` on axum 0.7. The route serves JSON-RPC over POST and SSE over GET per the MCP spec. No parallel listener; no new bind address; the existing `with_graceful_shutdown` integration covers the new route by virtue of being on the same `Router`. The single new Cargo.toml change is adding `transport-streamable-http-server` to the existing `rmcp` features list. (No new top-level crates.)

2. **In-process tool execution; no DaemonClient HTTP shim.** Tool handlers in `hmnd` call the search and vault-management backends directly via a new `HypomnemaBackend` trait. The trait has two implementations: `DaemonClient` (HTTP shim, used by `hmn mcp` per ADR-0012) and `InProcessBackend` (direct VaultManager + search-handler calls, used by `hmnd` for HTTP-MCP). `HypomnemaMcpServer` holds `Arc<dyn HypomnemaBackend + Send + Sync>` instead of a concrete `DaemonClient`. The trait covers all twelve current MCP tool ops (3 search + 2 vault-read + 7 vault-write) so the in-process path doesn't need a parallel call surface for vault management.

3. **Trust posture inheritance — no new auth, no new TLS, no new bind address.** The HTTP-MCP route inherits the daemon's HTTP listener trust posture: loopback-only by default (`config.http.bind = "127.0.0.1:7777"`), no auth, no TLS — extending [ADR-0005](./0005-local-everything.md)'s local-everything boundary unchanged. Authentication, TLS, and rate limiting are concerns of the listener as a whole, not of the MCP route. If `hmnd` later gains those, MCP-over-HTTP rides on them with no spec amendment. The MCP route does not open its own client, listener, or socket.

4. **Origin-header validation as DNS-rebinding defense.** A loopback-bound daemon is reachable from any process on the host, including a malicious browser page using DNS rebinding. The MCP spec recommends validating the `Origin` header on POST; this ADR adopts that defense as a hand-rolled axum middleware on the `/mcp` route. Allowed origins: `null`, `http://localhost[:port]`, `http://127.0.0.1[:port]`, `http://[::1][:port]`. Mismatches return HTTP 403 with body `Origin not allowed: <value>`. Requests with no `Origin` header are accepted (curl, non-browser clients). Origin validation is **not** CORS — it rejects unauthorized cross-origin POSTs at the request boundary; CORS preflight is a separate concern that v1 does not implement.

5. **v1 ships stateless; resumable SSE deferred; no CORS; `mcp.http.path` reserved.** Four open questions in the proposal resolved at the round-4 workplan:
   - **Sessions**: v1 uses rmcp's `LocalSessionManager::default()` to satisfy the `StreamableHttpService` type bound but treats each `tools/call` as independent — Hypomnema's tool surface has no session-scoped state to track between calls. Future-amendment trigger: a tool that needs cursor-in-session or multi-step transactional state.
   - **Resumable SSE** (`Last-Event-ID`): deferred; v1 has no notifications or server-pushed messages to send for the read-only search tools or the idempotent vault-management tools.
   - **CORS**: v1 sets no CORS headers; the first real browser-hosted consumer's requirements decide what to allow.
   - **`mcp.http.path`**: v1 rejects any value other than `"/mcp"` with the startup error `mcp.http.path must be "/mcp" in this version of Hypomnema`. Reserved-for-forward-compat for operators who reverse-proxy multiple Hypomnema daemons.

6. **Brand identity unchanged.** The `#[tool_handler(name = "hypomnema")]` attribute on `HypomnemaMcpServer` (per ADR-0012 § Resolution 4) applies unchanged to HTTP-MCP — both transports return `serverInfo.name = "hypomnema"` and the same auto-derived crate version.

This ADR amends [ADR-0012](./0012-mcp-transport-stdio-v0.md) (a one-row entry in 0012 § Amendments pointing at this ADR + a forward-edit in 0012 § Decision listing HTTP as the third transport) and [ADR-0008](./0008-two-binary-daemon-plus-cli.md) (one-row § Amendments entry noting HTTP-MCP joins the deferred socket transport as `hmnd`-resident MCP transports). It supersedes nothing.

## Consequences

### Positive

- **Browser-hosted hosts and remote-MCP scenarios become reachable.** The original ADR-0012 stdio-only landscape required every agent host to spawn `hmn mcp`; HTTP-MCP unblocks the consumer shapes that can't.
- **Trust model unchanged; no parallel listener.** HTTP-MCP rides on `hmnd`'s existing Axum router and graceful-shutdown integration. Operators don't get a new bind address to worry about, a new auth surface to harden, or a new shutdown path to verify.
- **The `HypomnemaBackend` trait extraction earns its keep across both transports.** Round-3 added nine vault-management methods to `DaemonClient`; the trait extraction pulls those into a single shared interface so the in-process path doesn't double the implementation surface. Future MCP transports (Unix socket, post-v1) plug into the same trait.
- **One route, no scope expansion.** The implementation is structurally smaller than v1 socket-transport would have been (no accept-loop, no per-connection task spawning, no mode-0600 permission handling, no daemon-shutdown rewiring). Round-4 ships the third transport without enlarging the daemon's runtime architecture.
- **Origin-validation positive grep.** After this ADR ships, `rg 'Origin' src/api/` returns at least one match in the MCP handler module — the validator is required, not optional, and easy to verify against rebase drift.
- **Brand identity carries over.** Users see "hypomnema" in their MCP host's tool listing on HTTP-MCP just as on stdio MCP; the round-2 brand-identity decision applies unchanged.

### Negative

- **Three transports means three sites to revisit on rmcp major upgrades.** Round-2 already flagged the brand-identity macro pin as rmcp-1.5.x-specific; HTTP-MCP adds a second integration surface (the `transport-streamable-http-server` feature and the `StreamableHttpService` mount shape) to revalidate at any future rmcp bump.
- **Origin validation is now a load-bearing security boundary.** A regression that disables Origin validation (or weakens its allow-list) opens the daemon to DNS-rebinding from a malicious page. Round-4 tests grep for negative fingerprints (`allow_any_origin`, `cors_allow_all`, `Access-Control-Allow-Origin: *`) in the MCP handler module to catch drift.
- **HTTP-MCP introduces an implicit dependence on axum's `nest_service` composing cleanly with rmcp's tower service.** Pre-round prep verified rmcp 1.5.0 ships the feature against axum 0.8; Hypomnema is on axum 0.7. The round-4 wiring task (Task 12.4) carries a hand-rolled-handler fallback if `nest_service` fails to compose. Future axum upgrades verify against both transports.

### Neutral

- **HTTP-MCP is structurally identical to `/search/*` in trust posture and binding.** Same listener, same loopback default, same no-auth posture. The trade-offs that earned `/search/*` apply unchanged; no new ones introduced.
- **No new authentication, TLS, or rate-limiting code.** The spec is a strict no-op on these — they belong on `hmnd`'s HTTP listener as a whole, not on any individual route. If `/search/*` later gains auth, `/mcp` rides on the same machinery automatically.
- **Stdout reservation does not apply.** The round-2 stdout invariant (no byte on stdout outside rmcp framing) is stdio-MCP-only; HTTP-MCP carries JSON-RPC over HTTP and SSE, with logging on `hmnd`'s existing tracing setup.
- **`tower-http` direct-dep promotion is deferred.** v1 has no CORS, no rate limiting, no auth, no request-id propagation requirements that would benefit from `tower-http`'s middleware shape. Origin validation is a hand-rolled axum middleware. When CORS lands (round-5+), `tower-http`'s `cors` module is the natural answer; promotion happens then.

---

## Notes

- Amends [ADR-0012: MCP transport — stdio in v0; socket deferred](./0012-mcp-transport-stdio-v0.md) — see its § Amendments entry. ADR-0012's Decision is updated to enumerate three transports going forward (stdio shipped, HTTP-MCP shipped, Unix socket deferred); the v0-shipping decision recorded in that ADR is unchanged. This ADR adds the third transport without superseding ADR-0012's present scope.
- Amends [ADR-0008: Two Binaries (hmnd + hmn) in One Crate](./0008-two-binary-daemon-plus-cli.md) — see its § Amendments entry. HTTP-MCP joins the deferred Unix-socket transport as the second `hmnd`-resident MCP transport. The "each transport's binary matches its lifetime" principle from ADR-0012's amendment applies unchanged: stdio-MCP → `hmn` (short-lived adapter); HTTP-MCP → `hmnd` (long-lived listener); socket-MCP (deferred) → `hmnd` (long-lived listener).
- Extends [ADR-0004: Three Search Modes as Peers](./0004-three-search-modes-as-peers.md) — the canonical names (`search_filesystem` / `search_content` / `search_semantic`) and the canonical "transports are peers, not forks" claim continue to hold. The `HypomnemaBackend` trait extraction makes the "peers" claim load-bearing at compile time across all three transports: HTTP, stdio MCP, and HTTP MCP serve the same operations through the same handlers.
- Extends [ADR-0005: Local Everything](./0005-local-everything.md) — the local-everything trust boundary is what justifies HTTP-MCP's no-auth / no-TLS / loopback-only default. The HTTP-MCP transport inherits whatever posture the listener is configured with; it does not introduce its own.
- Related to [ADR-0011: Vault Management Lives on `hmn`](./0011-vault-management-on-hmn.md) — the round-3 vault-management MCP tools (`vault_*`) are served over HTTP-MCP via the same `HypomnemaBackend` trait that backs stdio MCP. The `[mcp] enable_write_tools` flag governs both transports identically.
- Specification: [`docs/specs/mcp-streamable-http.md`](../specs/mcp-streamable-http.md) records the wire shape, Origin allow-list, configuration knobs, examples, and edge cases.
- Workplan: round-4 step 12 ([`notes/roadmap/step-12-workplan.md`](../../notes/roadmap/step-12-workplan.md), archived at boundary) records the deferred-decision resolutions A–E (rmcp transport availability + axum mount; sessions; resumable SSE; CORS; `mcp.http.path`) and the `HypomnemaBackend` trait shape (Resolution G).

## Amendments

<!-- None yet -->
