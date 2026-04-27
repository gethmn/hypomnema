# 04 · MCP

> Applies to: step 8. Prereqs:
> [`01-running-the-daemon.md`](./01-running-the-daemon.md) complete and
> `hmnd` is running against the fixture vault. Semantic search through
> MCP additionally requires TEI per [`00-setup.md`](./00-setup.md) §4.

This doc verifies the MCP-over-stdio surface end-to-end. Two paths:

- A **JSON-RPC driver** that pumps newline-delimited frames into
  `hmn mcp`'s stdio — the one you can run from a shell to confirm the
  wire shape against the fixture vault.
- A **Claude Code agent-host** path — the round-2 shipping gate's
  load-bearing test. You configure Claude Code's MCP client to spawn
  `hmn mcp`, then exercise each tool through Claude Code's UX.

The MCP wire shape is the same as the HTTP wire shape per
[ADR-0004](../../docs/decisions/0004-three-search-modes-as-peers.md):
results that match [`fixtures/README.md`](./fixtures/README.md) over
`/search/...` will match over `tools/call` too. Where this doc shows
expected counts and paths, they are the same expectations as
[`03-search.md`](./03-search.md).

`hmn mcp` lives on the **CLI** binary, not the daemon — see
[ADR-0008 § Amendments](../../docs/decisions/0008-two-binary-daemon-plus-cli.md#amendments)
and [ADR-0012](../../docs/decisions/0012-mcp-transport-stdio-v0.md) for
the binary-placement reasoning. The daemon's `mcp.transport` config
knob still parses, but a non-`stdio` value only produces a startup WARN
in `hmnd` (covered in §6 below).

---

## 1. JSON-RPC framing — what `hmn mcp` reads and writes

`hmn mcp` speaks **newline-delimited JSON-RPC 2.0** on stdio (the rmcp
1.5.0 stdio transport — no `Content-Length` headers, no length-prefix,
just one JSON object per line). Stdout carries only JSON-RPC frames;
stderr carries `tracing` logs (per [ADR-0012] Resolution G, enforced by
`BinaryKind::HmnMcp` in `src/logging.rs`). The process exits when
stdin is closed by the parent.

The handshake is the standard MCP three-step: client sends `initialize`,
server responds with capabilities, client sends the `notifications/initialized`
notification. After that, normal request/response pairs flow.

## 2. A small Python driver

Save this as `/tmp/mcp-drive.py`. It opens a single `hmn mcp`
subprocess, runs the handshake, then issues whichever JSON-RPC requests
you pass on the command line, prints each response, and closes stdin
to let the child wind down.

```python
#!/usr/bin/env python3
"""Drive `hmn mcp` over stdio with newline-delimited JSON-RPC."""
import json, subprocess, sys

PROTOCOL = "2025-06-18"

def main(requests):
    p = subprocess.Popen(
        ["hmn", "mcp"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
    )
    def send(msg):
        p.stdin.write((json.dumps(msg) + "\n").encode())
        p.stdin.flush()
    def read():
        return json.loads(p.stdout.readline().decode())

    # Handshake.
    send({"jsonrpc":"2.0","id":0,"method":"initialize","params":{
        "protocolVersion": PROTOCOL,
        "capabilities": {},
        "clientInfo": {"name":"runbook","version":"0"}}})
    print(json.dumps(read(), indent=2))
    send({"jsonrpc":"2.0","method":"notifications/initialized","params":{}})

    # Subsequent requests from argv (each one a JSON object string).
    for i, raw in enumerate(requests, start=1):
        req = json.loads(raw)
        req["jsonrpc"] = "2.0"
        req["id"] = i
        send(req)
        print(json.dumps(read(), indent=2))

    p.stdin.close()
    p.wait(timeout=5)

if __name__ == "__main__":
    main(sys.argv[1:])
```

Make it executable once:

```bash
chmod +x /tmp/mcp-drive.py
```

Each invocation below passes one or more JSON-RPC request objects on the
command line. The driver prints the `initialize` response first, then
one response per request you supplied.

> Stderr is silenced (`Stdio::DEVNULL`) so you only see stdout's JSON
> frames. To inspect logs, drop `stderr=subprocess.DEVNULL` from the
> driver — the daemon-side request lines, the rmcp transport lines,
> and the connect attempts will appear, all on stderr where the
> protocol expects them.

---

## 3. Handshake and tool listing

### A. `initialize`

Run the driver with no further requests:

```bash
/tmp/mcp-drive.py
```

Expect a single JSON-RPC response. Spot-check:

- `result.serverInfo.name` is `"hypomnema"` (the brand-identity override
  per [ADR-0012]; without it, rmcp would advertise itself as `"rmcp"`).
- `result.serverInfo.version` is the crate version (`"0.1.0"` at v0
  ship).
- `result.protocolVersion` is `"2025-06-18"`.
- `result.capabilities.tools` is an object (the server advertises tool
  capability).

### B. `tools/list`

```bash
/tmp/mcp-drive.py '{"method":"tools/list","params":{}}' | jq '.result.tools[] | .name'
```

Expect exactly three tools, in any order:

```
"search_filesystem"
"search_content"
"search_semantic"
```

Tool descriptions and per-field input schemas should also round-trip:

```bash
/tmp/mcp-drive.py '{"method":"tools/list","params":{}}' \
  | jq '.result.tools[] | {name, description, props: (.inputSchema.properties | keys)}'
```

Each tool's `description` field is non-empty (sourced from the
`#[tool(description = "...")]` macro). Each `inputSchema.properties`
includes the field names from the request types in `src/api/types.rs`
(e.g. `search_filesystem` exposes `glob` and `prefix`; per-field
descriptions appear at `properties.<name>.description`).

