# Proposal Intake: FTS5 / BM25 Content Search

**Status**: Intake complete
**Date**: 2026-05-02
**Intake inputs**:

- `notes/proposals/fts5-bm25-content-search.md` — Primary proposal (Status: Draft, 2026-04-30) defining an additive `mode: "ranked"` for `search_content`
- `notes/proposals/fts5-bm25-content-search-stories.md` — Five user stories + cross-story guardrails
- `notes/qmd-comparison.md` — Background motivation (FTS5/BM25 retrieval-quality gap vs qmd)

---

## Summary

Add a third matching strategy — `mode: "ranked"` — to `search_content` so agents can ask token-shaped, relevance-ordered questions ("notes mentioning vector indexes and SQLite") and get the most relevant files first instead of a path-ascending grep dump. The new mode is **additive**, not a replacement: substring and regex retain today's grep-shaped semantics so quote verification keeps working. The implementation lands an external-content FTS5 virtual table (`files_fts`) inside each per-vault `index.sqlite`, kept in sync with `files` inside the existing `spawn_blocking` SQL boundary on every upsert/delete/reset/rescan. No new top-level crate is needed — FTS5 is built into the SQLite that `rusqlite`'s `bundled` feature already compiles. The mode parameter (`substring` | `regex` | `ranked`) supersedes the legacy boolean `regex` flag while remaining backward-compatible with existing requests.

## Source Inputs

| Source | Type | Role in intake |
|---|---|---|
| `notes/proposals/fts5-bm25-content-search.md` | proposal | primary — defines the request/response contract, schema, validation, edge cases, and integration touchpoints |
| `notes/proposals/fts5-bm25-content-search-stories.md` | stories | primary — five acceptance-criteria stories covering ranking, exact-search preservation, freshness, validation, cross-vault merge, plus three cross-story guardrails |
| `docs/specs/content-search.md` | spec | background — canonical content-search contract this proposal amends (substring/regex/`mode` interplay; cross-vault merge; partial-results envelope) |
| `docs/specs/vault-management.md` § Cross-Vault Search Semantics | spec | background — pinned cross-vault execution rules ranked mode must reuse (vault scope, ordering tie-break, partial-failure envelope, `vaults: []` rejection) |
| `docs/decisions/0004-three-search-modes-as-peers.md` | ADR | background — three-mode peer model; ranked stays *inside* `search_content`, not a fourth peer |
| `docs/decisions/0006-outbox-outside-watched-directory.md` | ADR | background — load-bearing: `files_fts` lives in per-vault `index.sqlite`, never the watched vault |
| `notes/qmd-comparison.md` | comparison note | background — confirms FTS5 ships with `rusqlite` bundled SQLite (no new crate) and frames the retrieval-quality motivation |
| `Cargo.toml` (`rusqlite = { … features = ["bundled", "load_extension"] }`) | manifest | background — FTS5 dependency already satisfied in tree |
| `src/search/content.rs` | code | background — current substring/regex matcher, `ContentQuery` shape, `spawn_blocking` boundary, negative-fingerprint targets named in the proposal |
| `src/store/schema.rs` | code | background — current migrations 0001–0004 (`files`, `chunks`, `chunks_vec`); a new migration 0005 will add `files_fts` and backfill from `files.content` |
| `src/indexer/mod.rs` (file upsert/delete paths around lines 533/540/556–586) | code | background — the existing transactional sites where `files_fts` maintenance must be co-located |
| `src/api/types.rs` § `ContentQueryJson`, `ContentSearchResponse`, `ContentResultJson` | code | background — wire shape that grows `mode`, `score`, `rank` |

## Candidate Outcomes

- **Outcome: Ranked lexical content search**
  - Source: Story 1 (Ranked lexical content search)
  - User-visible result: `POST /search/content` with `mode: "ranked"` returns files ordered by BM25 relevance; high-density topical files surface above thin matches; results carry `score` and `rank` fields.
  - Verification signal: Story 1 acceptance criteria — three-file fixture (`a.md` repeated terms, `b.md` one term, `c.md` none) returns `a.md` before `b.md` and omits `c.md`; `rank: 1` on `a.md`.

