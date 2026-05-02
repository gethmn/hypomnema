# Content Retrieval Specification

**Version**: 1.0.0
**Date**: 2026-05-02
**Status**: Approved

---

## Overview

Content retrieval answers the question: *give me this file.* A consumer hands the daemon one or more vault-relative paths and receives back the indexed file text plus its content metadata (`content_hash`, `size`, `mtime`, `vault`, `vault_name`). It is the natural follow-on to the three search modes — search answers *which file*, content retrieval answers *give me this file*.

The source of truth is the indexed `files.content` column in each per-vault `index.sqlite`. The operation **never reads from the vault filesystem at query time** — it queries the index exclusively. An agent that searched, received a `content_hash`, and then retrieved sees the same state the search saw, because both operations read from the same indexed column.

Content retrieval is read-only by definition — no v0 read/write boundary concerns.

**Related Documents**:
- [ADR-0004: Three Search Modes as Peers](../decisions/0004-three-search-modes-as-peers.md)
- [ADR-0009: Multi-Vault per Daemon](../decisions/0009-multi-vault-per-daemon.md)
- [ADR-0012: MCP transport — stdio in v0](../decisions/0012-mcp-transport-stdio-v0.md)
- [ADR-0013: MCP transport — Streamable HTTP on `hmnd`](../decisions/0013-mcp-transport-streamable-http.md)
- [Vault Management § Cross-Vault Search Semantics](./vault-management.md#cross-vault-search-semantics)
- [Content Search](./content-search.md) — same fan-out and partial-results conventions
- [MCP Streamable HTTP](./mcp-streamable-http.md) — `content_get` tool surface

---

## Request Schema

```yaml
paths:              # required; one or more vault-relative paths
  - "notes/file.md"
  - "notes/other.md"
vaults:             # optional; restrict to a named subset of vaults
  - "personal"
```

### `ContentGetRequest`

| Field | Type | Required | Default | Description |
|---|---|---|---|---|
| `paths` | array of strings | yes | — | One or more vault-relative paths to retrieve. Must not be empty. Each entry must be vault-relative (no leading `/`, no `..` segments). |
| `vaults` | array of strings | no | none → all active | Restrict retrieval to a named subset of vaults, each matched by name first, then surrogate ID. Empty array is rejected as `invalid_request`. Omitting (or `null`) queries all currently active vaults. |

---

## Response Schema

### `ContentGetResponse`

```yaml
results:
  - path: "notes/file.md"
    content: "# File\n\nBody text."
    content_hash: "sha256:abc123..."
    size: 4096
    mtime: "2026-05-02T10:00:00.000000Z"
    vault: "018f3a7c-9b4e-7d2a-95f1-c8a6e3b2d1f0"
    vault_name: "personal"
  - path: "notes/missing.md"
    vault: "018f3a7c-9b4e-7d2a-95f1-c8a6e3b2d1f0"
    vault_name: "personal"
    error:
      code: "path_not_found"
      message: "path not found in vault index"
partial_results:    # optional; present when vaults were skipped or failed
  skipped: [...]
  failed: [...]
```

| Field | Type | Required | Description |
|---|---|---|---|
| `results` | array | yes | One item per (path, vault) combination looked up. Items are success or error variants (untagged union). |
| `partial_results` | object | no | Present when one or more vaults were skipped or failed. Omitted when all vaults were queried cleanly. |

### `ContentGetResultItem` — success variant

| Field | Type | Description |
|---|---|---|
| `path` | string | Vault-relative path as stored in the index. |
| `content` | string | Full indexed file text. May contain U+FFFD replacement characters for non-UTF-8 byte sequences (see Implementation Notes). |
| `content_hash` | string | SHA-256 hash of the original file bytes, in `sha256:<hex>` form. Matches the hash returned by search operations for the same file at the same index state. |
| `size` | integer | File size in bytes at index time. |
| `mtime` | string | File modification time at index time, in RFC 3339 with microsecond precision and `Z` suffix. |
| `vault` | string | Surrogate UUID of the vault that owns this result. |
| `vault_name` | string | Human-readable vault name. |

### `ContentGetResultItem` — error variant

| Field | Type | Description |
|---|---|---|
| `path` | string | The path that was requested and was not found. |
| `vault` | string | Surrogate UUID of the vault in which the lookup was attempted. |
| `vault_name` | string | Human-readable vault name. |
| `error` | object | Per-item error detail: `{ code: string, message: string }`. |

---

## Validation Rules

All validation is applied before any index lookup runs.

| Rule | HTTP status | Error code |
|---|---|---|
| `paths` is empty | 422 | `invalid_request` |
| `vaults` is provided but empty | 422 | `invalid_request` |
| Any path starts with `/` (absolute path) | 422 | `invalid_path` |
| Any path contains a `..` segment (including internal `notes/../escape.md`) | 422 | `invalid_path` |
| Any path is an empty string | 422 | `invalid_path` |
| A vault name in `vaults` does not match any known vault | 404 | `vault_not_found` |

Paths beginning with `./` are accepted; the `./` prefix is stripped during normalization before the index lookup. Paths are not further normalized (e.g., double slashes are collapsed but the path is otherwise taken as-is).

---

## Cross-Vault Fan-Out Behavior

When no `vaults` filter is provided, retrieval fans out across all currently active vaults. Each path is looked up in each vault's index in parallel; results from all vaults are merged and sorted by `(path ASC, vault_id ASC)`.

**Path collisions across vaults**: if the same path exists in multiple vaults, each vault produces a separate result item. The merged result list may therefore contain more items than the number of requested paths.

**Vault matching**: explicit `vaults` entries are matched against vault name first, then surrogate ID. The first match wins. Unknown entries produce a `vault_not_found` entry in `partial_results.failed`; they are not a per-item error.

**Result ordering**: deterministic across identical requests — sorted by `(path ASC, vault_id ASC)` globally across all vaults. The order is not request-input-order.

**HTTP status on per-item errors**: 200 OK even when all items in the batch are `path_not_found`. A 4xx or 5xx is returned only for request-level validation failures or total vault retrieval failures, not for per-path misses.

---

## Paused/Errored Vault Handling

These rules mirror the cross-vault conventions defined in [vault-management.md § Cross-Vault Search Semantics](./vault-management.md#cross-vault-search-semantics) and apply uniformly across all retrieval operations.

### Default scope (no `vaults` filter)

- **Paused vault**: silently excluded from the retrieval. An entry is added to `partial_results.skipped` with `status: "paused"`.
- **Errored vault**: silently excluded from the retrieval. An entry is added to `partial_results.skipped` with `status: "errored"`.
- In both cases the caller receives results from active vaults only; no error is raised for the exclusion.

### Explicit scope (`vaults` filter names a paused or errored vault)

- **Paused vault explicitly requested**: retrieval is **attempted** from the vault's `index.sqlite` (the index is preserved across pause/resume). If the index is readable, results are returned normally. An entry is added to `partial_results.skipped` with `status: "paused"` to signal the vault is not actively watching.
- **Errored vault explicitly requested**: same as paused — attempt retrieval; if the index is readable, return results with a `partial_results.skipped` entry (`status: "errored"`).
- **Vault index unreadable** (e.g., corrupted or inaccessible SQLite file): the vault appears in `partial_results.failed` with code `vault_retrieval_failed` and the underlying error message. No per-path items are emitted for this vault.

### All-vaults-failed case

If the default scope is used and every active vault fails to serve results (all land in `partial_results.failed`), the response is HTTP 503 with top-level code `vault_retrieval_failed` and the failure detail in `partial_results.failed`.

---

## Implementation Notes

### Lossy UTF-8 decode

`files.content` is populated at index time using `String::from_utf8_lossy(&bytes).into_owned()` (see `src/indexer/hash.rs`). Invalid UTF-8 byte sequences in the source file are replaced with the U+FFFD replacement character (the Unicode substitution character). The retrieval response returns `content` verbatim from the index — no re-decode or re-read occurs at query time.

**Consequence for consumers**: if you retrieve a file that contained non-UTF-8 bytes on disk, the `content` field will have U+FFFD characters in place of those bytes. The `content_hash` is computed from the original raw bytes (before decode), so it reflects the on-disk file faithfully. Consumers that need byte-exact content must read the file directly from disk; content retrieval is a convenience surface for text-oriented consumers.

This behavior is consistent with [content-search.md](./content-search.md), which uses the same stored text for its index.

### `content_not_indexed` invariant

The indexer never inserts a `files` row with a NULL or empty `content` column. Every row insertion goes through `write_blocking`, which always writes the full file body as part of the same atomic transaction (see `src/indexer/mod.rs`). On embedding failure, the entire transaction is abandoned and no `files` row is committed — the file simply does not appear in the index at all.

**Consequence**: `path_not_found` is the only per-item not-found code. The `content_not_indexed` code does not exist in this spec — it would represent an unreachable state. If a path is in the index, its content is populated.

### Symlink path handling

The filesystem walker uses `WalkDir::follow_links(true)` (see `src/indexer/walk.rs`). For a symlink within the vault, the walker traverses the link and indexes the target file's content. The **symlink path** (not the target's real path) is stored in `files.path` — the relative path is stripped from the canonicalized vault root using the link path as `walkdir` presents it.

**Consequence**: symlinked files are retrieved by their **symlink path** within the vault. If a vault contains `link.md` (a symlink to `real.md`), both `link.md` and `real.md` appear as separate index entries, and both are retrievable by their respective paths. Symlinks pointing outside the vault are rejected by the walker's canonicalization check and do not appear in the index.

### Source of truth

All text returned by `content_get` comes exclusively from the `files.content` column in each per-vault `index.sqlite`. Query-time reads from the vault filesystem are prohibited. The handler never calls `fs::read`, `tokio::fs::read`, `File::open`, or any equivalent on vault paths. Verification: `rg 'fs::read|tokio::fs::read|File::open' src/api src/search` returns zero matches in the content-retrieval handler path.

---

## Error Codes

| Code | Scope | HTTP status | Description |
|---|---|---|---|
| `path_not_found` | Per item | 200 | The requested path does not exist in the vault's index. The batch continues; other items are unaffected. |
| `vault_not_found` | Request | 404 | A vault named in the `vaults` filter does not match any known vault (by name or surrogate ID). |
| `vault_retrieval_failed` | Per vault or top-level | 200 (per vault) / 503 (all vaults failed) | The vault's `index.sqlite` was unreachable or returned an unexpected error. Per-vault: the vault appears in `partial_results.failed`. Top-level: all targeted vaults failed. |
| `invalid_request` | Request | 422 | `paths` is empty, or `vaults` is provided but empty. |
| `invalid_path` | Request | 422 | A path in `paths` is absolute, contains a `..` segment, or is an empty string. Validation fails fast on the first invalid path. |

---

## Integration Points

### HTTP

- **Route**: `POST /content/get`
- **Request body**: JSON object matching `ContentGetRequest`
- **Response body**: JSON object matching `ContentGetResponse`
- **Success status**: 200 OK (even when all items are `path_not_found`)
- **Error statuses**: 422 for validation errors, 404 for unknown vault, 503 when all vaults failed

### MCP

- **Tool name**: `content_get`
- **Transports**: both stdio (`hmn mcp`) and Streamable-HTTP (`/mcp` route on `hmnd`)
- **Availability**: registered regardless of `[mcp] enable_write_tools` setting — this tool is read-only and is never gated by the write-tools flag
- **Input schema**: same `paths` + `vaults` fields as the HTTP request, auto-generated JSON Schema
- **Output**: `ContentGetResponse` JSON wrapped in MCP `CallToolResult`

Both MCP transports call the same in-process backend via the `HypomnemaBackend::content_get` trait method; there is no code duplication between transports.

### CLI

- **Invocation**: `hmn content get PATH... [--vault NAME|ID] [--json]`
- **Positional args**: one or more vault-relative paths
- **`--vault NAME|ID`**: optional, repeatable; restricts retrieval to named vaults (maps to the `vaults` field)
- **`--json`**: output the full `ContentGetResponse` envelope as JSON (default: human-readable format)
- **Human-readable output**: per-file header block (`PATH:`, `VAULT:`, `HASH:`, `SIZE:`, `MTIME:`) followed by file content, separated by `---` between files
- **Exit codes**: 0 on partial success (at least one item succeeded); non-zero when all items errored, the request was rejected, or no vault was reachable
- **Per-item errors**: printed to stderr; successful items printed to stdout

The CLI constructs a `ContentGetRequest` and POSTs to the daemon's `/content/get` route via the existing `DaemonClient` pattern.

---

## Revision History

| Version | Date | Changes |
|---|---|---|
| 1.0.0 | 2026-05-02 | Initial canonical spec. Promoted from `notes/proposals/content-retrieval.md`. Resolved deferred decisions: lossy UTF-8 documented, `content_not_indexed` confirmed unreachable (invariant documented), symlink path handling verified and documented (symlink path stored verbatim). |
