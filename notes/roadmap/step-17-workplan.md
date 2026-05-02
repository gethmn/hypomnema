# Step 17 Workplan -- Search Result Payload Budget

**Status**: Drafted 2026-05-01. Awaiting human approval before build.

**Roadmap**: [`notes/roadmap/roadmap-8.md`](./roadmap-8.md) § Step 17.

**Intake**: [`notes/proposals/intake-search-result-payload-budget.md`](../proposals/intake-search-result-payload-budget.md) (six-story coverage map, deferred-decision recommendations).

**Inputs**:

- Proposal: [`notes/proposals/search-result-payload-budget.md`](../proposals/search-result-payload-budget.md)
- Stories: [`notes/proposals/search-result-payload-budget-stories.md`](../proposals/search-result-payload-budget-stories.md)
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

**How to apply**: Define `const SEMANTIC_PREVIEW_BYTES_MAX: usize = 2000;` next to the new semantic limit constant. Validate in the request-handler layer (`src/api/search.rs`), not in `src/search/semantic.rs`, so the SQL layer stays unaware of preview shape. Document the clamp + the rationale in the spec amendment (Task 17.7).

### D3. `content_hash` in semantic results: include it now

**Decision**: Add `content_hash` to the per-result wire shape. Project it from the `chunks` row in the kNN SQL and surface it in `SemanticResultJson` as a non-optional string.

**Why**:

- Semantic spec § Chunking already names `content_hash` on every chunk's metadata; absence in the response is drift, not a deliberate omission.
- Adding it now closes the search/retrieval boundary cleanly for Story 5: callers can verify a chunk is still backed by the same parent file before issuing a future content-retrieval request, without a separate metadata roundtrip.
- Cost is one extra column in the SQL projection and one new field on the response struct. No new joins, no schema migrations.

**How to apply**: Extend the kNN `SELECT` in `src/search/semantic.rs` to project `c.content_hash`; add `content_hash: String` to `SemanticResult` and `SemanticResultJson`; populate it in `semantic_to_json`.

---

## Task Breakdown

Tasks are ordered for safe incremental builds. Each task is independently runnable against `cargo test` and `cargo clippy -- -D warnings` without blocking the next. The coordinator may bundle 17.1 + 17.2 in a single batch and 17.4 + 17.5 + 17.6 in another, but the sequencing must hold.

### Task 17.1 -- Pin Semantic Default Limit

**Purpose**: Land the smallest, lowest-risk change first so the rest of the work happens against a corrected baseline.

**Work**:

- In `src/api/search.rs`:
  - Replace shared `DEFAULT_LIMIT` use in `run_semantic_search` with a new `const DEFAULT_SEMANTIC_LIMIT: usize = 10;` (keep `DEFAULT_LIMIT` for filesystem and content; do not delete it).
  - Update `req.limit.unwrap_or(...)` in `run_semantic_search` to use the new constant.
- In `src/api/types.rs`:
  - Update the `limit` `#[schemars(description = ...)]` on `SemanticQueryJson` to read "Defaults to 10" (currently reads "Defaults to 100"). Filesystem and content stay at 100.
- Add or update a test in `src/api/tests.rs` that asserts: `POST /search/semantic` with no `limit` against a fixture with > 10 chunks returns exactly 10 results.

**Files likely touched**:

- `src/api/search.rs`
- `src/api/types.rs`
- `src/api/tests.rs`

**Tests**:

- New: semantic default-limit caps at 10.
- New: semantic explicit `limit: 17` returns up to 17 results when fixture is large enough.
- Sanity: filesystem and content default-limit tests still pass at 100.

**Risk**: low. One constant, one schema description, one test. The change is the canonical contract; nothing else is shifting yet.

### Task 17.2 -- Add `truncated` to `SemanticSearchResponse`

**Purpose**: The semantic spec already requires `truncated` at the top level, but `SemanticSearchResponse` does not currently expose it. This is pre-existing drift that the proposal flagged; bundling it here keeps the limit-correction work and the response-shape correction in one round.

