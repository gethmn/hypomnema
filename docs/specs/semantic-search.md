# Semantic Search Specification

**Version**: 0.4.0
**Date**: 2026-05-08
**Status**: Draft

---

## Overview

Semantic search answers conceptual-similarity questions: *what in this vault is similar to this idea?* The query is embedded into a vector via the same model used for indexing, and compared against the stored chunk vectors by cosine similarity.

**Result granularity** (Step 25): responses can be delivered at two levels:

- **`chunk`** — flat list of individual chunks ranked by cosine similarity. Pre-Step-25 behavior; answers *"which passages are relevant?"*
- **`document`** (default) — chunks are grouped by parent file; each document result includes representative evidence chunks and a composite document score. Answers *"which notes are relevant?"* and reduces redundancy when many top-scoring chunks come from the same file.

The `granularity` request field controls which shape is returned.

The data substrate this spec reads from — the `chunks` metadata table and the `chunks_vec` virtual table — ships in step 6. The query handler (`POST /search/semantic` / `hmn search semantic`) ships in step 7.

**Related Documents**:
- [ADR-0003: Indexing in the Daemon](../decisions/0003-indexing-in-the-daemon.md)
- [ADR-0004: Three Search Modes as Peers](../decisions/0004-three-search-modes-as-peers.md)
- [ADR-0005: Local Everything](../decisions/0005-local-everything.md)
- [ADR-0007: sqlite-vec over Alternatives](../decisions/0007-sqlite-vec-over-alternatives.md)
- [ADR-0009: Multi-Vault per Daemon](../decisions/0009-multi-vault-per-daemon.md)
- [Vault Management § Cross-Vault Search Semantics](./vault-management.md#cross-vault-search-semantics)
- [Architecture: Indexer](../architecture/overview.md#indexer)

---

## Behavior

### Normal Flow

1. Consumer sends a natural-language query (optionally scoped via `vaults`)
2. Hypomnema calls the embedding service (e.g., TEI) once to convert query to a 768-dim vector
3. Resolve in-scope vaults: `vaults` filter narrows to the named subset; otherwise all currently active vaults
4. For each in-scope vault: query the vault's `chunks_vec` virtual table (sqlite-vec) for nearest neighbors and join back to its `chunks` metadata table
5. Merge per-vault result lists by `score` descending; break ties by `vault_id`
6. Truncate the merged list to `limit`; apply `min_similarity` filter; return top-N with `vault` + `vault_name` annotations when multi-vault is active

### Chunking

Chunks are produced by the indexer (not at query time) using pulldown-cmark to parse Markdown events and split on heading boundaries. See the `markdown-chunking` skill in `.claude/skills/` for the current boundary rules. This describes v0's Markdown chunking strategy; alternative strategies for non-Markdown text are out of v0 scope (see [ADR-0003 § Amendments](../decisions/0003-indexing-in-the-daemon.md#amendments) and [vision.md § Non-Goals](../product/vision.md#non-goals) → "Text-format coverage beyond Markdown").

Each chunk carries:
- `chunk_id` (the `chunks.id` column from the schema baked in step 6)
- `file_path` (vault-relative)
- `chunk_index` (ordinal within file)
- `heading_path` (e.g., `["Architecture", "Containers"]`)
- `text` (the chunk content)
- `content_hash` (of the parent file; chunks are invalidated when this changes)

### Cross-Vault Behavior

Cross-vault execution semantics — vault scoping, ordering, partial-failure handling, paused/errored vault inclusion, fan-out model — are pinned in [vault-management.md § Cross-Vault Search Semantics](./vault-management.md#cross-vault-search-semantics) and apply uniformly across the three search modes. The summary that's semantic-search-specific:

- **Default scope**: all currently active vaults; per-result `vault` + `vault_name` disambiguate origin.
- **Ordering**: global score-descending across vaults. Cosine similarity is bounded `[0.0, 1.0]` (see § Score conversion) and comparable across same-model embeddings, so no cross-vault score normalization is needed. Identical scores break ties by `vault_id`.
- **`limit`**: each vault contributes up to `limit` results to the merge pool; the merged list is then truncated to `limit`. `truncated: true` is set if any per-vault search reported truncation **or** the merged list was capped.
- **`vaults` filter**: `Some([...])` narrows to the named subset; `None` queries all active vaults; `Some([])` is a request validation error.
- **Same-embedding-model assumption**: every active vault's `chunks_vec` is built with the daemon-wide embedding model and dimension. The embedding service is configured per-daemon, not per-vault, and `chunks_vec`'s dimension is migration-baked per [ADR-0007](../decisions/0007-sqlite-vec-over-alternatives.md). A multi-model-embedding deployment (different embedding models per vault) is round-4+; until then, cross-vault score comparison is sound by construction.

For N=1 (single-vault deployment) the cross-vault wire shape collapses to v0/step-9 semantics — single slice already score-sorted, `vault` + `vault_name` populated but the `partial_results` field absent.

---

## Data Schema

### Request

```yaml
query: "how do we prevent spurious reindexes on Dropbox?"
limit: 10                       # optional; default 10
prefix: "notes/"                # optional; restrict to a subdirectory
min_similarity: 0.3             # optional; default 0.0
include_text: "preview"         # optional; preview | full | none; default preview
preview_bytes: 600              # optional; default 600, server max 2000
granularity: "document"        # optional; document | chunk; default document
chunks_per_document: 3          # optional; 1..=100; default 3; document mode only
vaults:                         # optional; multi-vault scoping
  - "personal"
  - "work"
```

| Request Field | Type | Required | Default | Description |
|---|---|---|---|---|
| `query` | string | yes | — | Natural-language query; embedded via the daemon's configured embedding service. |
| `limit` | integer | no | 10 | Global result cap after cross-vault merge. Validation: `1..=1000`. |
| `prefix` | string | no | none | Path-prefix scope. |
| `min_similarity` | float | no | 0.0 | Clamped to `[0.0, 1.0]`. Filtering happens *after* the kNN match per vault and *before* the cross-vault merge. |
| `include_text` | enum string | no | `preview` | Controls chunk text payload: `preview`, `full`, or `none`. `preview` returns up to `preview_bytes` bytes of the chunk text; `full` returns the complete stored chunk; `none` omits the `text` field entirely. |
| `preview_bytes` | integer | no | `600` | Maximum UTF-8 byte length for preview text. Applied only when `include_text: "preview"`; values above the server maximum (2000) are silently clamped. Validation: must be `> 0` when supplied. |
| `granularity` | enum string | no | `document` | Result granularity: `document` groups results by parent file with representative evidence chunks; `chunk` returns a flat list of individual chunk results. Invalid values return `invalid_request`. |
| `chunks_per_document` | integer | no | `3` | Maximum evidence chunks included per document result in `document` mode. Valid range: `1..=100`. Ignored in `chunk` mode. Values outside the range return `invalid_request`. |
| `vaults` | array of strings | no | none → all active | Subset of vaults to query, by name or surrogate ID. Empty array is rejected as `invalid_request`. |

**Request validation**:

- `min_similarity`: clamped to `[0.0, 1.0]` after deserialization. Negative values are clamped to `0.0`; values greater than `1.0` to `1.0`. Filtering happens *after* the kNN match — consumers see at most `limit` results, possibly fewer if `min_similarity` removes some.
- `granularity`: must be `document` or `chunk` when supplied. Any other string value returns `invalid_request`.
- `chunks_per_document`: must be in `1..=100` when supplied. Out-of-range values return `invalid_request`.

### Response

The response shape varies by `granularity`. The top-level envelope is the same; `results` contains either chunk items or document items.

#### Chunk granularity (`granularity: "chunk"`)

```yaml
results:
  - score: 0.82
    file_path: "notes/tools/hypomnema.md"
    chunk_index: 4
    heading_path: ["Pitfalls", "Sync conflicts"]
    text: "Syncthing and Dropbox write files in bursts…"
    text_kind: "preview"
    text_truncated: true
    content_hash: "sha256:abc123…"
    vault: "01951f6c-7c3b-7a2e-8c1d-1a2b3c4d5e6f"
    vault_name: "personal"
  - score: 0.71
    file_path: "notes/design/watchers.md"
    chunk_index: 2
    heading_path: ["Change detection"]
    text: "mtime alone is not enough; compare content hashes…"
    text_kind: "preview"
    text_truncated: false
    content_hash: "sha256:def456…"
    vault: "01951f6c-7c3b-7a2e-8c1d-1a2b3c4d5e6f"
    vault_name: "personal"
# `hint` omitted when results are populated; see § Edge Cases — Empty index.
# `partial_results` omitted in the all-success / all-active case
```

#### Document granularity (`granularity: "document"`, default)

```yaml
results:
  - score: 0.82
    file_path: "notes/tools/hypomnema.md"
    content_hash: "sha256:abc123…"
    vault: "01951f6c-7c3b-7a2e-8c1d-1a2b3c4d5e6f"
    vault_name: "personal"
    chunks:
      - chunk_index: 4
        heading_path: ["Pitfalls", "Sync conflicts"]
        score: 0.82
        text: "Syncthing and Dropbox write files in bursts…"
        text_kind: "preview"
        text_truncated: true
      - chunk_index: 2
        heading_path: ["Background"]
        score: 0.71
        text: "The daemon debounces events from notify…"
        text_kind: "preview"
        text_truncated: false
  - score: 0.68
    file_path: "notes/design/watchers.md"
    content_hash: "sha256:def456…"
    vault: "01951f6c-7c3b-7a2e-8c1d-1a2b3c4d5e6f"
    vault_name: "personal"
    chunks:
      - chunk_index: 1
        heading_path: ["Change detection"]
        score: 0.68
        text: "mtime alone is not enough…"
        text_kind: "preview"
        text_truncated: false
truncated: false
```

**Top-level envelope** (both granularities):

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `results` | array | yes | Per-result objects. Shape depends on `granularity` (see below). Empty array if no matches. |
| `hint` | string | no | Diagnostic hint about index state. Present as `"semantic index is building"` when **every** in-scope vault has zero `chunks_vec` rows but at least one of them has indexed files (`files` row count ≥ 1) — see [§ Edge Cases — Empty index](#empty-index). Omitted in every other case. |
| `truncated` | boolean | yes | True if any per-vault search reported truncation OR the merged list exceeded `limit`. |
| `partial_results` | object | no | Cross-vault diagnostic; present only when at least one vault was skipped or failed. See § Cross-Vault Partial Results. |

**Chunk result fields** (`granularity: "chunk"`):

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `score` | float | yes | Cosine similarity in `[0.0, 1.0]`. Conversion formula below. |
| `file_path` | string | yes | Vault-relative path of the file the chunk came from |
| `chunk_index` | integer | yes | Ordinal of the chunk within the file |
| `heading_path` | array of strings | yes | Heading hierarchy that contains the chunk |
| `text` | string | conditional | Present unless `include_text: "none"`. |
| `text_kind` | enum string | conditional | `preview` or `full`. Present alongside `text`. |
| `text_truncated` | boolean | conditional | `true` when preview text omits part of the chunk. Present alongside `text`. |
| `content_hash` | string | yes | `sha256:`-prefixed parent file content hash. |
| `vault` | string | no | Surrogate vault ID (UUIDv7). Populated in multi-vault deployments. |
| `vault_name` | string | no | Point-in-time vault display name. Populated alongside `vault`. |

**Document result fields** (`granularity: "document"`):

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `score` | float | yes | Document score: the maximum chunk score among all chunks of this file that appeared in the kNN candidate pool. |
| `file_path` | string | yes | Vault-relative path of the document |
| `content_hash` | string | yes | `sha256:`-prefixed file content hash |
| `vault` | string | no | Surrogate vault ID. Populated in multi-vault deployments. |
| `vault_name` | string | no | Point-in-time vault display name. Populated alongside `vault`. |
| `chunks` | array | yes | Top-scoring evidence chunks, up to `chunks_per_document` items, sorted by `score` descending. |

**Evidence chunk fields** (within `chunks` array of a document result):

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `chunk_index` | integer | yes | Ordinal within the file |
| `heading_path` | array of strings | yes | Heading hierarchy |
| `score` | float | yes | Individual chunk cosine similarity |
| `text` | string | conditional | Present unless `include_text: "none"` |
| `text_kind` | enum string | conditional | `preview` or `full`. Present alongside `text`. |
| `text_truncated` | boolean | conditional | Present alongside `text`. |

**Score conversion**: `score = 1.0 - (vec0_distance / 2.0)`, clamped to `[0.0, 1.0]`. The `chunks_vec` virtual table is created with `distance_metric=cosine` (schema-baked at migration 0004; see [ADR-0007 § Amendments](../decisions/0007-sqlite-vec-over-alternatives.md#amendments)), so `vec0_distance` is `1 − cos_sim` and ranges over `[0, 2]`. Identical vectors yield `score = 1.0` (distance `0`); orthogonal vectors yield `0.5` (distance `1`, `cos_sim = 0`); opposite vectors yield `0.0` (distance `2`, `cos_sim = −1`). The clamp is a defensive guard against floating-point edge cases at the endpoints.

**Document scoring**: in document mode the document-level `score` is the maximum of all evidence chunk scores in the kNN candidate pool. The candidate pool size is controlled by `[search.semantic] document_candidate_multiplier` and `document_candidate_limit` (see [§ Configuration](#configuration-knobs)). Increasing the candidate depth improves coverage at the cost of more candidate chunks to group.

### Configuration Knobs

The `[search.semantic]` config section in `hmnd.toml` controls defaults and candidate depth for document mode:

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `default_granularity` | string | `"document"` | Default value for `granularity` when the field is absent from the request. Valid values: `document`, `chunk`. |
| `default_chunks_per_document` | integer | `3` | Default value for `chunks_per_document` when absent. Range: `1..=100`. |
| `document_candidate_multiplier` | integer | `10` | Multiplied by the request `limit` to compute candidate chunk count fed to the document grouper. |
| `document_candidate_limit` | integer | `1000` | Hard cap on candidate count regardless of `limit × multiplier`. Prevents runaway memory use on very large `limit` values. |

### Cross-Vault Partial Results

Same shape as filesystem-search and content-search; pinned in [vault-management.md § Cross-Vault Search Semantics § Partial-Failure Handling](./vault-management.md#cross-vault-search-semantics).

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
      message: "embedding service timed out"
```

`partial_results` is omitted entirely when no vault was skipped or failed.

---

## Validation Rules

| Condition | Error code | Error message |
|---|---|---|
| `include_text` is not one of `preview`, `full`, `none` | `invalid_request` | `include_text must be one of preview, full, none` |
| `preview_bytes` is `0` | `invalid_request` | `preview_bytes must be greater than 0` |
| `preview_bytes` exceeds server maximum (2000) | — | Values above 2000 are silently clamped to 2000; no error is returned. Callers can detect truncation via `text_truncated`. |
| `vaults` is an empty array | `invalid_request` | `vaults filter must be non-empty` |
| `limit` is outside `1..=1000` | `invalid_request` | existing range-validation message |
| `granularity` is not `document` or `chunk` | `invalid_request` | `granularity must be one of document, chunk` |
| `chunks_per_document` is outside `1..=100` | `invalid_request` | `chunks_per_document must be between 1 and 100` |

---

## Examples

### Example 1: Default preview (recommended starting point)

**Request**:

```yaml
query: "why do sync tools cause watcher event storms?"
```

**Behavior**: Hypomnema embeds the query, queries each active vault's vector index, merges by score descending, caps to the default limit of 10, and returns preview text (up to 600 bytes) for each result.

**Response shape** (abbreviated):

```yaml
results:
  - score: 0.87
    file_path: "notes/tools/hypomnema.md"
    chunk_index: 3
    heading_path: ["Pitfalls", "Sync conflicts"]
    text: "Syncthing and Dropbox write files in bursts…"
    text_kind: "preview"
    text_truncated: true
    content_hash: "sha256:abc123…"
truncated: false
```

### Example 2: Full chunk text for a small targeted query

**Request**:

```yaml
query: "sqlite vec migration risks"
limit: 3
include_text: "full"
```

**Behavior**: Returns the top three matching chunks with complete stored chunk text. The larger payload is intentional and bounded by the caller's explicit `limit`.

**Response shape** (abbreviated):

```yaml
results:
  - score: 0.91
    file_path: "docs/decisions/0007-sqlite-vec-over-alternatives.md"
    chunk_index: 1
    heading_path: ["Amendments"]
    text: "Migration 0004 bakes `distance_metric=cosine` and the 768-dim shape into…"
    text_kind: "full"
    text_truncated: false
    content_hash: "sha256:def456…"
truncated: false
```

### Example 3: Metadata-only discovery pass

**Request**:

```yaml
query: "semantic search ranking problems"
limit: 20
include_text: "none"
```

**Behavior**: Returns scores, file paths, chunk indexes, heading paths, and content hashes without chunk text. The caller uses the response as a cheap candidate list, then fetches selected content via a retrieval operation.

**Response shape** (abbreviated):

```yaml
results:
  - score: 0.79
    file_path: "notes/design/search.md"
    chunk_index: 5
    heading_path: ["Ranking", "Score normalization"]
    content_hash: "sha256:ghi789…"
truncated: false
```

---

## Edge Cases

### Empty index

The `hint` discriminates "indexing in progress" from "no matches" by counting `chunks_vec` rows against `files` rows after a kNN that returns zero results. In multi-vault, the count is taken across the *in-scope* vaults (the `vaults` filter or all active):

| in-scope `chunks_vec` rows | in-scope `files` rows | Response |
|-------------------|--------------|----------|
| `0` | `≥ 1` | Empty `results` + `hint: "semantic index is building"` (chunks haven't been embedded for the existing files yet — fresh boot before scan, in-progress indexer, or an embedding-service outage during the initial scan). |
| `0` | `0` | Empty `results`, no `hint` (empty in-scope set — no progress signal is meaningful when there's nothing to index). |
| `≥ 1` | any | Empty `results`, no `hint` (honest "your query had no matches" — would be misleading to suggest indexing is incomplete). |

### Embedding service unavailable

Return a structured error (HTTP 503, MCP error). Do not fall back to content search silently — agents should know their semantic query didn't execute. The query is embedded once at request entry; an embedding-service outage fails the whole request before any per-vault iteration runs.

### Model dimension mismatch (per-vault)

If a vault's `chunks_vec` dimension disagrees with the daemon's configured embedding dimension (which shouldn't happen in practice — both are baked at migration time against the same daemon-wide config), the per-vault query is treated as a `failed` entry in `partial_results.failed` with `code: "dimension_mismatch"` rather than crashing the whole request. The other vaults' contributions are returned normally.

### Model dimension mismatch (daemon-wide)

If the configured embedding model dimension disagrees with the schema's baked-in dimension at *daemon startup*, the daemon fails loudly at startup (see [ADR-0007](../decisions/0007-sqlite-vec-over-alternatives.md) and the `sqlite-vec-extension` skill). Queries never run against a mismatched daemon.

### In-place vector updates

sqlite-vec's vec0 virtual table does not update rows gracefully; the indexer deletes and reinserts all chunks for a file on any content change. Consumers do not see partial-update states — either the old set is current or the new set is.

### Path collisions across vaults

Two vaults may contain a file at the same vault-relative path, and a chunk from each may surface in `results`. The `vault` + `vault_name` fields disambiguate. The merged ordering is by `score` descending, so co-pathed chunks of equal score break ties by `vault_id`.

### Paused or errored vault in scope

A vault in `paused` or `errored` status is silently skipped; one entry per skipped vault is appended to `partial_results.skipped` with its current status and (for `errored`) the registry's `last_error` text.

### Preview boundary in multibyte UTF-8

The `preview_bytes` cap is a byte limit, not a character limit. The implementation walks back from the byte cap to the nearest valid UTF-8 character boundary to avoid returning invalid UTF-8. In v0 there is no paragraph or sentence heuristic: the boundary is wherever the byte cap falls, aligned to a character boundary. A preview may therefore end mid-sentence or mid-word.

### Boilerplate-heavy chunks

The chunker does not strip fenced code blocks, Dataview queries, or other generated content. A matched chunk that is predominantly boilerplate will return preview text up to the byte cap, which may be entirely code or table syntax. Callers that need clean prose can request `include_text: "none"` for a discovery pass, then fetch the full file via a retrieval operation after reviewing `file_path` and `heading_path`.

---

## Open Questions

- [ ] Reranking: should we rerank the top-N using a cross-encoder, or return raw cosine-similarity order?
- [ ] Hybrid search: should semantic and content results be fused (e.g., RRF) into a single operation, or kept separate?
- [ ] Chunk/section retrieval: is full-file content retrieval enough for follow-up workflows, or is a dedicated chunk/section retrieval operation needed? — deferred to the [content-retrieval proposal](../../notes/proposals/content-retrieval.md).
- [ ] Pagination / cursor across N independent indexes — deferred to round 4+ per [vault-management.md § Open Questions](./vault-management.md#open-questions). Round 3 ships `truncated: bool` only; no cursor.
- [ ] Multi-model embeddings per vault — deferred to round 4+ per [vault-management.md § Open Questions](./vault-management.md#open-questions). The cross-vault score-desc ordering relies on the same-embedding-model assumption; relaxing it requires either score normalization or per-vault top-K with re-ranking.
- [ ] Streaming response shapes (chunked HTTP / SSE / NDJSON) for high-vault-count deployments — deferred per [vault-management.md § Open Questions](./vault-management.md#open-questions).

---

## Revision History

| Version | Date | Changes |
|---------|------|---------|
| 0.1.0 | 2026-04-23 | Initial draft, seeded from project handoff v0 scope |
| 0.2.0 | 2026-04-27 | Multi-vault adoption (round 3 / step 10): `vault` semantics flipped from "always absent" to "populated when multi-vault active"; added `vault_name`, request-side `vaults` filter, response-envelope `partial_results`, global score-desc cross-vault ordering with `vault_id` tie-break. Same-embedding-model assumption documented. Cross-vault execution semantics cross-referenced from [vault-management.md](./vault-management.md). |
| 0.4.0 | 2026-05-08 | Round 14 / Step 25: document granularity. Request: `granularity` (`document` \| `chunk`, default `document`) and `chunks_per_document` (1..=100, default 3). Response: new document-result shape with `chunks` evidence array and document-level `score` (max chunk score); chunk-result shape unchanged. Configuration knobs: `[search.semantic]` `default_granularity`, `default_chunks_per_document`, `document_candidate_multiplier`, `document_candidate_limit`. Validation: `invalid_request` on unrecognized `granularity` or out-of-range `chunks_per_document`. |
| 0.3.1 | 2026-05-03 | Clarification (no behavior change): § Chunking notes that the pulldown-cmark heading-aware strategy is v0's chunking strategy; alternative strategies for non-Markdown text are out of v0 scope. Cross-references ADR-0003 § Amendments and vision.md § Non-Goals → "Text-format coverage beyond Markdown" added by the same canon-positioning sweep. |
| 0.3.0 | 2026-05-01 | Round 8 / Step 17: payload budgeting added. Request: `include_text` (`preview` \| `full` \| `none`, default `preview`) and `preview_bytes` (default 600, server max 2000, silently clamped). Response: `text` changed from required to conditional; added `text_kind`, `text_truncated` (both conditional, present alongside `text`); added `content_hash` (required, `sha256:`-prefixed, projected from chunk metadata). `limit` default re-pinned as 10. Validation Rules section added; Examples section added (default preview, full-text, metadata-only); Edge Cases: added "Preview boundary in multibyte UTF-8" and "Boilerplate-heavy chunks". Resolved proposal questions (text-field strategy → `text` + `text_kind` + `text_truncated`; `preview_bytes` max → 2000, clamped; `content_hash` inclusion → yes; content-search `include_matches` default drift → corrected in step 17.6). Chunk/section-retrieval question deferred to content-retrieval proposal. |
