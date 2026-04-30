# Content Retrieval Specification

**Version**: 0.1.0
**Date**: 2026-04-30
**Status**: Draft

---

## Overview

Content retrieval answers direct-fetch questions: *give me the full text of this file*. Where the three search modes discover files by shape, string, or semantic similarity, content retrieval fetches the stored indexed content for one or more vault-relative paths the caller already knows. It is the operation an agent uses after discovering a file through search and wanting its complete text without re-reading the vault filesystem directly.

The source of truth is the indexed `files.content` column in the per-vault `index.sqlite`. The operation never reads from the vault filesystem at query time; it queries only the daemon's own store. This preserves index-consistency: an agent that searched, received a `content_hash`, and then retrieves content sees the same state the search saw — not a potentially-newer on-disk version.

Content retrieval is additive to the three peer search modes (per [ADR-0004](../decisions/0004-three-search-modes-as-peers.md)) — it is not a fourth search mode. The three search modes answer *which file*; content retrieval answers *give me this file*.

**Related Documents**:
- [ADR-0003: Indexing in the Daemon](../decisions/0003-indexing-in-the-daemon.md)
- [ADR-0004: Three Search Modes as Peers](../decisions/0004-three-search-modes-as-peers.md)
- [ADR-0009: Multi-Vault per Daemon](../decisions/0009-multi-vault-per-daemon.md)
- [ADR-0012: MCP Transport — stdio v0](../decisions/0012-mcp-transport-stdio-v0.md)
- [ADR-0013: MCP Transport — Streamable HTTP](../decisions/0013-mcp-transport-streamable-http.md)
- [Architecture: Search API](../architecture/overview.md#search-api)
- [Vault Management § Cross-Vault Search Semantics](./vault-management.md#cross-vault-search-semantics)
- [filesystem-search.md](./filesystem-search.md) · [content-search.md](./content-search.md) · [semantic-search.md](./semantic-search.md)
- **User Stories**: `notes/proposals/content-retrieval-stories.md`

---

## Behavior

### Normal Flow

1. Consumer sends a `content_get` request with one or more vault-relative paths and an optional `vaults` scope selector.
2. Resolve in-scope vaults: `vaults` filter narrows to the named subset; otherwise all currently active vaults. Active vaults whose index is readable are queried; paused and errored vaults whose index is readable are queried when explicitly selected (see § Paused / Errored Vault Behavior), otherwise skipped.
3. For each in-scope vault: look up each requested path in `files` by `(vault_id, path)`. A match produces a result item carrying `path`, `content`, `content_hash`, `size`, `mtime`, `vault`, and `vault_name`. A non-match produces a per-item `not_found` error entry in the same `results` array.
4. Collect all per-vault result items. For each path, if multiple vaults contain a match, all matches appear as separate result items — same behavior as search path-collision semantics (see [vault-management.md § Cross-Vault Search Semantics](./vault-management.md#cross-vault-search-semantics)).
5. Preserve request-input order within each vault's results. Across vaults, sort result items by `path` ascending, then `vault_id` ascending as the tie-break (mirroring content-search ordering).
6. Return the assembled `results` array; include `partial_results` only when at least one in-scope vault was skipped or failed.

**Content source**: the indexed `files.content` column, not a live filesystem read. Content reflects the state at the most recent index of that file; `content_hash` and `mtime` are from the same indexed row and are consistent with each other.

### State Machine

**State Machine**: N/A — this feature is stateless. Each request is an independent read against the per-vault store.

---

## Data Schema

### Request

```yaml
paths:
  - "notes/databases/pgvector.md"
  - "notes/databases/sqlite.md"
vaults:                              # optional; multi-vault scoping
  - "personal"
```

| Request Field | Type | Required | Default | Description |
|---|---|---|---|---|
| `paths` | array of strings | yes | — | One or more vault-relative paths to retrieve. Must be non-empty. Each path must be vault-relative (no leading `/`, no `..` segments). |
| `vaults` | array of strings | no | none → all active | Subset of vaults to query, by name or surrogate ID. Empty array is rejected as `invalid_request`. |

### Response

```yaml
results:
  - path: "notes/databases/pgvector.md"
    content: "# pgvector\n\nPostgreSQL extension for vector similarity…"
    content_hash: "sha256:abc123…"
    size: 4821
    mtime: "2026-04-22T14:31:08Z"
    vault: "01951f6c-7c3b-7a2e-8c1d-1a2b3c4d5e6f"
    vault_name: "personal"
  - path: "notes/databases/sqlite.md"
    error:
      code: "path_not_found"
      message: "notes/databases/sqlite.md not found in vault personal"
    vault: "01951f6c-7c3b-7a2e-8c1d-1a2b3c4d5e6f"
    vault_name: "personal"
# `partial_results` omitted in the all-vaults-active / no-skipped case
```

| Field | Type | Present | Description |
|---|---|---|---|
| `path` | vault-relative path string | always | The vault-relative path that was requested. |
| `content` | string | on success | Raw indexed file text (stored `files.content`); frontmatter is not parsed separately in v0. |
| `content_hash` | `sha256:` string | on success | SHA-256 hash of the file content at last index time. Consistent with `content`. |
| `size` | integer | on success | File size in bytes at last index time. |
| `mtime` | ISO-8601 string | on success | Last modification time at last index time. |
| `vault` | UUIDv7 string | always | Surrogate vault ID. |
| `vault_name` | string | always | Point-in-time display name for the source vault. Never appears in live change events. |
| `error` | object | on failure | Per-item error. Present instead of `content`/`content_hash`/`size`/`mtime` when the path was not found or the vault was skipped. |
| `error.code` | string | on failure | Error code. See § Error Handling. |
| `error.message` | string | on failure | Human-readable message. |
| `partial_results` | object | conditional | Cross-vault diagnostic; present only when at least one vault was skipped or failed. Same shape as in filesystem-search and content-search specs. |

### Cross-Vault Partial Results

Same shape as the search specs; pinned in [vault-management.md § Cross-Vault Search Semantics § Partial-Failure Handling](./vault-management.md#cross-vault-search-semantics).

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
      code: "vault_retrieval_failed"
      message: "index.sqlite: I/O error"
```

### CLI Shape

```sh
hmn content get "notes/databases/pgvector.md" [--vault NAME|ID] [--json]
hmn content get "notes/One.md" "notes/Two.md" --vault personal
```

- Without `--json`, human-readable output: file metadata header + content body, separated by `---` when multiple files.
- With `--json`, structured output matching the HTTP response envelope.
- `--vault` is optional; default behavior queries all active vaults.

### Validation Rules

- `paths` must be non-empty; an empty array is `invalid_request`.
- Each path must be vault-relative: no leading `/`, no `..` segments. Absolute paths and paths with `..` are rejected with `invalid_path`.
- `vaults` must be non-empty if provided; an empty `vaults` array is `invalid_request`.
- `paths` length has no hard cap in v0; very large batches are an operator concern (no pagination or streaming in v0).

---

## Examples

### Example 1: Single-file retrieval, single vault

**Input**:
```yaml
paths:
  - "notes/databases/pgvector.md"
vaults:
  - "personal"
```

**Behavior**: Daemon queries the `personal` vault's `index.sqlite` for the row with `path = "notes/databases/pgvector.md"`. The row exists; its `content`, `content_hash`, `size`, and `mtime` are returned.

**Result**:
```yaml
results:
  - path: "notes/databases/pgvector.md"
    content: "# pgvector\n\nPostgreSQL extension…"
    content_hash: "sha256:abc123…"
    size: 4821
    mtime: "2026-04-22T14:31:08Z"
    vault: "01951f6c-7c3b-7a2e-8c1d-1a2b3c4d5e6f"
    vault_name: "personal"
```

### Example 2: Batch request, mixed results, multi-vault fan-out

**Input**:
```yaml
paths:
  - "notes/index.md"
  - "notes/missing-file.md"
# no vaults selector → all active vaults
```

**Behavior**: Two active vaults (`personal`, `work`). `notes/index.md` exists in both; `notes/missing-file.md` exists in neither. The daemon fans out to both vaults. Result items for `notes/index.md` appear twice (once per vault). `notes/missing-file.md` appears twice as `path_not_found` error items (once per vault). Items are ordered by path ascending, then vault_id ascending.

**Result**:
```yaml
results:
  - path: "notes/index.md"
    content: "# Index…"
    content_hash: "sha256:abc…"
    size: 312
    mtime: "2026-04-20T10:00:00Z"
    vault: "01951f6c-…"
    vault_name: "personal"
  - path: "notes/index.md"
    content: "# Work Index…"
    content_hash: "sha256:def…"
    size: 512
    mtime: "2026-04-21T08:00:00Z"
    vault: "01951f6d-…"
    vault_name: "work"
  - path: "notes/missing-file.md"
    error:
      code: "path_not_found"
      message: "notes/missing-file.md not found in vault personal"
    vault: "01951f6c-…"
    vault_name: "personal"
  - path: "notes/missing-file.md"
    error:
      code: "path_not_found"
      message: "notes/missing-file.md not found in vault work"
    vault: "01951f6d-…"
    vault_name: "work"
```

---

## Edge Cases

### Path collision across vaults

**Scenario**: Two active vaults both contain `notes/index.md`. No `vaults` selector is provided.

**Behavior**: Both files are returned as separate result items, ordered by `path` ascending then `vault_id` ascending. The consumer uses `vault` and `vault_name` to disambiguate. If the consumer only wants the file from one vault, they should supply the `vaults` selector.

**Rationale**: Mirrors content-search and filesystem-search path-collision semantics. Silent deduplication would lose data; erroring on collision would break callers with multi-vault setups who want a broad fetch. Explicit vault selection is the right disambiguation surface.

### All requested paths missing from the selected vault

**Scenario**: `paths: ["notes/a.md"]`, `vaults: ["personal"]`, and the file does not exist in `personal`'s index.

**Behavior**: The response contains one result item with `error.code: "path_not_found"`. The top-level response is `200 OK` (the request was valid; the lookup produced a definitive not-found answer). There is no `partial_results` entry — the vault was active and reachable; the path simply wasn't there.

**Rationale**: Per-item failure is not a request-level failure. The operation succeeded; the datum is absent. `200 OK` with a per-item error matches the batch-request discipline established in the response shape.

### Paused vault — explicitly selected

**Scenario**: Consumer requests `vaults: ["archive"]` and `archive` is paused.

**Behavior**: The daemon queries `archive`'s `index.sqlite` if the file is accessible. If the query succeeds, the result item is returned normally. An entry for `archive` is also added to `partial_results.skipped` with `status: "paused"` and a note that the content may not reflect recent changes (the vault's watcher is inactive). If the index is not accessible, a `partial_results.failed` entry is added instead, and no result item is returned for that path+vault pair.

**Rationale**: Paused means the watcher is stopped, not that the index is gone. Consumers that explicitly target a paused vault have expressed intent to retrieve archived content. Silently erroring would be surprising; silently serving without a freshness warning would be misleading. The `partial_results.skipped` flag preserves honest communication without blocking the retrieval.

### Errored vault — explicitly selected

**Scenario**: Consumer requests `vaults: ["broken"]` and `broken` is in `errored` state.

**Behavior**: The daemon attempts to open `broken`'s `index.sqlite`. If the file is readable, result items are returned normally with an entry in `partial_results.skipped` (status `"errored"`, `last_error` text included in `reason`). If `index.sqlite` is inaccessible, a `partial_results.failed` entry is added and no result items are returned for that vault.

**Rationale**: The vault's error may be transient (e.g., watcher died but the index is intact). Serving from the index when it's readable is more useful than a hard block; the error state is surfaced via `partial_results.skipped` so the consumer knows the index may be stale.

### Paused or errored vault in the default scope

**Scenario**: No `vaults` selector; one of three active vaults is paused.

**Behavior**: The paused vault is silently skipped (no results contributed; one entry in `partial_results.skipped`). Active vaults continue normally. This mirrors search behavior per [vault-management.md § Cross-Vault Search Semantics](./vault-management.md#cross-vault-search-semantics).

**Rationale**: Consistent with search semantics. Non-explicitly-selected paused/errored vaults are excluded from the default scope; only when the caller explicitly names them does the "serve if readable" logic apply.

### Content not yet indexed (file very recently added)

**Scenario**: A file was just added to the vault; the watcher/indexer has not yet processed it.

**Behavior**: The path lookup returns nothing; the result item has `error.code: "path_not_found"`. The consumer should retry after a moment or subscribe to change events to detect when the file is indexed.

**Rationale**: The operation reads from the index, not the filesystem. If the file is not in the index, it is not retrievable. This is the correct behavior — indexing is asynchronous; the index is the source of truth.

---

## Error Handling

| Error Condition | Error Code | HTTP Status | Message | Recovery |
|---|---|---|---|---|
| `paths` is empty | `invalid_request` | 422 | "`paths` must be non-empty" | Supply at least one path. |
| `vaults` filter is an empty array | `invalid_request` | 422 | "`vaults` must be non-empty if provided" | Omit `vaults` to search all active vaults, or supply at least one name. |
| A path contains a leading `/` or `..` segment | `invalid_path` | 422 | `"path must be vault-relative: no leading '/' or '..' segments"` | Supply a vault-relative path. |
| Requested path not found in the vault's index | `path_not_found` | 200 (per-item error in `results`) | `"<path> not found in vault <name>"` | Wait for indexing to complete, or verify the path with filesystem search. |
| Named vault not found in the registry | `vault_not_found` | 404 | `"vault '<name>' not found"` (with optional closest-name hint) | Check vault name via `hmn vault list`. |
| Vault is active but its `index.sqlite` has an I/O error mid-query | `vault_retrieval_failed` | 200 (via `partial_results.failed`) | `"index.sqlite: <I/O error>"` | Vault may need `reset`; check `hmn vault status`. |
| All requested vaults failed (none produced any result) | `vault_retrieval_failed` | 503 | `"all targeted vaults failed; see partial_results"` | At least one vault must be queryable for a successful response. |

---

## Integration Points

### Store (per-vault `index.sqlite`, `files` table)

The retrieval operation reads from the `files` table in each targeted vault's `index.sqlite`. The relevant columns are:

| Column | Used by content retrieval |
|---|---|
| `path` | Lookup key (vault-relative path) |
| `content` | Returned verbatim as the `content` field |
| `content_hash` | Returned in the result item; primary freshness signal |
| `size` | Returned in the result item |
| `mtime` | Returned in the result item |

The `chunks` and `chunks_vec` tables are not read by this operation. Content retrieval reads the full file body, not the chunked semantic representation.

All rusqlite access goes through `tokio::task::spawn_blocking` per the load-bearing rule. See `.claude/skills/rusqlite-in-async/`.

**Data flow**:
```
Consumer request (paths[], vaults?) 
  → HypomnemaBackend::content_get
    → per-vault: spawn_blocking → SELECT path, content, content_hash, size, mtime FROM files WHERE path = ?
    → assemble per-vault items
  → merge + sort by (path ASC, vault_id ASC)
  → Response envelope
```

### HypomnemaBackend Trait

`content_get` must be added to the `HypomnemaBackend` trait so both MCP transports (stdio shim via `DaemonClient` and the in-process `InProcessBackend`) call the same handler. Neither transport forks the behavior; they differ only in how they frame the call.

### HTTP Transport (`hmnd` Axum router)

New route: `POST /content/get`. Accepts the JSON request body; returns the JSON response envelope. Mirrors the existing `/search/content` route pattern. Bound on the same listener as `/search/*` and `/vaults/*`.

### MCP Transports (stdio + Streamable HTTP)

New tool: `content_get`. Exposed on both the stdio shim (`hmn mcp`) and the in-process HTTP-MCP handler (`/mcp` on `hmnd`). Tool name is `content_get` to mirror `search_content`. Registered alongside the three search tools and the vault-management tools. Per [ADR-0012](../decisions/0012-mcp-transport-stdio-v0.md) and [ADR-0013](../decisions/0013-mcp-transport-streamable-http.md), both transports share implementation via the backend trait.

The `content_get` tool is a read-only tool and does NOT fall under the `[mcp] enable_write_tools` gate — it must always be available regardless of that config setting.

### CLI (`hmn` binary)

New subcommand: `hmn content get PATH... [--vault NAME|ID] [--json]`. Calls `POST /content/get` on the daemon over loopback HTTP. Human-readable output (without `--json`) formats each file as:

```
=== notes/databases/pgvector.md (vault: personal, 4821 bytes, sha256:abc123…)
<content>
```

When multiple files: separate file outputs with `---`.

Error results in the response are printed to stderr with a non-zero exit code only when **all** items errored; partial success (some items found, some not) exits 0 with per-item error lines on stderr.

### `mcp-streamable-http.md` spec

The tool surface table in [mcp-streamable-http.md](./mcp-streamable-http.md) should be updated to include `content_get` as a read-only tool.

---

## Implementation Notes

- **Source of truth is the index, not the filesystem.** The handler must query `files.content` from `index.sqlite`, not read from the vault path at query time. This is non-negotiable: live filesystem reads break the "stale-search / fresh-get" consistency the operation promises.

- **All rusqlite access via `spawn_blocking`.** No exceptions. See `.claude/skills/rusqlite-in-async/`.

- **Fan-out pattern**: follows the same cross-vault fan-out as `search_content` — `tokio::join_all` over per-vault async tasks, each taking an `Arc` clone of the vault runner before going async. See [architecture overview § Search API](../architecture/overview.md#search-api) for the Arc-clone + op_lock non-interference pattern.

- **Paused/errored vault resolution**: when a vault is explicitly named in `vaults`, the handler must attempt the index read regardless of vault status, then include the vault in `partial_results.skipped` alongside any successful results. The check is: can `index.sqlite` be opened? If yes, query. If no, add to `partial_results.failed`.

- **Request ordering vs. result ordering**: the response is sorted by `(path ASC, vault_id ASC)` across vaults, not by request-input order. This is consistent with search result ordering. Callers that care about request order must re-map by `path` + `vault`.

- **Negative-fingerprint check**: after implementation, `rg 'fs::read\|tokio::fs::read\|File::open' src/search/` or the equivalent handler path should return zero matches related to content retrieval — the handler must never open vault-path files directly.

- **CLI exit code**: `hmn content get` exits non-zero only when all requested paths errored or the request itself failed (validation error, no vaults reachable). Partial success (some paths found, some not) exits 0. Per-item `path_not_found` items are printed to stderr.

- **Type shape for per-item result**: in Rust the result item is likely a tagged enum (or a struct with `Option<content fields>` + `Option<error>`) serialized with `#[serde(untagged)]` or a discriminant field. Either shape is fine; the wire representation must match the schema above.

---

## Open Questions

- [ ] **Response content encoding**: `files.content` stores the raw UTF-8 bytes with lossy decode substitution applied at index time (per content-search spec). The retrieval response returns this stored string. Should v0 document the lossy-decode behavior explicitly in the content retrieval response, or defer the note to a future spec on index quality? — Workplan-time resolution; note it in the implementation workplan.

- [ ] **`path_not_found` vs `content_not_indexed`**: should there be a distinction between "this path is not in the files table at all" and "this path is in the files table but `content` is NULL or empty" (which can happen if the indexer ran but the content column was not populated)? — Unlikely edge case in v0 but worth confirming during implementation. If it can happen, the error code should be `content_not_indexed` for the second case.

- [ ] **Symlink handling**: filesystem-search follows symlinks to files within the vault root; should content retrieval also serve symlinked files from the index? This is transparent if the indexer already indexed them — it depends on what `files.path` stores (the symlink path or the real path). Resolve during implementation by checking the indexer's path-recording behavior.

- [ ] **`mcp-streamable-http.md` tool surface table amendment**: tracked here as an explicit cross-spec follow-up. After this spec is approved, [mcp-streamable-http.md](./mcp-streamable-http.md) should be amended to list `content_get`. This is a minor doc amendment, not a behavior change.

---

## Revision History

| Version | Date | Changes |
|---|---|---|
| 0.1.0 | 2026-04-30 | Initial draft from spec-generator session; all decisions locked with author |
