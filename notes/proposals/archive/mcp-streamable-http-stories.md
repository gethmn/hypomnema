# MCP Streamable HTTP Transport — User Stories

This stories file accompanies [`mcp-streamable-http.md`](./mcp-streamable-http.md). The spec defines behavior; this file defines delivery scope. Each story is independently testable, leaves the HOW to the workplan, and its acceptance criteria are observable from outside the daemon.

---

## Story 1: Agent host invokes a search tool over HTTP-MCP

**Story**: As the user, I want my agent host (Iris, a browser-hosted MCP client, or any MCP-compliant host that prefers HTTP transport) to invoke Hypomnema's search tools over HTTP-MCP so that hosts that cannot or do not spawn `hmn mcp` can use Hypomnema.

**Acceptance Criteria**:

- [ ] When `hmnd` is running and `mcp.http.enabled` is `true` (the default), POST `http://127.0.0.1:7777/mcp` with a valid JSON-RPC `initialize` returns HTTP 200 and a body containing `serverInfo.name == "hypomnema"` and `serverInfo.version == <Hypomnema crate version>`.
- [ ] After initialize, POST `tools/list` returns a list containing `search_filesystem`, `search_content`, `search_semantic` (names match the canonical names from ADR-0004).
- [ ] POST `tools/call` for `search_filesystem` against a known fixture vault returns the same `FilesystemSearchResponse` JSON (in `structuredContent`) that POST `/search/filesystem` returns for an equivalent request. The test computes the expected response by issuing the equivalent `/search/filesystem` request from the same test process — not by hand-encoding the expected payload — so the criterion would not pass if the MCP path returned a constant or a divergent fixture.
- [ ] The same equivalence holds for `search_content` and `search_semantic`.
- [ ] Setting `mcp.http.enabled = false` produces a daemon that responds 404 to `/mcp` while continuing to serve `/search/*`. (`/search/*` and `/mcp` are independently mountable.)

---

## Story 2: HTTP-MCP rejects DNS-rebinding attempts via Origin validation

**Story**: As the user, I want the HTTP-MCP endpoint to reject requests with an unexpected Origin so that a malicious page loaded in my browser cannot reach the loopback-bound daemon via DNS rebinding.

**Acceptance Criteria**:

- [ ] POST `http://127.0.0.1:7777/mcp` with `Origin: http://example.com` returns HTTP 403 with body `Origin not allowed: http://example.com`. Verified by inspecting the response and asserting that no tool-call audit log entry is emitted.
- [ ] POST with `Origin: http://localhost:1234`, `Origin: http://127.0.0.1`, `Origin: http://[::1]:7777`, or `Origin: null` is accepted (returns the normal MCP response, not 403).
- [ ] POST with no `Origin` header is accepted (curl/non-browser clients).
- [ ] Negative-fingerprint grep: `rg 'allow_any_origin|cors_allow_all|Access-Control-Allow-Origin: \*' src/` returns zero matches in the MCP handler module after this story ships.

---

## Story 3: HTTP-MCP inherits the daemon's HTTP listener trust posture without introducing its own

**Story**: As the user, I want the MCP HTTP transport to ride on whatever auth/TLS posture the daemon's HTTP listener has, so that I don't have a parallel control surface to harden separately.

**Acceptance Criteria**:

- [ ] With `hmnd` bound to `127.0.0.1:7777` (the default), `/mcp` is reachable with no auth headers and over plain HTTP (matching today's `/search/*` behavior).
- [ ] When `mcp.http.enabled` is `true`, the daemon's effective HTTP listener configuration (bind address, eventual TLS settings, eventual auth settings) applies identically to `/mcp`. Verified today by: a request with a deliberately-malformed Authorization header reaches `/mcp` with the same HTTP status the same request to `/search/filesystem` produces. (Today: 200 / processed normally, because `/search/*` ignores Authorization. If `/search/*` is later required to validate Authorization, `/mcp` automatically gets the same treatment with no spec amendment here.)
- [ ] Negative-fingerprint grep: `rg 'reqwest::Client|hyper::Client|TcpListener::bind' src/api/mcp_http*.rs` returns zero matches — the `/mcp` handler must not open its own client, listener, or socket. It is a route on the existing router only.
- [ ] Negative-fingerprint grep: `rg 'rustls|server_config|ServerCertVerifier|verify_token|api_key' src/api/mcp_http*.rs` returns zero matches — no auth or TLS code lives in the MCP handler.

---

## Story 4: HTTP-MCP respects the daemon's graceful shutdown

**Story**: As the user, I want HTTP-MCP connections (especially SSE streams) to close cleanly when I stop the daemon, so that I can restart `hmnd` without orphaned sockets or hung agent hosts.

**Acceptance Criteria**:

- [ ] When SIGTERM is sent to `hmnd` while a client has an open SSE GET to `/mcp`, the SSE stream closes cleanly within the daemon's existing graceful-shutdown timeout window.
- [ ] The shutdown path is the existing `with_graceful_shutdown` integration on the Axum server (`src/bin/hmnd.rs:165`); no new shutdown machinery is added in the MCP route handler. Verified by greppable absence of `select!` or signal handling in the MCP handler module.
- [ ] After shutdown, a fresh `hmnd` start binds the same `config.http.bind` without an "address in use" error.

---

## Story 5: Stdio MCP and HTTP-MCP coexist without interference

**Story**: As the user, I want to run `hmn mcp` (stdio MCP) and have HTTP-MCP enabled at the same time, with both serving the same tools, so that I can transition agent hosts between transports without flipping daemon flags.

**Acceptance Criteria**:

- [ ] With `hmnd` running and `mcp.http.enabled = true`, an `hmn mcp` process completes initialize and a `search_filesystem` call against the same daemon, while a separate HTTP-MCP client is also connected and issuing calls.
- [ ] Tool list returned by stdio MCP and HTTP-MCP is identical (same names, same input schemas, same descriptions).
- [ ] `serverInfo.name == "hypomnema"` on both transports (the brand-identity override applies to both).
- [ ] Tool calls complete independently; one transport's call does not block the other's.