**Work**:

- In `src/api/types.rs`:
  - Add `pub truncated: bool` to `SemanticSearchResponse` (mirrors `FilesystemSearchResponse` and `ContentSearchResponse`).
- In `src/search/semantic.rs`:
  - Change `search_semantic`'s success return type from `(Vec<SemanticResult>, Option<String>)` to `(Vec<SemanticResult>, Option<String>, bool)` where the `bool` is whether the per-vault SQL hit the `k = limit` cap (proxy for "this vault had more than the requested limit"). Compute it as `rows_before_min_similarity_filter.len() >= limit`.
- In `src/api/search.rs::run_semantic_search`:
  - Aggregate `any_truncated` across per-vault returns (mirror filesystem/content shape).
  - After the cross-vault merge truncation (`all_results.truncate(limit)`), set `was_capped` and union it with `any_truncated`. Set `truncated` on the response accordingly.
- Update `src/api/tests.rs` semantic shape tests (response tests near `semantic_search_response_serializes_vault_and_vault_name`) to construct `SemanticSearchResponse { ..., truncated: false }`.
- Update the in-process backend semantic test (`src/mcp/backend_in_process.rs`) and any HTTP test fixtures that construct or pattern-match on the response.

**Files likely touched**:

- `src/api/types.rs`
- `src/search/semantic.rs`
- `src/api/search.rs`
- `src/api/tests.rs`
- `src/mcp/backend_in_process.rs`
- `src/mcp/server.rs` (if test fixtures match the response)
- `tests/mcp.rs`, `tests/mcp_http.rs`, `tests/multi_vault_internal.rs` if they assert on the response keys

**Tests**:

- New: semantic search where total cross-vault rows > `limit` returns `truncated: true`.
- New: semantic search where total rows ≤ `limit` returns `truncated: false`.
- Existing: every test that constructs `SemanticSearchResponse` literal needs `truncated: <bool>`.

**Risk**: medium. The struct addition is mechanical, but a wide compile-fix sweep will hit several test fixtures. No behavior change for callers that did not look at `truncated` before; new field is additive on the wire.

### Task 17.3 -- Add `content_hash` to Semantic Results

**Purpose**: Close the metadata gap (D3) so Story 5 (search/retrieval boundary) lands cleanly.

**Work**:

- In `src/search/semantic.rs`:
  - Add `pub content_hash: String` to `SemanticResult`.
  - Extend the kNN `SELECT` to project `c.content_hash` (column is already on `chunks`, used in seed fixtures throughout the test suite).
  - Populate `content_hash` in the `query_map` row mapper.
- In `src/api/types.rs`:
  - Add `pub content_hash: String` to `SemanticResultJson` (non-optional; every chunk has a non-empty parent-file hash).
- In `src/api/search.rs::semantic_to_json`:
  - Forward `r.content_hash` into the JSON struct.
- In `src/search/semantic.rs` unit tests:
  - Update `seed_chunk` callers and assertions where a result is inspected, to assert `content_hash == "sha256:00"` (the value already seeded in fixtures).

**Files likely touched**:

- `src/search/semantic.rs`
- `src/api/types.rs`
- `src/api/search.rs`
- `src/api/tests.rs`
- Any HTTP/MCP test fixtures that inspect semantic results by field

**Tests**:

- New (or extended): the result-shape test asserts a populated `content_hash` matching the seeded value.
- Existing semantic kNN tests: still pass; only the result struct grows.

**Risk**: low. SQL projection + struct field; no new joins or schema work. The value is already on the row.

### Task 17.4 -- Request Validation: `include_text` + `preview_bytes`

**Purpose**: Land the new request fields and their validation contract before any response-shape changes depend on them.

**Work**:

