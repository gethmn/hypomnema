# Semantic Search Document Results Proposal

**Status**: Draft
**Date**: 2026-05-08

---

## Summary

Semantic search currently returns the top matching chunks directly. That is precise for passage-level evidence, but it can make the default result set less useful when one document contains many high-scoring chunks. With the default `limit: 10`, a single strong document can consume the whole response and hide other relevant documents.

This proposal adds document-grouped semantic results while keeping raw chunk search available. Hypomnema should continue searching embedded chunks, because small chunks are the right unit for vector similarity, but the user-facing result can be grouped by parent document when the caller wants "the most relevant notes" rather than "the most relevant passages."

The proposed default is document granularity, with a bounded number of evidence chunks per document. Existing chunk-oriented behavior remains available through an explicit request field.

---

## Motivation

The current semantic-search interface answers: "Which indexed chunks are closest to this query?" That is useful for quote finding and small retrieval contexts. It is less useful for exploratory work where the caller wants a diverse set of files to inspect.

Example failure mode:

1. `notes/design/search.md` has 15 chunks highly related to the query.
2. The caller sends semantic search with default `limit: 10`.
3. The vector query returns the 10 best chunks.
4. All 10 results may come from `notes/design/search.md`.
5. Other relevant notes are hidden unless the caller increases `limit`, deduplicates client-side, or runs follow-up searches.

This is not a bug in vector search; it is a mismatch between the retrieval unit and the result unit. Chunk search is still the right low-level operation. The missing feature is a document-level result shape built from chunk evidence.

---

## Current Baseline

The implemented semantic path is chunk-first:

- `src/search/semantic.rs` embeds the query, asks `chunks_vec` for nearest neighbors with `k = limit`, joins those rows to `chunks`, converts vector distance to score, and returns `SemanticResult` chunk records.
- `src/api/search.rs` merges per-vault chunk results by score and globally truncates to `limit`.
- `docs/specs/semantic-search.md` describes semantic results as chunks and defines `limit` as a global result cap.
- Existing payload-budget controls (`include_text`, `preview_bytes`, `text_kind`, `text_truncated`) shape returned chunk text, not the result granularity.

The current behavior is therefore:

```text
query -> query embedding -> top K chunk vectors -> flat chunk results
```

The proposed behavior for document mode is:

```text
query -> query embedding -> deeper chunk candidate set -> group by file -> document results with evidence chunks
```

No indexing, embedding, or vector-storage schema change is required.

---

## Problem Statement

Hypomnema needs semantic search to support two related but distinct retrieval intents:

- **Passage intent**: "Show me the best matching chunks." This is the current behavior and should remain available.
- **Document intent**: "Show me the best matching documents/notes, with enough chunk evidence to explain why." This is not currently available server-side.

Client-side deduplication is possible but not sufficient as the default strategy:

- MCP clients discover the tool contract from schemas and descriptions; expecting each caller to rediscover grouping semantics creates drift.
- The daemon already owns cross-vault merge, truncation, and payload budgeting, so document grouping belongs near that logic.
- Server-side grouping can apply consistent scoring, stable tie-breaks, and bounded evidence payloads across HTTP, CLI, and MCP.

---

## Design Goals

- Preserve raw chunk search for passage-level workflows.
- Make document-level semantic search the default result shape.
- Keep one semantic search operation rather than adding separate document/chunk tools.
- Keep request-time behavior configurable through daemon-level defaults, with per-request overrides.
- Avoid changing chunking, embedding generation, vector schema, or index rebuild requirements.
- Keep document scoring simple and explainable for the first version.
- Leave room for per-vault policy later without adding registry schema work now.

---

## Proposed Direction

Extend semantic search with a `granularity` request field:

```yaml
granularity: "document"  # document | chunk
```

Behavior:

- `granularity: "document"` returns grouped document results.
- `granularity: "chunk"` returns the current flat chunk results.
- If omitted, the daemon uses `[search.semantic].default_granularity`, which defaults to `"document"`.

