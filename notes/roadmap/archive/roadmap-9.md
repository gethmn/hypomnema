# Hypomnema Roadmap — Round 9: Search Completion and Content Retrieval

**Scope**: Ship two independent but complementary features: (1) content retrieval after search, and (2) ranked lexical content search. Both extend read-only search/discovery surfaces. Two focused steps.

**Status**: Shipped. Steps 19 and 20 both archived (`notes/roadmap/archive/step-19-workplan.md`, `notes/roadmap/archive/step-20-workplan.md`). Round-close archival of this file was missed at the time and applied during round-11 cleanup (2026-05-03).

**Process**: Two parallel steps. Coordinator + researcher + ephemeral builders per step. Same playbook as rounds 1–8. See [`notes/playbook/`](../playbook/) for the orchestration contract.

**Intakes**:
- [`notes/proposals/intake-content-retrieval.md`](../proposals/intake-content-retrieval.md) — Complete
- [`notes/proposals/intake-fts5-bm25-content-search.md`](../proposals/intake-fts5-bm25-content-search.md) — Complete

**Why this round**:

- Round 8 landed search-result payload budget (`content_hash`, `preview_bytes`) and explicitly deferred full-file content retrieval. Round 9's Step 19 completes that consumer flow: agents can now search, get a `(path, content_hash)` pair, and immediately call `content_get` for the full indexed text.
- Ranked content search (`mode: "ranked"` for `search_content`) addresses a real ergonomic gap: substring grep and regex work for quote verification but miss topical queries ("notes mentioning vector storage and SQLite") where relevance ranking beats path order. Step 20 lands FTS5/BM25 as an additive third strategy inside the existing `search_content` operation.
- **Independence**: Both steps are orthogonal. Retrieval doesn't depend on ranked search; ranked search doesn't depend on retrieval. They can ship separately if needed, but pair well: both extend read-only discovery and tell a coherent "search and retrieve reliably" story.
- **Risk is bounded**: Retrieval is low-risk (reuses `search_content` fan-out patterns, no schema changes). Ranked search is medium-risk (schema migration + FTS5 table sync, but additive with clear fallback to contentless FTS if rowid coupling proves brittle).
- **No blocking questions**: Both intakes' deferred decisions are scoped to workplan-time with clear owners. No cross-step dependencies.

**Skills carrying forward**:
- `rusqlite-in-async` (both steps use `spawn_blocking` pattern; Step 20 adds FTS5 SQL inside that boundary)
- `filesystem-watching` (no changes, but relevant context for understanding the stable `files` table both steps read from)

**New deps**: None. FTS5 is bundled with `rusqlite`'s `bundled` feature (already in `Cargo.toml`).

---

## Phasing

Two independent steps, can start in parallel:

| Step | Contents | Risk | Deferred Decisions |
| ---- | -------- | ---- | ---- |
| 19 | Content retrieval after search (`content_get` HTTP/MCP/CLI) | Low | 4 (UTF-8 lossy decode docs, `content_not_indexed` code, symlink handling, doc amendment) |
| 20 | Ranked lexical content search (`mode: "ranked"` with FTS5/BM25) | Medium | 6 (rowid stability, tokenizer choice, backfill timing, response `mode` echo, CLI `--regex`, default-flip benchmark) |

---

## Step 19 — Content Retrieval (`content_get`)

**Status**: Shipped 2026-05-02

**Goal**: Add a read-only content retrieval operation that fetches indexed file text by vault-relative path. Fans out across vaults using the same partial-results conventions as content-search. Exposes uniform `content_get` surface across HTTP, MCP (both transports), and CLI.

**Shipping criteria** (summarized from intake; full list in `step-19-workplan.md`):

