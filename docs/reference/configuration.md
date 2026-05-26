# Hypomnema Configuration Reference

**Version**: 0.4.0
**Generated**: 2026-04-30

---

> **Status**: Schema pinned in step 1 — see [step-01 workplan § TOML config schema](../roadmap/step-01-workplan.md#2-toml-config-schema). Format is TOML; default location is `~/.config/hypomnema/config.toml` (respects `XDG_CONFIG_HOME`). Vaults are runtime state managed via the control-plane API ([ADR-0010](../decisions/0010-vault-definitions-as-runtime-state.md)) — they are not declared in `config.toml`. The top-level `[vault]` block from v0.1.0 is **deprecated** as of 0.3.0 (round 3, step 9 — the per-vault internal refactor). The daemon continues to parse the block, logs a deprecation `WARN` on startups where the legacy migration could engage (i.e., the registry is empty), and translates it into a `vaults.sqlite` row on the first run that finds legacy v0 state. See [Legacy `[vault]` block migration](#legacy-vault-block-migration) below.

> **Scope**: Every option on this page is daemon-side — it affects the behavior of `hmnd`. The CLI client (`hmn`) reads only the daemon URL (derived from `[http].bind`) to know where to send requests; override on the client with `--daemon-url` or `HYPOMNEMA_DAEMON_URL`. See [cli.md](./cli.md) and [ADR-0008](../decisions/0008-two-binary-daemon-plus-cli.md).

---

## Prerequisites

- A C toolchain capable of building Rust crates (used by the bundled `sqlite-vec` build). Provided out-of-the-box by most platforms (Xcode Command Line Tools on macOS, `build-essential` on Debian/Ubuntu, the GitHub Actions `ubuntu-latest` and `macos-latest` runners). No separate sqlite-vec extension file is required at runtime — it is statically linked into `hmnd`.

---

## Configuration Files

| File | Purpose | Location |
|------|---------|----------|
| `config.toml` | Main daemon configuration | `~/.config/hypomnema/config.toml` (Linux/macOS); `%APPDATA%\hypomnema\config.toml` (Windows). Respects `XDG_CONFIG_HOME`. |

**Precedence** (highest to lowest):
1. Command-line flags (`--config`, `--rescan`, etc.)
2. Environment variables (`HYPOMNEMA_*`)
3. Configuration file
4. Built-in defaults

---

## Configuration Schema

### Top-Level Structure

```toml
# When a vault command omits a selector, resolve to this name. Set to ""
# to require explicit names on every command. Default "default".
default_vault_name = "default"

# HTTP server binding (defaults to localhost:7777; port TBD)
[http]
bind = "127.0.0.1:7777"

# MCP transport (stdio for agent embedding, socket for detached)
[mcp]
transport = "stdio"   # or "socket"
socket = "~/.local/share/hypomnema/mcp.sock"  # only if transport = "socket"
enable_write_tools = true   # set false to deny all seven vault write tools (create / pause / resume / reset / rename / rescan / terminate)

# HTTP-MCP transport (Streamable HTTP MCP on hmnd's Axum router; round 4)
[mcp.http]
enabled = true        # set false to disable HTTP-MCP without disabling stdio MCP
path = "/mcp"         # reserved-for-forward-compat; any other value rejected at startup

# Embedding service (OpenAI-compatible)
[embedding]
endpoint = "http://127.0.0.1:8080/v1/embeddings"
# The exact model identifier is the string TEI's /v1/embeddings endpoint expects.
model = "nomic-embed-text-v1.5"
dimension = 768
api_key = ""   # empty for local services that don't require one
timeout_ms = 30000   # per-request embed timeout
max_retries = 1      # retry once on transport error or HTTP 5xx; 250ms backoff
batch_size = 16      # max chunks per embed request; shrinks adaptively if rejected

# Watcher tuning
# The watcher only considers .md files; ignore_patterns further excludes matches within that set.
[watcher]
debounce_ms = 500
ignore_patterns = [
  ".git/**",
  ".obsidian/**",
  ".trash/**",
  "*.sync-conflict-*",
  "**/*.tmp",
]

# Storage locations (defaults shown)
[storage]
data_dir = "~/.local/share/hypomnema"      # index + registry + logs

# Logging
[logging]
level = "info"
notify_level = "warn"
tokio_level = "error"
```

---

## `default_vault_name`

When a control-plane command (e.g., `hmn vault status`) omits the vault selector, the daemon resolves to this name. On a fresh install with no legacy v0 state, the daemon does **not** auto-create a default vault on first run; the user must run `hmn vault create` (which uses `default_vault_name` if `--name` is omitted). On first startup against legacy v0 state — a populated top-level `[vault]` block paired with a pre-existing `<data_dir>/index.sqlite` — the daemon auto-creates one `vaults.sqlite` row using `default_vault_name` for the row's `name` column; see [Legacy `[vault]` block migration](#legacy-vault-block-migration) below.

| Option | Type | Required | Default | Description |
|--------|------|----------|---------|-------------|
| `default_vault_name` | string | no | `"default"` | Name resolved when a command omits the selector. Must be non-empty after trimming whitespace; the daemon fails at startup with a configuration error if empty. |

See [ADR-0010](../decisions/0010-vault-definitions-as-runtime-state.md) for why vaults live in runtime state rather than configuration.

---

## Legacy `[vault]` block migration

The top-level `[vault]` block from v0.1.0 is **deprecated** as of 0.3.0 (round 3, step 9 — the per-vault internal refactor). Vaults are now managed via `vaults.sqlite` ([ADR-0010](../decisions/0010-vault-definitions-as-runtime-state.md)); the daemon continues to parse the legacy block to ease upgrades from v0.1.0.

```toml
# Deprecated; parsed for backwards-compatibility only.
[vault]
path = "~/notes"
```

**Deprecation `WARN`**. When a `[vault]` block is present at startup AND the registry is currently empty, the daemon logs the following at `WARN` level (visible until the operator removes the block; subsequent starts with a populated registry log nothing about the deprecated block):

```
top-level [vault] config block is deprecated; vaults are now managed via
vaults.sqlite. Remove the [vault] block from your config when convenient
(it will continue to be honoured for first-run auto-migration only).
```

**Migration trigger**. The legacy-state migration runs at startup when **both** of these hold:

1. `<data_dir>/vaults.sqlite` is empty (no rows registered).
2. A top-level `[vault]` block is present in the config file.

When triggered, the daemon:

- Inserts one row into `vaults.sqlite` with name set to `default_vault_name`, the legacy `[vault].path` (canonicalized), status `active` if the path is accessible (or `errored` with a recorded `last_error` if not), and a fresh UUIDv7 ID.
- Atomically renames the legacy index files — `index.sqlite`, `index.sqlite-wal` (if present), and `index.sqlite-shm` (if present) — from `<data_dir>/` into `<data_dir>/vaults/<vault_id>/`. Any legacy `outbox.jsonl` is left in place and ignored; the JSONL outbox has been removed as the public event contract.
- Writes `<data_dir>/vaults/<vault_id>/meta.toml` (a human-readable mirror of the registry row, per ADR-0010).
- Starts the watcher and indexer for the migrated vault (when status is `active`).

The migration is **idempotent and crash-safe**. Each rename is per-file atomic on POSIX; if the daemon is killed mid-rename, the next startup retries the move on any file still at the legacy location and treats files already at the per-vault location as already-migrated. Subsequent startups (with `vaults.sqlite` populated) skip the migration entirely; the `[vault]` block, if still present, is ignored beyond the deprecation `WARN` (which itself only logs when the registry is empty — a populated registry suppresses the warning).

**Same-filesystem assumption**. The four-file rename uses `std::fs::rename`, which is atomic per-file on POSIX **only when source and destination are on the same filesystem**. In practice this is `<data_dir>/index.sqlite` → `<data_dir>/vaults/<id>/index.sqlite` — both under `<data_dir>` — which holds on every standard install. Cross-mount setups, where `<data_dir>/vaults/` lives on a separate mount from `<data_dir>/` itself, would error during the rename pass; the daemon surfaces a structured error and refuses to migrate. Point both at the same filesystem before upgrading.

**Inaccessible legacy path**. If the legacy `[vault].path` does not exist, is not a directory, or is not readable at first startup (per the resolutions of step 9 — `vault_management` § Vault Lifecycle State Machine), the migration still inserts a registry row but with `status = errored` and a recorded `last_error`. The daemon continues to start; no watcher or indexer for the errored vault. Search returns empty results from this vault. Recovery via `hmn vault resume` (when the underlying error has been resolved — e.g. the path is reachable again) or `hmn vault reset` (clears `last_error` and re-spawns) lands in step 11.

**Hard-removal of `[vault]`** is deferred to a future round (post-0.3.x). The round-3 boundary retro is the natural moment to schedule it.

---

## `[http]`

| Option | Type | Required | Default | Description |
|--------|------|----------|---------|-------------|
| `bind` | string | no | `127.0.0.1:7777` | Socket address for the HTTP endpoint. Loopback-only by default; Hypomnema currently does not implement auth. The daemon binds the Axum router on this address; failure to bind is fatal at daemon startup. |

> **Client note**: `hmn` derives its default daemon URL from this binding (e.g. `http://127.0.0.1:7777`). Override on the client with `--daemon-url` or `HYPOMNEMA_DAEMON_URL` — useful when the daemon runs on a different host or port than the local default.

---

## `[mcp]`

| Option | Type | Required | Default | Description |
|--------|------|----------|---------|-------------|
| `transport` | `"stdio"` \| `"socket"` | no | `"stdio"` | Forward-compat knob for the stdio-vs-socket MCP transport family. `transport = "stdio"` is served by the `hmn mcp` subcommand on the CLI binary. Streamable HTTP MCP is configured separately under `[mcp.http]` and lives on `hmnd` at `/mcp`. Setting `transport = "socket"` parses and validates but is **not bound today**; `hmnd` emits a `WARN`-level log at startup and continues running. The deferred socket transport will live in `hmnd` when it ships. See [step-08 workplan § Resolution D](../roadmap/step-08-workplan.md#d-connection-lifecycle-stdio-process-per-connection-vs-socket-long-lived) for the deferral rationale and binary-placement reasoning, and [ADR-0012](../decisions/0012-mcp-transport-stdio-v0.md) for the formal record. |
| `socket` | path | if `transport = "socket"` | `~/.local/share/hypomnema/mcp.sock` | Unix socket path. **Parsed and validated but not bound today** — see `transport` above. When the socket transport ships, the file will be created with mode `0600` (owner-only) per the deferred forward-compat decision in [step-08 workplan § Resolution E](../roadmap/step-08-workplan.md#e-authentication-on-the-socket-transport). |
| `enable_write_tools` | bool | no | `true` | Single-flag gate for **all seven** vault write tools (`vault_create`, `vault_pause`, `vault_resume`, `vault_reset`, `vault_rename`, `vault_rescan`, `vault_terminate`). When `true` (the default), the MCP surface advertises the three search tools, the read-only vault tools (`vault_list`, `vault_status`), and the seven write tools. When `false`, the read-only tools remain advertised and `tools/call` against any write tool returns a structured `write_tools_disabled` error envelope (the tool stays in the `tools/list` response — gating happens at call time, per [`docs/specs/vault-management.md` § MCP Tool Surface](../specs/vault-management.md#mcp-tool-surface)). The seven write tools share a single trust posture; per-tool gating is a future/backlog candidate if a use-case surfaces. A `vault_watch` read-only streaming tool is deferred pending rmcp streaming design/support; when it ships it will not be gated by this flag. |

> **MCP via the CLI**: the stdio MCP entry point is `hmn mcp` — see [`cli.md` § `hmn mcp`](./cli.md#mcp) and [ADR-0008 § Amendments](../decisions/0008-two-binary-daemon-plus-cli.md#amendments) for the binary-placement reasoning. Agent hosts (Claude Code, Iris) can configure `hmn` as the MCP command; `hmn mcp` translates MCP tool calls into HTTP requests against the running `hmnd`.

> **Why default-on for `enable_write_tools`**: the round-3 trust posture is localhost-only by default; agents that already invoke search tools have read access to every file in every vault. Per-tool gating fragments config without ergonomic gain (write tools share a single trust posture). Operators wanting strict opt-out get a single-line edit. See [`docs/specs/vault-management.md` § MCP Tool Surface](../specs/vault-management.md#mcp-tool-surface) for the full rationale.

---

## `[mcp.http]`

| Option | Type | Required | Default | Description |
|--------|------|----------|---------|-------------|
| `mcp.http.enabled` | bool | no | `true` | Whether to mount `/mcp` on the daemon's HTTP listener (loopback by default; inherits the listener's full trust posture). Set to `false` to disable HTTP-MCP without disabling stdio MCP. |
| `mcp.http.path` | string | no | `"/mcp"` | Route prefix. **Reserved-for-forward-compat in v1**: any value other than `"/mcp"` produces a startup error with the message `mcp.http.path must be "/mcp" in this version of Hypomnema`. Future versions may allow customization (e.g., for operators reverse-proxying multiple Hypomnema daemons under different prefixes). |

> **HTTP-MCP trust posture inheritance**: the `/mcp` route inherits whatever posture `hmnd`'s HTTP listener is configured with — bind address from [`[http]`](#http) (loopback-only by default), no auth, no TLS — extending [ADR-0005](../decisions/0005-local-everything.md)'s local-everything boundary. The HTTP-MCP transport does not introduce its own trust posture; if `hmnd` later gains auth or TLS, MCP-over-HTTP rides on them with no spec change. Origin-header validation is the one HTTP-MCP-specific defense (DNS-rebinding mitigation; allowed origins are loopback-only or `null`). See [ADR-0013](../decisions/0013-mcp-transport-streamable-http.md) and [`docs/specs/mcp-streamable-http.md`](../specs/mcp-streamable-http.md).

---

## `[embedding]`

| Option | Type | Required | Default | Description |
|--------|------|----------|---------|-------------|
| `endpoint` | URL | yes | — | OpenAI-compatible embeddings endpoint |
| `model` | string | yes | — | Model name sent in the API request |
| `dimension` | integer | yes | `768` | Must match the dimension baked into the schema. Mismatch → daemon fails at startup. |
| `api_key` | string | no | `""` | Sent as `Authorization: Bearer` if non-empty |
| `timeout_ms` | integer | no | `30000` | Per-request timeout for the embed HTTP call, in milliseconds. |
| `max_retries` | integer | no | `1` | Maximum retries on transport-level failures or HTTP `5xx`. Backoff before retry is 250ms. `4xx` responses are never retried (those are the daemon's bug, not the service's). Set to `0` to disable retries. |
| `batch_size` | integer | no | `16` | Maximum number of chunks sent per embed request. A file's chunks are split into requests of this size and concatenated in order. If the service rejects an over-large batch (HTTP `400`/`413`/`422`), the client halves the batch size (floor `1`) and retries, then keeps the learned ceiling for the rest of the daemon's lifetime — so a value above the service's limit self-corrects after a few probe requests. Set at or below the service's max client batch size (e.g. TEI's `max_client_batch_size`, default `32`) to skip the probe entirely. A rejection at size `1` is a hard error (e.g. a single chunk exceeds the model's max input length). |

Changing `dimension` after the index is built is not supported — the vec0 virtual table's dimension is fixed at creation time. A different embedding model with a different dimension requires a re-index (drop + rebuild).

**Skip-and-log on embedding failure**: if the embedding service is unreachable (transport error), responds with HTTP `5xx`, or returns a vector whose length disagrees with `dimension`, the indexer logs an `ERROR` and skips that file's chunks — `chunks` and `chunks_vec` rows are left in their previous state, the file's `content_hash` is not advanced, and the daemon stays responsive. The next watcher event or rescan retries naturally. `4xx` responses and JSON parse failures propagate as bugs in the daemon — those classes are not the service's fault. Daemon startup separately fails loudly when `dimension` disagrees with the schema-baked value (a configuration error, not a runtime one). At startup, after the embedding client is built, the daemon also issues a one-token health probe: a successful probe logs `INFO`, and a failure (unreachable, wrong dimension, etc.) logs `WARN` with both numbers, the configured `endpoint`, and the configured `model` — but never fails the daemon. See [`docs/specs/semantic-search.md`](../specs/semantic-search.md) § Edge Cases for the query-time complement.

See [ADR-0005: Local Everything](../decisions/0005-local-everything.md), [ADR-0007: sqlite-vec over Alternatives](../decisions/0007-sqlite-vec-over-alternatives.md).

---

## `[search.semantic]`

Controls defaults and candidate depth for the semantic search endpoint (`POST /search/semantic`, `hmn search semantic`, and the `search_semantic` MCP tool). All keys are optional; the daemon ships with the values shown as defaults.

| Option | Type | Required | Default | Description |
|--------|------|----------|---------|-------------|
| `default_granularity` | string | no | `"document"` | Default result granularity when the request omits `granularity`. Valid values: `"document"`, `"chunk"`. `"document"` groups results by file and answers *"which notes are relevant?"*; `"chunk"` returns a flat list and answers *"which passages are relevant?"* |
| `default_chunks_per_document` | integer | no | `3` | Default number of evidence chunks per document result when `chunks_per_document` is absent from the request. Range: `1..=100`. |
| `document_candidate_multiplier` | integer | no | `10` | Multiplied by the request `limit` to compute the kNN candidate pool fed to the document grouper (before `min_similarity` filtering). Higher values increase recall at the cost of more per-vault work. Minimum effective value is `1`. |
| `document_candidate_limit` | integer | no | `1000` | Hard cap on candidate chunk count regardless of `limit × multiplier`. Prevents runaway memory use on large `limit` values. Must be ≥ 1. |

**Example** — use chunk-first defaults for a low-resource deployment:

```toml
[search.semantic]
default_granularity = "chunk"
default_chunks_per_document = 1
document_candidate_multiplier = 5
document_candidate_limit = 200
```

See [`docs/specs/semantic-search.md`](../specs/semantic-search.md) § Configuration Knobs for the full document-mode candidate pipeline.

---

## `[watcher]`

| Option | Type | Required | Default | Description |
|--------|------|----------|---------|-------------|
| `debounce_ms` | integer | no | `500` | Coalescing window for `notify-debouncer-full`. Too short → event storms slip through; too long → user-visible indexing lag |
| `ignore_patterns` | list of glob strings | no | (sensible defaults for Git, Obsidian, Dropbox, Syncthing conflict files, tmp files) | Files matching any pattern are not indexed and do not appear in search. Defaults cover common dotfile directories (`.git/**`, `.obsidian/**`, `.trash/**`), sync-tool conflict files (Obsidian / Dropbox / Syncthing), and tmp-file extensions. No paths are filtered outside `ignore_patterns`; edit the list to change behavior. |

Sync tools that burst-write across more than the debounce window may justify `debounce_ms = 1000` or `2000`; do not raise it speculatively — the watcher logs backpressure, raise it when you see those logs.

> **Future direction:** honor `.gitignore` / `.dockerignore` when present; add a Mutagen-style `ignore_vcs_files` flag. See [product/vision.md#open-questions](../product/vision.md#open-questions).

---

## `[storage]`

| Option | Type | Required | Default | Description |
|--------|------|----------|---------|-------------|
| `data_dir` | path | no | `~/.local/share/hypomnema` on Linux and macOS; `%APPDATA%\hypomnema` on Windows. Respects `XDG_DATA_HOME`. | Root for daemon-owned state. **Never inside any registered vault path** — see [ADR-0006](../decisions/0006-outbox-outside-watched-directory.md) (amended 2026-04-26 for multi-vault layout). |

Layout under `data_dir`:

```
<data_dir>/
  vaults.sqlite               # authoritative vault registry
  vaults/
    <vault_id>/
      index.sqlite            # files, chunks, chunks_vec
      meta.toml               # human-readable copy of registry row
  hmnd.pid
  logs/
```

The per-vault `index.sqlite` is created when the vault is created (`hmn vault create …`) and removed when the vault is terminated (`hmn vault terminate …`). Live change notifications are exposed through `hmn vault watch`, HTTP streaming, and MCP; see [the change-events spec](../specs/change-events.md). The vault-id-to-path mapping is read from the registry; clients that don't know IDs in advance use `hmn vault list` or `GET /vaults`.

---

## `[logging]`

| Option | Type | Required | Default | Description |
|--------|------|----------|---------|-------------|
| `level` | tracing level | no | `info` | Daemon-level filter |
| `notify_level` | tracing level | no | `warn` | `notify` crate is chatty; quieter by default |
| `tokio_level` | tracing level | no | `error` | Suppress routine tokio traffic |

Levels: `trace`, `debug`, `info`, `warn`, `error`.

---

## Environment Variable Mapping

| Config Path | Environment Variable |
|-------------|---------------------|
| `default_vault_name` | `HYPOMNEMA_DEFAULT_VAULT_NAME` |
| `http.bind` | `HYPOMNEMA_HTTP_BIND` |
| `embedding.endpoint` | `HYPOMNEMA_EMBEDDING_ENDPOINT` |
| `embedding.api_key` | `HYPOMNEMA_EMBEDDING_API_KEY` |
| `storage.data_dir` | `HYPOMNEMA_DATA_DIR` |
| `logging.level` | `HYPOMNEMA_LOG_LEVEL` |

(Exact mapping syntax TBD — flag-style `HYPOMNEMA__EMBEDDING__ENDPOINT` double-underscore convention is also plausible.)

---

## Validation Rules

- `embedding.dimension` must match the schema; mismatch fails the daemon at startup with a message pointing at this reference
- `default_vault_name` must be non-empty after trimming whitespace; an empty string fails the daemon at startup with a configuration error
- `storage.data_dir` must not be under any registered vault path — the daemon fails at startup if it is, per [ADR-0006](../decisions/0006-outbox-outside-watched-directory.md). At vault creation time, a path that would place `data_dir` inside the new vault is rejected with `422 vault_path_invalid`.
- `storage.data_dir/vaults/` must exist or be creatable by the daemon (it is created on first startup if absent)
- `mcp.transport = "socket"` requires `mcp.socket` to be set and the parent directory to be writable
- `mcp.http.path` must equal `"/mcp"` in v1; any other value fails the daemon at startup with the message `mcp.http.path must be "/mcp" in this version of Hypomnema` ([ADR-0013](../decisions/0013-mcp-transport-streamable-http.md))
- A top-level `[vault]` block is parsed but **deprecated**; see [Legacy `[vault]` block migration](#legacy-vault-block-migration) — its presence does not fail validation, but logs a `WARN` while the legacy migration could still engage (registry empty)
- The daemon scans + reconciles each active vault on every startup; this is the only startup mode today.

---

## See Also

- [CLI Reference](./cli.md)
- [Architecture Overview](../architecture/overview.md)
- [ADR-0006: Daemon State Lives Outside the Watched Directory](../decisions/0006-outbox-outside-watched-directory.md)
- [ADR-0009: Multi-Vault per Daemon](../decisions/0009-multi-vault-per-daemon.md)
- [ADR-0010: Vault Definitions Are Runtime State, Not Configuration](../decisions/0010-vault-definitions-as-runtime-state.md)
- [Vault Management Spec](../specs/vault-management.md)