Document mode groups candidate chunks by `(vault_id, file_path, content_hash)`. Each document result gets:

- `score`: the highest chunk score in that document.
- source identity: `file_path`, `content_hash`, `vault`, `vault_name`.
- `chunks`: top matching evidence chunks, capped by `chunks_per_document`.

For v1, document score is **max chunk score**. This is intentionally simple: it says the document ranks according to its best evidence passage. Other formulas can be added later if usage shows max scoring is too spiky.

---

## Request and Response Shape

### Request

```yaml
query: "semantic search ranking problems"
granularity: "document"          # document | chunk; default from config
limit: 10                        # documents in document mode, chunks in chunk mode
chunks_per_document: 3           # document mode only; default from config
include_text: "preview"          # existing preview | full | none
preview_bytes: 600               # existing behavior
prefix: "notes/"                 # existing
min_similarity: 0.3              # existing
vaults:
  - "personal"
```

New request fields:

| Field | Type | Default | Description |
|---|---|---|---|
| `granularity` | `"document"` \| `"chunk"` | daemon config, default `"document"` | Result grouping mode. |
| `chunks_per_document` | integer | daemon config, default `3` | Maximum evidence chunks returned inside each document result. Ignored in chunk mode. |

Existing fields keep their current meanings except `limit`, whose result unit depends on granularity:

- Document mode: `limit` means maximum returned documents.
- Chunk mode: `limit` means maximum returned chunks, as today.

### Document Response

```yaml
results:
  - score: 0.91
    file_path: "notes/design/search.md"
    content_hash: "sha256:abc123..."
    vault: "01951f6c-7c3b-7a2e-8c1d-1a2b3c4d5e6f"
    vault_name: "personal"
    chunks:
      - score: 0.91
        chunk_index: 5
        heading_path: ["Ranking", "Document grouping"]
        text: "..."
        text_kind: "preview"
        text_truncated: true
      - score: 0.86
        chunk_index: 6
        heading_path: ["Ranking", "Evidence"]
        text: "..."
        text_kind: "preview"
        text_truncated: false
truncated: false
```

Chunk mode should preserve the existing flat `SemanticResultJson` shape:

```yaml
results:
  - score: 0.91
    file_path: "notes/design/search.md"
    chunk_index: 5
    heading_path: ["Ranking", "Document grouping"]
    text: "..."
    text_kind: "preview"
    text_truncated: true
    content_hash: "sha256:abc123..."
    vault: "01951f6c-7c3b-7a2e-8c1d-1a2b3c4d5e6f"
    vault_name: "personal"
truncated: false
```

The implementation can model this as an enum/untagged response item or as separate internal result types, but the public schema must make the two shapes clear to HTTP and MCP callers.

---

## Configurable Defaults

Add daemon-level search defaults:

```toml
[search.semantic]
default_granularity = "document"
default_chunks_per_document = 3
document_candidate_multiplier = 10
document_candidate_limit = 1000
```

Semantics:

- Config supplies defaults for omitted request fields.
- Request fields override config where exposed.
- `document_candidate_multiplier` and `document_candidate_limit` are internal retrieval-depth tuning knobs and should be config-only for now.

Suggested validation:

| Setting | Valid range |
|---|---|
| `default_granularity` | `"document"` or `"chunk"` |
| `default_chunks_per_document` | `1..=100` |
| `document_candidate_multiplier` | `1..=100` |
| `document_candidate_limit` | `1..=10000` |

Invalid config should fail `hmnd config-validate` and daemon startup with a clear configuration error.

### Why Global Config Now

Global semantic defaults fit the current configuration model. The daemon already has TOML config for process-wide behavior such as embedding service, watcher settings, MCP transport, storage, and logging. Result-shaping defaults belong in the same operational layer.

### Why Per-Vault Config Later

Per-vault search policy is plausible, but it should not be implemented as static TOML. Hypomnema vaults are runtime state managed through `vaults.sqlite` and the control plane, per ADR-0010. Per-vault overrides would require a registry schema change plus lifecycle/API design for reading and updating policy.

