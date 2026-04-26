# Filesystem Search Specification

**Version**: 0.1.0
**Date**: 2026-04-23
**Status**: Draft

---

## Overview

Filesystem search answers path-shaped questions: *what files exist in the vault*, *what's in this subdirectory*, *what matches this glob pattern*. It is the cheapest of the three search modes and is typically the first one an agent uses when exploring an unfamiliar vault.

**Related Documents**:
- [ADR-0004: Three Search Modes as Peers](../decisions/0004-three-search-modes-as-peers.md)
- [Architecture: Search API](../architecture/overview.md#search-api)

---

## Behavior

### Normal Flow

The consumer issues a filesystem search via HTTP or MCP, specifying zero or more of: a path prefix, a glob pattern, a maximum depth. Hypomnema answers from the filesystem index (paths, sizes, mtimes stored in SQLite) — it does not re-walk the directory on every call.

1. Receive request with optional `prefix`, `glob`, `max_depth`
2. Query the files table in the store
3. Return matching entries with path, size, mtime

Results are ordered by path, ascending, for stable output.

### Empty Vault

If the vault has no indexed files (fresh start before scan completes), return an empty list and a hint indicating the index is warming.

---

## Data Schema

### Request

```yaml
prefix: "notes/databases/"      # optional
glob: "**/*.md"                  # optional
max_depth: 3                     # optional
limit: 100                       # optional, default: 100
```

The HTTP endpoint accepts the same fields as a JSON body via `POST /search/filesystem`.

### Response

```yaml
results:
  - path: "notes/databases/pgvector.md"
    size: 4821
    mtime: "2026-04-22T14:31:08Z"
    content_hash: "sha256:abc123…"
  - path: "notes/databases/sqlite.md"
    size: 2104
    mtime: "2026-04-21T09:12:33Z"
    content_hash: "sha256:def456…"
truncated: false
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `path` | string | yes | Vault-relative path |
| `size` | integer | yes | File size in bytes |
| `mtime` | ISO-8601 string | yes | Last modification time (from filesystem) |
| `content_hash` | string | yes | `sha256:` hash of file content; primary change-detection signal |
| `vault` | string | no | Reserved; always absent in v0. Will carry the source vault identifier when multi-vault ships. |
| `truncated` | boolean | yes | True if results exceeded `limit` |

---

## Edge Cases

### Symlinks

v0: symlinks within the vault are followed. Symlinks pointing outside the vault are *not* followed (defensive). The walker rejects any entry whose `fs::canonicalize`'d real path is not under the canonicalized vault root. Open question: should symlinks be indexed at all? See Open Questions.

### Case-sensitivity

Path matching honors the host filesystem's case sensitivity. On macOS's default case-insensitive HFS+/APFS, `Notes/` matches `notes/`. On Linux, it does not.

### Hidden files

Dotfiles are not filtered unconditionally. Common dotfile directories (`.obsidian/`, `.trash/`, etc.) are matched by the default `ignore_patterns` list and so are not indexed, but they are skipped via config, not hard-coded. See [reference/configuration.md](../reference/configuration.md#watcher) for the defaults.

### Prefix semantics

`prefix` is a path-prefix string match, not a glob. Trailing `/` is normalized — `notes` and `notes/` both match `notes/...` and exclude `notesarchive/...`. Empty `prefix` matches everything. Absolute paths and `..` segments are rejected with `invalid_prefix`. Resolved in step 5; see [step-5 workplan § Deferred decision 4](../roadmap/step-05-workplan.md#4-regex-vs-glob-behavior-boundaries).

---

## Open Questions

- [ ] Should symlinks inside the vault be indexed, or just followed for reads?
- [ ] Do we need a `regex` alternative to `glob`? — v0 ships glob-only; see [step-5 workplan § Deferred decision 5](../roadmap/step-05-workplan.md#5-regex-alternative-to-glob) — no field added, additive when needed.
- [ ] Should results include a `frontmatter` summary (title, tags) for quick triage?

---

## Revision History

| Version | Date | Changes |
|---------|------|---------|
| 0.1.0 | 2026-04-23 | Initial draft, seeded from project handoff v0 scope |
