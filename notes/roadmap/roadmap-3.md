# Hypomnema Roadmap — Round 3: Multi-Vault (post-v0)

**Scope**: Implement the multi-vault adoption settled in [ADR-0009](../../docs/decisions/0009-multi-vault-per-daemon.md), [ADR-0010](../../docs/decisions/0010-vault-definitions-as-runtime-state.md), and [ADR-0011](../../docs/decisions/0011-vault-management-on-hmn.md). The canon was amended on 2026-04-26 and pre-staged this round so its scope was settled before round 2 (steps 6–8) shipped. This round turns the per-result `vault?` forward-compat field shipped in step 5 into a populated, load-bearing identifier and gives the daemon a Mutagen-shaped control plane for managing N vaults.

**Status**: Not started. Round 2 shipped on 2026-04-27 (`v0.1.0`); this round queues directly behind it. Workplans are created **just before** each step is implemented, per the round-1/2 cadence.

**Process**: Same as rounds 1 and 2. Each step gets a short workplan (`step-NN-workplan.md`) created immediately before that step is built. Deferred decisions are pulled forward to workplan-time. The orchestration shape (orchestrator + per-step coordinator + ephemeral task agents, see [`notes/coordinator-playbook.md`](../coordinator-playbook.md)) carries forward unchanged.

**Round-2 lessons feeding into this round** (see [`notes/project-planning-workflow-notes.md`](../project-planning-workflow-notes.md) § End-of-round retrospective for full text):

