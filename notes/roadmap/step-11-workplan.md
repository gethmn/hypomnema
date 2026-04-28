# Step 11 Workplan — Remaining lifecycle ops + `hmnd scan` removal (round-3 shipping gate)

**Step**: 11 of 11 (round 3 of 3 — **the round-3 shipping gate**). Lights up the remaining vault lifecycle surface (`pause`, `resume`, `reset`, `rename`, `rescan`) on top of step 10's create/list/status/terminate foundation. See [`../roadmap-3.md`](../roadmap-3.md) § Step 11 for the round and [`step-10-workplan.md`](../archive/step-10-workplan.md) for the immediately prior step.

**Status**: Workplan-phase; pending human review before build. Boundary is the **full round-3 shipping ritual** — milestone tag (likely `v0.2.0`), per-step retro for step 11, and end-of-round retro for round 3. See § Notes on round-3 shipping gate at the bottom of this file.

**Round-2 / round-3 lessons carrying forward** (from [`notes/project-planning-workflow-notes.md`](../../project-planning-workflow-notes.md) § End-of-round retrospective + § Step 9 + § Step 10):

- **MSRV cross-check** on any new top-level crate. Step 11 introduces zero new top-level crates — every lifecycle op is built on existing `axum` / `clap` / `rmcp` / `tokio` / `rusqlite` / `uuid` patterns. Verified at workplan-write; re-verified before each task that adds a `Cargo.toml` line (none anticipated).
- **Manual smoke verification** is load-bearing for medium-high-risk wiring tasks (now 6-of-6 across rounds 1–3). Step 11's medium-high-risk task is **11.7** (integration tests + multi-vault end-to-end manual smoke = the round-3 shipping gate). The roadmap explicitly calls for "real-shape multi-vault" pass before the round shipping tag — multiple vaults of mixed sizes, watcher behavior under concurrent FS events across vaults, control-plane operations triggered while indexing is in flight.
- **Forward-note prediction-vs-observation** check: round-3 step-11 has fewer external-library predictions than rounds 1–2 (rmcp, sqlite-vec, notify, axum are all settled). The closest fresh territory is the per-vault `op_lock` shape that step 10 constructed-but-didn't-acquire (see [`src/control_plane/runner.rs:19`](../../../src/control_plane/runner.rs)) — step 11 is the first step that actually takes the lock. If `op_lock` interaction with `runner.lifecycle: Mutex<Option<…>>` surfaces a deadlock or ordering surprise, that's the prediction worth verifying explicitly.
- **Workplan-prose-vs-load-bearing-decision drift** is a stable round-3 pattern (round-3 step-9 and step-10 retros). 5 instances in step 9, 5 in step 10, all `coordinator-only` audience — the workplan body's load-bearing decisions are correct; surrounding prose enumerations are fingerprints, not exhaustive contracts. **Carry-forward expectation**: this step's surface (5 lifecycle ops × 3 transports + scan removal + integration tests + multi-vault smoke) will likely surface 3–5 such flags; treat them as defer-to-boundary by default unless a downstream task is materially affected.
- **Internal-shape claims** (round-3 step-9 self-review addition): for any task that reshapes an existing module, re-read the task body against the current module signature at workplan self-review and flag aspirational language. Step 11's load-bearing reshape is **`VaultRunner.entry: Arc<VaultEntry>` → `entry: std::sync::RwLock<Arc<VaultEntry>>`** in Task 11.1 (interior mutability for pause/resume/rename status + name updates without runner teardown). Self-review pass at the bottom of this workplan covers it.
- **Soft-flag self-correction at boundary** (round-3 step-10 new pattern): when a forward-noted soft flag asks you to reconcile prose, **verify the prose is actually wrong before editing** — the prior task's observation may have been the drift. Task 11.8's reference-docs agent applies this rule when consuming any forward-noted reconciliation requests from earlier tasks.
- **Skills carrying forward**: [`rusqlite-in-async`](../../../.claude/skills/rusqlite-in-async/SKILL.md) (every new control-plane SQL site wraps `spawn_blocking`); [`filesystem-watching`](../../../.claude/skills/filesystem-watching/SKILL.md) (resume / reset re-spawn per-vault watchers per the round-1 + round-3-step-9 pattern); [`markdown-chunking`](../../../.claude/skills/markdown-chunking/SKILL.md) and [`sqlite-vec-extension`](../../../.claude/skills/sqlite-vec-extension/SKILL.md) remain load-bearing per-vault. No new skills anticipated; if a multi-vault concurrency or rescan-as-cold-start pattern proves worth codifying at boundary, write one then.

---

## Goal recap

`hmnd` exposes the **remaining five lifecycle operations** (`pause`, `resume`, `reset`, `rename`, `rescan`) over its three transports — HTTP, the `hmn` CLI, and MCP tools — on top of step 10's create/list/status/terminate foundation. The v0 standalone `hmnd scan` subcommand is removed; its behavior is subsumed by `hmn vault rescan [NAME|ID]` per ADR-0011.

**Operations shipping in step 11**: `pause`, `resume`, `reset`, `rename`, `rescan`. Their wire shapes are already pinned in [`docs/specs/vault-management.md`](../../../docs/specs/vault-management.md) v1.0.0 (committed at step 10) — this step ships against the spec, not at the spec's edge. **No spec amendments anticipated** unless implementation surfaces a contract surprise (in which case `vault-management.md` → 1.1.0 is the natural recovery, no ADR needed; flagged in step-10 retro § Step-boundary follow-ups).

**Compose-layer (deferred to round 4)**: per Resolution A below. Spec § Compose-Style Declarative Layer (deferred) already covers the surface; round-3 ships without it. The deferral lets step 11 ship as a clean "full vault lifecycle" gate rather than mixing in a fundamentally additive layer that benefits from its own design pass.

The **round-3 shipping gate** composes:

1. Behavior preservation: every step-9 / step-10 integration test passes unchanged. The control-plane surface from step 10 (create/list/status/terminate) keeps working; the new lifecycle ops are additive.
2. **Real-shape multi-vault end-to-end** (round-2 step-8 / round-3 step-9 / step-10 manual smoke precedent extended): two-or-three vaults of mixed sizes, lifecycle ops triggered while indexing is in flight, watcher behavior under concurrent FS events across vaults, search results coherent across the full operation matrix. This is the load-bearing manual smoke for the round shipping tag.
3. `hmnd scan` removal — code matches docs (cli.md already declares it removed in 0.2.0; step 11 makes it true).

The **boundary ritual** for this step is the full round-3 shipping variant per [`notes/project-planning-workflow-notes.md`](../../project-planning-workflow-notes.md) § The three phases / Step boundary ritual: milestone tag (`v0.2.0` likely; version-bump policy resolved at boundary alongside the human), per-step retro for step 11, end-of-round retro for round 3, round-3 roadmap archived.

---

## Deferred-decision resolutions

The five TBDs from [`roadmap-3.md`](../roadmap-3.md) § Step 11 are resolved below (A–E). Each resolution is the load-bearing input to the corresponding tasks below.

### A. Compose-layer ship-this-round vs. defer-to-round-4

**Resolution**: **defer Compose to round 4.**

The roadmap's pragmatic test was "if step 10's spec fleshout + control-plane work fits inside ~1200 lines of workplan, Compose can ride here; if not, queue to round 4." Step 10's workplan shipped at 703 lines (well under 1200). By that test, Compose *could* ride here.

The round-4-defer call is on a different axis — Compose is **fundamentally additive** over the full vault lifecycle:

- Step 11 without Compose is a clean "round shipping gate" — all nine vault lifecycle ops + scan removal + multi-vault smoke. Adding Compose pushes this from 8 tasks to 10+ tasks and from a focused "lifecycle round" to a "lifecycle + declarative-layer round." The 2-deep concept stack (control plane mutates state; Compose declaratively ensures vaults exist) is the kind of thing that benefits from its own dedicated round, not a 9th task.
- Compose is a **declarative semantics design surface**: file format (TOML keys, vault list shape, env-var interpolation policy?), merging rules (additive-on-startup vs. on-SIGHUP-reload?), interaction with runtime mutations (does `hmn vault terminate` against a Compose-listed vault re-create it on next startup?), error policy (what if a Compose-listed path is unreachable at startup?). These are non-trivial; rushing them into an already-busy shipping-gate step risks shipping a half-pinned shape that's hard to rev later (operators would already depend on whatever 0.2.0 happened to ship).
- The **vault-management spec already covers the surface** at v1.0.0 § Compose-Style Declarative Layer (deferred). A future round-4 workplan can pull Compose without canon rewrites — exactly the LDS shape the project uses elsewhere.
- Round 4 already has plenty of work queued ([backlog.md](../../backlog.md)): MCP Streamable HTTP, agent-host integration / MCP-tool-discoverability, public-presence/brand work, outbox rotation, the round-3 boundary follow-ups. Compose fits naturally alongside the agent-host integration work since Compose is the operator-facing analogue of "declarative provisioning that an agent might generate."

**Rationale alternatives considered**:
- *Ship-as-flag-gated stub* (parse the file but warn-and-no-op): rejected; ships a half-feature, confuses operators.
- *Ship a minimal "additive create-only" subset* (parse, call `vault create` for each entry on startup, ignore everything else): rejected; minimal subsets become operator-facing contracts that constrain the full design later.

**Backlog entry**: a round-4-candidate entry is added to `notes/backlog.md` at boundary (Task 11.8) under § Round 4 candidates, naming Compose alongside the existing round-4 items.

### B. Compose file format

**Resolution**: **N/A** given Resolution A. Workplan does not pin file format, merging rules, or env-var interpolation policy. The vault-management spec's § Compose-Style Declarative Layer (deferred) section preserves the surface description; the round-4 workplan that ships Compose pins the format then.