Future per-vault policy should follow this precedence:

```text
request field -> per-vault policy -> daemon search config -> built-in default
```

This proposal only implements:

```text
request field -> daemon search config -> built-in default
```

---

## Document Scoring

Recommended first scoring rule:

```text
document_score = max(score of matched candidate chunks in the document)
```

Rationale:

- Easy to explain to users and agents.
- Does not reward long documents merely for having more chunks.
- Preserves the existing cosine-similarity score interpretation.
- Does not require new statistics or reindexing.

Known limitation:

- A document with one excellent chunk may outrank a document with several very good chunks.

Deferred alternatives:

- Top-N average: average the best few chunk scores.
- Weighted evidence: best score as the main signal, with modest boost from additional chunks.
- Saturating sum: reward broad evidence while limiting long-document advantage.
- Dual fields: expose `score` as max plus `match_count` or `supporting_score`.

Those can be introduced later if max-score document ranking proves too narrow.

---

## Candidate Retrieval Depth

Document mode must search deeper than the requested document count. If the caller asks for 10 documents and one file has the top 15 chunks, fetching only 10 chunks cannot produce 10 documents.

Use config-controlled internal candidate depth:

```text
candidate_limit = min(limit * document_candidate_multiplier, document_candidate_limit)
```

With defaults:

```text
candidate_limit = min(limit * 10, 1000)
```

Per-vault behavior should mirror current cross-vault semantics:

- Each active vault contributes up to `candidate_limit` chunk candidates in document mode.
- The API groups and merges candidates globally.
- Final document results are truncated to request `limit`.

This is not a recall guarantee. It is a bounded, practical default that prevents one document from dominating ordinary result sets without introducing iterative pagination or unbounded vector scans.

---

## Chunk Evidence Per Document

Document results should include bounded chunk evidence because agents need to know why a document matched.

Default:

```text
chunks_per_document = 3
```

Ordering inside `chunks`:

1. chunk score descending.
2. `chunk_index` ascending as a stable tie-break.

Payload controls apply to nested evidence chunks exactly as they apply to flat chunk results:

- `include_text: "preview"` returns bounded text with `text_kind` and `text_truncated`.
- `include_text: "full"` returns full stored chunk text.
- `include_text: "none"` omits text fields and returns metadata only.

This keeps document mode compatible with the search-result payload budget work.

---

## Compatibility and Migration

### Behavior Compatibility

This proposal intentionally changes the default semantic result shape from chunk results to document results. That is a breaking/default-behavior change for callers that omit `granularity` and expect flat chunk rows.

Mitigations:

- `granularity: "chunk"` preserves current behavior.
- Operators can set:

```toml
[search.semantic]
default_granularity = "chunk"
```

to preserve legacy defaults daemon-wide during migration.

### API Compatibility

The response schema must document that semantic results are shape-dependent. MCP tool descriptions should explicitly tell agents:

- use document granularity for "which notes are relevant?"
- use chunk granularity for "which passages are relevant?"

### CLI Compatibility

The CLI should expose `--granularity chunk|document` and `--chunks-per-document N`, but the defaults should come from daemon config. The CLI should not grow flags for internal candidate depth.

### No Data Migration

No on-disk index migration is required. Existing `chunks` and `chunks_vec` tables contain enough metadata to group by document.

---

## Deferred / Out of Scope

- Per-vault semantic search policy.
- Registry schema changes for per-vault search settings.
- Alternative document scoring formulas.
- Reranking with a cross-encoder.
- HyDE/query expansion.
- Hybrid lexical + semantic fusion.
- Pagination or cursors for deep semantic result browsing.
- Changes to chunking strategy.
- Changes to embedding model or vector storage.
- Full-file/chunk retrieval beyond existing search response payload controls.
- Splitting semantic search into separate MCP tools for chunks and documents.

---

## Risks and Tradeoffs

### Default Shape Change

