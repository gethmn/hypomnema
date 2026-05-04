# Manual testing — Hypomnema through step 12 / round 4

Hand-driven runbook for verifying everything Hypomnema has shipped
through round 4: the round-1 single-vault foundation (skeleton,
scan + hash, watcher, outbox, HTTP search, chunking + embedding,
semantic search), the round-2 stdio MCP wrapper, the round-3
multi-vault registry and lifecycle ops, and the round-4 HTTP-MCP
(Streamable HTTP) transport. The automated test suite (`cargo
nextest run` or `cargo test`) is the primary regression net; this
directory is its complement — what to run end-to-end when something
feels off, when bringing the daemon up on a new machine, or when
wiring in a new capability and you want to feel the surface.

## Reading order

1. [`00-setup.md`](./00-setup.md) — build the binaries, bring up TEI,
   write a config, register the two fixture vaults via `hmn vault
   create`.
2. [`01-running-the-daemon.md`](./01-running-the-daemon.md) — start
   `hmnd` against a multi-vault registry, check `/health`,
   `hmn status`, `hmn vault list`, shut down cleanly.
3. [`02-watcher-and-outbox.md`](./02-watcher-and-outbox.md) — verify
   per-vault outboxes at `<data_dir>/vaults/<id>/outbox.jsonl`,
   created/modified/deleted events, ignore patterns, sync-conflict
   filtering, and debounce behavior.
4. [`03-search.md`](./03-search.md) — run all three search modes
   across both fixture vaults (cross-vault by default; `--vaults`
   filter; partial-results diagnostic for paused/errored vaults
   and unknown vault selectors).
5. [`04-mcp.md`](./04-mcp.md) — drive `hmn mcp` (stdio transport)
   via JSON-RPC, exercise all 12 tools (3 search + 9 vault), run
   the Claude Code agent-host integration test.
6. [`05-vault-management.md`](./05-vault-management.md) — exercise
   the full nine-op `hmn vault {create,list,status,pause,resume,
   reset,rename,rescan,terminate}` lifecycle.
7. [`06-mcp-http.md`](./06-mcp-http.md) — drive the round-4
   Streamable HTTP MCP transport at
   `http://127.0.0.1:7777/mcp` via curl + JSON-RPC; verify Origin
   allow-list; diff stdio vs HTTP `tools/list`; toggle
   `mcp.http.enabled` and `mcp.http.path`; optionally exercise an
   MCP-HTTP-capable client (Iris or browser-host).

## Fixture vaults

Two committed Markdown vaults engineered to produce predictable
search outcomes:

- [`fixtures/sample-vault/`](./fixtures/sample-vault/) — the
  databases-and-design vault from steps 1–8 (7 indexed files).
- [`fixtures/sample-vault-2/`](./fixtures/sample-vault-2/) — the
  cooking-and-technique vault added in round 4 (10 indexed files).

[`fixtures/README.md`](./fixtures/README.md) is the
expected-results contract for every example query against both
vaults. Treat that file as canonical for this runbook.

## Surface covered as of step 12 / round 4

| Area | Covered | Notes |
|---|---|---|
| Daemon boot, scan, idle | ✅ | `00`, `01` |
| Watcher + per-vault outbox | ✅ | `02` |
| `/health`, `/status`, `hmn status` | ✅ | `01` |
| `/search/filesystem` + `hmn search filesystem` | ✅ | `03` |
| `/search/content` + `hmn search content` (substring) | ✅ | `03` |
| `/search/content` regex + case modes (curl only) | ✅ | `03` — CLI flags not yet exposed |
| `/search/semantic` + `hmn search semantic` | ✅ | `03` (requires TEI) |
| Cross-vault search + `--vaults` filter + partial-results | ✅ | `03` (round 3) |
| MCP transport — stdio (round 1) | ✅ | `04` — `hmn mcp` + Claude Code agent-host path |
| `hmn vault {create,list,status,terminate}` | ✅ | `05` (round 3, step 10) |
| `hmn vault {pause,resume,reset,rename,rescan}` | ✅ | `05` (round 3, step 11) |
| MCP tool surface — 12 tools (3 search + 9 vault) | ✅ | `04`, `06` |
| MCP transport — HTTP (Streamable HTTP, round 4) | ✅ | `06` — same 12 tools served at `/mcp`; Origin allow-list; default-on |
| MCP transport — Unix socket (deferred) | ❌ | not shipped; daemon-side `mcp.transport = "socket"` produces a startup WARN per `04` §6 |

## Version-skew warning

Through round 4, [`docs/reference/configuration.md`](../../docs/reference/configuration.md)
and [`docs/reference/cli.md`](../../docs/reference/cli.md) describe
the same surface this runbook drives — multi-vault registry, full
nine-op vault lifecycle on `hmn vault …`, Streamable HTTP MCP under
`[mcp.http]`. Docs and shipped code align; there's no longer a
future-state preview.

The crate version reported by `serverInfo.version` (over MCP) and by
`hmn --version` / `hmnd --version` is whatever `Cargo.toml` records
at the time of the build. The round-4 ship target is `0.3.0`; a
development build of the round-4 commit before the boundary-ritual
version bump reports `0.2.0`. Either is consistent with the runbook;
the pass criteria check the brand identity (`hypomnema`), not a
specific version number.
