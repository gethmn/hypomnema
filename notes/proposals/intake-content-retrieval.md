# Proposal Intake: Content Retrieval

**Status**: Intake complete
**Date**: 2026-05-02
**Intake inputs**:

- `notes/proposals/content-retrieval.md` — Primary proposal (Status: Draft, 2026-04-30)
- `notes/proposals/content-retrieval-stories.md` — Acceptance criteria for nine user stories across four epics

---

## Summary

Content retrieval gives consumers a direct-fetch operation: hand the daemon one or more vault-relative paths and get back the indexed file text plus its content metadata (`content_hash`, `size`, `mtime`, `vault`, `vault_name`). It is the natural follow-on to the three search modes — search answers *which file*, content retrieval answers *give me this file*. The source of truth is the indexed `files.content` column in each per-vault `index.sqlite`; the operation never reads from the vault filesystem at query time, so an agent that searched, received a `content_hash`, and then retrieved sees the same state the search saw. Surface is uniform across HTTP (`POST /content/get`), MCP (new read-only tool `content_get`), and CLI (`hmn content get`); cross-vault fan-out mirrors content-search semantics, including paused/errored vault handling and partial-result diagnostics. Read-only by definition — no v0 read/write boundary concerns.

## Source Inputs

| Source | Type | Role in intake |
|---|---|---|
| `notes/proposals/content-retrieval.md` | proposal | primary — defines behavior, schema, edge cases, integration points, open questions |
| `notes/proposals/content-retrieval-stories.md` | stories | primary — nine acceptance-criteria stories across four epics (core retrieval, multi-vault, transport, validation) |
| `notes/proposals/intake-search-result-payload-budget.md` | prior intake | background — explicitly deferred full-file retrieval to this proposal; clarifies the search/retrieval boundary |
| `notes/roadmap/archive/roadmap-8.md` + `step-18-workplan.md` | shipped round | background — search-result payload budget (`include_text`, `preview_bytes`, `content_hash` in semantic results) shipped 2026-05-02; sets up the consumer flow that retrieval completes |
| `docs/specs/content-search.md` | spec | background — same cross-vault fan-out and partial-results conventions retrieval will reuse |
| `docs/specs/semantic-search.md` | spec | background — emits `content_hash` (round 8); retrieval consumes that identifier as the freshness anchor |
| `docs/specs/filesystem-search.md` | spec | background — establishes `(path, content_hash)` discovery → retrieval pairing |
| `docs/specs/vault-management.md` | spec | background — § Cross-Vault Search Semantics defines partial_results shape that retrieval mirrors |
| `docs/specs/mcp-streamable-http.md` | spec | background — tool surface table needs `content_get` added (cross-spec follow-up, called out in proposal § Open Questions) |
| ADR-0003 / 0004 / 0009 / 0012 / 0013 | decisions | background — index ownership, three peer search modes, multi-vault, MCP transports |

## Candidate Outcomes

- **Outcome: Direct file retrieval after search**
  - Source: Story 1 + Story 2 + Story 8
  - User-visible result: An agent that calls `search_filesystem` (or `search_content` / `search_semantic`) and gets back a `(path, content_hash)` can immediately call `content_get` with that path and receive the indexed text — no separate filesystem read, no transport round-trip per file.
  - Verification signal: An MCP round-trip of `search_filesystem` → `content_get` returns a `content_hash` in the retrieval response that matches the search result; `rg 'fs::read|tokio::fs::read|File::open' src/` shows zero matches in the content-retrieval handler path.

- **Outcome: Batched multi-file retrieval with per-item errors**
  - Source: Story 2 + Story 3
  - User-visible result: A single `POST /content/get` with N paths returns N result items (success or per-item error) without aborting on the first miss; agents can fan out one HTTP/MCP call instead of N.
  - Verification signal: A 5-path batch where 3 exist and 2 are missing returns HTTP 200 with 5 items (3 success, 2 `path_not_found`); `paths: []` returns HTTP 422.

