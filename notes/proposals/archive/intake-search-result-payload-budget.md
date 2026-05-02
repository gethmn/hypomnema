# Proposal Intake: Search Result Payload Budget

**Status**: Intake complete
**Date**: 2026-05-01
**Intake inputs**:

- `notes/proposals/archive/search-result-payload-budget.md` — Primary proposal defining the feature
- `notes/proposals/archive/search-result-payload-budget-stories.md` — Acceptance criteria for six user stories

---

## Summary

Semantic search currently returns unbounded full chunk text by default, which can flood an agent's conversation context when ranked-result counts default to 100. This proposal fixes two separable issues: (1) correct the semantic `limit` default from 100 to 10 across API schema, docs, and tests; (2) introduce explicit text-budgeting controls via `include_text` enum (`preview` | `full` | `none`) and `preview_bytes` parameter, so agents can request bounded preview text (default), full chunks (explicit opt-in), or metadata-only candidate lists. Filesystem and content search remain unchanged. Response shaping is applied after retrieval, not at index time.

## Source Inputs

| Source | Type | Role in intake |
|---|---|---|
| `notes/proposals/archive/search-result-payload-budget.md` | proposal | primary — defines the feature, design constraints, and implementation notes |
| `notes/proposals/archive/search-result-payload-budget-stories.md` | stories | primary — six acceptance-criteria user stories covering all changes |
| `docs/specs/semantic-search.md` | spec | background — canonical semantic-search contract that this proposal amends |
| `docs/specs/content-search.md` | spec | background — content search remains unchanged but stories audit its budget drift |
| `docs/specs/filesystem-search.md` | spec | background — filesystem search remains unchanged |
| `notes/proposals/content-retrieval.md` | proposal | background — deferred; this proposal explicitly keeps search and retrieval separate |

## Candidate Outcomes

- **Outcome: Low-payload semantic search default**
  - Source: Story 1 + Story 2
  - User-visible result: Semantic queries default to 10 results with 600-byte preview text, reducing default agent context load by ~70–85% vs current 100-full-text behavior.
  - Verification signal: `POST /search/semantic` without parameters returns ≤10 results; agents report smaller context footprint for typical queries.

- **Outcome: Explicit full-text semantic queries**
  - Source: Story 3
  - User-visible result: `include_text: "full"` lets callers trade payload size for immediate context when result count is small.
  - Verification signal: `include_text: "full"` + `limit: 3` returns complete chunk text; response metadata marks it as `full`.

- **Outcome: Cheap metadata-only semantic candidate pass**
  - Source: Story 4
  - User-visible result: `include_text: "none"` returns score, path, chunk index, heading breadcrumb, and vault identity without text; enables broad ranking filters before selective content retrieval.
  - Verification signal: `include_text: "none"` returns 30–40% smaller JSON than default preview for same query.

- **Outcome: Corrected search-mode documentation**
  - Source: Story 5 + Story 6
  - User-visible result: Specs and API schemas agree on `limit` defaults (semantic: 10, content/filesystem: 100) and content-search text budgets.
  - Verification signal: `rg 'Defaults to 100' src/api docs/specs tests` returns zero semantic-search survivors; canonical specs and generated schemas match.

## Proposed Roadmap Shape

### Step N — Semantic Limit Correction and Text Budgeting

**Goal**:
Reduce default semantic search payload size by capping result count to 10 and introducing bounded preview text, while keeping search and retrieval as separate operations.

**Shipping criteria**:

