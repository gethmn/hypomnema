# hmn CLI Reference

**Version**: 0.1.0 (draft — subcommand names not finalized)
**Generated**: 2026-04-23

---

> **Status**: This reference is a **draft**. The handoff's "Open questions for early implementation" section flags CLI subcommand naming as not yet decided — "`hmn start`, `hmn scan`, `hmn search`, `hmn status` is one obvious shape; could change." Revise this doc once the CLI is pinned down.

---

## Synopsis

```
hmn [global-options] <command> [command-options] [arguments]
```

The `hmn` binary is both the daemon (via `hmn start`) and a client for the running daemon (via `hmn search`, etc.). Client subcommands speak HTTP to the local daemon.

---

## Global Options

| Option | Short | Description | Default |
|--------|-------|-------------|---------|
| `--config` | `-c` | Path to configuration file | `~/.config/hypomnema/config.toml` |
| `--verbose` | `-v` | Enable verbose output | `false` |
| `--help` | `-h` | Show help | — |
| `--version` | | Show version | — |

---

## Commands

### `start`

Start the daemon in the foreground. Reads config, opens the SQLite store, starts the watcher and HTTP/MCP servers.

**Usage**:
```
hmn start [--config PATH] [--rescan]
```

**Options**:

| Option | Description | Default |
|--------|-------------|---------|
| `--rescan` | Force a full rescan and reconciliation of the vault on startup instead of trusting the existing index | TBD. Handoff suggested on-by-default (auto-rescan + reconcile); current lean is off-by-default for fast restarts. See open questions. |

**Examples**:

```bash
# Default config
hmn start

# Explicit config path
hmn start --config ~/vaults/work/hypomnema.toml

# Force full rescan
hmn start --rescan
```

---

### `scan`

Walk the vault and reconcile the index without starting the HTTP/MCP servers. Useful for one-shot reindexing or verifying the index from a cron job.

**Usage**:
```
hmn scan [--config PATH]
```

---

### `search`

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
| `--json` | Output JSON instead of human-formatted text | `false` |

**Examples**:

```bash
hmn search filesystem "notes/databases/*.md"
hmn search content "pgvector"
hmn search semantic "how do we prevent spurious reindexes"
```

---

### `status`

Report daemon health: process running?, index size, vault path, last indexed file, outbox size.

**Usage**:
```
hmn status [--json]
```

---

## Environment Variables

| Variable | Description | Equivalent Option |
|----------|-------------|-------------------|
| `HYPOMNEMA_CONFIG` | Path to config file | `--config` |
| `HYPOMNEMA_DATA_DIR` | Override the daemon data directory | (no flag; config file only, TBD) |

---

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Success |
| `1` | General error |
| `2` | Invalid arguments |
| `3` | Configuration error |
| `4` | Daemon not reachable (for client subcommands) |

---

## See Also

- [Configuration Reference](./configuration.md)
- [Specifications](../specs/) — per-search-mode semantics
- [Architecture: Search API](../architecture/overview.md#search-api)

---

## Notes

- Several options marked TBD above are open questions the handoff calls out explicitly; this file is expected to stabilize over steps 1–8 of the v0 plan (see [implementation/tech-stack.md](../implementation/tech-stack.md)).
