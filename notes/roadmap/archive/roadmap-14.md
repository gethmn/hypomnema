# Round 14 — Semantic Document Results

**Status**: Shipped 2026-05-08  
**Date**: 2026-05-08  
**Steps**: 25  
**Scope**: Single-step round making semantic search return document-grouped results by default, while preserving explicit chunk-level search for passage retrieval.

**Intakes**:
- [`notes/proposals/semantic-search-document-results-intake.md`](../proposals/semantic-search-document-results-intake.md) — Complete

---

## Overview

Round 14 implements the document-result semantic search shape proposed in `notes/proposals/semantic-search-document-results.md`. The vector index remains chunk-based: Hypomnema still embeds and searches chunks, then document mode groups a deeper candidate set by parent document and returns bounded evidence chunks for each document.

This round intentionally flips the default semantic result shape immediately. The maintainer resolved the rollout policy on 2026-05-08: there is no compatibility-window step; `granularity: "document"` is the built-in/default behavior and explicit `granularity: "chunk"` remains available for callers that need the current flat passage result shape.

No embedding, chunking, vector-table, or registry schema change is required. No new dependencies are expected.

Human-resolved decisions from intake:
- **Single-step round**: capability, default flip, compatibility path, and docs/tests ship together.
- **Default granularity**: document mode defaults immediately.
- **Compatibility path**: explicit `granularity: "chunk"` preserves flat chunk behavior.
- **Config scope**: daemon-global defaults only; no per-vault policy or registry schema work.

---

## Step 25 — Semantic Document Results

### Objective

Implement document-grouped semantic results as the default semantic-search shape, with top evidence chunks per document, while preserving the existing flat chunk-result behavior through explicit chunk granularity.

### Shipping Criteria

All items below must be complete and passing before the step is marked shipped:

- [ ] `granularity: "document" | "chunk"` request support exists for HTTP, CLI, and MCP semantic search
- [ ] Built-in daemon default is document granularity; daemon config can set `search.semantic.default_granularity = "chunk"` for compatibility
- [ ] Document-mode response returns grouped document results with `score = max(chunk score)` and bounded `chunks` evidence
- [ ] `chunks_per_document` request/default handling is implemented and validated
- [ ] Document-mode candidate retrieval depth uses config-only `document_candidate_multiplier` and `document_candidate_limit`
- [ ] Chunk mode preserves the current flat result shape and ranking behavior
- [ ] Cross-vault merge remains deterministic and includes `vault` / `vault_name` on results
- [ ] Partial-results and hint behavior remain compatible with the current semantic-search contract
- [ ] CLI help/output and MCP tool schema/descriptions explain document intent vs passage intent
- [ ] `docs/specs/semantic-search.md`, `docs/reference/configuration.md`, and `docs/reference/cli.md` reflect granularity-dependent limit and response semantics
- [ ] Tests cover dominance regression, scoring, evidence ordering/caps, text payload modes, config/request precedence, invalid request/config cases, multi-vault fields, and partial results
- [ ] `cargo test` and `cargo clippy -- -D warnings` clean

---

## Out of Scope

- Per-vault semantic search policy
- Registry schema changes for per-vault search defaults
- Alternative document scoring formulas beyond max chunk score
- Reranking, HyDE/query expansion, or hybrid lexical + semantic fusion
- Pagination/cursors for deep semantic browsing
- Chunking, embedding-model, or vector-storage changes
- Full-file retrieval beyond existing `content_get`
- Splitting semantic search into separate document and chunk tools
- Fixing the existing kNN-first prefix-filtering recall limitation

---

## Workplan-Time Decisions

These are resolved in `notes/roadmap/step-25-workplan.md` before build starts:

1. Public result shape signaling: untagged result enum, with effective `granularity` defining which variant appears.
2. Chunk-mode `chunks_per_document`: accepted but ignored.
3. `truncated` semantics: keep the existing boolean; set it for final-limit truncation or candidate-cap truncation.
4. Extra document fields: do not add `matched_chunk_count` or `best_chunk_index` in v1.
5. Prefix filtering: keep the current SQL strategy; document the known limitation.

---

## Related References

- **Proposal**: `notes/proposals/semantic-search-document-results.md`
- **Intake**: `notes/proposals/semantic-search-document-results-intake.md`
- **Current semantic spec**: `docs/specs/semantic-search.md`
- **Config reference**: `docs/reference/configuration.md`
- **CLI reference**: `docs/reference/cli.md`
- **Content retrieval boundary**: `docs/specs/content-retrieval.md`
- **Chunk semantic core**: `src/search/semantic.rs`
- **HTTP aggregation and payload shaping**: `src/api/search.rs`
- **HTTP/MCP schemas**: `src/api/types.rs`
- **CLI semantic rendering**: `src/bin/hmn.rs`
- **MCP backend/server surfaces**: `src/mcp/backend.rs`, `src/mcp/server.rs`

---

## Build Strategy (post-approval)

**Phase 1 — Workplan production** (coordinator/researcher-driven)
- Researcher drafts/reviews `notes/roadmap/step-25-workplan.md` from the proposal and intake
- Coordinator reviews the roadmap and workplan, then surfaces them for human go/no-go

**Phase 2 — Build orchestration** (coordinator-driven, if approved)
- 5 tasks with dependency structure: contract/config first, semantic core second, API aggregation third, CLI/MCP/docs fourth, and focused verification fifth
- Keep the persistent researcher available for contract or scoring questions during build