---

## 4. `tools/call` against the fixture vault

The expected counts and paths below match
[`fixtures/README.md`](./fixtures/README.md) — same wire shape over MCP
as over HTTP.

### A. `search_filesystem` — match every Markdown file

```bash
/tmp/mcp-drive.py '{"method":"tools/call","params":{
  "name":"search_filesystem","arguments":{"glob":"**/*.md"}}}' \
  | jq '.result | .isError, (.structuredContent.results | length)'
```

Expect:

```
false
7
```

(`isError: false`, seven indexed `.md` files — same set as
[`03-search.md`](./03-search.md) §A.)

The result paths land at `result.structuredContent.results[].path`,
ascending. `result.structuredContent.truncated` is `false`.

### B. `search_content` — substring match

```bash
/tmp/mcp-drive.py '{"method":"tools/call","params":{
  "name":"search_content","arguments":{"query":"pgvector"}}}' \
  | jq '.result.structuredContent.results[].path'
```

Expect exactly:

```
"notes/databases/pgvector.md"
```

`.results[0].match_count >= 2` (heading line plus body mention) — the
same contract as [`03-search.md`](./03-search.md) §A in the
content-search section.

### C. `search_semantic` — top-1 match

```bash
/tmp/mcp-drive.py '{"method":"tools/call","params":{
  "name":"search_semantic","arguments":{"query":"heading-aware document chunking"}}}' \
  | jq '.result.structuredContent.results[0].file_path'
```

Expect:

```
"notes/design/chunking.md"
```

Each result carries `score`, `file_path`, `chunk_index`, `heading_path`,
and `text` (same shape as `/search/semantic`).

> If TEI is **down**, `tools/call search_semantic` returns
> `result.isError: true` with
> `result.structuredContent.error.code == "embedding_unavailable"` —
> the HTTP envelope flows through unchanged. `search_filesystem` and
> `search_content` still succeed.

### D. Invalid glob → `structured_error`

```bash
/tmp/mcp-drive.py '{"method":"tools/call","params":{
  "name":"search_filesystem","arguments":{"glob":"[unterminated"}}}' \
  | jq '.result | .isError, .structuredContent.error.code'
```

Expect:

```
true
"invalid_glob"
```

The HTTP error envelopes (`invalid_glob`, `invalid_regex`,
`invalid_prefix`, `invalid_request`, `embedding_unavailable`,
`internal`) all flow through MCP unchanged at
`result.structuredContent.error.code`. `result.isError` is `true`.

---

## 5. Daemon-unreachable error path

This is the only error code that's **new at the MCP layer** —
`hmnd` doesn't itself emit `daemon_unreachable` over HTTP because the
TCP connect itself fails before any HTTP exchange. The MCP shim
synthesizes the envelope locally when the underlying `reqwest` connect
returns an error.

In terminal A, Ctrl+C `hmnd`. Then:

