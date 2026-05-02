# Hypomnema Roadmap — Round 8: Search Result Payload Budget

**Scope**: Land the search-result payload budget feature: correct the semantic search `limit` default from 100 to 10, and introduce explicit text-budgeting controls (`include_text`, `preview_bytes`). Single focused step.

**Status**: Shipped 2026-05-02. All shipping criteria met. See commit 2888743 (verification gate). Archived.

**Process**: Same as rounds 1–7. One step. Coordinator + researcher + ephemeral builders. See [`notes/coordinator-playbook.md`](../coordinator-playbook.md) and [`notes/playbook/`](../playbook/) for the orchestration contract.

**Intake**: Complete — [`notes/proposals/intake-search-result-payload-budget.md`](../proposals/intake-search-result-payload-budget.md)

**Why this round**:

- Semantic search currently returns unbounded full chunk text by default (limit 100), flooding agent context on typical queries.
- The fix is localized: one constant correction, new request/response fields, response-path string slicing. No index changes, no SQL schema changes, no new crates.
- Intake is complete. All six acceptance-criteria stories are mapped. Deferred decisions are lightweight and all resolved in this step.
- Risk is low. The limit correction is isolated; the text-budgeting fields are new additions with well-defined backward compatibility (old callers get preview by default).

**Skills carrying forward**: none specifically — this step is pure API/response-shaping work.

**New deps**: none.

---

## Phasing

One step:

| Step | Contents | Risk |
| ---- | -------- | ---- |
| 18 | Semantic limit correction + text budgeting (`include_text`, `preview_bytes`) | Low |

---

## Step 18 — Semantic Limit Correction and Text Budgeting

**Goal**: Correct the semantic search `limit` default from 100 to 10, and add `include_text` (`preview` | `full` | `none`) and `preview_bytes` request parameters with matching response fields (`text_kind`, `text_truncated`). HTTP and MCP schemas agree on the new contract. All six intake stories pass acceptance criteria.

**Shipping criteria**:

- [x] Semantic search defaults to `limit: 10`; filesystem and content search remain at 100.
- [x] Semantic responses include `text`, `text_kind` (`preview` | `full`), and `text_truncated` (boolean) when `include_text` is `preview` or `full`.
- [x] `preview_bytes` parameter accepted (default 600, max server-configurable, clamped to [1, max]); preview text never invalid UTF-8.
- [x] `include_text: "none"` omits all text payload; `include_text: "full"` returns complete stored chunk text.
- [x] Invalid `include_text` values return 400 `invalid_request`: `include_text must be one of preview, full, none`.
- [x] HTTP and MCP schemas agree on new request/response fields and defaults.
- [x] `content_hash` is included in semantic search results.
- [x] Negative fingerprint clean: `rg 'Defaults to 100|DEFAULT_LIMIT == 100' src/api tests docs/specs` returns no semantic-search survivors.
- [x] All six intake stories pass acceptance criteria.
- [x] Manual testing covers default (10-result, 600-byte preview), explicit large `limit`, and `include_text: "none"` modes.
- [x] `docs/specs/semantic-search.md` canonically specifies the text-budget contract and wire shape.
- [x] `cargo test` and `cargo clippy -- -D warnings` pass.

**Deferred decisions to resolve at workplan-time**:

- Exact server-side maximum for `preview_bytes` (proposal suggests 600–800; validate against dogfood; document in spec).
- Field strategy: `text` + `text_kind` + `text_truncated` is the recommended shape (backward-compat; old `text` field stays for preview/full).
- Whether content-search defaults have drifted from canonical spec (Story 6 audit; correction can bundle here if drift found).

**Risk**: low. See intake rationale.

---

## Notes on the round-8 shipping gate

1. Semantic search defaults to 10 results with bounded preview text.
2. `include_text` and `preview_bytes` are accepted and correctly applied.
3. `content_hash` is present in semantic results.
4. All six intake stories pass acceptance criteria.
5. Existing tests stay green; any expectation changes for the limit constant are called out explicitly.
6. Specs and schemas agree.
7. Round tag: `v0.7.0` once gate is met.

After the gate hits, round 8 archives alongside its step workplan, and round 9's roadmap is written when the human picks the next focus.

---

## Out of scope for round 8

These stay in [`notes/backlog.md`](../backlog.md) and are explicitly not part of this round:

- Full-file content retrieval (separate proposal: `notes/proposals/content-retrieval.md`)
- FTS5 BM25 content search improvements
- HyDE semantic search
- Search-error typed classification
- Release automation
- Any unrelated polish or backlog items
