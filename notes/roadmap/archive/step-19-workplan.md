# Step 19 — Content Retrieval (`content_get`) — Workplan

**Round**: 9  
**Status**: Shipped 2026-05-02  
**Authored**: 2026-05-02  
**Source Intake**: `notes/proposals/intake-content-retrieval.md`

---

## Executive Summary

Step 19 ships a read-only content retrieval operation (`content_get`) that fetches indexed file text by vault-relative path. It is the natural consumer follow-on to the three search modes: search answers *which file*, content retrieval answers *give me this file*. The source of truth is the indexed `files.content` column in each per-vault `index.sqlite`; the operation never reads from the vault filesystem at query time, so an agent that searched, received a `content_hash`, and then retrieved sees the same state the search saw.

Surface is uniform across HTTP (`POST /content/get`), MCP (new read-only tool `content_get`), and CLI (`hmn content get`); cross-vault fan-out mirrors content-search semantics, including paused/errored vault handling and partial-result diagnostics. Read-only by definition — no v0 read/write boundary concerns.

**Risk profile**: Low. Reuses established patterns from `search_content`. No new tables, no schema changes, no async/SQLite re-architecture.

---

## Deferred Decisions — Resolved

### Decision 1: Response content encoding documentation

**Status**: ✅ Resolved  
**Question**: Should we document lossy UTF-8 decode behavior in the response spec?  
**Finding**: Yes. The `files.content` column is already stored with lossy UTF-8 substitution applied at index time (per `src/indexer/` file-reading code). The retrieval response returns that string verbatim. Consumers comparing retrieved bytes to on-disk bytes for non-UTF-8 files will see substitutions without explanation — this is a meaningful caveat.  
**Resolution**: Document the lossy-decode behavior in `docs/specs/content-retrieval.md` § Implementation Notes. Reference the corresponding line in `docs/specs/content-search.md` to keep wording consistent.  
**Owner**: Builder during implementation (doc authoring)  
**Blocking**: No

### Decision 2: `content_not_indexed` vs `path_not_found` distinction

**Status**: ✅ Resolved  
**Question**: Is the per-item failure mode `content_not_indexed` (row exists in `files`, but `content` is NULL/empty) actually reachable?  
**Finding**: Verify during initial code review (first 2 hours of the build): inspect `src/indexer/mod.rs` file-reading and row-insertion code. Confirm whether `content` can ever be NULL/empty when a `files` row exists. **Expected outcome**: The indexer treats missing/unreadable file content as a skip-and-continue at the walk level, never inserting an incomplete row. Result: `path_not_found` is the only per-item not-found code; `content_not_indexed` is dead schema.  
**Resolution**: If reachable (unlikely), mint a separate `content_not_indexed` code and document in spec. If unreachable, document the invariant ("content is always present when row exists") in `docs/specs/content-retrieval.md` § Implementation Notes.  
**Owner**: Builder during initial code review  
**Blocking**: No (does not prevent implementation; only affects error-code cardinality)

### Decision 3: Symlink handling

**Status**: ✅ Resolved  
**Question**: Does the indexer record `files.path` as the symlink path or as the canonicalized real path?  
**Finding**: Verify during initial code review: inspect `src/indexer/mod.rs` filesystem walker and path-recording code. Check whether `walkdir::WalkDir` (or equivalent) follows symlinks and what path is stored.  
**Expected outcome**: Hypomnema's filesystem walker follows symlinks within the vault root and stores the **symlink path verbatim** (not the real path). Result: consumers can retrieve by symlink path transparently.  
**Resolution**: If symlink path is stored, document transparently: "Symlinked files are retrieved by their symlink path within the vault." If real path is stored, document: "Symlinked files appear in retrieval only by their real path; query the canonicalized path if you followed a symlink during search."  
**Owner**: Builder during initial code review  
**Blocking**: No (does not prevent implementation; only affects retrieval key semantics)

### Decision 4: `mcp-streamable-http.md` tool surface amendment

**Status**: ✅ Resolved  
**Question**: Should we amend the MCP tool-surface table to include `content_get`?  
**Resolution**: Yes. This is a doc-only follow-up; bundle as a one-line table edit in `docs/specs/mcp-streamable-http.md`. Add `content_get` to the read-only tools table. (This is already a shipping criterion in the intake; confirmed here as unblocking.)  
**Owner**: Builder during documentation phase  
**Blocking**: No (purely doc; does not affect code)