```bash
/tmp/mcp-drive.py '{"method":"tools/call","params":{
  "name":"search_filesystem","arguments":{"glob":"**/*.md"}}}' \
  | jq '.result | .isError, .structuredContent.error.code, .structuredContent.error.message'
```

Expect:

```
true
"daemon_unreachable"
"http://127.0.0.1:7777 did not respond: …<connect error detail>…"
```

The configured daemon URL is embedded verbatim in the error message
(useful when debugging an MCP host whose `command` / `args` point at
the wrong port). The `hmn mcp` process **does not** exit with code 4
when the daemon is unreachable — that exit-code-4 mapping is for
`hmn search …` mode. In MCP mode the connect error is reported via
`structured_error` and the process keeps serving (it exits only on
stdin close).

Restart `hmnd` in terminal A before continuing.

### Stderr / stdout split sanity check

`hmn mcp` must keep stdout free of anything but JSON-RPC frames. The
load-bearing test is to bump verbosity and confirm the split holds:

```bash
hmn -vv mcp < /dev/null > /tmp/mcp-stdout.txt 2> /tmp/mcp-stderr.txt
```

(`/dev/null` for stdin closes immediately; the child exits on stdin
EOF.)

```bash
wc -l /tmp/mcp-stdout.txt /tmp/mcp-stderr.txt
```

Expect **0 lines** in `mcp-stdout.txt` (no requests were sent, so no
responses) and **at least one line** in `mcp-stderr.txt` (the `-vv`
DEBUG tracing line(s) for the parsed CLI). If anything ever lands in
`mcp-stdout.txt` with `< /dev/null` and no requests, that's stdout
pollution — a regression in `BinaryKind::HmnMcp`'s logging-writer
override.

---

## 6. Non-stdio `mcp.transport` warning (daemon side)

`mcp.transport = "socket"` is **not implemented** in v0. The daemon
parses the value, logs a WARN at startup, and otherwise behaves
normally — `/health` keeps returning 200, the watcher runs, HTTP
search works. The deferred socket transport will live in `hmnd` when
it ships.

In a scratch directory:

```bash
cat > /tmp/hmn-socket-test.toml <<'TOML'
vault = "<ABSOLUTE_PATH_TO_REPO>/notes/manual-testing/fixtures/sample-vault"

[http]
bind = "127.0.0.1:7778"

[mcp]
transport = "socket"

[embedding]
endpoint = "http://127.0.0.1:8080/v1/embeddings"
model = "nomic-embed-text-v1.5"
dimension = 768
TOML
```

Stop the main `hmnd` in terminal A first (so the alternate config can
bind a different port). Then start the daemon with the alternate
config:

```bash
hmnd -c /tmp/hmn-socket-test.toml 2>&1 | tee /tmp/hmn-socket.log
```

Expect a single `WARN`-level line near startup, after the config
summary and before `Store::open`:

```
WARN hmnd: mcp.transport = "socket" is not implemented in v0; only stdio via the `hmn mcp` subcommand on the CLI binary is shipped. The socket file is NOT bound. To use MCP, invoke `hmn mcp` from the agent host. configured=socket socket=…
```

While the daemon is running, in terminal B:

```bash
curl -s http://127.0.0.1:7778/health
```

Expect `{"status":"ok"}` — the daemon did not crash; the socket file
named in the WARN is **not** present on disk; HTTP serves normally.

Ctrl+C the daemon, restart the main config in terminal A as before.

> This is a `hmnd` mode (set via `[mcp].transport` in config), not a
> `hmn mcp` mode. The MCP-over-stdio surface ships exclusively as the
> `hmn mcp` CLI subcommand in v0; the `mcp.transport` knob is
> forward-compat for the deferred socket transport.

---

## 7. Claude Code agent-host path (round-2 shipping gate)

The JSON-RPC driver path verifies the wire shape; this section verifies
that a real MCP host consumes it correctly. This is the load-bearing
acceptance test for the round-2 shipping gate — running this end-to-end
is the only way to catch host-specific UX issues (tool-name mangling,
schema rendering, structured-content display).

You will need a working Claude Code install. The configuration shape
below matches Claude Code's MCP client format at the time step 8
shipped; verify against current Claude Code docs at test time —
configuration locations and key names occasionally drift.

### A. Build release-mode binaries

From the repo root:

```bash
cargo build --release --bin hmnd --bin hmn
realpath target/release/hmn
```

