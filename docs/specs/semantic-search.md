# Semantic Search Specification

**Version**: 0.1.0
**Date**: 2026-04-23
**Status**: Draft

---

## Overview

Semantic search answers conceptual-similarity questions: *what in this vault is similar to this idea?* The query is embedded into a vector via the same model used for indexing, and compared against the stored chunk vectors by cosine similarity. Results are chunks (heading-aware slices of files), with metadata identifying the file and section they came from.

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
- `chunk_id` (primary key)
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
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `score` | float | yes | Cosine similarity in `[0.0, 1.0]` |
| `file_path` | string | yes | Vault-relative path of the file the chunk came from |
| `chunk_index` | integer | yes | Ordinal of the chunk within the file |
| `heading_path` | array of strings | yes | Heading hierarchy that contains the chunk |
| `text` | string | yes | The chunk content |
| `vault` | string | no | Reserved; always absent in v0. Will carry the source vault identifier when multi-vault ships. |

The `vault` field is present in the response shape from step 5 onwards (added when the HTTP filesystem and content endpoints lit up); see [step-5 workplan § Deferred decision 1](../roadmap/step-05-workplan.md#1-multi-vault-forward-compat-vault-field). Semantic search itself ships in step 7; the field is forward-compat scaffolding, always omitted in v0.

---

## Edge Cases

### Empty index

If no chunks have been embedded yet (fresh start), return empty results and a hint that the semantic index is building.

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
