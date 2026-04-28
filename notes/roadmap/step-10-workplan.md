# Step 10 Workplan — Vault control plane (read + create/terminate) + cross-vault search

**Step**: 10 of 11 (round 3 of 3). Lights up the user-visible control plane on top of step 9's per-vault foundation. See [`roadmap-3.md`](./roadmap-3.md) for the round and [`archive/step-09-workplan.md`](./archive/step-09-workplan.md) for the immediately prior step (the per-vault internal refactor + registry foundation).

**Status**: Workplan written 2026-04-28; awaiting human review before build phase.

**Round-2/3 lessons carrying forward** (from [`notes/project-planning-workflow-notes.md`](../project-planning-workflow-notes.md) § End-of-round retrospective + § Step 9):

- **MSRV cross-check** on any new top-level crate. Step 10 introduces zero new top-level crates — control plane is built on existing `axum` / `clap` / `rmcp` / `tokio` / `rusqlite` patterns. Verified at workplan-write; re-verified before each task that adds a `Cargo.toml` line (none anticipated).
- **Manual smoke verification** is load-bearing for medium-high-risk wiring tasks. Step 10's wiring tasks are **10.4** (`hmn vault` CLI subcommands — first user-mutable surface) and **10.7** (integration tests + multi-vault smoke). Smoke is bundled into 10.7 per the round-2 step-7 / round-3 step-9 precedent (5-of-5 wiring tasks across rounds 1–3 paid off).
- **Forward-note prediction-vs-observation** check: round-3 step 10 has fewer external-library predictions than round-2 step 8. The main external-library claim is `rmcp` tool registration and `axum::Router::route`-style mount under a path parameter (`/vaults/{name_or_id}`); both already exercised in earlier steps. The fresh territory is `rmcp` per-tool gating — i.e., conditionally registering tools at server-construction time. Self-review at workplan-write checks `tool_router` macro semantics for conditional registration; if not directly supported, the task body commits to a wrapper-around-tools shape rather than macro-level gating.
- **Workplan-prose-vs-load-bearing-decision drift** is now a stable round-3 pattern (round-3 step-9 retro). Per-vault refactor surface produces wider-than-anticipated ripple effects; coordinator-only soft flags absorbing the gap is the round-3 default. **Carry-forward expectation**: this step's wider-than-step-9 surface (control plane + spec amendments + CLI + MCP + search refinements) will likely surface 2–4 coordinator-only soft flags of this shape; treat them as defer-to-boundary by default unless a downstream task is materially affected.
- **Internal-shape claims** (round-3 step-9 self-review addition): for any task that reshapes an existing module (here: `src/api/mod.rs::ApiState`, the search handler iteration shape, `src/mcp/server.rs::HypomnemaMcpServer`), re-read the task body against the current module signature at workplan self-review and flag aspirational language. Self-review pass at the bottom of this workplan covers this.
- **Skills carrying forward**: [`rusqlite-in-async`](../../.claude/skills/rusqlite-in-async/SKILL.md) (every new control-plane SQL site wraps `spawn_blocking`); [`filesystem-watching`](../../.claude/skills/filesystem-watching/SKILL.md) (control-plane create-vault wires a fresh per-vault watcher per the round-1 + round-3-step-9 pattern); [`markdown-chunking`](../../.claude/skills/markdown-chunking/SKILL.md) and [`sqlite-vec-extension`](../../.claude/skills/sqlite-vec-extension/SKILL.md) remain load-bearing per-vault. No new skills anticipated; if cross-vault fan-out semantics prove worth codifying at boundary, write one then.

---

## Goal recap

`hmnd` exposes the **read** and **create/terminate** vault operations over its three transports — HTTP, the `hmn` CLI, and MCP tools — on top of step 9's per-vault foundation. Cross-vault search semantics (ordering, fan-out, partial-failure handling, paused/errored vault inclusion, semantic-search global top-N, request-side `vaults` filter) are pinned in this step's workplan and applied in build. The vault-management spec is fleshed from outline to full spec; the four search/event specs are amended to reflect the populated multi-vault wire shape (Solo todos 64 + 65; both pulled into workplan-write per [`roadmap-3.md`](./roadmap-3.md) § Step 10).

**Operations shipping in step 10**: `create`, `list`, `status`, `terminate` (the four ADR-0010 read + create/terminate operations). The five remaining lifecycle ops (`pause`, `resume`, `reset`, `rename`, `rescan`) ship in step 11 per the [`roadmap-3.md`](./roadmap-3.md) phasing decision. **Spec coverage is unconditional** per the LDS rule: the vault-management spec covers all nine operations even though step 10 only ships four; the workplan ships a subset of what the spec specifies.

**The four shipping operations exercise the full structural shape** that step 11's remaining operations will reuse:
- HTTP route + handler pattern (`/vaults`, `/vaults/{id_or_name}`, `/vaults/{id_or_name}/{op}`).
- Per-vault async-mutex serialization for in-vault op ordering (ADR-0010 § Concurrency).
- `hmn vault` CLI subcommand wrapping over HTTP.
- MCP tool registration (with the write-tool gating story this step pins).
- HTTP error envelope codes for the vault error catalog.
- `partial_results` diagnostic on search responses for paused/errored/failed vaults.

Step 11 picks up `pause`/`resume`/`reset`/`rename`/`rescan` against an established pattern.

The shipping gate composes round-3-step-9's behavior preservation gate (single-vault still works; existing tests pass) with two new properties:

1. **Two-vault end-to-end works**: create + list + status + cross-vault search + terminate on a real two-vault setup, over HTTP and CLI. (MCP is exercised in unit-test mocks; the round-2 step-8 manual MCP smoke against Claude Code is round-4-prep, not round-3-shipping.)
2. **Spec-write blockers closed**: vault-management.md is full spec (no longer outline); the four search/event specs are amended; the round-3 step-9 "ahead of spec" footnote is closed.

---

## Deferred-decision resolutions

The five TBDs from [`roadmap-3.md`](./roadmap-3.md) § Step 10 are resolved below (A–E). Each resolution is the load-bearing input that **Task 10.1's spec amendments** apply to spec text and that **Tasks 10.2–10.6** ship against.

### A. Cross-vault search semantics

