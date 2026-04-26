# Hypomnema Configuration Reference

**Version**: 0.1.0
**Generated**: 2026-04-23

---

> **Status**: Schema pinned in step 1 — see [step-01 workplan § TOML config schema](../roadmap/step-01-workplan.md#2-toml-config-schema). Format is TOML; default location is `~/.config/hypomnema/config.toml` (respects `XDG_CONFIG_HOME`).

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
# Path to the vault Hypomnema watches (required)
vault = "/home/user/Documents/vault"

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

## `vault`

The directory Hypomnema watches and indexes. Must exist. Must be readable. Hypomnema never writes here.

| Option | Type | Required | Default | Description |
|--------|------|----------|---------|-------------|
| `vault` | path | yes | — | Absolute or `~`-expanded path to the vault |

---

## `[http]`

| Option | Type | Required | Default | Description |
|--------|------|----------|---------|-------------|
| `bind` | string | no | `127.0.0.1:7777` | Socket address for the HTTP endpoint. Loopback-only by default; v0 does not implement auth. |

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

Changing `dimension` after the index is built is not supported — the vec0 virtual table's dimension is fixed at creation time. A different embedding model with a different dimension requires a re-index (drop + rebuild).

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
| `data_dir` | path | no | `~/.local/share/hypomnema` on Linux and macOS; `%APPDATA%\hypomnema` on Windows. Respects `XDG_DATA_HOME`. | Root for daemon-owned state. **Never inside the vault** — see [ADR-0006](../decisions/0006-outbox-outside-watched-directory.md) |
| `index_file` | relative path | no | `index.sqlite` | SQLite file containing all three indexes |
| `outbox_file` | relative path | no | `outbox.jsonl` | JSONL event log |

The outbox file is created at daemon startup if missing. Consumers that tail it should reopen on `ENOENT` or inode change — see [the change-events spec § Edge Cases](../specs/change-events.md#edge-cases).

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
| `vault` | `HYPOMNEMA_VAULT` |
| `http.bind` | `HYPOMNEMA_HTTP_BIND` |
| `embedding.endpoint` | `HYPOMNEMA_EMBEDDING_ENDPOINT` |
| `embedding.api_key` | `HYPOMNEMA_EMBEDDING_API_KEY` |
| `storage.data_dir` | `HYPOMNEMA_DATA_DIR` |
| `logging.level` | `HYPOMNEMA_LOG_LEVEL` |

(Exact mapping syntax TBD — flag-style `HYPOMNEMA__EMBEDDING__ENDPOINT` double-underscore convention is also plausible.)

---

## Validation Rules

- `vault` must exist and be readable
- `embedding.dimension` must match the schema; mismatch fails the daemon at startup with a message pointing at this reference
- `storage.data_dir` must not be under `vault` — the daemon fails at startup if it is, per [ADR-0006](../decisions/0006-outbox-outside-watched-directory.md)
- `mcp.transport = "socket"` requires `mcp.socket` to be set and the parent directory to be writable
- The daemon scans + reconciles on every startup; this is the only mode in v0.

---

## See Also

- [CLI Reference](./cli.md)
- [Architecture Overview](../architecture/overview.md)
- [ADR-0006: Outbox Lives Outside the Watched Directory](../decisions/0006-outbox-outside-watched-directory.md)
