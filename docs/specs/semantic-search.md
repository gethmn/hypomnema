# Semantic Search Specification

**Version**: 0.1.0
**Date**: 2026-04-23
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
- [Architecture: Indexer](../architecture/overview.md#indexer)

---

## Behavior

### Normal Flow

1. Consumer sends a natural-language query
2. Hypomnema calls the embedding service (e.g., TEI) to convert query to a 768-dim vector
3. Hypomnema queries the `chunks_vec` virtual table (sqlite-vec) for nearest neighbors
4. Joins back to the chunks metadata table for file path, heading, and chunk text
5. Returns the top-N results with similarity scores

### Chunking

Chunks are produced by the indexer (not at query time) using pulldown-cmark to parse Markdown events and split on heading boundaries. See the `markdown-chunking` skill in `.claude/skills/` for the current boundary rules.

Each chunk carries:
- `chunk_id` (the `chunks.id` column from the schema baked in step 6)
- `file_path` (vault-relative)
- `chunk_index` (ordinal within file)
- `heading_path` (e.g., `["Architecture", "Containers"]`)
- `text` (the chunk content)
- `content_hash` (of the parent file; chunks are invalidated when this changes)

---

## Data Schema

### Request

```yaml
query: "how do we prevent spurious reindexes on Dropbox?"
limit: 10                       # optional; default 10
prefix: "notes/"                # optional; restrict to a subdirectory
min_similarity: 0.3             # optional; default 0.0
```

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
  - score: 0.71
    file_path: "notes/design/watchers.md"
    chunk_index: 2
    heading_path: ["Change detection"]
    text: "mtime alone is not enough; compare content hashes…"
# `hint` omitted when results are populated; see § Edge Cases — Empty index.
```

**Top-level envelope**:

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `results` | array | yes | Per-result objects (shape below). Empty array if no matches or if the index has no chunks yet. |
| `hint` | string | no | Diagnostic hint about index state. Present as `"semantic index is building"` when `chunks_vec` has zero rows but `files` has at least one row — see [§ Edge Cases — Empty index](#empty-index). Omitted in every other case (no false signal of in-progress indexing for honest "no matches" results). |

**Per-result fields**:

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `score` | float | yes | Cosine similarity in `[0.0, 1.0]`. Conversion formula below. |
| `file_path` | string | yes | Vault-relative path of the file the chunk came from |
| `chunk_index` | integer | yes | Ordinal of the chunk within the file |
| `heading_path` | array of strings | yes | Heading hierarchy that contains the chunk |
| `text` | string | yes | The chunk content |
| `vault` | string | no | Reserved; always absent in v0. Will carry the source vault identifier when multi-vault ships. |

The `vault` field is present in the response shape from step 5 onwards (added when the HTTP filesystem and content endpoints lit up); see [step-5 workplan § Deferred decision 1](../roadmap/step-05-workplan.md#1-multi-vault-forward-compat-vault-field). Semantic search itself ships in step 7; the field is forward-compat scaffolding, always omitted in v0.

**Score conversion**: `score = 1.0 - (vec0_distance / 2.0)`, clamped to `[0.0, 1.0]`. The `chunks_vec` virtual table is created with `distance_metric=cosine` (schema-baked at migration 0004; see [ADR-0007 § Amendments](../decisions/0007-sqlite-vec-over-alternatives.md#amendments)), so `vec0_distance` is `1 − cos_sim` and ranges over `[0, 2]`. Identical vectors yield `score = 1.0` (distance `0`); orthogonal vectors yield `0.5` (distance `1`, `cos_sim = 0`); opposite vectors yield `0.0` (distance `2`, `cos_sim = −1`). The clamp is a defensive guard against floating-point edge cases at the endpoints.

---

## Edge Cases

### Empty index

The hint discriminates "indexing in progress" from "no matches" by counting `chunks_vec` rows against `files` rows after a kNN that returns zero results:

| `chunks_vec` rows | `files` rows | Response |
|-------------------|--------------|----------|
| `0` | `≥ 1` | Empty `results` + `hint: "semantic index is building"` (chunks haven't been embedded for the existing files yet — fresh boot before scan, in-progress indexer, or an embedding-service outage during the initial scan). |
| `0` | `0` | Empty `results`, no `hint` (empty vault — no progress signal is meaningful when there's nothing to index). |
| `≥ 1` | any | Empty `results`, no `hint` (honest "your query had no matches" — would be misleading to suggest indexing is incomplete). |

### Embedding service unavailable

Return a structured error (HTTP 503, MCP error). Do not fall back to content search silently — agents should know their semantic query didn't execute.

### Model dimension mismatch

If the configured embedding model dimension disagrees with the schema's baked-in dimension, the daemon fails loudly at startup (see [ADR-0007](../decisions/0007-sqlite-vec-over-alternatives.md) and the `sqlite-vec-extension` skill). Queries never run against a mismatched index.

### In-place vector updates

sqlite-vec's vec0 virtual table does not update rows gracefully; the indexer deletes and reinserts all chunks for a file on any content change. Consumers do not see partial-update states — either the old set is current or the new set is.

---

## Open Questions

- [ ] Reranking: should we rerank the top-N using a cross-encoder, or return raw cosine-similarity order?
- [ ] Hybrid search: should semantic and content results be fused (e.g., RRF) into a single operation, or kept separate?
- [ ] Should the response include adjacent chunks for context, or just the matched chunk?

---

## Revision History

| Version | Date | Changes |
|---------|------|---------|
| 0.1.0 | 2026-04-23 | Initial draft, seeded from project handoff v0 scope |