This is the load-bearing question for step 10. Eight sub-questions from [`docs/specs/vault-management.md` § Open Questions](../../docs/specs/vault-management.md#open-questions) and [`roadmap-3.md`](./roadmap-3.md) § Step 10. Resolved together because they interact (ordering, limit, fan-out execution, and partial-failure all touch the same merge step).

#### A.1 Result ordering across vaults — filesystem-search and content-search

**Resolution**: **lift today's per-vault path-ascending order to a global path-ascending order across all vaults.** The merge step interleaves results by `path` (ascending, byte-lexicographic). On identical paths across two vaults (rare but possible — the same relative path appears in both), break ties by `vault_id` (UUIDv7 → creation-time-stable).

For N=1, this is identical to v0/step-9 behavior — single slice already sorted. For N≥2, the cross-vault default is "as if you had one big vault" semantically, with `vault` + `vault_name` per result for origin disambiguation.

**Why global path-sort over per-vault-then-concat**:
- Filesystem-search's primary use is "what files exist matching this pattern?". Path-sorted is the natural answer regardless of vault provenance. Per-vault-then-concat with stable vault ordering creates a per-vault hierarchy in results that isn't reflected in the wire shape (no vault-grouping field).
- The same query against the same content via two different vault topologies (one-vault-of-everything vs. two-vaults-of-halves) returns the same result list in the same order. Operator can split or merge vaults without confusing consumers about ordering.
- Implementation cost is one extra `sort_by` after the gather step. Already infrastructure for `merge_and_truncate`.

**Same resolution for content-search**: today's spec is per-vault path-sorted; lift to global path-sorted with vault-id tie-break.

#### A.2 Result ordering across vaults — semantic-search

**Resolution**: **score-descending across all vaults**, vault-id tie-break for equal scores. Cosine similarity is bounded `[0.0, 1.0]` (per [`docs/specs/semantic-search.md`](../../docs/specs/semantic-search.md) § Score conversion), comparable across same-model embeddings, so no cross-vault score normalization is needed. Score-desc is the natural ranking.

**Pre-existing assumption**: all vaults use the same embedding model (and therefore same dimension). This is already a daemon-wide constraint per [ADR-0007](../../docs/decisions/0007-sqlite-vec-over-alternatives.md) — the embedding service is configured per-daemon, not per-vault, and `chunks_vec`'s dimension is migration-baked. Document the constraint in the amended `semantic-search.md` and the fleshed `vault-management.md`. A multi-model-embedding round (different embedding models per vault) is round-4+.

#### A.3 Pagination / cursor across N independent indexes

**Resolution**: **defer to a future round.** v0 / round-2 specs have no pagination — `truncated: bool` is the only signal. Multi-vault doesn't need to introduce pagination; the `limit` semantics (A.4 below) are sufficient for the round-3 surface. The Open Question stays open in the fleshed `vault-management.md` spec for a future round; the four search-spec amendments do not add pagination fields.

**Why defer**: pagination is its own design surface (cursor stability under concurrent indexing, cross-vault cursor encoding, equivalence of `limit + cursor` and `limit * page`). Folding it into step 10 doubles the scope; doing it later in a dedicated round gives it the room it needs. Round-3 ships without it, same as v0.

#### A.4 `limit` semantics across vaults

**Resolution**: **gather all per-vault results, merge-sort by the mode-specific ordering, truncate to global `limit`.**

For each vault: run the search with the same `limit` value (so each vault contributes up to `limit` results to the merge pool). The per-vault `truncated` flag is preserved into the merged response's `truncated` (any per-vault truncation **OR** post-merge truncation → global `truncated: true`).

**Why gather-and-truncate over per-vault budget**:
- Matches v0 semantics (`limit=N` means "top-N globally"). Operator's mental model of `limit` is unchanged when vault count grows.
- Worst-case cost is `N_vaults * limit` rows in memory pre-merge. For typical limits (100) and typical vault counts (1–5), this is a few hundred rows — negligible.
- Per-vault budget (each vault returns `limit / N`) creates surprising behavior when one vault has many matches and another has none: the all-matches vault under-fills its share. Global `limit` doesn't have this issue.
- Proportional split requires an extra round-trip (count rows per vault before fetching) or biased estimates. Not worth the complexity for round 3.

**Edge case**: `limit=0` is a request validation error (matches v0). `limit > 1000` is also rejected at request validation (defense against runaway memory).

#### A.5 Fan-out execution model

**Resolution**: **gather-then-respond with sequential per-vault iteration.** Step 9 already established this pattern in the search handlers (see `src/api/search.rs::filesystem`/`content`/`semantic`); step 10 keeps it.

**Why sequential over parallel**:
- Step 9's pattern is already sequential and shipped working with the N=1 → N=2 transition tested in `tests/multi_vault_internal.rs::cross_vault_search_returns_intermingled_results_with_vault_id`.
- For typical vault counts (1–5) on a single host, sequential per-vault SQLite reads complete in tens of milliseconds; the parallelism gain is marginal.
- Parallel fan-out introduces tokio-task-spawn overhead, partial-failure-during-spawn questions, and per-vault timeout semantics — non-trivial work that can wait.
- **Streaming responses** (chunked HTTP / SSE / NDJSON) are also deferred to round-4+. The Open Question stays open in the fleshed vault-management spec.

**Forward note**: when a deployment surfaces with N≥10 vaults or measured vault-search-latency that begs for parallelism, that's the trigger to revisit. Round 3 ships against the assumption that operator-vault counts are 1–5.

#### A.6 Partial-failure handling

**Resolution**: **silent-skip + a `partial_results` diagnostic on the response envelope.** When a per-vault search errors (vault-side database error, vault disappeared mid-query, etc.), the daemon logs the error and continues; the merged response carries a non-empty `partial_results` field listing which vaults were skipped or failed and why.

Wire shape (added to all three search response envelopes — filesystem-search, content-search, semantic-search):

```yaml
results: [...]
truncated: false
partial_results:                           # OMITTED when no skips/failures
  skipped:
    - vault: "<id>"
      vault_name: "<name>"
      status: "paused" | "errored"
      reason: "vault is paused" | "vault is errored: <last_error>"
  failed:
    - vault: "<id>"
      vault_name: "<name>"
      code: "vault_search_failed"
      message: "<short detail>"
```

The `partial_results` field is present **only when at least one vault was skipped or failed**. Empty arrays are not emitted; the field itself is absent in the all-success / all-active case. Consumers that ignore the field continue to see well-formed `results` and `truncated` (no breaking change for v0-style consumers; the field is additive).

**Why this shape**:
- Skip-and-diagnose is the natural multi-vault correctness story: the operator's expectation when one vault is briefly errored is "the daemon keeps serving from the others," not "503 the entire query."
- Distinguishing `skipped` (intentional — paused/errored) from `failed` (unexpected — runtime error) gives consumers the signal they need without overloading one channel.
- The shape generalizes to step 11's pause/resume cleanly — paused vaults always show up in `skipped` until resumed.
- Wire-bytes additive: v0/step-9 consumers parsing `results` / `truncated` see exactly the same fields when no skip/fail happens. No bumped-major-version breaking change.

#### A.7 Paused vault inclusion in default scope

**Resolution**: **silent skip + diagnostic.** Default scope (`/search/...` with no `vaults` filter) does not query paused vaults; each paused vault that would have been queried is added to `partial_results.skipped` with `status: "paused"` and `reason: "vault is paused"`. Pause is a user-initiated state; the user's intent is "stop querying this vault until I resume it."

When the request includes `vaults: [...]` filter and the named subset includes paused vaults, the same skip-and-diagnose treatment applies: filtering names a vault, paused state still skips it, the consumer learns via `partial_results`.

Step 10 doesn't actually ship `pause`/`resume` (those are step 11), but the registry already supports the `paused` state from step 9 (a paused vault row can be inserted directly via test fixture or future control plane). The skip behavior must work in step 10 to be ready for step 11's user surface.

#### A.8 Errored vault inclusion in default scope

**Resolution**: **silent skip + diagnostic with the registry's `last_error` propagated to `reason`.** Same treatment as paused, with `status: "errored"` and `reason: "vault is errored: <last_error>"`. The `last_error` text is operator-supplied diagnostic content (e.g., "vault path /home/foo no longer accessible") — propagate it verbatim in the skip diagnostic so consumers can act on it.

**Edge case**: if `last_error` is `NULL` in the registry (which shouldn't happen for an `errored`-status row but is permitted by the schema), use a static fallback `"vault is errored (no last_error recorded)"`.

#### A.9 Cross-cutting: request-side `vaults?: string[]` filter

**Resolution**: **add `vaults: Option<Vec<String>>` to all three search request types**. Each entry in the array is matched against name first, then against id. Names take precedence on collision (impossible by uniqueness — but documented).

Behavior:
- `vaults: None` (or omitted) → query all currently active vaults.
- `vaults: Some([])` → request validation error (`invalid_request: vaults filter must be non-empty`).
- `vaults: Some([...])` → query only the named subset; unknown names produce `partial_results.failed` entries with `code: "vault_not_found"`. Paused/errored vaults in the subset are skipped per A.7/A.8.

The filter is the operator's "narrow scope" tool. Search handlers iterate `active_vaults` filtered by the request's `vaults` field if present.

#### A.10 Cross-cutting summary

The above eight resolutions land in code at **Task 10.5** (search refinements + `vaults` filter + `partial_results`) and in spec text at **Task 10.1**. Step-9's `merge_and_truncate` helper in `src/api/search.rs` is the integration point: extend it to accept the partial-results accumulator, and to apply the score-desc / path-asc ordering per mode.

### B. Vault-management spec fleshout (Solo todo 65)

**Resolution**: **Task 10.1 expands `docs/specs/vault-management.md` from outline to full spec via the `spec-generator` skill**, against the resolutions in this workplan. The fleshed spec covers all nine operations (`create`, `list`, `status`, `pause`, `resume`, `reset`, `rename`, `rescan`, `terminate`) per the LDS rule that specs cover the full intended surface; step 10 ships only the four read+create+terminate operations, and step 11 ships the remaining five against the same spec.

The fleshout commits these previously-Open Questions to spec text:
- **ID format**: UUIDv7 (resolved at step 9, Resolution A; spec § Identifier Model lifts it from Open Questions).
- **Cross-vault search semantics**: all eight sub-resolutions in § A above land in spec § Behavior § Cross-Vault Search Semantics. The corresponding spec § Open Questions entries are removed from "open" — replaced by the resolutions — and replaced by the next round's open questions (pagination, streaming, multi-model embeddings).
- **MCP tool gating** (§ C below): default-on with `[mcp] enable_write_tools = false` opt-out. § Behavior § MCP Tool Surface documents gating.
- **HTTP error envelope codes** (§ D below): § Error Handling table adds the resolutions.
- **Concurrency posture** (§ E below): per-vault async-mutex; § Behavior § Concurrency documents the implementation shape.
- **Default-name-resolution semantics**: when `default_vault_name = ""`, every command must specify a name or ID; the daemon never resolves a default. Already documented in step-9 Resolution C; spec § Identifier Model picks it up.

The fleshout preserves these Open Questions (deferred to round 4+):
- Pagination / cursor across N independent indexes.
- Streaming response shapes (chunked / SSE / NDJSON).
- Compose-file format and merging rules (round-3 step 11 decides whether to ship).
- Multi-model-embedding-per-vault.
- Cross-platform rename safety for the legacy-state migration (step-9 boundary follow-up).

Version bump: **0.1.0 → 1.0.0** (the spec moves from outline to full spec, status from "Draft" to "Approved"). Revision History entry dated 2026-04-28 (or actual ship date).

### C. MCP write-tool gating

**Resolution**: **default-on with single config opt-out.** Add `[mcp] enable_write_tools: bool` to `McpConfig` (default `true`). When `true`, the MCP server registers all four step-10 vault tools (`vault_list`, `vault_status`, `vault_create`, `vault_terminate`). When `false`, only the read-only tools (`vault_list`, `vault_status`) are registered; the write tools are not advertised in the `tools/list` response and `tools/call` against them returns the standard rmcp `unknown tool` error.

**Why single config over per-tool gating**:
- The two write tools (create, terminate) have the same trust posture — both mutate vault registry. Per-tool gating fragments config without ergonomic gain.
- Default-on matches the round-2 trust posture (localhost-only daemon by default; agents already trusted to invoke search tools that read every file in every vault).
- Operators who want strict opt-out get a single-line config edit (`[mcp] enable_write_tools = false`).
- Future write tools (`vault_pause`, `vault_resume`, `vault_reset`, `vault_rename`, `vault_rescan` in step 11) inherit the same gate. No config-key-explosion across rounds.

**Implementation**: Task 10.6 extends `HypomnemaMcpServer` to accept the `enable_write_tools` flag at construction. Implementation shape is one of (a) two `impl HypomnemaMcpServer` blocks gated by a builder pattern that conditionally registers tools, or (b) the simpler "always register, but write-tool fns short-circuit to `unknown_tool` error when gated off." Self-review at workplan close confirms which fits rmcp's `tool_router` macro best; default to (b) if (a) requires non-trivial macro work.

### D. HTTP error envelope codes for vault operations

**Resolution**: **the catalog in `docs/specs/vault-management.md` § Error Handling stands as authoritative.** Codes pinned:

| Error Condition | HTTP Status | `code` | Notes |
|---|---|---|---|
| Vault not found | 404 | `vault_not_found` | Message includes the requested name/ID and (if name) a closest-match hint computed via Levenshtein on the name list. |
| Path already registered | 409 | `vault_path_conflict` | Message includes the existing vault's name. |
| Name already in use | 409 | `vault_name_conflict` | Message includes the existing vault's path. |
| Path invalid (canonicalize fails, not absolute, contains `..` after canonicalization, etc.) | 422 | `vault_path_invalid` | Message describes the validation failure. |
| `data_dir` is under any vault path | 422 | `vault_path_invalid` | Spec-required edge case. |
| Vault is errored, op requires active state | 503 | `vault_errored` | Step 10 doesn't actually ship operations that require active state for any of the four shipping ops (create/list/status/terminate are all status-agnostic). Reserved for step 11 (pause/resume/reset/rescan against errored — which `reset` is the natural recovery for). |
| Registry corrupt / read failure | 500 | `registry_corrupt` | Operator restores from backup. |
| Vault create canonicalize-failed (path doesn't exist or is unreadable) | 422 | `vault_path_invalid` | A vault's path must exist at create-time (the fresh-create flow then immediately starts watching it; nonexistent paths are a configuration error, not an `errored`-state vault). |
| Default-name collision (auto-resolution → existing vault) | 409 | `vault_name_conflict` | Operator passes `--name` or terminates the existing default vault. |
| Per-vault concurrent op conflict (e.g., terminate-during-terminate) | 409 | `vault_op_conflict` | Operations on the same vault serialize per ADR-0010 § Concurrency; the second op waits on the per-vault async-mutex. This code is reserved for non-blocking-conflict cases (e.g., terminate-while-create-in-flight where waiting would deadlock). Step-10 implementation should rarely emit this; step 11's pause-during-rescan is the more likely surface. Document the reservation. |

Validation cross-check: these codes are consistent with `src/api/error.rs`'s existing `ApiError` shape (`{status, code, message}` triple → JSON envelope). Task 10.3 extends `ApiError`'s constructors with the new codes; the existing `From<anyhow::Error>` mapping in `error.rs` continues to work for non-vault-specific failures (e.g., search-side `invalid_glob`).

**Cross-cutting note**: search responses also gain the `partial_results` diagnostic for paused/errored/failed vaults (§ A.6). This is **not** an HTTP error — it's a successful 200 response with a soft signal field. The HTTP error catalog above applies only to control-plane operations (`POST /vaults`, `DELETE /vaults/{id}`, etc.).

### E. Concurrency posture for control-plane operations

**Resolution**: **per-vault async-mutex** in a `VaultManager` struct that owns:

```rust
pub struct VaultManager {
    registry: Arc<VaultRegistry>,
    runners: Arc<RwLock<HashMap<VaultId, Arc<VaultRunner>>>>,
    config: Arc<Config>,
    embedder: Arc<dyn Embedder>,
    embedding_dimension: u32,
    data_dir: PathBuf,
}

pub struct VaultRunner {
    entry: Arc<VaultEntry>,                // immutable for the runner's lifetime
    op_lock: tokio::sync::Mutex<()>,       // serializes ops on this vault
    shutdown: WatcherShutdownHandle,        // for terminate
}
```

Read-side (`active_vaults()` for search handlers): read-lock on `runners`, clone-and-filter Arc<VaultEntry>s for active vaults. No mutex acquisition per search.

Write-side (`create`, `terminate`): write-lock on `runners` for the create-or-remove operation itself; the registry insert/delete and per-vault subdir creation/removal are inside the write-lock window. Per-vault operations that mutate vault state without changing the runner set (e.g., step-11's `pause`/`resume`/`reset`) take the per-vault `op_lock` instead, while the outer read-lock provides Arc-clone access.

**Why this shape over alternatives**:
- ADR-0010 commits to "operations on the same vault are serialized; operations on different vaults run in parallel." The two-mutex shape (outer RwLock + per-vault Mutex) implements both invariants cleanly.
- The actor-task variant (one tokio task per vault, channels for ops) is more complex without payoff: vault ops are infrequent (operator-initiated, not request-rate), so the per-op tokio-task-startup cost is irrelevant.
- The channel-with-id-key variant (one channel, dispatch by vault id) loses the natural read-side parallelism of search handlers.
- `tokio::sync::Mutex` (not std::sync::Mutex) is required because async control-plane ops await across the mutex boundary (registry SQL via `spawn_blocking`, fs ops, watcher shutdown).

**Construction lifecycle**:
- `VaultManager::open(config, registry, embedder, embedding_dim, data_dir)` is called once at daemon startup; it consumes the active-vault snapshot from the registry's `list_active()` and constructs a `VaultRunner` for each (which is what step 9's startup loop already does, just refactored into the manager).
- `VaultManager::create(req)` validates the path, mints a UUIDv7, inserts the registry row, creates the per-vault subdir + `meta.toml`, constructs a `VaultRunner`, inserts into the runner map.
- `VaultManager::terminate(name_or_id)` resolves the target, takes the write-lock, removes from the runner map, signals watcher/indexer shutdown via the runner's `WatcherShutdownHandle`, awaits drain (max 30s), removes the registry row, removes the per-vault subdir.

**Step-9 → step-10 ApiState refactor**: replace `ApiState.vaults: Arc<Vec<VaultEntry>>` with `ApiState.vault_manager: Arc<VaultManager>`. Search handlers call `vault_manager.active_vaults()` instead of iterating `s.vaults` directly. This is the **load-bearing refactor** of step 10's first wiring task; it preserves search behavior for N=1 while opening the door to dynamic vault count.

---

## Self-review for prose accuracy

This workplan is projected at ~900–1100 lines (larger than step-9's ~430 lines but smaller than the round-2 step-8 ~1100 lines that triggered the boundary heuristic). The heuristic does fire; running the spot-check on testable claims:

### Internal-shape claims (round-3-step-9 self-review addition)

1. **`ApiState.vaults: Arc<Vec<VaultEntry>>` is the current shape** (`src/api/mod.rs:33`). Workplan Task 10.2 reshapes this to `Arc<VaultManager>` with `active_vaults()` → `Vec<Arc<VaultEntry>>`. Verified by reading `src/api/mod.rs` at workplan-write — current shape matches the prescription. The reshape is a non-trivial refactor; Task 10.2 owns it.

2. **`src/api/search.rs` already iterates `s.vaults` and merges via `merge_and_truncate`** (`src/api/search.rs:33`, `:66`, `:105`). Step-10 Task 10.5 extends `merge_and_truncate` and the per-vault loop to apply the resolutions in § A. Verified at workplan-write: the iteration pattern is already in place; the additions are cross-vault sort, partial-results accumulation, and request-side `vaults` filter. No "the handler doesn't iterate yet" prose drift.

3. **`HypomnemaMcpServer` uses `tool_router` macro** (`src/mcp/server.rs:22`). Task 10.6's MCP tools follow the same pattern (one `#[tool]` fn per tool). Verified: `tool_router` is one impl block, `tool_handler` is another; conditional registration at construction time is not a documented `tool_router` feature in the rmcp 0.10 crate. Workplan commits to the simpler shape (b) in § C: register all four tools always, short-circuit write tools to `unknown_tool` error when `enable_write_tools = false`. Task 10.6 confirms-or-corrects against the current rmcp version at task time.

4. **`McpConfig` does not yet have `enable_write_tools`** (`src/config.rs:98`). Task 10.6 adds the field with `#[serde(default)] pub enable_write_tools: bool` and a default `true` via `default_enable_write_tools()`. No prose drift.

5. **`src/cli.rs::Command` enum** (`src/cli.rs:30`) currently has `Search`, `Status`, `Mcp`. Task 10.4 adds `Vault { #[command(subcommand)] op: VaultOp }` with the four shipping ops as `VaultOp` variants. No prose drift.

6. **`DaemonClient`** (`src/client.rs`) currently has `search_filesystem` / `search_content` / `search_semantic` (verified by inspection of `src/mcp/server.rs:31`/`:48`/`:65` callers). Task 10.3 / 10.4 add `create_vault` / `list_vaults` / `get_vault` / `terminate_vault` methods with the wire-shape mapping per § D's error codes. No prose drift.

7. **`src/legacy_state_migration.rs`** (step 9) handles legacy `[vault]` config-key migration. Step 10 does not modify this module. Soft-deprecation continues to fire on each startup until the operator removes the block (step-9 Resolution C); step 10 builds on top of an already-working legacy path.

### External-library claims

1. **rmcp tool registration with conditional advertisement**: claim — "register all four tools, short-circuit write tools when gated off." Task 10.6 verifies at task time by reading rmcp 0.10 docs and the `tool_router`/`tool_handler` macro source if needed. Fallback: if always-register-but-gate proves to violate the MCP protocol contract (i.e., listing a tool that always errors confuses agent UX), fall back to two-impl-block conditional registration with one extra task-time investigation step. Forward note for Task 10.6: explicitly verify-or-correct this prediction in your results comment per the round-2 step-8 task-8.2 → 8.3 prediction-vs-observation pattern.

2. **`axum::Router::route` with path parameter**: `route("/vaults/{name_or_id}", get(...))` is documented. Step 5 already uses path parameters for `/files/...` (verify by inspection at workplan-write). **Confirmed**: `src/api/mod.rs:38–46` shows axum's static-route style; `axum::extract::Path` extractor for `{name_or_id}` is the standard idiom.

3. **`std::fs::canonicalize` semantics**: returns the absolute, symlink-resolved path; errors if the path doesn't exist. Used at `vault create` request-validation time per § D. Path-must-exist behavior is what we want (a vault path that doesn't exist is a configuration error, not a runtime errored-state — that's what `errored` is for, when a previously-good path becomes inaccessible). Documented.

4. **`tokio::sync::Mutex` vs `std::sync::Mutex` for async ops**: `tokio::sync::Mutex` is required when the lock is held across `.await`. § E's per-vault `op_lock: Mutex<()>` is `tokio::sync::Mutex` (not std). No prose drift.

5. **clap subcommand under nested subcommand**: `hmn vault create [...]` requires `Command::Vault { op: VaultOp }` with `VaultOp::Create { name: Option<String>, path: PathBuf }` etc. clap supports this (verified by reading the existing `Command::Search { mode: SearchMode }` pattern in `src/cli.rs:31`). No drift.

### Cross-platform claims

1. **`std::fs::canonicalize` behavior on macOS / Linux** (the supported platforms per round 1): symlinks resolved, `..` collapsed. On Windows, `canonicalize` returns UNC-prefixed paths (`\\?\C:\foo`) which can confuse downstream code; round-3 inherits round-1's macOS+Linux scope. If a Windows operator surfaces, revisit.

2. **`tokio::sync::Mutex` Drop behavior**: dropping a held mutex without explicit `unlock` releases it. Used implicitly throughout the workplan; this is canonical Rust. No verification needed.

The cross-platform-rename safety claim from step 9 (Resolution B) is documented as a step-9 boundary follow-up; step 10 does not introduce new rename-based logic.

---

## Tasks

The 8-task decomposition follows the round-1/2/3 pattern (default-not-batch; 8-task density matches steps 5 / 6 / 9). Each task ships its own commit per the playbook's TASK AGENT § Reporting; risk grades and dependencies noted at each task header.

### Task 10.1 — Spec amendments + vault-management spec fleshout (closes Solo todos 64 + 65)

**Risk**: low. Doc-only by design; closes the round-3 step-9 "ahead-of-spec" gap that step-9 intentionally deferred. Lands first so subsequent build tasks ship against committed spec text.

**Scope**:
- Apply the cross-vault search semantics resolutions in § A to:
  - `docs/specs/filesystem-search.md` — version 0.1.0 → 0.2.0. Update per-result `vault?: string` semantics from "always absent in v0" to "populated when multi-vault is active (round 3+)". Add per-result `vault_name?: string`. Add request-side `vaults?: string[]` filter. Update Behavior section with cross-vault default ordering (§ A.1: global path-asc with vault-id tie-break). Add response-envelope `partial_results?` field per § A.6. Add Open Question for pagination across N independent indexes (deferred per § A.3). Revision History entry dated at ship date.
  - `docs/specs/content-search.md` — same shape as filesystem-search; same ordering resolution per § A.1 (lift per-vault path-sorted to global).
  - `docs/specs/semantic-search.md` — same shape; ordering is score-desc per § A.2; same Open Question for pagination; spec the multi-model-embedding-per-vault assumption per § A.2.
  - `docs/specs/change-events.md` — version 0.1.1 → 0.2.0. Update `vault?: string` semantics: surrogate ID only, populated in multi-vault. **No `vault_name`** (outbox is durable; names rot — per ADR-0009). Update Consumer Model and File Format sections to reference per-vault outbox path `<data_dir>/vaults/<id>/outbox.jsonl`. Update Intentional Reset edge case with a per-vault wrinkle (resetting one vault's outbox vs. all). Revision History entry dated at ship date.
- Flesh `docs/specs/vault-management.md` from outline (v0.1.0, "Draft") to full spec (v1.0.0, "Approved") per § B. Use the `spec-generator` skill. Pin all step-9 + step-10 resolutions in spec text:
  - § Identifier Model — UUIDv7 (lifted from Open Questions; `vault_<uuid>` user-facing prefix is display-only).
  - § Behavior — full operations table (all nine ops fully specified, even though step 10 ships only four).
  - § Behavior § Cross-Vault Search Semantics — § A's eight resolutions land here, in spec text.
  - § Behavior § Concurrency — § E's per-vault async-mutex shape.
  - § Behavior § MCP Tool Surface — § C's gating story.
  - § Data Schema § Registry — UUIDv7 storage form; `vaults.sqlite` single-CREATE-TABLE schema (no migrations module; from step 9 Resolution D).
  - § Data Schema § Per-Vault Layout — `<data_dir>/vaults/<id>/{index.sqlite,outbox.jsonl,meta.toml}` (already in outline; preserved verbatim).
  - § Data Schema § Control-Plane HTTP Wire Shapes — full request/response shapes for the four step-10 ops; step-11 ops noted as "specified for forward-compat; ships in step 11."
  - § Edge Cases — preserve outline's edge cases; add the cross-vault search-side edge cases from § A (paused-vault inclusion, errored-vault inclusion, partial-results diagnostic).
  - § Error Handling — full table per § D.
  - § Integration Points — preserve outline's content; refresh against fleshed Behavior.
  - § Open Questions — preserve only round-4+ items: pagination/cursor, streaming responses, Compose-file format, multi-model-embedding-per-vault, cross-platform rename safety. Remove resolved items (ID format, MCP gating, cross-vault-search-default-scope, etc.).
  - § Revision History — `1.0.0 | 2026-04-28 (or ship date) | Fleshed from outline; commits step-10 workplan resolutions.`
- Cross-reference each amended search spec to vault-management.md § Behavior § Cross-Vault Search Semantics (so the four search specs don't duplicate cross-vault prose).

**Tests**:
- Doc-only; no code tests in this task.
- Lint check: `cargo doc --no-deps` if any rustdoc references the spec files (none anticipated; doc-side refresh).

**Files touched**: `docs/specs/filesystem-search.md`, `docs/specs/content-search.md`, `docs/specs/semantic-search.md`, `docs/specs/change-events.md`, `docs/specs/vault-management.md`.

**Dependencies**: none (lands first).

**Soft-flag-ready territory**: spec-generator output prose-vs-resolution drift is a likely soft-flag shape — the workplan's resolutions in § A–E are load-bearing; spec-generator's expansion may add prose around them that needs minor coordinator-time correction. Default to coordinator-only soft flags per the round-3-step-9 workplan-prose-vs-load-bearing-decision pattern.

### Task 10.2 — `src/control_plane/` module + `VaultManager` + per-vault async-mutex

**Risk**: medium-high. **Load-bearing for tasks 10.3–10.7.** Reshapes `ApiState` (the current `Arc<Vec<VaultEntry>>` becomes `Arc<VaultManager>`); introduces the per-vault async-mutex shape; consolidates the runner-lifecycle code that step 9 spread across `src/bin/hmnd.rs` startup-sequence + `legacy_state_migration` + `vault_registry`.

**Scope**:
- New module: `src/control_plane/{mod.rs,manager.rs,runner.rs}` (or equivalent decomposition; coordinator/agent decides at task time).
- Public surface (sketch):
  ```rust
  pub struct VaultManager { /* per § E */ }
  pub struct VaultRunner { /* per § E */ }
  
  pub struct CreateVaultRequest {
      pub name: Option<String>,           // None → resolves to default_vault_name
      pub path: PathBuf,                  // must be absolute or expandable; canonicalized at create
  }
  
  #[derive(Debug, thiserror::Error)]
  pub enum ControlPlaneError {
      VaultNotFound { name_or_id: String, hint: Option<String> },
      VaultPathConflict { existing_name: String, path: PathBuf },
      VaultNameConflict { existing_path: PathBuf, name: String },
      VaultPathInvalid { detail: String },
      VaultErrored { name_or_id: String, last_error: Option<String> },  // reserved for step 11
      VaultOpConflict { detail: String },                                // reserved
      RegistryCorrupt { detail: String },
      Internal(anyhow::Error),
  }
  
  impl VaultManager {
      pub async fn open(/* ... */) -> Result<Self, anyhow::Error>;
      pub async fn create(&self, req: CreateVaultRequest) -> Result<VaultRow, ControlPlaneError>;
      pub async fn list(&self) -> Result<Vec<VaultRow>, ControlPlaneError>;
      pub async fn get(&self, name_or_id: &str) -> Result<VaultRow, ControlPlaneError>;
      pub async fn terminate(&self, name_or_id: &str) -> Result<(), ControlPlaneError>;
      pub fn active_vaults(&self) -> Vec<Arc<VaultEntry>>;
      pub fn resolve(&self, name_or_id: &str) -> Result<VaultId, ControlPlaneError>;  // helper for handlers
  }
  ```
- `create()` flow:
  1. Validate request: `path.is_absolute()` or expand `~`; `canonicalize()` the path; reject if `data_dir` is under the canonicalized vault path.
  2. Resolve name: if `req.name.is_none()`, use `config.default_vault_name`; if that is empty (Resolution C exception), return `VaultPathInvalid { detail: "name is required when default_vault_name is empty" }`.
  3. Acquire write-lock on `runners`.
  4. Check name + path uniqueness against the current registry (registry has UNIQUE constraints, but pre-check for a clean error envelope vs. a constraint-violation anyhow chain).
  5. Mint UUIDv7 vault id; insert registry row.
  6. Create `<data_dir>/vaults/<id>/` subdir; write `meta.toml`.
  7. Construct `VaultRunner`: open the per-vault `Store`, start watcher + indexer (the same startup-sequence-step-5 pattern from step 9, refactored into the runner constructor).
  8. Insert into `runners` map.
  9. Release write-lock; return the inserted `VaultRow`.
- `terminate()` flow:
  1. Acquire write-lock on `runners`.
  2. Resolve `name_or_id` to a `VaultId` (error envelope `VaultNotFound` if missing).
  3. Remove from `runners`; signal watcher + indexer shutdown via `WatcherShutdownHandle`; await drain (max 30s — beyond which we force-stop and continue).
  4. Delete registry row.
  5. Remove `<data_dir>/vaults/<id>/` subdir (using `std::fs::remove_dir_all`; same-filesystem assumption inherited from step 9).
  6. Release write-lock; return `Ok(())`.
- `active_vaults()` is sync: read-lock on `runners`, filter by `entry.status == VaultStatus::Active`, clone-and-collect Arcs.

- **Crash safety for `create`**: ADR-0010 § Reconciliation pattern + step 9 Resolution B's idempotency. If `create` crashes between step 5 (registry row inserted) and step 7 (subdir created), next-startup reconcile sees registry row + missing subdir → recreates the subdir as part of step-9's reconcile pass. Step-9's reconcile already handles this for the legacy-migration path; verify it covers the new control-plane path too at Task 10.7's integration test.
- **Crash safety for `terminate`**: similarly, if terminate crashes between step 4 (registry row deleted) and step 5 (subdir removed), the orphan subdir is harmless until next-startup reconcile detects "subdir without registry row" → removes it. **Step-9's reconcile may not currently handle this case**; Task 10.2 adds the reconcile-orphan-subdirs pass if missing. Verify at workplan-write-or-task-time.

- **Refactor**: replace `ApiState.vaults: Arc<Vec<VaultEntry>>` with `ApiState.vault_manager: Arc<VaultManager>`. Update search handler iteration in `src/api/search.rs` to call `s.vault_manager.active_vaults()` instead of iterating `s.vaults`. Update `hmnd.rs` startup-sequence to construct `VaultManager` instead of building the static `Vec<VaultEntry>`.

**Tests** (in-module unit tests):
- `vault_manager_open_loads_active_runners` — open with a registry containing 2 active + 1 paused row; confirm `active_vaults()` returns 2.
- `create_inserts_row_subdir_and_runner` — create with a fresh path; confirm registry row, subdir, runner-map entry.
- `create_rejects_path_already_registered` — second `create` against the same path returns `VaultPathConflict`.
- `create_rejects_name_already_in_use` — second `create` against the same name with a different path returns `VaultNameConflict`.
- `create_rejects_data_dir_under_vault_path` — `data_dir = /tmp/data; vault path = /tmp; data_dir is under vault path` → `VaultPathInvalid`.
- `create_resolves_default_name_when_omitted` — `req.name = None`, `config.default_vault_name = "personal"` → row inserted with `name = "personal"`.
- `create_rejects_when_default_name_empty_and_no_explicit_name` — `req.name = None`, `config.default_vault_name = ""` → `VaultPathInvalid` per Resolution C exception.
- `terminate_removes_runner_row_and_subdir` — terminate on an existing vault; confirm runner-map entry gone, registry row gone, subdir removed.
- `terminate_returns_vault_not_found_for_unknown` — terminate against a non-existent name → `VaultNotFound { hint: ... }`.
- `terminate_then_create_with_same_name_succeeds` — ADR-0010 § Idempotency — terminate, then create with the same name + path → succeeds, fresh UUIDv7, fresh subdir.
- `concurrent_creates_on_different_names_dont_block` — spawn two creates in parallel; both complete; both rows present.
- `concurrent_terminate_on_same_vault_serializes` — spawn two terminates on the same vault; one succeeds, the other returns `VaultNotFound` (the first removed it). (Per-vault op_lock isn't needed for this case because the outer write-lock provides serialization for runner-map mutations; the op_lock is for step-11 ops that read-lock the outer map and op-mutate inside.)

**Files touched**: `src/control_plane/{mod.rs,manager.rs,runner.rs}` (new), `src/api/mod.rs` (refactor `ApiState`), `src/api/search.rs` (update iteration call site), `src/bin/hmnd.rs` (refactor startup-sequence to use `VaultManager`), `src/lib.rs` (re-export `control_plane`), possibly small adjustments in `src/legacy_state_migration.rs` if the orphan-subdir reconcile pass needs to live there.

**Dependencies**: 10.1 (committed spec text is referenced by the module's docstrings; nice-to-have, not load-bearing for the code).

**Soft-flag-ready territory**:
- The `WatcherShutdownHandle` shape may need refactoring to support cooperative drain with timeout; if step 9's existing handle doesn't have a drain primitive, surface as a `coordinator-only` soft flag with the chosen replacement.
- The orphan-subdir reconcile pass (for terminate-crash-safety) may surface as a workplan-prose-vs-shipped-shape drift if step-9's reconcile already handles it implicitly (i.e., if step-9's reconcile only knows about missing-subdir-with-row-present, not the inverse). Coordinator-only soft flag.
- Manager API changes that ripple wider than the workplan's "Files touched" list (e.g., test fixtures in `tests/multi_vault_internal.rs` that constructed `Vec<VaultEntry>` directly need to migrate to `VaultManager`-construction). Coordinator-only soft flag per the round-3-step-9 stable pattern.

### Task 10.3 — HTTP control-plane routes

**Risk**: medium. Mostly serde plumbing on top of Task 10.2's `VaultManager`. Tests are unit-level against the route surface.

**Scope**:
- New module: `src/api/vaults.rs` (or equivalent — colocated with `api/search.rs` per the existing module structure).
- Routes:
  - `POST /vaults` — body `{name?: string, path: string}`; response `{id, name, path, status, created_at, last_error?}` (`200 OK` or `409 vault_path_conflict` / `409 vault_name_conflict` / `422 vault_path_invalid`).
  - `GET /vaults` — response `{vaults: [{id, name, path, status, created_at, last_error?}, ...]}`.
  - `GET /vaults/{id_or_name}` — response single vault row (`200 OK` or `404 vault_not_found`).
  - `DELETE /vaults/{id_or_name}` — response `{terminated: true, id: <id>}` (`200 OK` or `404 vault_not_found`).
- Wire `ControlPlaneError` → `ApiError` per § D's error code table. Extend `ApiError` constructors with the new codes (`vault_not_found`, `vault_path_conflict`, etc.). Consider a `From<ControlPlaneError> for ApiError` impl in `src/api/error.rs`.
- The `closest-name hint` in `vault_not_found` is a Levenshtein-distance lookup against the current name list — small cost (1 `O(N)` scan over registry names). If no candidate is within distance 3, omit the hint. Implement in `VaultManager::get` / `VaultManager::resolve` as a side-channel and propagate to the error envelope.

**Tests** (extend `src/api/tests.rs` and add a `tests/vault_control_plane.rs` integration test scaffold; the integration tests proper land in Task 10.7):
- Unit-level handler tests:
  - `post_vaults_returns_200_on_create`.
  - `post_vaults_returns_409_on_path_conflict`.
  - `post_vaults_returns_409_on_name_conflict`.
  - `post_vaults_returns_422_on_invalid_path`.
  - `get_vaults_returns_list`.
  - `get_vaults_id_returns_single`.
  - `get_vaults_unknown_returns_404_with_hint`.
  - `delete_vaults_returns_200_on_terminate`.
  - `delete_vaults_unknown_returns_404`.
- Error-envelope shape pinned: `{"error": {"code": "vault_path_conflict", "message": "..."}}` matching v0/round-2 shape.

**Files touched**: `src/api/vaults.rs` (new), `src/api/mod.rs` (router wiring), `src/api/error.rs` (new ApiError constructors + From impl), `src/api/types.rs` (request/response types — `CreateVaultRequest`, `VaultListResponse`, etc.), `src/api/tests.rs`.

**Dependencies**: 10.2.

### Task 10.4 — `hmn vault` CLI subcommands + DaemonClient extension

**Risk**: medium. **First user-mutable surface**; the CLI is the primary way an operator interacts with the daemon's mutable state. Manual smoke is bundled into Task 10.7, not here, because the CLI's manual-feel quality is best assessed against the full multi-vault setup the integration tests exercise.

**Scope**:
- Extend `src/cli.rs::Command` with `Vault { #[command(subcommand)] op: VaultOp }`. New `VaultOp` enum:
  ```rust
  #[derive(Debug, Subcommand)]
  pub enum VaultOp {
      Create {
          /// Path to the vault directory. Must exist and be canonicalizable.
          path: PathBuf,
          /// Vault name. Defaults to config's default_vault_name.
          #[arg(long)]
          name: Option<String>,
      },
      List,
      Status {
          /// Vault name or surrogate ID. Defaults to default_vault_name when omitted.
          target: Option<String>,
      },
      Terminate {
          /// Vault name or surrogate ID.
          target: String,
          /// Skip the destructive-op confirmation prompt.
          #[arg(long)]
          yes: bool,
      },
  }
  ```
- Subcommand handlers in `src/bin/hmn.rs` (or equivalent — the existing `hmn` entry point):
  - `Create`: call `DaemonClient::create_vault({name, path})`; render result as a single-row table or JSON depending on `--json`.
  - `List`: call `DaemonClient::list_vaults()`; render rows as a table (`id | name | path | status | created_at`) or JSON.
  - `Status`: call `DaemonClient::get_vault(target_or_default)`; render single-vault detail (table or JSON).
  - `Terminate`: prompt `"Terminate vault '<name>'? (y/N) "` unless `--yes` is passed; if confirmed, call `DaemonClient::terminate_vault(target)`; render `{terminated: true, id: ...}` (table or JSON).
- Extend `DaemonClient` (`src/client.rs`) with the four new methods. Each is a thin reqwest wrapper matching `search_filesystem`/etc.'s pattern. Error-envelope passthrough (the daemon-side `ApiError` JSON envelope deserialized into anyhow-chain-with-stable-prefix per `src/api/error.rs`'s pattern).
- Confirmation prompt is stdin-attached; `--yes` skips it. CI / non-interactive use must pass `--yes`.

**Tests** (extend `src/cli.rs::tests` for parsing; `tests/cli.rs` for end-to-end against a live daemon):
- `parses_vault_create_with_path` — `hmn vault create /tmp/foo`.
- `parses_vault_create_with_name_and_path` — `hmn vault create --name=personal /tmp/foo`.
- `parses_vault_list` — `hmn vault list`.
- `parses_vault_status_with_target` — `hmn vault status personal`.
- `parses_vault_status_without_target` — `hmn vault status`.
- `parses_vault_terminate_with_yes` — `hmn vault terminate personal --yes`.
- E2E (in `tests/cli.rs`):
  - `hmn_vault_create_then_list_returns_the_new_vault`.
  - `hmn_vault_terminate_with_yes_succeeds`.
  - `hmn_vault_terminate_without_yes_prompts_and_aborts_on_no` — pipe `n\n` to stdin; assert no termination.

**Files touched**: `src/cli.rs`, `src/bin/hmn.rs`, `src/client.rs`, possibly `src/api/types.rs` (re-exports for client).

**Dependencies**: 10.3.

### Task 10.5 — Cross-vault search refinements (`vaults` filter + `partial_results` + ordering per § A)

**Risk**: medium. Composes Task 10.2's `VaultManager.active_vaults()` + the eight resolutions in § A into the search handler iteration shape. **Critical for round-3's `vault` field to be load-bearing rather than scaffolding**: this is where multi-vault search semantics actually become correct.

**Scope**:
- Extend request types in `src/api/types.rs`:
  - `FilesystemQueryJson`, `ContentQueryJson`, `SemanticQueryJson` each get `pub vaults: Option<Vec<String>>`.
- Extend `src/cli.rs::SearchMode` with a `--vaults` flag on each search-mode variant: `#[arg(long, value_delimiter = ',')] vaults: Vec<String>`. The CLI threads the value into the corresponding `*QueryJson::vaults` field via `DaemonClient`. Empty vec (no flag) → `vaults: None` in the request; non-empty vec → `vaults: Some(vec)`.
- Extend response types:
  - `FilesystemSearchResponse`, `ContentSearchResponse`, `SemanticSearchResponse` each get `pub partial_results: Option<PartialResults>`.
  - `PartialResults { skipped: Vec<SkippedVault>, failed: Vec<FailedVault> }`.
  - `SkippedVault { vault: String, vault_name: String, status: String, reason: String }`.
  - `FailedVault { vault: String, vault_name: String, code: String, message: String }`.
  - The `partial_results` field uses `#[serde(skip_serializing_if = "Option::is_none")]` so v0/step-9 wire bytes are unchanged when no skips/failures occur.
- Update `src/api/search.rs::filesystem`/`content`/`semantic` handlers per § A:
  - Resolve `req.vaults` filter against `manager.active_vaults()` — if `Some(names)`, narrow to the named subset; unknown names produce `failed` entries; empty array errors `invalid_request`.
  - For each in-scope vault: if `status != Active`, skip and append to `partial_results.skipped`; otherwise run the per-vault search; on per-vault error, append to `partial_results.failed` and continue.
  - Merge per-vault result lists per the mode-specific ordering (§ A.1 path-asc for filesystem/content; § A.2 score-desc for semantic).
  - Vault-id tie-break for equal sort keys (§ A.1, § A.2).
  - Apply global `limit` truncation on the merged list per § A.4.
  - Set `truncated: true` when any per-vault search reported truncation **OR** the merged list was capped at `limit`.
  - Set `partial_results: Some(...)` when any skipped or failed; `None` otherwise.
- Update `merge_and_truncate` (or replace with a richer helper) to accept the partial-results accumulator and the ordering function.

**Tests** (extend `src/api/tests.rs` + add cross-vault cases):
- `cross_vault_filesystem_results_global_path_sorted` — set up 2 vaults with overlapping path patterns; confirm merged results are in global path-asc order.
- `cross_vault_content_results_global_path_sorted` — same shape.
- `cross_vault_semantic_results_score_desc_sorted` — set up 2 vaults; confirm score-desc with vault-id tie-break.
- `vaults_filter_narrows_to_subset_by_name`.
- `vaults_filter_narrows_to_subset_by_id`.
- `vaults_filter_unknown_name_appears_in_partial_results_failed`.
- `vaults_filter_empty_array_returns_invalid_request`.
- `paused_vault_skipped_with_partial_results_diagnostic`.
- `errored_vault_skipped_with_last_error_propagated`.
- `partial_results_omitted_when_all_active`.
- `truncated_true_when_global_limit_capped_after_merge`.
- `cross_vault_path_collision_breaks_tie_by_vault_id`.
- `merged_limit_applied_after_per_vault_search` — set `limit=2`, two vaults each return 2 results → merged list of 4 truncated to 2; `truncated: true`.
- `semantic_search_assumes_same_dimension_across_vaults` — sanity-check: if a per-vault `search_semantic` errors on dimension mismatch (which shouldn't happen in practice but is defensible), the error is propagated to `partial_results.failed` rather than crashing the whole query.

**Files touched**: `src/api/types.rs`, `src/api/search.rs`, `src/cli.rs` (search-mode `--vaults` flag), `src/bin/hmn.rs` (thread CLI flag into request), possibly `src/api/error.rs` (new validation error for empty `vaults` array).

**Dependencies**: 10.2.

### Task 10.6 — MCP tools (`vault_list`, `vault_status`, `vault_create`, `vault_terminate`) + write-tool gating

**Risk**: medium. Builds on the round-2 step-8 MCP wrapper (`src/mcp/server.rs`); the new tool registrations follow the existing `tool_router` macro pattern. The gating story (§ C) is the new design surface; verify-or-correct the conditional-registration approach against rmcp at task time.

**Scope**:
- Extend `src/mcp/server.rs::HypomnemaMcpServer` with four new `#[tool]` methods. Each is a thin shim over the corresponding `DaemonClient` method (Task 10.4 added the methods):
  - `vault_list(&self) -> CallToolResult` — calls `client.list_vaults()`; structured response.
  - `vault_status(&self, Parameters(VaultStatusInput)) -> CallToolResult` — input `{target: Option<String>}`; calls `client.get_vault(target)`.
  - `vault_create(&self, Parameters(VaultCreateInput)) -> CallToolResult` — input `{name: Option<String>, path: String}`; calls `client.create_vault(...)`. **Gated by `enable_write_tools`**.
  - `vault_terminate(&self, Parameters(VaultTerminateInput)) -> CallToolResult` — input `{target: String}`; calls `client.terminate_vault(target)`. **Gated by `enable_write_tools`**.
- Add `enable_write_tools: bool` to `McpConfig` (`src/config.rs`):
  ```rust
  pub struct McpConfig {
      // ... existing fields ...
      #[serde(default = "default_enable_write_tools")]
      pub enable_write_tools: bool,
  }
  fn default_enable_write_tools() -> bool { true }
  ```
- Implementation pattern per § C self-review item 1: register all four tools always; the write tools' fn bodies short-circuit to a structured error envelope `{"error": {"code": "write_tools_disabled", "message": "vault.create/terminate are disabled by config; set [mcp] enable_write_tools = true"}}` when `self.enable_write_tools == false`. Forward note for Task 10.6 task agent: **explicitly verify this approach against current rmcp behavior in your results comment** per round-2 step-8 prediction-vs-observation pattern. If rmcp's `tool_router` macro doesn't allow conditional registration cleanly, the always-register-with-short-circuit shape is the fallback.
- Tool descriptions follow the round-2 step-8 convention — concise, references the relevant spec section. Examples:
  - `vault_list`: `"List all registered vaults with their status, path, and creation time. See docs/specs/vault-management.md § Operations."`
  - `vault_status`: `"Get detail for a single vault by name or ID. Defaults to the configured default vault when target is omitted. See docs/specs/vault-management.md § Operations."`
  - `vault_create`: `"Create a new vault. Path must be absolute and exist. Name defaults to the configured default name. Disabled when [mcp] enable_write_tools = false. See docs/specs/vault-management.md § Operations § create."`
  - `vault_terminate`: `"Permanently remove a vault from the registry; deletes its index and event log; never touches the vault directory itself. Disabled when [mcp] enable_write_tools = false. See docs/specs/vault-management.md § Operations § terminate."`
- The `HypomnemaMcpServer::new` constructor (or whatever it currently is) accepts the gating flag from config.

**Tests** (extend `src/mcp/server.rs::tests` per the existing MockDaemon pattern):
- `mcp_vault_list_returns_list` — mock daemon `GET /vaults`; tool returns structured response.
- `mcp_vault_status_returns_single` — mock daemon `GET /vaults/{id}`; tool returns single-vault response.
- `mcp_vault_create_succeeds_when_write_tools_enabled` — `enable_write_tools = true`; mock daemon `POST /vaults`; tool returns structured response.
- `mcp_vault_create_returns_write_tools_disabled_when_gated` — `enable_write_tools = false`; tool returns the disabled-tool error envelope.
- `mcp_vault_terminate_succeeds_when_write_tools_enabled` — analogous to create.
- `mcp_vault_terminate_returns_write_tools_disabled_when_gated`.
- `mcp_vault_list_propagates_daemon_unreachable_envelope` — mock unbound port; tool returns `daemon_unreachable` (already covered by existing search-tool tests' pattern).

**Files touched**: `src/mcp/server.rs`, `src/config.rs`, `src/api/types.rs` (input types for the new MCP tools — `VaultStatusInput`, `VaultCreateInput`, `VaultTerminateInput`), `src/mcp/mod.rs` (constructor wiring if applicable).

**Dependencies**: 10.3 (shares request/response types), 10.4 (DaemonClient methods).

**Soft-flag-ready territory**: the conditional-registration approach is the prediction; if rmcp's `tool_router` macro turns out to support it cleanly via a builder pattern (worth a quick check of the macro source), surface as a `next-task-agent` soft flag with the alternative shape. Otherwise the always-register-with-short-circuit shape is canonical.

### Task 10.7 — Integration tests + manual smoke verification

**Risk**: medium-high. **Manual smoke verification is load-bearing here** per the round-2 step-7 / round-2 step-8 / round-3 step-9 precedent for medium-high-risk wiring tasks. Composes 10.2–10.6 against a real two-vault setup over both HTTP and CLI.

**Scope**:
- New integration test file: `tests/vault_control_plane.rs` (or extend `tests/multi_vault_internal.rs`; Coordinator decides at task time based on test surface size).
- Tests:
  - `http_create_vault_succeeds_and_appears_in_list` — `POST /vaults` then `GET /vaults`.
  - `http_create_vault_path_conflict_returns_409` — second create with the same path.
  - `http_create_vault_name_conflict_returns_409` — second create with the same name.
  - `http_create_vault_invalid_path_returns_422` — non-absolute or non-existent path.
  - `http_get_vault_unknown_returns_404_with_hint` — closest-match hint on near-miss.
  - `http_delete_vault_succeeds_and_removes_from_list`.
  - `http_delete_vault_unknown_returns_404`.
  - `http_terminate_then_create_with_same_name_succeeds` — ADR-0010 § Idempotency at the HTTP surface.
  - `http_terminate_removes_per_vault_subdir`.
  - `concurrent_creates_on_different_names_succeed_in_parallel` — two POST /vaults in parallel, both 200.
  - `concurrent_terminate_on_same_vault_one_404s` — second terminate gets `vault_not_found`.
  - `cross_vault_filesystem_search_returns_intermingled_global_path_sorted` — 2 vaults with seeded files; confirm path-asc across vaults.
  - `cross_vault_content_search_returns_intermingled_global_path_sorted`.
  - `cross_vault_semantic_search_returns_intermingled_score_desc_sorted` — requires the test embedding service from step-7.
  - `vaults_filter_narrows_subset` — request with `vaults: ["personal"]`; only personal-vault results.
  - `paused_vault_skipped_in_default_scope_with_partial_results_diagnostic` — directly insert a registry row with `status=paused`; confirm search response has `partial_results.skipped[0].status = "paused"`.
  - `errored_vault_skipped_with_last_error_propagated`.
- 3× consecutive flake-check clean run per the round-1/2/3 anti-flake convention.
- **Manual smoke verification** (per round-2/3 lessons; bundled into this task):
  1. **Two-vault create-and-search smoke**: empty `<data_dir>`, no `[vault]` config, `default_vault_name = "default"`. Daemon starts, idles. `hmn vault create /path/to/vault-a` → vault-a created. `hmn vault create --name=vault-b /path/to/vault-b` → vault-b created. Seed both vaults with overlapping markdown files (e.g., `notes/x.md` in vault-a and `notes/x.md` in vault-b with different content). Wait for indexing (~5s typical). `hmn search content "shared-keyword"` → results from both vaults with `vault_name` annotations; ordering is global path-asc with vault-id tie-break; `partial_results` omitted (both active).
  2. **Cross-vault filter smoke**: same setup as (1). `hmn search content "shared-keyword" --vaults vault-a` (assuming `--vaults` makes it onto the CLI per Task 10.4; if not, via raw `curl`) → results only from vault-a.
  3. **Terminate-then-recreate smoke**: same setup. `hmn vault terminate vault-b --yes` → vault-b removed; `hmn vault list` shows only vault-a; `<data_dir>/vaults/<vault-b-id>/` is gone. `hmn vault create --name=vault-b /different/path` → vault-b reborn with fresh UUIDv7; search returns results from the fresh vault as it indexes.
  4. **Errored-vault smoke**: directly insert a registry row with a non-existent path (or use `hmn vault create` against a path that's then unmounted). Restart daemon; the row enters `errored` status with `last_error` populated. `hmn search content "anything"` → `partial_results.skipped` contains the errored vault with `last_error` propagated; results from active vaults still returned.
  5. **MCP tool roundtrip smoke** (optional; round-4 prep — only if Claude Code is available locally and step-8's MCP setup still works): `claude` invokes `vault_list` and gets a structured response with both vaults. `vault_create` either works (default config) or returns `write_tools_disabled` (with `[mcp] enable_write_tools = false` set in config).

  Document each smoke run's transcript in the task's results comment per the round-2 step-7/8 / round-3 step-9 precedent.

**Files touched**: `tests/vault_control_plane.rs` (new) or extension of `tests/multi_vault_internal.rs`.

**Dependencies**: 10.3 (HTTP routes), 10.4 (CLI for smoke), 10.5 (cross-vault refinements), 10.6 (MCP tools — light-touch in tests since the round-2 step-8 mock-daemon pattern covers the unit-level cases).

### Task 10.8 — Reference docs + roadmap-3.md status

**Risk**: low. Doc-only by design; lands at boundary so any forward-noted soft-flag corrections from earlier tasks can be incorporated.

**Scope**:
- `docs/reference/cli.md`: full `hmn vault` documentation. New section under "Commands":
  - `hmn vault create [--name NAME] PATH` — full flag reference + examples.
  - `hmn vault list` — output table format + `--json` mode.
  - `hmn vault status [TARGET]` — default-name resolution behavior.
  - `hmn vault terminate TARGET --yes` — confirmation flow + `--yes` flag.
  - Cross-reference to `docs/specs/vault-management.md` for full operation semantics.
  - Update existing search subcommand docs with the new `--vaults` filter shipped in Task 10.5; comma-separated value semantics; behavior on unknown name (appears in `partial_results.failed`).
- `docs/reference/configuration.md`: 
  - Add `[mcp] enable_write_tools = true` (default) under § `[mcp]`. Document the gate: when `false`, `vault_create` and `vault_terminate` MCP tools return a structured `write_tools_disabled` error.
  - Verify `default_vault_name` documentation from step 9 still accurate.
  - Document the per-vault data layout (already done in step 9; verify accurate).
- `docs/architecture/overview.md`: 
  - Add `Vault Manager / Control Plane` section describing the per-vault async-mutex shape, the runner-map, and the create/terminate lifecycle.
  - Update § Search API to remove the "step-9 ahead of spec" footnote (closed in Task 10.1).
  - Update § Storage to mention the orphan-subdir reconcile pass added in Task 10.2 (if applicable).
- Update `notes/roadmap/roadmap-3.md` § Step 10 status: add `**Status**: Shipped <date>` at top of step's section; cross-reference the workplan archive path.
- Verify `notes/roadmap/step-10-workplan.md` is up-to-date with shipping criteria; archive moves at boundary ritual time.

**Files touched**: `docs/reference/cli.md`, `docs/reference/configuration.md`, `docs/architecture/overview.md`, `notes/roadmap/roadmap-3.md`. (Workplan archive is the boundary ritual itself, run by the coordinator after this task ships.)

**Dependencies**: 10.1–10.7. Lands last.

---

## Shipping criteria

The step ships when **all** of these hold:

- [ ] All step-9 integration tests pass unchanged: `tests/scan.rs`, `tests/watch.rs`, `tests/outbox.rs`, `tests/embedding.rs`, `tests/mcp.rs`, `tests/multi_vault_internal.rs`, plus skeleton/config tests. Existing single-vault and step-9-internal-multi-vault behavior is fully preserved.
- [ ] `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test` all green.
- [ ] `hmn vault create [--name NAME] PATH` against a running daemon creates a vault: registry row inserted, per-vault subdirectory created with `index.sqlite` + `outbox.jsonl` + `meta.toml`, watcher + indexer started.
- [ ] `hmn vault list` returns the registered vaults with `{id, name, path, status, created_at, last_error?}`.
- [ ] `hmn vault status [TARGET]` returns single-vault detail; defaults to `default_vault_name` when target omitted.
- [ ] `hmn vault terminate TARGET --yes` removes the registry row, stops watcher + indexer, removes the per-vault subdirectory; never touches the vault path's own files. Without `--yes`, prompts and aborts on `n`. Terminate-then-create with the same name succeeds.
- [ ] `curl -X POST http://127.0.0.1:7777/vaults -d '{"path":"~/foo"}'` creates a vault over HTTP and returns the `{id, name, path, status, created_at}` shape; HTTP error envelopes for `vault_path_conflict` (409), `vault_name_conflict` (409), `vault_path_invalid` (422), `vault_not_found` (404 with hint), `registry_corrupt` (500) match § D.
- [ ] `hmn search content "X"` against a daemon with two vaults returns intermingled results with `vault` (id) + `vault_name` populated on each result; ordering is global path-asc with vault-id tie-break (filesystem-search and content-search) or score-desc with vault-id tie-break (semantic-search).
- [ ] `hmn search content "X" --vaults personal,work` (or HTTP-side `vaults: [...]` filter) narrows to the named subset; unknown names appear in `partial_results.failed` with `code: vault_not_found`; empty array returns `invalid_request`.
- [ ] Search response includes `partial_results: {skipped: [...], failed: [...]}` when at least one vault was paused, errored, or had a runtime error mid-query; the field is omitted when all in-scope vaults completed successfully (additive to v0/step-9 wire shape).
- [ ] Paused vault inclusion: search default scope skips paused vaults; the skip is visible in `partial_results.skipped` with `status: "paused"`.
- [ ] Errored vault inclusion: search default scope skips errored vaults; the skip is visible with `status: "errored"` and `reason` carrying the registry's `last_error`.
- [ ] MCP tools `vault_list`, `vault_status`, `vault_create`, `vault_terminate` are advertised; an MCP-capable agent (Claude Code or Iris) can invoke each and get back the spec response shapes. `vault_create` and `vault_terminate` return `write_tools_disabled` envelope when `[mcp] enable_write_tools = false`.
- [ ] All four manual smoke scenarios in Task 10.7 produce the documented outputs.
- [ ] 3× consecutive flake-check clean run on `cargo test`.
- [ ] Spec amendments: `docs/specs/filesystem-search.md`, `docs/specs/content-search.md`, `docs/specs/semantic-search.md` bumped to 0.2.0 with revision history; `docs/specs/change-events.md` bumped to 0.2.0 with revision history; `docs/specs/vault-management.md` fleshed to 1.0.0 ("Approved") with revision history. The round-3 step-9 "ahead of spec" footnote is closed.
- [ ] Reference docs (cli, configuration, architecture) updated; roadmap-3 § Step 10 marked shipped.
- [ ] One commit per task per the playbook (Task 10.1 may use the round-2-step-8 two-commit pattern for separability if the four-spec amendments are naturally separable from the vault-management fleshout — coordinator decides at task time).

## Step boundary follow-ups (anticipated)

- **Pagination/cursor across N independent indexes**: deferred per § A.3. Stays as Open Question in fleshed `vault-management.md` and the four search specs. Round 4+ candidate.
- **Streaming response shapes** (chunked / SSE / NDJSON): deferred per § A.5. Round 4+.
- **Multi-model-embedding-per-vault**: noted as round-4+ in fleshed spec § Behavior § Cross-Vault Search Semantics § Semantic Search.
- **Compose-style declarative layer**: round-3 step 11 decides whether to ship; spec covers the surface either way.
- **Cross-platform rename safety** (step-9 boundary follow-up): no change in step 10; still pending for any Windows operator surfacing.
- **MCP write-tool gating granularity**: § C committed to single `enable_write_tools` flag; per-tool gating is round-4+ if a use-case surfaces.
- **Per-vault op_lock for in-flight indexing during terminate**: § E commits to cooperative drain with 30s timeout; if the timeout proves too tight or too loose against real-shape vaults, surface for round-4 tuning.
- **Closest-name hint algorithm**: § D commits to Levenshtein-distance-3; if the lookup is slow or noisy on large name lists, revisit.

---

## Notes on workplan-write blocker handling

Solo todos 64 and 65 — the workplan-write blockers per [`roadmap-3.md`](./roadmap-3.md) § Step 10 — are pulled into this workplan as follows:

- **Solo todo 64** (four search/event spec amendments) — resolutions pinned in § A; spec text written by Task 10.1 task agent via `spec-generator`. Closed when Task 10.1 ships.
- **Solo todo 65** (vault-management spec fleshout) — resolutions pinned in § B; spec text written by Task 10.1 via `spec-generator` per the Solo todo 65 prompt. Closed when Task 10.1 ships.

Both stay open through Task 10.1's commit; the task agent closes them as part of its results comment.

The cross-vault search semantics decisions in § A and the surrounding resolutions in § B–E are the **load-bearing input** to the spec-generator runs; the task agent does not re-derive them. If the spec-generator skill's prose surfaces a resolution conflict (i.e., spec text drifts from the workplan resolution), the task agent ships the workplan resolution and surfaces the drift as a `coordinator-only` soft flag per the round-3-step-9 stable workplan-prose-vs-load-bearing-decision pattern.
