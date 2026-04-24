# CLI Reference: `hmnd` and `hmn`

**Version**: 0.1.0 (draft — subcommand names not finalized)
**Generated**: 2026-04-23

---

> **Status**: This reference is a **draft**. The handoff's "Open questions for early implementation" section flags CLI subcommand naming as not yet decided — "`hmn start`, `hmn scan`, `hmn search`, `hmn status` is one obvious shape; could change." Hypomnema ships two binaries per [ADR-0008](../decisions/0008-two-binary-daemon-plus-cli.md): the daemon (`hmnd`) and the CLI client (`hmn`). Revise this doc once the CLI is pinned down.

---

Hypomnema ships two binaries:

- **`hmnd`** — the daemon. Owns the watched directory, the SQLite store, and the HTTP + MCP servers. Long-running; typically managed by systemd or an equivalent supervisor.
- **`hmn`** — the CLI client. Thin wrapper that speaks HTTP to a running `hmnd`. Used for day-to-day search, status, and scripts.

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

Start the daemon in the foreground. Reads config, opens the SQLite store, starts the watcher, the HTTP server, and (depending on config) the MCP server over the configured transport.

**Usage**:
```
hmnd [--config PATH] [--rescan] [--mcp-stdio]
```

**Options**:

| Option | Description | Default |
|--------|-------------|---------|
| `--rescan` | Force a full rescan and reconciliation of the vault on startup instead of trusting the existing index | TBD. Handoff suggested on-by-default (auto-rescan + reconcile); current lean is off-by-default for fast restarts. See open questions. |
| `--mcp-stdio` | Serve the MCP surface over stdio instead of starting the HTTP server. Intended for agent hosts (Claude Code, Iris) that launch the daemon as a child process. Final flag shape TBD. | `false` |

**Examples**:

```bash
# Foreground daemon with default config
hmnd

# Explicit config path
hmnd --config ~/vaults/work/hypomnema.toml

# Force full rescan on startup
hmnd --rescan

# Launched by an agent host over stdio
hmnd --mcp-stdio
```

#### `scan`

Walk the vault and reconcile the index without starting the HTTP / MCP servers. Useful for one-shot reindexing or verifying the index from a cron job.

**Usage**:
```
hmnd scan [--config PATH]
```

#### `config-validate`

Parse the configuration file, run validation rules (vault exists, `data_dir` not under `vault`, embedding dimension matches the schema, etc.), and exit with a zero status on success. Final subcommand name TBD.

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
| `--limit N` | Max results | 10 (semantic), 100 (filesystem, content) |

**Examples**:

```bash
hmn search filesystem "notes/databases/*.md"
hmn search content "pgvector"
hmn search semantic "how do we prevent spurious reindexes"
```

#### `status`

Report daemon health: is `hmnd` reachable, index size, vault path, last indexed file, outbox size.

**Usage**:
```
hmn status [--json]
```

### Exit Codes (client)

| Code | Meaning |
|------|---------|
| `0` | Success |
| `1` | General error |
| `2` | Invalid arguments |
| `4` | Daemon not reachable |

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
- [Specifications](../specs/) — per-search-mode semantics
- [Architecture: Search API](../architecture/overview.md#search-api)

---

## Notes

- Several options marked TBD above are open questions the handoff calls out explicitly; this file is expected to stabilize over steps 1–8 of the v0 plan (see [implementation/tech-stack.md](../implementation/tech-stack.md)).
- `hmnd --mcp-stdio` is the current placeholder for the stdio-transport mode that agent hosts will use; the flag shape may change to a subcommand (`hmnd mcp-stdio`) or an environment variable once step 8 lands.
