# 06 · MCP over HTTP (Streamable HTTP transport)

> Applies to: round 4 / step 12. Prereqs:
> [`01-running-the-daemon.md`](./01-running-the-daemon.md) complete and
> `hmnd` is running against both fixture vaults; semantic search
> additionally requires TEI per [`00-setup.md`](./00-setup.md) §4.

This doc verifies the round-4 HTTP-MCP transport — the same 12-tool
surface as [`04-mcp.md`](./04-mcp.md), served over
`http://127.0.0.1:7777/mcp` on the daemon's existing Axum listener.
Round 4 ships the rmcp Streamable HTTP transport ([ADR-0013],
[spec](../../docs/specs/mcp-streamable-http.md)) as a peer to the
round-1 stdio transport; both sit on top of the same
`HypomnemaMcpServer` handler and the same `HypomnemaBackend` trait,
so the wire shapes are byte-equivalent across transports.

The defenses on the HTTP-MCP route are deliberately minimal in v1:
the route inherits `hmnd`'s loopback-only HTTP listener trust
posture (no auth, no TLS), and the only HTTP-MCP-specific guard is
an Origin-header allow-list for browser DNS-rebinding mitigation
(see §6 below).

[ADR-0013]: ../../docs/decisions/0013-mcp-transport-streamable-http.md

## 1. Confirm HTTP-MCP is mounted

The runbook's config in [`00-setup.md`](./00-setup.md) §5 sets
`mcp.http.enabled = true` (also the shipped default). At daemon
startup, expect this `INFO` log line in terminal A:

```
hmnd: mcp http transport mounted path=/mcp enabled=true
```

If you see `hmnd: mcp http transport disabled` instead, the config
toggled it off — see §8 below.

A quick sanity check from terminal B:

```bash
curl -s -o /dev/null -w '%{http_code}\n' \
  -X POST http://127.0.0.1:7777/mcp \
  -H 'Accept: application/json, text/event-stream' \
  -H 'Content-Type: application/json' \
  -d '{}'
```

Expect a non-404 status (typically `400` for an empty body, or `200`
with an MCP error frame). Anything that's **not** `404` confirms the
route is mounted.

## 2. JSON-RPC handshake — `initialize`

The Streamable HTTP transport is plain HTTP — request/response
JSON-RPC 2.0 frames in POST bodies, with optional SSE upgrades for
notifications. The simplest exercise is curl with `Content-Type:
application/json`:

```bash
curl -s -X POST http://127.0.0.1:7777/mcp \
  -H 'Accept: application/json, text/event-stream' \
  -H 'Content-Type: application/json' \
  -d '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "initialize",
    "params": {
      "protocolVersion": "2025-06-18",
      "capabilities": {},
      "clientInfo": {"name": "runbook", "version": "0"}
    }
  }' | jq '.result.serverInfo, .result.protocolVersion'
```

Expect:

```json
{"name":"hypomnema","version":"<crate version from Cargo.toml>"}
"2025-06-18"
```

`serverInfo.name` is the brand-identity override per [ADR-0012]; the
HTTP transport advertises the same identity as stdio. `serverInfo.version`
is the crate version (`0.3.0` post-round-4-bump; `0.2.0` on a
development build of this commit).

[ADR-0012]: ../../docs/decisions/0012-mcp-transport-stdio-v0.md

> Per the Streamable HTTP transport's lifecycle, a real client follows
> `initialize` with a `notifications/initialized` POST before issuing
> `tools/list` or `tools/call`. The runbook's curl examples below
> elide the notification — the server is permissive about ordering
> for stateless request/response calls — but Iris and other compliant
> clients will send it automatically.

## 3. `tools/list`

```bash
curl -s -X POST http://127.0.0.1:7777/mcp \
  -H 'Accept: application/json, text/event-stream' \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' \
  | jq '.result.tools | length, [.[] | .name] | sort'
```

Expect **12** tools — the canonical set from
[`04-mcp.md`](./04-mcp.md) §3.B:

```
"search_filesystem"
"search_content"
"search_semantic"
"vault_list"
"vault_status"
"vault_create"
"vault_pause"
"vault_resume"
"vault_reset"
"vault_rename"
"vault_rescan"
"vault_terminate"
```

Diff against the stdio transport — they must match exactly:

```bash
# stdio (per 04-mcp.md §3.B):
/tmp/mcp-drive.py '{"method":"tools/list","params":{}}' \
  | jq '.result.tools | [.[] | .name] | sort' > /tmp/stdio-tools.json

# HTTP:
curl -s -X POST http://127.0.0.1:7777/mcp \
  -H 'Accept: application/json, text/event-stream' \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}' \
  | jq '.result.tools | [.[] | .name] | sort' > /tmp/http-tools.json

diff /tmp/stdio-tools.json /tmp/http-tools.json
```

Expect zero diff. Tool descriptions and `inputSchema.properties`
should also match across transports — both render from the same
`#[tool(...)]` macros on `HypomnemaMcpServer`.

## 4. `tools/call` — search tools

The expected counts and paths below match
[`fixtures/README.md`](./fixtures/README.md) and
[`03-search.md`](./03-search.md) — same wire shape as HTTP and stdio.

### A. `search_filesystem` across both vaults

```bash
curl -s -X POST http://127.0.0.1:7777/mcp \
  -H 'Accept: application/json, text/event-stream' \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{
    "name":"search_filesystem",
    "arguments":{"glob":"**/*.md"}}}' \
  | jq '.result | .isError, (.structuredContent.results | length)'
```

Expect:

```
false
17
```

(7 from `sample` + 10 from `sample-2`.)

### B. `search_content` — substring (vault A by content)

```bash
curl -s -X POST http://127.0.0.1:7777/mcp \
  -H 'Accept: application/json, text/event-stream' \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{
    "name":"search_content",
    "arguments":{"query":"pgvector"}}}' \
  | jq '.result.structuredContent.results[] | {vault_name, path, match_count}'
```

Expect a single row from vault `sample`:

```json
{"vault_name":"sample","path":"notes/databases/pgvector.md","match_count":2}
```

### C. `search_semantic` — top-1 (vault B)

```bash
curl -s -X POST http://127.0.0.1:7777/mcp \
  -H 'Accept: application/json, text/event-stream' \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{
    "name":"search_semantic",
    "arguments":{"query":"wild yeast culture maintenance"}}}' \
  | jq '.result.structuredContent.results[0] | {path, vault_name}'
```

Expect:

```json
{"path":"ingredients/sourdough-starter.md","vault_name":"sample-2"}
```

### D. Equivalence check against `/search/...`

The HTTP-MCP route and the plain `/search/...` routes share the same
backend. Issue both for the same query and compare:

```bash
# HTTP-MCP
curl -s -X POST http://127.0.0.1:7777/mcp \
  -H 'Accept: application/json, text/event-stream' \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{
    "name":"search_filesystem","arguments":{"glob":"**/*.md"}}}' \
  | jq '.result.structuredContent' > /tmp/mcp-fs.json

# Plain HTTP
curl -s -X POST http://127.0.0.1:7777/search/filesystem \
  -H 'Content-Type: application/json' \
  -d '{"glob":"**/*.md"}' > /tmp/http-fs.json

diff /tmp/mcp-fs.json /tmp/http-fs.json
```

Expect zero diff — modulo `partial_results` ordering, which is
typically empty for an all-active multi-vault setup.

## 5. `tools/call` — vault management

The full nine-op vault surface is reachable over HTTP-MCP using the
same shapes the CLI uses (see
[`05-vault-management.md`](./05-vault-management.md)).

### A. `vault_list`

```bash
curl -s -X POST http://127.0.0.1:7777/mcp \
  -H 'Accept: application/json, text/event-stream' \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{
    "name":"vault_list","arguments":{}}}' \
  | jq '.result.structuredContent.vaults | length, [.[].name] | sort'
```

Expect:

```
2
["sample","sample-2"]
```

Cross-check against `GET /vaults`:

```bash
curl -s http://127.0.0.1:7777/vaults | jq '.vaults | length, [.[].name] | sort'
```

Should match.

### B. `vault_status`

```bash
curl -s -X POST http://127.0.0.1:7777/mcp \
  -H 'Accept: application/json, text/event-stream' \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":8,"method":"tools/call","params":{
    "name":"vault_status","arguments":{"target":"sample-2"}}}' \
  | jq '.result.structuredContent | {name, status}'
```