---

## Shipping Criteria (Detailed Task Breakdown)

### Tier 1: Backend Trait and Core Logic

**Task 1.1**: Add `HypomnemaBackend::content_get` trait method

- Define trait method signature: `async fn content_get(&self, req: &ContentGetRequest) -> Result<ContentGetResponse>` (or equivalent)
- The signature mirrors search-handler trait methods (takes normalized request, returns response envelope)
- Implementation must be backend-independent: does not call HTTP, does not fork by transport
- Owner: Builder  
- Verification: Type-checks; method is called from HTTP, CLI, and both MCP shims with zero conditional logic
- Criterion: ✅ `impl HypomnemaBackend` includes `content_get` method

**Task 1.2**: Implement core content retrieval logic in the trait method

- Input: `ContentGetRequest { paths: Vec<String>, vaults: Option<Vec<String>> }`
- Validation tier: all `paths` vault-relative (no `/` prefix, no `..` segments); `vaults` nonempty if provided
- Fan-out: iterate over active vaults (or explicit `vaults` list); for each vault:
  - Acquire per-vault `Arc<VaultRunner>` and spawn into `tokio::task::spawn_blocking`
  - Inside the closure: call `vault.content_get(&paths)` (per-vault logic; see Task 1.3)
  - Collect results: `Vec<ContentGetResultItem>` per vault
- Cross-vault merge: flatten per-vault results; sort by `(path ASC, vault_id ASC)`
- Paused/errored vault handling:
  - Paused vault explicitly requested: serve from index; add entry to `partial_results.skipped` with `status: "paused"`
  - Errored vault explicitly requested: attempt to serve; if index unreadable, entry in `partial_results.failed` with error detail
  - Paused/errored vault in default scope: silently skip; add entry to `partial_results.skipped`
- Output: `ContentGetResponse { results: Vec<ContentGetResultItem>, partial_results: Option<PartialResults> }`
- Criterion: ✅ Full cross-vault fan-out with correct ordering and partial-results envelope

**Task 1.3**: Implement per-vault `content_get` inside `VaultRunner` (or equivalent)

- Input: `Vec<&str>` paths (vault-relative)
- SQL: `SELECT path, content, content_hash, size, mtime FROM files WHERE path IN (...)`
- Order results: `ORDER BY path ASC` (this is *per*-vault; global sort happens at Task 1.2 tier)
- Transform each row to `ContentGetResultItem { path, content, content_hash, size, mtime, vault, vault_name }`
- Per-item errors: if path not found, return `ContentGetResultItem::Error { code: "path_not_found", message: "..." }`
- All SQLite access inside `tokio::task::spawn_blocking`
- Criterion: ✅ Per-vault logic produces correct items + errors without any unwrap panics

### Tier 2: HTTP Surface

**Task 2.1**: Add HTTP route `POST /content/get`

- Route path: `/content/get`
- Request body: JSON envelope with `{ paths: [string], vaults?: [string] }`
- Response status: 200 OK (even if all items are `path_not_found`)
- Response body: JSON envelope `{ results: [...], partial_results?: {...} }`
- Validation errors:
  - `paths: []` → 422 `invalid_request`
  - `vaults: []` → 422 `invalid_request`
  - absolute path (e.g., `/etc/passwd`) or `..` segments → 422 `invalid_path`
  - nonexistent vault name → 404 `vault_not_found` (top-level, not per-item)
- All-vaults-failed case: if default scope and all active vaults errored, return 503 `vault_retrieval_failed` with detail in `partial_results.failed`
- Owner: Builder  
- Criterion: ✅ Route exists; curl tests pass (see testing section)

**Task 2.2**: Schema for HTTP types

- `ContentGetRequest { paths: Vec<String>, vaults: Option<Vec<String>> }`
- `ContentGetResultItem` (success): `{ path, content, content_hash, size, mtime, vault, vault_name }`
- `ContentGetResultItem` (error): `{ path, error: { code: string, message: string } }` (untagged enum or tagged Option)
- `ContentGetResponse { results: Vec<ContentGetResultItem>, partial_results: Option<PartialResults> }`
- `PartialResults` reuses vault-management shape: `{ skipped?: [...], failed?: [...], truncated?: bool }`
- Owner: Builder  
- Criterion: ✅ Types defined in `src/api/types.rs`; serde/validation attributes applied

