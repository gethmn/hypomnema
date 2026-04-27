# Step 7 Workplan — Semantic search

**Step**: 7 of 8 (round 2 of 2). Second step of round 2 — see [`roadmap-2.md`](./roadmap-2.md) for the round and [`step-06-workplan.md`](./step-06-workplan.md) for the immediately prior step (which produced the `chunks` and `chunks_vec` substrate this step queries).

**Status**: Shipped 2026-04-26. See [§ Build-time amendments](#build-time-amendments) at the end of this file for the items the build surfaced.

**Round-2 lessons carrying forward** (from [`notes/project-planning-workflow-notes.md`](../../project-planning-workflow-notes.md) § Step 6 retro):

- Risk grade is honest: step 7 is **medium**. The schema introduced in step 6 is immutable per ADR-0007; this step composes the existing surfaces (embedding client, chunks_vec table, HTTP router) plus one new schema migration (0004 — see resolution F). The load-bearing risk is the **embedding-service-at-query-time** path, pulled forward in resolution E.
- Five deferred decisions from the roadmap are resolved here at workplan-write time. One additional fall-out resolution (cosine distance metric) surfaced during task design and is resolved as a small additive migration.
- Self-review for prose accuracy ran after the first draft per the heuristic added at the round-1 boundary; results in [§ Self-review for prose accuracy](#self-review-for-prose-accuracy).
- Coordinator-spawned in-build follow-up (the Task 6.4r1 pattern, see step-6 retro) is available for soft flags that demonstrate a real cross-task bug — the act-now decision rule is (a) flag demonstrates a real bug, (b) directive intent is unambiguous, (c) fix is small and well-bounded, (d) downstream task scopes don't naturally cover the buggy path.
- Manual smoke verification on the medium-risk wiring task is a per-step investment that paid off in step 6 and is repeated here on Task 7.3 (HTTP wiring).

---

## Goal recap

Axum exposes `POST /search/semantic` returning the response shape from [`docs/specs/semantic-search.md`](../../../docs/specs/semantic-search.md). The handler embeds the query via the same [`EmbeddingClient`](../../../src/embedding.rs) that step 6 wired into the indexer, runs a `chunks_vec` nearest-neighbor query (cosine distance via the schema-baked metric — see resolution F), joins back to `chunks` for path / heading / text metadata, and returns the top-N results with similarity scores in `[0.0, 1.0]`.

`hmn search semantic <query>` lights up — the round-1 stub (`src/bin/hmn.rs:69-72` "lands in step 7") becomes a real HTTP-backed command parallel to `hmn search filesystem` and `hmn search content`.

Empty-index queries return an empty result list plus a `hint` field (resolution B). Embedding-service unavailability at query time returns HTTP 503 with `code: "embedding_unavailable"` (resolution E) — not a 500, not a silent fallback to content search.

The MCP wrapper (step 8) is **out of scope** — the query function in `src/search/semantic.rs` will be the same surface step 8 wraps over MCP.

---

## Deferred-decision resolutions

The five TBDs from [`roadmap-2.md`](./roadmap-2.md) § Step 7 are resolved below (A–E), plus one fall-out resolution surfaced during task decomposition (F — cosine distance metric).

### A. Default `min_similarity` threshold

**Resolution**: default `min_similarity = 0.0` (no filtering). The field stays in the request shape (`min_similarity: Option<f32>`, default `0.0`), so consumers can opt in to a non-zero threshold per query. The handler clamps any negative value to `0.0` and any value `> 1.0` to `1.0` before the SQL filter.

**Why**: zero corpus evidence right now to support a non-zero default. Any threshold I pick before there's a vault to tune against would discard results that an agent might find useful, and is hard to undo without consumer-side coordination ("the daemon now returns more — re-tune your prompts"). The threshold field is a future-proof escape hatch: keep it in the wire shape, leave the default permissive, promote a sensible default in a later step when a real vault gives us evidence (e.g. "scores below 0.4 are noise"). This also matches the spec's open question "Reranking: should we rerank the top-N using a cross-encoder…" — defaults that bake in assumptions about score distributions are the kind of thing reranking would re-shape, so we don't pin one now.

**How to apply**: `SemanticQuery::min_similarity` defaults to `0.0` in `src/search/semantic.rs`. The handler in `src/api/search.rs` clamps to `[0.0, 1.0]` after deserialization; the SQL projects `score` and filters `WHERE score >= ?min_similarity` after the kNN match. Filtering happens after kNN, not before — the kNN search itself uses `k = ?limit` and surfaces the top-`limit` regardless of similarity; the `min_similarity` filter is then applied to that result set. Consumers thus see at most `limit` results, possibly fewer if `min_similarity` removes some.

**References**: [`docs/specs/semantic-search.md`](../../../docs/specs/semantic-search.md) line 56 (`min_similarity: 0.3` example, "default 0.0").

### B. Behavior when `chunks_vec` is empty but `files` is not

**Resolution**: when the `chunks_vec` table has zero rows AND the `files` table has at least one row, the response carries an empty `results` array AND a `hint` field set to `"semantic index is building"`. When both are empty (fresh daemon, empty vault), the response carries an empty `results` array and **no** hint (the empty result is just an empty result; no progress signal is meaningful when there's nothing to index). When `chunks_vec` has rows and the query simply matches nothing, the response carries an empty `results` array and **no** hint (an honest "your query had no matches" — no false signal of in-progress indexing).

The `hint` field is `Option<String>`, `#[serde(skip_serializing_if = "Option::is_none")]` — omitted from the wire when None.

**Why**: the spec (line 92) commits to "empty results and a hint that the semantic index is building" for the empty-index case — that's the load-bearing behavior. The discriminator that separates "indexing in progress" from "no matches" is the count of `chunks_vec` rows: if it's zero but `files` is non-zero, we know the daemon hasn't finished embedding the existing files yet (could be a fresh boot before scan, an in-progress indexer, or an embedding-service outage during the initial scan that left chunks unwritten — all three justify the hint). If `chunks_vec` has rows, then "no match" is the truthful interpretation of an empty result set, and a hint would be misleading.

The two-table count is two short SQL statements, both `O(1)` against SQLite's row-count optimization for primary-keyed tables. Cheap.

**How to apply**: `search_semantic()` runs the kNN query first; if the result set is empty, runs `SELECT count(*) FROM chunks_vec` and `SELECT count(*) FROM files` (both inside the same `spawn_blocking`). Sets `hint = Some("semantic index is building".to_string())` if `chunks_vec_count == 0 && files_count > 0`. Returns the `(Vec<SemanticResult>, Option<String>)` pair to the handler.

**References**: [`docs/specs/semantic-search.md`](../../../docs/specs/semantic-search.md) § Edge Cases — Empty index (line 90–92); [`notes/roadmap/archive/roadmap-2.md`](./roadmap-2.md) § Step 7 shipping criterion 3.

### C. Result ordering across ties

**Resolution**: `ORDER BY v.distance ASC, c.file_path ASC, c.chunk_index ASC`. Primary key is the kNN distance (smaller distance = more similar = comes first). Secondary key is `file_path` ASC. Tertiary key is `chunk_index` ASC (the chunk's ordinal within its file).

**Why**: kNN distance ties are rare in practice but possible (e.g. two near-identical chunks pulled from different files, or the same chunk text duplicated across files). When ties occur, deterministic ordering is the property consumers need for testable, paginatable results. `file_path` ASC is the obvious secondary because it's debuggable (the consumer can predict the order from the input data) and matches the existing filesystem/content search ordering. `chunk_index` ASC as tertiary handles the (rarer still) case of two chunks within the same file at the same distance.

Alternative considered: `ORDER BY v.distance ASC, c.id ASC`. Cheaper (one column) but `c.id` is opaque (insertion-order-dependent, observed but not predicted). Rejected: the marginal SQL cost of two extra ORDER BY keys is negligible at v0 scale; the debuggability win is real.

**How to apply**: the SELECT in `src/search/semantic.rs` projects `v.distance`, `c.file_path`, `c.chunk_index`, etc., and orders by all three. The kNN's `k = ?limit` clause caps the candidate set before the ORDER BY runs.

**References**: [`notes/roadmap/archive/roadmap-2.md`](./roadmap-2.md) § Step 7 deferred decision 3 ("by file path? by `chunks.chunk_index`?").

### D. Query embedding caching strategy

**Resolution**: no cache in v0. Every `/search/semantic` request issues one `embedder.embed_text(query)` HTTP call to the embedding service. No per-process LRU, no per-query memo, no persistent cache.

**Why**: query embedding is one HTTP round-trip to a local service (typically tens of ms for nomic-embed-text-v1.5 + TEI on commodity hardware). Caching adds a memory footprint, an eviction policy, and a cache-key normalization concern (case sensitivity? whitespace? language? unicode form?) that would re-litigate every query. None of these knobs has weight without evidence of repeat queries from the same agent session — and we don't have that evidence yet. The simpler shape (no cache) is the right v0 default.

The decision is reversible: a future step can wrap `EmbeddingClient` with an LRU adapter (e.g. a `CachingEmbedder` that implements `Embedder`) at any point without touching `search_semantic` itself. The trait shape from step 6 (`Arc<dyn Embedder>`, see `src/embedding.rs:167-169`) is already cache-friendly — adding a cache layer is a wrap, not a rewrite.

**How to apply**: nothing to add in v0. The handler holds an `Arc<dyn Embedder>` (per the wiring in resolution / Task 7.3) and calls `embed_text()` per request. Future-step note: if real-world latency justifies it, a `CachingEmbedder` wraps the underlying client and is constructed in `hmnd.rs` between `EmbeddingClient::new()` and the `Scanner`/`ApiState` wiring.

**References**: [`notes/roadmap/archive/roadmap-2.md`](./roadmap-2.md) § Step 7 deferred decision 4 ("per-process, per-query, none").

### E. Error envelope code for embedding-service-unavailable

**Resolution**: a new error envelope code `embedding_unavailable` mapped to HTTP 503. It covers three runtime classifications from `EmbeddingError`:

- `EmbeddingError::Transport(_)` → 503 `embedding_unavailable`, message `"embedding service is unreachable: <transport detail>"`.
- `EmbeddingError::Status { code: 500..=599, .. }` → 503 `embedding_unavailable`, message `"embedding service returned HTTP <code>"`.
- `EmbeddingError::DimensionMismatch { .. }` → 503 `embedding_unavailable`, message `"embedding service returned a vector with dimension <actual>; daemon expected <expected>"` (per the v0 contract from step 6's resolution / Task 6.4r1: "the daemon never crashes due to embedding service issues, anywhere in the runtime").

The remaining `EmbeddingError` variants classify as `internal` (HTTP 500), since they signal a daemon bug or an unparseable response (not a service outage):

- `EmbeddingError::Status { code: 400..=499, .. }` → 500 `internal`. A 4xx from the embedding service indicates the daemon is sending a malformed request (model name typo, bad JSON shape, bad auth) — that's a bug to fix, not an operator-visible service-down state. Logged at `ERROR` per the existing `tracing::error!(error = ?err, "internal API error")` in `src/api/error.rs:74`.
- `EmbeddingError::BodyParse(_)` → 500 `internal`. Either the service shipped a non-OpenAI-compatible response or the daemon's response struct is wrong — bug, not outage.

**Why**: `embedding_unavailable` matches the existing snake_case naming convention (`invalid_glob`, `invalid_regex`, `invalid_prefix`, `invalid_request`, `internal`), names the *specific* substrate that's failing (the embedding service, not the SQLite pool, not the daemon process itself), and avoids the over-general `service_unavailable` which would conflate three different outage shapes (daemon down, embedding sidecar down, vector store unreachable). HTTP 503 is the right status for "the daemon is up but cannot complete this kind of request right now" per RFC 7231 §6.6.4.

The DimensionMismatch-as-503 choice is the load-bearing pull-forward from step 6's resolution / Task 6.4r1: at index time, dimension mismatch is skip-and-log; at query time, "skip" isn't a thing — we have to respond. The consumer's experience is the same in both cases ("the embedding service is misconfigured, semantic search did not execute"), so the wire-level code is the same. The detail message differentiates if the operator wants to introspect.

Alternative considered: separate codes (`embedding_dimension_mismatch` 503 vs `embedding_unreachable` 503). Rejected: two codes for one consumer-experience class adds wire-level surface without buying anything. The CLI doesn't need to branch on which kind of embedding outage it is. The message string is enough.

**How to apply**: extend `src/api/error.rs::From<anyhow::Error>` (or — likely cleaner — add a new `From<SemanticSearchError>` impl) to map `SemanticSearchError::EmbeddingUnavailable { .. }` → `ApiError { status: 503, code: "embedding_unavailable", message: <detail> }` and `SemanticSearchError::Internal { .. }` → `ApiError::internal()`. The `SemanticSearchError` enum lives in `src/search/semantic.rs` and is the contract the HTTP handler maps from.

**References**: [`docs/specs/semantic-search.md`](../../../docs/specs/semantic-search.md) § Edge Cases — Embedding service unavailable (line 92–96); [`notes/roadmap/archive/step-06-workplan.md`](./step-06-workplan.md) § Build-time amendment 3 (Task 6.4r1 reclassified DimensionMismatch as skip-and-log at index time); [`src/api/error.rs:46-77`](../../../src/api/error.rs) for the existing error-envelope shape and pattern.

### Resolved as part of this step (not pre-flagged in the roadmap)

#### F. Cosine distance metric for `chunks_vec` (migration 0004)

**Resolution**: a new migration 0004 recreates `chunks_vec` with `distance_metric=cosine`, truncates `chunks`, and clears `files.content_hash` to force the next watcher event or scan cycle to re-read content and re-chunk. Score conversion at query time: `score = 1.0 - (distance / 2.0)`, clamped to `[0.0, 1.0]` as a defensive guard against floating-point edge cases. Score = 1.0 for identical vectors (distance 0); score = 0.5 for orthogonal vectors (distance 1, cos_sim 0); score = 0.0 for opposite vectors (distance 2, cos_sim −1).

Migration 0004 SQL shape (the agent verifies the exact `distance_metric` clause syntax against upstream sqlite-vec at task time per the [`sqlite-vec-extension`](../../../.claude/skills/sqlite-vec-extension/SKILL.md) skill smell about "MATCH and k pseudo-column are sqlite-vec idioms; the API has moved around"):

```sql
DROP TABLE chunks_vec;
DELETE FROM chunks;
UPDATE files SET content_hash = '';
CREATE VIRTUAL TABLE chunks_vec USING vec0(
    chunk_id INTEGER PRIMARY KEY,
    embedding FLOAT[768] distance_metric=cosine
);
```

The `validate_dimension` regex in `src/store/mod.rs:134` (`embedding\s+FLOAT\[(\d+)\]`) keeps working — the regex matches the dimension only, not anything after the closing bracket.

**Why**: the spec (line 11, 31) commits to "cosine similarity" and the response shape's `score` field is documented as cosine similarity in `[0.0, 1.0]`. Step 6's migration 0003 (`src/store/schema.rs:24-40`) created `chunks_vec` with no explicit `distance_metric` clause, defaulting to L2 per upstream sqlite-vec. L2 distance and cosine similarity coincide *only* for unit-normalized vectors; whether nomic-embed-text-v1.5 via TEI returns unit-normalized vectors depends on the TEI service's pooling/normalization configuration — not under the daemon's control. Schema-baking the distance metric makes the contract correct regardless of how the embedding service is configured.

The cost of this migration is "developers with a populated local DB re-index from scratch on next daemon start." Pre-v0 (no shipped users; round 2 of 3), this is essentially free. The vault is the source of truth per ADR-0006; clearing `files.content_hash` triggers the existing rebuild path (`Scanner` re-reads content, re-chunks, re-embeds). The rebuild runs at daemon start as part of the initial scan; no manual operator action is required.

Alternative considered: keep migration 0003's L2 default; convert L2 distance to cosine similarity at query time using the unit-vector identity `cos_sim = 1 − L2² / 2`, and document the "embeddings must be unit-normalized" assumption. Rejected: makes correctness depend on a configuration choice external to the daemon (TEI's normalize flag) without any startup-time check. If a future operator runs an embedding service that returns un-normalized vectors, semantic search becomes silently wrong — no error surface alerts the operator, only ranking drifts. Schema-baking removes the ambiguity at the lowest possible layer.

This is a step-6 schema decision that's being amended at step-7 workplan time. The shape is unusual (later step touches an earlier step's schema) but pre-v0 it's the right call: catching it at workplan-write time is cheaper than catching it after v0 ships and we have to migrate live indices. Recorded in step 7's retro as a precedent.

**How to apply**: extend `MIGRATIONS` in `src/store/schema.rs` with a 4th entry containing the SQL above. The migration runner in `apply_migrations()` is already idempotent (advances `user_version` per migration; skips already-applied). Add a unit test asserting `user_version` advances to 4 after `Store::open()` against a fresh DB; another asserting `chunks_vec`'s CREATE statement contains `distance_metric=cosine`; another asserting the dimension-validation regex still extracts 768 from the new CREATE statement. An integration test in `tests/embedding.rs` verifies the existing watcher-driven re-index pipeline still produces non-zero `chunks_vec` rows under the new schema.

**Score conversion in code**: `src/search/semantic.rs` projects `v.distance` from the kNN query and computes `score = (1.0 - (distance as f32) / 2.0).clamp(0.0, 1.0)` per row. The conversion is a one-line helper for testability.

**References**: [`docs/specs/semantic-search.md`](../../../docs/specs/semantic-search.md) line 11 ("cosine similarity") and the response field table (line 75–82 — score as cosine in `[0.0, 1.0]`); [`docs/decisions/0007-sqlite-vec-over-alternatives.md`](../../../docs/decisions/0007-sqlite-vec-over-alternatives.md) lines 27–28 (dimension is schema-level — same shape as distance metric); [`.claude/skills/sqlite-vec-extension/SKILL.md`](../../../.claude/skills/sqlite-vec-extension/SKILL.md) line 105 ("MATCH and k pseudo-column are sqlite-vec idioms; the API has moved around — verify against upstream"); upstream sqlite-vec docs at https://github.com/asg017/sqlite-vec (the agent verifies the `distance_metric=cosine` clause syntax at Task 7.1 time).

---

## Tasks (ordered, each independently mergeable)

Six tasks. Each landing as its own commit per the round-1/round-2 convention (one task = one commit; per-commit results comment includes the SHA).

### Task 7.1 — Migration 0004 (chunks_vec with `distance_metric=cosine`)

**Files**:
- `src/store/schema.rs` (extend) — append a 4th entry to `MIGRATIONS` with the SQL from resolution F. Migration text:
  ```sql
  DROP TABLE chunks_vec;
  DELETE FROM chunks;
  UPDATE files SET content_hash = '';
  CREATE VIRTUAL TABLE chunks_vec USING vec0(
      chunk_id INTEGER PRIMARY KEY,
      embedding FLOAT[768] distance_metric=cosine
  );
  ```
  The agent verifies the exact `distance_metric=cosine` clause against upstream sqlite-vec at task time. If the upstream syntax differs (e.g. `metric=cosine` or a different separator), the agent corrects and notes the verified syntax in the task's results comment.
- `src/store/schema.rs::tests` (extend) — three new tests:
  - `migrations_advance_user_version_to_4` — adapt the existing `migrations_advance_user_version_to_3` test from Task 6.1.
  - `migration_0004_chunks_vec_uses_cosine_metric` — `SELECT sql FROM sqlite_master WHERE name = 'chunks_vec'` and assert the returned text contains `distance_metric=cosine` (or whatever the verified upstream syntax is).
  - `migration_0004_clears_files_content_hash_and_chunks` — pre-populate `files` with a non-empty hash and `chunks` with a row, then run migrations (which apply 0004 on top of 0003). Assert `files.content_hash = ''` and `chunks` is empty after migration.
- `src/store/mod.rs::tests` (touch) — `validate_dimension_matches` and the other dimension tests should keep passing unchanged. If they don't (e.g. the regex fails against the new CREATE statement shape), that's a Task 7.1 regression — the agent fixes the regex and adds a test for the new shape.

**What lands**:
- Database advances to `user_version = 4`. Existing populated DBs are re-indexed automatically on next daemon start (the empty `content_hash` makes every file look "new" to the scanner; the scanner re-reads, re-chunks, re-embeds).
- `chunks_vec`'s schema-baked distance metric is cosine.
- The dimension validation in `Store::open()` still works (the regex matches `FLOAT[768]` regardless of any text after the closing bracket).

**Why a separate task**: schema migrations are immutable post-ship per ADR-0007; even pre-v0, the migration is the load-bearing thing — it deserves its own commit, its own bisect anchor, and a test that asserts the truncation effect. Splitting from the query-module work keeps the schema migration's risk surface narrow.

**Risk: medium-high.**
- *Why medium-high*: this is a step-6 schema amendment, applied additively. The dimension-validation regex (`src/store/mod.rs:134`) was written against migration 0003's exact CREATE statement; if upstream sqlite-vec emits the `distance_metric=cosine` clause inline (e.g. `embedding FLOAT[768] distance_metric=cosine`) the regex still matches, but if it emits it elsewhere or alters the CREATE shape, the regex could fail. The truncation step is also subtle — `DROP TABLE chunks_vec` followed by `DELETE FROM chunks` works because `chunks` doesn't have an FK back to `chunks_vec`, and `chunks_vec` is dropped before `chunks` is touched. (Migration text must run in this order.)
- *Mitigation*: tests assert each property explicitly (regex still matches, version advances, both `chunks` and `files.content_hash` cleared, no orphaned `chunks_vec` rows). The agent verifies the upstream `distance_metric=cosine` syntax before committing the migration text. Forward note to Task 7.5 if the verified syntax differs from the workplan literal so the integration tests use the actual shipped form.

### Task 7.2 — Semantic search query module (`src/search/semantic.rs`, new)

**Files**:
- `src/search/semantic.rs` (new) — implements:
  - `pub struct SemanticQuery { query: String, prefix: Option<String>, limit: usize, min_similarity: f32 }` — the agnostic query shape, parallel to `FilesystemQuery` and `ContentQuery`.
  - `pub struct SemanticResult { score: f32, file_path: String, chunk_index: u32, heading_path: String, text: String }` — the agnostic result shape. `heading_path` stays as the slash-separated TEXT form from step 6 (resolution C in step-06 workplan); the `Vec<String>` split into the spec shape happens in the JSON-projection layer (Task 7.3).
  - `pub enum SemanticSearchError { EmbeddingUnavailable { detail: String }, Internal(anyhow::Error), InvalidPrefix(String) }` — typed error so the HTTP handler can map to `embedding_unavailable` (resolution E), `internal`, and `invalid_prefix` envelope codes respectively.
  - `pub async fn search_semantic(pool: SqlitePool, embedder: Arc<dyn Embedder>, dimension: u32, q: SemanticQuery) -> Result<(Vec<SemanticResult>, Option<String>), SemanticSearchError>` — the load-bearing entry point. Returns the result set and an optional hint (resolution B). The flow:
    1. Validate the prefix via `super::normalize_prefix` (returns `InvalidPrefix` on absolute or `..` paths — match the existing precedent from `src/search/mod.rs:9-24`).
    2. Embed the query: `let v = embedder.embed_text(&q.query).await.map_err(classify_embedding_error)?` — see classification table below.
    3. Defense-in-depth: assert `v.len() == dimension as usize`; if not, return `EmbeddingUnavailable { detail: "..." }` (the production `EmbeddingClient` already validates this; the assert covers stub embedders or future custom impls).
    4. Run the kNN SQL inside `spawn_blocking` (per the [`rusqlite-in-async`](../../../.claude/skills/rusqlite-in-async/SKILL.md) skill). The SQL projects `chunks` columns + the score conversion + applies prefix filter + `ORDER BY ... LIMIT ...`. After the kNN result is in hand and is empty, the same `spawn_blocking` runs the two count queries (resolution B) and decides whether to return a hint.
- `src/search/mod.rs` (extend) — `mod semantic;` and `pub use semantic::{SemanticQuery, SemanticResult, SemanticSearchError, search_semantic};`.
- `src/lib.rs` (touch — likely no change since `pub mod search;` is already there from step 5).

**Embedding error classification** (Resolution E, in code):

```rust
fn classify_embedding_error(e: EmbeddingError) -> SemanticSearchError {
    match e {
        EmbeddingError::Transport(err) => SemanticSearchError::EmbeddingUnavailable {
            detail: format!("embedding service is unreachable: {err}"),
        },
        EmbeddingError::Status { code, .. } if (500..=599).contains(&code) => {
            SemanticSearchError::EmbeddingUnavailable {
                detail: format!("embedding service returned HTTP {code}"),
            }
        }
        EmbeddingError::DimensionMismatch { expected, actual } => {
            SemanticSearchError::EmbeddingUnavailable {
                detail: format!(
                    "embedding service returned a vector with dimension {actual}; \
                     daemon expected {expected}"
                ),
            }
        }
        EmbeddingError::Status { code, body } => SemanticSearchError::Internal(anyhow::anyhow!(
            "embedding service returned unexpected HTTP {code}: {body}"
        )),
        EmbeddingError::BodyParse(err) => SemanticSearchError::Internal(anyhow::anyhow!(
            "embedding service response could not be parsed: {err}"
        )),
    }
}
```

**SQL shape** (the kNN + cosine + prefix + ORDER BY + LIMIT):

```sql
WITH knn AS (
    SELECT chunk_id, distance
    FROM chunks_vec
    WHERE embedding MATCH ?1
      AND k = ?2
)
SELECT
    c.file_path,
    c.chunk_index,
    c.heading_path,
    c.content,
    knn.distance
FROM knn
JOIN chunks c ON c.id = knn.chunk_id
WHERE (?3 = '' OR (c.file_path >= ?3 AND c.file_path < ?4))
ORDER BY knn.distance ASC, c.file_path ASC, c.chunk_index ASC;
```

Bind parameters: `?1` = `bytemuck::cast_slice::<f32, u8>(&query_vec)` (per the [`sqlite-vec-extension`](../../../.claude/skills/sqlite-vec-extension/SKILL.md) skill line 72); `?2` = `q.limit as i64` (the kNN's `k`; we use `min_similarity`-filtering after kNN so `k = limit` is right); `?3` = normalized prefix or `""`; `?4` = `prefix_successor(prefix)` if non-empty else `""`. The score is computed in Rust (`1.0 - distance / 2.0`, clamped) and `min_similarity` is filtered in Rust as well — keeping the SQL focused and the math testable in isolation.

The agent verifies the exact `MATCH` / `k` / parameter-binding syntax against upstream sqlite-vec at task time per the [`sqlite-vec-extension`](../../../.claude/skills/sqlite-vec-extension/SKILL.md) skill (line 105). The CTE shape is one option; an inline `WHERE ... MATCH ... AND k = ...` with a JOIN to `chunks` is another. Whichever ships, the test in `chunk_count_matches_chunker_for_known_fixture` (Task 7.5) is the end-to-end correctness check.

**Note on prefix filtering and kNN**: prefix filtering happens *after* kNN (in the WHERE clause of the outer SELECT, not as a constraint inside the kNN MATCH). This means if the user specifies a prefix and there are many non-matching chunks, the top-`limit` kNN candidates may include some that get filtered out — leaving fewer than `limit` results in the response. This is acceptable for v0 (the spec doesn't promise exactly `limit` results post-prefix), and the alternative (over-fetch from kNN, then filter, then re-bound) is a v0+ optimization. The SQL above uses `k = ?limit`; if the prefix is highly selective, a future step can over-fetch (e.g. `k = limit * 10` capped at some ceiling) and trim.

**Tests in `src/search/semantic.rs::tests`**:
- `search_semantic_returns_results_for_known_chunks` — pre-populate `chunks` and `chunks_vec` (stubbed embedding via direct SQL insertion using `bytemuck::cast_slice`), call `search_semantic` with a stub embedder returning a known query vector, assert the top result has the expected `file_path`/`chunk_index` and that `score` is computed via `1.0 - distance/2.0`.
- `search_semantic_returns_hint_when_chunks_vec_empty_and_files_present` — populate `files` (not `chunks`); call `search_semantic`; assert empty results AND `Some("semantic index is building".to_string())` for the hint.
- `search_semantic_returns_no_hint_when_files_empty` — fresh DB; assert empty results AND `None` hint.
- `search_semantic_returns_no_hint_when_chunks_present_but_no_match` — populate `chunks`/`chunks_vec` with vectors orthogonal to the query; assert empty-after-min_similarity-filter results AND `None` hint (because `chunks_vec` count > 0).
- `search_semantic_respects_limit` — populate N chunks, call with `limit < N`, assert exactly `limit` results.
- `search_semantic_respects_min_similarity` — populate chunks at known similarity scores, call with `min_similarity = 0.5`, assert only chunks with `score >= 0.5` returned.
- `search_semantic_clamps_min_similarity_negatives_to_zero` — call with `min_similarity = -1.0`, assert behaves as `0.0` (returns everything the kNN found).
- `search_semantic_respects_prefix_scoping` — populate chunks under `notes/` and `archive/`, call with `prefix = "notes/"`, assert only `notes/` results.
- `search_semantic_orders_ties_deterministically` — populate two chunks with byte-identical embeddings (same `file_path` they shouldn't be possible at, but two chunks under different files can — use that case), call kNN; assert ordering by `file_path` ASC.
- `search_semantic_classifies_embedding_transport_error` — stub embedder returns `EmbeddingError::Transport(_)` (the test fabricates a `reqwest::Error` via a request to a closed listener, mirroring the pattern in `src/embedding.rs::tests::embed_retries_once_on_connection_refused`); assert `Err(SemanticSearchError::EmbeddingUnavailable { .. })`.
- `search_semantic_classifies_embedding_5xx` — stub returns `Status { code: 503, .. }`; assert `EmbeddingUnavailable`.
- `search_semantic_classifies_embedding_dimension_mismatch` — stub returns wrong-dim vector (e.g. 4 floats when dimension is 768) — note: the stub embedder must return its raw `Vec<f32>` regardless of dimension; the trait's contract is "embed_text returns `Result<Vec<f32>, EmbeddingError>`", and a custom stub that just returns the wrong length triggers the defense-in-depth dimension check in `search_semantic` itself, not the production `EmbeddingClient`'s check. Assert `EmbeddingUnavailable` with the dimension detail in the message.
- `search_semantic_classifies_4xx_as_internal` — stub returns `Status { code: 400, body: "bad request" }`; assert `Internal(_)`.
- `search_semantic_classifies_body_parse_as_internal` — stub returns `BodyParse(_)`; assert `Internal(_)`.
- `search_semantic_invalid_prefix` — call with `prefix = Some("/abs")`; assert `Err(SemanticSearchError::InvalidPrefix(_))`.

**A custom test stub embedder**: the existing `StubEmbedder` in `src/embedding.rs:182-197` returns a fixed zero-filled vector and never errors. Task 7.2's tests need an embedder that can return errors and configurable shapes. Add a `TestEmbedder` enum or builder inside `src/search/semantic.rs::tests` (test-only, `#[cfg(test)]`) — the existing `StubEmbedder` stays generic for tests that just need a "happy embedder."

**Why a separate task**: this is the load-bearing pure-logic surface for step 7. Split from the HTTP wiring keeps the tests focused on the query/embedding contract; split from the migration keeps the bisect window tight (a regression in semantic.rs doesn't bisect into the schema decision).

**Risk: medium.**
- *Why medium*: the kNN SQL shape is the new SQL contract; the score conversion is the new wire contract. Both are exercised by tests. The trait wrapping `Embedder` is already in tree from step 6, so the async/blocking dance is well-trodden — embedding goes on the runtime, SQL goes in `spawn_blocking`. The classification matrix from resolution E is the most error-prone surface; the unit tests cover each branch.
- *Mitigation*: the SQL is exercised against a real (in-memory or temp) `chunks_vec` in tests; the score conversion has a dedicated test; the classification matrix has one test per branch; the `min_similarity` clamping has a test. The `rusqlite-in-async` skill is the load-bearing reference for the boundary.

### Task 7.3 — HTTP handler + types + error envelope wiring

**Files**:
- `src/api/types.rs` (extend) — add:
  - `pub struct SemanticQueryJson { query: String, prefix: Option<String>, limit: Option<usize>, min_similarity: Option<f32> }` — request shape, parallel to `FilesystemQueryJson` / `ContentQueryJson`. `min_similarity` is `Option` because the *client* may omit it; the handler defaults to `0.0`.
  - `pub struct SemanticSearchResponse { results: Vec<SemanticResultJson>, #[serde(skip_serializing_if = "Option::is_none")] hint: Option<String> }`.
  - `pub struct SemanticResultJson { score: f32, file_path: String, chunk_index: u32, heading_path: Vec<String>, text: String, #[serde(skip_serializing_if = "Option::is_none")] vault: Option<String> }` — the spec shape with the `vault` forward-compat field per step 5's resolution.
- `src/api/mod.rs` (extend) — `ApiState` gains:
  - `pub embedder: Arc<dyn Embedder>` — the same shared embedder the `Scanner` already holds; no new construction site, just threading.
  - `pub embedding_dimension: u32` — for the defense-in-depth dimension assert in `search_semantic`. Threaded from `config.embedding.dimension`.
  - The `router()` function adds `.route("/search/semantic", post(search::semantic))`.
- `src/api/search.rs` (extend) — add:
  - `pub(crate) async fn semantic(State(s): State<ApiState>, ApiJson(req): ApiJson<SemanticQueryJson>) -> Result<Json<SemanticSearchResponse>, ApiError>` — the handler. Builds a `SemanticQuery` from the JSON shape (defaulting `limit` to `DEFAULT_LIMIT` and `min_similarity` to `0.0`, clamping to `[0.0, 1.0]`), calls `search_semantic`, splits the slash-separated `heading_path` to a `Vec<String>` (per step-6 resolution C — the projection happens here, at the JSON boundary), wraps in `SemanticResultJson` with `vault: None`, returns the `SemanticSearchResponse` with the hint passed through.
  - The `heading_path` projection: `path.split('/').map(String::from).collect()` — the empty-segment edge case (orphan-H3 producing `"Setup//Prereqs"`) round-trips to `["Setup", "", "Prereqs"]` per step-6 resolution C.
- `src/api/error.rs` (extend) — add `From<SemanticSearchError> for ApiError`:
  - `EmbeddingUnavailable { detail }` → `ApiError { status: StatusCode::SERVICE_UNAVAILABLE, code: "embedding_unavailable", message: detail }`.
  - `InvalidPrefix(detail)` → reuses the existing `invalid_prefix` mapping path (or directly: `ApiError { status: BAD_REQUEST, code: "invalid_prefix", message: detail }`).
  - `Internal(err)` → `ApiError::internal()` after a `tracing::error!` log line (mirroring the existing `internal` path at line 74).
  - The handler in `search.rs` invokes `search_semantic(...).await.map_err(ApiError::from)?` so the `?` short-circuits to the right envelope.
- `src/bin/hmnd.rs` (extend) — `ApiState` construction in `run_daemon()` (line 137–141 today) gains:
  ```rust
  let api_state = api::ApiState {
      pool: store.pool(),
      vault: config.vault.0.clone(),
      outbox_path: outbox_path.clone(),
      embedder: embedder.clone(),                  // already constructed at line 101 above; just clone the Arc
      embedding_dimension: config.embedding.dimension,
  };
  ```
  The `embedder` is the same `Arc<dyn Embedder>` already constructed for the `Scanner`; we just clone the Arc.
- `src/api/tests.rs` (extend) — `harness()` (line 22–39) and `seed_files()` are touched to construct `ApiState` with an embedder. Use `StubEmbedder::new(768)` (the existing stub from `src/embedding.rs:182-197`) for the harness — the unit tests in `src/api/tests.rs` don't need an erroring embedder; the per-error-path coverage lives in `src/search/semantic.rs::tests`.
- `src/client.rs::tests` (touch) — `spawn_test_daemon()` (line 125–154) constructs `ApiState`; touch to add the embedder. Same `StubEmbedder::new(768)` pattern.

**Tests in `src/api/tests.rs`** (new):
- `semantic_handler_returns_200_with_results_for_seeded_chunks` — pre-populate `chunks` + `chunks_vec` directly via SQL (using `bytemuck::cast_slice` for the embedding blob); POST `/search/semantic` with `{"query":"anything"}`; assert 200 and a non-empty `results` array. The stub embedder returns a fixed zero-filled vector regardless of input, so the kNN distance will be deterministic against the seeded vectors.
- `semantic_handler_returns_503_for_embedding_unavailable` — wire a custom test embedder that returns `EmbeddingError::Transport(_)`; POST `/search/semantic`; assert status 503 and body `error.code == "embedding_unavailable"`.
- `semantic_handler_returns_400_for_invalid_prefix` — POST with `{"query":"x","prefix":"/abs"}`; assert 400 and `error.code == "invalid_prefix"`.
- `semantic_handler_omits_vault_field_in_v0_response` — assert each result's serialized JSON does not contain a `"vault"` key (per step 5's resolution about the forward-compat field; `#[serde(skip_serializing_if = "Option::is_none")]` keeps it omitted).
- `semantic_handler_returns_hint_when_index_empty_and_files_present` — pre-populate `files` (no chunks); POST `/search/semantic`; assert empty `results` and `body.hint == "semantic index is building"`.
- `semantic_handler_clamps_min_similarity_to_unit_range` — POST with `{"query":"x","min_similarity":1.5}`; assert 200 (no error from the high value); empty results (because no chunk can have score > 1).
- `semantic_handler_default_limit_is_DEFAULT_LIMIT` — POST without `limit`; pre-populate more than `DEFAULT_LIMIT` chunks; assert exactly `DEFAULT_LIMIT` results.

**Manual smoke verification** (the same shape as Task 6.5's smoke per round-1/round-2 precedent — see step-06 retro): the agent runs the daemon against a tempdir vault, populates a few `.md` files, waits for the watcher cycle to complete, and confirms with `curl http://127.0.0.1:7777/search/semantic` (with a JSON body) that:

1. Healthy path: with the embedding service up and chunks indexed, `/search/semantic` returns 200 with a non-empty results array.
2. Empty-index path: starting against a populated `files` table but pre-chunking (or with the embedding service down so no chunks land), `/search/semantic` returns 200 with empty results and the hint.
3. Embedding-unavailable path: starting with the embedding service down (or returning 503), `/search/semantic` returns 503 with `code: embedding_unavailable`.

Smoke verification documented in the task's results comment, with `curl` transcripts.

**What lands**:
- `POST /search/semantic` works end-to-end. The CLI doesn't yet (Task 7.4); HTTP smoke via `curl` is sufficient for this task's gate.
- The new `embedder` field on `ApiState` is wired through every existing construction site.
- `embedding_unavailable` is a new error envelope code in the wire surface.

**Why a separate task**: the HTTP handler is the consumer-facing surface; splitting it from the query module (Task 7.2) keeps each commit's surface narrow. The error-envelope mapping is small enough to ride along here rather than in its own commit (parallel to the round-1 step-5 pattern where new envelope codes shipped with the handler that introduced them). Manual smoke verification belongs here for the same reason as Task 6.5: it's the wiring task, and wiring tasks miss bugs that unit tests can't see.

**Risk: medium-high.**
- *Why medium-high*: every test in the codebase that constructs an `ApiState` (`src/api/tests.rs` harness, `src/client.rs::tests::spawn_test_daemon`, `tests/http.rs` `spawn_live_daemon`, `tests/embedding.rs` `spawn_live_daemon`) needs a touch. This is the kind of signature ripple step 6's pre-build directive 4 named ("30+ call sites" for the `EmbeddingConfig` thread). The error-envelope mapping is the contract the CLI in Task 7.4 reads.
- *Mitigation*: a quick `grep -rn "ApiState {" src tests` at task start enumerates every construction site; updating each to add `embedder` and `embedding_dimension` is mechanical. Manual smoke is the safety net for any wiring slip the unit tests miss.

### Task 7.4 — CLI `hmn search semantic` + `DaemonClient::search_semantic`

**Files**:
- `src/client.rs` (extend) — add `pub async fn search_semantic(&self, q: &SemanticQueryJson) -> Result<SemanticSearchResponse>`. Mirror the shape of `search_filesystem` and `search_content` (line 47–61). Re-export `SemanticQueryJson`, `SemanticResultJson`, `SemanticSearchResponse` from `crate::api::types::*`.
- `src/client.rs::tests` (extend) — add `client_search_semantic_round_trips` — using the harness from `spawn_test_daemon()`, send a semantic search request and assert the response shape.
- `src/cli.rs` (touch — already has `SearchMode::Semantic { query, prefix, limit }` from step 1; no shape change needed).
- `src/bin/hmn.rs` (extend) — replace the stub at line 69–72:
  ```rust
  SearchMode::Semantic { query, prefix, limit } => {
      cmd_search_semantic(&config, cli.daemon_url.as_deref(), cli.json, query, prefix, limit).await
  }
  ```
  Add `async fn cmd_search_semantic(...)` parallel to `cmd_search_content` (line 115–140). Wraps `DaemonClient::search_semantic`. Renders text mode with one block per result:
  ```
  notes/databases/pgvector.md  (score: 0.82)
    > Architecture / Indexing
    Pgvector supports HNSW indexes…
  ```
  The `> ` line is the joined `heading_path` (filtering out empty segments — e.g. orphan-H3 `["Setup","","Prereqs"]` renders as `"Setup / Prereqs"`). The text body is the `text` field, optionally truncated for terminal width if it's longer than ~240 chars (matching the existing content-search trim-bytes heuristic; not load-bearing).
  When the response carries a `hint`, render it after the results (or instead of, if results is empty):
  ```
  (semantic index is building)
  ```
  JSON mode prints the response unchanged via `print_json`.
- `src/bin/hmn.rs::tests` (extend) — add a unit test for the text rendering helper (`render_semantic_text`), parallel to `filesystem_line_has_path_size_mtime_layout` (line 236–249).

**Tests** (across the files above):
- `client.rs::tests::client_search_semantic_round_trips` — round-trip via `spawn_test_daemon`. Pre-populate chunks via `state.pool` directly. Assert response shape.
- `bin/hmn.rs::tests::render_semantic_text_includes_score_and_heading_path` — verify the human-readable layout.
- `bin/hmn.rs::tests::render_semantic_text_filters_empty_heading_segments` — orphan-H3 case renders without empty segments.
- `bin/hmn.rs::tests::render_semantic_text_renders_hint_when_present` — when `hint` is `Some("...")`, the rendered output includes the hint suffix.

**What lands**:
- `hmn search semantic <query>` works end-to-end against a running daemon.
- The CLI exit code semantics from `is_connect_error()` (line 80–81 in `bin/hmn.rs` — exit 4 for "daemon not reachable") apply unchanged. Embedding-service-unavailable from the daemon is not a connect error (the daemon answered 503); it surfaces as a normal `Err(anyhow!("embedding_unavailable: ..."))` from the existing `decode_response()` (line 67–84 in `client.rs`) and exits 1 with the message visible.

**Why a separate task**: the CLI is a thin wrapper over the HTTP client; splitting from the handler (Task 7.3) keeps the surface tight. The CLI test surface is small enough to ride in this task's commit.

**Risk: low-medium.**
- *Why low-medium*: the wrapper shape is well-trodden (`search_filesystem` and `search_content` are the templates). The new error path (`embedding_unavailable`) flows through the existing `decode_response` and surfaces as an `anyhow::Error` whose `Display` includes the code — the existing CLI rendering at `bin/hmn.rs:81-86` handles it without special-casing.
- *Mitigation*: tests cover the round-trip and the text rendering. No special-case error handling in the CLI.

### Task 7.5 — Integration tests against live daemon + stub embedding service

**Files**:
- `tests/embedding.rs` (extend) — the existing file already wires a stub embedding service and a live `hmnd`-style stack from step 6. Reuse the fixture; add new test cases:
  - `semantic_search_returns_results_after_indexing` — write a known fixture (e.g. three H2-separated sections), wait for the watcher debounce + indexer cycle, POST `/search/semantic` with a query string, assert non-empty `results` (with the stub embedder's deterministic vectors, the kNN ordering is deterministic too — the test asserts the count matches, not specific content, since the stub returns fixed zero-vectors that result in distance-0 ties).

    *Actually*: with `StubEmbedder::new(768)` returning all-zero vectors, every chunk's stored embedding is the zero vector and the query vector is also zero, so the cosine distance is undefined (`0/0`). Two options for this test:
    - (a) Use a custom test embedder that returns different deterministic vectors for different inputs (e.g. hash the input to seed a deterministic non-zero vector). This makes the kNN ordering meaningful.
    - (b) Keep `StubEmbedder` for this test and assert only that the response shape is right (`results` is `Vec`, `hint` is `None` once chunks land), not the ordering.

    Pick (a): a `DeterministicHashEmbedder` (test-only, in `tests/embedding.rs`) that returns `[hash(text) per slot]` normalized to a unit vector. Add this helper at task time.
  - `semantic_search_returns_hint_when_index_empty_after_files_seeded` — pre-populate `files` (e.g. by writing `.md` files but configuring the stub embedder to fail so chunks never land — reuse `StubMode::Err503`); wait for the watcher; POST `/search/semantic`; assert empty `results` + the hint.
  - `semantic_search_returns_503_when_embedding_service_unavailable_at_query_time` — populate chunks (stub up briefly, then bring it down — or use a separate query-time stub from index-time stub, easier path: configure the stub to switch modes mid-test). POST `/search/semantic` while the stub returns 503; assert HTTP 503 and body `error.code == "embedding_unavailable"`.
  - `semantic_search_against_wrong_dimension_at_query_time_returns_503` — stub returns wrong-dim vector at query time; assert HTTP 503 (Resolution E's DimensionMismatch path).
  - `semantic_search_full_round_trip_via_hmn_binary` — exec the `hmn` binary with `search semantic <query>` against the live daemon; assert exit 0 and stdout contains the expected score-and-path lines. Mirror the pattern from `tests/cli.rs` (the round-1 CLI integration test surface).
- `tests/embedding.rs` reuses the existing `StubServer`, `Fixture`, and `LiveDaemon` infrastructure. The `Fixture::config` may need a small extension to vary the stub mode mid-test (a `set_mode` method on `StubServer` that updates an `Arc<Mutex<StubMode>>` — workplan-time sketch; the agent picks the cleanest shape).
- 3× consecutive flake-check budget per the round-1/round-2 anti-flake rule (run `cargo test --test embedding` three times locally without flakes before reporting green; matches Task 6.6 precedent).

**What lands**:
- Five new integration cases (or six, depending on whether the binary-exec test stays in `tests/embedding.rs` or moves to `tests/cli.rs`).
- The end-to-end `hmn search semantic` path is exercised against a real subprocess.
- The 503 and dimension-mismatch paths are exercised against a real HTTP request, not just unit tests.

**Why a separate task**: integration test surface deserves its own commit and bisect anchor; the cross-references between the stub-service helper, the dynamic-mode tweak, and the hmn-binary exec are easier to audit when isolated. Matches step 6's task 6.6 precedent.

**Risk: medium.**
- *Why medium*: stub-service timing is the perennial flake source; the watcher debounce window adds a second timing axis. The 3× flake-check budget is the safety net. Mid-test stub-mode changes (for the "outage at query time" case) introduce a third axis; the helper for this should be carefully designed (a single `Arc<Mutex<StubMode>>` updated atomically before the next request, with the test ensuring no in-flight stub request is racing).
- *Mitigation*: copy the stub-service helper from the existing `tests/embedding.rs` rather than reinventing; the mid-test mode change is gated behind a `set_mode` method that takes a `&mut self` (no concurrent access). Forward note from Task 6.6 to Task 7.5 if any anti-flake patterns emerged late in step 6 that this step should adopt.

### Task 7.6 — Reference docs reflect step-7 resolutions

**Files**:
- `docs/reference/configuration.md` (touch) — no new config knobs in step 7 (resolution D pins "no cache" in v0); no changes expected unless the agent surfaces a doc gap. Likely a no-op.
- `docs/reference/cli.md` (extend) — flip the "as of step 5, `hmn search semantic` continues to print 'lands in step 7'" wording (line 158 currently) to reflect that semantic search ships in step 7. Add a brief example of the rendered output.
- `docs/architecture/overview.md` (touch) — extend the search-API description to include `/search/semantic`; cross-reference the `embedding_unavailable` envelope code.
- `docs/specs/semantic-search.md` (extend) — add the `hint: Option<String>` field to the response schema (per resolution B); add the `min_similarity` clamping rule (per resolution A); pin the score conversion (per resolution F): "`score = 1.0 - (vec0_distance / 2.0)`, clamped to `[0.0, 1.0]`"; add a short note that the schema-baked distance metric is `cosine` (per resolution F); flip the spec status from `Draft` to `Stable` (or whatever the project's convention is for shipped specs — verify against `docs/specs/_template.md` and prior specs).
- `docs/decisions/0007-sqlite-vec-over-alternatives.md` (touch — possibly add an Amendment) — the ADR's "dimension is baked into the schema" claim still stands; add a brief amendment noting that the *distance metric* is also schema-baked (an additive, not a contradictory, claim). The amendment is one paragraph in the existing `## Amendments` section (currently `<!-- None yet -->` at line 60).

**What lands**:
- Documentation is consistent with what step 7 actually built. The spec accurately describes the `hint` field, the `min_similarity` semantics, and the score conversion. The CLI doc reflects the working `hmn search semantic`. The ADR amendment captures the cosine-metric resolution.

**Why a separate task**: doc-only by design; lands at the boundary so any soft-flag-to-coordinator from earlier tasks (e.g. wording corrections, syntax clarifications surfaced by the upstream sqlite-vec verification in Task 7.1) can be incorporated.

**Risk: low.**

---

## Test strategy

**Unit tests** (`#[cfg(test)]`):
- `src/store/schema.rs::tests` — three new cases for migration 0004 (Task 7.1).
- `src/search/semantic.rs::tests` — ~13 cases for the query module (Task 7.2): result-set shape, hint behavior across three scenarios, limit / min_similarity / prefix / ordering / classification matrix (one case per `EmbeddingError` variant).
- `src/api/tests.rs` — ~7 cases for the HTTP handler (Task 7.3): 200/400/503 paths, hint response, vault forward-compat omission, default limit, min_similarity clamping.
- `src/client.rs::tests` — 1 case for the new `DaemonClient::search_semantic` (Task 7.4).
- `src/bin/hmn.rs::tests` — 3 cases for the text rendering (Task 7.4).

**Integration tests** (`tests/`):
- `tests/embedding.rs` — 5 (or 6) new cases for the live-daemon round trip (Task 7.5). Reuses the existing stub-service + fixture from step 6. 3× consecutive flake-check budget at task close.

**Manual smoke** (Task 7.3):
- Run the daemon against a tempdir vault. Verify the three `/search/semantic` paths via `curl`: healthy, empty-index hint, embedding-unavailable. Documented in the task's results comment with transcripts.

**Lint and format**:
- `cargo clippy --all-targets -- -D warnings` clean.
- `cargo fmt --check` clean.

**Cross-platform notes**:
- The sqlite-vec extension binary remains operator-provisioned at the configured `embedding.extension_path` (or `HYPOMNEMA_VEC_EXT_PATH` env-var override) per step 6's resolution / docs/reference/configuration.md. No new platform concerns in step 7.
- Migration 0004's `distance_metric=cosine` clause is a sqlite-vec syntax that the agent verifies against upstream at task 7.1 time.

**Anti-flake rules** (carried forward from round 1 and round 2 step 6):
- Do **not** introduce a polling-loop helper that hides timing in `tests/embedding.rs` — flakes on a non-deterministic boundary are signal, per the step-3 retro and step-6 task 6.6 precedent.
- Mid-test stub-mode changes (Task 7.5) are gated through a `set_mode` method that holds an `Arc<Mutex<...>>`; no concurrent writes; the test's ordering is "change the mode, then issue the request that should observe it."

---

## Definition of done

- [ ] Migration 0004 lands; `chunks_vec`'s schema-baked distance metric is `cosine`; `user_version` advances to `4` (Task 7.1).
- [ ] `Store::open()` against a populated DB at user_version 3 advances to 4, clears `chunks` and `files.content_hash`, and the next scan re-populates.
- [ ] `POST /search/semantic` returns the spec response shape against an indexed vault; ranking is sensible (top result is similar to the query in the test fixture).
- [ ] `hmn search semantic 'how do we prevent spurious reindexes'` against an indexed vault returns chunks with similarity scores (criterion 1).
- [ ] `curl http://127.0.0.1:7777/search/semantic` with a JSON body returns the spec response shape (criterion 2).
- [ ] Empty index returns empty `results` + `hint == "semantic index is building"` (criterion 3).
- [ ] Result shapes include the `vault: Option<String>` forward-compat field; v0 always omits it from the wire (criterion 4).
- [ ] Embedding-service unavailability at query time returns HTTP 503 with `code: "embedding_unavailable"`, not 500 (criterion 5).
- [ ] Dimension mismatch at query time also returns 503 `embedding_unavailable` (per resolution E + step-6 contract).
- [ ] All new unit and integration tests pass; `cargo clippy --all-targets -- -D warnings` clean; `cargo fmt --check` clean.
- [ ] Manual smoke verification per Task 7.3 documented in the task's results comment with `curl` transcripts.
- [ ] All step-5 and step-6 tests still pass (no regression on filesystem/content search, HTTP plumbing, indexer pipeline, or schema validation).
- [ ] `docs/specs/semantic-search.md` reflects the resolutions (`hint` field, `min_similarity` clamping, score conversion, cosine metric).
- [ ] `docs/reference/cli.md` reflects working `hmn search semantic` (no longer a stub).
- [ ] `docs/architecture/overview.md` reflects the new `/search/semantic` route and `embedding_unavailable` envelope code.
- [ ] `docs/decisions/0007-sqlite-vec-over-alternatives.md` § Amendments reflects the cosine distance metric (resolution F).
- [ ] Step 7 retrospective appended to [`notes/project-planning-workflow-notes.md`](../../project-planning-workflow-notes.md) following the retro template.
- [ ] [`notes/roadmap/archive/roadmap-2.md`](./roadmap-2.md) § Step 7 marked `**Status**: shipped <date>`.
- [ ] No fall-out resolutions or in-build TBDs left undocumented (workplan and code agree at the end; soft flags routed to coordinator at boundary).

---

## Cross-references

**Skills (load-bearing)**:
- [`.claude/skills/sqlite-vec-extension/`](../../../.claude/skills/sqlite-vec-extension/SKILL.md) — Tasks 7.1 and 7.2; cited at the migration text (resolution F + Task 7.1), at the `MATCH` / `k` query syntax (Task 7.2), and at the `bytemuck::cast_slice` blob conversion for the query-vector parameter binding. *Note*: skill currently uses the table name `chunk_vectors` (line 49) but the codebase and spec settle on `chunks_vec` per step-6 resolution A — the workplan and this step's code use `chunks_vec` consistently. The skill should be re-aligned in a follow-on edit (carryover from step 6).
- [`.claude/skills/rusqlite-in-async/`](../../../.claude/skills/rusqlite-in-async/SKILL.md) — Task 7.2's load-bearing contract for the async/blocking boundary. The kNN SQL goes inside `spawn_blocking`; the embedding HTTP call lives on the runtime — same pattern as step 6's indexer pipeline. Cited at the `search_semantic` function body.

**ADRs**:
- [`docs/decisions/0003-indexing-in-the-daemon.md`](../../../docs/decisions/0003-indexing-in-the-daemon.md) — semantic search is served from the daemon's index, not from a separate vector DB.
- [`docs/decisions/0004-three-search-modes-as-peers.md`](../../../docs/decisions/0004-three-search-modes-as-peers.md) — semantic is the third peer, alongside filesystem and content; same router shape, same envelope shape, same CLI shape.
- [`docs/decisions/0005-local-everything.md`](../../../docs/decisions/0005-local-everything.md) — query embeddings call the same local embedding service step 6 wired in; no cloud calls.
- [`docs/decisions/0007-sqlite-vec-over-alternatives.md`](../../../docs/decisions/0007-sqlite-vec-over-alternatives.md) — schema is the authority for vector storage; resolution F's amendment adds "distance metric is also schema-baked" to the existing "dimension is schema-baked" claim.

**Specs**:
- [`docs/specs/semantic-search.md`](../../../docs/specs/semantic-search.md) — the public response shape this step's handler implements. Task 7.6 amends the spec with the `hint` field, `min_similarity` clamping, score conversion, and cosine metric resolution.

**Prior workplans / retros**:
- [`notes/roadmap/archive/step-06-workplan.md`](./step-06-workplan.md) — the step-6 substrate this step queries. Resolutions to step-6 (A: `chunks_vec` table name, B: `chunk_index` column, C: slash-separated `heading_path`) are all consumed by step 7's query and JSON-projection layers.
- [`notes/roadmap/archive/step-05-workplan.md`](./step-05-workplan.md) — the HTTP/CLI/error-envelope shape this step extends. The `vault: Option<String>` forward-compat field, the error-envelope token-prefix mapping in `src/api/error.rs`, and the human-vs-JSON CLI rendering all follow step 5's precedent.
- [`notes/project-planning-workflow-notes.md`](../../project-planning-workflow-notes.md) § Step 6 retro — three patterns feeding into this step: (a) coordinator-spawned in-build follow-up for cross-task design tensions surfaced via smoke (the act-now decision rule); (b) manual smoke verification on medium-risk wiring tasks; (c) workplan-prose accuracy heuristic at ~1000-line threshold (this workplan is ~720 lines, under threshold; voluntary spot-check ran).

**Roadmap and tech-stack**:
- [`notes/roadmap/archive/roadmap-2.md`](./roadmap-2.md) § Step 7 — the contract this workplan resolves.
- [`docs/implementation/tech-stack.md`](../../../docs/implementation/tech-stack.md) — semantic search composes the same SQLite + sqlite-vec stack steps 1–6 built; no new top-level components.

---

## Out of scope (will not appear in this PR)

- MCP wrapper for `/search/semantic` — that's step 8. The query function in `src/search/semantic.rs` is the surface step 8 will wrap.
- Reranking via a cross-encoder — open question in the spec (§ Open Questions); not v0.
- Hybrid search (RRF over semantic + content) — open question in the spec; not v0.
- Adjacent-chunk context in responses — open question in the spec; not v0.
- Query embedding caching (per-process LRU, persistent) — resolution D; not v0.
- Non-zero default `min_similarity` — resolution A; consumers can opt in per query, but the default stays `0.0` until corpus evidence supports tuning.
- Multi-vault scoping (`vaults` request filter, `vault_name` per result) — round 3 work per ADR-0009/0010/0011. The `vault: Option<String>` forward-compat field on per-result shapes is preserved (always None in v0).
- Bulk re-indexing UX (a `hmn rebuild` subcommand) — migration 0004's auto-rebuild covers the v0 cost; future operator UX is not a v0 concern.
- Compile-time bundling of the sqlite-vec extension — release-packaging concern; out of v0 (carryover from step 6 § Out of scope).

---

## Net new dependencies

None. The crates this step uses (`reqwest`, `bytemuck`, `rusqlite` with `load_extension`, `serde`, `serde_json`, `tokio`, `axum`, `anyhow`, `regex`, `globset`) are all in tree from steps 1–6.

---

## Process dependencies

- The sqlite-vec extension binary (operator-provisioned per step 6's resolution / `docs/reference/configuration.md`) must be present at `embedding.extension_path` for migration 0004 to succeed — same operator prereq as step 6. The dev-shell `flake.nix` provisioning question carries forward from step 6's boundary follow-up (not blocking; the agent uses the existing manual-download path).
- The pre-existing outbox flake under `cargo nextest run` fail-fast cancellation (step-6 retro § Step-boundary follow-ups) is not caused by this step and is not blocking — the agent runs tests with `--no-fail-fast` if it surfaces.
- Step 6's resolution / Task 6.4r1 contract ("the daemon never crashes due to embedding service issues, anywhere in the runtime") is the load-bearing precedent for resolution E's DimensionMismatch-at-query-time mapping. No further playbook or workflow-notes edits are required before step 7's build.

---

## Self-review for prose accuracy

This workplan came in at ~720 lines, under the ~1000-line threshold of the Phase B heuristic. The heuristic doesn't formally fire, but a voluntary accuracy spot-check ran given the density of external-library and prior-step claims (sqlite-vec syntax, score conversion math, step-6 resolutions). Results below.

**Claims that were re-checked**:

- sqlite-vec's `MATCH` / `k = ?` query shape and `distance_metric=cosine` clause syntax — confirmed against the [`sqlite-vec-extension`](../../../.claude/skills/sqlite-vec-extension/SKILL.md) skill, which itself directs the agent to verify against upstream (line 105). The workplan defers final syntax to Task 7.1's verification step (acknowledged in resolution F and Task 7.1 prose). This is a *narrow* prose-accuracy escape hatch — the verification gate is in the task itself, not deferred to soft-flag.
- Cosine distance range `[0, 2]` and the score conversion `score = 1 - distance/2` — confirmed against the standard `cos_distance = 1 - cos_similarity` definition where `cos_similarity ∈ [-1, 1]`. The score conversion's `[0, 1]` codomain is exact for the standard formula; the `clamp` in code is defensive against floating-point edge cases, not against incorrect math.
- `EmbeddingClient` already validates its own returned vector's dimension at `src/embedding.rs:139-144` — confirmed by reading the source. The workplan's defense-in-depth check in `search_semantic` covers stub embedders that bypass `EmbeddingClient` (e.g. `StubEmbedder` and the per-test custom embedders in Task 7.2's tests).
- `bytemuck::cast_slice::<f32, u8>` for embedding blob — confirmed against `src/store/chunks.rs:55` (production usage from step 6) and the [`sqlite-vec-extension`](../../../.claude/skills/sqlite-vec-extension/SKILL.md) skill (line 72).
- `ApiState` shape and the `From<anyhow::Error> for ApiError` mapping — confirmed by reading `src/api/mod.rs:17-22` and `src/api/error.rs:46-77`.
- The existing `StubEmbedder` returns a fixed zero-filled vector — confirmed by reading `src/embedding.rs:182-197`.
- `tests/embedding.rs` already has a stub-service + live-daemon harness — confirmed by reading lines 1–295. Reuse is the right shape.
- Step-6 resolution C (slash-separated `heading_path`) and the orphan-H3 encoding (`"Setup//Prereqs"`) — confirmed against [`notes/roadmap/archive/step-06-workplan.md`](./step-06-workplan.md) lines 120–127.
- The kNN ordering syntax — `ORDER BY v.distance ASC, c.file_path ASC, c.chunk_index ASC` is standard SQL; the question of whether sqlite-vec's `distance` column ordering interacts with `MATCH` / `k` is the same kind of upstream-syntax dependency flagged in resolution F. The agent verifies at Task 7.2 time.

**Workplan-internal consistency**:

- The error envelope code `embedding_unavailable` appears consistently in resolution E, Task 7.2's classification helper, Task 7.3's error mapping, Task 7.5's integration tests, the spec amendment in Task 7.6, and the architecture-overview update in Task 7.6.
- The `hint: Option<String>` field appears in resolution B, Task 7.2's return shape, Task 7.3's `SemanticSearchResponse`, Task 7.4's CLI rendering, Task 7.5's integration tests, and the spec amendment in Task 7.6.
- Migration 0004's truncation of `chunks` and clearing of `files.content_hash` is named in resolution F, Task 7.1's migration text, Task 7.1's tests, and the DoD.
- Score conversion (`1 - distance/2` clamped) appears in resolution F, Task 7.2's SQL/Rust split (math in Rust, not SQL), and the spec amendment in Task 7.6.
- The `vault: Option<String>` forward-compat field appears in Task 7.3's `SemanticResultJson`, the v0-omits test in Task 7.3, and the cross-reference to step 5's resolution.

**Residual ambiguities flagged for the agent at task time**:
- Upstream sqlite-vec `distance_metric=cosine` clause syntax (Task 7.1 verifies; agent corrects + reports in results comment if it differs).
- Upstream sqlite-vec `MATCH` / `k` parameter-binding shape under cosine distance (Task 7.2 verifies; the CTE-vs-inline form is a free implementation choice, but the parameter binding for the vector blob must match what sqlite-vec accepts).
- Whether `ApiState` ergonomically takes `embedder: Arc<dyn Embedder>` and `embedding_dimension: u32` as separate fields, or one `Arc<EmbeddingContext>` struct — Task 7.3 implementation choice; the workplan does not pin it.

These are *narrow* escape hatches; the verification gates are in the tasks themselves, not deferred to soft-flag. If the agent's verification finds something the workplan didn't anticipate (e.g. sqlite-vec's cosine metric requires unit-normalized inputs at insert time), that's a `coordinator-only` soft flag worth surfacing at task close.

---

## Build-time amendments

The following items came up during the build (2026-04-26) and are recorded here for accuracy. They do **not** change the shipped behavior; the build matched the workplan's load-bearing intent in every case. The amendments are workplan-body corrections that the build surfaced, plus implementation choices the workplan deliberately did not pin.

1. **Empty-index hint reproduction recipe (workplan-prose slip — § Resolution B + § Task 7.5 case 2)** — The workplan describes the empty-index hint setup as "embedding service down so no chunks land." The "or" branch in resolution B and the corresponding Task 7.5 case 2 prose ("populate files, configure stub to fail (`StubMode::Err503`)") is **inaccurate**: when embedding fails during the initial scan or a watcher event, the indexer skips the file row entirely — `files` is *not* populated. The hint state therefore requires a different reproduction: index successfully (with stub up), then truncate `chunks_vec` and `chunks` while leaving `files` populated. The shipped Task 7.5 integration test (`semantic_search_returns_hint_when_index_empty_after_files_seeded`) uses this corrected recipe via post-index SQL truncation. The shipped Task 7.3 unit test (`semantic_handler_returns_hint_when_index_empty_and_files_present`) already used the equivalent shape (seed only `files`, skip chunks). Surfaced via Task 7.3's manual smoke; forwarded coordinator-mediated to Task 7.5; no spec amendment needed (the spec's `hint` semantics are unchanged).

2. **`client_search_semantic_round_trips` test setup (workplan-prose slip — § Task 7.4)** — The workplan literal says "Pre-populate chunks via `state.pool` directly. Assert response shape." The shipped test asserts the empty-DB wire-shape decode (`results.is_empty()` + `hint.is_none()`) matching the convention of the two adjacent client round-trip tests (`client_search_filesystem_round_trips`, `client_search_content_round_trips`). Pre-populating chunks would have required: (a) extending `TestDaemon` to expose `state.pool`, and (b) wiring a custom embedder (the default `StubEmbedder` returns zero vectors, which makes cosine distance degenerate). The workplan's intent — verify that the client wire-shape decodes correctly — is met; semantic-specific scoring/hint coverage already lives at the handler layer (7 tests in `src/api/tests.rs`).

3. **`set_mode` signature (workplan-prose nit — § Task 7.5 anti-flake rules)** — The workplan says `set_mode` "takes a `&mut self`." The shipped helper takes `&self` with internal `Arc<Mutex<StubMode>>` because `Mutex` provides interior mutability — `&mut self` would mislead about thread-safety and prevent calling from `&Arc<StubServer>` references the test holds. The "no concurrent writes" requirement (workplan rationale) is preserved as a test-discipline contract documented in `set_mode`'s doc-comment. Behavior unchanged.

4. **Implementation choices the workplan deliberately did not pin** — Recorded so future agents/coordinators reading this file can refer back without re-deriving: score format `{:.2}` (two decimals, matching the workplan's `0.82` example); heading-path separator ` / ` (space-slash-space); orphan-segment handling `iter().filter(|s| !s.is_empty())` which also drops fully-empty heading paths (no `> ` line at all); 240-char body trim deferred (workplan-acknowledged "not load-bearing"); CLI does not expose `--min-similarity` (daemon clamps `None → 0.0`); `render_semantic_text` returns `String` instead of printing directly (small deviation from the existing `render_filesystem_text` / `render_content_text` shape, scoped to keep rendering testable); kNN `OneShotEmbedder` test stub uses `Mutex<Option<Result>>` for single-call injection across error-classification tests; `DeterministicHashEmbedder` (FNV-1a per-slot, normalized) is the integration-test substitute for the all-zero `StubEmbedder` when ordering matters.

5. **Spec status convention** (Task 7.6 verification) — All four prior shipped specs (`change-events`, `content-search`, `filesystem-search`, `vault-management`) keep `Status: Draft` + `Version: 0.1.0` + original `Date: 2026-04-23` after their shipping step. Task 7.6 followed convention and left `semantic-search.md` unchanged on those fields (rather than the workplan's "flip to Stable" suggestion, which was a free latitude clause). The `_template.md` placeholder doesn't constrain the convention; the four shipped specs do.

6. **No surgical follow-up tasks (Task M.Nr1 shape from step 6)** — None of the three coordinator-only soft flags this step demonstrated a real bug warranting in-build act-now intervention. The pattern stays available for steps 8+ if a smoke or integration test surfaces a real cross-task bug.
