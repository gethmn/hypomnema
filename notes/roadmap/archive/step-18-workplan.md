# Step 18 Workplan -- Search Result Payload Budget

**Status**: Shipped 2026-05-02. All 8 tasks completed. Verification gate passed (todo 163). Archived.

**Roadmap**: [`notes/roadmap/roadmap-8.md`](./roadmap-8.md) § Step 18.

**Intake**: [`notes/proposals/archive/intake-search-result-payload-budget.md`](../../proposals/archive/intake-search-result-payload-budget.md) (six-story coverage map, deferred-decision recommendations).

**Inputs**:

- Proposal: [`notes/proposals/archive/search-result-payload-budget.md`](../../proposals/archive/search-result-payload-budget.md)
- Stories: [`notes/proposals/archive/search-result-payload-budget-stories.md`](../../proposals/archive/search-result-payload-budget-stories.md)
- Spec to amend: [`docs/specs/semantic-search.md`](../../docs/specs/semantic-search.md)
- Spec audited: [`docs/specs/content-search.md`](../../docs/specs/content-search.md)
- Skill: [`.claude/skills/rusqlite-in-async/SKILL.md`](../../.claude/skills/rusqlite-in-async/SKILL.md)

---

## Step Goal

Cap default semantic search payload size and make text inclusion explicit. Correct the semantic `limit` default from 100 to 10, add `include_text` (`preview` | `full` | `none`) and `preview_bytes` request fields with matching response metadata (`text_kind`, `text_truncated`), and wire it through HTTP, MCP, and CLI without changing the index, the embedding path, or the SQL shape beyond projecting `content_hash`. Filesystem and content search remain at `limit: 100`. The drift discovered during Story 6 audit (content-search `include_matches` default in code vs canonical spec) is corrected in this step rather than deferred. Backward compatibility is preserved: callers that omit the new fields receive the new default behavior (10 results, 600-byte preview), which is the desired contract — no compat shim required.

---

## Pre-Build Deferred-Decision Resolutions

These resolve the three open items called out in the roadmap and intake. They are pinned so the builder does not have to relitigate them.

### D1. Field strategy: keep `text`, add `text_kind` + `text_truncated`

**Decision**: The wire field stays named `text`. New companion fields `text_kind` (`"preview"` | `"full"`) and `text_truncated` (`bool`) are added alongside it. No new `preview` field.

**Why**:

- Backward-compatible reads: any consumer that already projects `result.text` continues to work for `include_text: "preview"` (default) and `include_text: "full"`.
- `include_text: "none"` is the only mode that changes wire shape — `text` is omitted entirely. Consumers that want metadata-only opt in deliberately, so they expect the absence.
- Two new metadata fields are cheaper to add and document than a renamed payload field plus a deprecation window for `text`.

**How to apply**: In `SemanticResultJson`, mark `text` as `Option<String>`, `text_kind` as `Option<String>`, `text_truncated` as `Option<bool>`, all three serde-skipped when `None`. They are all `None` exactly when `include_text == "none"`. Otherwise `text_kind` is `"preview"` or `"full"` and `text_truncated` is set per the truncation outcome.

### D2. `preview_bytes` server maximum: 2000 bytes

**Decision**: Server maximum for `preview_bytes` is **2000 bytes**. Default remains **600 bytes**. Values are clamped to `[1, 2000]` (zero is rejected as `invalid_request` per the canonical proposal contract; values above 2000 are clamped, not rejected, and the response carries `text_truncated: true` if the underlying chunk was longer than the clamp).

**Why**:

- 600 bytes is the agent-friendly common path. A typical English paragraph fits; full code blocks usually do not.
- 2000 bytes gives roughly 3× headroom for callers who want a "broader preview" without crossing into `include_text: "full"` territory. It is well below typical multi-paragraph chunk sizes, so the preview/full distinction stays meaningful.
- Hard-rejecting above-max would force callers into try/retry loops; clamping is friendlier and consistent with `min_similarity` clamp behavior already in the canonical spec.
- Zero is rejected because the proposal explicitly enumerates it as `invalid_request: preview_bytes must be greater than 0`.

**How to apply**: Define `const SEMANTIC_PREVIEW_BYTES_MAX: usize = 2000;` next to the new semantic limit constant. Validate in the request-handler layer (`src/api/search.rs`), not in `src/search/semantic.rs`, so the SQL layer stays unaware of preview shape. Document the clamp + the rationale in the spec amendment (Task 18.7).

### D3. `content_hash` in semantic results: include it now

**Decision**: Add `content_hash` to the per-result wire shape. Project it from the `chunks` row in the kNN SQL and surface it in `SemanticResultJson` as a non-optional string.

**Why**:

- Semantic spec § Chunking already names `content_hash` on every chunk's metadata; absence in the response is drift, not a deliberate omission.
- Adding it now closes the search/retrieval boundary cleanly for Story 5: callers can verify a chunk is still backed by the same parent file before issuing a future content-retrieval request, without a separate metadata roundtrip.
- Cost is one extra column in the SQL projection and one new field on the response struct. No new joins, no schema migrations.

**How to apply**: Extend the kNN `SELECT` in `src/search/semantic.rs` to project `c.content_hash`; add `content_hash: String` to `SemanticResult` and `SemanticResultJson`; populate it in `semantic_to_json`.

---

(Full task list available in the shipped workplan — 8 tasks, all complete as of 2026-05-02)