### Tier 3: MCP Tool Registration

**Task 3.1**: Register `content_get` MCP tool on stdio transport

- Tool name: `content_get`
- Input schema: mirrors HTTP schema (JSON Schema auto-generated from type)
- Visibility: registered regardless of `[mcp] enable_write_tools` (it is read-only)
- Handler: calls the trait method; applies minimal error handling to conform to tool result shape
- Owner: Builder  
- Criterion: ✅ `hmn mcp --json` tool list includes `content_get`

**Task 3.2**: Register `content_get` MCP tool on Streamable-HTTP transport

- Same tool name, input schema, visibility, handler
- Implementation: the transport layer calls the shared trait method (no duplication)
- Verification: tool list from both transports includes `content_get` with identical schema
- Owner: Builder (likely reuses helper to register on both; minimal transport-specific code)  
- Criterion: ✅ Both transports register the tool; a client can discover it and call it

### Tier 4: CLI Surface

**Task 4.1**: Add CLI subcommand `hmn content get`

- Invocation: `hmn content get PATH... [--vault NAME|ID] [--json]`
- Positional args: one or more vault-relative paths
- Flags:
  - `--vault NAME|ID`: optional, repeatable (e.g., `--vault vault1 --vault vault2`); scopes retrieval to named vaults
  - `--json`: output full response envelope as JSON (default: human-readable format)
- Human-readable output: per file, print content with metadata header (separated by `---` for multi-file):
  ```
  PATH: notes/example.md
  VAULT: vault1
  HASH: abc123...
  SIZE: 4096
  MTIME: 2026-05-02T10:00:00Z
  ---
  [file content here]
  ```
- JSON output: response envelope verbatim
- Exit codes:
  - 0: partial success (at least one item succeeded)
  - 1: all items errored, request validation failed, or no vault reachable
  - (per-item errors printed to stderr; success items printed to stdout)
- Owner: Builder  
- Criterion: ✅ `hmn content get notes/file.md --json` works; `hmn content get notes/{a,b,c}.md` works

**Task 4.2**: Loopback HTTP call from CLI

- CLI implementation: constructs `ContentGetRequest`, POSTs to daemon `/content/get`, parses response, formats output
- Uses existing `DaemonClient` pattern from other CLI subcommands
- Error handling: HTTP errors (e.g., 404 vault not found) propagate to CLI exit code
- Owner: Builder  
- Criterion: ✅ CLI and HTTP surface accept same request; return consistent response (up to formatting)

### Tier 5: Request Validation

**Task 5.1**: Centralized path validation

- Location: API layer (before calling trait method)
- Rules:
  - Empty `paths` → reject (422 `invalid_request`)
  - Any path starting with `/` → reject (422 `invalid_path`)
  - Any path containing `..` segment (including internal `../`) → reject (422 `invalid_path`)
  - Paths normalized but not validated further (e.g., `./notes/file` is normalized to `notes/file`)
- Owner: Builder  
- Criterion: ✅ Curl test: `{"paths": ["/etc/passwd"]}` returns 422; `{"paths": ["../escape.md"]}` returns 422; `{"paths": ["notes/file.md"]}` succeeds

**Task 5.2**: Vault validation

- Empty `vaults` list → reject (422 `invalid_request`)
- Nonexistent vault name → reject (404 `vault_not_found`, not per-item error)
- Owner: Builder  
- Criterion: ✅ Curl test: `{"paths": ["file.md"], "vaults": ["nonexistent"]}` returns 404

### Tier 6: Ordering and Determinism

**Task 6.1**: Global cross-vault ordering

- Results sorted by `(path ASC, vault_id ASC)`
- Path collisions across vaults yield separate result items per vault
- Order is **deterministic across identical requests** (no request-input-order dependence, no random shuffling)
- Owner: Builder  
- Criterion: ✅ Repeated requests with same paths/vaults return results in same order

### Tier 7: Async/SQLite Compliance

**Task 7.1**: Spawn blocking for all SQLite access

- Every `VaultRunner::content_get` call wrapped in `tokio::task::spawn_blocking`
- Every fan-out spawn returns a future that the caller joins
- No direct rusqlite calls on the async runtime
- Owner: Builder  
- Verification: `rg 'spawn_blocking' src/search/content.rs` shows the pattern (now adapted for retrieval)
- Criterion: ✅ No runtime panics about "blocking operation on runtime"; tests pass