- **MSRV cross-check** in workplan self-review for any new top-level crate added by a step (round-2 step 8's escalation 81 / rmcp 1.5.0 / Rust 1.88 shape). Round 3 likely adds zero new top-level crates — this round is mostly internal refactor and surface expansion against existing deps — but flag any new dep at workplan time and check `cargo info <crate>` against `rust-toolchain.toml`.
- **Act-now vs defer-to-boundary** decision rule for soft flags that demonstrate real bugs is now a 2-step pattern (step 6 Task 6.4r1; step 8 escalation 81 auto-mode resolution). Carry forward unchanged; codify in playbook before round 3 starts if not already (recorded in [`notes/backlog.md`](../backlog.md) § Process / playbook).
- **Forward-note prediction-vs-observation** check: when a forward note makes a testable prediction about external library behavior, the receiving task agent should explicitly verify and report agreement or correction (round-2 step 8 task 8.2 → 8.3 `serverInfo.name` shape). Round 3 has fewer external-library predictions in flight, but the rule applies wherever it does (e.g. notify watcher behavior at higher concurrency).
- **Manual smoke verification on medium-high-risk wiring tasks** has paid off in 4 of the last 4 wiring-shape tasks (steps 5 task 5.5, 6 task 6.5, 7 task 7.3, 8 task 8.3). Default to including a smoke task on each round-3 step; round-3 step 10 (control plane) and step 11 (full lifecycle ops) are the natural candidates.
- **Real-external-dependency pass before the shipping tag** (round-2 boundary discovery: `7379dd0` indexer scan progress logs, `fcc4aa3` reqwest TLS + HTTP/2 features, both surfaced by Phase 2 manual testing against a hosted embedding service with a non-trivial vault). Round 3 should include at least one "real-shape multi-vault" pass before the round shipping tag — multiple vaults of mixed sizes, watcher behavior under concurrent file-system events across vaults.
- **Skills carrying forward**: `rusqlite-in-async` (per-vault stores all use the same `spawn_blocking` pattern), `filesystem-watching` (per-vault watchers all share the same notify + debouncer + sync-conflict pattern), `markdown-chunking` and `sqlite-vec-extension` (round-2's two new skills, now load-bearing per-vault). No new skills anticipated, but write one if a round-3 step's experience suggests it (e.g. cross-vault fan-out patterns, atomic registry writes).

**Specs amended or created this round**:

- **`docs/specs/vault-management.md`** — currently outline at v0.1.0. Fleshed to full spec via `spec-generator` at step 10's workplan-write phase. Solo todo 65 captures the prompt; the cross-vault search semantics deferred to its § Open Questions are resolved inline at step 10 workplan time.
- **`docs/specs/filesystem-search.md`** — add per-result `vault` (id) + `vault_name` (display label); add request-side `vaults?: string[]` filter; describe cross-vault behavior. Bump to 0.2.0.
- **`docs/specs/content-search.md`** — same shape as filesystem-search.
- **`docs/specs/semantic-search.md`** — same shape, plus cross-vault ranking semantics (per-vault top-K then merge, or alternative — resolved at step 10 workplan).
- **`docs/specs/change-events.md`** — add `vault` (id) only; **no** `vault_name` (outbox is durable; names rot). Bump to 0.2.0.

Solo todo 64 captures the four search/event spec amendments; 65 captures the vault-management.md fleshout. Both stay open and are pulled into step 10's workplan-write phase. Step 9 (internal refactor) does not depend on the spec amendments — it preserves single-vault behavior end-to-end.

**Implementation surface across the round**:

- New top-level state file: `<data_dir>/vaults.sqlite` — authoritative vault registry per ADR-0010.
- New per-vault subdirectory layout: `<data_dir>/vaults/<vault_id>/{index.sqlite,outbox.jsonl,meta.toml}`.
- New modules: `src/vault_registry/` (registry CRUD + reconcile), `src/control_plane/` (HTTP routes + tool handlers).
- Per-vault refactor of `src/store/`, `src/indexer/`, `src/watcher/`, `src/outbox/`.
- Daemon startup-sequence rewrite: read registry, reconcile per-vault state, spawn one watcher + indexer per active vault (per ADR-0010).
- Control-plane surface: HTTP routes (`POST /vaults`, `GET /vaults`, `GET /vaults/{id}`, `POST /vaults/{id}/{op}`, `DELETE /vaults/{id}`) + `hmn vault {create,list,status,pause,resume,reset,rename,rescan,terminate}` subcommands + the same operations as MCP tools.
- Removal of `hmnd scan` (subsumed by `hmn vault rescan`).
- Removal of top-level `[vault]` config key; addition of `default_vault_name` (or its post-resolution shape).
- Search-side wiring: handlers fan out across active vaults; `vaults` filter narrows scope.

The exact ordering of these across steps is the phasing decision below.

---

## Phasing decision

The round-2 outline ([`archive/roadmap-2.md`](./archive/roadmap-2.md) § Round 3 — Phasing options) listed two illustrative options: *Single-shot* (everything in one workplan; rejected by the round-2 outline's own risk note) and *Phased* (vault create/list/status/terminate first; pause/reset/rename/rescan + Compose layer in a follow-on workplan; cross-vault search semantics resolved in the first workplan regardless).

**Decision**: refine "Phased" into **3 steps** rather than 2. The round-2 outline's "Phased" option implicitly bundles the per-vault internal refactor with the create/list/status/terminate control plane and cross-vault search semantics into one workplan; that workplan would land in the 1500–2000-line range — well above round 1's step-5 (~1100 lines) and round 2's step-8 (~1100 lines), the two largest workplans shipped, and well above the round-1 boundary heuristic that flagged ~1000 lines as the size threshold for prose-accuracy self-review.

Splitting the foundational refactor into its own step preserves the round-1/2 pattern: structural change lands first (steps 1–4 internal work; step 6 chunking + embedding), user-visible surface follows (step 5 HTTP shipping gate; step 7 `/search/semantic` HTTP handler). Round 3's step 9 plays the same structural-foundation role for the control plane that lights up in step 10.

The 3-step phasing is also forward-compatible with the human's option to pull steps together at workplan time: if step 10's workplan-write reveals that the create/list/status/terminate surface is materially smaller than expected, that workplan can absorb pieces of step 11; conversely, if step 9's refactor reveals more friction than expected, that step's shipping criteria can stay tight on behavior preservation and step 10 can pick up any spillover.

**The 2-step "Phased" option from the round-2 outline remains a valid alternative.** Reviewing this roadmap, the human can redirect by saying "fold step 9 into step 10's workplan" — the 3-step phasing is opinion, not commitment. The shipping-criteria-per-step framing below is independent of which phasing wins; only the workplan boundaries change.

---

## Step 9 — Per-vault internal refactor + registry foundation

**Status**: Shipped 2026-04-28. See [`archive/step-09-workplan.md`](./archive/step-09-workplan.md) for the workplan and [`notes/project-planning-workflow-notes.md`](../project-planning-workflow-notes.md) § Step 9 for the retrospective.

**Goal**: `hmnd` is internally per-vault. The watcher, indexer, store, and outbox are all per-vault instances. A new `vaults.sqlite` registry holds the authoritative list of vaults; daemon startup reads and reconciles the registry, then spawns one watcher + indexer per active vault. **No new user-visible surface lands in this step** — no new HTTP routes, no new CLI subcommands, no new MCP tools. Existing single-vault behavior is preserved end-to-end against an unchanged search/outbox surface.

The migration story for an existing v0 deployment: on first startup with an empty `vaults.sqlite` and a pre-existing top-level `[vault]` config block (or its post-resolution equivalent), the daemon auto-creates one vault using the legacy config's path and the configured default name (`default_vault_name`, default `"default"`). Existing `index.sqlite` and `outbox.jsonl` either move into the new per-vault subdirectory or are wiped and rescanned — the migration strategy is a deferred decision resolved in the workplan (see below).

**Shipping criteria**:
- All existing integration tests pass unchanged. The behavior-preservation gate is the load-bearing criterion for this step.
- Fresh `hmnd` against the legacy `[vault]` config produces exactly one row in `vaults.sqlite`, exactly one subdirectory under `<data_dir>/vaults/<vault_id>/` with `index.sqlite`, `outbox.jsonl`, `meta.toml` (the latter mirroring the registry row for human readability per ADR-0010).
- `hmn search filesystem`, `hmn search content`, `hmn search semantic`, `hmn status`, and outbox tailing all return identical results to the v0.1.0 behavior against the same vault contents.
- Per-result `vault` field on every search response is populated with the registry surrogate ID (no longer omitted as in v0). Per-result `vault_name` is also populated. The top-level `vault?` field is still optional for forward-compat with v0 consumers; v0 consumers parsing those responses see the field present without a meaning change.
- Outbox events carry `vault` (id) on every event line. No `vault_name` field on outbox (per ADR-0009 — names rot, durable channel uses surrogate ID).
- Daemon survives crash mid-init without orphaning state: an interrupted vault create (subdirectory exists, registry row missing — or vice versa) reconciles cleanly on next startup. (No control plane in step 9, so this is exercised against the auto-create-from-legacy-config path; the full crash-mid-create story for user-initiated creates is exercised in step 10.)
- New top-level config key `default_vault_name` (default `"default"`) replaces the top-level `[vault]` block. Migration handling for existing configs: hard-error with a clear migration message at first startup, OR accept-and-warn-and-translate — resolved at step-9 workplan-write time.

**Deferred decisions to resolve at workplan-time**:
- Surrogate ID format: `vault_<base32>` vs UUIDv7 vs ULID (per [`docs/specs/vault-management.md` § Open Questions](../../docs/specs/vault-management.md#open-questions)). Affects the registry schema and the per-vault subdirectory naming, so resolution must land at this step.
- Migration of existing v0 single-vault state: move `<data_dir>/index.sqlite` → `<data_dir>/vaults/<id>/index.sqlite` and `<data_dir>/outbox.jsonl` → `<data_dir>/vaults/<id>/outbox.jsonl`, **or** wipe and rescan, **or** require a manual migration step. Each has trade-offs against operator-disruption vs. implementation-complexity vs. crash-safety.
- Top-level `[vault]` config key removal vs. deprecation: hard-remove with a migration error message, or accept-and-warn-and-translate at startup. Round-1/2 v0 deployments are the only real-world case affected.
- `vaults.sqlite` schema migration strategy: copy the round-1 migrations module pattern (numbered SQL files, `applied_migrations` table) or simpler (single CREATE TABLE at first run, no migrations until a schema change is actually needed). The registry is small and slow-moving — likely simpler is fine, but the workplan should commit explicitly.
- Reconcile behavior for an `errored`-or-`paused` legacy auto-create: if the legacy `[vault]` config's path is no longer accessible, does the daemon refuse to start, enter `errored`, or skip and warn? (The full vault-state machine isn't user-exposed until step 10, but the underlying state machinery exists in step 9.)
- Daemon behavior with zero registered vaults AND no legacy `[vault]` config: error and exit, or warn-and-idle? (No CLI to add vaults yet — error-and-exit risks bricking a fresh install; warn-and-idle is more forgiving.)
- Search fan-out shape for the *internal* surface: in v0.1.0 search handlers operate on a single store. With one vault registered, the handler still returns the same shape, but the underlying call is now "fan out across N=1 active vaults." That fan-out machinery is pre-staged in step 9 even though step 10 introduces the cross-vault path semantically.

**New deps**: none anticipated. Per-vault state is built on existing `rusqlite` / `r2d2_sqlite` / `notify` / `notify-debouncer-full` / `tokio` patterns. The registry's atomic-write semantics are SQLite-native (transactions); no new lock library needed.

**Risk**: medium-high. Daemon startup-sequence rewrite + per-vault refactor across 4 modules + new top-level state file + migration of existing v0 state. The behavior-preservation gate is tight: any drift in single-vault search/outbox behavior is a regression, not a feature. Manual smoke verification against a populated v0 vault (with existing `index.sqlite` and `outbox.jsonl`) is the natural quality gate; recommend including a smoke task in the workplan per the round-2 step-8 precedent.

**Cross-references**: [ADR-0009 § Decision (storage layout)](../../docs/decisions/0009-multi-vault-per-daemon.md), [ADR-0010 § Decision (registry schema, reconciliation)](../../docs/decisions/0010-vault-definitions-as-runtime-state.md), [`docs/specs/vault-management.md` § Behavior, § Data Schema](../../docs/specs/vault-management.md). The vault-management spec is still an outline at step 9 — step 9's workplan does not depend on the fleshed spec.

---

## Step 10 — Vault control plane (read + create/terminate) + cross-vault search

**Goal**: `hmnd` exposes the read and create/terminate vault operations over its three transports — HTTP, the `hmn` CLI, and MCP tools. The four search specs and `change-events` spec are amended in this step (Solo todo 64). The `vault-management.md` spec is fleshed from outline to full spec at workplan-write phase (Solo todo 65). Cross-vault search semantics — result ordering, pagination, fan-out execution, partial-failure handling, paused/errored vault inclusion — are resolved in this step's workplan.

**Shipping criteria**:
- `hmn vault create [--name NAME] PATH` against a running daemon creates a new vault: registry row inserted, per-vault subdirectory created, watcher + indexer started. The first search query after create returns results scoped to the new vault as it indexes.
- `hmn vault list` returns the registered vaults with `{id, name, path, status, file_count, last_indexed_at, ...}`.
- `hmn vault status [NAME|ID]` returns single-vault detail with `last_error` populated if the vault is in `errored` state.
- `hmn vault terminate NAME|ID` removes the registry row, stops the watcher + indexer, removes the per-vault subdirectory, never touches the vault path's own files. Terminate-then-create-with-same-name is supported and cheap (registry row gone, subdir gone, fresh create builds anew per ADR-0010 § Idempotency).
- `hmn search content "X"` against a daemon with two vaults returns intermingled results with `vault` (id) + `vault_name` populated on each result. The cross-vault behavior matches the resolution committed at step 10 workplan-write (ordering, limit semantics, fan-out execution model, etc.).
- `hmn search content "X" --vaults personal,work` filters to the named subset.
- `curl -X POST http://127.0.0.1:7777/vaults -d '{"path":"~/foo"}'` creates a vault over HTTP and returns the same response shape as `hmn vault create`. The HTTP handler is the load-bearing implementation; the CLI is a thin wrapper.
- MCP read tools (`vault.list`, `vault.status`) and write tools (`vault.create`, `vault.terminate`) are advertised by the MCP server; an MCP-capable agent (Claude Code or Iris) can invoke each and get back the spec response shapes. Per-tool gating to disable write tools is workplan-time decided per [ADR-0011 § Negative consequences](../../docs/decisions/0011-vault-management-on-hmn.md).
- The four spec amendments (filesystem-search, content-search, semantic-search, change-events; Solo todo 64) and the fleshed vault-management spec (Solo todo 65) all land in this step. Bump search/event specs to 0.2.0 with revision-history entries dated at step-10 ship date.

**Deferred decisions to resolve at workplan-time** (this is the step where most round-3 deferred decisions get pinned):
- Cross-vault search semantics (the load-bearing question for this step):
  - Result ordering across vaults — for filesystem-search ("ascending path" today): per-vault-then-concat with stable vault ordering, or interleaved-by-path? Same question per spec mode.
  - Pagination / cursor across N independent indexes — opaque cursor encoding per-vault offsets, per-vault paginated then merge per page, or sort-key-based.
  - Fan-out execution model — gather-then-respond (simplest), streaming (chunked HTTP / SSE / NDJSON), or async-with-completion.
  - `limit` semantics — global limit applied after merge, per-vault limit, or proportional split.
  - Partial-failure handling — fail-whole-query, return-partial-with-warning, silent-skip — and how the wire shape signals it (e.g. `truncated_due_to: ["vault_x"]`).
  - Paused vault inclusion in default scope — likely silent skip but unspecified; document the choice.
  - Errored vault inclusion in default scope — same default with a wire-shape diagnostic.
  - Semantic-search global top-N — per-vault top-K then merge with score normalization, or alternatives.
- Vault-management spec fleshout via `spec-generator` (Solo todo 65 captures the prompt). Bump to 0.2.0 (or 1.0.0 if shipping commits the spec to "Approved" status). The fleshout converts the outline's `Open Questions` into resolved sections; the round-3 workplan resolves them inline.
- MCP write-tool gating: should `vault.create` and `vault.terminate` be gated separately from read-only ops (`vault.list`, `vault.status`)? Default-on-localhost-only matches the round-2 trust posture; opt-out via config key (`mcp.write_tools_enabled = false`) is one shape; per-tool gating is more granular.
- HTTP error envelope codes for vault operations: `404 vault_not_found`, `409 vault_path_conflict`, `409 vault_name_conflict`, `422 vault_path_invalid`, `503 vault_errored`, `500 registry_corrupt` (per the vault-management spec's error catalog). Confirm these against the existing error-mapping pattern in `src/api/error.rs` and pin specific codes/messages.
- Concurrency posture for control-plane operations: ADR-0010 commits to "operations on the same vault are serialized; operations on different vaults run in parallel." The implementation shape (per-vault async-mutex, per-vault actor-task, channel-with-id-key) is workplan-time.

**New deps**: none anticipated. The MCP tool surface extends the existing `rmcp` integration from step 8; HTTP routes extend the existing Axum router; CLI subcommands extend the existing `clap`-based `Command` enum.

**Risk**: medium-high. First user-mutable-state surface (write API, where v0 only had read-side search). Cross-vault search semantics is genuine design work — multiple valid resolutions, each with different complexity and ergonomics trade-offs. Spec fleshout via `spec-generator` is a workplan-time-blocker (the workplan can't commit to operations whose behavior the spec hasn't pinned). Manual smoke verification with two real vaults of mixed sizes is the natural quality gate.

**Cross-references**: [ADR-0009 § Wire-shape implications](../../docs/decisions/0009-multi-vault-per-daemon.md), [ADR-0010 § Decision (control-plane API)](../../docs/decisions/0010-vault-definitions-as-runtime-state.md), [ADR-0011 § Decision](../../docs/decisions/0011-vault-management-on-hmn.md), [`docs/specs/vault-management.md`](../../docs/specs/vault-management.md), Solo todos 64 and 65.

---

## Step 11 — Remaining lifecycle ops + Compose layer (round shipping gate)

**Goal**: Round out the full vault lifecycle surface — `pause`, `resume`, `reset`, `rename`, `rescan` — over all three transports. Remove `hmnd scan` (subsumed by `hmn vault rescan`). Decide whether the Compose-style declarative provisioning layer ships in this round or queues to round 4 (per the deferred-decision list in the round-2 outline). On shipping the full **after-step-11 boundary ritual** runs (milestone tag — likely `v0.2.0` if the round bumps the minor; per-step + end-of-round retros).

**Shipping criteria**:
- `hmn vault pause NAME|ID` and `hmn vault resume NAME|ID` mutate registry status without touching the underlying index. A paused vault's watcher and indexer are stopped; resume re-spawns them. Search behavior for paused vaults matches the resolution committed at step-10 workplan-write (likely silent skip in default scope).
- `hmn vault reset NAME|ID` clears `last_error`, restarts watcher + indexer; index preserved unless `--rebuild` is passed (the optional `--rebuild` flag for full re-index is a step-11 workplan decision).
- `hmn vault rename [NAME|ID] --name NEW_NAME` is a single registry row UPDATE; the per-vault subdirectory is keyed by surrogate ID and never moves. Search responses immediately reflect the new `vault_name` on subsequent queries.
- `hmn vault rescan [NAME|ID]` forces full reconciliation against vault contents; outbox emits events as if from cold start (per [`docs/specs/vault-management.md` § Operations — rescan](../../docs/specs/vault-management.md)).
- `hmnd scan` is removed. The CLI surface is `hmn vault rescan ...` only. Migration path: deprecation warning in step 10 if `hmnd scan` is still mentioned anywhere; full removal in step 11.
- All five remaining MCP tools (`vault.pause`, `vault.resume`, `vault.reset`, `vault.rename`, `vault.rescan`) are advertised and invokable, matching their HTTP/CLI counterparts.
- The Compose decision (workplan-time): if shipped this round, `<data_dir>/hmnd-compose.toml` is parsed at startup, the listed vaults are reconciled additively (created if missing; never destroyed; state remains canonical per ADR-0010), and the file format is documented in `docs/reference/configuration.md`. If deferred to round 4, that decision is recorded in the step-11 workplan and the round-3 backlog or round-4 roadmap entry is added.
- Round shipping gate: end-to-end manual integration test against a real-shape multi-vault setup (multiple vaults of mixed sizes, watcher behavior under concurrent FS events across vaults, control-plane operations triggered while indexing is in flight, search results coherent across the full operation matrix). This is the round-3 analogue of round-2 step-8's Claude-Code-in-the-loop test.

**Deferred decisions to resolve at workplan-time**:
- Compose-layer ship-this-round vs. defer-to-round-4. Round-2 outline framed it as "queue as a follow-on workplan if scope grows." The pragmatic test: if step 10's spec fleshout + control-plane work fits inside ~1200 lines of workplan, Compose can ride here; if not, queue to round 4. Recorded in the step-11 workplan with rationale either way.
- Compose file format and merging rules (only relevant if shipping this round) — pinned at step-11 workplan-write per [`docs/specs/vault-management.md` § Compose-Style Declarative Layer](../../docs/specs/vault-management.md).
- `--rebuild` flag on `reset`: ship in step 11, defer to round 4, or ship under a flag-gate (`hmn vault reset --rebuild` accepted but a no-op-with-warning until the rebuild path is implemented). Ship-in-step-11 is the cleanest if the rebuild path is small.
- Deprecation period for `hmnd scan`: full removal in step 11 vs. accept-and-warn through one minor release. Round 3 is post-v0.1.0; minor-release discipline isn't yet established. Workplan records the chosen policy.
- Concurrency posture for in-flight indexing during control-plane mutations: a `pause` issued mid-indexing — does the indexer drain to a clean point, or stop immediately? Workplan time. Affects user-perceived "did pause take effect immediately" UX.

**New deps**: none anticipated.

**Risk**: medium. Composes step 9's foundation + step 10's control plane with the remaining lifecycle ops. The Compose layer is the new structural piece if it ships here; otherwise the step is mostly surface-expansion. The end-to-end multi-vault manual integration test is the load-bearing risk (round-2 step-8 precedent: external-host-in-the-loop-or-multi-vault-in-the-loop tests are qualitatively new each round).

**Cross-references**: [`docs/specs/vault-management.md` § Operations — pause/resume/reset/rename/rescan](../../docs/specs/vault-management.md), [ADR-0010 § Decision (Compose layer)](../../docs/decisions/0010-vault-definitions-as-runtime-state.md), [ADR-0011 § Consequences (`hmnd scan` removal)](../../docs/decisions/0011-vault-management-on-hmn.md).

---

## After round 3

When step 11 ships:
1. Tag the milestone in git (likely `v0.2.0` or `v0.next` — version-bump policy is a separate question; round-3 boundary is the natural moment to settle it).
2. Capture any ADRs that hardened during the build. Likely candidates: cross-vault search semantics (if the resolution is load-bearing enough to warrant ADR rather than spec-only), Compose file format (if shipped this round).
3. Per-step + end-of-round retros in [`notes/project-planning-workflow-notes.md`](../project-planning-workflow-notes.md). The round-3 end-of-round retro answers: did the roadmap → workplan → build cadence still work at round-3 risk shape? What surprised us about per-vault concurrency and cross-vault semantics that the docs did not predict?
4. Move on to round 4, or close the project's "open in scope" list. Candidates for round 4 (from [`notes/backlog.md`](../backlog.md)): MCP Streamable HTTP transport (Solo todos 83 + 84), agent-host integration / MCP-tool-discoverability work, public-presence / brand work, outbox rotation, Compose layer if it didn't ship in round 3, any round-3 boundary follow-ups.

---

## Notes — round-3 setup at this moment

- Solo todos pre-queued for this round: **64** (four search/event spec amendments — picked up in step 10's workplan-write phase via `spec-generator`), **65** (vault-management.md fleshout — also step 10 workplan-write). Both stay open until step 10. **83** and **84** (MCP Streamable HTTP) are explicitly outside round 3 per [`notes/backlog.md`](../backlog.md) § Agent-host integration.
- The vault-management spec's `Open Questions` section is the canonical list of cross-vault semantics decisions that get resolved at step 10 workplan-write; do not re-derive them in this roadmap.
- The 3-step phasing above is opinion, not commitment. The human can redirect to a 2-step phasing (fold step 9 into step 10 per the round-2 outline's "Phased" option) or a single-shot phasing at workplan-write time. The shipping-criteria framing for each step is independent of where workplan boundaries land.
- Round 1 = steps 1–5 (skeleton through HTTP shipping gate); round 2 = steps 6–8 (chunking + embedding through MCP wrapper). Round 3 = steps 9–11 (per-vault refactor through full lifecycle + Compose decision).
