# ADR-0007: sqlite-vec over Lance, qdrant, and Other Vector Stores

**Status**: accepted
**Date**: 2026-04-23
**Decision-Makers**: Beau Simensen

---

## Context

Hypomnema needs a vector store for the semantic search index. Serious options considered:

- **sqlite-vec** — SQLite extension. One file on disk; no separate process; loaded into the existing connection pool.
- **Lance / LanceDB** — Columnar store with vector support; embedded; more full-featured for analytical workloads.
- **qdrant** (embedded mode, or standalone) — Purpose-built vector DB; strong filtering/payload features; most mature.
- **Chroma, Weaviate embedded, others** — Roughly similar shape to qdrant-embedded.

Criteria:
1. No separate process required at runtime
2. Single-binary-plus-extension deployment
3. Integrates with the SQLite connection pool already used for metadata, hashes, and the filesystem/content index — unified transactions, one backup concern
4. Performance sufficient for a personal-to-small-team vault (tens of thousands of chunks, not billions)

## Decision

Use `sqlite-vec` as the vector store.

The vector column dimension is baked into the schema at creation time (`768` for `nomic-embed-text-v1.5`). Model-switching is not a v0 concern; if it ever becomes real, the path is a re-index (drop + rebuild the vec table with a new dimension), not a runtime switch.

Ship as: the Hypomnema binary, plus the sqlite-vec extension (`.so` / `.dylib` / `.dll`). Two files. No service architecture.

## Consequences

### Positive

- One process, one file on disk for *all* indexes (metadata, filesystem, content, semantic). Unified transactions; unified backup story.
- No separate vector-DB process to supervise, upgrade, or fail independently of the daemon
- Deployment is trivially portable — copy the binary and the extension; done
- Integrates naturally with rusqlite's extension-loading API

### Negative

- Less-featured than qdrant on the filtering / payload / index-tuning axes. For the vault-scale use case this is fine; for bigger workloads it would start to pinch.
- Dimension is a schema-level commitment. Changing embedding model requires a re-index (and in practice, a migration path when that happens)
- Vec0 virtual tables do not update rows gracefully — the pattern is *delete and reinsert* on any change, which the `sqlite-vec-extension` skill captures

### Neutral

- If the project ever outgrows sqlite-vec, the vector-store layer is small enough to swap. But that's not abstraction-for-today's-sake — v0 builds directly against sqlite-vec without a swap-in seam (consistent with the handoff's "no abstraction until a second concrete implementation demands one")

---

## Notes

- Skills in `.claude/skills/sqlite-vec-extension/` capture the extension-loading, schema, and delete-and-reinsert patterns that this choice makes mandatory
- Related to [ADR-0003](./0003-indexing-in-the-daemon.md) (indexing in the daemon — sqlite-vec is what makes the daemon self-contained rather than dependent on an external vector DB) and [ADR-0005](./0005-local-everything.md) (local everything)

## Amendments

### 2026-04-26 — distance metric is also schema-baked (additive)

The original Decision pinned the *vector dimension* (`768` for `nomic-embed-text-v1.5`) at schema-creation time. Step 7's migration 0004 extends the same shape to the **distance metric**: `chunks_vec` is created with the explicit `distance_metric=cosine` clause, so the metric the kNN MATCH operator uses is fixed by the schema rather than inherited from upstream defaults or implied by an external embedding-service configuration. This is additive to — not in tension with — the original "dimension is schema-level" claim.

The motivation: sqlite-vec's vec0 default distance metric is L2. L2 distance and cosine similarity coincide *only* for unit-normalized vectors, and whether the embedding service returns unit-normalized vectors depends on its pooling/normalization configuration, which is outside the daemon's control. Schema-baking `cosine` makes the contract correct regardless of how the embedding service is configured. Score conversion at query time pins to `score = 1.0 - (vec0_distance / 2.0)`, clamped to `[0.0, 1.0]` (see [`docs/specs/semantic-search.md`](../specs/semantic-search.md) § Response).

The cost — re-indexing existing populated DBs at next daemon start — is covered by clearing `files.content_hash` in the same migration, which causes the watcher's existing rebuild path to re-read, re-chunk, and re-embed every file with no operator action. See [step-7 workplan § Resolution F](../roadmap/step-07-workplan.md#f-cosine-distance-metric-for-chunks_vec-migration-0004) for the full rationale and the alternative (query-time L2-to-cosine conversion) that was rejected.