- In `src/api/types.rs`:
  - Add to `SemanticQueryJson`:
    - `pub include_text: Option<String>` (`#[serde(default, skip_serializing_if = "Option::is_none")]`, with a `#[schemars(description = "...")]` enumerating `preview | full | none` and the default `preview`).
    - `pub preview_bytes: Option<usize>` with description noting default 600, server max 2000, clamp behavior, and applicability only when `include_text == "preview"`.
  - **Note**: An enum-typed `include_text` would be schema-cleaner than a raw `String`, but rmcp/schemars's enum-string handling needs a small adapter and changes the JSON shape. Use `Option<String>` and validate at the handler layer to keep the schema flat. The validation boundary is the request handler, where we already do `min_similarity` clamping.
- In `src/api/search.rs`:
  - Define `const DEFAULT_PREVIEW_BYTES: usize = 600;` and `const SEMANTIC_PREVIEW_BYTES_MAX: usize = 2000;` near the new `DEFAULT_SEMANTIC_LIMIT`.
  - Add a small `IncludeText` enum `{ Preview, Full, None }` and a `parse_include_text(opt: Option<&str>) -> Result<IncludeText, ApiError>` that:
    - returns `Preview` for `None` (default) and `Some("preview")`,
    - returns `Full` for `Some("full")`,
    - returns `None` (the variant) for `Some("none")`,
    - returns `ApiError::invalid_request("include_text must be one of preview, full, none")` for any other value.
  - Add a `resolve_preview_bytes(opt: Option<usize>) -> Result<usize, ApiError>` helper:
    - `None` → 600.
    - `Some(0)` → `ApiError::invalid_request("preview_bytes must be greater than 0")`.
    - `Some(n)` → `n.min(SEMANTIC_PREVIEW_BYTES_MAX)` (clamp; do not error).
  - In `run_semantic_search`, call both helpers up front and propagate to Task 17.5's response shaping. Do NOT pass `preview_bytes` into the SQL layer; pass it as a field on a new shaping struct local to `run_semantic_search`.