Defaulting to document granularity is more useful for exploratory search, but it changes the shape of `results` for callers that omit `granularity`.

Mitigation: support `default_granularity = "chunk"` in daemon config and document `granularity: "chunk"` as the compatibility path.

### Candidate Depth Can Still Under-Fill

If one document dominates the top 1000 chunks and `document_candidate_limit = 1000`, document mode may return fewer than `limit` documents.

Mitigation: mark `truncated: true` when candidate retrieval hits the cap, document the behavior, and leave iterative fill/pagination for future work.

### Score Semantics Are Approximate

Max chunk score is an explainable document score, but it does not measure whole-document relevance.

Mitigation: keep the scoring rule explicit in docs and leave room for later formulas.

### Prefix Filtering Behavior

Current semantic SQL materializes kNN candidates before applying the prefix filter. Document mode's deeper candidate set may reduce under-returning, but it does not fully solve scoped search recall if many nearest candidates are outside the prefix.

Mitigation: keep this as a known semantic-search limitation unless a workplan chooses to address prefix filtering in the SQL/query strategy.

---

## Validation and Test Strategy

Required tests:

- Regression: one document with 15 high-scoring chunks no longer consumes all default document-mode results.
- Chunk mode still returns flat raw chunks and can be dominated by one document.
- Document mode `limit` caps documents, not chunks.
- Document score equals the best nested chunk score.
- Nested chunks are ordered by score and capped by `chunks_per_document`.
- `include_text: "preview"` works for nested chunks and preserves UTF-8 preview behavior.
- `include_text: "full"` returns full nested chunk text.
- `include_text: "none"` omits nested text fields.
- Request `granularity` overrides config `default_granularity`.
- Request `chunks_per_document` overrides config `default_chunks_per_document`.
- Omitted request fields use daemon config defaults.
- Invalid request `granularity` returns `invalid_request`.
- Invalid `chunks_per_document = 0` returns `invalid_request`.
- Invalid config values fail config validation/startup.
- Multi-vault document grouping includes `vault` and `vault_name`.
- Cross-vault partial failures/skips still produce `partial_results`.

Manual testing should include:

- A query where multiple chunks from one note match strongly.
- A document-mode query showing diverse documents.
- A chunk-mode query showing raw passages.
- A config-default flip to `default_granularity = "chunk"` for compatibility.

---

## Open Questions

- Should the public response use a tagged result enum, or rely on the request's `granularity` to define the result shape?
- Should `chunks_per_document` appear in chunk-mode requests as ignored, or should the API reject it when `granularity: "chunk"`?
- Should `truncated` distinguish between final document truncation and internal candidate-depth truncation, or is the existing boolean enough?
- Should document results expose additional fields such as `matched_chunk_count` or `best_chunk_index` in v1?
- Should prefix filtering be revisited in the same implementation step, given that the current kNN-first SQL can under-return scoped results?

None of these block the proposal. They should be resolved during proposal intake or the step workplan.

---

## Related Documents

- [`docs/specs/semantic-search.md`](../../docs/specs/semantic-search.md)
- [`docs/specs/content-retrieval.md`](../../docs/specs/content-retrieval.md)
- [`docs/reference/configuration.md`](../../docs/reference/configuration.md)
- [`docs/reference/cli.md`](../../docs/reference/cli.md)
- [`docs/decisions/0004-three-search-modes-as-peers.md`](../../docs/decisions/0004-three-search-modes-as-peers.md)
- [`docs/decisions/0009-multi-vault-per-daemon.md`](../../docs/decisions/0009-multi-vault-per-daemon.md)
- [`docs/decisions/0010-vault-definitions-as-runtime-state.md`](../../docs/decisions/0010-vault-definitions-as-runtime-state.md)
- [`notes/proposals/hyde-semantic-search.md`](./hyde-semantic-search.md)
- [`notes/proposals/archive/search-result-payload-budget.md`](./archive/search-result-payload-budget.md)
- [`notes/project-planning-workflow-notes.md`](../project-planning-workflow-notes.md)
