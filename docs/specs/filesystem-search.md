# Filesystem Search Specification

**Version**: 0.2.0
**Date**: 2026-04-27
**Status**: Draft

---

## Overview

Filesystem search answers path-shaped questions: *what files exist in the vault*, *what's in this subdirectory*, *what matches this glob pattern*. It is the cheapest of the three search modes and is typically the first one an agent uses when exploring an unfamiliar vault.

**Related Documents**:
- [ADR-0004: Three Search Modes as Peers](../decisions/0004-three-search-modes-as-peers.md)
- [ADR-0009: Multi-Vault per Daemon](../decisions/0009-multi-vault-per-daemon.md)
- [Vault Management § Cross-Vault Search Semantics](./vault-management.md#cross-vault-search-semantics)
- [Architecture: Search API](../architecture/overview.md#search-api)

---

## Behavior

### Normal Flow

The consumer issues a filesystem search via HTTP or MCP, specifying zero or more of: a path prefix, a glob pattern, a maximum depth. Hypomnema answers from the filesystem index (paths, sizes, mtimes stored in SQLite) — it does not re-walk the directory on every call.

1. Receive request with optional `prefix`, `glob`, `max_depth`, `vaults`
2. Resolve in-scope vaults: `vaults` filter narrows to the named subset; otherwise all currently active vaults
3. For each in-scope vault: query the files table in the vault's `index.sqlite`
4. Merge per-vault results by path (ascending, byte-lexicographic); break ties by `vault_id`
5. Truncate the merged list to `limit`; return matching entries with `path`, `size`, `mtime`, `content_hash`, and (when multi-vault is active) `vault` + `vault_name`

### Cross-Vault Behavior

Cross-vault execution semantics — vault scoping, ordering, partial-failure handling, paused/errored vault inclusion, fan-out model — are pinned in [vault-management.md § Cross-Vault Search Semantics](./vault-management.md#cross-vault-search-semantics) and apply uniformly across the three search modes. The summary that's filesystem-search-specific:

- **Default scope**: all currently active vaults; per-result `vault` + `vault_name` disambiguate origin.
- **Ordering**: global path-ascending across vaults (lifted from v0/step-9's per-vault path-asc; the merged list is sorted as a single slice). Identical paths across two vaults break ties by `vault_id` (UUIDv7 → creation-time-stable).
- **`limit`** (§ A.4): each vault contributes up to `limit` results to the merge pool; the merged list is then truncated to `limit`. `truncated: true` is set if any per-vault search reported truncation **or** the merged list was capped.
- **`vaults` filter** (§ A.9): `Some([...])` narrows to the named subset; `None` queries all active vaults; `Some([])` is a request validation error.

For N=1 (single-vault deployment) the cross-vault wire shape collapses to v0/step-9 semantics — single slice already path-sorted, `vault` + `vault_name` populated but the `partial_results` field absent.

### Empty Vault

If a vault has no indexed files (fresh start before scan completes), it contributes zero results and is not flagged in `partial_results` (it's active and reachable; the empty contribution is honest, not a partial failure).

---

## Data Schema

### Request

```yaml
prefix: "notes/databases/"      # optional
glob: "**/*.md"                  # optional
max_depth: 3                     # optional
limit: 100                       # optional, default: 100
vaults:                          # optional; multi-vault scoping
  - "personal"                   # name or surrogate ID
  - "work"
```

The HTTP endpoint accepts the same fields as a JSON body via `POST /search/filesystem`.

| Request Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `prefix` | string | no | none | Path-prefix string match (not a glob); see Edge Cases. |
| `glob` | string | no | none | Glob pattern over vault-relative paths. |
| `max_depth` | integer | no | none | Maximum directory depth from prefix (or vault root). |
| `limit` | integer | no | 100 | Global result cap after cross-vault merge. Validation: `1..=1000`. |
| `vaults` | array of strings | no | none → all active | Subset of vaults to query, by name or surrogate ID. Empty array is rejected as `invalid_request`. |

### Response

```yaml
results:
  - path: "notes/databases/pgvector.md"
    size: 4821
    mtime: "2026-04-22T14:31:08Z"
    content_hash: "sha256:abc123…"
    vault: "01951f6c-7c3b-7a2e-8c1d-1a2b3c4d5e6f"
    vault_name: "personal"
  - path: "notes/databases/sqlite.md"
    size: 2104
    mtime: "2026-04-21T09:12:33Z"
    content_hash: "sha256:def456…"
    vault: "01951f6c-7c3b-7a2e-8c1d-1a2b3c4d5e6f"
    vault_name: "personal"
truncated: false
# `partial_results` omitted in the all-success / all-active case
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `path` | string | yes | Vault-relative path |
| `size` | integer | yes | File size in bytes |
| `mtime` | ISO-8601 string | yes | Last modification time (from filesystem) |
| `content_hash` | string | yes | `sha256:` hash of file content; primary change-detection signal |
| `vault` | string | no | Surrogate vault ID (UUIDv7). Populated when multi-vault is active (round 3+); omitted for v0/step-9 single-vault wire shape. |
| `vault_name` | string | no | Mutable, point-in-time-accurate display name for the source vault. Populated alongside `vault`. Never appears in live change events (see [change-events.md](./change-events.md)). |
| `truncated` | boolean | yes | True if any per-vault search reported truncation OR the merged list exceeded `limit`. |
| `partial_results` | object | no | Cross-vault diagnostic; present only when at least one vault was skipped or failed. See § Cross-Vault Partial Results. |

### Cross-Vault Partial Results

The `partial_results` envelope on the response surfaces per-vault skips and failures without failing the whole query. Field shape and semantics are pinned in [vault-management.md § Cross-Vault Search Semantics § Partial-Failure Handling](./vault-management.md#cross-vault-search-semantics).

```yaml
partial_results:
  skipped:
    - vault: "01951f6c-…"
      vault_name: "archive"
      status: "paused"
      reason: "vault is paused"
  failed:
    - vault: "01951f6d-…"
      vault_name: "external"
      code: "vault_search_failed"
      message: "index.sqlite: I/O error"
```

`partial_results` is omitted entirely when no vault was skipped or failed (additive wire change; v0/step-9 consumers ignoring the field continue to see the same `results` and `truncated` shape).

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

### Path collisions across vaults

Two vaults may contain a file at the same vault-relative path (e.g., `notes/index.md` in both). Both rows appear in `results`, ordered by `path` then `vault_id`. The `vault` + `vault_name` fields disambiguate. Operators who want a single result for that path should use the `vaults` filter to scope the query.

### Paused or errored vault in scope

A vault in `paused` or `errored` status is silently skipped; one entry per skipped vault is appended to `partial_results.skipped` with its current status and (for `errored`) the registry's `last_error` text. The `results` list contains only matches from active vaults.

---

## Open Questions

- [ ] Should symlinks inside the vault be indexed, or just followed for reads?
- [ ] Do we need a `regex` alternative to `glob`? — v0 ships glob-only; see [step-5 workplan § Deferred decision 5](../roadmap/step-05-workplan.md#5-regex-alternative-to-glob) — no field added, additive when needed.
- [ ] Should results include a `frontmatter` summary (title, tags) for quick triage?
- [ ] Pagination / cursor across N independent indexes — deferred to round 4+ per [vault-management.md § Open Questions](./vault-management.md#open-questions). Round 3 ships `truncated: bool` only; no cursor.
- [ ] Streaming response shapes (chunked HTTP / SSE / NDJSON) for high-vault-count deployments — deferred per [vault-management.md § Open Questions](./vault-management.md#open-questions).

---

## Revision History

| Version | Date | Changes |
|---------|------|---------|
| 0.1.0 | 2026-04-23 | Initial draft, seeded from project handoff v0 scope |
| 0.2.0 | 2026-04-27 | Multi-vault adoption (round 3 / step 10): `vault` semantics flipped from "always absent" to "populated when multi-vault active"; added `vault_name`, request-side `vaults` filter, response-envelope `partial_results`, global path-asc cross-vault ordering with `vault_id` tie-break. Cross-vault execution semantics cross-referenced from [vault-management.md](./vault-management.md). |