**Task 7.2**: Negative fingerprint — no filesystem reads in handler

- `rg 'fs::read|tokio::fs::read|File::open' src/api src/search` returns no matches in the content-retrieval handler path
- All text comes from `files.content` in the index; never from vault filesystem
- Owner: Builder (gate-time verification)  
- Criterion: ✅ Fingerprint sweep clean

### Tier 8: Testing

**Task 8.1**: Unit tests for path validation

- Test valid paths: `notes/file.md`, `a.md`, `deeply/nested/file.md`
- Test invalid paths: `/abs`, `../escape`, `notes/../escape.md`, empty string
- Owner: Builder  
- Criterion: ✅ Unit tests in `src/api/tests.rs` or inline `#[cfg(test)]`

**Task 8.2**: Integration test — single-file retrieval

- Fixture: one vault with one indexed file
- Request: `{"paths": ["notes/file.md"]}`
- Verify: response includes `content`, `content_hash`, `size`, `mtime`, `vault`, `vault_name`
- Owner: Builder  
- Criterion: ✅ Integration test passes

**Task 8.3**: Integration test — multi-file batch with mixed hits/misses

- Fixture: one vault with two indexed files
- Request: `{"paths": ["notes/a.md", "notes/missing.md", "notes/b.md"]}`
- Verify:
  - Response includes 3 items (2 success, 1 error with `path_not_found`)
  - HTTP status is 200
  - Items ordered by path ASC
- Owner: Builder  
- Criterion: ✅ Integration test passes

**Task 8.4**: Integration test — multi-vault fan-out

- Fixture: two active vaults, each with one indexed file at same path
- Request: `{"paths": ["shared/file.md"]}` (no vault scope)
- Verify:
  - Response includes 2 items (one per vault)
  - Both items succeed
  - Ordered by `(path, vault_id)`
- Owner: Builder  
- Criterion: ✅ Integration test passes

**Task 8.5**: Integration test — explicit vault scoping

- Fixture: two active vaults, each with indexed file
- Request: `{"paths": ["file.md"], "vaults": ["vault1"]}`
- Verify:
  - Response includes only vault1's item
  - HTTP status is 200
- Owner: Builder  
- Criterion: ✅ Integration test passes

**Task 8.6**: Integration test — paused vault retrieval

- Fixture: one active vault (vault1), one paused vault (vault2), both with indexed file
- Request 1: `{"paths": ["file.md"]}` (default scope)
- Verify:
  - Response includes only vault1 item
  - `partial_results.skipped` includes vault2 with `status: "paused"`
- Request 2: `{"paths": ["file.md"], "vaults": ["vault2"]}`
- Verify:
  - Response includes vault2 item (served from readable index)
  - `partial_results.skipped` includes vault2 with `status: "paused"`
- Owner: Builder  
- Criterion: ✅ Integration test passes both request shapes

**Task 8.7**: Transport parity test (HTTP ↔ MCP)

- Same request sent via HTTP `POST /content/get` and `content_get` MCP tool
- Verify responses are identical (JSON shape match)
- Owner: Builder  
- Criterion: ✅ Test passes; tools/HTTP return consistent results

**Task 8.8**: Manual-testing fixture

- Refresh `notes/manual-testing/` (or create new entry if missing) with `content_get` recipe
- Recipe includes:
  - Single-file curl example
  - Multi-file batch example
  - Default-vault fan-out example
  - Error case (missing file)
  - Error case (invalid path)
- Owner: Builder or docs lead  
- Criterion: ✅ Fixture file updated and verified

### Tier 9: Documentation

**Task 9.1**: Promote proposal to canonical spec

- Source: `notes/proposals/content-retrieval.md`
- Destination: `docs/specs/content-retrieval.md`
- Contents:
  - Overview (same as proposal)
  - Request/response schema (detailed)
  - Validation rules (detailed)
  - Cross-vault fan-out behavior
  - Paused/errored vault handling
  - **Implementation Notes** (new):
    - Lossy UTF-8 decode behavior (Task 1 decision)
    - `content_not_indexed` reachability (Task 2 decision)
    - Symlink path handling (Task 3 decision)
    - Invariant: source of truth is `files.content` in index; never reads vault filesystem
  - Error codes: `path_not_found`, `vault_not_found`, `vault_retrieval_failed`, `invalid_request`, `invalid_path`
  - Integration points (HTTP, MCP, CLI)
