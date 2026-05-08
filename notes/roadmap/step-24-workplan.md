# Step 24 Workplan — Async Bootstrap Indexing

**Status**: Draft  
**Date**: 2026-05-07  
**Round**: 13  
**Intake**: `notes/proposals/async-bootstrap-indexing-intake.md`

---

## Workplan-Time Decision Resolutions

Three decisions flagged by the intake as non-blocking but required before build begins:

1. **`wait_for_bootstrap` visibility**: test-only (not a `pub` method)
   - Rationale: v0 has no blocking wait requirement from CLI; test-only keeps the public API minimal. `hmn vault status --wait` deferred.

2. **Progress counter mechanism**: `Arc<AtomicU64>` injected into scanner
   - Rationale: Minimal interface change to `Scanner::run`; no new abstraction; scanner updates counter at low frequency.

3. **`wait_for_bootstrap` implementation**: `watch::Receiver` channel
   - Rationale: Cleaner than polling; single-subscribe pattern fits the test use case; aligns with per-vault joiner pattern already in use.

---

## Task Structure

Three tasks with dependency chain: **Task 1 → Tasks 2 & 3 (parallel)**

### Task 1: Control-Plane Refactor — Async Bootstrap State

**Goal**: Move initial vault scan to background task; bind listener immediately.

**Shipping Criteria**:
- [ ] `VaultEntry` has `bootstrap_state: Arc<RwLock<BootstrapState>>` enum
- [ ] `BootstrapState = Indexing { started_at, files_seen, files_indexed } | Ready | Errored(msg)`
- [ ] `spawn_runner_parts` split into:
  - Sync phase: open store, create entry, insert into manager
  - Background phase: spawn scan + watcher, update state on completion
- [ ] `manager.rs:create_vault` has symmetric async-bootstrap behavior
- [ ] Per-vault joiner pattern reused for shutdown integration
- [ ] Scanner receives `Arc<AtomicU64>` progress counter; updates `files_indexed` at reasonable frequency
- [ ] Store is open and queryable before scan starts (partial results OK)

**Files**:
- `src/control_plane/manager.rs` — load-bearing change
- `src/bin/hmnd.rs` — verify ordering still correct (likely no change)
- `src/control_plane/events.rs` — optional `VaultBootstrapped` event (deferred)

**New Dependencies**: none

**Risk**: medium (structure change is contained; the joiner pattern is reused; main execution risk is async task lifecycle)

---

### Task 2: API Extension — Status Response Bootstrap Block

**Goal**: Expose per-vault bootstrap state via `/status` so clients observe indexing progress.

**Shipping Criteria**:
- [ ] `StatusResponse` has per-vault `bootstrap: { state, started_at, files_seen, files_indexed, message }`
- [ ] Top-level legacy fields (`indexed_file_count`, `last_indexed_at`) continue to populate for back-compat
- [ ] `/status` handler reads `bootstrap_state` from manager at query time (pull-only, no push events)
- [ ] Wire shape is additive; v0 clients ignoring unknown fields continue to work
- [ ] Search queries during bootstrap return partial results without panic or 5xx (no code change required for correctness)
- [ ] *Optional*: widen semantic-search `hint` to fire on `Indexing` state (deferred unless one-liner)
- [ ] *Optional*: expose `bootstrap` block on `/health` (deferred unless trivial pattern exists)

**Files**:
- `src/api/types.rs` — `StatusResponse` extension
- `src/api/status.rs` — populate bootstrap block

**New Dependencies**: none

**Risk**: low (additive wire change; no schema change)

---

### Task 3: Test Migration — `wait_for_bootstrap` Helper

**Goal**: Allow existing tests to remain simple while adapting to async bootstrap.

**Shipping Criteria**:
- [ ] `pub(crate) wait_for_bootstrap(manager: &VaultManager, vault_name: &str) -> impl Future`
- [ ] Implementation: `watch::Receiver` channel flipped to `Ready` when bootstrap completes
- [ ] Helper returns immediately if vault already bootstrapped
- [ ] Every test currently assuming "indexed when `open()` returns" gets a one-line `.wait_for_bootstrap().await` addition
- [ ] `cargo test` and `cargo clippy -- -D warnings` clean
- [ ] Verify shipping criteria from Step 24 roadmap:
  - HTTP listener responds within 1s on ~900-file vault
  - `/status` reflects bootstrap state, transitions correctly
  - Search during bootstrap returns partial results
  - SIGTERM during indexing exits cleanly
  - `hmn vault create` returns promptly

**Files**:
- `src/control_plane/manager.rs` — add `wait_for_bootstrap` helper
- `tests/` — update integration tests
- `src/api/tests.rs` — update status API tests
- `src/control_plane/tests.rs` — update manager tests
- `tests/cli.rs` — update CLI tests if needed

**New Dependencies**: none

**Risk**: low (test infrastructure; mechanical changes)

---

## Batching Plan

| Batch | Tasks | Sequencing | Rationale |
|---|---|---|---|
| 1 | Task 1 | Start immediately | Unblocks Tasks 2 & 3; required for both |
| 2 | Tasks 2, 3 | Parallel after Task 1 | Independent; Task 2 reads bootstrap_state, Task 3 tests it; both ready after Task 1 |

---

## Shipping Criteria Verification

At step boundary, verify all criteria from `notes/roadmap/roadmap-13.md` § Step 24:

- [ ] HTTP listener responds within 1 s of process start on ~900-file vault
- [ ] `/status` per-vault `bootstrap` block (state, started_at, files_seen, files_indexed, message)
- [ ] Bootstrap state transitions (`indexing` → `ready` | `errored`)
- [ ] Top-level legacy fields back-compat
- [ ] Search queries during bootstrap return partial results, no 5xx
- [ ] `hmn vault create` returns promptly, vault enters background indexing
- [ ] SIGTERM mid-scan exits cleanly within shutdown timeout
- [ ] `VaultStatus` enum and schema unchanged
- [ ] `wait_for_bootstrap` helper in place
- [ ] `cargo test` and `cargo clippy -- -D warnings` clean

---

## Related References

- **Intake**: `notes/proposals/async-bootstrap-indexing-intake.md`
- **Roadmap**: `notes/roadmap/roadmap-13.md`
- **Proposal**: `notes/proposals/async-bootstrap-indexing.md`
- **Load-bearing site**: `src/control_plane/manager.rs:1259-1358`
- **Tests**: `tests/`, `src/control_plane/tests.rs`, `src/api/tests.rs`