- **Outcome: Multi-vault fan-out with consistent partial-results shape**
  - Source: Story 4 + Story 5 + Story 6
  - User-visible result: Default scope retrieves from all active vaults; explicit `vaults` selector restricts; explicit selection of a paused/errored vault is honored when its `index.sqlite` is readable, with the vault status surfaced via `partial_results.skipped`.
  - Verification signal: Default-scope request returns one item per vault for path collisions; `vaults: ["nonexistent"]` returns HTTP 404; explicit-paused-vault request returns content + a `partial_results.skipped` entry with `status: "paused"`.

- **Outcome: Uniform surface across HTTP / MCP / CLI**
  - Source: Story 7 + Story 8
  - User-visible result: `hmn content get`, `POST /content/get`, and the `content_get` MCP tool all hit the same backend handler and accept the same `paths` + `vaults` shape. The MCP tool is read-only and does NOT fall under the `[mcp] enable_write_tools` gate.
  - Verification signal: Tool list from `hmn mcp` and from the Streamable-HTTP MCP endpoint both include `content_get`; calling it returns the same shape as the HTTP endpoint; `[mcp] enable_write_tools = false` does not hide the tool.

- **Outcome: Vault-relative path validation at the request boundary**
  - Source: Story 9
  - User-visible result: Absolute paths and `..` segments are rejected with `invalid_path` 422 before any index lookup runs.
  - Verification signal: `paths: ["/abs"]`, `paths: ["../escape.md"]`, and `paths: ["notes/../escape.md"]` all return 422 `invalid_path`; `paths: ["notes/file.md"]` is accepted.

## Proposed Roadmap Shape

### Step N — Content Retrieval (`content_get`)

**Goal**:
Add a read-only content retrieval operation that fetches indexed file text by vault-relative path, fans out across vaults using the same partial-results conventions as content-search, and exposes a uniform `content_get` surface across HTTP, MCP, and CLI.

**Shipping criteria**:

- [ ] `HypomnemaBackend::content_get` is added to the backend trait and implemented once; both the stdio MCP shim (via `DaemonClient`) and the in-process backend share the implementation.
- [ ] HTTP route `POST /content/get` accepts the request envelope (`paths: [string]`, optional `vaults: [string]`) and returns the response envelope (`results: [...]`, optional `partial_results`) per the spec.
- [ ] MCP tool `content_get` is registered on both stdio and Streamable-HTTP transports with input schema matching `paths` + `vaults`. The tool is available regardless of `[mcp] enable_write_tools` (it is read-only).
- [ ] CLI `hmn content get PATH... [--vault NAME|ID] [--json]` prints content + metadata header per file (separated by `---` for multi-file); JSON mode emits the response envelope verbatim.
- [ ] Result items contain `path`, `content`, `content_hash`, `size`, `mtime`, `vault`, `vault_name` on success; per-item `error.code` + `error.message` on failure (`path_not_found`).
- [ ] Per-item errors do not abort the batch; HTTP status remains 200 even when every item is `path_not_found`.
- [ ] Validation: empty `paths` → 422 `invalid_request`; empty `vaults` → 422 `invalid_request`; absolute path or path with `..` segments (including internal) → 422 `invalid_path`; nonexistent vault name → 404 `vault_not_found`.
- [ ] Cross-vault fan-out reuses the content-search pattern (`tokio::join_all` over per-vault `Arc`-cloned vault runners). Path collisions across vaults yield separate result items per vault.
- [ ] Result ordering: `(path ASC, vault_id ASC)` across all vaults; not by request input order.
- [ ] Paused vault behavior: silently skipped from default scope (entry in `partial_results.skipped`); served from index when explicitly named in `vaults` with a `partial_results.skipped` entry annotated `status: "paused"`.
- [ ] Errored vault behavior: same as paused but with `status: "errored"`; if the index file itself is unreadable, vault appears in `partial_results.failed` with a `vault_retrieval_failed` code.
- [ ] All-targeted-vaults-failed case: top-level 503 `vault_retrieval_failed` with the failure detail in `partial_results.failed`.
- [ ] CLI exit code policy: exits 0 on partial success (any item succeeded); exits non-zero only when every item errored, the request itself was rejected, or no vault was reachable. Per-item errors print to stderr; success items print to stdout.
- [ ] All rusqlite access wrapped in `tokio::task::spawn_blocking` per the load-bearing rule (no async-runtime SQL).
- [ ] Negative-fingerprint clean: `rg 'fs::read|tokio::fs::read|File::open' src/api src/search` returns no matches attributable to the content-retrieval handler — the handler reads only from the index, never from the vault filesystem.
- [ ] All nine stories pass acceptance criteria.
- [ ] `docs/specs/content-retrieval.md` published as the canonical feature spec (promoted from `notes/proposals/content-retrieval.md`).
- [ ] `docs/specs/mcp-streamable-http.md` tool surface table amended to list `content_get` as a read-only tool.
- [ ] `cargo test` green; `cargo clippy -- -D warnings` clean.
- [ ] Manual-testing fixture exercises single-file retrieval, multi-file batch with mixed hits/misses, default-scope multi-vault fan-out, explicit `--vault` scoping, and explicit paused-vault retrieval.