Expect:

```json
{"name":"sample-2","status":"active"}
```

### C. `vault_pause` / `vault_resume` round-trip

```bash
# pause
curl -s -X POST http://127.0.0.1:7777/mcp \
  -H 'Accept: application/json, text/event-stream' \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{
    "name":"vault_pause","arguments":{"target":"sample-2"}}}' \
  | jq '.result.structuredContent.status'
# expect: "paused"

# resume
curl -s -X POST http://127.0.0.1:7777/mcp \
  -H 'Accept: application/json, text/event-stream' \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":10,"method":"tools/call","params":{
    "name":"vault_resume","arguments":{"target":"sample-2"}}}' \
  | jq '.result.structuredContent.status'
# expect: "active"
```

The other write tools (`vault_create`, `vault_reset`, `vault_rename`,
`vault_rescan`, `vault_terminate`) work the same way — body shape
matches the HTTP control-plane request types in
[`docs/specs/vault-management.md` § Control-Plane HTTP Wire Shapes](../../docs/specs/vault-management.md#control-plane-http-wire-shapes).
The `enable_write_tools = false` gate (see
[`04-mcp.md`](./04-mcp.md) §3.B) applies identically over HTTP-MCP.

## 6. Origin validation

The HTTP-MCP route is gated by an Origin-header allow-list to defend
against browser DNS-rebinding attacks (the spec's only HTTP-MCP-
specific defense beyond the loopback bind). Loopback origins,
missing origins, and `Origin: null` are accepted; everything else
returns 403.

### A. Remote origin → 403

```bash
curl -s -o /dev/null -w '%{http_code}\n' \
  -X POST http://127.0.0.1:7777/mcp \
  -H 'Origin: http://example.com' \
  -H 'Accept: application/json, text/event-stream' \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}'
```

Expect `403`. Include `-i` (or drop `-o /dev/null`) to inspect the
body — exactly:

```
Origin not allowed: http://example.com
```

### B. Loopback origins are accepted

```bash
for origin in 'http://localhost' 'http://localhost:1234' \
              'http://127.0.0.1' 'http://127.0.0.1:7777' \
              'http://[::1]' 'http://[::1]:7777' 'null'; do
  echo -n "$origin -> "
  curl -s -o /dev/null -w '%{http_code}\n' \
    -X POST http://127.0.0.1:7777/mcp \
    -H "Origin: $origin" \
    -H 'Accept: application/json, text/event-stream' \
    -H 'Content-Type: application/json' \
    -d '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}'
done
```

Expect every line to be **not** 403 (typically 200). The same is true
when the `Origin` header is absent entirely (the curl examples in §2–§5
above run without an Origin header).

### C. HTTPS loopback is rejected

```bash
curl -s -o /dev/null -w '%{http_code}\n' \
  -X POST http://127.0.0.1:7777/mcp \
  -H 'Origin: https://127.0.0.1:7777' \
  -H 'Accept: application/json, text/event-stream' \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}'
```

Expect `403`. Only `http://` loopback origins are in the allow-list
per the spec — browser contexts that hit a local server use http; an
`https://127.0.0.1` origin is unusual and not whitelisted.

## 7. Graceful shutdown

In terminal A, send SIGTERM to `hmnd` (Ctrl+C, or `kill -TERM <pid>`).
Expect:

- the daemon's per-vault watchers and indexers drain.
- the Axum listener stops accepting new connections.
- in-flight `tools/call` POSTs that are mid-handler complete before
  the listener exits (`with_graceful_shutdown` semantics).
- the `hmnd: drain complete, exiting cleanly` log line.

A subsequent `hmnd` start binds the same `[http].bind` cleanly — no
"address in use" error.

## 8. Disabling HTTP-MCP

Edit the config:

```toml
[mcp.http]
enabled = false
```

Stop the daemon and restart. Expect this log line at startup:

```
hmnd: mcp http transport disabled enabled=false
```

`/mcp` now returns 404:

```bash
curl -s -o /dev/null -w '%{http_code}\n' \
  -X POST http://127.0.0.1:7777/mcp \
  -H 'Accept: application/json, text/event-stream' \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}'
```

Expect `404`.

The plain HTTP search routes continue to serve normally — the toggle
is local to the `/mcp` route only:

```bash
curl -s -o /dev/null -w '%{http_code}\n' \
  -X POST http://127.0.0.1:7777/search/filesystem \
  -H 'Content-Type: application/json' \
  -d '{"glob":"**/*.md"}'
```

Expect `200`.

The MCP-over-stdio transport (`hmn mcp` per
[`04-mcp.md`](./04-mcp.md)) is unaffected by `mcp.http.enabled` —
stdio is owned by the CLI binary; the HTTP transport is owned by
the daemon. Set `mcp.http.enabled = true` (or remove the override —
true is the default) to restore HTTP-MCP, then restart `hmnd` before
continuing.

### Path validation

```toml
[mcp.http]
enabled = true
path = "/foo"
```

Restart `hmnd`. Expect daemon startup to fail with the message:

```
mcp.http.path must be "/mcp" in this version of Hypomnema
```

Path is reserved-for-forward-compat in v1. Reset to `path = "/mcp"`
(or remove the line — the default is `/mcp`) before continuing.

## 9. Optional: an MCP-HTTP-capable client

The runbook's load-bearing exercise above is curl + JSON-RPC, which
verifies the wire shape. To exercise the transport from a real MCP
host, configure a Streamable-HTTP-capable client to point at
`http://127.0.0.1:7777/mcp`. As of round 4, Iris supports
the Streamable HTTP transport and is the reference client; other
hosts (a browser-based MCP client, custom integration) work too if
they speak the spec. The exact configuration UI is host-specific —
consult the host's docs for how to add a Streamable HTTP MCP server.

Steps (Iris-flavored — adapt to whatever client you have):

1. In the client's MCP server config, add a Streamable-HTTP entry
   pointing at `http://127.0.0.1:7777/mcp`.
2. Restart or hot-reload the client's MCP connection.
3. From the client's tool listing, expect the same 12 tools as
   [`04-mcp.md`](./04-mcp.md) §3.B; spot-check `serverInfo.name ==
   "hypomnema"` if the client surfaces it.
4. Invoke `search_filesystem` with `{"glob":"**/*.md"}`; expect 17
   results across the two vaults.
5. Invoke `vault_list`; expect both fixture vault rows.
6. Stop `hmnd` (Ctrl+C terminal A); invoke any tool from the client;
   expect a structured transport error visible in the client's UI.
   Restart `hmnd`; subsequent calls should succeed without
   reconnecting the client (the Streamable HTTP transport reconnects
   on demand).

If the client errors on `Mcp-Session-Id` handling, on Origin-header
restrictions, or on body-format validation — that's a real
integration surface for the round-4 retro and a candidate for an
addendum to [`docs/specs/mcp-streamable-http.md`](../../docs/specs/mcp-streamable-http.md).
The runbook's curl-driven path is the load-bearing wire-shape check;
a real MCP host adds host-rendering coverage on top.

## Pass criteria summary

If everything above lined up, the round-4 HTTP-MCP transport is
healthy:

- `hmnd` startup logs `mcp http transport mounted path=/mcp` when
  `mcp.http.enabled = true` (the default).
- `initialize` returns `serverInfo.name == "hypomnema"` and
  `protocolVersion == "2025-06-18"`.
- `tools/list` returns the same 12 tools as the stdio transport;
  diffing both produces zero output.
- `tools/call` for each tool round-trips with the same wire shape as
  the equivalent `/search/...` or `/vaults/...` HTTP request; the
  byte-equivalence check in §4.D passes.
- Origin allow-list: remote and `https://` origins return 403 with
  the exact body `Origin not allowed: <value>`; loopback origins,
  `null`, and missing-Origin requests pass through.
- Graceful shutdown completes; subsequent restart binds the same
  port without error.
- `mcp.http.enabled = false` makes `/mcp` return 404 while
  `/search/*` continues to serve normally.
- `mcp.http.path != "/mcp"` rejects daemon startup with the
  documented error message.
- (Optional) An MCP-HTTP-capable client lists the 12 tools and
  round-trips at least one search and one vault op.

You're done with the runbook. The full smoke matrix run by the
round-4 shipping gate composes
[`00-setup.md`](./00-setup.md) through this doc end-to-end against a
multi-vault daemon — see the workplan's § Task 12.7.