Capture the absolute path to `hmn`. The MCP host needs an absolute
path because it spawns the binary directly (no shell, no PATH
expansion).

### B. Configure Claude Code

Add a `hypomnema` entry under `mcpServers` in Claude Code's MCP config
(per current Claude Code docs at test time):

```json
{
  "mcpServers": {
    "hypomnema": {
      "command": "/abs/path/to/target/release/hmn",
      "args": ["mcp"]
    }
  }
}
```

Restart Claude Code (or hot-reload its MCP server connection per its
UX). Make sure `hmnd` is running in the meantime — the spawned
`hmn mcp` will try to connect to it.

### C. Verify tool listing

In Claude Code's tool listing for the `hypomnema` MCP server, expect
exactly three tools:

- `search_filesystem`
- `search_content`
- `search_semantic`

Each tool's description and parameter schema should render. Spot-check
the **server identity**: Claude Code displays the MCP server's
`serverInfo.name`, which should be `hypomnema` (not `rmcp`). If you see
`rmcp`, the brand-identity override from
[ADR-0012] regressed.

### D. Exercise each tool

Through Claude Code's tool-call UX (or by asking Claude Code to run
the tool), invoke each tool against the fixture vault:

| Tool | Arguments | Expect |
|---|---|---|
| `search_filesystem` | `{"glob":"**/*.md"}` | 7 paths from the fixture vault |
| `search_content` | `{"query":"pgvector"}` | One result: `notes/databases/pgvector.md` |
| `search_semantic` | `{"query":"heading-aware document chunking"}` | Top result is `notes/design/chunking.md` (TEI must be up) |

The same expected-results contract from
[`03-search.md`](./03-search.md) and
[`fixtures/README.md`](./fixtures/README.md) applies — if a query
returns the right shape over `/search/...`, it should return the same
shape via Claude Code's MCP UX.

### E. Daemon-down error path

In terminal A, Ctrl+C `hmnd`. In Claude Code, invoke any tool. Expect
Claude Code to render a structured error whose `error.code` is
`daemon_unreachable` and whose message embeds the configured daemon
URL. (The exact rendering is at Claude Code's discretion — what
matters is that the structured error is visible, not buried as a
generic transport failure.)

Restart `hmnd` and confirm the next tool call succeeds without
restarting Claude Code or its MCP connection.

### F. What passing looks like

All of these must hold for the gate to pass:

- All three tools list with descriptions and parameter schemas.
- Each tool round-trips through Claude Code with a structured result
  (`structuredContent` rendered or surfaced in some user-facing way).
- The daemon-down path produces a structured error visible to the
  user with `daemon_unreachable` as the code.
- No stdout-pollution incidents — Claude Code does not surface
  malformed-message errors at any point during the session.

If any criterion fails, capture the transcript (Claude Code's tool-call
panel screenshots are sufficient) and route per the workplan's failure
handling — typically a `coordinator-only` soft flag, with an act-now
follow-up only if the failure is a real bug with a small, well-bounded
fix.

---

## 8. Pass criteria summary

Mirroring [`03-search.md`](./03-search.md) §Wrapping up — if everything
above lined up, the MCP surface is healthy:

- Driver path: handshake returns `serverInfo.name == "hypomnema"`,
  three tools advertised with non-empty descriptions and per-field
  schemas, each tool round-trips with the same wire shape
  [`fixtures/README.md`](./fixtures/README.md) documents for HTTP.
- Error paths: invalid glob produces `structuredContent.error.code ==
  "invalid_glob"`; daemon-down produces `daemon_unreachable` with the
  configured URL embedded; semantic-with-TEI-down produces
  `embedding_unavailable`.
- Stdout/stderr split: `hmn -vv mcp < /dev/null` writes 0 stdout
  lines, ≥1 stderr line.
- Daemon mode: `mcp.transport = "socket"` produces a startup WARN, no
  crash, `/health` returns 200.
- Claude Code: tool listing shows the three tools under server name
  `hypomnema`, each tool round-trips, daemon-down surfaces a
  structured error.

Drift on any specific check points at either fixture-content drift or
a real regression in the MCP wrapper, the brand-identity override, the
logging-writer split, or the daemon-side warning emission — investigate.

[ADR-0012]: ../../docs/decisions/0012-mcp-transport-stdio-v0.md