**Deferred decisions to resolve at workplan-time**:

- Decision: Response content encoding documentation
  - Source: Proposal § Open Questions (lossy UTF-8 decode at index time)
  - Why this step: `files.content` already stores UTF-8 with lossy substitution applied at index time. The retrieval response just returns that string. Decide whether to document the lossy-decode behavior in the canonical spec or defer to a future "index quality" surface. **Intake recommendation**: document it in `docs/specs/content-retrieval.md` § Implementation Notes — it's a meaningful caveat for consumers comparing retrieved bytes to on-disk bytes.

- Decision: `path_not_found` vs `content_not_indexed` distinction
  - Source: Proposal § Open Questions
  - Why this step: If a row in `files` can exist with `content` NULL or empty (indexer ran but content column unpopulated), that's a different failure mode from "no row at all." Workplan should confirm whether this case is reachable in practice and, if so, mint a separate `content_not_indexed` code. **Intake recommendation**: confirm reachability via a quick read of the indexer; if unreachable, leave `path_not_found` as the only per-item not-found code and call that out in spec.

- Decision: Symlink handling
  - Source: Proposal § Open Questions
  - Why this step: Filesystem-search follows symlinks within the vault root. Whether retrieval can serve a symlinked file depends on what `files.path` stores (the symlink path or the real path). Verify during implementation by reading the indexer's path-recording code. **Intake recommendation**: if the indexer stores the symlink path verbatim, retrieval works transparently; if it canonicalizes to the real path, document that and note callers should query the canonical path.

- Decision: `mcp-streamable-http.md` tool surface amendment
  - Source: Proposal § Open Questions; § Integration Points
  - Why this step: Doc-only follow-up; bundle into this step's shipping criteria. No design risk — it's a one-line table edit.

**New deps**:

- (none — content retrieval reuses axum, rmcp, clap, rusqlite, anyhow, tokio. No new crates.)

**Risk**: low

Rationale: The handler is structurally a smaller version of `search_content` — same per-vault fan-out, same partial-results conventions, same `spawn_blocking` SQL pattern. No new tables, no new columns, no embedding work, no schema migration. The trait method is additive. The CLI subcommand reuses existing argument parsing patterns. The single non-trivial design surface is the per-item-error envelope, but the spec already pins it (untagged enum or `Option`-of-fields), and content-search already uses an analogous shape. The two notable hazards — paused-vault behavior and path-collision handling — both have established patterns from search to mirror. No async/SQLite re-architecture; no concurrent-access changes.

**Source coverage**:

- Story 1 (Single file by path): This step
- Story 2 (Multiple files in one request): This step
- Story 3 (Per-item errors don't abort): This step
- Story 4 (Default fan-out to all active vaults): This step
- Story 5 (Scope to a named vault): This step
- Story 6 (Explicit retrieval from paused vault): This step
- Story 7 (CLI surface): This step
- Story 8 (MCP tool surface, both transports, write-tools-gate exemption): This step
- Story 9 (Reject `..` and absolute paths): This step

---

## Coverage Map

| Source item | Proposed step | Status | Notes |
|---|---|---|---|
| Story 1: Retrieve a single file by vault-relative path | Step N | planned | Core happy path; proves index-as-source-of-truth invariant |
| Story 2: Retrieve multiple files in one request | Step N | planned | Batched lookup with deterministic ordering |
| Story 3: Per-item errors do not abort the batch | Step N | planned | Establishes batch-error envelope shape |
| Story 4: Fan out to all active vaults by default | Step N | planned | Reuses content-search fan-out scaffolding |
| Story 5: Scope retrieval to a named vault | Step N | planned | Reuses vault-resolution + closest-name-hint helpers |
| Story 6: Explicit retrieval from a paused vault | Step N | planned | Mirrors vault-management partial-results conventions |
| Story 7: CLI `hmn content get` | Step N | planned | New subcommand; loopback HTTP call to daemon |
| Story 8: MCP tool `content_get` (stdio + HTTP) | Step N | planned | Both transports register via shared backend trait; bypasses `enable_write_tools` |
| Story 9: Reject invalid paths (`..`, leading `/`) | Step N | planned | Validation at request boundary, before any index lookup |
| Proposal § Open Question: lossy UTF-8 decode docs | Step N | planned | Document in `docs/specs/content-retrieval.md` § Implementation Notes |
| Proposal § Open Question: `content_not_indexed` separate code | Step N | planned | Confirm reachability during workplan; mint code only if real |
| Proposal § Open Question: symlink handling | Step N | planned | Verify indexer path-recording behavior; document |
| Proposal § Open Question: amend `mcp-streamable-http.md` tool table | Step N | planned | Bundle as a doc-only criterion in this step |
| Proposal § Integration Points: `HypomnemaBackend::content_get` trait method | Step N | planned | Single shared implementation behind the trait |
| Proposal § Integration Points: `POST /content/get` HTTP route | Step N | planned | Mirrors `/search/content` route pattern |
| Proposal § Integration Points: `content_get` MCP tool registration | Step N | planned | Read-only; not gated by `enable_write_tools` |
| Proposal § Integration Points: CLI `hmn content get` | Step N | planned | Includes human-readable + `--json` modes |
| Proposal § Edge Cases: path collision across vaults | Step N | planned | Tested via Story 4 acceptance |
| Proposal § Edge Cases: paused/errored vault behavior | Step N | planned | Tested via Story 6 acceptance |
| Proposal § Edge Cases: content not yet indexed | Step N | planned | Returns `path_not_found`; documented behavior |
| Proposal § Implementation Notes: spawn_blocking, fan-out, ordering | Step N | planned | Same patterns as content-search |
| Proposal § Implementation Notes: negative-fingerprint check | Step N | planned | Encoded as a shipping-criteria item |

---

## Deferred / Out-of-Scope Items

- Item: Chunk- or section-level retrieval
  - Source: Proposal § Overview implies file-grain only; intake-search-result-payload-budget.md § Deferred Items explicitly defers chunk/section retrieval
  - Reason: V0 retrieval scope is full-file. Chunk/section retrieval would either amend this spec or land as its own small proposal.
  - Revisit trigger: Agents reporting that full-file payloads are too large for follow-up reads after a `search_semantic` hit, or a use case for surgical chunk fetches that aren't well-served by `search_semantic` with `include_text: "full"`.

- Item: Pagination or streaming for very large `paths` batches
  - Source: Proposal § Validation Rules ("`paths` length has no hard cap in v0; very large batches are an operator concern")
  - Reason: V0 keeps the request shape simple; consumers controlling their own batch size is sufficient. Streaming adds transport complexity that isn't justified yet.
  - Revisit trigger: Operator reports of memory pressure on large multi-file batches, or agents reporting that they hit response-size limits.

- Item: Conditional retrieval (`If-None-Match` against `content_hash`)
  - Source: Not in proposal; natural extension once consumers have the hash
  - Reason: Search-result `content_hash` (now landed in semantic results per round 8) makes a conditional retrieval surface viable, but no consumer has asked for it yet. Adding the cache header now is speculative.
  - Revisit trigger: An agent host implementing a content cache and asking for the conditional surface.

- Item: Vault-write surface for content (write-back from agent edits)
  - Source: Proposal § Behavior is read-only; explicitly out of v0 per `AGENTS.md` § What not to build
  - Reason: V0 is read-only. Writes are post-v0 by project rule.
  - Revisit trigger: Post-v0 phase that adds the write surface; the retrieval shape established here is the natural read pair to a future write surface but does not constrain it.

- Item: Frontmatter parsing in retrieval response
  - Source: Proposal § Data Schema notes "frontmatter is not parsed separately in v0"
  - Reason: V0 returns raw indexed text. Structured frontmatter access would be a larger separate feature.
  - Revisit trigger: A consumer feature that needs frontmatter as a structured field — likely overlaps with a future query-by-frontmatter capability.

---

## Open Questions

- Question: Does the round-8 search payload budget shape (`include_text`, `preview_bytes`) constrain the `content_get` API design?
  - Why it matters: Round 8's intake explicitly deferred full-file retrieval to this proposal. Consumers will see two text-bearing surfaces — `search_semantic` with `include_text: "full"` and `content_get` — and we want to make sure the retrieval design doesn't accidentally fork the contract.
  - Blocks roadmap? No
  - Suggested owner: Workplan author
  - **Intake recommendation**: **No constraint.** The two surfaces serve different purposes and stay separate by design. `include_text` is a search-result text-budget knob — it shapes what discovery returns. `content_get` IS the retrieval and always returns the indexed text in full (there is no "preview" for retrieval; if you wanted a preview, you would have used search). The shared identifier across both surfaces is `content_hash` — round 8 added it to semantic results, and content retrieval already returns it. That hash is the freshness anchor that ties search-time and retrieval-time observations to the same indexed row. Recommend the workplan call this out explicitly in `docs/specs/content-retrieval.md` § Integration Points so the design pair is documented for future readers.

- Question: Should the response document the lossy UTF-8 decode behavior (`files.content` is decoded with substitution at index time)?
  - Why it matters: Consumers comparing retrieved bytes to fresh on-disk bytes for non-UTF-8 files will see substitutions in the retrieval response. Without a documented note this is surprising.
  - Blocks roadmap? No
  - Suggested owner: Workplan author
  - **Intake recommendation**: Yes. Document in `docs/specs/content-retrieval.md` § Implementation Notes alongside the existing "source of truth is the index" rule. Cite the corresponding line in `docs/specs/content-search.md` to keep the wording consistent.

- Question: Is the per-item failure mode `content_not_indexed` (row exists in `files`, but `content` is NULL/empty) actually reachable?
  - Why it matters: If reachable, it warrants its own error code so consumers can distinguish "index doesn't know about this path" from "index knows but doesn't have the content." If unreachable, adding a code is dead schema.
  - Blocks roadmap? No
  - Suggested owner: Workplan author (verify against `src/indexer/`)
  - **Intake recommendation**: Verify during the workplan reading pass. If the indexer guarantees `content` is populated whenever the row exists, leave `path_not_found` as the only per-item code and document that invariant. If reachable (e.g., via a partial-write or aborted-index path), mint `content_not_indexed`.

- Question: Does the indexer record `files.path` as the symlink path or as the canonicalized real path?
  - Why it matters: Determines whether consumers can retrieve a symlinked file by its symlink path or only by its target path.
  - Blocks roadmap? No
  - Suggested owner: Workplan author (verify against the filesystem walker)
  - **Intake recommendation**: Verify and document in `docs/specs/content-retrieval.md` § Implementation Notes. No design change either way; users deserve to know which key works.

- Question: Should the round number host any orthogonal work, or run as a single-step round?
  - Why it matters: Round 8 was deliberately a single-step round and shipped cleanly. Round-9 could similarly be content-retrieval-only, or could pair retrieval with another small item (e.g., the FTS5 BM25 content-search proposal that's also in `notes/proposals/`).
  - Blocks roadmap? No
  - Suggested owner: Orchestrator + human at round-planning time
  - **Intake note**: Out of intake scope. Pairing decision is a roadmap-write concern, not an intake concern. The retrieval step itself stands alone.

---

## Recommendation

**This proposal is ready to anchor a focused round. Recommend "start step N now" — proceed to draft `notes/roadmap/roadmap-9.md` and the matching `step-NN-workplan.md`.**

Rationale:

1. **Inputs are complete and well-formed.** Proposal v0.1.0 (2026-04-30) is fully drafted with behavior, schema, edge cases, error catalog, integration points, implementation notes, and four open questions. Stories cover all behavior dimensions across four epics (core, multi-vault, transport, validation). Cross-spec dependencies (`content-search.md`, `vault-management.md`, `mcp-streamable-http.md`, the four ADRs) are pinned. No critical questions remain.

2. **Design fits inside the v0 envelope.** Content retrieval is read-only by construction — it queries `files.content` from the per-vault index and never touches the vault filesystem. The "no writes to vault" v0 rule is automatically satisfied; no story implies a write. The "no Hypomnema state inside the watched vault" rule is satisfied (the daemon reads from its own per-vault `index.sqlite`, which lives in the data dir, not under the watched path).

3. **LDS layer impact is well-bounded.** New canonical spec at `docs/specs/content-retrieval.md` (promoted from the proposal). Tool-surface-table amendment in `docs/specs/mcp-streamable-http.md`. No new ADR needed — ADR-0004 already established the three-search-modes-as-peers shape, and content retrieval is explicitly *additive* to those peers, not a fourth peer. No changes to existing ADRs.

4. **Risk is genuinely low.** No new dependencies. No SQL schema changes. No index changes. No async/SQLite re-architecture. The handler is structurally a smaller version of `search_content` — same fan-out pattern, same partial-results conventions, same `spawn_blocking` discipline. The trait method is additive; the HTTP route, MCP tool, and CLI subcommand follow established patterns.

5. **Round 8 set up the consumer flow cleanly.** Round 8 added `content_hash` to semantic results, which is the freshness anchor that pairs search-time observations with retrieval-time fetches. The two surfaces are the natural complement to each other: search returns `(path, content_hash)`; content retrieval returns `(content, content_hash)` for that path. The fact that round 8 explicitly deferred full-file retrieval to this proposal makes round 9 (or whatever round picks this up) the obvious follow-on.

6. **Recommended next steps when round-9 (or successor) planning begins**:
   - Draft `notes/roadmap/roadmap-N.md` as a single-step round (matching round-8's shape) OR pair this step with one orthogonal item (e.g., a small backlog cleanup) if the orchestrator wants more breadth — that's a roadmap-write call, not an intake call.
   - Draft `step-NN-workplan.md` with the shipping criteria above. Verify the four open questions against current code as the workplan's first task; record the answers in the workplan's deferred-decisions section.
   - Confirm step number against `notes/roadmap/archive/`: highest shipped step workplan is `step-18-workplan.md` (round 8). Step numbers 14 and 17 never had standalone workplans (they only appear inside earlier roadmaps), so the next step number to assign is the orchestrator's call at roadmap-write time.
   - Manual-testing fixture refresh should be folded in (per the rolling backlog item on `notes/manual-testing/` drift) — content retrieval is a new top-level surface and warrants its own fixture entry.

---

## Human Review Notes

(append review decisions here)
