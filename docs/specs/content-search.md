# Content Search Specification

**Version**: 0.2.0
**Date**: 2026-04-27
**Status**: Draft

---

## Overview

Content search answers exact-string questions: *which files contain this phrase?* It is grep-shaped — queries are strings or regexes; results are files (optionally with matching lines). This is the search mode an agent uses to verify a reference or find a specific quote.

**Related Documents**:
- [ADR-0004: Three Search Modes as Peers](../decisions/0004-three-search-modes-as-peers.md)
- [ADR-0009: Multi-Vault per Daemon](../decisions/0009-multi-vault-per-daemon.md)
- [Vault Management § Cross-Vault Search Semantics](./vault-management.md#cross-vault-search-semantics)
- [Architecture: Search API](../architecture/overview.md#search-api)

---

## Behavior

### Normal Flow

1. Consumer sends a query (substring or regex) with optional path filter and optional `vaults` scope
2. Resolve in-scope vaults: `vaults` filter narrows to the named subset; otherwise all currently active vaults
3. For each in-scope vault: query the content index in the vault's `index.sqlite`
4. Merge per-vault results by path (ascending, byte-lexicographic); break ties by `vault_id`
5. Truncate the merged list to `limit`; return matching files (optionally with line-level matches) and per-result `vault` + `vault_name` when multi-vault is active

File text is stored inside each per-vault SQLite store as part of the indexer's work — content search does not re-read files on every query.

### Semantics

- Default: case-insensitive substring match (ASCII-folded; Unicode case folding is not applied today).
- Optional: case-sensitive mode
- Optional: regex mode using the Rust `regex` crate's default Unicode flavor. The request's `case_sensitive` flag is ignored when `regex: true`; case-sensitivity is a property of the pattern (`(?i)foo`).
- A file matches if it contains at least one occurrence of the query
- Phrase searches span line boundaries — the matcher operates over the file's full byte content, not per-line. See [step-5 workplan § Deferred decision 3](../roadmap/step-05-workplan.md#3-phrase-search-across-line-boundaries).

### Cross-Vault Behavior

Cross-vault execution semantics — vault scoping, ordering, partial-failure handling, paused/errored vault inclusion, fan-out model — are pinned in [vault-management.md § Cross-Vault Search Semantics](./vault-management.md#cross-vault-search-semantics) and apply uniformly across the three search modes. The summary that's content-search-specific:

- **Default scope**: all currently active vaults; per-result `vault` + `vault_name` disambiguate origin.
- **Ordering**: global path-ascending across vaults (lifted from the original single-vault path-asc behavior; the merged list is sorted as a single slice). Identical paths across two vaults break ties by `vault_id`.
- **`limit`**: each vault contributes up to `limit` results to the merge pool; the merged list is then truncated to `limit`. `truncated: true` is set if any per-vault search reported truncation **or** the merged list was capped.
- **`vaults` filter**: `Some([...])` narrows to the named subset; `None` queries all active vaults; `Some([])` is a request validation error.

For N=1 (single-vault deployment) the cross-vault wire shape collapses to legacy single-vault semantics.

---

## Data Schema

### Request

```yaml
query: "pgvector"             # required
regex: false                   # optional; if true, query is a regex
case_sensitive: false          # optional
prefix: "notes/databases/"     # optional; restrict to a subdirectory
include_matches: true          # optional; return matching lines
max_matches_per_file: 5        # optional; default 5
limit: 100                     # optional; default 100
vaults:                        # optional; multi-vault scoping
  - "personal"
  - "work"
```

| Request Field | Type | Required | Default | Description |
|---|---|---|---|---|
| `query` | string | yes | — | Substring or regex pattern. |
| `regex` | bool | no | false | Treat `query` as a Rust-regex pattern. |
| `case_sensitive` | bool | no | false | Ignored when `regex: true`. |
| `prefix` | string | no | none | Path-prefix scope. |
| `include_matches` | bool | no | false | When true, response includes per-line `matches`. |
| `max_matches_per_file` | integer | no | 5 | Cap on per-file matches when `include_matches: true`. |
| `limit` | integer | no | 100 | Global result cap after cross-vault merge. Validation: `1..=1000`. |
| `vaults` | array of strings | no | none → all active | Subset of vaults to query, by name or surrogate ID. Empty array is rejected as `invalid_request`. |

### Response

```yaml
results:
  - path: "notes/databases/pgvector.md"
    match_count: 7
    vault: "01951f6c-7c3b-7a2e-8c1d-1a2b3c4d5e6f"
    vault_name: "personal"
    matches:
      - line: 12
        text: "pgvector supports HNSW and IVF indexes."
      - line: 45
        text: "Compared to pgvector, sqlite-vec trades features for portability."
truncated: false
# `partial_results` omitted in the all-success / all-active case
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `path` | string | yes | Vault-relative path |
| `match_count` | integer | yes | Total matches in the file (may exceed `matches.len()` when `max_matches_per_file` truncates) |
| `matches` | array | no | Per-line match details when `include_matches: true`; omitted otherwise |
| `vault` | string | no | Surrogate vault ID (UUIDv7). Populated when multi-vault is active; omitted only by legacy single-vault wire shapes. |
| `vault_name` | string | no | Mutable, point-in-time-accurate display name for the source vault. Populated alongside `vault`. Never appears in live change events (see [change-events.md](./change-events.md)). |
| `truncated` | boolean | yes | True if any per-vault search reported truncation OR the merged list exceeded `limit`. |
| `partial_results` | object | no | Cross-vault diagnostic; present only when at least one vault was skipped or failed. See § Cross-Vault Partial Results. |

### Cross-Vault Partial Results

Same shape as filesystem-search; pinned in [vault-management.md § Cross-Vault Search Semantics § Partial-Failure Handling](./vault-management.md#cross-vault-search-semantics).

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

`partial_results` is omitted entirely when no vault was skipped or failed (additive wire change; older consumers ignoring the field continue to see the same `results` and `truncated` shape).

---

## Edge Cases

### Binary or very large files

Not a current concern: only Markdown files are indexed.

### Query too broad

If `limit` is exceeded after cross-vault merge, results are truncated and `truncated: true` is set. There is no pagination today.

### Regex with catastrophic backtracking

Rust's `regex` crate does not support backreferences and has linear-time matching, so pathological patterns are not a current DoS concern.

### Lossy UTF-8

Invalid UTF-8 byte sequences in file bodies are decoded with `String::from_utf8_lossy` before storage (replacement char `U+FFFD` substituted in). Matches against the lossy form are still surfaced — vault hygiene problems become searchable-but-noisy rather than indexer crashes. The `content_hash` continues to be computed over the raw bytes; lossy decode is a storage-side concern only.

### Path collisions across vaults

Two vaults may contain a file at the same vault-relative path. Both rows appear in `results`, ordered by `path` then `vault_id`. Operators who want a single result for that path should use the `vaults` filter to scope the query.

### Paused or errored vault in scope

A vault in `paused` or `errored` status is silently skipped; one entry per skipped vault is appended to `partial_results.skipped` with its current status and (for `errored`) the registry's `last_error` text.

---

## Open Questions

- [x] Should we support phrase search across line boundaries? (Probably yes — Markdown prose wraps.) — Resolved in step 5 as line-agnostic matching. See [step-5 workplan § Deferred decision 3](../roadmap/step-05-workplan.md#3-phrase-search-across-line-boundaries).
- [ ] Should frontmatter-only matches be distinguishable from body matches?
- [ ] Pagination / cursor across N independent indexes — deferred to round 4+ per [vault-management.md § Open Questions](./vault-management.md#open-questions). Round 3 ships `truncated: bool` only; no cursor.
- [ ] Streaming response shapes (chunked HTTP / SSE / NDJSON) for high-vault-count deployments — deferred per [vault-management.md § Open Questions](./vault-management.md#open-questions).

---

## Revision History

| Version | Date | Changes |
|---------|------|---------|
| 0.1.0 | 2026-04-23 | Initial draft, seeded from project handoff v0 scope |
| 0.2.0 | 2026-04-27 | Multi-vault adoption (round 3 / step 10): `vault` semantics flipped from "always absent" to "populated when multi-vault active"; added `vault_name`, request-side `vaults` filter, response-envelope `partial_results`, global path-asc cross-vault ordering with `vault_id` tie-break. Cross-vault execution semantics cross-referenced from [vault-management.md](./vault-management.md). |
