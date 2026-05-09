# Proposal Intake: Semantic Search Document Results

**Status**: Intake complete
**Date**: 2026-05-08
**Intake inputs**:

- `notes/proposals/semantic-search-document-results.md` — Primary proposal (Status: Draft, 2026-05-08)
- `docs/specs/semantic-search.md` — Current semantic search contract and cross-vault behavior
- `docs/reference/configuration.md` — Current daemon config model
- `docs/specs/content-retrieval.md` — Follow-on retrieval boundary and source-of-truth precedent

---

## Summary

The proposal changes semantic-search result shaping from flat chunk rows to document-grouped results by default, while preserving chunk-level retrieval via an explicit `granularity` request field. The core implementation keeps chunk-level vector search intact, then groups candidate chunks by document and returns bounded evidence chunks per document. It also introduces daemon-level defaults for granularity and evidence limits, plus internal candidate-depth tuning to reduce single-document domination at low limits.

## Source Inputs

| Source | Type | Role in intake |
|---|---|---|
| `notes/proposals/semantic-search-document-results.md` | proposal | primary |
| `docs/specs/semantic-search.md` | spec | supporting — current chunk-result contract, limit/truncation semantics, and multi-vault merge behavior |
| `docs/reference/configuration.md` | reference | supporting — daemon-level TOML configuration precedent and validation expectations |
| `docs/specs/content-retrieval.md` | spec | background — reinforces the boundary between search-result previews and full-file retrieval |

## Candidate Outcomes

- Outcome:
  - Source: Proposal § Summary, § Proposed Direction
  - User-visible result: Semantic search can return document-grouped results with top evidence chunks, while `granularity: "chunk"` preserves flat chunk output.
  - Verification signal: Request/response behavior matches granularity, with expected scoring, limits, and nested evidence semantics.
- Outcome:
  - Source: Proposal § Configurable Defaults, § Compatibility and Migration
  - User-visible result: Operators can configure daemon defaults (`default_granularity`, `default_chunks_per_document`), with document mode as the built-in default and explicit chunk mode available when needed.
  - Verification signal: Config validation/startup checks and default/override tests pass.

## Proposed Roadmap Shape

Shape recommendation: **single-step feature**.

Rationale: the project has very few users, mostly the maintainer, so the compatibility-window cost is not worth a separate roadmap step. Implement document-grouped semantic results as the default immediately, while retaining explicit `granularity: "chunk"` for passage-level search and for callers that need the old flat result shape.

### Step N — Semantic Document Results

**Goal**:
Implement document-grouped semantic results as the default semantic-search shape, while preserving explicit chunk granularity for passage search.

**Shipping criteria**:

- [ ] `granularity: document|chunk` request support exists for HTTP/CLI/MCP semantic search.
- [ ] Daemon built-in default is `default_granularity = "document"`.
- [ ] Document-mode response shape ships with grouped docs, `score = max(chunk score)`, and bounded `chunks` evidence.
- [ ] `chunks_per_document` request/default handling is implemented with validation.
- [ ] Candidate-depth retrieval logic is implemented (`multiplier`, `limit`) and documented as internal tuning.
- [ ] Chunk mode preserves existing flat shape and behavior.
- [ ] Compatibility path is explicit and tested (`default_granularity = "chunk"` and request override), but there is no separate compatibility-window rollout.
- [ ] CLI help/docs and MCP tool descriptions clearly explain intent split (document intent vs passage intent).
- [ ] Specs/reference docs reflect granularity-dependent limit semantics and response shapes.
- [ ] Tests cover proposal-required behavioral cases (dominance regression, scoring, chunk ordering/caps, payload text modes, override precedence, invalid request/config cases, multi-vault fields, partial results).

**Deferred decisions resolved in this step**:

- Decision: Public shape signaling approach (tagged enum vs granularity-implied)
  - Source: Proposal § Open Questions
  - Why this step: API/schema choice is required before implementation and tests can stabilize.