- `HypomnemaBackend::content_get` trait method implemented once; both stdio and Streamable-HTTP MCP transports inherit.
- HTTP route `POST /content/get` with request envelope (`paths: [string]`, optional `vaults: [string]`) returns response envelope (`results: [...]`, optional `partial_results`).
- MCP tool `content_get` registered on both transports. Read-only; not gated by `[mcp] enable_write_tools`.
- CLI `hmn content get PATH... [--vault NAME|ID] [--json]` with human-readable + JSON modes.
- Result items: `path`, `content`, `content_hash`, `size`, `mtime`, `vault`, `vault_name` on success; per-item `error.code` + `error.message` on failure.
- Per-item errors do not abort batch; HTTP 200 even if all items `path_not_found`.
- Validation: empty `paths` → 422; absolute path or `..` segments → 422; nonexistent vault → 404.
- Cross-vault fan-out reuses content-search pattern. Path collisions yield separate result items per vault.
- Result ordering: `(path ASC, vault_id ASC)`.
- Paused/errored vault behavior: silently skipped from default scope; served when explicitly named with `partial_results.skipped` entry.
- All rusqlite access wrapped in `tokio::task::spawn_blocking`.
- Negative fingerprint: `rg 'fs::read|tokio::fs::read|File::open' src/api src/search` returns no matches in content-retrieval handler path.
- All nine intake stories pass acceptance criteria.
- `docs/specs/content-retrieval.md` published (promoted from proposal).
- `docs/specs/mcp-streamable-http.md` tool surface table amended to include `content_get`.
- `cargo test` green; `cargo clippy -- -D warnings` clean.
- Manual-testing fixture exercised.

**Deferred decisions to resolve at workplan-time**:

1. **Response content encoding documentation**: Document lossy UTF-8 decode behavior in spec § Implementation Notes (intake recommendation: yes).
2. **`content_not_indexed` vs `path_not_found` distinction**: Verify indexer behavior — is `content` ever NULL/empty? Mint separate code only if reachable.
3. **Symlink handling**: Confirm whether indexer stores symlink path or canonicalized path; document accordingly.
4. **`mcp-streamable-http.md` amendment**: Bundle as doc-only criterion (one-line table edit).

**Risk**: Low. Reuses established patterns from `search_content`. No new tables, no schema changes, no async/SQLite re-architecture.

**Coverage**: All nine intake stories map directly to this step.

---

## Step 20 — FTS5 / BM25 Ranked Content Search

**Status**: Shipped 2026-05-02

**Goal**: Add a third matching strategy (`mode: "ranked"`) to `search_content` backed by an external-content FTS5 virtual table (`files_fts`) per vault. Kept transactionally in sync with `files` on upsert/delete/reset. Exposed identically over HTTP, stdio MCP, HTTP MCP, and `hmn` CLI.

**Shipping criteria** (summarized from intake; full list in `step-20-workplan.md`):

- `ContentQueryJson` accepts `mode: "substring" | "regex" | "ranked"` (default `"substring"`); legacy `regex: true` interpreted as `mode: "regex"`.
- `ContentResultJson` includes `score: number` and `rank: integer` for ranked-mode results only.
- New migration 0005: `CREATE VIRTUAL TABLE files_fts USING fts5(path UNINDEXED, content, ...)` with backfill from `files`.
- Indexer upsert/delete paths maintain `files_fts` inside the same transaction that touches `files`.
- All FTS5 schema creation, maintenance, and ranked queries run inside `tokio::task::spawn_blocking`.
- Ranked-mode validation: empty query → `invalid_query`; FTS-syntax-invalid → `invalid_query`; `case_sensitive: true` with ranked → `invalid_request`; `regex: true` with explicit `mode` → `invalid_request`.
- Cross-vault merge for ranked mode: collect per-vault candidates, merge by `(score asc, path asc, vault_id asc)`, truncate to global `limit`. Reuses existing partial-results envelope.
- CLI: `hmn search content "<query>" --mode ranked` (no `--regex` flag in this step; legacy `regex: true` survives on wire).
- HTTP, stdio MCP, HTTP MCP expose same request/response shape.
- `docs/specs/content-search.md` updated to canonically describe `mode`, ranked semantics, BM25 sign convention, `score`/`rank` fields, validation rules.
- All five intake stories pass acceptance criteria.
- All three cross-story guardrails verified at gate time.
- Negative fingerprints: `rg "SELECT path, content FROM files" src/search/content.rs` is not the only content-search query path; `rg "ORDER BY path ASC" src/search/content.rs` is not the ordering for ranked mode; `rg "files_fts|bm25" src | rg -v "spawn_blocking|schema|test"` reviewed by hand.
- `cargo test` and `cargo clippy -- -D warnings` pass; existing `content_*` tests stay green.
- Manual testing covers: ranked default flow, tied-score determinism, prefix-scoped ranked, ranked + paused vault, FTS-syntax-invalid query, ranked vs substring behavior.

