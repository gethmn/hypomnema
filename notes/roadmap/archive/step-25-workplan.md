# Step 25 Workplan — Semantic Document Results

**Status**: Shipped 2026-05-08  
**Date**: 2026-05-08  
**Round**: 14  
**Intake**: `notes/proposals/semantic-search-document-results-intake.md`

---

## Workplan-Time Decision Resolutions

Five proposal/intake questions are resolved here so builders can implement without reopening product scope:

1. **Public response shape signaling**: use an untagged result enum, with the request/effective `granularity` defining which variant appears.
   - Rationale: matches the proposal examples and the existing `ContentGetResultItem` precedent, while still allowing the generated schema to describe both shapes.

2. **`chunks_per_document` in chunk mode**: accept but ignore it.
   - Rationale: the proposal marks it document-mode only / ignored in chunk mode; rejecting it would make harmless clients brittle.

3. **`truncated` semantics**: keep the existing boolean.
   - Rationale: no new response field is needed for v1. Set `truncated: true` when final merged results exceed `limit`, when any per-vault document candidate query reaches its candidate cap, or when chunk mode hits the existing per-vault cap.

4. **Extra document explainability fields**: do not add `matched_chunk_count` or `best_chunk_index` in v1.
   - Rationale: `score`, document identity, and ordered evidence chunks are enough for the first contract. Additional fields can be proposed after usage feedback.

5. **Prefix filtering SQL strategy**: do not fix it in this step.
   - Rationale: the current kNN-first query can under-return scoped results, but the proposal treats that as a known limitation. Candidate-depth tuning helps ordinary document-mode diversity without expanding scope into SQL strategy work.

---

## Task Structure

Five tasks with dependency chain: **Task 1 -> Task 2 -> Task 3 -> Tasks 4 & 5**.

### Task 1: Contract and Config Defaults

**Goal**: Add the public request/response contract and daemon-global semantic defaults.

**Shipping Criteria**:
- [ ] `SemanticQueryJson` accepts optional `granularity` and `chunks_per_document`
- [ ] Result schema supports both flat chunk results and grouped document results
- [ ] Config includes `[search.semantic]` with built-in defaults:
  - `default_granularity = "document"`
  - `default_chunks_per_document = 3`
  - `document_candidate_multiplier = 10`
  - `document_candidate_limit = 1000`
- [ ] Config validation rejects invalid values:
  - `default_granularity` not in `document|chunk`
  - `default_chunks_per_document` outside `1..=100`
  - `document_candidate_multiplier` outside `1..=100`
  - `document_candidate_limit` outside `1..=10000`
- [ ] Request validation rejects invalid `granularity` and `chunks_per_document` outside `1..=100`
- [ ] Effective defaults follow precedence: request field -> daemon config -> built-in default
- [ ] No per-vault config, registry schema, or new dependency is introduced

**Files**:
- `src/config.rs`
- `src/api/types.rs`
- `src/api/search.rs`
- `tests/config.rs`
- `src/api/tests.rs`

**Risk**: medium-low. The main risk is schema churn across HTTP/MCP serialization; keep the type change narrow.

---

### Task 2: Semantic Core Candidate Retrieval

**Goal**: Keep chunk kNN as the low-level operation, but allow document mode to fetch a deeper candidate set and preserve enough metadata for grouping.

**Shipping Criteria**:
- [ ] `SemanticQuery` can express effective candidate limit separately from final result limit
- [ ] Chunk mode uses existing `limit` behavior and flat `SemanticResult` rows
- [ ] Document mode computes per-vault `candidate_limit = min(limit * document_candidate_multiplier, document_candidate_limit)`
- [ ] Candidate rows remain ordered by score descending, with stable tie-breaks
- [ ] `min_similarity`, `prefix`, embedding error classification, hint logic, and spawn_blocking SQLite boundary remain intact
- [ ] Existing chunk-mode unit tests still pass with minimal expectation changes
- [ ] New unit tests cover candidate limit, min-similarity filtering, and deterministic ordering

**Files**:
- `src/search/semantic.rs`
- `src/search/mod.rs`

**Risk**: medium. This is the load-bearing search path; every SQLite query must remain inside `tokio::task::spawn_blocking`.

---

### Task 3: API Document Grouping and Cross-Vault Merge

**Goal**: Build document results from chunk candidates and merge them correctly across vaults.

**Shipping Criteria**:
- [ ] Document grouping key is `(vault_id, file_path, content_hash)`
- [ ] Document result `score` equals the highest candidate chunk score in that document
- [ ] Nested `chunks` evidence is sorted by score descending, then `chunk_index` ascending
- [ ] Nested evidence count is capped by effective `chunks_per_document`
- [ ] Document-mode `limit` caps documents, not chunks
- [ ] Chunk-mode `limit` continues to cap flat chunks
- [ ] `include_text: preview|full|none`, `preview_bytes`, `text_kind`, and `text_truncated` apply identically to nested evidence chunks
- [ ] Cross-vault document merge sorts by score descending, then vault id, then file path for deterministic ties
- [ ] `partial_results` behavior remains consistent with current semantic search
- [ ] Existing hint logic is preserved: emit only when in-scope `chunks_vec` rows are empty and in-scope `files` rows are non-empty; do not emit when chunks exist but the query has no matches

**Files**:
- `src/api/search.rs`
- `src/api/types.rs`
- `src/api/tests.rs`
- `tests/embedding.rs`
- `tests/semantic_smoke.rs`
- `tests/multi_vault_internal.rs`

