# CLI Reference: `hmnd` and `hmn`

**Version**: 0.3.0
**Generated**: 2026-04-27

---

> **Status**: Subcommand surface pinned in step 1 — see [step-01 workplan § CLI subcommand naming](../roadmap/step-01-workplan.md#1-cli-subcommand-naming). Hypomnema ships two binaries per [ADR-0008](../decisions/0008-two-binary-daemon-plus-cli.md): the daemon (`hmnd`) and the CLI client (`hmn`). Vault management lives on `hmn vault …` per [ADR-0011](../decisions/0011-vault-management-on-hmn.md).

---

Hypomnema ships two binaries:

- **`hmnd`** — the daemon. Owns the watched directory, the SQLite store, and the HTTP server. Long-running; typically managed by systemd or an equivalent supervisor. (The deferred Unix-socket MCP transport will also live in `hmnd` when it ships — see [ADR-0012](../decisions/0012-mcp-transport-stdio-v0.md).)
- **`hmn`** — the CLI client. Thin wrapper that speaks HTTP to a running `hmnd`. Used for day-to-day search, status, scripts, and the MCP-over-stdio surface (`hmn mcp`).

Both read the same configuration file by default ([configuration.md](./configuration.md)); `hmn` only consults the subset it needs to reach `hmnd` (the daemon URL).

---

## `hmnd` — the daemon

### Synopsis

```
hmnd [global-options] [subcommand] [subcommand-options]
```

Running `hmnd` with no subcommand starts the daemon in the foreground.

### Global Options

| Option | Short | Description | Default |
|--------|-------|-------------|---------|
| `--config` | `-c` | Path to configuration file | `~/.config/hypomnema/config.toml` |
| `--verbose` | `-v` | Enable verbose output | `false` |
| `--help` | `-h` | Show help | — |
| `--version` | | Show version | — |

### Commands

#### (default — no subcommand)

Start the daemon in the foreground. Reads config, opens the SQLite store, starts the watcher and the HTTP server. v0 does not bind any MCP transport in `hmnd`: the MCP surface ships as the `hmn mcp` subcommand (stdio); the deferred socket transport will land in `hmnd` in a follow-on workplan. See [ADR-0012](../decisions/0012-mcp-transport-stdio-v0.md).

Implemented in step 3; the watcher runs for the daemon's lifetime, debounces filesystem events, and updates the index in place for files whose content hash changed.

After the watcher applies an indexer outcome, the outbox writer appends a JSONL line for each real change. Tail `~/.local/share/hypomnema/outbox.jsonl` to subscribe; see [the change-events spec](../specs/change-events.md) for envelope shape.

Step 5 ships the HTTP server alongside the watcher. `/health` returns 200 OK; `/status` returns a JSON snapshot; `/search/filesystem` and `/search/content` accept POST with a JSON body. See the search specs for shapes.

**Usage**:
```
hmnd [--config PATH] [--rescan]
```

**Options**:

| Option | Description | Default |
|--------|-------------|---------|
| `--rescan` | Force a full rescan and reconciliation of the vault on startup instead of trusting the existing index | deferred (forces re-hashing every file regardless of stat; not implemented in v0). |

**Examples**:

```bash
# Foreground daemon with default config
hmnd

# Explicit config path
hmnd --config ~/etc/hypomnema/config.toml

# Force full rescan of all active vaults on startup
hmnd --rescan
```

> **Note**: the MCP-over-stdio mode lives on the CLI binary as `hmn mcp` (not `hmnd`). See the `hmn mcp` subcommand below and [ADR-0008 § Amendments](../decisions/0008-two-binary-daemon-plus-cli.md#amendments). The daemon's `mcp.transport` config knob continues to parse and validate; non-`stdio` values produce a `WARN`-level log at startup but do not crash — the deferred socket transport will live in `hmnd` when it ships (see [ADR-0012](../decisions/0012-mcp-transport-stdio-v0.md)).

> **Note**: `hmnd scan` (the v0 standalone scan subcommand) was removed in 0.2.0. Equivalent behavior is available via `hmn vault rescan [NAME|ID]` against a running daemon. See [ADR-0011](../decisions/0011-vault-management-on-hmn.md).

#### `config-validate`

Parse the configuration file, run validation rules (vault exists, `data_dir` not under `vault`, embedding dimension matches the schema, etc.), and exit with a zero status on success.

**Usage**:
```
hmnd config-validate [--config PATH]
```

### Exit Codes (daemon)

| Code | Meaning |
|------|---------|
| `0` | Success / clean shutdown |
| `1` | Unexpected runtime error |
| `2` | Invalid arguments |
| `3` | Configuration error |

---

## `hmn` — the CLI client

### Synopsis

```
hmn [global-options] <command> [command-options] [arguments]
```

`hmn` never indexes or watches directly; every subcommand reaches out to a running `hmnd` over HTTP.

### Global Options

| Option | Short | Description | Default |
|--------|-------|-------------|---------|
| `--daemon-url` | | Base URL of the running daemon | Derived from config `[http].bind` |
| `--config` | `-c` | Path to configuration file | `~/.config/hypomnema/config.toml` |
| `--verbose` | `-v` | Enable verbose output | `false` |
| `--json` | | Output JSON instead of human-formatted text | `false` |
| `--help` | `-h` | Show help | — |
| `--version` | | Show version | — |

### Commands

#### `search`

Query the running daemon. Thin CLI over the HTTP search endpoints.

**Usage**:
```
hmn search <mode> <query> [options]
```

**Arguments**:

| Argument | Description |
|----------|-------------|
| `<mode>` | One of `filesystem`, `content`, `semantic` |
| `<query>` | The query string (glob for filesystem, substring/regex for content, natural language for semantic) |

**Options**:

| Option | Description | Default |
|--------|-------------|---------|
| `--prefix PATH` | Restrict results to a vault subdirectory | — |
| `--vaults LIST` | Comma-separated names or IDs to restrict the search to | — (search all active vaults) |
| `--limit N` | Max results | 10 (semantic), 100 (filesystem, content) |

**Examples**:

```bash
hmn search filesystem "notes/databases/*.md"
hmn search content "pgvector"
hmn search semantic "how do we prevent spurious reindexes"
hmn search content "pgvector" --vaults personal,work
```

As of step 7, all three modes — `hmn search filesystem`, `hmn search content`, and `hmn search semantic` — are functional. Output is human-formatted by default; pass `--json` to render the daemon's JSON response unchanged. When `truncated == true` the text mode prints `(truncated; raise --limit)` after the results. Each filesystem/content result carries a `vault` (id) and `vault_name`; text mode prefixes results with the vault name when more than one vault contributed.

`hmn search semantic` text-mode output renders one block per result: a leading `<file_path>  (score: N.NN)` line (cosine similarity in `[0.0, 1.0]`, two decimals), the slash-joined heading path on its own indented `> …` line (omitted when every heading segment is empty), and the chunk text on a final indented line. Example:

```
notes/tools/hypomnema.md  (score: 0.82)
  > Pitfalls / Sync conflicts
  Syncthing and Dropbox write files in bursts…

notes/design/watchers.md  (score: 0.71)
  > Change detection
  mtime alone is not enough; compare content hashes…
```

When the daemon's response carries a top-level `hint` (e.g. `"semantic index is building"` — see [`docs/specs/semantic-search.md`](../specs/semantic-search.md) § Edge Cases — Empty index), the CLI prints it on its own line, parenthesized: `(semantic index is building)`. The hint appears after the result blocks if both are present; in the empty-index case the hint stands alone. When the embedding service is unreachable or returns an unexpected dimension at query time, the daemon returns HTTP 503 with envelope code `embedding_unavailable`; the CLI surfaces the message and exits non-zero.

> **Note**: Detailed cross-vault search semantics — result ordering across vaults, pagination across N independent indexes, fan-out execution model, partial-failure handling, treatment of paused/errored vaults — are deferred to the round-3 workplan. See [`docs/specs/vault-management.md` § Open Questions](../specs/vault-management.md#open-questions). The wire shapes (per-result `vault` + `vault_name`, request-side `vaults` filter) are forward-compat with any resolution.

#### `vault`

Manage vaults on a running daemon. Each subcommand maps to a control-plane HTTP route ([ADR-0010](../decisions/0010-vault-definitions-as-runtime-state.md)). Either a vault name or its surrogate ID may be passed for selectors; the daemon resolves at request entry. When a selector is omitted, the daemon resolves to `default_vault_name` from the configuration.

**Usage**:
```
hmn vault create [--name NAME] PATH
hmn vault list
hmn vault status [NAME|ID]
hmn vault pause NAME|ID
hmn vault resume NAME|ID
hmn vault reset NAME|ID
hmn vault rename [NAME|ID] --name NEW_NAME
hmn vault rescan [NAME|ID]
hmn vault terminate NAME|ID
```

**Subcommand summary**:

| Subcommand | Effect |
|---|---|
| `create [--name NAME] PATH` | Register a new vault at `PATH`. If `--name` is omitted, uses `default_vault_name`. Allocates an ID; creates the per-vault subdirectory; starts the watcher and indexer. |
| `list` | Print all registered vaults with their id, name, path, status, file count, last-indexed timestamp. |
| `status [NAME\|ID]` | Single-vault detail. With no selector, resolves to `default_vault_name`. |
| `pause NAME\|ID` | Stop the watcher and indexer for the named vault; vault remains registered. |
| `resume NAME\|ID` | Restart the watcher and indexer for a paused vault. |
| `reset NAME\|ID` | Clear `last_error`; restart the watcher and indexer. |
| `rename [NAME\|ID] --name NEW_NAME` | Single registry UPDATE; index unchanged. The surrogate ID is unchanged. |
| `rescan [NAME\|ID]` | Force a full reconciliation; emits change events as if from a cold start. Subsumes the v0 `hmnd scan` subcommand. |
| `terminate NAME\|ID` | Stop the watcher/indexer; remove the registry row; remove the per-vault subdirectory under `data_dir`. **Never touches the vault path's own files.** |

**Examples**:

```bash
# Create a default-named vault
hmn vault create ~/personal

# Create a named vault
hmn vault create --name personal ~/personal

# List all vaults
hmn vault list

# Status of one vault (by name)
hmn vault status personal

# Status by surrogate ID
hmn vault status vault_abc123

# Rename
hmn vault rename personal --name my-vault

# Pause / resume
hmn vault pause my-vault
hmn vault resume my-vault

# Force a fresh scan
hmn vault rescan my-vault

# Terminate then recreate (idempotent in practice)
hmn vault terminate my-vault
hmn vault create --name my-vault ~/my-vault
```

See [vault-management spec](../specs/vault-management.md) for the full schema, error catalog, and edge cases.

#### `status`

Report daemon health and per-vault status: is `hmnd` reachable, the list of registered vaults with each vault's path, indexed file count, last-indexed timestamp, outbox size, and active/paused/errored state.

**Usage**:
```
hmn status [--json]
```

The output shows the daemon-level info (PID, uptime, registry size) followed by a per-vault block for each registered vault. Exit code 4 if the daemon is not reachable.

#### `mcp`

Serve the MCP surface over stdio. Intended for MCP-capable agent hosts (Claude Code, Iris) that launch the binary as a child process and communicate via stdio. Translates MCP tool calls into HTTP requests against a running `hmnd`.

**Usage**: `hmn mcp [--daemon-url URL]`

**Behavior**:
- Reads MCP messages from stdin; writes MCP messages to stdout.
- All tracing/log output goes to stderr (stdout is reserved for the MCP transport).
- Tool calls are forwarded to `hmnd` over HTTP using the same `DaemonClient` machinery as `hmn search …`.
- If `hmnd` is not reachable, tool calls return a structured MCP error with `error.code = "daemon_unreachable"`.
- Process exits when stdin is closed by the parent.
- The MCP server identifies itself as `serverInfo.name = "hypomnema"`, `serverInfo.version = <crate version>` (brand-identity override per [ADR-0012](../decisions/0012-mcp-transport-stdio-v0.md)).

The three tools advertised match the three search modes one-for-one (per [ADR-0004](../decisions/0004-three-search-modes-as-peers.md)):

| Tool | Maps to | Spec |
|---|---|---|
| `search_filesystem` | `POST /search/filesystem` | [filesystem-search.md](../specs/filesystem-search.md) |
| `search_content` | `POST /search/content` | [content-search.md](../specs/content-search.md) |
| `search_semantic` | `POST /search/semantic` | [semantic-search.md](../specs/semantic-search.md) |

Tool inputs derive their JSON schemas from the same request types the HTTP API uses; tool outputs land in MCP `structured_content` as the same `*SearchResponse` shapes the HTTP API returns. HTTP error envelopes (`invalid_glob`, `invalid_regex`, `invalid_prefix`, `invalid_request`, `embedding_unavailable`, `internal`) flow through unchanged as MCP `structured_error`. The `daemon_unreachable` code is new at the MCP layer for the case where the daemon isn't running.

**Example MCP host configuration**:

```json
{
  "mcpServers": {
    "hypomnema": {
      "command": "/path/to/hmn",
      "args": ["mcp"]
    }
  }
}
```

### Exit Codes (client)

| Code | Meaning |
|------|---------|
| `0` | Success |
| `1` | General error |
| `2` | Invalid arguments |
| `3` | Configuration error |
| `4` | Daemon not reachable |
| `5` | Vault not found / not in expected state |

---

## Environment Variables

| Variable | Used by | Description | Equivalent Option |
|----------|---------|-------------|-------------------|
| `HYPOMNEMA_CONFIG` | both | Path to config file | `--config` |
| `HYPOMNEMA_DATA_DIR` | `hmnd` | Override the daemon data directory | (no flag; config file only, TBD) |
| `HYPOMNEMA_DAEMON_URL` | `hmn` | Base URL of the daemon (overrides `[http].bind`) | `--daemon-url` |

---

## See Also

- [Configuration Reference](./configuration.md)
- [ADR-0008: Two Binaries (hmnd + hmn) in One Crate](../decisions/0008-two-binary-daemon-plus-cli.md)
- [ADR-0009: Multi-Vault per Daemon](../decisions/0009-multi-vault-per-daemon.md)
- [ADR-0010: Vault Definitions Are Runtime State, Not Configuration](../decisions/0010-vault-definitions-as-runtime-state.md)
- [ADR-0011: Vault Management Lives on `hmn`](../decisions/0011-vault-management-on-hmn.md)
- [Specifications](../specs/) — per-search-mode semantics; [vault-management spec](../specs/vault-management.md)
- [Architecture: Search API](../architecture/overview.md#search-api)

---

## Notes

- Several options marked TBD above are open questions the handoff calls out explicitly; this file is expected to stabilize over steps 1–8 of the v0 plan (see [implementation/tech-stack.md](../implementation/tech-stack.md)).
- The MCP-over-stdio surface ships in step 8 as the `hmn mcp` subcommand on the CLI binary (not `hmnd --mcp-stdio` as earlier drafts of this file suggested). The Unix-socket MCP transport is deferred to a follow-on workplan and will live in `hmnd` when it ships. See [ADR-0008 § Amendments](../decisions/0008-two-binary-daemon-plus-cli.md#amendments) and [ADR-0012](../decisions/0012-mcp-transport-stdio-v0.md).
