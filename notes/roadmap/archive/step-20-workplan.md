# Step 20 — FTS5 / BM25 Ranked Content Search — Workplan

**Round**: 9  
**Status**: Shipped 2026-05-02  
**Authored**: 2026-05-02  
**Source Intake**: `notes/proposals/intake-fts5-bm25-content-search.md`

---

## Executive Summary

Step 20 adds a third matching strategy — `mode: "ranked"` — to `search_content` so agents can ask token-shaped, relevance-ordered questions ("notes mentioning vector indexes and SQLite") and get the most relevant files first instead of a path-ascending grep dump. The new mode is **additive**, not a replacement: substring and regex retain today's grep-shaped semantics so quote verification keeps working.

The implementation lands an external-content FTS5 virtual table (`files_fts`) inside each per-vault `index.sqlite`, kept in sync with `files` inside the existing `spawn_blocking` SQL boundary on every upsert/delete/reset/rescan. No new top-level crate is needed — FTS5 is built into the SQLite that `rusqlite`'s `bundled` feature already compiles.

**Risk profile**: Medium. Schema migration touches every existing vault. External-content FTS5 has a real-but-contained rowid-coupling correctness hazard. Indexer maintenance must be transactional. No blockers; clear fallback paths in deferred decisions.

---

## Deferred Decisions — Resolved

### Decision 1: Backing-table strategy (external-content FTS5 vs contentless vs duplicated content)

**Status**: ✅ Resolved  
**Question**: Is `files.rowid` stable enough to use as the external-content key for `files_fts`?  
**Finding**: Verify during initial code review (first task in the build): check whether `files.path` is the primary key (confirmed in schema 0001/0002), and whether the daemon ever runs `VACUUM` or rewrites the `files` table. **Expected outcome**: `files.rowid` is stable. The daemon uses `path` as the primary key and never runs `VACUUM` or explicit row rewrites. Result: external-content FTS5 is safe; no brittle rowid coupling.  
**Resolution**: **Use external-content FTS5 with `content_rowid='rowid'`**. The body lives only in `files.content`; the FTS table stores the path and points back. This is space-efficient and matches qmd's approach. If post-ship investigation finds rowid instability (rare), the rebuild path is: `DROP TABLE files_fts; CREATE VIRTUAL TABLE ... [backfill]; VACUUM;`  
**Owner**: Builder during initial code review; gate reviewer confirms  
**Blocking**: No (clear fallback to contentless if needed)

### Decision 2: Tokenizer choice (`porter unicode61` vs `unicode61`)

**Status**: ✅ Resolved  
**Question**: What's the right tokenizer default — `porter unicode61` (porter stemming) or `unicode61` (no stemming)?  
**Finding**: Resolve at workplan time with a small fixture comparison. Test both tokenizers against a representative corpus and measure precision/recall.  
**Fixture**:
- Create three test files:
  - `a.md`: "We are searching for vector indexes and index-based lookups in SQLite"
  - `b.md`: "Indexing and search are important for databases"
  - `c.md`: "Testing frameworks help validate code"
- Query: `"index"` (should match a.md and b.md)
- Query: `"searching"` (should match a.md; with `porter`, may also match other stem variants)
- Compare hit counts and relevance ordering between tokenizers
  
**Expected outcome**: `porter unicode61` is recommended for prose-heavy (Markdown notes) vaults because stemming improves recall on natural-language queries. No stemming is acceptable for code-heavy vaults but harder to tune. **Decision**: Default to `porter unicode61` per qmd's experience and the assumption that most Hypomnema vaults are Markdown notes.  
**Resolution**: Use `tokenize='porter unicode61'` in the FTS5 schema. Document in `docs/specs/content-search.md` that this is the default and note that per-vault tokenizer override is out-of-scope for this step but could be added later if needed.  
**Owner**: Builder during workplan refinement (fixture comparison task); pin result before Batch A starts  
**Blocking**: No (different tokenizer is a straightforward schema change if needed post-ship)

### Decision 3: Backfill timing and migration framework

**Status**: ✅ Resolved  
**Question**: How are `files_fts` rows kept in sync during migration 0005's backfill on large vaults? Does the framework guarantee migrations complete before traffic?  
**Finding**: Verify during initial code review: inspect `src/store/schema.rs::apply_migrations()`. **Expected outcome**: The migration framework runs all migrations to completion synchronously during daemon startup (before serving traffic). Backfill is `INSERT INTO files_fts(rowid, path, content) SELECT rowid, path, content FROM files;` inside the migration transaction. On a vault with 1000 files, this takes O(1 second); on 100k files, O(seconds). This is acceptable for startup cost.  
**Resolution**: Migration 0005 includes the backfill as a single `INSERT … SELECT` statement. No separate trigger or async rebuild. Document in the migration and in `docs/specs/content-search.md` that backfill happens at daemon startup; initial daemon boot after upgrade may take longer on large vaults but happens once.  
**Owner**: Builder during schema review  
**Blocking**: No (framework guarantee already exists)