- [ ] Semantic search defaults to `limit: 10`, not 100; filesystem and content remain at 100.
- [ ] Semantic responses include `text`, `text_kind` (preview / full), and `text_truncated` (boolean) fields when `include_text` is `preview` or `full`.
- [ ] `preview_bytes` parameter accepted (default 600, max server-configurable, clamped to [1, max]); preview text never invalid UTF-8.
- [ ] `include_text: "none"` omits all text payload; `include_text: "full"` returns complete stored chunk text.
- [ ] Invalid `include_text` values return 400 `invalid_request` with message: `include_text must be one of preview, full, none`.
- [ ] HTTP and MCP schemas agree on new request/response fields and default values.
- [ ] Negative fingerprint clean: `rg 'Defaults to 100|DEFAULT_LIMIT == 100' src/api tests docs/specs` returns no semantic-search survivors.
- [ ] All six user stories pass acceptance criteria.
- [ ] Manual testing covers default (10-result, 600-byte preview), explicit large `limit`, and `include_text: "none"` modes.
- [ ] `docs/specs/semantic-search.md` canonically specifies the text-budget contract and wire shape.

**Deferred decisions resolved in this step**:

- Decision: Use `text` + `text_kind` + `text_truncated` metadata vs new `preview` field
  - Source: Proposal § Open Questions
  - Why this step: Backward compatibility favors keeping `text` for `preview`/`full` responses and adding metadata; clarity favors a separate field. Backward-compat choice is the safer default and can be revisited if agents report confusion.

- Decision: Server maximum for `preview_bytes`
  - Source: Proposal § Open Questions
  - Why this step: Proposal suggests 600–800; implementation testing against real dogfood payloads should land on a specific value (e.g., 600 for ≤3 preview chunks, 1000 for broader browsing). Must be documented in `docs/specs/semantic-search.md`.

- Decision: Include `content_hash` in semantic results now
  - Source: Proposal § Open Questions
  - Why this step: Semantic spec names it in chunk metadata; adding it to the response improves retrieval consistency and cache validation. Recommend **yes, add it now** — it's a stable identifier with clear reuse in future content-retrieval ops.

**New deps**:

- (none — response shaping is pure string trimming, no new crates)

**Risk**: low

Rationale: The limit correction is isolated and low-risk (one constant, schema cleanup, test fix). Text budgeting adds new request/response fields but the contract is explicit and the truncation logic is straightforward. No chunking changes, no index changes, no SQL schema changes. Backward compatibility is well-defined (old code sending no `include_text` gets preview by default). The only new runtime cost is preview-text generation, which is string slicing in the response path — no `spawn_blocking` needed.

**Source coverage**:

- Story 1 (Correct Semantic Default Limit Drift): This step
- Story 2 (Return Bounded Semantic Preview Text by Default): This step
- Story 3 (Allow Explicit Full-Text Semantic Results): This step
- Story 4 (Allow Metadata-Only Semantic Candidate Lists): This step
- Story 5 (Keep Search and Retrieval Boundaries Separate): This step (confirms separation, defers content-retrieval to separate proposal)
- Story 6 (Verify Content Search Budget Drift): This step (audit only; confirm content/filesystem defaults match specs, no changes to those modes)

---

## Coverage Map

| Source item | Proposed step | Status | Notes |
|---|---|---|---|
| Story 1: Semantic default limit 10 | Step N | planned | Low-risk; isolated to constant + schema |
| Story 2: Bounded preview text default | Step N | planned | Core feature; preview generation is pure-string work |
| Story 3: Explicit `include_text: "full"` | Step N | planned | Request/response field addition |
| Story 4: `include_text: "none"` for metadata-only | Step N | planned | Simple conditional field omission |
| Story 5: Search/retrieval separation | Step N | planned | Confirms via stable identifiers (`file_path`, `chunk_index`, `content_hash`, vault fields) |
| Story 6: Content search budget audit | Step N | planned | Verify defaults in specs/schema/code agree; no code changes unless drift found |
| Proposal § Deferred decision: text field strategy | Step N | planned | Decide between `text` + metadata vs new `preview` field; recommend existing-field + metadata |
| Proposal § Deferred decision: server max preview_bytes | Step N | planned | Confirm via dogfood testing; document in semantic spec |
| Proposal § Deferred decision: include content_hash | Step N | planned | Recommend **yes**; adds stability for follow-up retrieval |

---

## Deferred / Out-of-Scope Items