### C. `--rebuild` flag on `reset`

**Resolution**: **ship `--rebuild` in step 11.** Implementation reuses migration 0004's existing pattern (DELETE chunks_vec, DELETE chunks, UPDATE files SET content_hash = '') applied at the per-vault store boundary, then triggers a rescan to re-embed.

The vault-management spec already commits to `--rebuild` as a body field on `POST /vaults/{name_or_id}/reset` and the CLI flag (see [vault-management.md § reset (step 11)](../../../docs/specs/vault-management.md#reset-step-11): "With `--rebuild`, also drop and rebuild the per-vault `chunks` + `chunks_vec` tables (keeps `files`; preserves outbox)"). The implementation cost is the open question — and it's small:

**Implementation shape** (Task 11.2):

1. Take the per-vault `op_lock`.
2. Drain runner's lifecycle via `shutdown_with_timeout(30s)` (same as terminate).
3. If `rebuild = true`:
   - Open the per-vault `Store` directly (the runner's Arc<Store> is now drained out).
   - Run a transaction: `DELETE FROM chunks_vec; DELETE FROM chunks; UPDATE files SET content_hash = '';`. This mirrors migration 0004 line-for-line — the empty-string sentinel is the existing "needs re-embedding" marker per migration 0004 (see [`src/store/schema.rs:51`](../../../src/store/schema.rs)).
   - Outbox is **preserved** per spec (rebuild affects index, not durable event log).
4. Clear `last_error` (registry UPDATE: `last_error = NULL`, `status = 'active'`).
5. Spawn fresh `RunnerLifecycle` via existing `spawn_runner_for_row` helper. The next scanner pass re-reads every file (because `content_hash = ''` ≠ on-disk hash), re-chunks, re-embeds. Outbox emits `modified` events as the indexer reaches each file.
6. Replace runner's entry Arc with status=Active, last_error=None.
7. Release op_lock.

**Why this shape**:
- Reuses the migration-0004 pattern verbatim — no new "rebuild" code paths in the indexer; the existing "content_hash differs" path handles it.
- Preserves `files` rows (so file IDs are stable across rebuilds; outbox-tailing consumers see the same vault-relative paths).
- Preserves outbox (per spec; outbox is durable event log, not a rebuild side-effect).
- The 30s drain matches terminate; in practice, drain completes in <1s (single-file granularity).

**Without `--rebuild`** (default): the reset is "clear `last_error`, restart watcher + indexer." The watcher + indexer pick up where they left off; `content_hash`-stable files are not re-embedded. This is the cheap "kick the vault" recovery path for transient errors.

### D. `hmnd scan` deprecation policy

**Resolution**: **hard-remove in step 11.** No deprecation period.

- [`docs/reference/cli.md`](../../../docs/reference/cli.md) already declares: "`hmnd scan` ... was removed in 0.2.0. Equivalent behavior is available via `hmn vault rescan [NAME|ID]` against a running daemon" (text added at step 10's boundary; forward-looking prose). Round 3 ships as v0.2.0 (per Resolution F below + boundary version-bump call); step 11 makes that prose true.
- Pre-v1.0, the project has no committed deprecation cycle (round 1 / round 2 / round 3 retros never invoked one). Round 3 is the right moment to settle: `hmn vault rescan [NAME|ID]` is the replacement and ships **contemporaneously** in this same step (Task 11.4 CLI / 11.3 HTTP / 11.5 MCP), so an operator pinning `hmnd scan` upgrades to `hmn vault rescan` in the same release that removes the old surface.
- Operators running `hmnd scan` post-removal get a clean clap "unknown subcommand" error (`hmnd: unknown subcommand 'scan'. Try 'hmnd --help'.`); release notes / CHANGELOG point them at the migration. The round-3 boundary retro records this as the v0.x deprecation precedent.

**Concrete actions** (Task 11.6 — small surgical task):
- Remove `Scan` variant from `enum Command` in [`src/bin/hmnd.rs:33`](../../../src/bin/hmnd.rs).
- Remove the `Some(Command::Scan) => do_scan(&config).await` arm in `dispatch` ([`src/bin/hmnd.rs:69`](../../../src/bin/hmnd.rs)).
- Remove the `do_scan` function in `src/bin/hmnd.rs` (and any helpers it pulls in that nothing else uses).
- Update the `hmnd scan` reference in [`src/watcher/mod.rs:34`](../../../src/watcher/mod.rs) (module docstring) to reference `hmn vault rescan` instead.
- Verify `docs/reference/cli.md` text is accurate (it already declares removal — the boundary doc agent re-reads it; if any "currently available as `hmnd scan`" prose still exists elsewhere, fix it).
- Verify no integration test still invokes `hmnd scan` (`grep -rn "hmnd scan" tests/`).

This is intentionally a small standalone task (~50 LoC delta) because a clean bisect-anchor for "scan removed" is more valuable than burying it in another lifecycle op.

### E. Concurrency posture for in-flight indexing during pause

**Resolution**: **drain to single-file boundary; force-abort at 30s.** Same posture as terminate (`TERMINATE_DRAIN_TIMEOUT = 30s` per [`src/control_plane/manager.rs:43`](../../../src/control_plane/manager.rs)). Single constant `LIFECYCLE_DRAIN_TIMEOUT = 30s` shared across pause/resume/reset/terminate.

The deferred question: does `pause` issued mid-indexing (a) drain to the next clean point or (b) interrupt immediately?

- **Drain wins**. The indexer's natural unit of work is one file at a time inside a `spawn_blocking` task (per [`src/indexer/mod.rs`](../../../src/indexer/mod.rs)'s `process_entry` shape). Cooperative drain via the existing `tokio::sync::watch` shutdown channel lets the in-flight file complete before the consumer loop exits — no torn writes, no partial chunks_vec inserts.
- Step-10's `VaultRunner::shutdown_with_timeout(30s)` already implements this: send shutdown signal; await consumer drain up to 30s; force-abort + drop watcher beyond. Pause reuses this verbatim.
- Real-world drain time on a typical large file is well under 1s (single-file embedding round-trip + chunks_vec insert). 30s is the force-abort fallback for pathological cases (e.g., hung embedding service mid-call) — fairly forgiving but bounded enough to keep operator UX responsive.

**Concurrency posture (across all five lifecycle ops)** — Resolution G, supplementing § E:

ADR-0010 invariant: "operations on the same vault are serialized; operations on different vaults run in parallel." Step 10 implements this for create/terminate via the **outer write-lock on `runners`** (because they mutate the runner-map membership). Step 11's five ops do **not** mutate runner-map membership — they keep the runner in the map and either replace its `entry` Arc, or refresh its `RunnerLifecycle`. They take the **per-vault `op_lock`** while keeping a **read-lock on the outer map** — different vaults' ops genuinely run in parallel.

Concrete:

| Op | Outer lock | Per-vault op_lock | Runner mutations |
|---|---|---|---|
| `pause` | read | write | replace entry (Active→Paused); shutdown_with_timeout drains lifecycle |
| `resume` | read | write | replace entry (Paused/Errored→Active); install fresh `RunnerLifecycle` |
| `reset` | read | write | replace entry (clear last_error); drain + spawn fresh `RunnerLifecycle`; if `--rebuild`, run rebuild SQL between drain and spawn |
| `rename` | read | write | replace entry (new name); rewrite meta.toml; no lifecycle change |
| `rescan` | read | write | trigger scanner re-walk via existing scanner-task channel; no entry or lifecycle change |

The per-vault op_lock is **already constructed in step 10** ([`src/control_plane/runner.rs:19`](../../../src/control_plane/runner.rs)): `pub(crate) op_lock: Mutex<()>`. Step 11 is the first step that acquires it.

**The interior-mutability requirement** (Resolution F, supplementing § E): pause/resume/rename all need to replace the in-Arc `VaultEntry` (status for pause/resume, name for rename) without tearing the runner down. Step 10's `VaultRunner.entry: Arc<VaultEntry>` is immutable post-construction. **Task 11.1 reshapes this to `entry: std::sync::RwLock<Arc<VaultEntry>>`** with a getter that clones the Arc and a `replace_entry()` setter. Read-side callers (`active_vaults()`, `search_scope()`) take a brief read-lock + Arc clone; cheap. The annotation at [`src/api/mod.rs:24`](../../../src/api/mod.rs) ("step 10 only inserts `Active` entries into the runners map, but step 11 will mutate it for pause/resume without tearing the runner down") is the step-10 forward note that anticipates this exact refactor.

---

## Self-review for prose accuracy

This workplan is projected at ~700–900 lines (smaller than step-10's 703 lines × 1.1 = ~775; smaller than step-9's 427 lines × ~1.5 = ~640 because step 11 is mostly additive on step-10's foundation, with the interior-mutability refactor being the only substantive reshape). The round-1 ~1000-line heuristic does not fire automatically; the step-9 internal-shape-claims heuristic does. Running the spot-check on testable claims:

### Internal-shape claims (round-3 step-9 self-review addition)

1. **`VaultRunner.entry: Arc<VaultEntry>`** is the current shape ([`src/control_plane/runner.rs:12`](../../../src/control_plane/runner.rs)). Workplan Task 11.1 reshapes this to `RwLock<Arc<VaultEntry>>` with a clone-on-read getter and a `replace_entry()` setter. Verified by reading `src/control_plane/runner.rs` at workplan-write — current shape matches the prescription. Round-3-step-9 retro called this out as the round-3 self-review pattern; carrying it forward. Internal callers of `runner.entry()` (search.rs:69/143/224, manager.rs active_vaults() and search_scope() lines 281–360, control_plane tests) all clone the Arc post-call; ergonomic ripple is small (each call site picks up a brief read-lock, no other change).

2. **`VaultRunner.lifecycle: Mutex<Option<RunnerLifecycle>>`** is the existing shape ([`src/control_plane/runner.rs:23`](../../../src/control_plane/runner.rs)). Pause / reset / resume reuse this `Option` shape — pause's `shutdown_with_timeout` makes lifecycle = `None`; resume/reset spawn a fresh `RunnerLifecycle` and `lifecycle.lock().await.replace(new)`. The infrastructure exists; step 11 is the first user. Verified at workplan-write.

3. **`VaultRunner.op_lock: Mutex<()>`** is constructed-but-unused at step 10 ([`src/control_plane/runner.rs:19`](../../../src/control_plane/runner.rs)) with the `#[allow(dead_code)]` attribute and the comment "Reserved for step-11 ops (pause/resume/reset/rename/rescan) ...". Step 11 acquires it for the first time. The `#[allow(dead_code)]` is removed in Task 11.1.

4. **`spawn_runner_for_row`** ([`src/control_plane/manager.rs:703`](../../../src/control_plane/manager.rs)) is the helper that constructs a `RunnerLifecycle` from a `VaultRow`. Step 11's resume / reset / rebuild paths reuse it directly — no new spawn helper needed. Verified by reading lines 703–795 of `manager.rs` at workplan-write. (Note: the helper currently constructs a fresh shutdown channel; the runner's existing channel is dropped as part of `shutdown_with_timeout`'s `lifecycle.take()`. Resume / reset get a fresh channel via this same helper — no edge case.)

5. **`VaultManager.terminate`** flow ([`src/control_plane/manager.rs:540`](../../../src/control_plane/manager.rs)) is the closest existing analogue for pause's drain-then-update-status flow. Pause differs in two ways: (a) does not remove from the runner-map; (b) does not delete the registry row or the per-vault subdir. Pattern is "shutdown_with_timeout + UPDATE registry," not "remove + DELETE + remove_dir_all." Verified at workplan-write.

6. **`migration 0004` SQL** ([`src/store/schema.rs:49–55`](../../../src/store/schema.rs)) is the exact pattern reset's `--rebuild` reuses: `DROP TABLE chunks_vec; DELETE FROM chunks; UPDATE files SET content_hash = ''; CREATE VIRTUAL TABLE chunks_vec USING vec0(...)`. Step 11's `--rebuild` runs this same SQL (modulo: it doesn't need to re-CREATE chunks_vec since the table already exists post-migration; just `DELETE FROM chunks_vec; DELETE FROM chunks; UPDATE files SET content_hash = ''`). Empty-string sentinel for `content_hash` is the established "needs re-embedding" marker; verified by reading the migration's comment at lines 41–48.

7. **`enum Command`** in `src/bin/hmnd.rs:33` currently has variants `Scan` and `ConfigValidate`. Task 11.6 removes `Scan`. The `do_scan` function exists at [`src/bin/hmnd.rs`](../../../src/bin/hmnd.rs); `dispatch` matches `Some(Command::Scan) => do_scan(&config).await` at line 69. All three (variant, match arm, function) are removed in Task 11.6.

8. **`hmnd scan` reference in [`src/watcher/mod.rs:34`](../../../src/watcher/mod.rs)** module docstring needs update ("symlink inside the vault will be picked up by `hmnd scan` and on the…"). Task 11.6 changes this to reference `hmn vault rescan`.

9. **`docs/reference/cli.md`** declares `hmnd scan` removed in 0.2.0 (lines verified at workplan-write). Task 11.8's boundary-doc agent re-reads to confirm consistency post-removal — applies the round-3-step-10 "soft-flag self-correction at boundary" pattern: verify the prose is actually current before editing.

10. **`enum Command`** in `src/cli.rs:30` currently has `Search`, `Status`, `Mcp`, `Vault { op: VaultOp }`. `enum VaultOp` at line 50 currently has `Create`, `List`, `Status`, `Terminate`. Task 11.4 adds five new `VaultOp` variants: `Pause`, `Resume`, `Reset`, `Rename`, `Rescan`. Verified by reading `src/cli.rs` at workplan-write. clap's nested-subcommand pattern carries unchanged from step 10.

11. **`DaemonClient`** ([`src/client.rs`](../../../src/client.rs)) currently exposes `create_vault`, `list_vaults`, `get_vault`, `terminate_vault`, plus search methods. Task 11.4 adds `pause_vault`, `resume_vault`, `reset_vault`, `rename_vault`, `rescan_vault`. Each is a thin reqwest wrapper matching `terminate_vault`'s pattern (verified at workplan-write — `terminate_vault` uses `reqwest::Url::path_segments_mut` for percent-encoding per Task 10.4 forward note).

12. **`HypomnemaMcpServer`** ([`src/mcp/server.rs`](../../../src/mcp/server.rs)) uses `tool_router` macro with `#[tool(...)]` per-method registration. Step 10 added `vault_list`, `vault_status`, `vault_create`, `vault_terminate`. Task 11.5 adds `vault_pause`, `vault_resume`, `vault_reset`, `vault_rename`, `vault_rescan` following the same pattern. The `enable_write_tools` gating in `McpConfig` from step 10 covers these new tools verbatim — they are write tools, gated by the same flag.

13. **HTTP route shapes**: spec § Data Schema § Control-Plane HTTP Wire Shapes pins `POST /vaults/{name_or_id}/{op}` for the five new ops. Existing axum router pattern in [`src/api/mod.rs:55–58`](../../../src/api/mod.rs) uses `/vaults/:name_or_id` for get/delete; the new `/vaults/:name_or_id/pause`, `/resume`, `/reset`, `/rename`, `/rescan` are additive routes. Verified at workplan-write.

### External-library claims

1. **`std::sync::RwLock` for `VaultRunner.entry`**: pure synchronous read/write; never held across `.await`. Search-handler iterations call `runner.entry()` to clone the Arc, drop the lock, then proceed. Lock-poisoning behavior is `unwrap_or_else(|e| e.into_inner())` per existing code conventions in [`src/control_plane/manager.rs:286`](../../../src/control_plane/manager.rs) (which uses `.expect("vault manager runners RwLock poisoned")`). Step 11 follows the same convention.

2. **`tokio::sync::Mutex` for `op_lock`**: required because the lock is held across `.await` in pause/resume/reset/rescan/rename (registry SQL via spawn_blocking, fs ops, lifecycle drain). Already constructed in step 10 ([`src/control_plane/runner.rs:19`](../../../src/control_plane/runner.rs)). No prose drift.

3. **`reqwest` `POST` with body**: pause/resume/rescan have empty bodies; reset has optional `{rebuild?: bool}`; rename has `{new_name: string}`. Existing `DaemonClient::create_vault` uses `.json(&req)` per the reqwest pattern; new methods follow.

4. **clap subcommand with optional bool flag**: `--rebuild` on reset → `#[arg(long)] rebuild: bool` (clap's standard pattern; default false). `--new-name=<NEW_NAME>` on rename → `#[arg(long, value_name = "NEW_NAME")] new_name: String`. Both verified against existing patterns in `src/cli.rs:50–77`.

5. **rmcp 1.5 `tool_router` conditional registration**: step-10 verified at task time that always-register-with-short-circuit is canonical (write-tool gating short-circuits to `unknown_tool` error when disabled). Step 11's five new write tools follow the same pattern; no fresh upstream verification needed (step 10 closed this prediction; step 11 inherits the answer).

### Cross-platform claims

1. **`std::fs::remove_dir_all` for `--rebuild`**: not used; rebuild operates inside the existing per-vault `index.sqlite` via SQL, never on the filesystem. No cross-platform path-handling concerns.

2. **`std::fs::write` for meta.toml rewrite on rename**: `tempfile + rename` pattern is the existing meta.toml write shape from step-10 `create_subdir_and_meta` ([`src/control_plane/manager.rs:796`](../../../src/control_plane/manager.rs)). Same-filesystem assumption inherited from step 9; the per-vault subdir doesn't move on rename (subdirectory is keyed by surrogate ID).

The round-3 step-9 cross-platform-rename safety follow-up (legacy state migration) does not apply here — rename in step 11 only renames the registry row's `name` column and the meta.toml's `name = "..."` line; the per-vault subdirectory's filesystem name (UUIDv7) is unchanged.

---

## Tasks

The 8-task decomposition matches step 10's density. Per the round-1/2/3 default-not-batch rule (now 10-of-10 consecutive clean steps), tasks ship as solo agents. Each task ships its own commit per the playbook's TASK AGENT § Reporting; risk grades and dependencies noted at each task header.

### Task 11.1 — VaultRunner interior-mutability refactor + `pause` + `resume` + `rename`

**Risk**: medium-high. **Load-bearing for tasks 11.2–11.5.** Reshapes `VaultRunner.entry` from `Arc<VaultEntry>` to `std::sync::RwLock<Arc<VaultEntry>>`; introduces the per-vault `op_lock` acquisition pattern; adds three of the five lifecycle ops. Pause / resume / rename share the "replace entry without lifecycle teardown… or with controlled teardown" shape.

**Scope**:

- **Refactor `VaultRunner.entry`** ([`src/control_plane/runner.rs`](../../../src/control_plane/runner.rs)):
  ```rust
  pub struct VaultRunner {
      entry: std::sync::RwLock<Arc<VaultEntry>>,  // was: Arc<VaultEntry>
      pub(crate) op_lock: Mutex<()>,              // unchanged; remove #[allow(dead_code)]
      lifecycle: Mutex<Option<RunnerLifecycle>>,  // unchanged
  }

  impl VaultRunner {
      pub fn entry(&self) -> Arc<VaultEntry> {
          self.entry
              .read()
              .unwrap_or_else(|e| e.into_inner())
              .clone()
      }

      pub(crate) fn replace_entry(&self, entry: Arc<VaultEntry>) {
          *self.entry.write().unwrap_or_else(|e| e.into_inner()) = entry;
      }
  }
  ```
  Update callers at [`src/control_plane/manager.rs:281–360`](../../../src/control_plane/manager.rs) (`active_vaults`, `search_scope`, `list_names`, `get`) and [`src/api/search.rs:69/143/224`](../../../src/api/search.rs). Each call site changes from `runner.entry()` returning `&Arc<VaultEntry>` to returning `Arc<VaultEntry>` (owned); the `.clone()` calls at [`search.rs:69`](../../../src/api/search.rs) etc. become Arc-clone-of-already-cloned-Arc which the type system handles cleanly.

- **Implement `VaultManager::pause`** ([`src/control_plane/manager.rs`](../../../src/control_plane/manager.rs)):
  ```rust
  pub async fn pause(&self, name_or_id: &str) -> Result<VaultRow, ControlPlaneError>;
  ```
  Flow:
  1. `let id = self.resolve(name_or_id)?;`
  2. Acquire read-lock on `runners`; clone the runner Arc; drop the read-lock.
  3. Acquire `runner.op_lock`.
  4. Read current `runner.entry()`; if status is already `Paused`, return idempotently with the existing row (200 OK with current registry row).
  5. Drain runner via `runner.shutdown_with_timeout(LIFECYCLE_DRAIN_TIMEOUT)`.
  6. UPDATE registry: `status = 'paused'`, `last_error = NULL`.
  7. Construct new `VaultEntry` (clone old, override `status: VaultStatus::Paused`); call `runner.replace_entry(Arc::new(new_entry))`.
  8. Re-read registry row for the response.
  9. Release op_lock; return `VaultRow`.

- **Implement `VaultManager::resume`** ([`src/control_plane/manager.rs`](../../../src/control_plane/manager.rs)):
  ```rust
  pub async fn resume(&self, name_or_id: &str) -> Result<VaultRow, ControlPlaneError>;
  ```
  Flow:
  1. `let id = self.resolve(name_or_id)?;`
  2. Acquire read-lock on `runners`; clone the runner Arc; drop the read-lock.
  3. Acquire `runner.op_lock`.
  4. Read current `runner.entry()`; if status is already `Active`, return idempotently.
  5. If status is `Errored`: validate path accessibility (same check as `reconcile_active_rows` does — `std::fs::metadata(&row.path)` + `is_dir()`). If still inaccessible, return `ControlPlaneError::VaultErrored { name_or_id, last_error: <unchanged> }` (HTTP 503).
  6. Read current registry row; spawn fresh `RunnerLifecycle` via `spawn_runner_for_row(...)` (the existing helper). Install via `runner.lifecycle.lock().await.replace(new_lifecycle)`.
  7. UPDATE registry: `status = 'active'`, `last_error = NULL`.
  8. Construct new `VaultEntry` (status=Active, last_error=None); replace_entry.
  9. Release op_lock; return updated `VaultRow`.

- **Implement `VaultManager::rename`** ([`src/control_plane/manager.rs`](../../../src/control_plane/manager.rs)):
  ```rust
  pub async fn rename(&self, name_or_id: &str, new_name: &str) -> Result<VaultRow, ControlPlaneError>;
  ```
  Flow:
  1. Validate `new_name` against the regex `^[A-Za-z0-9_-]+$` (per spec § Identifier Model). On failure: `ControlPlaneError::VaultPathInvalid { detail: "new_name must match [A-Za-z0-9_-]+ ..." }`.
  2. `let id = self.resolve(name_or_id)?;`
  3. Acquire read-lock on `runners`; clone the runner Arc; drop the read-lock.
  4. Acquire `runner.op_lock`.
  5. Pre-check uniqueness: registry SELECT for any row with `name = new_name` AND `id != current_id`. If found, return `ControlPlaneError::VaultNameConflict { existing_path, name }`.
  6. UPDATE registry: `name = new_name`. Wrap in `spawn_blocking` per `rusqlite-in-async` skill.
  7. Rewrite per-vault meta.toml using the existing tempfile-rename pattern from `create_subdir_and_meta` ([`src/control_plane/manager.rs:796`](../../../src/control_plane/manager.rs)).
  8. Construct new `VaultEntry` (clone old, override `name: new_name.to_string()`); replace_entry. Lifecycle is unchanged — watcher / indexer don't read `name`, so no restart required.
  9. Release op_lock; return updated `VaultRow`.

- **Constants**: add `pub(crate) const LIFECYCLE_DRAIN_TIMEOUT: Duration = Duration::from_secs(30);` at the top of `manager.rs`. Reuse the value from `TERMINATE_DRAIN_TIMEOUT` (or rename `TERMINATE_DRAIN_TIMEOUT` → `LIFECYCLE_DRAIN_TIMEOUT`; either is fine, coordinator decides at task time. Default to the rename for consistency with the broader use.)

**Tests** (in-module unit tests in `src/control_plane/tests.rs`):

- `pause_drains_runner_and_updates_status` — pause on an active vault; assert registry status = paused, runner still in map, lifecycle drained.
- `pause_idempotent_on_already_paused` — pause → pause; second call returns 200 with existing row.
- `pause_returns_vault_not_found_for_unknown`.
- `resume_from_paused_restores_active` — pause then resume; assert lifecycle re-spawned, status=active.
- `resume_from_errored_with_path_accessible_succeeds` — directly insert an `errored`-status row pointing at an accessible path; resume; assert status=active, last_error cleared.
- `resume_from_errored_with_path_inaccessible_returns_503_vault_errored` — point at a non-existent path; resume; assert error.
- `resume_idempotent_on_already_active` — resume on active vault; returns existing row.
- `rename_updates_registry_and_meta_toml` — rename; assert registry name updated, meta.toml's `name = "..."` line updated, surrogate ID unchanged, lifecycle unchanged.
- `rename_validates_new_name_regex` — invalid name (spaces, slashes); assert `VaultPathInvalid` with detail.
- `rename_rejects_name_already_in_use` — second rename to a name another vault holds; assert `VaultNameConflict`.
- `rename_to_same_name_is_noop` — rename to current name; should be allowed (no UPDATE side-effect; meta.toml rewrite is harmless).
- `concurrent_renames_on_different_vaults_run_in_parallel` — spawn two renames against two different vaults; assert both complete without serializing on the outer write-lock.
- `concurrent_pause_and_search_dont_deadlock` — spawn pause + search_scope concurrently; both complete (search read-locks the outer map; pause read-locks the outer map then takes the per-vault op_lock). Verifies the ADR-0010 invariant.

**Files touched**: `src/control_plane/runner.rs` (refactor entry to RwLock; add replace_entry getter/setter), `src/control_plane/manager.rs` (add pause/resume/rename; drop `TERMINATE_DRAIN_TIMEOUT` const if renamed; tighten ControlPlaneError docstrings since `VaultErrored` is no longer reserved-for-step-11), `src/control_plane/tests.rs` (new test cases per above), `src/api/search.rs` (call-site update for new `entry()` ownership shape, if needed — most likely a no-op since `.clone()` already happens).

**Dependencies**: none (lands first; the load-bearing foundation for 11.2–11.5).

**Soft-flag-ready territory**:
- The `VaultRunner.entry` reshape may surface ripples beyond the named files (e.g., test fixtures in `tests/multi_vault_internal.rs` or `tests/vault_control_plane.rs` that hold `&Arc<VaultEntry>` references). Surface as a `coordinator-only` soft flag with the chosen call-site fix per round-3 stable workplan-prose-vs-load-bearing-decision pattern.
- The `TERMINATE_DRAIN_TIMEOUT` rename to `LIFECYCLE_DRAIN_TIMEOUT` (or the alternative of keeping the old name and adding a new one) is task-time judgment; either is acceptable. Default to the rename for clarity.
- If `runner.shutdown_with_timeout` proves to leak any non-Send state across await (it shouldn't — verified by step-10 task 10.2) surface as `next-task-agent` soft flag for Task 11.2's reset-with-rebuild.

### Task 11.2 — `reset` (with `--rebuild`) + `rescan`

**Risk**: medium. Composes 11.1's runner-replacement pattern with two new shapes — `--rebuild`'s SQL-level chunk wipe (Resolution C) and rescan's scanner-trigger surface. Mostly additive on the patterns 11.1 lights up.

**Scope**:

- **Implement `VaultManager::reset`** ([`src/control_plane/manager.rs`](../../../src/control_plane/manager.rs)):
  ```rust
  pub async fn reset(&self, name_or_id: &str, rebuild: bool) -> Result<VaultRow, ControlPlaneError>;
  ```
  Flow:
  1. `let id = self.resolve(name_or_id)?;`
  2. Acquire read-lock on `runners`; clone runner Arc; drop the read-lock.
  3. Acquire `runner.op_lock`.
  4. Drain runner via `runner.shutdown_with_timeout(LIFECYCLE_DRAIN_TIMEOUT)`.
  5. **If `rebuild = true`**: open the per-vault `Store` directly (the runner's Arc<Store> is preserved across drain; spawn_blocking the SQL block). Run a single transaction: `DELETE FROM chunks_vec; DELETE FROM chunks; UPDATE files SET content_hash = '';`. (The migration-0004 line-for-line pattern; spec preserves the outbox so we don't touch outbox.jsonl.)
  6. UPDATE registry: `status = 'active'`, `last_error = NULL`.
  7. Spawn fresh `RunnerLifecycle` via `spawn_runner_for_row`; install in `runner.lifecycle`.
  8. Replace entry with status=Active, last_error=None.
  9. Release op_lock; return updated `VaultRow`.

  **Soft-deletion ordering note**: the rebuild SQL must run *between* the lifecycle drain (step 4) and the lifecycle re-spawn (step 7) — otherwise the new scanner could try to read partially-deleted chunks_vec rows. Document this ordering in the function's body comment.

- **Implement `VaultManager::rescan`** ([`src/control_plane/manager.rs`](../../../src/control_plane/manager.rs)):
  ```rust
  pub async fn rescan(&self, name_or_id: &str) -> Result<RescanResponse, ControlPlaneError>;

  pub struct RescanResponse {
      pub row: VaultRow,
      pub rescan_initiated_at: chrono::DateTime<chrono::Utc>,
  }
  ```
  Flow:
  1. `let id = self.resolve(name_or_id)?;`
  2. Acquire read-lock on `runners`; clone runner Arc; drop the read-lock.
  3. Acquire `runner.op_lock`.
  4. Trigger the scanner via the existing scanner channel surface (per [`src/indexer/mod.rs`](../../../src/indexer/mod.rs)'s scanner-task design). The scanner runs a full directory walk and emits `created` / `modified` events per file as if from cold start — this is the existing scanner behavior; rescan is "kick off another scanner pass."
  5. The rescan runs **asynchronously** per spec (response returns before completion). Set `rescan_initiated_at = Utc::now()`.
  6. Release op_lock; return `RescanResponse { row, rescan_initiated_at }`.

  **Implementation note for rescan**: the existing scanner in step 9 runs once at startup. Step 11's rescan needs to invoke it again on demand. The cleanest shape is an additional channel on the runner that the manager sends a "rescan requested" signal to; the consumer task observes the signal and runs another scanner pass. Coordinator+task agent decides at task time whether to add a new `tokio::sync::watch<bool>` for "rescan-requested" or expose a method on the runner that takes a fresh `Scanner::scan()` call. Default to the watch-channel shape to mirror the existing shutdown-channel pattern.

  **Cold-start emission policy**: per spec, rescan emits events for every file as if from cold start. The consumer's natural behavior already matches this — the indexer's hash-comparison logic produces `modified` events for content_hash differences, `created` for new files. Rescan-without-rebuild may produce few outbox events on a vault that's already up-to-date (every file's content_hash matches); operators wanting "cold-start emission for every file" should use `reset --rebuild` (which clears content_hash and forces re-emit). This is a subtle distinction; document it inline.

**Tests** (extend `src/control_plane/tests.rs`):

- `reset_without_rebuild_clears_last_error_and_restarts_runner` — directly insert errored row; reset; assert active + last_error=None + lifecycle re-spawned.
- `reset_with_rebuild_clears_chunks_chunks_vec_and_content_hash` — pre-populate per-vault `index.sqlite` with files/chunks/chunks_vec rows; reset --rebuild; assert chunks_vec empty, chunks empty, files retained but content_hash = ''.
- `reset_with_rebuild_preserves_outbox` — pre-populate outbox.jsonl with a known event; reset --rebuild; assert outbox.jsonl unchanged at the byte level.
- `reset_returns_vault_not_found_for_unknown`.
- `rescan_returns_rescan_initiated_at_timestamp` — rescan; assert response carries a recent timestamp; doesn't block on scanner completion.
- `rescan_re_emits_outbox_events_for_all_files_after_rebuild` — pre-populate vault directory; reset --rebuild; rescan; tail outbox; assert every file produced a created or modified event. (Ordering: rebuild then rescan is a common operator workflow.)
- `rescan_returns_vault_not_found_for_unknown`.
- `concurrent_reset_and_search_dont_deadlock` — spawn reset + search_scope concurrently; both complete cleanly.

**Files touched**: `src/control_plane/manager.rs` (add reset / rescan methods + RescanResponse type), `src/control_plane/runner.rs` (add rescan-requested channel if Coordinator chooses that shape), `src/control_plane/tests.rs` (new test cases), possibly `src/indexer/mod.rs` (small surface for "trigger another scanner pass" if the existing scanner-task design doesn't already expose it).

**Dependencies**: 11.1.

**Soft-flag-ready territory**:
- The rescan-channel shape (new `watch<bool>` vs. method-on-runner) is task-time judgment; surface either choice as a `next-task-agent` soft flag for Task 11.3 (HTTP routes) only if the choice affects the route handler shape. Default-shape: watch-channel mirrors shutdown-channel; minimal ripple.
- If the "rescan emits few events on already-up-to-date vault" subtlety produces operator confusion in Task 11.7's smoke runs, that's a documentation surface for Task 11.8 — coordinator-only soft flag.

### Task 11.3 — HTTP control-plane routes for the five lifecycle ops

**Risk**: medium. Mostly serde plumbing + ApiError mapping on top of Tasks 11.1 + 11.2's `VaultManager` methods. Tests are unit-level against the route surface.

**Scope**:

- New routes (extend [`src/api/vaults.rs`](../../../src/api/vaults.rs) and [`src/api/mod.rs`](../../../src/api/mod.rs)):
  - `POST /vaults/{name_or_id}/pause` — no body; returns updated `VaultRow`.
  - `POST /vaults/{name_or_id}/resume` — no body; returns updated `VaultRow`.
  - `POST /vaults/{name_or_id}/reset` — body `{rebuild?: bool}` (default false); returns updated `VaultRow`.
  - `POST /vaults/{name_or_id}/rename` — body `{new_name: string}`; returns updated `VaultRow`.
  - `POST /vaults/{name_or_id}/rescan` — no body; returns `{...VaultRow, rescan_initiated_at: ISO-8601}`.
- Wire `ControlPlaneError::VaultErrored` → `ApiError` per spec § Error Handling table (HTTP 503 `vault_errored`). The existing `From<ControlPlaneError> for ApiError` impl in [`src/api/error.rs`](../../../src/api/error.rs) already covers `VaultErrored`; verify at task time.
- Add request/response types to [`src/api/types.rs`](../../../src/api/types.rs):
  ```rust
  pub struct ResetRequest { #[serde(default)] pub rebuild: bool }
  pub struct RenameRequest { pub new_name: String }
  pub struct RescanResponse { /* flatten VaultRowJson + rescan_initiated_at */ }
  ```
- Idempotency: pause-on-paused / resume-on-active return 200 with the existing row (per spec § pause § resume).
- Router wiring at [`src/api/mod.rs`](../../../src/api/mod.rs): five new routes following the existing `/vaults/:name_or_id` pattern.

**Tests** (extend [`src/api/tests.rs`](../../../src/api/tests.rs); deeper integration tests land in Task 11.7):

- `post_vaults_pause_returns_200_with_updated_row`.
- `post_vaults_pause_unknown_returns_404`.
- `post_vaults_resume_returns_200_with_updated_row`.
- `post_vaults_resume_errored_path_inaccessible_returns_503_vault_errored`.
- `post_vaults_reset_returns_200_with_updated_row` — without rebuild.
- `post_vaults_reset_with_rebuild_true_returns_200_and_clears_chunks` — verify rebuild side-effect via store inspection.
- `post_vaults_rename_returns_200_with_new_name`.
- `post_vaults_rename_invalid_new_name_returns_422_vault_path_invalid`.
- `post_vaults_rename_collision_returns_409_vault_name_conflict`.
- `post_vaults_rescan_returns_200_with_rescan_initiated_at`.
- `post_vaults_unknown_op_path_returns_404` (e.g., `/vaults/foo/bogus`) — axum's standard 404; not load-bearing but documents the shape.

**Files touched**: `src/api/vaults.rs` (new handlers), `src/api/mod.rs` (router wiring for 5 new routes), `src/api/types.rs` (3 new types), `src/api/error.rs` (verify `VaultErrored` mapping; tighten if needed), `src/api/tests.rs` (new test cases).

**Dependencies**: 11.1, 11.2.

### Task 11.4 — `hmn vault {pause,resume,reset,rename,rescan}` CLI subcommands + DaemonClient extension

**Risk**: medium. Operator-facing surface for the new lifecycle ops. Bundles destructive-op confirmation prompts (per spec § Integration Points § With CLI: "Confirmation prompts on destructive ops (`terminate`, `reset --rebuild`, `rescan`); skipped with `--yes` for non-interactive use").

**Scope**:

- Extend `enum VaultOp` in [`src/cli.rs:50`](../../../src/cli.rs):
  ```rust
  pub enum VaultOp {
      // ... existing four variants ...
      Pause {
          target: String,
      },
      Resume {
          target: String,
      },
      Reset {
          target: String,
          /// Drop and rebuild chunks + chunks_vec; preserves files + outbox.
          #[arg(long)]
          rebuild: bool,
          /// Skip the destructive-op confirmation prompt (required for --rebuild).
          #[arg(long)]
          yes: bool,
      },
      Rename {
          target: String,
          #[arg(long, value_name = "NEW_NAME")]
          new_name: String,
      },
      Rescan {
          target: String,
          /// Skip the destructive-op confirmation prompt.
          #[arg(long)]
          yes: bool,
      },
  }
  ```
- Subcommand handlers in [`src/bin/hmn.rs`](../../../src/bin/hmn.rs) (or its dispatch shape — verify at task time):
  - `Pause`: call `client.pause_vault(&target)`; render result row.
  - `Resume`: call `client.resume_vault(&target)`; render result row.
  - `Reset`: if `rebuild == true && !yes`, prompt `"Reset vault '<target>' and rebuild chunks? (y/N) "`. On confirm or `--yes`, call `client.reset_vault(&target, rebuild)`. Without `--rebuild`, skip the prompt (clearing last_error is non-destructive).
  - `Rename`: call `client.rename_vault(&target, &new_name)`; render result row.
  - `Rescan`: prompt `"Rescan vault '<target>'? This will re-emit outbox events. (y/N) "` unless `--yes`. On confirm, call `client.rescan_vault(&target)`; render result row + rescan_initiated_at.
- Extend `DaemonClient` in [`src/client.rs`](../../../src/client.rs) with five new methods:
  ```rust
  pub async fn pause_vault(&self, target: &str) -> Result<VaultRowJson>;
  pub async fn resume_vault(&self, target: &str) -> Result<VaultRowJson>;
  pub async fn reset_vault(&self, target: &str, rebuild: bool) -> Result<VaultRowJson>;
  pub async fn rename_vault(&self, target: &str, new_name: &str) -> Result<VaultRowJson>;
  pub async fn rescan_vault(&self, target: &str) -> Result<RescanResponseJson>;
  ```
  Each is a thin reqwest wrapper. Path-segment percent-encoding via `reqwest::Url::path_segments_mut` per Task 10.4's existing pattern.
- The `--yes` flag is **required for `reset --rebuild`** in non-interactive mode (CI, scripted runs). The CLI prompt lives in `src/bin/hmn.rs`; CI pipes `y\n` or passes `--yes`.

**Tests**:

- clap parsing tests in `src/cli.rs::tests`:
  - `parses_vault_pause_with_target`.
  - `parses_vault_resume_with_target`.
  - `parses_vault_reset_with_target`.
  - `parses_vault_reset_with_rebuild_and_yes`.
  - `parses_vault_rename_with_new_name`.
  - `parses_vault_rescan_with_target_and_yes`.
- E2E tests in [`tests/cli.rs`](../../../tests/cli.rs) extending the round-3 step-10 fixture:
  - `hmn_vault_pause_then_resume_round_trip` — pause; assert status=paused; resume; assert status=active.
  - `hmn_vault_rename_updates_vault_list`.
  - `hmn_vault_reset_without_rebuild_clears_errored_state` — directly insert errored row; reset; assert active.
  - `hmn_vault_reset_with_rebuild_yes_succeeds` — full rebuild flow.
  - `hmn_vault_rescan_with_yes_returns_rescan_initiated_at`.
  - `hmn_vault_rescan_without_yes_prompts_and_aborts_on_no` — pipe `n\n` to stdin; assert no rescan triggered (subsequent outbox tail shows no new events).

**Files touched**: `src/cli.rs` (extend VaultOp), `src/bin/hmn.rs` (new dispatch arms + prompt UX), `src/client.rs` (five new DaemonClient methods), possibly `src/api/types.rs` (re-exports for client if `RescanResponseJson` lands there).

**Dependencies**: 11.3.

### Task 11.5 — MCP tools (`vault_pause`, `vault_resume`, `vault_reset`, `vault_rename`, `vault_rescan`)

**Risk**: medium. Builds on step-10 task 10.6's MCP wrapper pattern; the new tool registrations follow the existing `tool_router` macro. Each is a thin shim over the corresponding `DaemonClient` method. All five are write tools and gated by the existing `[mcp] enable_write_tools` flag (already wired in step 10).

**Scope**:

- Extend `HypomnemaMcpServer` in [`src/mcp/server.rs`](../../../src/mcp/server.rs) with five new `#[tool]` methods:
  - `vault_pause(&self, Parameters(VaultPauseInput))` — input `{target: String}`.
  - `vault_resume(&self, Parameters(VaultResumeInput))` — input `{target: String}`.
  - `vault_reset(&self, Parameters(VaultResetInput))` — input `{target: String, rebuild?: bool}` (default false).
  - `vault_rename(&self, Parameters(VaultRenameInput))` — input `{target: String, new_name: String}`.
  - `vault_rescan(&self, Parameters(VaultRescanInput))` — input `{target: String}`.
- Each method short-circuits to the structured `write_tools_disabled` error envelope when `self.enable_write_tools == false` (mirrors step-10 task 10.6's pattern verbatim).
- Tool descriptions reference the relevant spec section. Examples:
  - `vault_pause`: `"Pause a vault: stop its watcher and indexer; index preserved; vault silently skipped from default search scope. Disabled when [mcp] enable_write_tools = false. See docs/specs/vault-management.md § pause."`
  - `vault_reset`: `"Reset a vault: clear last_error and restart watcher + indexer. With rebuild=true, also drop and rebuild chunks + chunks_vec (preserves files + outbox). Disabled when [mcp] enable_write_tools = false. See docs/specs/vault-management.md § reset."`
- Input types in [`src/api/types.rs`](../../../src/api/types.rs) (or a new `src/mcp/types.rs` module if the surface grows large; coordinator decides at task time):
  ```rust
  pub struct VaultPauseInput { pub target: String }
  pub struct VaultResumeInput { pub target: String }
  pub struct VaultResetInput { pub target: String, #[serde(default)] pub rebuild: bool }
  pub struct VaultRenameInput { pub target: String, pub new_name: String }
  pub struct VaultRescanInput { pub target: String }
  ```
  schemars derive per step-10's existing pattern (`#[schemars(crate = "rmcp::schemars")]`).

**Tests** (extend `src/mcp/server.rs::tests`):

- `mcp_vault_pause_succeeds_when_write_tools_enabled` (mock daemon).
- `mcp_vault_pause_returns_write_tools_disabled_when_gated`.
- (5 × 2 = 10 cases following the step-10 task 10.6 pattern: pause, resume, reset, rename, rescan, each with enabled and gated variants.)
- `mcp_vault_reset_with_rebuild_passes_rebuild_through_to_daemon` — verify the rebuild flag round-trips through the wire.

**Files touched**: `src/mcp/server.rs` (5 new `#[tool]` methods), `src/api/types.rs` (5 new input types), `src/mcp/mod.rs` (re-exports if needed). No `src/config.rs` change — `enable_write_tools` is already there from step 10.

**Dependencies**: 11.3 (shares request/response types), 11.4 (DaemonClient methods).

### Task 11.6 — Remove `hmnd scan`

**Risk**: low. Surgical removal per Resolution D. Distinct task with its own commit so the bisect anchor for "scan removed" is clean.

**Scope**:

- Remove `Scan` variant from `enum Command` in [`src/bin/hmnd.rs:33`](../../../src/bin/hmnd.rs).
- Remove the `Some(Command::Scan) => do_scan(&config).await` arm in `dispatch` ([`src/bin/hmnd.rs:69`](../../../src/bin/hmnd.rs)).
- Remove the `do_scan` async function (full body; verify via `cargo check` that nothing else calls it).
- Remove any imports that become unused as a result (e.g., `Scanner`, `Store` imports may stay used by `run_daemon`; verify each at task time).
- Update the module docstring in [`src/watcher/mod.rs:34`](../../../src/watcher/mod.rs) — replace `hmnd scan` reference with `hmn vault rescan`.
- Verify `docs/reference/cli.md`'s `hmnd scan` removal note is accurate post-change. (Step-10 already declared "removed in 0.2.0"; this task makes that prose true. Apply the round-3-step-10 "soft-flag self-correction at boundary" pattern: verify the prose is current before editing.)
- Grep `tests/` for any `hmnd scan` references; update or remove. (Verified at workplan-write: no matches in `tests/scan.rs` for `hmnd scan` invocation; the test runs the daemon's scan logic via library APIs, not the CLI subcommand.)

**Tests**:

- Adjust `tests/scan.rs` if it depends on the `hmnd scan` CLI subcommand. (Workplan-write spot-check: `tests/scan.rs` exists; verify by reading at task-time whether it invokes `Command::Scan` at all. If yes, replace with library-API equivalent. If no, no test changes needed.)
- `cargo test` clean post-change.
- `cargo run --bin hmnd -- scan` should produce a clean clap error (manual verification, not a unit test).

**Files touched**: `src/bin/hmnd.rs` (remove Scan variant + arm + do_scan fn), `src/watcher/mod.rs` (docstring update), possibly `tests/scan.rs` (adjust if it depends on the CLI subcommand). No spec or ADR changes — ADR-0011 already specifies removal.

**Dependencies**: none functional; can land in parallel with 11.3–11.5 in principle, but coordinator may sequence after 11.4 so the CLI replacement is in tree alongside the removal. Default sequencing: after 11.4.

**Soft-flag-ready territory**:
- If `tests/scan.rs` invokes `hmnd scan` and the test surface is non-trivial to rewrite, surface as a `coordinator-only` soft flag with the recommended replacement pattern (likely "drop the test entirely if scan-via-library is already covered by tests/multi_vault_internal.rs's scanner cases").
- If any operator-facing doc beyond `cli.md` mentions `hmnd scan`, defer the cleanup to Task 11.8's boundary doc pass — coordinator-only soft flag.

### Task 11.7 — Integration tests + multi-vault end-to-end manual smoke (round-3 shipping gate)

**Risk**: medium-high. **Manual smoke verification is load-bearing here** per the round-2 step-7 / round-2 step-8 / round-3 step-9 / round-3 step-10 precedent (now 6-of-6 wiring tasks). Composes 11.1–11.6 into the round-3 shipping gate test matrix. The roadmap specifies this test as "the round-3 analogue of round-2 step-8's Claude-Code-in-the-loop test."

**Scope**:

- Extend [`tests/vault_control_plane.rs`](../../../tests/vault_control_plane.rs) (the step-10 fixture) with the five new lifecycle ops over HTTP. Tests:
  - `http_pause_then_resume_round_trip` — create vault; pause; assert search response includes the vault in `partial_results.skipped` with `status: "paused"`; resume; assert search query returns vault's results normally.
  - `http_reset_clears_errored_state` — directly insert errored row; POST /reset; assert active.
  - `http_reset_with_rebuild_clears_chunks_chunks_vec_and_content_hash` — full rebuild flow + outbox preservation.
  - `http_rename_updates_search_response_vault_name` — rename mid-test; subsequent search results carry new vault_name; outbox events continue to carry surrogate ID (unchanged).
  - `http_rescan_emits_outbox_events_for_existing_files` — rescan; tail outbox; assert events.
  - `http_pause_idempotent`.
  - `http_resume_idempotent`.
  - `http_reset_returns_404_for_unknown`.
  - `http_rename_returns_409_on_collision`.
  - `concurrent_pause_and_terminate_dont_corrupt_state` — serialize-on-same-vault test: pause + terminate → first wins, second errors cleanly (`vault_not_found` on the post-terminate op).
- 3× consecutive flake-check clean run on the new tests (matching round-1/2/3 anti-flake convention).
- **Manual smoke verification** — the round-3 shipping gate. Per the roadmap: "real-shape multi-vault" pass: multiple vaults of mixed sizes, watcher behavior under concurrent FS events across vaults, control-plane operations triggered while indexing is in flight, search results coherent across the full operation matrix.

  **Smoke matrix** (document each in the task's results comment with the full transcript):
  1. **Three-vault setup with mixed sizes**: empty `<data_dir>`, no `[vault]` config, `default_vault_name = "default"`. Daemon starts, idles. Create vault A (small — ~10 markdown files), vault B (medium — ~100 files), vault C (large — ~1000 files; can be a checkout of an existing repo's `docs/` directory). Wait for indexing convergence (timing depends on embedding service availability — operator-controlled).
  2. **Concurrent FS events across vaults**: while indexing is in flight on vault C, write 3 markdown files to vault A and 3 to vault B. Verify outbox tails on both vaults emit `created` events; cross-vault search returns intermingled results during the indexing window.
  3. **Pause vault B mid-indexing**: pause B while it's still indexing. Verify (a) the in-flight file completes (outbox shows the `modified` event for it), (b) subsequent files are NOT indexed (no further events), (c) registry shows status=paused, (d) search responses include B in `partial_results.skipped` with status=paused.
  4. **Resume vault B**: resume; verify watcher + indexer restart; subsequent file writes to B produce outbox events again.
  5. **Reset vault A with --rebuild**: `hmn vault reset vault-a --rebuild --yes`. Verify (a) chunks_vec rows for A are cleared, (b) files rows preserved (count unchanged), (c) outbox preserved (byte-level unchanged before reset), (d) post-reset, indexer re-embeds every file (semantic search recovery test: query for content known to be in vault A; result returned with vault_name=vault-a).
  6. **Rename vault A to my-notes**: `hmn vault rename vault-a --new-name=my-notes`. Verify (a) `hmn vault list` shows new name, (b) `<data_dir>/vaults/<id>/meta.toml` updated, (c) per-vault subdirectory unchanged (UUIDv7 path), (d) cross-vault search results carry `vault_name: "my-notes"` for vault A's results, (e) outbox events continue to carry the same surrogate `vault` ID.
  7. **Rescan vault C**: `hmn vault rescan vault-c --yes`. Verify (a) outbox tail shows events for every file currently in vault C (after embedding completes), (b) `hmn vault status vault-c` shows `rescan_initiated_at` populated, (c) the rescan completes asynchronously (response returns before indexing converges).
  8. **`hmnd scan` removal smoke**: `cargo run --bin hmnd -- scan` produces a clean clap error pointing at help. Documented as expected behavior.
  9. **Optional MCP smoke** (round-4 prep — only if Claude Code is available locally): `claude` invokes `vault_pause`, `vault_resume`, `vault_reset`, `vault_rename`, `vault_rescan` against a multi-vault daemon. If `[mcp] enable_write_tools = false` is set, all five return `write_tools_disabled` envelope.

  Document each smoke run's transcript verbatim in the task's results comment per the round-2/3 precedent.

**Files touched**: `tests/vault_control_plane.rs` (extend with ~10 new tests) — or a new `tests/vault_lifecycle.rs` if the file size grows beyond ~1500 lines (coordinator decides at task time based on test surface).

**Dependencies**: 11.3 (HTTP routes), 11.4 (CLI for smoke), 11.5 (MCP tools — light-touch in tests since the round-2 step-8 mock-daemon pattern covers the unit-level cases), 11.6 (scan removal — Smoke 8 verifies).

**Soft-flag-ready territory**:
- The pre-existing outbox flake (`tests/outbox.rs::rename_emits_deleted_then_created_lines`, ~17% repro rate per step-10 retro) may surface in the full-suite quality-gate sweep. Coordinator-only soft flag: scope is round-4 flake-hardening pass; not step-11's surface.
- Smoke transcripts: if any smoke step surfaces operator-UX friction (e.g., the rescan prompt is confusing, the reset --rebuild output doesn't clearly indicate the rebuild ran), surface as coordinator-only for Task 11.8's docs and the boundary retro.
- If a per-vault concurrent FS-event-storm surfaces a real watcher bug under the heightened multi-vault load, that's a task-time escalation candidate per the round-3-step-6 act-now-vs-defer-to-boundary decision rule.

### Task 11.8 — Reference docs + roadmap-3 status + boundary prep

**Risk**: low. Doc-only by design; lands at boundary so any forward-noted soft-flag corrections from earlier tasks can be incorporated. Includes the round-3-shipping-gate boundary preparations (version-bump notes, round-3 archival notes, round-4 backlog seed).

**Scope**:

- [`docs/reference/cli.md`](../../../docs/reference/cli.md):
  - Add full subcommand documentation for `hmn vault {pause,resume,reset,rename,rescan}`. Each with flag reference, examples, exit codes, JSON output mode (`--json`).
  - Document the `--yes` confirmation behavior on `terminate`, `reset --rebuild`, `rescan`. (The existing `terminate --yes` doc carries forward; extend with the new ops.)
  - Verify the `hmnd scan` removal note (already present at doc step-10 boundary; verify-before-editing per step-10's "soft-flag self-correction at boundary" pattern).
  - Cross-reference to `docs/specs/vault-management.md` for full operation semantics.
- [`docs/reference/configuration.md`](../../../docs/reference/configuration.md):
  - Verify `[mcp] enable_write_tools` documentation from step 10 still accurate; extend to mention the new write tools (`vault_pause` through `vault_rescan`) are gated by the same flag.
  - Verify `default_vault_name` documentation from step 9 still accurate.
- [`docs/architecture/overview.md`](../../../docs/architecture/overview.md):
  - Extend § Vault Manager / Control Plane (the section step-10 added) with the five new lifecycle ops + the per-vault `op_lock` acquisition pattern + the interior-mutability shape on `VaultRunner.entry`.
  - Document the rescan-channel pattern (if Task 11.2 adds one) under § Watcher / Indexer integration.
- Update [`notes/roadmap/roadmap-3.md`](../roadmap-3.md) § Step 11 status:
  - Add `**Status**: Shipped <ship date>` at top of Step 11 section.
  - Cross-reference the workplan archive path.
  - **Round-3 shipping note**: this is the round-3 shipping gate; the round-3 boundary moves the round-3 roadmap to `notes/roadmap/archive/roadmap-3.md` per the round-1/2 archival precedent.
- Update [`notes/backlog.md`](../../backlog.md):
  - Add a Round-4 candidate entry for **Compose-style declarative layer** with rationale (Resolution A: deferred from step 11; vault-management.md § Compose-Style Declarative Layer (deferred) covers the surface; round-4 workplan pins format + merging rules).
  - Note any round-3 boundary follow-ups surfaced by Tasks 11.1–11.7's soft flags.
- Verify [`notes/roadmap/step-11-workplan.md`](./step-11-workplan.md) (this file) is up-to-date with shipping criteria; the boundary ritual handles archival to `notes/roadmap/archive/step-11-workplan.md`.
- **Version-bump prep note**: `Cargo.toml` is at `version = "0.1.0"` at workplan-write. Round-3 shipping gate aligns with `v0.2.0` per the boundary ritual call. The version bump itself is a boundary-ritual action (alongside the milestone tag), not a Task 11.8 action — Task 11.8 prepares the docs to be consistent with `v0.2.0` (release notes-style content can live in a CHANGELOG or in the round-3 archival note; coordinator decides at task time based on whether the project has adopted a CHANGELOG.md yet).

**Tests**: doc-only; no code tests in this task. `cargo doc --no-deps` runs cleanly post-edit.

**Files touched**: `docs/reference/cli.md`, `docs/reference/configuration.md`, `docs/architecture/overview.md`, `notes/roadmap/roadmap-3.md`, `notes/backlog.md`. (The workplan archive itself and the round-3 archival are part of the post-task boundary ritual run by the coordinator after this task ships.)

**Dependencies**: 11.1–11.7. Lands last.

**Soft-flag-ready territory**:
- Forward-noted soft-flag reconciliations from earlier tasks (likely 3–5 of them per the round-3 stable pattern). Apply the round-3-step-10 "soft-flag self-correction at boundary" rule: verify the prose is current before editing; the prior task's observation may have been the drift.
- Version-bump policy is a workplan-time decision left for the boundary ritual itself (the human + coordinator decide together). If `v0.2.0` is the call, Task 11.8 records the call in roadmap-3.md's § After round 3 section; if a different version policy emerges, Task 11.8 picks up that signal.
- CHANGELOG.md adoption is a project-wide policy question that may or may not surface at this boundary; coordinator-only soft flag if it does (decides whether to start a CHANGELOG now or defer to round 4).

---

## Shipping criteria

The step ships when **all** of these hold:

- [ ] All step-9 / step-10 integration tests pass unchanged: `tests/scan.rs`, `tests/watch.rs`, `tests/outbox.rs`, `tests/embedding.rs`, `tests/mcp.rs`, `tests/multi_vault_internal.rs`, `tests/vault_control_plane.rs`, plus skeleton/config tests. Existing single-vault and step-10-multi-vault behavior is fully preserved.
- [ ] `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test` all green.
- [ ] `hmn vault pause NAME|ID` and `hmn vault resume NAME|ID` round-trip cleanly: pause stops watcher + indexer, search response includes the vault in `partial_results.skipped`; resume restarts both, search returns the vault's results normally.
- [ ] `hmn vault reset NAME|ID` clears `last_error` and restarts watcher + indexer; `hmn vault reset NAME|ID --rebuild --yes` additionally drops chunks + chunks_vec and clears `files.content_hash`, preserving outbox; the next indexing pass re-embeds every file.
- [ ] `hmn vault rename TARGET --new-name=NEW_NAME` is a single registry UPDATE; per-vault subdirectory unchanged; subsequent search responses carry the new `vault_name`; outbox events continue to carry the unchanged surrogate `vault` ID.
- [ ] `hmn vault rescan TARGET --yes` triggers a fresh scanner pass; outbox emits per-file events; HTTP response carries `rescan_initiated_at`; the rescan runs asynchronously.
- [ ] `hmnd scan` is removed from the CLI surface; `cargo run --bin hmnd -- scan` produces a clap error; `src/bin/hmnd.rs::Command::Scan` and `do_scan` are gone; `src/watcher/mod.rs` references `hmn vault rescan` instead.
- [ ] HTTP control-plane routes for the five lifecycle ops (`POST /vaults/{name_or_id}/pause`, `/resume`, `/reset`, `/rename`, `/rescan`) match the spec § Data Schema § Control-Plane HTTP Wire Shapes; HTTP error codes match § Error Handling (404 `vault_not_found`, 409 `vault_name_conflict` on rename, 422 `vault_path_invalid` on rename validation, 503 `vault_errored` on resume-from-errored-with-inaccessible-path).
- [ ] All five MCP tools (`vault_pause`, `vault_resume`, `vault_reset`, `vault_rename`, `vault_rescan`) advertised when `[mcp] enable_write_tools = true`; return `write_tools_disabled` envelope when `false`. An MCP-capable agent can invoke each and get back the spec response shapes.
- [ ] Cross-vault search semantics from step 10 still hold: paused vaults skipped, errored vaults skipped, partial_results diagnostic populated; rename mid-query is safe (Arc clone protects the in-flight search).
- [ ] All eight scenarios in Task 11.7's manual smoke matrix produce the documented outputs; transcripts are recorded in the Task 11.7 results comment.
- [ ] 3× consecutive flake-check clean run on `cargo test` (matching round-1/2/3 anti-flake convention).
- [ ] Reference docs (cli, configuration, architecture) updated; roadmap-3 § Step 11 marked shipped; backlog.md has a round-4 Compose entry; round-3 boundary version-bump prep noted.
- [ ] One commit per task per the playbook (Task 11.7's smoke can use the round-3 step-9 / step-10 single-commit-with-inline-transcripts pattern).

## Step boundary follow-ups (anticipated)

- **Compose-style declarative layer** (Resolution A): deferred to round 4. `notes/backlog.md` § Round 4 candidates lists it alongside MCP Streamable HTTP, agent-host integration, etc. The round-4 workplan pins format + merging rules.
- **Round-4 flake-hardening pass** (carried forward from step-10 retro): pre-existing outbox flake (`tests/outbox.rs::rename_emits_deleted_then_created_lines`) needs investigation against macOS / Linux event-coalescing semantics. Step-11 does not touch this surface.
- **`flake.nix` dylib provisioning** (carried forward from step-6 boundary): operator prereq for sqlite-vec dylib remains in `docs/reference/configuration.md`. Round-4 candidate.
- **Cross-platform rename safety** (step-9 boundary follow-up): documented same-filesystem assumption in `docs/reference/configuration.md`. If a Windows operator surfaces, revisit.
- **CHANGELOG.md adoption**: round-3 shipping gate is a natural moment to settle whether the project starts a CHANGELOG. Round-4 candidate if not adopted at boundary.
- **Multi-model embedding per vault**: spec § Open Questions; round-4+ if a use-case surfaces.
- **MCP write-tool gating granularity**: step-10 committed to single `enable_write_tools` flag; per-tool gating is round-4+ if a use-case surfaces.
- **Rescan emission policy** (Task 11.2 documentation surface): rescan-without-rebuild on an up-to-date vault produces few outbox events (content_hash matches → no `modified` event). Operators wanting cold-start emission for every file should use `reset --rebuild`. Documented inline; if operators consistently confuse the two, reconsider at round 4.
- **Pre-existing inactive-row defensive path** (carried forward from step-10 task 10.5): `(VaultStatus::Active, None) => failed.push(no_runner_failure(...))` defensive arm in [`src/api/search.rs:82/156/238`](../../../src/api/search.rs) was a step-11-pause/resume drop-in pre-stage. Step 11 makes the inactive-with-runner case real (pause leaves a runner with `lifecycle = None`); verify the defensive path is still correct or update at task time.

---

## Notes on workplan-write deferred-decision handling

The five workplan-time deferred decisions per [`roadmap-3.md`](../roadmap-3.md) § Step 11 are resolved in § Deferred-decision resolutions above:

- **Resolution A** — Compose-layer ship-vs-defer: defer to round 4.
- **Resolution B** — Compose file format: N/A given A.
- **Resolution C** — `--rebuild` flag on reset: ship in step 11; reuses migration-0004 SQL pattern.
- **Resolution D** — `hmnd scan` deprecation policy: hard-remove in step 11.
- **Resolution E** — Concurrency posture for in-flight indexing during pause: drain to single-file boundary; 30s force-abort cap.
- **Resolution F** (workplan-surfaced supplement) — `VaultRunner.entry` interior-mutability refactor: needed to support pause/resume/rename without runner teardown. Lands in Task 11.1.
- **Resolution G** (workplan-surfaced supplement) — Per-vault `op_lock` acquisition pattern: ADR-0010 invariant ("operations on the same vault are serialized; operations on different vaults run in parallel") is implemented for the five new ops via the `op_lock` constructed in step 10 and acquired for the first time in step 11.

No spec amendments anticipated (vault-management.md is at v1.0.0 covering all nine operations from step 10 fleshout). If implementation surfaces a contract surprise, the natural recovery is `vault-management.md` → 1.1.0 amendment (no ADR needed); flagged in step-10 retro § Step-boundary follow-ups.

---

## Notes on round-3 shipping gate

This step is the round-3 shipping gate. The boundary ritual is the **full milestone-tag + per-step + end-of-round retro variant** per the prompt and per [`notes/project-planning-workflow-notes.md`](../../project-planning-workflow-notes.md) § Step boundary ritual + § End-of-round retrospective.

Boundary ritual sequence (run by coordinator after Task 11.8 ships):

1. **Mark step 11 shipped** in `notes/roadmap/roadmap-3.md` § Step 11 with shipping date.
2. **Tag the milestone in git** — likely `v0.2.0` (round-3 bumps minor for the multi-vault surface). The version-bump call is the human's; coordinator drafts the tag message and asks before tagging.
3. **Bump `Cargo.toml` version**: `0.1.0` → `0.2.0` (or whatever the version-bump call resolves to).
4. **Capture any ADRs** that hardened during the build. Likely candidates: none anticipated (cross-vault search semantics landed in spec at step 10; lifecycle ops are pure spec-execution; no new ADR-shaped decisions surfaced at workplan-write). If anything surfaces during build, surface as `coordinator-only` soft flag.
5. **Per-step retro for step 11** in `notes/project-planning-workflow-notes.md` § Step 11. Apply the retro template; capture structured eval + free-form notes.
6. **End-of-round retro for round 3** in the same file. Round scope: roadmap steps 9–11 — per-vault internal refactor (9) → control plane create/list/status/terminate + cross-vault search (10) → remaining lifecycle ops + scan removal (11). 24 task agents across 3 steps (8+8+8), 3 coordinators, 1 orchestrator (Solo agent across the round). Apply the round-1/2 end-of-round retro shape.
7. **Archive round-3 roadmap**: `notes/roadmap/roadmap-3.md` → `notes/roadmap/archive/roadmap-3.md` per the round-1/2 archival precedent (round-2 archived immediately when round 2 shipped).
8. **Archive step-11 workplan**: `notes/roadmap/step-11-workplan.md` → `notes/roadmap/archive/step-11-workplan.md` per the step-archival policy.
9. **Update `notes/backlog.md`** with round-3 boundary follow-ups (already partially seeded by Task 11.8).
10. **Round-4 roadmap?** Whether round 4 begins immediately is the human's call. If yes, the next conversational turn after the round-3 retro lands creates `notes/roadmap/roadmap-4.md`. If no, the project rests at v0.2.0 with the multi-vault control plane shipped.

The round-3 end-of-round retro answers (per [`roadmap-3.md`](../roadmap-3.md) § After round 3): "did the roadmap → workplan → build cadence still work at round-3 risk shape? What surprised us about per-vault concurrency and cross-vault semantics that the docs did not predict?"

The cadence has held for 10 consecutive clean steps (rounds 1+2 = 8; round 3 steps 9+10 = 2 more); step 11 is the third round-3 data point. The end-of-round retro consolidates round-3's structural observations: per-vault refactor surface produced wider-than-anticipated ripple effects (workplan-prose-vs-load-bearing-decision drift), API-error stall as a new case-4 failure mode + tail-peek diagnostic, soft-flag self-correction at boundary, single coordinator process with two phases (workplan + build), spec-fleshout-at-workplan-write paying off across step 10.