- Decision: Chunk-mode handling of `chunks_per_document` (ignore vs reject)
  - Source: Proposal § Open Questions
  - Why this step: Validation behavior must be pinned for compatibility and docs.
- Decision: `truncated` semantics for candidate-cap vs final-cap
  - Source: Proposal § Risks and Tradeoffs, § Open Questions
  - Why this step: Needed for correct operator expectations and test assertions.
- Decision: Default-flip rollout policy
  - Source: Maintainer review, 2026-05-08
  - Why this step: Resolved in favor of immediate document default because the user base is small; no separate compatibility step.

**New deps**:

- (none)

**Risk**: medium

**Source coverage**:

- `notes/proposals/semantic-search-document-results.md`: Summary, Motivation, Problem Statement, Proposed Direction, Request and Response Shape, Configurable Defaults, Document Scoring, Candidate Retrieval Depth, Chunk Evidence Per Document, Validation and Test Strategy
- `notes/proposals/semantic-search-document-results.md`: Compatibility and Migration, API Compatibility, CLI Compatibility, Risks and Tradeoffs (Default Shape Change)
- `docs/specs/semantic-search.md`: current flat chunk contract, cross-vault fan-out/merge/truncation rules, payload text controls
- `docs/specs/semantic-search.md`: documentation surface that must change once the default result shape changes
- `docs/reference/configuration.md`: daemon-global config precedent and config validation/startup-error expectations

## Coverage Map

| Source item | Proposed step | Status | Notes |
|---|---|---|---|
| § Summary | Step N | planned | Core capability scope |
| § Motivation | Step N | planned | Dominance failure mode drives grouping |
| § Current Baseline | Step N | planned | Implementation delta anchor |
| § Problem Statement | Step N | planned | Preserves both passage and document intents |
| § Design Goals | Step N | planned | Direct implementation targets |
| § Proposed Direction | Step N | planned | Granularity + grouped evidence |
| § Request and Response Shape | Step N | planned | Contract work |
| § Configurable Defaults | Step N | planned | Default/override/config validation support |
| § Document Scoring | Step N | planned | Max-score v1 rule |
| § Candidate Retrieval Depth | Step N | planned | Internal retrieval-depth knobs |
| § Chunk Evidence Per Document | Step N | planned | Nested chunk caps/order/payload controls |
| § Compatibility and Migration | Step N | planned | Default flips immediately; explicit chunk mode remains the compatibility path |
| § Deferred / Out of Scope | Step N | deferred | Explicitly held items listed below |
| § Risks and Tradeoffs | Step N | planned | Default-shape risk accepted due to small user base |
| § Validation and Test Strategy | Step N | planned | Core acceptance coverage |
| § Open Questions | Step N | planned | Resolve implementation-critical questions in Step N |
| `docs/specs/semantic-search.md` current chunk response | Step N | planned | Chunk mode must preserve current flat result shape |
| `docs/specs/semantic-search.md` cross-vault semantics | Step N | planned | Document grouping must preserve vault scoping, partial results, and deterministic merge/truncation behavior |
| `docs/reference/configuration.md` config model | Step N | planned | New defaults belong under daemon config; no per-vault config or registry schema work |
| `docs/specs/content-retrieval.md` retrieval boundary | Step N | planned | Document results should remain evidence previews/full chunks, not become full-file retrieval |

## Deferred / Out-of-Scope Items

- Item: Per-vault semantic search policy
  - Source: Proposal § Deferred / Out of Scope
  - Reason: Requires registry/API lifecycle design beyond v1 scope.
  - Revisit trigger: Dedicated policy/config proposal.
- Item: Registry schema changes for per-vault search settings
  - Source: Proposal § Deferred / Out of Scope
  - Reason: Explicitly out of current daemon-global defaults model.
  - Revisit trigger: Per-vault policy project starts.