- Item: Full-file content retrieval
  - Source: Proposal § Integration Points, § What not to build
  - Reason: Search should stay cheap discovery; full-file reads belong in a separate operation. Proposal explicitly defers to `notes/proposals/content-retrieval.md`.
  - Revisit trigger: When content-retrieval proposal is accepted and begins implementation.

- Item: Chunk/section-level retrieval
  - Source: Proposal § Open Questions, § What not to build
  - Reason: Out of scope for this proposal. If needed, should amend content-retrieval or get its own small proposal.
  - Revisit trigger: If agents report needing finer-grained retrieval than full-file.

- Item: Boilerplate stripping at index time
  - Source: Proposal § Edge Cases § Boilerplate-heavy chunks
  - Reason: Code blocks and raw sections can be semantically important. Stripping at chunking time would mutate embeddings and searchable corpus; needs separate research.
  - Revisit trigger: If dogfood reports consistent Dataview-noise problems in results.

- Item: Content search `include_matches` default audit → correction
  - Source: Proposal § Open Questions (half-question)
  - Reason: Story 6 audits whether content-search defaults have drifted from canonical spec. If drift is found, correction can be bundled into this step or deferred to separate content-search hardening.
  - Revisit trigger: If Story 6 acceptance criteria uncover actual drift; prioritize in this step if found.

---

## Open Questions

- Question: Should `content_hash` be added to semantic results in this step?
  - Why it matters: Semantic spec names it in chunk metadata but does not currently expose it in responses. Adding it improves retrieval consistency and cache validation, and sets up stability for future content-retrieval operations.
  - Blocks roadmap? No
  - Suggested owner: Task agent (implementation detail; can be decided at workplan time)
  - **Intake recommendation**: **Yes, add it now.** Low friction, clear reuse, matches semantic spec intent.

- Question: What is the final server maximum for `preview_bytes`?
  - Why it matters: Proposal suggests 600–800 bytes as a starting point. Too small and previews are unhelpful; too large and they reintroduce the context-load problem. Real dogfood data should inform the final value.
  - Blocks roadmap? No
  - Suggested owner: Task agent (validation during implementation testing)
  - **Intake recommendation**: Default to 600 bytes for initial implementation; validate during dogfood pass; document the value and rationale in `docs/specs/semantic-search.md`.

- Question: Will semantic search ever need a chunk/section-level retrieval operation, or is full-file content-retrieval sufficient?
  - Why it matters: Affects the scope of future content-retrieval work and retrieval UX for large files.
  - Blocks roadmap? No
  - Suggested owner: Future round that implements content-retrieval
  - **Intake note**: Out of scope for this intake; deferred until content-retrieval proposal is under way.

---

## Recommendation

**This proposal is a strong candidate for Round 8. Input is complete; triage for inclusion in the next roadmap.**

Rationale:

1. **Input is complete and well-formed.** Proposal + stories + existing specs (semantic, content, filesystem) provide sufficient detail. No missing context. No questions block planning.

2. **Design is solid and scoped tightly.** The feature separates two concerns (limit correction + text budgeting) but they ship together; change is localized to response shaping (no index, SQL, schema changes); backward compatibility is preserved (old callers get preview by default); and deferred decisions are lightweight (field strategy, preview-bytes cap, content-hash inclusion).

3. **Risk is manageable.** Low-risk correction (limit constant) + medium-risk addition (text-budget fields) with clear contract and well-defined error cases. No new dependencies. No concurrent-access hazards. No async/SQLite interaction.

4. **Timing context.** Round 7 (dependency upgrades) shipped 2026-05-01. No Round 8 roadmap exists yet. This proposal is ready to anchor an early step in Round 8, clearing a high-traffic problem (semantic search payload bloat) that has been noted in multiple retros and design reviews.

5. **Recommended next step**: When Round 8 roadmap planning begins:
   - Confirm this proposal for inclusion as an early step.
   - If accepted, pair it with orthogonal work in the round so Round 8 doesn't depend solely on search-result budgeting.
   - Assign a step number and draft the workplan incorporating the acceptance criteria and deferred-decision guidance above.

---

## Human Review Notes

(append review decisions here)