**Risk**: medium. The grouping is straightforward, but truncation and cross-vault merge semantics need precise tests.

---

### Task 4: CLI, MCP, and Documentation Surface

**Goal**: Expose the new semantic controls and document the intent split clearly.

**Shipping Criteria**:
- [ ] `hmn search semantic` supports `--granularity document|chunk`
- [ ] `hmn search semantic` supports `--chunks-per-document N`
- [ ] CLI text rendering handles document results with nested evidence and preserves flat chunk rendering for chunk mode
- [ ] `--json` emits the daemon response unchanged
- [ ] MCP `search_semantic` schema includes `granularity` and `chunks_per_document`
- [ ] MCP descriptions explain:
  - document granularity for "which notes are relevant?"
  - chunk granularity for "which passages are relevant?"
- [ ] `docs/specs/semantic-search.md` describes request fields, response variants, scoring, candidate depth, truncation, and validation
- [ ] `docs/reference/configuration.md` documents `[search.semantic]`
- [ ] `docs/reference/cli.md` documents new flags and text output behavior

**Files**:
- `src/bin/hmn.rs`
- `src/client.rs`
- `src/mcp/backend.rs`
- `src/mcp/server.rs`
- `docs/specs/semantic-search.md`
- `docs/reference/configuration.md`
- `docs/reference/cli.md`
- `tests/cli.rs`
- `tests/mcp.rs`
- `tests/mcp_http.rs`

**Risk**: medium-low. Most work is surface plumbing and docs, but MCP schema clarity matters for agent callers.

---

### Task 5: Verification and Regression Coverage

**Goal**: Prove the default result shape changed intentionally without regressing chunk-mode behavior.

**Shipping Criteria**:
- [ ] Regression: one document with many high-scoring chunks no longer consumes all default document-mode results
- [ ] Chunk mode can still return multiple flat chunks from the same document
- [ ] Document score equals best nested chunk score
- [ ] Nested chunks are ordered and capped correctly
- [ ] `include_text: "preview"` preserves UTF-8 preview behavior inside nested chunks
- [ ] `include_text: "full"` returns full nested chunk text
- [ ] `include_text: "none"` omits nested text fields
- [ ] Request `granularity` overrides config `default_granularity`
- [ ] Request `chunks_per_document` overrides config `default_chunks_per_document`
- [ ] Omitted fields use daemon config defaults
- [ ] Invalid request/config cases return the documented errors
- [ ] Multi-vault document results include `vault` and `vault_name`
- [ ] Partial vault failures/skips still produce `partial_results`
- [ ] Document mode sets `truncated: true` when a per-vault candidate query reaches `document_candidate_limit`, even if final document count is `<= limit`
- [ ] Manual smoke covers document-mode diversity, explicit chunk mode, and config default flip
- [ ] `cargo test` and `cargo clippy -- -D warnings` pass

**Files**:
- `src/api/tests.rs`
- `src/search/semantic.rs`
- `tests/embedding.rs`
- `tests/semantic_smoke.rs`
- `tests/cli.rs`
- `tests/mcp.rs`
- `tests/mcp_http.rs`
- `tests/config.rs`

**Risk**: low. This is mostly acceptance coverage and smoke verification after the implementation tasks land.

---

## Batching Plan

| Batch | Tasks | Sequencing | Rationale |
|---|---|---|---|
| 1 | Task 1 | Start immediately | Establishes the effective request/config contract used by all later work |
| 2 | Task 2 | After Task 1 | Adds candidate-depth mechanics while preserving chunk retrieval |
| 3 | Task 3 | After Task 2 | Builds document grouping and cross-vault result semantics on candidate rows |
| 4 | Tasks 4, 5 | Parallel after Task 3 | Surface/docs and verification can proceed independently once response shape is stable |

---

## Shipping Criteria Verification

At step boundary, verify all criteria from `notes/roadmap/roadmap-14.md` § Step 25:

- [ ] HTTP, CLI, and MCP accept `granularity`
- [ ] Document granularity is default without request fields
- [ ] Explicit `granularity: "chunk"` preserves flat chunk results
- [ ] Config default flip to chunk works
- [ ] Document results group by document with bounded evidence chunks
- [ ] Candidate-depth config controls document-mode retrieval depth
- [ ] Cross-vault ordering and `partial_results` remain deterministic
- [ ] Semantic-search docs/config/CLI reference updated
- [ ] Required behavioral tests pass
- [ ] `cargo test` and `cargo clippy -- -D warnings` clean

---

## Needs-Human Blockers

None for planning. The maintainer already resolved the only rollout blocker on 2026-05-08: ship a single-step default flip to document granularity while keeping explicit chunk mode.

---

## Related References

- **Roadmap**: `notes/roadmap/roadmap-14.md`
- **Proposal**: `notes/proposals/semantic-search-document-results.md`
- **Intake**: `notes/proposals/semantic-search-document-results-intake.md`
- **Current semantic spec**: `docs/specs/semantic-search.md`
- **Config reference**: `docs/reference/configuration.md`
- **CLI reference**: `docs/reference/cli.md`
- **Content retrieval boundary**: `docs/specs/content-retrieval.md`
- **Core semantic search**: `src/search/semantic.rs`
- **API aggregation**: `src/api/search.rs`
- **Schema types**: `src/api/types.rs`
- **CLI renderer**: `src/bin/hmn.rs`
