# FTS5 / BM25 Content Search Specification

**Version**: 0.1.0
**Date**: 2026-04-30
**Status**: Draft

---

## Overview

FTS5 / BM25 content search adds a ranked lexical path to `search_content` so agents can ask token-shaped questions like "notes mentioning vector indexes and SQLite" and get the most relevant files first, instead of scanning `files.content` with substring or regex and returning path order. The feature is motivated by the qmd comparison in [`notes/qmd-comparison.md`](../qmd-comparison.md): qmd's FTS5 + BM25 layer is a low-architecture-cost retrieval-quality upgrade Hypomnema can borrow because SQLite is already the per-vault store.

This is an additive amendment candidate for [`docs/specs/content-search.md`](../../docs/specs/content-search.md), not a new top-level search mode. `search_filesystem`, `search_content`, and `search_semantic` remain the three peer operations from ADR-0004. Exact substring and regex behavior remain available because the existing content-search contract is grep-shaped; ranked FTS5 answers a different lexical question and must not silently replace exact verification.

**Related Documents**:
- [ADR-0004: Three Search Modes as Peers](../../docs/decisions/0004-three-search-modes-as-peers.md)
- [ADR-0006: Daemon State Lives Outside the Watched Directory](../../docs/decisions/0006-outbox-outside-watched-directory.md)
- [ADR-0007: sqlite-vec over Lance, qdrant, and Other Vector Stores](../../docs/decisions/0007-sqlite-vec-over-alternatives.md)
- [Content Search Specification](../../docs/specs/content-search.md)
- [Architecture: Search API](../../docs/architecture/overview.md#search-api)
- [Implementation: spawn_blocking for rusqlite](../../docs/implementation/tech-stack.md#spawn_blocking-for-rusqlite)

---

## Behavior

### Normal Flow

1. Consumer sends `search_content` with a `query`, optional path prefix, optional vault scope, and optional `mode`.
2. Resolve in-scope vaults using the existing cross-vault search semantics.
3. For each active in-scope vault:
   - `mode: "substring"` uses the existing ASCII-case-folded substring matcher over stored `files.content`.
   - `mode: "regex"` uses the existing Rust `regex` matcher over stored `files.content`.
   - `mode: "ranked"` queries an FTS5 virtual table over indexed file bodies and computes `bm25(...)` scores.
4. Merge per-vault results:
   - substring and regex keep current path-ascending ordering.
   - ranked mode sorts by relevance score first, then `path`, then `vault_id` for deterministic ties.
5. Truncate the merged list to `limit`; return matching files with existing result metadata plus ranked-mode score fields.

### State Machine

**State Machine**: N/A -- content search is stateless at request time. FTS5 index freshness is owned by the existing indexer lifecycle: file upsert, delete, reset, and rescan update the content index transactionally with `files`.

---

## Data Schema

### Request

```yaml
query: "sqlite vector index"
mode: "ranked"                 # optional: substring | regex | ranked
case_sensitive: false          # substring only
prefix: "notes/databases/"     # optional
include_matches: true          # optional
max_matches_per_file: 5        # optional
limit: 100                     # optional
vaults:
  - "personal"
```

| Request Field | Type | Required | Default | Description |
|---|---|---:|---|---|
| `query` | string | yes | - | Search text. In `ranked` mode this is an FTS5 query string; in `substring` mode it is a literal substring; in `regex` mode it is a Rust regex pattern. |
| `mode` | enum string | no | `substring` | Matching strategy: `substring`, `regex`, or `ranked`. Existing `regex: true` maps to `mode: "regex"` for backward compatibility if both are not supplied. |
| `regex` | bool | no | false | Backward-compatible alias for `mode: "regex"`. Rejected if supplied with a conflicting `mode`. |
| `case_sensitive` | bool | no | false | Applies only to `substring`; ignored by `regex`; rejected for `ranked` if true because FTS5 tokenization owns case behavior. |
| `prefix` | string | no | none | Existing vault-relative path-prefix scope. |
| `include_matches` | bool | no | false | When true, response includes line snippets. Ranked mode snippets are derived from matched file content after FTS candidate selection. |
| `max_matches_per_file` | integer | no | 5 | Cap on per-file snippets when `include_matches: true`. |
| `limit` | integer | no | 100 | Global result cap after cross-vault merge. Validation: `1..=1000`. |
| `vaults` | array of strings | no | none -> all active | Existing cross-vault scope filter. |

### Ranked Response

```yaml
results:
  - path: "notes/databases/sqlite-vec.md"
    match_count: 4
    score: -2.173
    rank: 1
    vault: "01951f6c-7c3b-7a2e-8c1d-1a2b3c4d5e6f"
    vault_name: "personal"
    matches:
      - line: 18
        text: "sqlite-vec stores vectors in a vec0 virtual table."
truncated: false
```

| Field | Type | Required | Default | Description |
|---|---|---:|---|---|
| `path` | vault-relative path | yes | - | Existing content-search result path. |
| `match_count` | integer | yes | - | Number of line/snippet matches found while producing optional snippets. In ranked mode this is not the FTS term frequency and must not be used as rank. |
| `score` | number | ranked only | omitted | Raw SQLite `bm25(...)` value. Lower is better in SQLite FTS5; consumers should sort by response order, not reinterpret the score. |
| `rank` | integer | ranked only | omitted | 1-based rank after per-mode merge and final truncation. |
| `matches` | array | no | omitted | Existing match snippets when `include_matches: true`. |
| `vault` | string | no | existing behavior | Existing surrogate vault ID. |
| `vault_name` | string | no | existing behavior | Existing point-in-time vault name. |
| `truncated` | boolean | yes | - | Existing cross-vault truncation flag. |
| `partial_results` | object | no | omitted | Existing cross-vault partial-result envelope. |

### Persisted FTS Table

```sql
CREATE VIRTUAL TABLE files_fts USING fts5(
  path UNINDEXED,
  content,
  content='files',
  content_rowid='rowid',
  tokenize='porter unicode61'
);
```

| Column | Type | Required | Default | Description |
|---|---|---:|---|---|
| `rowid` | integer | yes | SQLite-managed | Mirrors the backing `files.rowid` for external-content synchronization. |
| `path` | text | yes | - | Vault-relative path for result projection and deterministic tie-breaks; not token-indexed. |
| `content` | text | yes | - | File body indexed by FTS5. |

### Validation Rules

- `mode` must be one of `substring`, `regex`, or `ranked`.
- `regex: true` and `mode: "regex"` are equivalent; `regex: true` with any other `mode` is `invalid_request`.
- `case_sensitive: true` with `mode: "ranked"` is `invalid_request`.
- Ranked mode rejects an empty or FTS-syntax-invalid `query` as `invalid_query`.
- Prefix, limit, vault-scope, paused/errored vault, and partial-result behavior reuse the existing content-search validation rules.

---

## Examples

### Example 1: Ranked lexical discovery

**Input**:

```yaml
query: "sqlite vector index"
mode: "ranked"
include_matches: true
limit: 3
```

**Behavior**: Hypomnema queries each active vault's `files_fts` table, scores rows with `bm25(files_fts)`, merges candidates by score, and returns the three best files.

**Result**: Files that discuss SQLite vector indexes rank above files that merely contain one isolated token. The response includes `score` and `rank`; exact score values are intentionally not stable across SQLite/tokenizer changes.

### Example 2: Exact verification still uses substring

**Input**:

```yaml
query: "sqlite-vec stores vectors"
mode: "substring"
include_matches: true
```

**Behavior**: Hypomnema uses the existing substring matcher over `files.content`, preserving phrase-across-line-boundary behavior and path-ascending ordering.

**Result**: Only files containing that literal phrase match. Ranked FTS is not used because this is the exact-verification shape the current content-search spec already promises.

---

## Edge Cases

### Tokenized search is not exact search

**Scenario**: A consumer searches `query: "sqlite vector index"` in `ranked` mode.

**Behavior**: FTS5 tokenizes, stems, and ranks terms; it may match inflected forms and does not mean "the exact byte substring `sqlite vector index` appears."

**Rationale**: This is the core reason ranked mode is additive. Replacing substring search outright would violate the grep-shaped promise in the existing content-search spec and make agents worse at quote verification.

### FTS index drift

**Scenario**: A migration creates `files_fts`, or a bug leaves `files` and `files_fts` out of sync.

**Behavior**: Migration rebuilds `files_fts` from `files`. Runtime upsert/delete operations update `files` and `files_fts` in the same transaction. A reset/rebuild clears and repopulates both indexes from the vault.

**Rationale**: The vault remains the source of truth, but request-time ranked search must not see stale rows after a file delete or rename.

### Prefix-scoped ranked search

**Scenario**: A ranked query includes `prefix: "notes/databases/"`.

**Behavior**: FTS5 selects ranked candidates and SQL applies the same normalized prefix range filter used by existing search. Results outside the prefix are excluded before per-vault limit and cross-vault merge.

**Rationale**: Prefix is a structural filter, not a ranking hint. It should narrow the candidate set before the response is capped.

### SQLite score direction

**Scenario**: A consumer tries to sort ranked results by `score` descending.

**Behavior**: The response order is authoritative. The spec documents that lower `bm25(...)` values are better and exposes `rank` to prevent score-direction ambiguity.

**Rationale**: SQLite FTS5's BM25 sign convention is easy to misuse. `rank` gives consumers a stable display field while still exposing raw scores for debugging.

---

## Error Handling

| Error Condition | Error Code/Type | Message | Recovery |
|---|---|---|---|
| `mode` is not recognized | `invalid_request` | `mode must be one of substring, regex, ranked` | Fix the request. |
| `regex` conflicts with `mode` | `invalid_request` | `regex=true cannot be combined with mode=<mode>` | Use either legacy `regex` or explicit `mode`. |
| `case_sensitive: true` with ranked mode | `invalid_request` | `case_sensitive is only supported for substring mode` | Use substring mode or remove the flag. |
| Ranked FTS query syntax is invalid | `invalid_query` | `invalid FTS query: <sqlite message>` | Escape or simplify the query. |
| Existing regex compile failure | `invalid_regex` | Existing regex error text | Fix the regex pattern. |
| Existing invalid prefix | `invalid_prefix` | Existing prefix error text | Use a vault-relative prefix without `..` or absolute path segments. |
| Per-vault storage/search failure | `vault_search_failed` | Existing partial-results message | Inspect daemon logs or retry after rescan/reset. |

---

## Integration Points

### Store Schema

`files_fts` lives in each per-vault `index.sqlite`, outside the watched vault. The schema should use SQLite FTS5 as bundled by `rusqlite`'s bundled SQLite build; no new daemon process, vector service, or crate-level search backend is introduced. If the implementation chooses an external-content FTS table, every mutation of `files` must keep the FTS row synchronized in the same SQL transaction.

### Indexer

File insert/update/delete flows update `files`, `files_fts`, chunks, and `chunks_vec` inside the existing `spawn_blocking` SQL sections. Embedding generation remains async and outside `spawn_blocking`; FTS maintenance is SQLite work and stays inside the blocking closure.

### Search API

HTTP `/search/content`, stdio MCP `search_content`, and HTTP MCP `search_content` expose the same request/response shape. Transport layers must not fork ranked-mode behavior. Cross-vault fan-out, partial results, skipped vaults, and limit semantics reuse `vault-management.md` cross-vault rules.

### CLI

`hmn search content` needs a way to request ranked mode without disturbing existing positional query use. A likely shape is `hmn search content "sqlite vector index" --mode ranked`; legacy `--regex` remains accepted and maps to `--mode regex`.

---

## Implementation Notes

- All SQL, including FTS5 queries and index maintenance, must run inside `tokio::task::spawn_blocking`; acquire the r2d2 connection inside the closure.
- Do not add a fourth search operation. This belongs under `search_content` unless a future ADR amends ADR-0004.
- Prefer an external-content FTS5 table keyed to `files.rowid` so stored content remains single-sourced in `files.content`. If workplan investigation shows rowid coupling is too brittle, a contentless or duplicated-content FTS table is acceptable only with an explicit rebuild path.
- Ranked result order is by `bm25` ascending, then `path`, then `vault_id`; exact scores are diagnostic, not a stable API promise.
- Negative fingerprints after implementation: `rg "SELECT path, content FROM files" src/search/content.rs` should not find the ranked-mode query path, and `rg "ORDER BY path ASC" src/search/content.rs` should not be the only ordering used by content search.
- See peer stories in [`fts5-bm25-content-search-stories.md`](./fts5-bm25-content-search-stories.md).

---

## Open Questions

- [ ] Workplan-time benchmark: should `mode: "ranked"` become the CLI default later, or should `substring` remain default indefinitely? This needs a small fixture-based comparison because the current vision emphasizes exact verification while the qmd comparison emphasizes retrieval quality.
- [ ] Workplan-time tokenizer choice: `porter unicode61` is the draft default because qmd uses Porter stemming and Markdown notes are mostly prose, but code-heavy vaults may prefer `unicode61` without stemming.
- [ ] Future spec: should `search_filesystem` gain ranked path/name search over file paths and headings? This proposal is limited to file contents.

---

## Revision History

| Version | Date | Changes |
|---|---|---|
| 0.1.0 | 2026-04-30 | Initial draft proposal for additive FTS5/BM25 ranked mode inside content search. |