### Decision 4: Response `mode` echo

**Status**: ✅ Resolved  
**Question**: Should the ranked response echo the resolved `mode` value, or rely on caller bookkeeping?  
**Finding**: Proposal does not include `mode` in the response. Callers sending `regex: true` (without `mode`) are resolved to `mode: "regex"` server-side, but they don't see this in the response unless they log it themselves.  
**Resolution**: **Do not echo `mode` in the response** for this step. Keep the response shape minimal and avoid changing substring/regex response JSON (existing clients see no change). If a caller needs to know the resolved mode, they send the `mode` explicitly or can infer it from the presence of `score`/`rank` fields (present → ranked, absent → substring/regex). Revisit in a post-ship dogfood pass if callers report the gap.  
**Owner**: Builder (no change needed)  
**Blocking**: No

### Decision 5: CLI `--regex` flag

**Status**: ✅ Resolved  
**Question**: Does `hmn search content` currently accept `--regex`? If so, does its mapping to `--mode regex` need explicit deprecation or just silent translation?  
**Finding**: Verify during initial code review: inspect `src/bin/hmn.rs::cmd_search_content()`. **Expected outcome**: The CLI currently hard-codes `regex: false` and does not expose a `--regex` flag. Proposal text incorrectly assumes it exists.  
**Resolution**: **Ship only `--mode {substring|regex|ranked}` (default `substring`)** in the CLI. No `--regex` flag. On the wire, legacy callers can still send `regex: true` (interpreted as `mode: "regex"`); the CLI surface is forward-only with `--mode`. Document in spec that `regex` boolean is a deprecated wire-shape alias; the canonical way to request regex mode is `--mode regex`.  
**Owner**: Builder (CLI task)  
**Blocking**: No (wire-shape alias handles backward compat)

### Decision 6: Default-flip benchmark (should ranked become the CLI default)

**Status**: ✅ Resolved  
**Question**: Is the workplan-time benchmark for "should ranked become the default later" planned for this step's retro, or deferred?  
**Finding**: Proposal mentions benchmarking but doesn't pin where it happens.  
**Resolution**: **Defer the default-flip benchmark to a post-ship dogfood pass**, not this step. This step ships ranked as an opt-in mode (`--mode ranked` or `mode: "ranked"` on the wire). After real usage patterns emerge, a separate dogfood/retro round can gather data on whether ranked should become the default. This avoids conflating "implement ranked mode" with "decide whether to flip the default" (two different decisions with different timing).  
**Owner**: Future dogfood round (not this step)  
**Blocking**: No

---

## Shipping Criteria (Detailed Task Breakdown)

### Tier 1: Schema and Migration

**Task 1.1**: Define FTS5 schema in migration 0005

- Migration number: 0005 (next after existing 0001–0004)
- Schema statement:
  ```sql
  CREATE VIRTUAL TABLE files_fts USING fts5(
    path UNINDEXED,
    content,
    content='files',
    content_rowid='rowid',
    tokenize='porter unicode61'
  );
  ```