- Item: Alternative document scoring formulas (top-N avg, weighted, saturating, dual fields)
  - Source: Proposal § Document Scoring, § Deferred / Out of Scope
  - Reason: Max-score is selected v1 baseline.
  - Revisit trigger: Ranking-quality feedback shows max-score issues.
- Item: Reranking / HyDE / hybrid fusion
  - Source: Proposal § Deferred / Out of Scope
  - Reason: Separate retrieval-quality features.
  - Revisit trigger: Separate proposal acceptance.
- Item: Pagination/cursors for deep semantic browsing
  - Source: Proposal § Deferred / Out of Scope
  - Reason: Not required for bounded v1 grouped output.
  - Revisit trigger: Frequent under-fill or deep-browse requirements.
- Item: Chunking, embedding-model, or vector-storage changes
  - Source: Proposal § Deferred / Out of Scope
  - Reason: Proposal is result-shaping only.
  - Revisit trigger: Independent indexing/embedding initiative.
- Item: Splitting semantic search into separate tools
  - Source: Proposal § Deferred / Out of Scope
  - Reason: Proposal keeps one operation with request-time mode.
  - Revisit trigger: API ergonomics evidence justifies split.
- Item: Prefix filtering SQL strategy fix in this feature
  - Source: Proposal § Risks and Tradeoffs (Prefix Filtering Behavior)
  - Reason: Known recall limitation acknowledged but not required to ship grouped results.
  - Revisit trigger: If scoped/prefix semantic recall becomes a blocking defect.

## Open Questions

- Question: Tagged result enum vs granularity-implied shape?
  - Why it matters: Affects schema clarity and client parsing stability.
  - Blocks roadmap? no
  - Suggested owner: API/spec implementer.
- Question: Should `chunks_per_document` be ignored or rejected in chunk mode?
  - Why it matters: Affects validation contract and client error expectations.
  - Blocks roadmap? no
  - Suggested owner: API implementer + spec editor.
- Question: Should `truncated` distinguish final-limit truncation from candidate-cap truncation?
  - Why it matters: Operator observability and debugging of under-fill behavior.
  - Blocks roadmap? no
  - Suggested owner: API/spec implementer.
- Question: Should document results include extra fields (`matched_chunk_count`, `best_chunk_index`) in v1?
  - Why it matters: Improves explainability but expands contract surface.
  - Blocks roadmap? no
  - Suggested owner: Product/contract owner.
- Question: Should prefix-filtering strategy be fixed in the same step?
  - Why it matters: Current kNN-first behavior can still under-return scoped queries.
  - Blocks roadmap? no
  - Suggested owner: Search implementation owner.
- Question: Should default flip to `document` happen immediately after capability lands, or after one compatibility window?
  - Why it matters: Controls migration risk for callers relying on implicit chunk shape.
  - Blocks roadmap? no
  - Suggested owner: resolved by maintainer review, 2026-05-08: flip immediately; keep explicit chunk mode.

## Recommendation

Proceed to:

- [x] Draft/update `notes/roadmap/roadmap-N.md`
- [x] Draft/update `notes/roadmap/step-NN-workplan.md`
- [ ] Refine planning inputs first

Rationale:

Recommended next action: **start Step N after the active Step 24 / Round 13 work is either completed or intentionally paused.** The step is plannable now: add `granularity`, implement document-mode grouping, preserve explicit chunk mode, and make document mode the built-in default immediately.

Maintainer review resolved the rollout decision in favor of a single step. The next roadmap item should implement document granularity and flip the default in the same step, while preserving explicit chunk mode. If the orchestrator wants a concrete next step number after Round 13, the natural assignment is **Step 25 — Semantic Document Results**.

## Human Review Notes

- 2026-05-08: Maintainer accepted the risk of an immediate default-shape change because the user base is very small. Collapse the prior two-step compatibility rollout into one step: document mode defaults immediately; explicit `granularity: "chunk"` remains available.
