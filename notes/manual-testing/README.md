# Manual testing — Hypomnema through step 8

Hand-driven runbook for verifying everything Hypomnema has shipped so far
(steps 1–8: skeleton, scan + hash, watcher, outbox, HTTP filesystem +
content search, chunking + embedding, semantic search, MCP-over-stdio
wrapper). The automated test suite (`cargo nextest run`) is the primary
regression net; this directory is its complement — what to run
end-to-end when something feels off, when bringing the daemon up on a
new machine, or when wiring in a new capability and you want to feel
the surface.

## Reading order

1. [`00-setup.md`](./00-setup.md) — build the binaries, install the
   sqlite-vec extension, bring up TEI, write a config.
2. [`01-running-the-daemon.md`](./01-running-the-daemon.md) — start
   `hmnd` against the fixture vault, check `/health`, `hmn status`,
   shut down cleanly.
3. [`02-watcher-and-outbox.md`](./02-watcher-and-outbox.md) — verify
   created/modified/deleted events, ignore patterns, sync-conflict
   filtering, and debounce behavior.
4. [`03-search.md`](./03-search.md) — run all three search modes
   (filesystem, content, semantic) against the fixture vault and check
   results against [`fixtures/README.md`](./fixtures/README.md).
5. [`04-mcp.md`](./04-mcp.md) — drive `hmn mcp` via JSON-RPC, run the
   Claude Code agent-host integration test, verify the daemon-side
   non-stdio `mcp.transport` warning.

## Fixture vault

[`fixtures/sample-vault/`](./fixtures/sample-vault/) is a small,
committed Markdown vault engineered to produce predictable search
outcomes. [`fixtures/README.md`](./fixtures/README.md) is the
expected-results contract for every example query. Treat that table as
canonical for this runbook.

## Surface covered as of step 8

| Area | Covered | Notes |
|---|---|---|
| Daemon boot, scan, idle | ✅ | `00`, `01` |
| Watcher + outbox | ✅ | `02` |
| `/health`, `/status`, `hmn status` | ✅ | `01` |
| `/search/filesystem` + `hmn search filesystem` | ✅ | `03` |
| `/search/content` + `hmn search content` (substring) | ✅ | `03` |
| `/search/content` regex + case modes (curl only) | ✅ | `03` — CLI flags not yet exposed |
| `/search/semantic` + `hmn search semantic` | ✅ | `03` (requires TEI) |
| MCP transport (step 8) | ✅ | `04` — `hmn mcp` stdio + Claude Code agent-host path |
| `hmn vault …` subcommands (round 3) | ❌ | not yet shipped |

## Version-skew warning

[`docs/reference/configuration.md`](../../docs/reference/configuration.md)
and [`docs/reference/cli.md`](../../docs/reference/cli.md) are at
version `0.2.0` and describe the **future** multi-vault state
(`hmn vault create`, top-level `vault =` removed, runtime vault
registry). The shipped code through step 8 is still single-vault — it
reads the top-level `vault` key from `config.toml` and `hmn` has
`status`, `search {filesystem,content,semantic}`, and `mcp` subcommands
only. The docs in this directory target the shipped reality.

When round 3 (multi-vault) lands, this directory will need updating
alongside.
