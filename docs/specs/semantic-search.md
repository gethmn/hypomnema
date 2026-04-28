# Semantic Search Specification

**Version**: 0.2.0
**Date**: 2026-04-27
**Status**: Draft

---

## Overview

Semantic search answers conceptual-similarity questions: *what in this vault is similar to this idea?* The query is embedded into a vector via the same model used for indexing, and compared against the stored chunk vectors by cosine similarity. Results are chunks (heading-aware slices of files), with metadata identifying the file and section they came from.

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

Chunks are produced by the indexer (not at query time) using pulldown-cmark to parse Markdown events and split on heading boundaries. See the `markdown-chunking` skill in `.claude/skills/` for the current boundary rules.

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
| `vaults` | array of strings | no | none → all active | Subset of vaults to query, by name or surrogate ID. Empty array is rejected as `invalid_request`. |

**Request validation**:

- `min_similarity`: clamped to `[0.0, 1.0]` after deserialization. Negative values are clamped to `0.0`; values greater than `1.0` to `1.0`. Filtering happens *after* the kNN match — consumers see at most `limit` results, possibly fewer if `min_similarity` removes some.

### Response

```yaml
results:
  - score: 0.82
    file_path: "notes/tools/hypomnema.md"
    chunk_index: 4
    heading_path: ["Pitfalls", "Sync conflicts"]
    text: "Syncthing and Dropbox write files in bursts…"
    vault: "01951f6c-7c3b-7a2e-8c1d-1a2b3c4d5e6f"
    vault_name: "personal"
  - score: 0.71
    file_path: "notes/design/watchers.md"
    chunk_index: 2
    heading_path: ["Change detection"]
    text: "mtime alone is not enough; compare content hashes…"
    vault: "01951f6c-7c3b-7a2e-8c1d-1a2b3c4d5e6f"
    vault_name: "personal"
# `hint` omitted when results are populated; see § Edge Cases — Empty index.
# `partial_results` omitted in the all-success / all-active case
```

**Top-level envelope**:

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `results` | array | yes | Per-result objects (shape below). Empty array if no matches or if no in-scope vault has chunks yet. |
| `hint` | string | no | Diagnostic hint about index state. Present as `"semantic index is building"` when **every** in-scope vault has zero `chunks_vec` rows but at least one of them has indexed files (`files` row count ≥ 1) — see [§ Edge Cases — Empty index](#empty-index). Omitted in every other case. |
| `truncated` | boolean | yes | True if any per-vault search reported truncation OR the merged list exceeded `limit`. |
| `partial_results` | object | no | Cross-vault diagnostic; present only when at least one vault was skipped or failed. See § Cross-Vault Partial Results. |

**Per-result fields**:

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `score` | float | yes | Cosine similarity in `[0.0, 1.0]`. Conversion formula below. |
| `file_path` | string | yes | Vault-relative path of the file the chunk came from |
| `chunk_index` | integer | yes | Ordinal of the chunk within the file |
| `heading_path` | array of strings | yes | Heading hierarchy that contains the chunk |
| `text` | string | yes | The chunk content |
| `vault` | string | no | Surrogate vault ID (UUIDv7). Populated when multi-vault is active (round 3+); omitted for v0/step-9 single-vault wire shape. |
| `vault_name` | string | no | Mutable, point-in-time-accurate display name for the source vault. Populated alongside `vault`. Never appears in the durable outbox (see [change-events.md](./change-events.md)). |

**Score conversion**: `score = 1.0 - (vec0_distance / 2.0)`, clamped to `[0.0, 1.0]`. The `chunks_vec` virtual table is created with `distance_metric=cosine` (schema-baked at migration 0004; see [ADR-0007 § Amendments](../decisions/0007-sqlite-vec-over-alternatives.md#amendments)), so `vec0_distance` is `1 − cos_sim` and ranges over `[0, 2]`. Identical vectors yield `score = 1.0` (distance `0`); orthogonal vectors yield `0.5` (distance `1`, `cos_sim = 0`); opposite vectors yield `0.0` (distance `2`, `cos_sim = −1`). The clamp is a defensive guard against floating-point edge cases at the endpoints.

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

---

## Open Questions

- [ ] Reranking: should we rerank the top-N using a cross-encoder, or return raw cosine-similarity order?
- [ ] Hybrid search: should semantic and content results be fused (e.g., RRF) into a single operation, or kept separate?
- [ ] Should the response include adjacent chunks for context, or just the matched chunk?
- [ ] Pagination / cursor across N independent indexes — deferred to round 4+ per [vault-management.md § Open Questions](./vault-management.md#open-questions). Round 3 ships `truncated: bool` only; no cursor.
- [ ] Multi-model embeddings per vault — deferred to round 4+ per [vault-management.md § Open Questions](./vault-management.md#open-questions). The cross-vault score-desc ordering relies on the same-embedding-model assumption; relaxing it requires either score normalization or per-vault top-K with re-ranking.
- [ ] Streaming response shapes (chunked HTTP / SSE / NDJSON) for high-vault-count deployments — deferred per [vault-management.md § Open Questions](./vault-management.md#open-questions).

---

## Revision History

| Version | Date | Changes |
|---------|------|---------|
| 0.1.0 | 2026-04-23 | Initial draft, seeded from project handoff v0 scope |
| 0.2.0 | 2026-04-27 | Multi-vault adoption (round 3 / step 10): `vault` semantics flipped from "always absent" to "populated when multi-vault active"; added `vault_name`, request-side `vaults` filter, response-envelope `partial_results`, global score-desc cross-vault ordering with `vault_id` tie-break. Same-embedding-model assumption documented. Cross-vault execution semantics cross-referenced from [vault-management.md](./vault-management.md). |