**Deferred decisions to resolve at workplan-time**:

1. **Backing-table strategy**: External-content FTS5 vs contentless vs duplicated content. Default to external-content; if `files.rowid` stability proves brittle, fallback to contentless with documented rebuild path.
2. **Tokenizer choice**: `porter unicode61` vs plain `unicode61`. Resolve with small fixture comparison; pin in spec.
3. **Backfill timing**: Confirm migration framework runs migrations to completion before daemon serves traffic.
4. **Response `mode` echo**: Whether to include resolved `mode` in ranked response. Default to no; revisit if dogfood reveals the gap.
5. **CLI `--regex` flag**: Proposal mentions it but current CLI doesn't expose `--regex`. Recommend shipping only `--mode` for content search; wire-shape `regex: true` legacy alias is sufficient.
6. **Default-flip benchmark**: Defer "should ranked become default later" to post-ship dogfood pass, not this step.

**Risk**: Medium. Schema migration touches every existing vault. External-content FTS5 has a real-but-contained rowid-coupling correctness hazard. Indexer maintenance must be transactional. No blockers; clear fallback paths in deferred decisions.

**Coverage**: All five intake stories + three cross-story guardrails map directly to this step.

---

## Step Sequencing

Both steps can start immediately after coordinator + researcher setup:

1. **Parallel research start**: Coordinator spawns researcher and requests initial workplan outline for both steps.
2. **Parallel build phases**: Two independent builder teams work on Steps 19 and 20 concurrently if desired, or sequentially if human prefers serialization.
3. **Independent shipping**: Step 19 can ship anytime after its criteria are met. Step 20 can ship independently. Coordination happens at gate time.

If the orchestrator prefers strict serialization (Step 19 first, then Step 20), that's also viable — the steps have zero runtime dependencies.

---

## Out of scope for round 9

These stay in [`notes/backlog.md`](../backlog.md) and are explicitly not part of this round:

- Chunk- or section-level retrieval (content retrieval is file-grain only; chunk retrieval deferred)
- Pagination or streaming for large `paths` batches (simple request size is v0 design)
- Conditional retrieval with `If-None-Match` (cache surface deferred)
- Vault-write surface for content (write-back deferred; v0 is read-only)
- Frontmatter parsing in retrieval response (structured frontmatter access deferred)
- Per-vault tokenizer override for ranked search (all vaults share one tokenizer in this step)
- Ranked search as CLI default (substring stays default; flip deferred to dogfood pass)
- Ranked path/heading search inside `search_filesystem` (filesystem search is separate; would need its own FTS table)
- Pagination / cursor across N independent FTS indexes (already deferred from earlier rounds)
- Streaming response shapes (chunked HTTP / SSE / NDJSON) for ranked mode (deferred from earlier rounds)
- Score normalization across vaults (deterministic merge by `(score, path, vault_id)` is sufficient for v0)
- New ADR for additive `mode` parameter (ADR-0004 already pins three search modes as peers; ranked is additive inside one peer)

---

## Recommended next actions after this roadmap is approved

1. Coordinator spawns researcher process and requests initial workplan outline for both steps (or both sequentially).
2. Researcher produces `step-19-workplan.md` and `step-20-workplan.md` with full task breakdown, deferred-decision resolutions, and testing strategy.
3. Coordinator reviews workplans for structure/completeness; surfaces for human review.
4. On human approval (`build/go/approved`), coordinator creates step context scratchpads and begins orchestrating builders per step.
5. Builder teams execute tasks per step workplan.
6. Gate reviews verify all shipping criteria + negative fingerprints + deferred decisions resolved.
7. Steps ship independently or in sequence per human direction.

---

## Notes on round-9 philosophy

This round demonstrates the coordination model working at scale: two independent intakes, both complete, synthesized into a unified roadmap with clear parallel work boundaries. Deferred decisions are explicit and scoped. No blocking questions. Both steps anchor to existing patterns (content-search fan-out, spawn_blocking discipline) and avoid introducing new abstractions. Shipping together tells a story: search better, retrieve reliably. Shipping separately is viable if human prefers. The choice is scheduling, not design.