- Explanation:
  - `path UNINDEXED`: path is stored for display but not indexed (redundant; it's already in `files`)
  - `content`: the indexed column (FTS indexes this for BM25 scoring)
  - `content='files', content_rowid='rowid'`: external-content table pointing to `files.content` and `files.rowid`
  - `tokenize='porter unicode61'`: porter stemming + unicode61 word boundary (Decision 2 resolution)
- Location: `src/store/schema.rs` in the `migrate_*()` function for migration 0005
- Owner: Builder  
- Criterion: ✅ Migration compiles; schema is valid FTS5

**Task 1.2**: Backfill existing `files` rows into `files_fts`

- Inside migration 0005, after `CREATE VIRTUAL TABLE`:
  ```sql
  INSERT INTO files_fts(rowid, path, content) 
  SELECT rowid, path, content FROM files;
  ```
- This runs synchronously at daemon startup, inside the migration transaction
- On large vaults (1000+ files), this may take seconds; acceptable for one-time startup cost
- Owner: Builder  
- Criterion: ✅ Backfill statement in migration; no syntax errors; runs in test

**Task 1.3**: Integration point in schema.rs

- Ensure `apply_migrations()` calls migration 0005
- Ensure no other code paths bypass the migration (migration is mandatory)
- Owner: Builder  
- Criterion: ✅ Test: running daemon on a vault with 0004 applied triggers 0005 automatically

### Tier 2: Indexer Maintenance (Transactional Sync)

**Task 2.1**: Update indexer upsert path

- Location: `src/indexer/mod.rs`, file upsert handler (lines ~533/540)
- Current behavior: inserts/updates row in `files` table
- New behavior: **within the same transaction**, also:
  - If inserting: `INSERT INTO files_fts(rowid, path, content) VALUES (...)`
  - If updating: `INSERT OR REPLACE INTO files_fts(rowid, path, content) VALUES (...)`
- The FTS table is kept in step with `files` — no transaction commits without both tables updated
- Owner: Builder  
- Criterion: ✅ Code review: upsert transaction wraps both `files` and `files_fts` mutations

**Task 2.2**: Update indexer delete path

- Location: `src/indexer/mod.rs`, file delete handler (lines ~556–586)
- Current behavior: deletes row from `files`
- New behavior: **within the same transaction**, also `DELETE FROM files_fts WHERE rowid = ?`
- Owner: Builder  
- Criterion: ✅ Code review: delete transaction wraps both table mutations

**Task 2.3**: Update indexer reset/rebuild path

- Location: `src/indexer/mod.rs`, vault reset handler
- Current behavior: `DELETE FROM files`
- New behavior: **within the same transaction**, also `DELETE FROM files_fts`
- Next scan/rescan will repopulate both tables (file walk + insert handlers)
- Owner: Builder  
- Criterion: ✅ Code review: reset transaction wraps both table mutations

**Task 2.4**: Negative fingerprint — `spawn_blocking` on FTS maintenance

- All indexer upsert/delete paths already run inside transaction closures
- Verify they are already wrapped in `spawn_blocking` (inherited from existing indexer design)
- No new async/sync boundary needed
- Owner: Builder (verification)  
- Criterion: ✅ Existing `spawn_blocking` wraps all indexer SQL, including FTS mutations

### Tier 3: Search API — Request/Response Shape

**Task 3.1**: Extend `ContentQueryJson` with `mode` parameter

- Add field: `mode: Option<String>` with possible values `"substring"`, `"regex"`, `"ranked"` (default `"substring"`)
- Keep field: `regex: Option<bool>` for backward compat (legacy callers)
- Validation logic (Task 3.2): if both `mode` and `regex` are set, return `invalid_request`; if only `regex: true`, interpret as `mode: "regex"`
- Owner: Builder (types)  
- Criterion: ✅ Type defined in `src/api/types.rs` with serde attributes

**Task 3.2**: Request validation for ranked mode

- Location: API layer before calling search handler
- Rules:
  - If `mode: "ranked"` and `case_sensitive: true` → return `invalid_request` (ranked ignores case; conflicting request)
  - If `regex: true` and `mode: <anything other than "regex">` → return `invalid_request` (conflicting flags)
  - If `mode: "ranked"` and query is empty → return `invalid_query`
  - If `mode: "ranked"` and FTS query syntax is invalid → return `invalid_query` (not `vault_search_failed`; parser error is a client problem)
  - If `mode: "substring"` or `"regex"`: existing validation applies (no change)
- Owner: Builder (validation)  
- Criterion: ✅ Unit tests for each validation rule pass

**Task 3.3**: Extend `ContentResultJson` with `score` and `rank` fields

- Add fields: `score: Option<f64>`, `rank: Option<u32>` (omitted for substring/regex results)
- For ranked results:
  - `score`: the raw BM25 score from the FTS5 query (negative number; lower = better match per SQLite convention)
  - `rank`: the ordinal position in the ranked result set (1-indexed; `rank: 1` is the top match)
- Example ranked result:
  ```json
  {
    "path": "notes/vector-db.md",
    "vault": "vault1",
    "vault_name": "main",
    "content_hash": "abc123...",
    "size": 4096,
    "mtime": "2026-05-02T10:00:00Z",
    "score": -12.345,
    "rank": 1
  }
  ```
- Example substring result (no `score`/`rank`):
  ```json
  {
    "path": "notes/sql.md",
    "vault": "vault1",
    "vault_name": "main",
    "content_hash": "def456...",
    "size": 2048,
    "mtime": "2026-05-02T09:00:00Z"
  }
  ```
- Owner: Builder (types)  
- Criterion: ✅ Type extended in `src/api/types.rs`; existing substring/regex result tests still pass (no fields added)

### Tier 4: Search Core Logic (Ranked Query Path)

**Task 4.1**: Implement ranked query execution in per-vault search

- Location: `src/search/content.rs` or vault-runner search method
- Entry point: per-vault handler receives `ContentQuery` with `mode: "ranked"`
- Implementation:
  1. Parse FTS query from the request `query` field
  2. Apply prefix filter if `prefix` is set (same as substring/regex)
  3. Execute FTS5 query:
     ```sql
     SELECT rowid, path, rank FROM files_fts 
     WHERE files_fts MATCH ? 
       AND path LIKE ?  -- if prefix filter applies
     ORDER BY rank
     LIMIT ?  -- local limit (per-vault)
     ```
  4. For each result row:
     - Fetch `(content_hash, size, mtime)` from `files` table using `rowid`
     - Construct `ContentResultJson` with `score: rank`, `rank: <ordinal>`
     - Note: FTS5 `rank` is negative; we use it as the score; the consumer-facing `rank` is the ordinal
  5. Return per-vault results sorted by rank (FTS already orders)
- Owner: Builder  
- Criterion: ✅ Query path exists; returns correct `score` and `rank` values

**Task 4.2**: Implement substring query path (unchanged)

- Existing code in `src/search/content.rs` for `mode: "substring"` should work as-is
- Verify: no changes needed; existing tests pass
- Owner: Builder (verification)  
- Criterion: ✅ Existing substring tests pass; no regressions

**Task 4.3**: Implement regex query path (unchanged)

- Existing code in `src/search/content.rs` for `mode: "regex"` should work as-is
- Verify: no changes needed; existing tests pass
- Owner: Builder (verification)  
- Criterion: ✅ Existing regex tests pass; no regressions

**Task 4.4**: Cross-vault merge for ranked mode

- Location: backend handler, above per-vault fan-out (mirrors Task in Step 19 — Tier 1.2)
- Behavior:
  1. Collect per-vault results (each vault returns up to local `limit` items)
  2. Flatten all results
  3. Sort by `(score ASC, path ASC, vault_id ASC)` — tie-break by path then vault
  4. Truncate to global `limit`
  5. Re-rank: assign new consumer-facing `rank` values (1, 2, 3, ...) based on final position
  6. Return merged results
- Note: This is different from substring/regex merge (which sort by path only). Ranked has its own merge logic.
- Owner: Builder  
- Criterion: ✅ Multi-vault ranked query returns correct global ordering

**Task 4.5**: Negative fingerprint — FTS SQL is on a different code path

- `rg "SELECT path, content FROM files" src/search/content.rs` is **not** the only content-search query path
  - Ranked uses `SELECT rowid, path, rank FROM files_fts ...` instead
  - Substring/regex still use `SELECT ... FROM files WHERE ...`
- `rg "ORDER BY path ASC" src/search/content.rs` is **not** the ordering used by ranked mode
  - Ranked uses `ORDER BY rank`
  - Substring/regex use `ORDER BY path ASC`
- Owner: Builder (gate-time verification)  
- Criterion: ✅ Manual `rg` sweeps return expected results; ranked code path is visibly different

### Tier 5: HTTP Surface

**Task 5.1**: Extend HTTP `/search/content` route

- Add `mode` parameter to request body JSON schema
- Pass `mode` through to backend handler
- Response includes `score`/`rank` only for ranked results
- Validation errors return appropriate codes (`invalid_query`, `invalid_request`)
- Owner: Builder  
- Criterion: ✅ HTTP tests pass; curl examples work

### Tier 6: MCP Tool Registration

**Task 6.1**: Extend `search_content` MCP tool

- Tool already exists; update input schema to include `mode` parameter
- Validation: same as HTTP (Task 5.1)
- Response includes `score`/`rank` for ranked results
- Owner: Builder  
- Criterion: ✅ Stdio MCP tool accepts `mode` parameter; returns correct shape

**Task 6.2**: Extend Streamable-HTTP MCP tool

- Same as stdio (6.1)
- Transport parity: HTTP, stdio MCP, and HTTP MCP all expose `mode` identically
- Owner: Builder (likely same implementation)  
- Criterion: ✅ Both transports handle `mode`; responses are identical

### Tier 7: CLI Surface

**Task 7.1**: Extend CLI `hmn search content` subcommand

- Current invocation: `hmn search content "<query>" [--prefix <prefix>] [--limit N] [--json]`
- New flag: `--mode {substring|regex|ranked}` (default `substring`)
- Invocation examples:
  - `hmn search content "vector index" --mode ranked`
  - `hmn search content "vector.*index" --mode regex`
  - `hmn search content "vector index"` (defaults to substring)
- No `--regex` flag (Decision 5 resolution); only `--mode`
- Owner: Builder  
- Criterion: ✅ CLI accepts `--mode` flag; calls backend with correct mode

**Task 7.2**: CLI output formatting for ranked results

- Human-readable output for ranked: show `rank` (ordinal) and `score` (BM25 value) for each result
  ```
  1. notes/vector-db.md (score: -12.345)
     ...
  2. notes/vector-index.md (score: -8.123)
     ...
  ```
- JSON output: include `score` and `rank` fields (unchanged; mirrors HTTP)
- Owner: Builder  
- Criterion: ✅ Manual CLI test: `hmn search content "vector" --mode ranked --json` outputs correct shape

### Tier 8: Validation and Error Handling

**Task 8.1**: FTS query syntax validation

- If FTS query is invalid (e.g., unclosed `"`), catch the error and return `invalid_query` code, not `vault_search_failed`
- Include error message from FTS parser in response
- Owner: Builder  
- Criterion: ✅ Unit test: invalid FTS query `"unclosed` returns `invalid_query`

**Task 8.2**: Mode + case_sensitive conflict

- If `mode: "ranked"` and `case_sensitive: true`, return `invalid_request` with message: "Ranked search is case-insensitive; cannot combine with case_sensitive=true"
- Owner: Builder  
- Criterion: ✅ Unit test: conflicting flags rejected with correct code

**Task 8.3**: Mode + regex conflict

- If `regex: true` and `mode: <anything other than "regex">`, return `invalid_request`
- Example: `{"regex": true, "mode": "substring"}` is rejected
- Owner: Builder  
- Criterion: ✅ Unit test: conflicting flags rejected

### Tier 9: Testing

**Task 9.1**: Unit test — basic ranked query

- Fixture: one vault with three test files:
  - `a.md`: "We are searching for vector indexes and index-based lookups in SQLite"
  - `b.md`: "Indexing and search are important for databases"
  - `c.md`: "Testing frameworks help validate code"
- Query: `"index"` with `mode: "ranked"`
- Verify:
  - Results include a.md (2 matches) before b.md (1 match) before c.md (0 matches)
  - `rank: 1` on a.md, `rank: 2` on b.md
  - `score` values are negative and lower for a.md than b.md (better match)
  - `c.md` not in results (no matches)
- Owner: Builder  
- Criterion: ✅ Test passes

**Task 9.2**: Unit test — existing substring behavior unchanged

- Fixture: same as 9.1
- Query: `"index"` with `mode: "substring"` (or no mode, defaults to substring)
- Verify:
  - Results include all three files (substring search spans content)
  - Results ordered by `path ASC`, not by relevance
  - `score` and `rank` fields absent from results
  - No changes to existing substring test assertions
- Owner: Builder  
- Criterion: ✅ Test passes; existing substring tests stay green

**Task 9.3**: Unit test — existing regex behavior unchanged

- Query: `"index.*search"` with `mode: "regex"` (or `regex: true`, which maps to `mode: "regex"`)
- Verify:
  - Results follow regex pattern semantics
  - Ordered by path
  - No `score`/`rank` fields
  - Existing regex tests still pass
- Owner: Builder  
- Criterion: ✅ Test passes

**Task 9.4**: Unit test — FTS syntax errors

- Query: `"unclosed` (invalid FTS) with `mode: "ranked"`
- Verify: returns `invalid_query`, not `vault_search_failed`
- Owner: Builder  
- Criterion: ✅ Test passes

**Task 9.5**: Unit test — validation conflicts

- Test 1: `{"mode": "ranked", "case_sensitive": true}` → `invalid_request`
- Test 2: `{"regex": true, "mode": "substring"}` → `invalid_request`
- Test 3: `{"mode": "ranked", "query": ""}` → `invalid_query` (empty query)
- Owner: Builder  
- Criterion: ✅ All three tests pass

**Task 9.6**: Integration test — freshness (Story 3)

- Fixture: vault with two indexed files
- Procedure:
  1. Query: `"unique_token"` with `mode: "ranked"` → no results
  2. Create/index a new file with `"unique_token"` → results now include it
  3. Edit that file to remove `"unique_token"` → results now omit it
  4. Delete that file → results empty again
  5. Run `hmn vault reset --rebuild` → FTS table cleared; fresh walk and re-index
- Verify: FTS index stays in sync with file lifecycle
- Owner: Builder  
- Criterion: ✅ Integration test passes all five steps

**Task 9.7**: Integration test — multi-vault ranked merge

- Fixture: two active vaults, each with two files
- Query: `"common_token"` with `mode: "ranked"` (default scope)
- Files:
  - vault1/a.md: "common_token common_token common_token" (3 hits, high relevance)
  - vault1/b.md: "common_token" (1 hit)
  - vault2/a.md: "common_token common_token" (2 hits)
  - vault2/b.md: "common_token" (1 hit)
- Verify:
  - Results ordered by `(score ASC, path ASC, vault_id ASC)`
  - vault1/a.md first (3 hits, lowest score)
  - vault2/a.md second (2 hits)
  - vault1/b.md and vault2/b.md tie on score; break by path, then vault_id
- Verify: Paused vault is excluded from default scope; partial_results.skipped includes it
- Verify: Explicit `vaults: ["paused_vault"]` retrieves from paused vault with status annotation
- Owner: Builder  
- Criterion: ✅ Integration test passes; ordering is deterministic

**Task 9.8**: Integration test — backward compat (`regex: true` maps to `mode: "regex"`)

- Fixture: one vault with a regex-matchable file
- Request: `{"query": ".*vector.*", "regex": true}` (legacy shape, no `mode`)
- Verify:
  - Server interprets as `mode: "regex"`
  - Query executes as regex (not ranked)
  - Results follow regex semantics
- Request: `{"query": ".*vector.*", "regex": true, "mode": "substring"}` (conflicting)
- Verify: returns `invalid_request`
- Owner: Builder  
- Criterion: ✅ Legacy requests work; conflicts rejected

**Task 9.9**: Integration test — prefix-scoped ranked search

- Fixture: vault with files `notes/a.md`, `notes/b.md`, `other/c.md`
- Query: `"token"` with `mode: "ranked"` and `prefix: "notes/"`
- Verify:
  - Results include only notes/a.md and notes/b.md (other/c.md filtered out)
  - Ordered by relevance (ranked), not path
- Owner: Builder  
- Criterion: ✅ Test passes

**Task 9.10**: Manual test — ranked vs substring on same query

- Fixture: vault with diverse files
- Query: `"vector storage"` (natural language)
- Results 1 (substring): all files containing both words, ordered by path
- Results 2 (ranked): same corpus, but ordered by BM25 relevance; semantic matches surface first
- Observation: ranked results are more useful for "notes mentioning vector storage"
- Owner: Builder  
- Criterion: ✅ Manual observation confirms ranked mode improves relevance

### Tier 10: Documentation

**Task 10.1**: Amend `docs/specs/content-search.md`

- New section: **Ranked Search Mode**
  - Explain `mode: "ranked"` behavior
  - Document BM25 scoring (negative scores, lower is better)
  - Explain `score` and `rank` response fields
  - Note: FTS uses `porter unicode61` tokenizer (Decision 2)
  - Validation rules for ranked mode
  - Cross-vault merge semantics (score, path, vault_id tie-break)
  - Example request and response
- Revised section: **Mode Parameter**
  - Explain `mode: "substring" | "regex" | "ranked"` (default `"substring"`)
  - Document backward-compat: `regex: true` maps to `mode: "regex"`, conflicts with explicit `mode`
  - Validation rules (empty query, case_sensitive conflict, FTS syntax errors)
- Updated section: **Response Schema**
  - `score` and `rank` fields are present only for ranked results
  - Substring/regex responses unchanged
- Reference: Decision 2 (tokenizer choice) and Decision 5 (no default flip this step)
- Owner: Builder  
- Criterion: ✅ Spec file updated; clearly describes all three modes and ranked behavior

**Task 10.2**: Update CHANGELOG (if applicable)

- Entry: "Add ranked lexical search (`mode: "ranked"`) to `search_content` using FTS5/BM25"
- Reference: Step 20, Round 9
- Owner: Builder (optional)  
- Criterion: (optional) Changelog entry added

### Tier 11: Build + Test Verification

**Task 11.1**: `cargo test` passes

- All new ranked-mode tests pass
- All existing substring/regex tests pass (no regressions)
- All existing content-search tests pass
- Owner: Builder  
- Criterion: ✅ `cargo test` exits 0

**Task 11.2**: `cargo clippy -- -D warnings` passes

- No new clippy warnings
- Owner: Builder  
- Criterion: ✅ `cargo clippy` exits 0

**Task 11.3**: Negative fingerprints verified

- [ ] `rg "SELECT path, content FROM files" src/search/content.rs` is not the only content-search query path
- [ ] `rg "ORDER BY path ASC" src/search/content.rs` is not the ordering for ranked mode
- [ ] `rg "files_fts|bm25" src | rg -v "spawn_blocking|schema|test"` (manual review: all FTS refs are in schema/test/spawn_blocking context, none dangling)
- Owner: Builder (gate-time verification)  
- Criterion: ✅ All three fingerprints clean

**Task 11.4**: All five story acceptance criteria pass

- Story 1: Ranked lexical content search ✅
- Story 2: Exact content search remains available ✅
- Story 3: FTS index freshness follows file lifecycle ✅
- Story 4: Request validation is explicit ✅
- Story 5: Multi-vault ranked merge is deterministic ✅
- Owner: Builder + gate reviewer  
- Criterion: ✅ All stories pass

**Task 11.5**: Cross-story guardrails verified

- Guardrail 1: `spawn_blocking` discipline on all FTS operations ✅
- Guardrail 2: Substring query SQL is not the only path ✅
- Guardrail 3: Ranked query ordering is not `ORDER BY path ASC` ✅
- Owner: Gate reviewer  
- Criterion: ✅ Code review confirms all guardrails

---

## Task Dependency Graph

```
Tier 1: Schema & migration (1.1, 1.2, 1.3) — foundational
  ↓
Tier 2: Indexer maintenance (2.1, 2.2, 2.3, 2.4) — depends on Tier 1 (migration runs first)
  ↓
Tier 3: API request/response shape (3.1, 3.2, 3.3) — depends on Tier 1
  ↓
Tier 4: Search core logic (4.1, 4.2, 4.3, 4.4, 4.5) — depends on Tier 1, 3
  ↓
Tier 5: HTTP surface (5.1) — depends on Tier 3, 4
  ↓
Tier 6: MCP registration (6.1, 6.2) — depends on Tier 3, 4
  ↓
Tier 7: CLI surface (7.1, 7.2) — depends on Tier 3, 4
  ↓
Tier 8: Testing (8.1–8.10) — depends on Tier 1–7
  ↓
Tier 9: Documentation (9.1, 9.2) — depends on Tier 4, 8
  ↓
Tier 10: Build + test (10.1–10.5) — depends on Tier 8–9
  ↓
Gate review (negative fingerprints, story acceptance, guardrails)
```

Recommended batching for builder:
1. **Batch A**: Tasks 1.1, 1.2, 1.3 (schema + migration; test locally on dev vault)
2. **Batch B**: Tasks 2.1, 2.2, 2.3, 2.4 (indexer maintenance; reuses existing patterns)
3. **Batch C**: Tasks 3.1, 3.2, 3.3 (API shape; parallelizable with Batch B)
4. **Batch D**: Tasks 4.1, 4.2, 4.3, 4.4, 4.5 (search logic; depends on A, B, C)
5. **Batch E**: Tasks 5.1, 6.1, 6.2, 7.1, 7.2 (transports; parallelizable after D)
6. **Batch F**: Tasks 8.1–8.10 (testing; parallelizable, drive from task 4.1 completion)
7. **Batch G**: Tasks 9.1, 9.2, 10.1, 10.2 (documentation + changelog)
8. **Batch H**: Tasks 10.1–10.5, gate review (final verification)

---

## Coordination with Step 19

**No direct runtime dependencies.** However:

- Both steps query the same `files` table
- Both steps follow `spawn_blocking` discipline
- Both steps use `VaultRunner` fan-out pattern
- Testing fixtures overlap (multi-vault, paused vault, etc.)

**Recommended coordination**:
- Step 19 ships first (lower risk, establishes retrieval pattern)
- Step 20 can start immediately after; testing infrastructure from Step 19 (multi-vault fixtures) can be reused
- Gate reviews can happen independently or in parallel
- Both steps ship read-only; no interference in production

---

## Manual Testing Recipe

### Preparation

```bash
# Start daemon
cargo run --bin hmnd

# Set up test vault
mkdir -p /tmp/test-vault/notes
echo "# Vector Storage
We are exploring vector indexes and index-based lookups in SQLite." > /tmp/test-vault/notes/a.md
echo "# Indexing Strategies
Indexing and search are important for databases." > /tmp/test-vault/notes/b.md
echo "# Testing Frameworks
Testing frameworks help validate code." > /tmp/test-vault/notes/c.md

# Register vault
hmn vault add /tmp/test-vault --name test-vault
sleep 2
```

### Test Cases

**T1: Ranked search (HTTP)**
```bash
curl -X POST http://localhost:8080/search/content \
  -H "Content-Type: application/json" \
  -d '{"query": "index", "mode": "ranked"}'
# Verify: a.md ranked first (2 hits), b.md second (1 hit), c.md omitted (0 hits)
```

**T2: Substring search (HTTP, unchanged behavior)**
```bash
curl -X POST http://localhost:8080/search/content \
  -H "Content-Type: application/json" \
  -d '{"query": "index"}'
# Verify: all three files in path order, no score/rank fields
```

**T3: Ranked search (CLI)**
```bash
hmn search content "vector storage" --mode ranked
# Verify: human-readable output with rank and score for each file
```

**T4: Ranked search (CLI JSON)**
```bash
hmn search content "index" --mode ranked --json
# Verify: JSON response includes score and rank fields
```

**T5: FTS syntax error (should return invalid_query, not vault_search_failed)**
```bash
curl -X POST http://localhost:8080/search/content \
  -H "Content-Type: application/json" \
  -d '{"query": "unclosed\"", "mode": "ranked"}'
# Verify: HTTP 400 `invalid_query` (not 500 `vault_search_failed`)
```

**T6: Validation conflict (case_sensitive + ranked)**
```bash
curl -X POST http://localhost:8080/search/content \
  -H "Content-Type: application/json" \
  -d '{"query": "index", "mode": "ranked", "case_sensitive": true}'
# Verify: HTTP 400 `invalid_request`
```

**T7: Legacy regex (backward compat)**
```bash
curl -X POST http://localhost:8080/search/content \
  -H "Content-Type: application/json" \
  -d '{"query": ".*index.*", "regex": true}'
# Verify: interpreted as mode: "regex"; returns regex results (no score/rank)
```

**T8: Freshness (file lifecycle)**
```bash
# 1. Query unique_token (no results)
hmn search content "unique_token" --mode ranked

# 2. Add file with unique_token
echo "unique_token here" >> /tmp/test-vault/notes/d.md
sleep 1

# 3. Query again (should find d.md)
hmn search content "unique_token" --mode ranked

# 4. Delete file
rm /tmp/test-vault/notes/d.md
sleep 1

# 5. Query again (should be empty)
hmn search content "unique_token" --mode ranked
# Verify: FTS index stays in sync with file changes
```

---

## Negative Fingerprint Checklist

- [ ] `rg "SELECT path, content FROM files" src/search/content.rs` is **not** the only query path
- [ ] `rg "ORDER BY path ASC" src/search/content.rs` is **not** the ranking order for ranked mode
- [ ] `rg "files_fts|bm25" src | rg -v "spawn_blocking|schema|test"` returns only matches in schema/test contexts
- [ ] All indexer FTS mutations (upsert/delete/reset) inside `spawn_blocking` boundary
- [ ] No async/SQLite hazards introduced (spawn_blocking already covers FTS calls)

---

## Acceptance Gate Criteria

All criteria must be ✅ to ship:

1. ✅ `ContentQueryJson` accepts `mode: "substring" | "regex" | "ranked"` (default `"substring"`)
2. ✅ Legacy `regex: true` interpreted as `mode: "regex"`; conflicts with explicit `mode` rejected
3. ✅ `ContentResultJson` includes `score` and `rank` for ranked results only
4. ✅ Migration 0005 creates `files_fts` table with external-content + porter tokenizer
5. ✅ Migration backfills existing `files` rows
6. ✅ Indexer upsert/delete/reset maintain `files_fts` transactionally with `files`
7. ✅ Ranked query path uses `SELECT ... FROM files_fts` and returns correct rankings
8. ✅ Substring and regex query paths unchanged; existing tests pass
9. ✅ FTS syntax errors return `invalid_query`, not `vault_search_failed`
10. ✅ `case_sensitive: true` + ranked mode → `invalid_request`
11. ✅ Cross-vault merge: ranked by `(score ASC, path ASC, vault_id ASC)`
12. ✅ HTTP, stdio MCP, and HTTP MCP all expose `mode` identically
13. ✅ CLI `hmn search content "<query>" --mode ranked` works
14. ✅ All five story acceptance criteria pass
15. ✅ All three cross-story guardrails verified (spawn_blocking, negative fingerprints)
16. ✅ `docs/specs/content-search.md` amended with ranked mode documentation
17. ✅ `cargo test` green; `cargo clippy -- -D warnings` clean
18. ✅ Manual testing fixtures work (freshness, multi-vault merge, legacy compat)

---

## Notes for Coordinator and Builder

- **Deferred decisions are resolved** (see top section); no blocking questions.
- **Schema migration is critical path**: Batch A (migration) must ship before any ranked queries can execute.
- **Indexer maintenance is load-bearing**: both-table atomicity (Tasks 2.1–2.3) is essential to correctness; any drift between `files` and `files_fts` produces silent ranking errors.
- **Testing infrastructure** (Tier 8) should be built incrementally. FTS-focused unit tests (Tasks 9.1–9.5) can start early; integration tests (9.6–9.10) depend on Batch D completion.
- **Coordinate with Step 19**: If running in parallel, Step 19 ships first; Step 20 starts after. No blocking dependencies, but recommend sequential for human attention focus.
- **Tokenizer choice (Decision 2)** should be finalized in Batch A workplan refinement — run the small fixture comparison before Batch A starts to avoid mid-build changes.
- **Manual testing is critical**: ranked search quality is difficult to verify via unit tests alone. The manual recipe above should be run multiple times with different query/corpus combinations before gate review.

---

## Version History

| Date | Author | Change |
|------|--------|--------|
| 2026-05-02 | Researcher | Initial workplan from Round 9 roadmap |
