# Hypomnema Configuration Reference

**Version**: 0.2.0
**Generated**: 2026-04-26

---

> **Status**: Schema pinned in step 1 — see [step-01 workplan § TOML config schema](../roadmap/step-01-workplan.md#2-toml-config-schema). Format is TOML; default location is `~/.config/hypomnema/config.toml` (respects `XDG_CONFIG_HOME`). The top-level `vault` setting was removed in 0.2.0 — vaults are now runtime state managed via the control-plane API ([ADR-0010](../decisions/0010-vault-definitions-as-runtime-state.md)).

> **Scope**: Every option on this page is daemon-side — it affects the behavior of `hmnd`. The CLI client (`hmn`) reads only the daemon URL (derived from `[http].bind`) to know where to send requests; override on the client with `--daemon-url` or `HYPOMNEMA_DAEMON_URL`. See [cli.md](./cli.md) and [ADR-0008](../decisions/0008-two-binary-daemon-plus-cli.md).

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

# Embedding service (OpenAI-compatible)
[embedding]
endpoint = "http://127.0.0.1:8080/v1/embeddings"
# The exact model identifier is the string TEI's /v1/embeddings endpoint expects.
model = "nomic-embed-text-v1.5"
dimension = 768
api_key = ""   # empty for local services that don't require one
# Path to the sqlite-vec dynamic library. `<ext>` is `dylib` (macOS), `so` (Linux), `dll` (Windows).
# The HYPOMNEMA_VEC_EXT_PATH environment variable, if set, overrides this value.
extension_path = "~/.local/share/hypomnema/sqlite-vec.dylib"
timeout_ms = 30000   # per-request embed timeout
max_retries = 1      # retry once on transport error or HTTP 5xx; 250ms backoff
batch_size = 1       # chunks per embed request (v0 always 1)

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
data_dir = "~/.local/share/hypomnema"      # index + outbox + logs
index_file = "index.sqlite"                 # relative to data_dir
outbox_file = "outbox.jsonl"                # relative to data_dir

# Logging
[logging]
level = "info"
notify_level = "warn"
tokio_level = "error"
```

---

## `default_vault_name`

When a control-plane command (e.g., `hmn vault status`) omits the vault selector, the daemon resolves to this name. The daemon does **not** auto-create a default vault on first run; the user must run `hmn vault create` (which uses `default_vault_name` if `--name` is omitted).

| Option | Type | Required | Default | Description |
|--------|------|----------|---------|-------------|
| `default_vault_name` | string | no | `"default"` | Name resolved when a command omits the selector. Set to `""` to require explicit names on every command. |

See [ADR-0010](../decisions/0010-vault-definitions-as-runtime-state.md) for why vaults live in runtime state rather than configuration.

---

## `[http]`

| Option | Type | Required | Default | Description |
|--------|------|----------|---------|-------------|
| `bind` | string | no | `127.0.0.1:7777` | Socket address for the HTTP endpoint. Loopback-only by default; v0 does not implement auth. Step 5 binds the Axum router on this address; failure to bind is fatal at daemon startup. |

> **Client note**: `hmn` derives its default daemon URL from this binding (e.g. `http://127.0.0.1:7777`). Override on the client with `--daemon-url` or `HYPOMNEMA_DAEMON_URL` — useful when the daemon runs on a different host or port than the local default.

---

## `[mcp]`

| Option | Type | Required | Default | Description |
|--------|------|----------|---------|-------------|
| `transport` | `"stdio"` \| `"socket"` | no | `"stdio"` | How MCP clients connect |
| `socket` | path | if `transport = "socket"` | `~/.local/share/hypomnema/mcp.sock` | Unix socket path |

---

## `[embedding]`

| Option | Type | Required | Default | Description |
|--------|------|----------|---------|-------------|
| `endpoint` | URL | yes | — | OpenAI-compatible embeddings endpoint |
| `model` | string | yes | — | Model name sent in the API request |
| `dimension` | integer | yes | `768` | Must match the dimension baked into the schema. Mismatch → daemon fails at startup. |
| `api_key` | string | no | `""` | Sent as `Authorization: Bearer` if non-empty |
| `extension_path` | path | no | `~/.local/share/hypomnema/sqlite-vec.<ext>` (`<ext>` = `dylib` macOS / `so` Linux / `dll` Windows) | Filesystem path to the sqlite-vec dynamic library, loaded into every SQLite connection in the pool. Daemon fails at startup if the file does not exist. The `HYPOMNEMA_VEC_EXT_PATH` environment variable, if set, overrides this value. |
| `timeout_ms` | integer | no | `30000` | Per-request timeout for the embed HTTP call, in milliseconds. |
| `max_retries` | integer | no | `1` | Maximum retries on transport-level failures or HTTP `5xx`. Backoff before retry is 250ms. `4xx` responses are never retried (those are the daemon's bug, not the service's). Set to `0` to disable retries. |
| `batch_size` | integer | no | `1` | Number of chunks per embed request. v0 ships at `1`; future steps may promote to batching when chunk volume justifies the coordination cost. |

Changing `dimension` after the index is built is not supported — the vec0 virtual table's dimension is fixed at creation time. A different embedding model with a different dimension requires a re-index (drop + rebuild).

**sqlite-vec extension prerequisite**: the dynamic library at `extension_path` is **not** bundled with `hmnd` and is **not** provisioned by the development shell. The operator must download a prebuilt artifact for their platform from the [sqlite-vec releases](https://github.com/asg017/sqlite-vec/releases) and place it at the configured path (or set `HYPOMNEMA_VEC_EXT_PATH`). If the file is missing at startup, the daemon exits with a structured error naming both the configured path and the env-var override.

**Skip-and-log on embedding failure**: if the embedding service is unreachable (transport error), responds with HTTP `5xx`, or returns a vector whose length disagrees with `dimension`, the indexer logs an `ERROR` and skips that file's chunks — `chunks` and `chunks_vec` rows are left in their previous state, the file's `content_hash` is not advanced, and the daemon stays responsive. The next watcher event or rescan retries naturally. `4xx` responses and JSON parse failures propagate as bugs in the daemon — those classes are not the service's fault. Daemon startup separately fails loudly when `dimension` disagrees with the schema-baked value (a configuration error, not a runtime one). At startup, after the embedding client is built, the daemon also issues a one-token health probe: a successful probe logs `INFO`, and a failure (unreachable, wrong dimension, etc.) logs `WARN` with both numbers, the configured `endpoint`, and the configured `model` — but never fails the daemon. See [`docs/specs/semantic-search.md`](../specs/semantic-search.md) § Edge Cases for the query-time complement.

See [ADR-0005: Local Everything](../decisions/0005-local-everything.md), [ADR-0007: sqlite-vec over Alternatives](../decisions/0007-sqlite-vec-over-alternatives.md).

---

## `[watcher]`

| Option | Type | Required | Default | Description |
|--------|------|----------|---------|-------------|
| `debounce_ms` | integer | no | `500` | Coalescing window for `notify-debouncer-full`. Too short → event storms slip through; too long → user-visible indexing lag |
| `ignore_patterns` | list of glob strings | no | (sensible defaults for Git, Obsidian, Dropbox, Syncthing conflict files, tmp files) | Files matching any pattern are not indexed and do not appear in search. Defaults cover common dotfile directories (`.git/**`, `.obsidian/**`, `.trash/**`), sync-tool conflict files (Obsidian / Dropbox / Syncthing), and tmp-file extensions. No paths are filtered outside `ignore_patterns`; edit the list to change behavior. |

Sync tools that burst-write across more than the debounce window may justify `debounce_ms = 1000` or `2000`; do not raise it speculatively — the watcher logs backpressure, raise it when you see those logs.

> **Future direction (not v0):** honor `.gitignore` / `.dockerignore` when present; add a Mutagen-style `ignore_vcs_files` flag. See [product/vision.md#open-questions](../product/vision.md#open-questions).

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
      outbox.jsonl            # per-vault append-only log
      meta.toml               # human-readable copy of registry row
  hmnd.pid
  logs/
```

The per-vault `index.sqlite` and `outbox.jsonl` are created when the vault is created (`hmn vault create …`) and removed when the vault is terminated (`hmn vault terminate …`). Outbox consumers tail the per-vault file under the vault's subdirectory; reopen on `ENOENT` or inode change — see [the change-events spec § Edge Cases](../specs/change-events.md#edge-cases). The vault-id-to-path mapping is read from the registry; clients that don't know IDs in advance use `hmn vault list` or `GET /vaults`.

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
| `embedding.extension_path` | `HYPOMNEMA_VEC_EXT_PATH` |
| `storage.data_dir` | `HYPOMNEMA_DATA_DIR` |
| `logging.level` | `HYPOMNEMA_LOG_LEVEL` |

(Exact mapping syntax TBD — flag-style `HYPOMNEMA__EMBEDDING__ENDPOINT` double-underscore convention is also plausible.)

---

## Validation Rules

- `embedding.dimension` must match the schema; mismatch fails the daemon at startup with a message pointing at this reference
- `storage.data_dir` must not be under any registered vault path — the daemon fails at startup if it is, per [ADR-0006](../decisions/0006-outbox-outside-watched-directory.md). At vault creation time, a path that would place `data_dir` inside the new vault is rejected with `422 vault_path_invalid`.
- `storage.data_dir/vaults/` must exist or be creatable by the daemon (it is created on first startup if absent)
- `mcp.transport = "socket"` requires `mcp.socket` to be set and the parent directory to be writable
- The daemon scans + reconciles each active vault on every startup; this is the only mode in v0.

---

## See Also

- [CLI Reference](./cli.md)
- [Architecture Overview](../architecture/overview.md)
- [ADR-0006: Outbox Lives Outside the Watched Directory](../decisions/0006-outbox-outside-watched-directory.md)
- [ADR-0009: Multi-Vault per Daemon](../decisions/0009-multi-vault-per-daemon.md)
- [ADR-0010: Vault Definitions Are Runtime State, Not Configuration](../decisions/0010-vault-definitions-as-runtime-state.md)
- [Vault Management Spec](../specs/vault-management.md)