- Add validation tests in `src/api/tests.rs`:
  - Reject `include_text: "preivew"` (typo) with HTTP 400, code `invalid_request`, message exactly matching the canonical string.
  - Reject `preview_bytes: 0` with HTTP 400, code `invalid_request`.
  - Accept `preview_bytes: 999999` and observe clamp via the response (combine with Task 17.5's tests).

**Files likely touched**:

- `src/api/types.rs`
- `src/api/search.rs`
- `src/api/tests.rs`

**Tests**:

- New: invalid `include_text` enum value → 400.
- New: `preview_bytes: 0` → 400.
- New: `preview_bytes` above max → no error; subsequent task verifies clamp behavior in the response.
- New: schema test asserting `SemanticQueryJson` schema lists `include_text` and `preview_bytes` properties with non-empty descriptions (mirror existing `semantic_query_json_schema_has_min_similarity`).

**Risk**: low-medium. Validation surface is small, but the new request shape needs schema parity for MCP. The schema test pins the contract.

### Task 17.5 -- Response Shaping: Apply `include_text` and `preview_bytes`

**Purpose**: Apply the validated request fields to the per-result wire shape.

**Work**:

- In `src/api/types.rs`:
  - Change `SemanticResultJson::text` from `String` to `Option<String>` (`#[serde(default, skip_serializing_if = "Option::is_none")]`).
  - Add `pub text_kind: Option<String>` and `pub text_truncated: Option<bool>` with the same serde-skip guard. They are `Some` exactly when `text` is `Some`.
- In `src/api/search.rs::semantic_to_json` (or a new shaping function alongside it):
  - Take the resolved `IncludeText` and `preview_bytes` as parameters.
  - Build the JSON result based on the mode:
    - `IncludeText::None` → `text: None`, `text_kind: None`, `text_truncated: None`.
    - `IncludeText::Full` → `text: Some(r.text.clone())`, `text_kind: Some("full")`, `text_truncated: Some(false)`.
    - `IncludeText::Preview` → call `make_preview(&r.text, preview_bytes)` which returns `(preview_string, was_truncated_bool)`. Set `text: Some(preview_string)`, `text_kind: Some("preview")`, `text_truncated: Some(was_truncated_bool)`.
- Implement `make_preview(s: &str, max_bytes: usize) -> (String, bool)` as a free function in `src/api/search.rs`:
  - If `s.len() <= max_bytes` → `(s.to_string(), false)`.
  - Otherwise: walk the string by `char_indices()` to find the largest byte index `<= max_bytes` that lands on a UTF-8 boundary; truncate there. Return `(slice.to_string(), true)`.
  - Note: byte-cap, UTF-8 safe; do not try to land on paragraph or sentence boundaries in v0 (the spec's edge-case section explicitly says boundary heuristics are workplan-time details and can be omitted unless they change the wire contract).
- Update `src/bin/hmn.rs::append_semantic_block` to handle `text: Option<String>` (currently unconditional `r.text`):
  - If `text` is `Some`, render it as today.
  - If `text` is `None`, render a placeholder (e.g., `(no text — include_text: none)`) or just skip the body line. Skip is simpler; document the choice in the function comment.
- The `SemanticQueryJson` from `src/bin/hmn.rs::cmd_search_semantic` should pass through the new fields. The CLI does not yet expose them as flags in this step (defer flag wiring to a follow-up backlog item); leave them at `None` (= server defaults) at the call site.

**Files likely touched**:

- `src/api/types.rs`
- `src/api/search.rs`
- `src/bin/hmn.rs`
- `src/api/tests.rs`
- Any test fixture that constructs `SemanticResultJson` literals

**Tests**:

- New: `include_text: "none"` returns results with `text`/`text_kind`/`text_truncated` absent; metadata fields (`score`, `file_path`, `chunk_index`, `heading_path`, `content_hash`, `vault`, `vault_name`) all populated.
- New: default `include_text` (omitted) returns `text_kind: "preview"`; chunk longer than 600 bytes returns `text_truncated: true` and `text` no longer than 600 bytes.
- New: chunk shorter than 600 bytes returns `text_truncated: false` and full chunk in `text`.
- New: `include_text: "full"`, `limit: 3` returns `text_kind: "full"`, `text_truncated: false`, full stored content.
- New: UTF-8 truncation never produces invalid UTF-8 — fixture chunk seeded with mid-multibyte boundary characters; assert `text` is valid UTF-8 and `text.len() <= 600`.
- New: `preview_bytes: 100000` is clamped to 2000 — assert `text.len() <= 2000` for a long chunk and `text_truncated: true` if the chunk exceeded 2000.

**Risk**: medium. Most behavior change in this step lives here. The struct change to `Option<String>` is a wire-shape break for any external consumer that already deserialized into a struct with `text: String` — but no Hypomnema-internal consumer does that today, and the proposal explicitly takes this hit. Document it in the spec amendment.

### Task 17.6 -- Audit Content Search Default (Story 6) and Correct Drift

**Purpose**: Story 6 asks us to verify content-search defaults agree across spec, schema, and code. The audit reveals real drift.

**Audit findings (already verified at workplan time)**:

- Canonical spec [`docs/specs/content-search.md`](../../docs/specs/content-search.md) § Data Schema § Request: `include_matches` default = **`false`**, `max_matches_per_file` default = **5**.
- Code at `src/api/types.rs:80-84`: `#[serde(default = "default_include_matches")]` where `default_include_matches() -> bool { true }`. Schema description: "Defaults to true."
- Code at `src/api/types.rs:85-89`: `max_matches_per_file: Option<usize>`, defaults to 5 in handler (`src/api/search.rs:129-131`). No drift here.

**Decision**: Correct the code (and schema description) to match the canonical spec — default `include_matches = false`. The intake explicitly authorizes bundling this correction into Step 17. The proposal calls the schema's "Defaults to true" comment "drift" by name.

**Work**:

- In `src/api/types.rs`:
  - Change `default_include_matches()` from `true` to `false`. (Or simplify to `#[serde(default)]` and let `bool` default to `false`; either is fine — the helper-fn approach was added precisely to advertise the default loudly, so keep the function for symmetry but flip its return.)
  - Update the `#[schemars(description = ...)]` for `include_matches` to read "Defaults to false" and to clarify that snippets are returned only when explicitly requested.
- Audit `src/search/content.rs` and the integration test fixtures (`tests/http.rs`, `src/api/tests.rs`) for any test that depends on `include_matches`'s implicit default. Update those tests to either pass `include_matches: true` explicitly (when they want match snippets) or drop the snippet assertion (when they want metadata-only).
- Confirm `max_matches_per_file` default and per-snippet text trim behavior already match the spec (verified in audit; no change expected).
- If the audit also surfaces drift in `max_matches_per_file` or per-snippet text byte-cap (unexpected, but verify), correct it under this task; otherwise, note "no other content-search drift found" in the per-task outcome.

**Files likely touched**:

- `src/api/types.rs`
- `src/api/tests.rs`
- `tests/http.rs`
- `tests/cli.rs` (if content-search CLI tests rely on the implicit default)
- `tests/mcp.rs`, `tests/mcp_http.rs` (if they assert on default content-search shape)

**Tests**:

- New (or updated): content search without `include_matches` returns `matches` absent / empty per the canonical spec.
- New: content search with `include_matches: true` returns `matches` populated, capped at `max_matches_per_file`.
- Existing CLI/MCP shape tests updated for the corrected default.

**Risk**: medium. This is the most visible behavior change in the step for non-semantic callers — any consumer that called content search expecting per-line matches "for free" will now get an empty `matches` array unless they opt in. The proposal flags this exact drift as the reason to do the audit; the fix is deliberate. Call it out in the CHANGELOG ritual at round-gate close.

### Task 17.7 -- Spec Amendment: `docs/specs/semantic-search.md`

**Purpose**: Make the canonical spec the authoritative description of the new contract. Per CLAUDE.md and the project policy in `notes/project-planning-workflow-notes.md`, the spec covers the full surface, not just what shipped this round.

**Work** — amend `docs/specs/semantic-search.md`:

- Bump version to `0.3.0`, update Date to `2026-05-01`, status remains Draft (or flips to Active per project-planning-workflow-notes.md spec-versioning policy — confirm with coordinator at build time).
- § Data Schema § Request:
  - Document `include_text` (`preview` | `full` | `none`, default `preview`).
  - Document `preview_bytes` (default 600, server max 2000, clamp behavior, applicability only when `include_text: "preview"`).
  - Confirm `limit` default is 10 (already correct in this section; cross-check the surrounding prose).
- § Data Schema § Response:
  - Change `text` from required to conditional (present unless `include_text: "none"`).
  - Add `text_kind` (`preview` | `full`, conditional, present alongside `text`).
  - Add `text_truncated` (bool, conditional, present alongside `text`).
  - Add `content_hash` (`sha256:` string, required, projected from chunk metadata).
  - Confirm `truncated` is present at the envelope level (cross-check; this spec already requires it, so the only correction here is making sure the wire example reflects the now-implemented field).
- § Validation Rules:
  - Pin the `include_text must be one of preview, full, none` error message verbatim.
  - Pin the `preview_bytes must be greater than 0` error message verbatim.
  - Document the 2000-byte server max and the clamp policy for above-max values.
- § Examples:
  - Add or update the three examples (default preview, full-text, metadata-only) to match the proposal § Examples.
- § Edge Cases:
  - Add a "Preview boundary in multibyte UTF-8" subsection: byte-cap, UTF-8-safe, no paragraph/sentence heuristic in v0.
  - Note "Boilerplate-heavy chunks" — chunker does not strip code blocks; preview returns whatever is at the byte boundary.
- § Open Questions:
  - Remove the four resolved open questions (text-field strategy, preview_bytes max, content_hash inclusion, content-search drift) and note them in Revision History.
  - Keep the chunk/section-retrieval question (out of scope for this round, deferred to content-retrieval proposal).
- § Revision History:
  - Add `0.3.0 | 2026-05-01 | Round 8 / Step 17: limit default 10 (re-pinned), include_text + preview_bytes request fields, text_kind + text_truncated + content_hash response fields, content_hash projection.`

**Spec amendment guidance for the builder**: keep it surgical. Do not touch the cross-vault behavior section (round-3 / step-10 work), the score-conversion math, the empty-index hint table, or any section unrelated to payload budgeting. The diff should read as "payload budgeting added to existing canonical spec," not "spec re-architected."

**Also amend `docs/specs/content-search.md`** if Task 17.6 found drift beyond `include_matches` (audit-only otherwise; the spec already says default `false`, which is what we're aligning code to — no spec change needed if only the default flag drifted).

**Files likely touched**:

- `docs/specs/semantic-search.md`
- `docs/specs/content-search.md` (only if Task 17.6 surfaces additional drift beyond `include_matches`)

**Tests**: doc-only; verify with `rg 'Defaults to 100|DEFAULT_LIMIT == 100' src tests docs/specs` returning no semantic-survivors.

**Risk**: low. Spec edits are scoped and the contract is already pinned by code in tasks 17.1–17.6.

### Task 17.8 -- Verification, Negative Fingerprint Sweep, Manual Smoke

**Purpose**: Prove the round-8 shipping gate — defaults match canonical spec, tests pass, no stale "Defaults to 100" claims survive in semantic surfaces, manual smoke shows the three modes behave.

**Work**:

- Run the negative-fingerprint sweeps from the proposal § Implementation Notes:
  - `rg 'Defaults to 100' src/api docs/specs tests -g '*.rs' -g '*.md'` — verify no semantic-search hits remain (filesystem and content "Defaults to 100" hits are expected).
  - `rg 'DEFAULT_LIMIT' src/api/search.rs` — verify `DEFAULT_LIMIT` is still used for filesystem/content but not for semantic; `DEFAULT_SEMANTIC_LIMIT` exists.
  - `rg 'semantic_handler_default_limit_caps_at_default|DEFAULT_LIMIT == 100' src tests` — verify zero hits.
  - `rg 'struct SemanticSearchResponse' -n src/api/types.rs` — confirm one definition with `truncated: bool`.
- Run focused tests:
  - `cargo test -p hypomnema --lib api::tests`
  - `cargo test -p hypomnema --lib search::semantic`
  - `cargo test --test http`
  - `cargo test --test mcp`
  - `cargo test --test mcp_http`
- Run full quality gate:
  - `cargo fmt`
  - `cargo test`
  - `cargo clippy -- -D warnings`
  - `git diff --check`
- Manual smoke against a temp vault:
  1. Spawn `hmnd` against a vault with at least 12 chunks long enough to exceed 600 bytes each.
  2. `POST /search/semantic` with body `{"query": "..."}` (no other fields). Verify response carries exactly 10 results, `text_kind: "preview"`, `text_truncated: true` for long chunks, `text.len() <= 600` for those, `content_hash` populated, `truncated: true`.
  3. Repeat with `{"query": "...", "include_text": "full", "limit": 3}`. Verify 3 results, `text_kind: "full"`, full content, `truncated: false`.
  4. Repeat with `{"query": "...", "include_text": "none", "limit": 20}`. Verify text fields absent on every result; metadata still populated.
  5. Repeat with `{"query": "...", "include_text": "bogus"}`. Verify HTTP 400 with `code: "invalid_request"` and the canonical message.
  6. Repeat with `{"query": "...", "preview_bytes": 0}`. Verify HTTP 400.
  7. Repeat with `{"query": "...", "preview_bytes": 100000}`. Verify it succeeds and `text.len() <= 2000` on long chunks, `text_truncated: true`.
- Compare HTTP and MCP transport parity for at least one query (existing `mcp_http_*` parity tests should still pass without modification beyond the new fields).

**Files likely touched**: none (verification only).

**Tests**: full suite + manual smoke as above.

**Risk**: medium. The shipping gate is the integration story, not any individual task. If smoke surfaces a behavior gap, it routes to a small follow-up task before round close.

---

## Shipping Criteria Checklist

Copied from `notes/roadmap/roadmap-8.md` § Step 17. Annotations show pre-build resolution status.

- [ ] Semantic search defaults to `limit: 10`; filesystem and content search remain at 100. _[Task 17.1]_
- [ ] Semantic responses include `text`, `text_kind` (`preview` | `full`), and `text_truncated` (boolean) when `include_text` is `preview` or `full`. _[Task 17.5; field strategy resolved at D1 — keep `text` + add metadata]_
- [ ] `preview_bytes` parameter accepted (default 600, max server-configurable, clamped to [1, max]); preview text never invalid UTF-8. _[Tasks 17.4 + 17.5; max resolved at D2 — 2000 bytes]_
- [ ] `include_text: "none"` omits all text payload; `include_text: "full"` returns complete stored chunk text. _[Task 17.5]_
- [ ] Invalid `include_text` values return 400 `invalid_request`: `include_text must be one of preview, full, none`. _[Task 17.4]_
- [ ] HTTP and MCP schemas agree on new request/response fields and defaults. _[Tasks 17.4 + 17.5; schema test in 17.4]_
- [ ] `content_hash` is included in semantic search results. _[Task 17.3; resolved at D3 — yes, add now]_
- [ ] Negative fingerprint clean: `rg 'Defaults to 100|DEFAULT_LIMIT == 100' src/api tests docs/specs` returns no semantic-search survivors. _[Task 17.8]_
- [ ] All six intake stories pass acceptance criteria. _[Verified across tasks; see story-coverage map below]_
- [ ] Manual testing covers default (10-result, 600-byte preview), explicit large `limit`, and `include_text: "none"` modes. _[Task 17.8]_
- [ ] `docs/specs/semantic-search.md` canonically specifies the text-budget contract and wire shape. _[Task 17.7]_
- [ ] `cargo test` and `cargo clippy -- -D warnings` pass. _[Task 17.8]_

---

## Story Coverage Map

| Story | Task(s) | Notes |
|---|---|---|
| 1 — Correct Semantic Default Limit Drift | 17.1, 17.2 | Limit fix + `truncated` field add. |
| 2 — Return Bounded Semantic Preview Text by Default | 17.5 | Default preview at 600 bytes, UTF-8-safe truncation. |
| 3 — Allow Explicit Full-Text Semantic Results | 17.4, 17.5 | `include_text: "full"` opts into complete chunk text. |
| 4 — Allow Metadata-Only Semantic Candidate Lists | 17.4, 17.5 | `include_text: "none"` strips text fields. |
| 5 — Keep Search and Retrieval Boundaries Separate | 17.3, 17.7 | `content_hash` added; spec keeps content-retrieval separate. |
| 6 — Verify Content Search Budget Drift | 17.6 | Audit + correct `include_matches` default to `false`. |

---

## Risk Notes Per Task

- **17.1**: low. One constant change.
- **17.2**: medium. Wide compile-fix sweep across test fixtures that construct `SemanticSearchResponse`. No callers depended on `truncated` before, so the new field is additive on the wire.
- **17.3**: low. SQL projection + struct field; column already exists.
- **17.4**: low-medium. Validation surface is small; schema parity with MCP is the real risk and is covered by a schema property test.
- **17.5**: medium. Most behavior change here; `text` becomes `Option<String>` which is a wire-shape change for any external Rust consumer that deserializes into a struct with `text: String`. The proposal explicitly takes this hit. Document in the spec amendment so external callers know to expect optionality.
- **17.6**: medium. Most visible non-semantic behavior change in the step — content-search consumers who relied on the implicit `include_matches: true` default will see empty `matches` arrays unless they opt in. The proposal flags this as drift to correct, not as a bug to preserve. Call out in the round-gate CHANGELOG entry.
- **17.7**: low. Surgical spec edits.
- **17.8**: medium. Shipping gate is the manual smoke; if it fails, route to a focused follow-up before round close.

---

## Spec Amendment Guidance

`docs/specs/semantic-search.md` is the canonical doc; amend it directly per Task 17.7. Concrete diff outline:

1. Bump header to `Version: 0.3.0` / `Date: 2026-05-01`.
2. § Data Schema § Request — add `include_text` and `preview_bytes` rows to the request table; update YAML example to include them.
3. § Data Schema § Response — change `text` from required to conditional; add `text_kind`, `text_truncated`, `content_hash` rows; update YAML example.
4. § Validation Rules — add the three new validation messages verbatim and the 2000-byte clamp policy.
5. § Examples — add the three proposal examples (default preview, full-text, metadata-only).
6. § Edge Cases — add "Preview boundary in multibyte UTF-8" entry; note the chunker does not strip code blocks.
7. § Open Questions — remove the four resolved questions; keep the chunk/section-retrieval question.
8. § Revision History — append the `0.3.0` row.

`docs/specs/content-search.md` is touched **only if** Task 17.6's audit surfaces additional drift beyond `include_matches`'s default. The spec already says default `false`, which is what we are aligning code to; if `include_matches` is the only drift, the code change is sufficient — no spec edit needed.

---

## Non-Goals

- No writes to the watched vault.
- No chunker changes (no boilerplate stripping, no fenced-code-block special-casing).
- No new dependencies (preview-bytes truncation is pure-string work in the response path; no `spawn_blocking`, no new crates).
- No changes to the embedding path, the `chunks_vec` schema, or the kNN SQL beyond projecting one extra column.
- No content-retrieval surface (deferred per `notes/proposals/content-retrieval.md`).
- No CLI flag wiring for `--include-text` / `--preview-bytes` (deferred to backlog; CLI keeps server defaults this round).
- No HyDE, reranking, or hybrid search (round 9+ candidates).

---

## Definition of Done

- [ ] All shipping-criteria checkboxes in roadmap-8.md § Step 17 are checked.
- [ ] All six intake stories pass acceptance criteria.
- [ ] Negative-fingerprint sweep returns no semantic-search "Defaults to 100" or `DEFAULT_LIMIT == 100` hits.
- [ ] `cargo fmt`, `cargo test`, `cargo clippy -- -D warnings`, and `git diff --check` are clean.
- [ ] `docs/specs/semantic-search.md` v0.3.0 is the canonical spec; revision history records this round.
- [ ] Manual smoke covers default / explicit `full` / `none` modes against a real `hmnd`.
- [ ] CHANGELOG entry drafted for round 8 boundary (separate ritual; not blocking on this workplan but flagged for the coordinator).

---

## Build-Phase Notes for Coordinator

- Suggested batching:
  - Batch 1 (low-risk baseline): 17.1, 17.2, 17.3 in parallel — independent struct/SQL changes, easy to land together.
  - Batch 2 (validation + shaping): 17.4 then 17.5 sequentially — 17.5 depends on 17.4's helpers.
  - Batch 3 (audit): 17.6 alone — non-semantic, lowest cross-task contention.
  - Batch 4 (docs + verify): 17.7 then 17.8 sequentially.
- The researcher (this process) stays alive for the lifetime of the step. Consult requests welcome on:
  - UTF-8 boundary heuristic edge cases (Task 17.5).
  - MCP schema parity check if a builder hits a schema serialization mismatch (Task 17.4).
  - Content-search test fixture updates if Task 17.6's blast radius is wider than expected.
- Do not let any task quietly grow into chunker work, content-retrieval work, or CLI flag wiring. Those belong to later rounds and are explicitly out of scope.