- Owner: Builder  
- Criterion: ✅ Spec file published; covers all decisions and edge cases

**Task 9.2**: Amend `mcp-streamable-http.md` tool table

- Add one row to the read-only tools table:
  - Tool: `content_get`
  - Description: "Fetch indexed file content by vault-relative path"
  - Availability: Available regardless of `enable_write_tools` setting (read-only)
- Owner: Builder  
- Criterion: ✅ Table updated; spec renders without errors

**Task 9.3**: Update `CHANGELOG.md` (if project maintains one)

- Entry summarizing: "Add `content_get` read-only operation for fetching indexed file content"
- Cross-references: Step 19, Round 9
- Owner: Builder (optional; skip if no changelog)  
- Criterion: (optional) Changelog entry added

### Tier 10: Build + Test Verification

**Task 10.1**: `cargo test` passes

- All new unit and integration tests pass
- Existing tests unaffected
- Owner: Builder  
- Criterion: ✅ `cargo test` exits 0

**Task 10.2**: `cargo clippy -- -D warnings` passes

- No new warnings introduced
- All new code follows project style
- Owner: Builder  
- Criterion: ✅ `cargo clippy` exits 0

**Task 10.3**: All nine acceptance stories verified

- Story 1: Single file by path ✅
- Story 2: Multiple files in one request ✅
- Story 3: Per-item errors don't abort ✅
- Story 4: Default fan-out to all active vaults ✅
- Story 5: Scope to named vault ✅
- Story 6: Explicit retrieval from paused vault ✅
- Story 7: CLI surface ✅
- Story 8: MCP tool surface (both transports) ✅
- Story 9: Reject invalid paths ✅
- Owner: Builder + gate reviewer  
- Criterion: ✅ All stories pass acceptance criteria

---

## Task Dependency Graph

```
Tier 1: Backend logic (1.1, 1.2, 1.3) — foundational
  ↓
Tier 2: HTTP surface (2.1, 2.2) — depends on Tier 1
  ↓
Tier 3: MCP registration (3.1, 3.2) — depends on Tier 1
  ↓
Tier 4: CLI surface (4.1, 4.2) — depends on Tier 1
  ↓
Tier 5: Validation (5.1, 5.2) — feeds into Tier 2, 3, 4
  ↓
Tier 6: Ordering (6.1) — part of Tier 1
  ↓
Tier 7: Async compliance (7.1, 7.2) — verification across Tier 1–4
  ↓
Tier 8: Testing (8.1–8.8) — depends on Tier 1–4
  ↓
Tier 9: Documentation (9.1–9.3) — depends on Tier 1–4
  ↓
Tier 10: Verification (10.1–10.3) — final gate
```

Recommended batching for builder:
1. **Batch A**: Tasks 1.1, 1.2, 1.3 (core logic)
2. **Batch B**: Tasks 2.1, 2.2, 3.1, 3.2 (HTTP + MCP; parallelizable)
3. **Batch C**: Tasks 4.1, 4.2 (CLI; depends on A)
4. **Batch D**: Tasks 5.1, 5.2 (validation; integrated into earlier batches)
5. **Batch E**: Tasks 6.1, 7.1, 7.2 (ordering + compliance; review-time)
6. **Batch F**: Tasks 8.1–8.8 (testing; parallelizable)
7. **Batch G**: Tasks 9.1–9.3, 10.1–10.3 (documentation + gate)

---

## Coordination with Step 20

**No direct runtime dependencies** on Step 20. However:

- Both steps query `files.content` from the per-vault index
- Both steps follow `spawn_blocking` discipline
- Both steps use `VaultRunner` fan-out pattern
- Recommend: Step 19 ships cleanly before Step 20 starts; Step 20 can reuse Step 19's testing infrastructure (multi-vault fixtures are similar)

**Sequencing recommendation**: Ship Step 19 first (lower risk, establishes content-retrieval pattern). Step 20 can start immediately after, as both are read-only and do not interfere.

---

## Manual Testing Recipe

### Preparation

