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
hmn search content "pgvector" --vaults personal --vaults work   # repeating works too
```

As of step 7, all three modes — `hmn search filesystem`, `hmn search content`, and `hmn search semantic` — are functional. Output is human-formatted by default; pass `--json` to render the daemon's JSON response unchanged. When `truncated == true` the text mode prints `(truncated; raise --limit)` after the results. Each filesystem/content result carries a `vault` (id) and `vault_name`; text mode prefixes results with the vault name when more than one vault contributed.

**`--vaults` semantics** (step 10): values are matched against vault names first, then surrogate IDs. Unknown values do **not** fail the request — they appear in the response's `partial_results.failed` array with `code: "vault_not_found"` and the search proceeds against the recognized subset. Passing `--vaults` with an empty value list (e.g. `--vaults ""`) is rejected as `invalid_request`. Omitting `--vaults` queries every active vault. Paused or errored vaults that fall in scope are reported in `partial_results.skipped` with their current `status` and `reason` (the registry's `last_error` for `errored`); see [`docs/specs/vault-management.md` § Cross-Vault Search Semantics](../specs/vault-management.md#cross-vault-search-semantics).

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

#### `vault`

Manage vaults on a running daemon. Each subcommand maps to a control-plane HTTP route ([ADR-0010](../decisions/0010-vault-definitions-as-runtime-state.md)). Either a vault name or its surrogate ID may be passed for selectors; the daemon resolves at request entry. When a selector is omitted, `status` resolves to `default_vault_name` from the configuration ([§ `default_vault_name`](./configuration.md#default_vault_name)).

Step 10 ships the read + create/terminate subset (`create`, `list`, `status`, `terminate`). The remaining lifecycle ops (`pause`, `resume`, `reset`, `rename`, `rescan`) are pinned in [`docs/specs/vault-management.md`](../specs/vault-management.md) and ship in step 11.

**Usage**:
```
hmn vault create [--name NAME] PATH
hmn vault list
hmn vault status [NAME|ID]
hmn vault terminate NAME|ID [--yes]
```

**Subcommand summary**:

| Subcommand | Effect |
|---|---|
| `create [--name NAME] PATH` | Register a new vault at `PATH`. If `--name` is omitted, uses `default_vault_name` from the daemon's config. Allocates a UUIDv7 surrogate ID; creates the per-vault subdirectory under `<data_dir>/vaults/<id>/`; starts the watcher and indexer. Errors: `409 vault_path_conflict` (path already registered), `409 vault_name_conflict` (name already in use), `422 vault_path_invalid` (path does not exist, is not a directory, or would place `data_dir` inside the vault). |
| `list` | Print every registered vault as a table (`ID  NAME  STATUS  CREATED  PATH`). With `--json`, emits the underlying `{"vaults": [...]}` shape verbatim — see [`docs/specs/vault-management.md` § Control-Plane HTTP Wire Shapes](../specs/vault-management.md#control-plane-http-wire-shapes) for the row fields shipped in step 10. |
| `status [NAME\|ID]` | Single-vault detail printed as a labelled key/value block (id, name, path, status, created_at, optional last_error). With no selector, resolves to `default_vault_name`. With `--json`, emits the `VaultRow` shape verbatim. Errors: `404 vault_not_found` (response carries a closest-name `hint` when the registry has a near match within Levenshtein distance 3). |
| `terminate NAME\|ID [--yes]` | Stop the watcher/indexer; remove the registry row; remove the per-vault subdirectory under `<data_dir>/vaults/<id>/`. **Never touches the vault path's own files.** Without `--yes`, prompts on stderr (`Terminate vault 'NAME'? (y/N)`) and reads from stdin; any answer that does not begin with `y`/`Y` aborts. With `--json` and an aborted prompt, emits `{"terminated": false, "aborted": true}` to stdout. With `--yes`, skips the prompt. Successful termination emits `{"terminated": true, "id": "<uuid>"}`. Terminate-then-create with the same name succeeds (the new vault gets a fresh UUID). |

**Global options that apply** (from § Global Options above): `--daemon-url`, `--config`, `--json`.

**Examples**:

```bash
# Create a default-named vault (uses default_vault_name from config)
hmn vault create ~/personal