- **Outcome: Existing substring and regex contracts preserved**
  - Source: Story 2 (Exact content search remains available)
  - User-visible result: Existing `regex: false` and `regex: true` callers keep current behavior with no observable change. Substring still spans line boundaries; regex still rejects invalid Rust patterns with `invalid_regex`.
  - Verification signal: Story 2 acceptance criteria + the existing `content_*` tests in `src/search/content.rs` stay green; legacy `regex: true` round-trips through new `mode` interpretation as `mode: "regex"`.

- **Outcome: FTS index tracks file lifecycle in step with `files`**
  - Source: Story 3 (FTS index freshness follows file lifecycle)
  - User-visible result: Created/edited/deleted files appear/disappear from ranked results as soon as the indexer commits the change; `hmn vault reset --rebuild` followed by rescan rebuilds `files_fts` cleanly.
  - Verification signal: Story 3 acceptance criteria — unique-term query reflects create, edit-removal, delete, and reset/rebuild without daemon restart; integration test exercises the upsert→delete→reset chain.

- **Outcome: Explicit, fail-fast request validation for ranked mode**
  - Source: Story 4 (Request validation is explicit)
  - User-visible result: Conflicting flags (`case_sensitive: true` + `mode: "ranked"`, `regex: true` + `mode: <anything>`) return 400 `invalid_request` with a precise message; FTS-syntax-invalid queries return `invalid_query`, not `vault_search_failed`.
  - Verification signal: Story 4 acceptance criteria — four request shapes return the documented envelope and CLI exits non-zero with the structured error.

- **Outcome: Deterministic cross-vault ranked merge**
  - Source: Story 5 (Multi-vault ranked merge is deterministic)
  - User-visible result: Multi-vault ranked queries merge by score, then `path`, then `vault_id`; paused/errored vaults appear in `partial_results.skipped`/`failed`; unknown `vaults: [...]` entries fail in `partial_results.failed` while recognized vaults still run.
  - Verification signal: Story 5 acceptance criteria — five fixture configurations (active/active, tied scores, single-vault scope, paused vault, unknown vault name) match the documented order and partial-results envelope.

- **Outcome: Negative fingerprints prove ranked-mode SQL is on a different code path**
  - Source: Cross-story guardrails in `fts5-bm25-content-search-stories.md` + Implementation Notes in proposal
  - User-visible result: After implementation, `rg "SELECT path, content FROM files" src/search/content.rs` is no longer the only content-search query path, and `rg "ORDER BY path ASC" src/search/content.rs` is not the ordering used by `mode: "ranked"`.
  - Verification signal: Manual `rg` sweep at gate time, plus a code review that all FTS5/`bm25` references are inside `spawn_blocking` closures.

## Proposed Roadmap Shape

**Recommended phasing**: One step. The work is cohesive (additive mode + schema migration + indexer maintenance + cross-vault merge), each piece is load-bearing for the others (you cannot ship the new mode without the migration, and you cannot ship the migration without keeping it transactional with file upserts), and the proposal is already small enough to fit a single-step round in the round-7 / round-8 mold.

If splitting becomes necessary (e.g. if the orchestrator wants to ship the schema migration first and the API surface in a separate step), the natural seam is "migration + indexer maintenance" → "request/response surface + CLI/MCP wire". Both seams are listed in the deferred-decisions block below; default to a single step until told otherwise.

### Step N — FTS5 / BM25 Ranked Mode for Content Search

**Goal**:
Land an additive `mode: "ranked"` for `search_content` backed by an external-content FTS5 virtual table (`files_fts`) per vault, kept transactionally in sync with `files`, exposed identically over HTTP, stdio MCP, HTTP MCP, and the `hmn` CLI.

**Shipping criteria**:

- [ ] `ContentQueryJson` accepts `mode: "substring" | "regex" | "ranked"` (default `"substring"`); legacy `regex: true` is interpreted as `mode: "regex"` and is rejected with `invalid_request` if combined with any explicit `mode`.
- [ ] `ContentResultJson` includes `score: number` and `rank: integer` only for ranked-mode results; both fields are omitted for substring/regex modes.
- [ ] New migration adds `CREATE VIRTUAL TABLE files_fts USING fts5(path UNINDEXED, content, content='files', content_rowid='rowid', tokenize='porter unicode61');` and backfills it from existing `files` rows.
- [ ] Indexer upsert/delete paths in `src/indexer/mod.rs` (upsert at lines ~533/540, delete at lines ~556–586, reset path) maintain `files_fts` inside the same transaction that touches `files`. No path mutates one without the other.
- [ ] All FTS5 schema creation, FTS maintenance, and ranked query SQL runs inside `tokio::task::spawn_blocking`; r2d2 connections are acquired inside the closure.
- [ ] Ranked-mode request validation: empty query → `invalid_query`; FTS-syntax-invalid query → `invalid_query` (not `vault_search_failed`); `case_sensitive: true` with ranked mode → `invalid_request`; `regex: true` with `mode` other than `regex` → `invalid_request`.
- [ ] Cross-vault merge for ranked mode: per-vault candidates collected up to `limit`, then merged by `(score asc, path asc, vault_id asc)`, then truncated to global `limit`; `truncated`, paused/errored skipping, and `partial_results` envelope reuse the existing cross-vault contract from `vault-management.md`.
- [ ] `hmn search content "<query>" --mode ranked` works; legacy `--regex` (if present in the CLI) maps to `--mode regex`. No double-flagging accepted.
- [ ] HTTP, stdio MCP, and HTTP MCP all expose the same request/response shape; transport layers do not fork ranked-mode behavior.
- [ ] `docs/specs/content-search.md` updated to canonically describe `mode`, ranked semantics, the BM25 sign convention, the `score` / `rank` fields, the additional validation rules, and the additive amendment story.
- [ ] All five Story acceptance criteria pass (ranking, exact-search preservation, freshness, validation, cross-vault merge).
- [ ] All three cross-story guardrails verified manually at gate time.
- [ ] Negative fingerprint clean: `rg "SELECT path, content FROM files" src/search/content.rs` is not the only content-search query path; `rg "ORDER BY path ASC" src/search/content.rs` is not the ordering used by `mode: "ranked"`; `rg "files_fts|bm25" src | rg -v "spawn_blocking|schema|test"` reviewed by hand.
- [ ] `cargo test` and `cargo clippy -- -D warnings` pass; existing `content_*` tests in `src/search/content.rs` stay green.
- [ ] Manual testing covers: ranked default flow, tied-score determinism, prefix-scoped ranked, ranked + paused vault, FTS-syntax-invalid query, ranked default vs substring default behavior on the same query.

**Deferred decisions resolved in this step**:

- Decision: Backing-table strategy — external-content FTS5 vs contentless vs duplicated content
  - Source: Proposal § Implementation Notes
  - Why this step: Proposal recommends external-content keyed to `files.rowid` so the body lives only in `files.content`. Workplan-time investigation should confirm `files.rowid` is stable enough for external-content (today `files.path` is the primary key — the rowid is implicit and changes only on `VACUUM`, which the daemon doesn't run; verify against schema 0001/0002). If brittle, a contentless or duplicated-content FTS table is acceptable but **must** carry an explicit rebuild path.

- Decision: Tokenizer choice — `porter unicode61` vs plain `unicode61`
  - Source: Proposal § Open Questions, Story 1
  - Why this step: Proposal draft uses `porter unicode61` because qmd does and Markdown notes are mostly prose. Code-heavy vaults may prefer no stemming. Resolve at workplan time with a small fixture comparison; the chosen value is the spec-canonical default and lives in `docs/specs/content-search.md`. A schema field for per-vault tokenizer override is **out of scope** for this step.

- Decision: Whether ranked mode becomes the CLI default
  - Source: Proposal § Open Questions
  - Why this step: Substring stays default for `mode` and for the `hmn` CLI in this step. Flipping the default is a follow-up after dogfood data accumulates; revisiting the default is a behavior change that warrants its own change-log entry, not a quiet flip inside this step.

- Decision: Backward-compatibility envelope for `regex: true`
  - Source: Proposal § Validation Rules; Story 2 AC 3–4
  - Why this step: `regex: true` becomes shorthand for `mode: "regex"`. Sending both is rejected. Sending neither defaults to `mode: "substring"`. Existing tests in `src/search/content.rs` covering legacy regex behavior stay green; the wire shape change is purely additive.

- Decision: Whether to require `score` / `rank` to be present on substring/regex results too
  - Source: Proposal § Ranked Response (lists them as ranked-only)
  - Why this step: Keep them ranked-only so the substring/regex JSON shape is unchanged for current callers. Old clients that ignore unknown fields see no difference; new clients can branch on `mode` (echoed in response or implicit from the request).

- Decision: Migration shape for backfill
  - Source: Proposal § Edge Cases — FTS index drift
  - Why this step: Migration 0005 issues `INSERT INTO files_fts(rowid, path, content) SELECT rowid, path, content FROM files;` after the `CREATE VIRTUAL TABLE` so existing vaults rebuild from already-stored bodies — no rescan required. Reset / `hmn vault reset --rebuild` clears `files_fts` (and `files`) and lets the next walk repopulate both.

**New deps**:

- (none — FTS5 ships with `rusqlite`'s `bundled` feature, already in `Cargo.toml`. No new top-level crate; no MSRV cross-check needed.)

**Risk**: medium

Rationale: Lower than a typical schema-changing round because the new structure is purely additive and the failure mode is local (ranked queries return wrong rankings; substring/regex unaffected). Higher than payload-budget-style work because:

1. **Schema migration touches every existing vault.** Migration 0005 must succeed against vaults populated by migrations 0001–0004 already in the field. Backfill cost on large vaults is real but bounded — the SQL is `INSERT … SELECT FROM files` and runs once.
2. **External-content rowid coupling is brittle if mishandled.** External-content FTS5 tables require the backing `files` to keep `rowid` stable. The daemon doesn't `VACUUM`, so this should hold, but workplan investigation must confirm and either pin it or fall back to contentless/duplicated-content.
3. **Indexer upsert paths must update both tables atomically.** A bug that mutates one without the other produces silent ranking drift visible only via wrong query results. Negative-fingerprint sweep + a freshness integration test mitigate.
4. **Cross-vault merge ordering.** Multi-vault ranked merge is one of the easier places to get a tie-break wrong; the spec already pins `(score asc, path asc, vault_id asc)`. Story 5 has explicit fixtures.

No async/SQLite hazards beyond the standard `spawn_blocking` rule, which the proposal calls out and the existing search code follows. No new transports. No outbox interaction (round 6 retired it).

**Source coverage**:

- Story 1 (Ranked lexical content search): This step
- Story 2 (Exact content search remains available): This step
- Story 3 (FTS index freshness follows file lifecycle): This step
- Story 4 (Request validation is explicit): This step
- Story 5 (Multi-vault ranked merge is deterministic): This step
- Cross-story guardrail 1 (`spawn_blocking` discipline): This step (gate-time review)
- Cross-story guardrail 2 (negative fingerprint: substring SQL not the only path): This step
- Cross-story guardrail 3 (negative fingerprint: ranked mode not using `ORDER BY path ASC`): This step

---

## Coverage Map

| Source item | Proposed step | Status | Notes |
|---|---|---|---|
| Story 1 — Ranked lexical content search (4 AC) | Step N | planned | Core ranking outcome; fixture-driven test seeds the three-file scenario |
| Story 2 — Exact content search remains available (4 AC) | Step N | planned | Backward-compat work; existing tests in `src/search/content.rs` stay green |
| Story 3 — FTS index freshness follows file lifecycle (4 AC) | Step N | planned | Indexer upsert/delete + reset/rebuild integration tests |
| Story 4 — Request validation is explicit (4 AC) | Step N | planned | Validation lives in the API/MCP layer; producers emit `invalid_query` / `invalid_request` |
| Story 5 — Multi-vault ranked merge is deterministic (5 AC) | Step N | planned | Reuses `vault-management.md` cross-vault rules; tie-break ordering pinned in spec |
| Guardrail — `spawn_blocking` discipline | Step N | planned | Gate-time `rg` sweep on `files_fts|bm25` |
| Guardrail — `SELECT path, content FROM files` not the only path | Step N | planned | Negative fingerprint at gate time |
| Guardrail — ranked mode not ordered by `path ASC` | Step N | planned | Negative fingerprint at gate time |
| Proposal § Validation rules (mode enum, regex/mode conflict, case_sensitive ranked, FTS query syntax) | Step N | planned | Source of truth for Story 4 acceptance criteria |
| Proposal § Persisted FTS Table | Step N | planned | Migration 0005 + backfill `INSERT … SELECT` |
| Proposal § Integration Points — Store Schema | Step N | planned | `index.sqlite` only — confirms outside-vault rule from ADR-0006 |
| Proposal § Integration Points — Indexer | Step N | planned | Upsert/delete paths in `src/indexer/mod.rs` ~lines 533/540/556–586 |
| Proposal § Integration Points — Search API | Step N | planned | HTTP + stdio MCP + HTTP MCP wire-shape parity |
| Proposal § Integration Points — CLI | Step N | planned | `hmn search content … --mode ranked`; `--regex` legacy alias maps to `--mode regex` |
| Proposal § Edge Cases — Tokenized search is not exact search | Step N (spec) | planned | Spec text in `docs/specs/content-search.md` documents this explicitly |
| Proposal § Edge Cases — FTS index drift | Step N | planned | Migration backfill + transactional upsert guarantees freshness |
| Proposal § Edge Cases — Prefix-scoped ranked search | Step N | planned | Prefix range filter applied to FTS candidate set before per-vault truncation |
| Proposal § Edge Cases — SQLite score direction | Step N (spec) | planned | Spec documents BM25 sign convention; `rank` is the consumer-stable display field |
| Proposal § Open Question — ranked as CLI default | Deferred | deferred | Revisit after dogfood data accumulates; substring stays default |
| Proposal § Open Question — tokenizer (`porter unicode61` vs `unicode61`) | Step N (workplan-time deferred decision) | planned | Resolved at workplan time with a small fixture comparison |
| Proposal § Open Question — `search_filesystem` ranked path/heading search | Out-of-scope | out-of-scope | Filesystem search is not in this proposal's contract; future round |

---

## Deferred / Out-of-Scope Items

- Item: Per-vault tokenizer override
  - Source: Proposal § Open Questions (tokenizer choice)
  - Reason: This step pins one tokenizer for all vaults so the spec stays simple. A per-vault override would require a config-shaped surface (config field, migration, runtime read path), which is outside the proposal's contract.
  - Revisit trigger: A user surfaces a vault where the chosen tokenizer noticeably degrades retrieval (e.g. code-heavy vault with `porter` stemming).

- Item: Ranked search as CLI default
  - Source: Proposal § Open Questions
  - Reason: Default-flip is a behavior change that needs its own consideration after real ranked-mode usage patterns emerge. Substring stays default in this step.
  - Revisit trigger: A round of dogfood with ranked mode shows it's the more common ergonomic default.

- Item: Ranked path/heading search inside `search_filesystem`
  - Source: Proposal § Open Questions
  - Reason: Out of scope of this proposal's contract — filesystem search is a separate operation per ADR-0004 and would need its own FTS table, schema migration, and stories.
  - Revisit trigger: A future proposal explicitly extending ranked search beyond file bodies.

- Item: Frontmatter-only vs body-only match distinction
  - Source: `docs/specs/content-search.md` § Open Questions
  - Reason: Pre-existing question on the substring/regex side; ranked mode does not solve or worsen it. Out of scope.
  - Revisit trigger: A user surfaces a query case where frontmatter-vs-body distinction is load-bearing.

- Item: Pagination / cursor across N independent FTS indexes
  - Source: `docs/specs/content-search.md` § Open Questions, `docs/specs/vault-management.md` § Open Questions
  - Reason: Cross-vault pagination is already deferred for substring/regex; ranked mode reuses the same `truncated: bool` envelope and inherits the deferral.
  - Revisit trigger: A future cross-vault search pagination round (already on the backlog).

- Item: Streaming response shapes (chunked HTTP / SSE / NDJSON) for ranked mode
  - Source: `docs/specs/content-search.md` § Open Questions
  - Reason: Already deferred for the existing modes; ranked mode inherits the deferral.
  - Revisit trigger: High-vault-count deployments that need streaming.

- Item: Score normalization across vaults
  - Source: Proposal § Edge Cases — SQLite score direction (implicit)
  - Reason: BM25 scores are corpus-relative; a row from a 10-document vault is not directly comparable to a row from a 10000-document vault. The proposal's `(score asc, path asc, vault_id asc)` ordering is a deliberate choice to ship a deterministic merge without claiming cross-corpus score comparability. Documenting the limitation in the spec is enough; a normalization scheme is its own design problem.
  - Revisit trigger: A user reports that cross-vault ranked merge produces visibly bad orderings on real data.

- Item: A new ADR for the additive `mode` parameter
  - Source: This intake (LDS layer question)
  - Reason: ADR-0004 already pins three search modes as peers and explicitly leaves matcher choice inside `search_content` open. Ranked mode is an additive matcher inside one peer, not a fourth peer — a spec amendment in `docs/specs/content-search.md` is the right LDS layer. An ADR is only needed if the design expands beyond the additive `mode` shape (e.g. promoting ranked to a fourth peer; ranked across `search_filesystem`).
  - Revisit trigger: Workplan-time discovery that the additive shape doesn't survive contact with implementation.

---

## Open Questions

- Question: Is `files.rowid` stable enough to use as the external-content key for `files_fts`?
  - Why it matters: External-content FTS5 stores the body once (in `files.content`) and points back via `content_rowid`. If a future operation rewrites `files` rows (`VACUUM`, drop+recreate as part of a migration, manual repair) the FTS index goes stale silently.
  - Blocks roadmap? **No** — workplan-time deferred decision. Default to external-content with a documented rebuild path; if investigation finds the coupling brittle, fall back to contentless or duplicated-content (also documented in proposal § Implementation Notes).
  - Suggested owner: Task agent at workplan time.

- Question: What's the right tokenizer default — `porter unicode61` or `unicode61`?
  - Why it matters: Porter stemming improves prose-style query recall but conflates code identifiers (`searches` and `searched` would match `search`). Hypomnema vaults are mixed.
  - Blocks roadmap? **No** — workplan-time deferred decision. Resolve with a small fixture comparison and pin the choice in `docs/specs/content-search.md`.
  - Suggested owner: Task agent at workplan time.

- Question: How are `files_fts` rows kept in sync during schema migration 0005's backfill on large vaults?
  - Why it matters: A backfill `INSERT INTO files_fts SELECT FROM files` on a vault with thousands of rows runs synchronously inside the migration transaction. If the daemon needs to start serving search before the migration finishes, the FTS index is incomplete.
  - Blocks roadmap? **No** — workplan-time deferred decision. The existing migration framework (`apply_migrations` in `src/store/schema.rs`) runs all migrations to completion before the daemon serves traffic, so the backfill is implicitly synchronous with startup. Workplan can confirm this and call out worst-case backfill cost in the rationale.
  - Suggested owner: Task agent at workplan time.

- Question: Should the ranked response echo the resolved `mode` value, or rely on caller bookkeeping?
  - Why it matters: When `regex: true` is sent without `mode`, the resolved mode is `"regex"`. Callers may want to know which mode produced the response (e.g. for logging). The proposal does not currently include `mode` in the response.
  - Blocks roadmap? **No** — purely additive question. Default to **not** echoing it (keeps the response shape minimal and existing tests untouched); revisit if dogfood reveals the gap.
  - Suggested owner: Task agent at workplan time.

- Question: Does `hmn search content` currently accept `--regex`? If so, does its mapping to `--mode regex` need explicit deprecation or just silent translation?
  - Why it matters: `src/bin/hmn.rs::cmd_search_content` (lines 144–171 today) hard-codes `regex: false`, so the CLI has no `--regex` flag in the current shape. The proposal § Integration Points — CLI assumes `--regex` exists. This is a **factual mismatch in the proposal** that the workplan should resolve: either add `--regex` (legacy shorthand, mapped to `--mode regex`) at the same time as `--mode`, or skip `--regex` entirely and ship only `--mode`.
  - Blocks roadmap? **No** — workplan-time deferred decision. Recommend skipping `--regex` since it doesn't exist today; ship only `--mode {substring|regex|ranked}` (default `substring`) and let `regex: true` survive on the wire as the legacy alias.
  - Suggested owner: Task agent at workplan time. Flag for human review at workplan-write time so the proposal text is corrected if needed.

- Question: Is the workplan-time benchmark for "should ranked become the default later" planned for this step's retro, or deferred?
  - Why it matters: Proposal § Open Questions names the benchmark as workplan-time. It's small but easy to lose between the workplan and the retro.
  - Blocks roadmap? **No**. Recommend: defer the benchmark to a follow-up dogfood pass after the step ships, not before. Workplan should not conflate "implement ranked mode" with "decide whether to flip the default."
  - Suggested owner: Future dogfood/retro pass after this step ships.

---

## Recommendation

**Recommendation: Refine inputs first, then this proposal is a strong candidate for the next round.**

Rationale:

1. **Proposal and stories are well-formed and load-bearing in the same direction.** The contract (additive `mode`, FTS5 + `bm25`, external-content table, transactional upserts, cross-vault merge tie-break) is internally consistent, ADR-aligned (ADR-0004, ADR-0006), and matches what `notes/qmd-comparison.md` flagged as "borrow this from qmd; no architectural cost."

2. **The proposal contains one factual mismatch worth fixing before the workplan.** Proposal § Integration Points — CLI says "legacy `--regex` remains accepted and maps to `--mode regex`," but `src/bin/hmn.rs::cmd_search_content` does not currently expose `--regex`. The CLI should ship only `--mode` for content search; the wire-shape `regex: true` legacy alias is sufficient. This is a one-line correction to the proposal but worth catching before workplan-time.

3. **One low-risk wire-shape question is worth deciding before the workplan.** Whether `score` and `rank` should appear on substring/regex results too (recommendation: no — keep them ranked-only) is an easy reading-room decision that the proposal can explicitly state instead of leaving implicit.

4. **No new dependencies; no MSRV concern.** FTS5 is built into `rusqlite`'s bundled SQLite (`rusqlite = { … features = ["bundled", …] }` in `Cargo.toml`, confirmed against `notes/qmd-comparison.md`). No new top-level crate, no MSRV cross-check needed.

5. **Risk is medium, not low.** Schema migration touches every existing vault, external-content FTS5 has a real-but-contained correctness hazard around rowid coupling, and indexer maintenance must be transactional. None of these are blockers — they are workplan-time deferred decisions with explicit fallbacks (contentless / duplicated-content; rebuild path). But this is heavier than the round-7 (deps-only) or round-8 (response-shaping-only) shape.

6. **LDS layers touched**:
   - Spec amendment in `docs/specs/content-search.md` (canonical)
   - **No new ADR needed** — additive matcher inside an existing peer; ADR-0004 still holds
   - Code in `src/store/schema.rs` (migration 0005), `src/store/mod.rs` (if FTS-aware helpers), `src/indexer/mod.rs` (upsert/delete maintenance), `src/search/content.rs` (ranked query path), `src/api/types.rs` + `src/api/search.rs` (wire shape + validation), `src/mcp/server.rs` + `src/mcp/backend*.rs` (MCP parity), `src/bin/hmn.rs` (CLI surface)
   - Tests: extend `src/search/content.rs` `#[cfg(test)]` for ranked-mode unit tests; add integration tests covering freshness (Story 3) and cross-vault merge (Story 5); update `src/api/tests.rs` for validation-error shapes; update `src/mcp/server.rs` round-trip tests for ranked mode.

7. **Recommended next step**:
   - Apply the small-but-load-bearing corrections to `notes/proposals/fts5-bm25-content-search.md` (CLI `--regex` mismatch; explicit "ranked-only fields" note for `score`/`rank`).
   - On the next round-planning pass, **promote this proposal as a strong candidate** for a single-step round.
   - When the workplan is drafted, resolve the six workplan-time questions above (rowid stability, tokenizer choice, backfill timing, response `mode` echo, CLI `--regex` decision, default-flip benchmark) inside the workplan's deferred-decisions section.
   - If the orchestrator wants to ship a smaller round first, the natural seam is "migration + indexer maintenance" → "API + CLI surface," but a single-step shape is preferred unless there's a scheduling reason to split.

---

## Human Review Notes

(append review decisions here)