```bash
# Start daemon
cargo run --bin hmnd

# In another terminal, set up a test vault with known files
mkdir -p /tmp/test-vault/notes
echo "# File A" > /tmp/test-vault/notes/a.md
echo "# File B" > /tmp/test-vault/notes/b.md

# Register vault via CLI or API
hmn vault add /tmp/test-vault --name test-vault

# Wait for indexing to complete
sleep 2
```

### Test Cases

**T1: Single file retrieval (HTTP)**
```bash
curl -X POST http://localhost:8080/content/get \
  -H "Content-Type: application/json" \
  -d '{"paths": ["notes/a.md"]}'
# Verify: HTTP 200, response includes `content`, `content_hash`, `size`, `mtime`
```

**T2: Multi-file batch (HTTP)**
```bash
curl -X POST http://localhost:8080/content/get \
  -H "Content-Type: application/json" \
  -d '{"paths": ["notes/a.md", "notes/missing.md", "notes/b.md"]}'
# Verify: HTTP 200, 3 items (2 success, 1 error with `path_not_found`), ordered by path
```

**T3: CLI retrieval (human-readable)**
```bash
hmn content get notes/a.md notes/b.md
# Verify: human-readable output with PATH, VAULT, HASH, SIZE, MTIME header per file
```

**T4: CLI retrieval (JSON)**
```bash
hmn content get notes/a.md --json
# Verify: JSON response envelope matches HTTP shape
```

**T5: Invalid path (should reject)**
```bash
curl -X POST http://localhost:8080/content/get \
  -H "Content-Type: application/json" \
  -d '{"paths": ["/etc/passwd"]}'
# Verify: HTTP 422 `invalid_path`
```

**T6: MCP tool invocation**
```bash
hmn mcp --json | jq '.tools[] | select(.name == "content_get")'
# Verify: tool `content_get` present in list
```

---

## Negative Fingerprint Checklist

- [ ] `rg 'fs::read|tokio::fs::read|File::open' src/api src/search` returns no matches in content-retrieval handler
- [ ] All SQLite access inside `spawn_blocking` (verified via code review)
- [ ] No `unwrap()` on fallible operations in the retrieval path
- [ ] Response shape matches HTTP/MCP/CLI consistently (transport parity test passes)

---

## Acceptance Gate Criteria

All criteria must be ✅ to ship:

1. ✅ `HypomnemaBackend::content_get` trait method exists and is called from HTTP, CLI, both MCP transports
2. ✅ HTTP route `POST /content/get` works; curl tests pass
3. ✅ MCP tool `content_get` registered on both transports; tool list includes it
4. ✅ CLI `hmn content get PATH... [--vault ...] [--json]` works
5. ✅ Result items include `path`, `content`, `content_hash`, `size`, `mtime`, `vault`, `vault_name` on success; `error.code` + `error.message` on failure
6. ✅ Per-item errors do not abort batch; HTTP 200 even if all items fail
7. ✅ Validation: empty `paths` → 422, absolute/`..` → 422, nonexistent vault → 404
8. ✅ Cross-vault fan-out reuses content-search pattern; results ordered `(path ASC, vault_id ASC)`
9. ✅ Paused/errored vault behavior correct; explicit paused vault retrieval works
10. ✅ All rusqlite inside `spawn_blocking`
11. ✅ Negative fingerprint: no filesystem reads in handler
12. ✅ All nine acceptance stories pass
13. ✅ `docs/specs/content-retrieval.md` published
14. ✅ `docs/specs/mcp-streamable-http.md` amended with `content_get` row
15. ✅ `cargo test` green; `cargo clippy -- -D warnings` clean
16. ✅ Manual-testing fixture updated

---

## Notes for Coordinator and Builder

- **Deferred decisions are resolved** (see top section); no blocking questions remain.
- **Task 1 (backend logic) is critical path**; other tiers can start in parallel once Task 1 types are defined.
- **Transport parity (HTTP ↔ MCP)** is a natural break point for parallel work (Tier 2 + Tier 3).
- **Testing infrastructure** (Tier 8) should be built incrementally alongside implementation, not deferred to the end.
- **Coordinate with Step 20**: no blocking dependencies, but recommend shipping Step 19 first to establish patterns before Step 20 adds schema complexity.
- **Manual testing is essential** — the fixture recipe above should be run before gate review to catch edge cases (e.g., ordering correctness, error handling).

---

## Version History

| Date | Author | Change |
|------|--------|--------|
| 2026-05-02 | Researcher | Initial workplan from Round 9 roadmap |
