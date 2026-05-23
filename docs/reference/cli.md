# CLI Reference: `hmnd` and `hmn`

**Version**: 0.4.0
**Generated**: 2026-04-30

---

> **Status**: Subcommand surface pinned in step 1 — see [step-01 workplan § CLI subcommand naming](../roadmap/step-01-workplan.md#1-cli-subcommand-naming). Hypomnema ships two binaries per [ADR-0008](../decisions/0008-two-binary-daemon-plus-cli.md): the daemon (`hmnd`) and the CLI client (`hmn`). Vault management lives on `hmn vault …` per [ADR-0011](../decisions/0011-vault-management-on-hmn.md).

---

Hypomnema ships two binaries:

- **`hmnd`** — the daemon. Owns the watched directory, the SQLite store, the HTTP server, and the HTTP-MCP route at `/mcp`. Long-running; typically managed by systemd or an equivalent supervisor. (The deferred Unix-socket MCP transport will also live in `hmnd` when it ships — see [ADR-0012](../decisions/0012-mcp-transport-stdio-v0.md).)
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

Start the daemon in the foreground. Reads config, opens the SQLite store, starts the watcher and the HTTP server, and mounts Streamable HTTP MCP at `/mcp` when enabled. Stdio MCP ships as the `hmn mcp` subcommand; the deferred socket transport will land in `hmnd` if a future workplan ships it. See [ADR-0012](../decisions/0012-mcp-transport-stdio-v0.md) and [ADR-0013](../decisions/0013-mcp-transport-streamable-http.md).

The watcher runs for the daemon's lifetime, debounces filesystem events, and updates the index in place for files whose content hash changed.

After the watcher applies an indexer outcome, the daemon publishes a live change event for each real indexed change. Subscribe with `hmn vault watch` or HTTP streaming; MCP `vault_watch` is deferred pending streaming support. See [the change-events spec](../specs/change-events.md) for envelope shape and live-only semantics.

The HTTP server runs alongside the watcher. `/health` returns 200 OK; `/status` returns a JSON snapshot; `/search/filesystem` and `/search/content` accept POST with a JSON body. See the search specs for shapes.

**Usage**:
```
hmnd [--config PATH]
```

**Options**:

| Option | Description | Default |
|--------|-------------|---------|
| `--config PATH` | Path to configuration file | platform config default |

**Examples**:

```bash
# Foreground daemon with default config
hmnd

# Explicit config path
hmnd --config ~/etc/hypomnema/config.toml
```

> **Note**: the MCP-over-stdio mode lives on the CLI binary as `hmn mcp` (not `hmnd`). Streamable HTTP MCP lives on `hmnd` at `/mcp`; the deferred Unix-socket transport will also live in `hmnd` when it ships. See the `hmn mcp` subcommand below, [ADR-0008 § Amendments](../decisions/0008-two-binary-daemon-plus-cli.md#amendments), [ADR-0012](../decisions/0012-mcp-transport-stdio-v0.md), and [ADR-0013](../decisions/0013-mcp-transport-streamable-http.md).

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
| `--include-matches` | `content` mode only: include per-line match snippets in each result | off (omitted → no `matches`) |
| `--max-matches-per-file N` | `content` mode only: cap snippets per file when `--include-matches` is set | 5 |
| `--granularity GRANULARITY` | Result granularity for `semantic` mode: `document` or `chunk` | `document` (daemon default; configurable via `[search.semantic]`) |
| `--chunks-per-document N` | Max evidence chunks per document result in `document` mode (1..=100) | 3 (daemon default; configurable via `[search.semantic]`) |

**Examples**:

```bash
hmn search filesystem "notes/databases/*.md"
hmn search content "pgvector"
hmn search semantic "how do we prevent spurious reindexes"
hmn search semantic "sync conflicts" --granularity document --chunks-per-document 5
hmn search semantic "exact passage about debouncing" --granularity chunk
hmn search content "pgvector" --vaults personal,work
hmn search content "pgvector" --vaults personal --vaults work   # repeating works too
```

As of step 7, all three modes — `hmn search filesystem`, `hmn search content`, and `hmn search semantic` — are functional. Output is human-formatted by default; pass `--json` to render the daemon's JSON response unchanged. When `truncated == true` the text mode prints `(truncated; raise --limit)` after the results. Each filesystem/content result carries a `vault` (id) and `vault_name`; text mode prefixes results with the vault name when more than one vault contributed.

**Content snippets are opt-in (parity with MCP)**: `hmn search content` does not request per-line match snippets unless you pass `--include-matches`. This makes `hmn search content "q" --json` send the same logical request as a default MCP/HTTP `search_content` call (`include_matches` defaults to `false` on the wire), so both return `match_count` without a populated `matches` array until you opt in. Pass `--include-matches` (optionally with `--max-matches-per-file N`, default 5) to receive snippet lines in both human and `--json` output. Ranked mode (`--mode ranked`) renders rank/score and does not use snippets.

**Canonical path field**: across all search and content-retrieval JSON responses — filesystem, content, semantic (chunk and document), and `content_get` — the vault-relative file path is named `path`. The `content_get` request takes `paths` (a list). This rule holds identically over CLI `--json`, the HTTP API, and stdio/HTTP MCP, since all surfaces serialize the same response types.

**`--vaults` semantics** (step 10): values are matched against vault names first, then surrogate IDs. Unknown values do **not** fail the request — they appear in the response's `partial_results.failed` array with `code: "vault_not_found"` and the search proceeds against the recognized subset. Passing `--vaults` with an empty value list (e.g. `--vaults ""`) is rejected as `invalid_request`. Omitting `--vaults` queries every active vault. Paused or errored vaults that fall in scope are reported in `partial_results.skipped` with their current `status` and `reason` (the registry's `last_error` for `errored`); see [`docs/specs/vault-management.md` § Cross-Vault Search Semantics](../specs/vault-management.md#cross-vault-search-semantics).

**`hmn search semantic` — document granularity (default, step 25)**: results are grouped by parent file. Text mode renders one block per document: a leading `<path>  (score: N.NN)` line, then each evidence chunk indented below it with its own heading path and text. Example:

```
notes/tools/hypomnema.md  (score: 0.82)
  > Pitfalls / Sync conflicts
  Syncthing and Dropbox write files in bursts…
  > Background
  The daemon debounces events from notify…

notes/design/watchers.md  (score: 0.68)
  > Change detection
  mtime alone is not enough; compare content hashes…
```

**`hmn search semantic` — chunk granularity**: text mode renders one block per chunk (pre-step-25 behavior): a leading `<path>  (score: N.NN)` line, the heading path, and the chunk text. Example:

```
notes/tools/hypomnema.md  (score: 0.82)
  > Pitfalls / Sync conflicts
  Syncthing and Dropbox write files in bursts…

notes/design/watchers.md  (score: 0.71)
  > Change detection
  mtime alone is not enough; compare content hashes…
```

When the daemon's response carries a top-level `hint` (e.g. `"semantic index is building"` — see [`docs/specs/semantic-search.md`](../specs/semantic-search.md) § Edge Cases — Empty index), the CLI prints it on its own line, parenthesized: `(semantic index is building)`. The hint appears after the result blocks if both are present; in the empty-index case the hint stands alone. When the embedding service is unreachable or returns an unexpected dimension at query time, the daemon returns HTTP 503 with envelope code `embedding_unavailable`; the CLI surfaces the message and exits non-zero.

#### `debug chunks`

Inspect how one indexed Markdown file is chunked for semantic search. The work runs in `hmnd`; `hmn` only routes the request and renders the response.

**Usage**:
```
hmn debug chunks PATH [--vault NAME|ID] [--mode indexed|preview|diff] [--show-text preview|full|none]
```

`indexed` (default) shows the chunks currently stored in SQLite. `preview` also re-runs the daemon's current chunker over the indexed file content. `diff` returns both views plus changed/added/removed chunk indexes. Text output includes byte ranges, heading paths, boundary reasons, fenced-code counts/bytes/languages, thematic-break diagnostics, and warnings such as `code-heavy chunk`. Pass `--json` to receive the daemon response unchanged.

#### `vault`

Manage vaults on a running daemon. Each subcommand maps to a control-plane HTTP route ([ADR-0010](../decisions/0010-vault-definitions-as-runtime-state.md)). Either a vault name or its surrogate ID may be passed for selectors; the daemon resolves at request entry. When a selector is omitted, `status` resolves to `default_vault_name` from the configuration ([§ `default_vault_name`](./configuration.md#default_vault_name)).

As of step 11, the full nine-operation lifecycle ships: read + create/terminate (`create`, `list`, `status`, `terminate`) plus the five lifecycle ops (`pause`, `resume`, `reset`, `rename`, `rescan`). Hypomnema also exposes `watch` as a live-only event subscription over CLI/HTTP. The full surface is pinned in [`docs/specs/vault-management.md`](../specs/vault-management.md).

**Usage**:
```
hmn vault create [--name NAME] PATH
hmn vault list
hmn vault status [NAME|ID]
hmn vault pause NAME|ID
hmn vault resume NAME|ID
hmn vault reset NAME|ID [--rebuild] [--yes]
hmn vault rename NAME|ID --new-name NEW_NAME
hmn vault rescan NAME|ID [--yes]
hmn vault watch [NAME|ID] [--all]
hmn vault terminate NAME|ID [--yes]
```

**Subcommand summary**:

| Subcommand | Effect |
|---|---|
| `create [--name NAME] PATH` | Register a new vault at `PATH`. If `--name` is omitted, uses `default_vault_name` from the daemon's config. Allocates a UUIDv7 surrogate ID; creates the per-vault subdirectory under `<data_dir>/vaults/<id>/`; starts the watcher and indexer. Errors: `409 vault_path_conflict` (path already registered), `409 vault_name_conflict` (name already in use), `422 vault_path_invalid` (path does not exist, is not a directory, or would place `data_dir` inside the vault). |
| `list` | Print every registered vault as a table (`ID  NAME  STATUS  CREATED  PATH`). With `--json`, emits the underlying `{"vaults": [...]}` shape verbatim — see [`docs/specs/vault-management.md` § Control-Plane HTTP Wire Shapes](../specs/vault-management.md#control-plane-http-wire-shapes) for the row fields. |
| `status [NAME\|ID]` | Single-vault detail printed as a labelled key/value block (id, name, path, status, created_at, optional last_error). With no selector, resolves to `default_vault_name`. With `--json`, emits the `VaultRow` shape verbatim. Errors: `404 vault_not_found` (response carries a closest-name `hint` when the registry has a near match within Levenshtein distance 3). |
| `pause NAME\|ID` | Transition `active → paused`. Drains the watcher + indexer (cooperative, 30s drain cap), preserves `index.sqlite` in place. Search responses report the vault under `partial_results.skipped` while paused. Idempotent on already-paused (returns the existing row). Errors: `404 vault_not_found`. |
| `resume NAME\|ID` | Transition `paused → active`, or `errored → active` when the underlying error is no longer present (e.g. the vault path is reachable again). Re-spawns the watcher + indexer; clears `last_error` on success. Idempotent on already-active. Errors: `404 vault_not_found`, `503 vault_errored` (when `errored → active` is attempted but the path is still inaccessible — `last_error` is updated rather than cleared). |
| `reset NAME\|ID [--rebuild] [--yes]` | Clear `last_error`, drain and re-spawn the watcher + indexer. With `--rebuild`, additionally drop the `chunks` + `chunks_vec` tables and clear every `files.content_hash` in the per-vault `index.sqlite` — the next indexing pass re-embeds every file. The `files` rows are preserved across `--rebuild`. `--yes` skips the rebuild confirmation prompt; required for non-interactive `--rebuild` runs. Plain `reset` (no `--rebuild`) is non-destructive and skips the prompt. Errors: `404 vault_not_found`. |
| `rename NAME\|ID --new-name NEW_NAME` | Single registry UPDATE on the row's `name` column; rewrites the per-vault `meta.toml`. The surrogate ID and the per-vault subdirectory path are unchanged; live events continue to carry the same `vault` ID. Subsequent search responses carry the new `vault_name`. `NEW_NAME` must match `[A-Za-z0-9_-]+` and must not already be in use. Errors: `404 vault_not_found`, `409 vault_name_conflict`, `422 vault_path_invalid` (when the new name fails the regex). |
| `rescan NAME\|ID [--yes]` | Force a full directory walk for the vault: every file is re-stat'd and re-hashed; live `modified` events are emitted only for files whose `content_hash` differs from the stored value (a fully-indexed vault with stable content emits few or zero events). For cold-start emission against every file regardless of hash, use `reset --rebuild` (which clears every `files.content_hash` first, so the subsequent indexing pass treats every file as newly seen). The HTTP response carries `rescan_initiated_at`; the rescan itself runs asynchronously. `--yes` skips the confirmation prompt. Rescan on a paused or errored vault is a silent no-op (returns the row unchanged; no signal sent). Errors: `404 vault_not_found`. |
| `watch [NAME\|ID] [--all]` | Subscribe to live change events. With a selector, watches one vault; with no selector, resolves to `default_vault_name`; with `--all`, watches all active vaults. Output is newline-delimited JSON event envelopes. The stream is live-only: disconnects, daemon restarts, or lag require the consumer to re-query the index for current state. Errors: `404 vault_not_found`. |
| `terminate NAME\|ID [--yes]` | Stop the watcher/indexer; remove the registry row; remove the per-vault subdirectory under `<data_dir>/vaults/<id>/`. **Never touches the vault path's own files.** Without `--yes`, prompts on stderr (`Terminate vault 'NAME'? (y/N)`) and reads from stdin; any answer that does not begin with `y`/`Y` aborts. With `--json` and an aborted prompt, emits `{"terminated": false, "aborted": true}` to stdout. With `--yes`, skips the prompt. Successful termination emits `{"terminated": true, "id": "<uuid>"}`. Terminate-then-create with the same name succeeds (the new vault gets a fresh UUID). |

**Global options that apply** (from § Global Options above): `--daemon-url`, `--config`, `--json`.

**Destructive-op confirmation prompts**. Three subcommands prompt on stderr unless `--yes` is passed:

- `terminate` — `Terminate vault 'NAME'? (y/N)`
- `reset --rebuild` — `Reset vault 'NAME' and rebuild chunks? (y/N)` (only the `--rebuild` form prompts; plain `reset` is non-destructive and skips the prompt)
- `rescan` — `Rescan vault 'NAME'? This will re-emit live change events for changed files. (y/N)`

In all three cases, an answer that does not begin with `y`/`Y` aborts; with `--json` an aborted prompt emits the operation-specific aborted envelope (`{"terminated": false, "aborted": true}` / `{"reset": false, "aborted": true}` / `{"rescan": false, "aborted": true}`) to stdout. Concurrent destructive ops on the same vault serialize at the daemon (per-vault `op_lock`); operations on different vaults run in parallel.

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

# Pause / resume
hmn vault pause personal
hmn vault resume personal

# Reset (clears last_error; re-spawns watcher + indexer)
hmn vault reset personal

# Reset with full re-embed (drops chunks + chunks_vec; clears content_hash)
hmn vault reset personal --rebuild --yes

# Rename (registry UPDATE only; surrogate ID + subdirectory unchanged)
hmn vault rename personal --new-name notes

# Rescan (force a full walk; emits live modified events for files whose content_hash drifted)
hmn vault rescan notes --yes

# Watch live changes as NDJSON event envelopes
hmn vault watch notes

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

Report daemon health and per-vault status: is `hmnd` reachable, the list of registered vaults with each vault's path, indexed file count, last-indexed timestamp, and active/paused/errored state.

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

Step 11 advertises twelve request/response tools — the three search modes (per [ADR-0004](../decisions/0004-three-search-modes-as-peers.md)) plus the nine vault control-plane operations. The live change stream adds `vault_watch` as a read-only long-lived control-plane operation pending rmcp streaming verification (Task 16.6); if no clean MCP framing is found, `vault_watch` remains absent from the MCP tool surface and this table will be updated accordingly:

| Tool | Maps to | Trust posture | Spec |
|---|---|---|---|
| `search_filesystem` | `POST /search/filesystem` | Read | [filesystem-search.md](../specs/filesystem-search.md) |
| `search_content` | `POST /search/content` | Read | [content-search.md](../specs/content-search.md) |
| `search_semantic` | `POST /search/semantic` | Read | [semantic-search.md](../specs/semantic-search.md) |
| `vault_list` | `GET /vaults` | Read | [vault-management.md](../specs/vault-management.md) |
| `vault_status` | `GET /vaults/{id_or_name}` | Read | [vault-management.md](../specs/vault-management.md) |
| `vault_create` | `POST /vaults` | Write (gated) | [vault-management.md](../specs/vault-management.md) |
| `vault_pause` | `POST /vaults/{id_or_name}/pause` | Write (gated) | [vault-management.md](../specs/vault-management.md) |
| `vault_resume` | `POST /vaults/{id_or_name}/resume` | Write (gated) | [vault-management.md](../specs/vault-management.md) |
| `vault_reset` | `POST /vaults/{id_or_name}/reset` | Write (gated) | [vault-management.md](../specs/vault-management.md) |
| `vault_rename` | `POST /vaults/{id_or_name}/rename` | Write (gated) | [vault-management.md](../specs/vault-management.md) |
| `vault_rescan` | `POST /vaults/{id_or_name}/rescan` | Write (gated) | [vault-management.md](../specs/vault-management.md) |
| `vault_terminate` | `DELETE /vaults/{id_or_name}` | Write (gated) | [vault-management.md](../specs/vault-management.md) |
| `vault_watch` | live event stream (deferred — pending Task 16.6 rmcp verification) | Read | [change-events.md](../specs/change-events.md) |

Tool inputs derive their JSON schemas from the same request types the HTTP API uses; tool outputs land in MCP `structured_content` as the same `*SearchResponse` / `VaultRow` / `VaultListResponse` / `RescanResponseJson` / `TerminateVaultResponse` shapes the HTTP API returns. HTTP error envelopes (`invalid_glob`, `invalid_regex`, `invalid_prefix`, `invalid_request`, `embedding_unavailable`, `vault_not_found`, `vault_path_conflict`, `vault_name_conflict`, `vault_path_invalid`, `vault_errored`, `internal`) flow through unchanged as MCP `structured_error`. The `daemon_unreachable` code is new at the MCP layer for the case where the daemon isn't running.

`vault_status` accepts an optional `target` field (vault name or surrogate ID); when omitted, the tool resolves to `default_vault_name` from the daemon's config — matching the CLI's `hmn vault status` ergonomics. `vault_create` mirrors `hmn vault create` (an optional `name` defaulting to `default_vault_name`, a required `path`). The five lifecycle tools (`vault_pause` / `vault_resume` / `vault_reset` / `vault_rename` / `vault_rescan`) each take a required `target` field; `vault_reset` accepts an optional `rebuild` boolean (default `false`); `vault_rename` requires `new_name`. Idempotency guarantees match the HTTP layer (pause-on-paused / resume-on-active return the existing row). `vault_watch` is deferred pending Task 16.6 rmcp streaming verification; if it ships, it will accept `target?: string` or `all?: bool` and stream live events only with no `since` argument unless durable replay is designed.

**Write-tool gating**. The seven write tools (`vault_create`, `vault_pause`, `vault_resume`, `vault_reset`, `vault_rename`, `vault_rescan`, `vault_terminate`) are advertised by default and may be disabled via `[mcp] enable_write_tools = false` in the daemon's config (see [configuration.md § `[mcp]`](./configuration.md#mcp)). When the gate is closed, `tools/call` against any of the seven returns a structured `write_tools_disabled` error envelope naming the gated tool and the config knob to flip; the read tools remain available. The gate is single-flag at the daemon level — per-tool gating is round-4+ if a use-case surfaces. See [`docs/specs/vault-management.md` § MCP Tool Surface](../specs/vault-management.md#mcp-tool-surface).

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

- Several options marked TBD above are historical open questions from the handoff and should be refreshed when that surface is touched.
- The MCP-over-stdio surface ships as the `hmn mcp` subcommand on the CLI binary (not `hmnd --mcp-stdio` as earlier drafts of this file suggested). The Unix-socket MCP transport is deferred to a follow-on workplan and will live in `hmnd` when it ships. See [ADR-0008 § Amendments](../decisions/0008-two-binary-daemon-plus-cli.md#amendments) and [ADR-0012](../decisions/0012-mcp-transport-stdio-v0.md).
