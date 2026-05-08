# Round 13 — Async Bootstrap Indexing

**Status**: Planning phase  
**Date**: 2026-05-07  
**Steps**: 24  
**Scope**: Single-step round making `hmnd`'s HTTP listener bind immediately on startup and exposing per-vault bootstrap state via `/status`.

**Intakes**:
- [`notes/proposals/async-bootstrap-indexing-intake.md`](../proposals/async-bootstrap-indexing-intake.md) — Complete

---

## Overview

Round 13 fixes the daemon's startup responsiveness defect described in `notes/proposals/async-bootstrap-indexing.md`. Currently `hmnd` blocks its HTTP listener for the full duration of the initial vault scan; on a ~900-file vault, clients receive connection-refused for the entire startup period with no observable state. This round moves the scan to a background task, binds the listener immediately, and exposes an in-memory `Indexing | Ready | Errored` bootstrap state per vault through the existing `/status` surface.

No new dependencies. No data-plane change. No schema change. Additive wire extension only.

Human-resolved decisions (from intake):
- **Single-step round**: the control-plane change (`spawn_runner_parts` split) and the API extension (`StatusResponse` bootstrap block) are not independently verifiable; neither ships a useful, testable increment on its own.
- **Option A** (background bootstrap, pull-only status) is the selected design.
- Bootstrap state is **in-memory only** — not persisted to `vaults.sqlite`.
- `VaultStatus` enum and on-disk schema are **not changed**.
- Push-based progress events (Option B), concurrent watcher-during-bootstrap (Option C), and durable bootstrap state across restarts (Option D) are all deferred.

---

## Step 24 — Async Bootstrap Indexing

### Objective

Move the initial vault scan to a background task so the HTTP listener binds immediately on process start, and expose per-vault bootstrap state via `/status` so clients can observe indexing progress and know when search results are complete.

### Shipping Criteria

All items below must be complete and passing before the step is marked shipped:

- [ ] HTTP listener responds within 1 s of process start on a ~900-file vault (connection-refused regression gone)
- [ ] `/status` returns a per-vault `bootstrap` block: `state` (`"indexing"` | `"ready"` | `"errored"`), `started_at`, `files_seen`, `files_indexed`, `message`
- [ ] `bootstrap.state` transitions to `"ready"` on scan completion; transitions to `"errored"` with message on scan failure
- [ ] Top-level `/status` legacy fields (`indexed_file_count`, `last_indexed_at`) continue to populate (back-compat)
- [ ] Content and semantic search queries during bootstrap return partial results without panic or 5xx
- [ ] `hmn vault create` returns promptly; vault enters background indexing symmetrically to startup case
- [ ] SIGTERM during indexing exits cleanly within existing shutdown timeout; per-vault joiner pattern reused
- [ ] `VaultStatus` enum and on-disk schema are **not** changed (bootstrap state is in-memory only)
- [ ] `wait_for_bootstrap` helper added so existing tests continue to work with one-line additions
- [ ] `cargo test` and `cargo clippy -- -D warnings` clean

---

## Out of Scope

- Push-based progress events on the SSE bus (Option B)
- Concurrent watcher-during-bootstrap — start watcher before scan completes (Option C)
- Durable bootstrap state across restarts (Option D)
- `/health` bootstrap exposure (degraded-while-indexing signaling for load balancers)
- Persisting `bootstrap_state` to `vaults.sqlite`
- Optional `VaultBootstrapped` event on the event bus
- Widening semantic-search `hint` to fire on `Indexing` state (current zero-chunks hint is sufficient for correctness)
- Any change to `VaultStatus` enum or on-disk schema
- `hmn vault status --wait` blocking CLI flag

---

## Workplan-Time Decisions (resolved in step-24-workplan.md)

1. **`wait_for_bootstrap` visibility** — test-only or `pub`?
2. **Progress counter mechanism** — how does `files_indexed` increment during the scan?
3. **`wait_for_bootstrap` implementation strategy** — poll `Arc<RwLock<BootstrapState>>` or `watch::Receiver`?

All three are resolved in `notes/roadmap/step-24-workplan.md` § Workplan-Time Decision Resolutions.

---

## Related References

- **Proposal**: `notes/proposals/async-bootstrap-indexing.md`
- **Intake**: `notes/proposals/async-bootstrap-indexing-intake.md`
- **Load-bearing change site**: `src/control_plane/manager.rs:1259-1358` — `spawn_runner_for_row` / `spawn_runner_parts`
- **Startup sequence**: `src/bin/hmnd.rs:111-119`
- **Status API types**: `src/api/types.rs`
- **Status handler**: `src/api/status.rs`
- **Partial-results hint precedent**: `docs/specs/semantic-search.md` §263–271
- **Indexing ADR**: `docs/decisions/0003-indexing-in-the-daemon.md`

---

## Build Strategy (post-approval)

**Phase 1 — Workplan production** (coordinator-driven)
- Coordinator drafts `notes/roadmap/step-24-workplan.md` directly from intake (intake is sufficiently complete; no separate researcher phase required)
- Surface workplan to human for go/no-go

**Phase 2 — Build orchestration** (coordinator-driven, if approved)
- 3 tasks with a dependency structure: control-plane refactor first, then API extension and test migration in parallel
- Create step-24-context scratchpad, spawn builders, verify shipping criteria at step boundary
