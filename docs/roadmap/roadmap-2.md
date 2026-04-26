# Hypomnema Roadmap — Round 2: Semantic + MCP

**Scope**: Steps 6–8 of the eight enumerated in [`docs/implementation/tech-stack.md`](../implementation/tech-stack.md). Step 8 (the MCP wrapper) is the natural shipping gate for this round — it puts the daemon's full search surface in front of the agent host (Claude Code, Iris) it was built for.

**Status**: Not started. Round 1 (steps 1–5, the v0 HTTP shipping gate) shipped 2026-04-26 — see [`roadmap.md`](./roadmap.md) for the prior round, [`notes/project-planning-workflow-notes.md`](../../notes/project-planning-workflow-notes.md) for the per-step retrospectives and the round-1 end-of-round retro that motivates this doc.

**Process**: Same as round 1. Each step gets a short workplan (`step-NN-workplan.md`) created **just before** that step is implemented. TBDs flagged in the docs are resolved at or before the step that needs them. The orchestration shape (orchestrator + per-step coordinator + ephemeral task agents, see [`notes/coordinator-playbook.md`](../../notes/coordinator-playbook.md)) carries forward unchanged.

**Round-1 lessons feeding into this round** (see end-of-round retro for full text):
- Risk grades stay useful for predicting wall-clock; grade steps 6 and 7 honestly (likely **high** — embedding-service contract and sqlite-vec extension loading are the round's two new failure surfaces).
- Pull deferred decisions forward into workplan-time, especially for steps that introduce a new external contract (step 6's embedding service in particular).
- Long workplans (~1000+ lines) warrant a self-review pass for prose-accuracy claims about external library semantics — round 1's only build-time soft flags caught two such slips.
- The two skills `markdown-chunking` and `sqlite-vec-extension` are already in tree and become load-bearing for this round the way `rusqlite-in-async` and `filesystem-watching` were for round 1.

---

## Step 6 — Chunking and embedding

**Status**: shipped 2026-04-26. See [`step-06-workplan.md`](./step-06-workplan.md) for the workplan and [`notes/project-planning-workflow-notes.md`](../../notes/project-planning-workflow-notes.md) § Step 6 for the retrospective.

**Goal**: On each real change, `hmnd` parses the changed file with `pulldown-cmark`, splits it into heading-aware chunks (per the `markdown-chunking` skill), embeds each chunk via HTTP to a local OpenAI-compatible embedding service (default: a TEI sidecar), and persists the chunk metadata + vector to a `chunks` metadata table and a sibling `chunks_vec` virtual table (per the `sqlite-vec-extension` skill). The vec0 dimension is baked at schema creation; mismatch with config fails the daemon at startup.

**Shipping criteria**:
- Editing a watched file results in a fresh set of `chunks` rows for that file (delete-and-reinsert per the sqlite-vec skill); old chunks are gone.
- Chunk count for a known fixture file matches what the chunker emits in unit tests.
- A `chunks_vec` row exists for every `chunks` row, with the right dimension.
- Embedding-service unavailability does not crash the daemon — the chunk + vec rows for that file are skipped, the change is logged, and the daemon stays responsive to search queries.
- `hmnd config-validate` fails loudly when `embedding.dimension` differs from the schema-baked value.

**Deferred decisions to resolve here**:
- Embedding-service contract details: timeout, retry policy, batch size, on-failure behavior (skip vs. queue) — pull these forward at workplan time.
- Chunk size cap and overflow rule (the `markdown-chunking` skill describes the boundary rules; the cap is the one number not yet pinned).
- Whether chunks carry a `frontmatter` summary or only the body slice.
- Whether `chunks_vec`'s dimension lives in config or in the schema (and how mismatch is detected at startup — `PRAGMA` introspection vs. a known-shape probe).
- How the sqlite-vec extension binary is located (bundled, dynamic-load path, env-var override).

**New deps**: `pulldown-cmark`, `sqlite-vec` (or its bundled-loading shim), `reqwest` is already in tree from step 5.

**Risk**: high. New external contract (embedding service), new SQLite extension, new schema with an immutable dimension. The roadmap explicitly calls this out as "the step most likely to surprise you."

---

## Step 7 — Semantic search

**Goal**: Axum exposes `/search/semantic` with the response shape from [`docs/specs/semantic-search.md`](../specs/semantic-search.md). The handler embeds the query via the same embedding service, runs a `chunks_vec` nearest-neighbor query (cosine similarity), joins back to `chunks` for path / heading / text metadata, and returns the top-N results. `hmn search semantic <query>` lights up.

**Shipping criteria**:
- `hmn search semantic 'how do we prevent spurious reindexes'` against an indexed vault returns sensibly-ranked chunks with similarity scores.
- `curl http://127.0.0.1:7777/search/semantic` with a JSON body returns the spec response shape.
- Empty index returns empty results + a hint indicating the semantic index is building (per the spec's empty-index edge case).
- Result shapes include the `vault: Option<String>` forward-compat field per round 1's resolution 1; v0 always omits.
- Embedding-service unavailability at query time returns a 503 with `code: "embedding_unavailable"` (or similar — to be pinned in the workplan), not a 500.

**Deferred decisions to resolve here**:
- Default `min_similarity` threshold (or skip the threshold entirely in v0).
- Behavior when `chunks_vec` is empty but `files` is not (ranked-empty vs. hint).
- Result ordering across ties (by file path? by `chunks.chunk_index`?).
- How the query embedding is cached (per-process, per-query, none).
- The error envelope code for embedding-service-unavailable (`embedding_unavailable` vs. `internal` vs. a new `service_unavailable` class).

**New deps**: none (embedding client + sqlite-vec already landed in step 6).

**Risk**: medium. Composes step 6's embedding + storage with step 5's HTTP surface. The composition is structurally similar to step 5's filesystem/content handlers; the load-bearing risk is the embedding-service-at-query-time path.

---

## Step 8 — MCP wrapper (round shipping gate)

**Goal**: `hmnd` exposes the same three search operations (`search_filesystem`, `search_content`, `search_semantic`) over MCP via the `rmcp` crate, transported either over stdio (default — agent hosts spawn `hmnd --mcp-stdio`) or over a Unix socket. The MCP layer is a thin wrapper over the same query functions in `src/search/`. Test against an actual agent (Claude Code or Iris) end-to-end.

**Shipping criteria**:
- `hmnd --mcp-stdio` (or final flag shape) starts an MCP server over stdio with the three tools advertised.
- Claude Code (or another MCP-capable agent) can invoke each tool against the running daemon and get back results that match the spec response shapes.
- The `vault` forward-compat field passes through MCP responses identically to HTTP responses (omitted in v0; serde shape preserved).
- Unix-socket transport is wired (per the existing `mcp.transport = "socket"` config) but stdio is the default.
- The MCP error mapping mirrors HTTP's error-envelope codes.

**Deferred decisions to resolve here**:
- Final flag shape: `hmnd --mcp-stdio` vs. `hmnd mcp-stdio` (subcommand) vs. env-var-driven mode.
- MCP tool names — keep `search_filesystem` / `search_content` / `search_semantic` (matching ADR-0004) or use a flatter naming.
- Tool parameter schemas — derive from `src/api/types.rs` request shapes (with adjustments) or hand-author.
- Connection lifecycle: process-per-connection (stdio, fork-and-exec) vs. long-lived (socket) — implications for shutdown.
- Authentication on the socket transport (filesystem permissions vs. nothing vs. a token).

**New deps**: `rmcp` (the official MCP transport crate).

**Risk**: medium. New transport surface; the underlying query code is shared with HTTP. Real risk is the agent-integration test (round 1 had no analogous agent-in-the-loop dependency).

---

## After step 8

When step 8 ships:
1. Tag the milestone in git (likely `v0.1.0` or `v0`).
2. Capture any ADRs that hardened during the build.
3. Write a short retrospective for steps 6–8 in [`notes/project-planning-workflow-notes.md`](../../notes/project-planning-workflow-notes.md), and an end-of-round retro answering: did the roadmap → workplan → build cadence still work at higher risk? What surprised us about embedding/sqlite-vec/MCP that the docs did not predict?
4. Move on to round 3 (Multi-vault) below, or close the project's "open in scope" list. The handoff doc's "Out of scope" list is the natural source for additional follow-on work.

---

## Round 3 — Multi-vault (post-v0)

**Scope**: Implement the multi-vault adoption settled in [ADR-0009](../decisions/0009-multi-vault-per-daemon.md), [ADR-0010](../decisions/0010-vault-definitions-as-runtime-state.md), and [ADR-0011](../decisions/0011-vault-management-on-hmn.md). The canon was amended on 2026-04-26 (immediately after round 1 shipped), pre-staging this round so its scope is settled before round 2 (steps 6–8) lands.

**Status**: Not started. Round 2 (steps 6–8) has not yet begun; this round queues behind it. Workplan(s) created just before implementation per the round-1/2 cadence.

**Specs to amend / create**:
- `docs/specs/vault-management.md` (already drafted as an outline; `spec-generator` flesh-out at workplan time)
- `docs/specs/filesystem-search.md` — add per-result `vault` (id) and `vault_name`; add request-side `vaults` filter; describe cross-vault behavior
- `docs/specs/content-search.md` — same shape
- `docs/specs/semantic-search.md` — same shape, plus cross-vault ranking semantics
- `docs/specs/change-events.md` — add `vault` (id); **no** `vault_name` (durability)

**Implementation surface**:
- New modules: `src/vault_registry/`, `src/control_plane/`
- Per-vault refactor of `src/watcher/`, `src/indexer/`, `src/store/`, `src/outbox.rs`
- `vaults.sqlite` registry; per-vault data subdirectory layout
- Control-plane HTTP routes; `hmn vault …` subcommands; MCP tools (subset to ship per workplan)
- `hmnd scan` subcommand removed (subsumed by `hmn vault rescan`)
- Removal of top-level `vault` config key; addition of `default_vault_name`

**Deferred decisions to resolve at the round-3 workplan**:
- Cross-vault search semantics: result ordering, pagination/cursor, fan-out execution, `limit` semantics, partial-failure handling, paused/errored vault inclusion (see [`vault-management.md` § Open Questions](../specs/vault-management.md#open-questions))
- Surrogate ID format (`vault_<base32>` vs. UUIDv7 vs. ULID)
- Compose-style declarative provisioning: ship inline with this round, or queue as a follow-on workplan
- Which subset of vault-management MCP tools ships in this round (read-only vs. full)
- Whether `hmnd-compose.toml` ships in this round

**Phasing options** (workplan-time):
- *Single-shot*: full vault-management surface + cross-vault search semantics in one workplan
- *Phased*: vault create/list/status/terminate first; pause/reset/rename/rescan + Compose layer in a follow-on workplan; cross-vault search semantics resolved in the first workplan regardless

**Risk**: medium-to-high. Daemon startup-sequence rewrite + per-vault refactor + new HTTP/MCP surface + new state file requiring atomic write semantics. The cross-vault search semantics question is a genuine design unknown and should be the workplan's first deferred-decision target.

**Shipping criteria** (round-level):
- `hmn vault create / list / status / terminate` against a running daemon work end-to-end
- `hmn search content "X"` against a daemon with two vaults returns intermingled results, each with `vault` (id) + `vault_name`
- `--vaults personal,work` filter narrows scope
- Daemon survives crash mid-create / mid-terminate without orphaning state
- The four spec amendments and the new vault-management spec land

---

## After round 3

Decide whether v0 is *done done* (publish the binary; close the "open in scope" list) or whether a fourth round is justified for follow-on work (e.g., outbox rotation, regex-over-paths, Compose-style declarative provisioning if it didn't ship in round 3, MCP write-tool gating, push notifications). The handoff doc's "Out of scope" list is the natural source for such a round.