# Create a named vault
hmn vault create --name personal ~/personal

# List all vaults (table view)
hmn vault list

# List as JSON
hmn vault list --json

# Status of one vault (by name)
hmn vault status personal

# Status of the default vault (selector omitted)
hmn vault status

# Status by surrogate ID
hmn vault status 019dd258-3992-7c3b-7a2e-8c1d1a2b3c4d

# Terminate with explicit confirmation
hmn vault terminate personal
# Terminate vault 'personal'? (y/N) y
# terminated: true
# id:         019dd258-3992-7c3b-7a2e-8c1d1a2b3c4d

# Terminate non-interactively (e.g. in scripts)
hmn vault terminate personal --yes

# Terminate then recreate the same name (fresh ID)
hmn vault terminate personal --yes
hmn vault create --name personal ~/personal
```

See [vault-management spec](../specs/vault-management.md) for the full schema, error catalog, and edge cases. The HTTP control-plane routes that back these subcommands are documented in [§ Architecture Overview](../architecture/overview.md#vault-manager--control-plane); the MCP tool surface is documented under [`hmn mcp`](#mcp) below.

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

Step 10 advertises seven tools — the three search modes (per [ADR-0004](../decisions/0004-three-search-modes-as-peers.md)) plus the four vault control-plane operations that ship in step 10:

| Tool | Maps to | Trust posture | Spec |
|---|---|---|---|
| `search_filesystem` | `POST /search/filesystem` | Read | [filesystem-search.md](../specs/filesystem-search.md) |
| `search_content` | `POST /search/content` | Read | [content-search.md](../specs/content-search.md) |
| `search_semantic` | `POST /search/semantic` | Read | [semantic-search.md](../specs/semantic-search.md) |
| `vault_list` | `GET /vaults` | Read | [vault-management.md](../specs/vault-management.md) |
| `vault_status` | `GET /vaults/{id_or_name}` | Read | [vault-management.md](../specs/vault-management.md) |
| `vault_create` | `POST /vaults` | Write (gated) | [vault-management.md](../specs/vault-management.md) |
| `vault_terminate` | `DELETE /vaults/{id_or_name}` | Write (gated) | [vault-management.md](../specs/vault-management.md) |

Tool inputs derive their JSON schemas from the same request types the HTTP API uses; tool outputs land in MCP `structured_content` as the same `*SearchResponse` / `VaultRow` / `VaultListResponse` / `TerminateVaultResponse` shapes the HTTP API returns. HTTP error envelopes (`invalid_glob`, `invalid_regex`, `invalid_prefix`, `invalid_request`, `embedding_unavailable`, `vault_not_found`, `vault_path_conflict`, `vault_name_conflict`, `vault_path_invalid`, `internal`) flow through unchanged as MCP `structured_error`. The `daemon_unreachable` code is new at the MCP layer for the case where the daemon isn't running.

`vault_status` accepts an optional `target` field (vault name or surrogate ID); when omitted, the tool resolves to `default_vault_name` from the daemon's config — matching the CLI's `hmn vault status` ergonomics. `vault_create` mirrors `hmn vault create` (an optional `name` defaulting to `default_vault_name`, a required `path`).

**Write-tool gating**. The two write tools (`vault_create`, `vault_terminate`) are advertised by default and may be disabled via `[mcp] enable_write_tools = false` in the daemon's config (see [configuration.md § `[mcp]`](./configuration.md#mcp)). When the gate is closed, `tools/call` against either tool returns a structured `write_tools_disabled` error envelope naming the gated tool and the config knob to flip; the read tools remain available. Step-11 lifecycle ops (`vault_pause` / `vault_resume` / `vault_reset` / `vault_rename` / `vault_rescan`) inherit the same gate when they ship — see [`docs/specs/vault-management.md` § MCP Tool Surface](../specs/vault-management.md#mcp-tool-surface).

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
