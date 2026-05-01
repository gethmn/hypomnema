# Step 17 Workplan -- Round 7 Dependency Upgrade Round

**Status**: Draft for review

**Roadmap Source**: Solo scratchpad 20 (`Round 7 Candidate: Dependency Upgrade Round`).

**Goal**: land the three open major dependency upgrades (`notify`, `notify-debouncer-full`, `axum`) in a focused, low-noise round with explicit verification of watcher correctness and HTTP/MCP surface stability.

## Workplan Decisions

### A. Scope Boundaries

Step 17 is a dependency-upgrade round only.

- In scope: `notify` 6 -> 8, `notify-debouncer-full` 0.3 -> 0.7, `axum` 0.7 -> 0.8, and required compatibility edits/tests/docs.
- Out of scope: release automation, CodeQL/Scorecard, Windows matrix expansion, additional opportunistic dependency churn.
- Out of scope: any v0-incompatible feature work (vault writes, durable/replay event history, ownership model additions, abstraction refactors).

### B. Sequencing

Execution order is fixed:

1. `notify` + `notify-debouncer-full` upgrade (narrower surface, watcher correctness first).
2. `axum` upgrade (broader HTTP/MCP surface after watcher is stable).
3. Final gate verification and documentation sweep.

This preserves bisectability and avoids cross-upgrade compiler noise.

### C. Constraint Pins

- Keep Hypomnema load-bearing watcher invariants:
  - continue using `notify` + `notify-debouncer-full` (no custom debouncer);
  - preserve content-hash gating behavior (no mtime-only behavior drift);
  - preserve cancellation/shutdown behavior.
- Keep async/SQLite invariant: no direct rusqlite calls on async runtime threads; existing `spawn_blocking` boundaries remain intact.
- Keep v0 scope guardrails from `AGENTS.md`.

### D. Toolchain Policy

- `notify`/`notify-debouncer-full` updates require compatibility with their current MSRV floor.
- `axum 0.8` requires Rust 1.80; verify `rust-toolchain.toml` and update explicitly if needed.
- Any toolchain bump is part of this step and must be explicit in diffs and task outcome notes.

## Relevant Inputs

- Scratchpad: Solo scratchpad 20, "Round 7 Candidate: Dependency Upgrade Round".
- Process: [`notes/project-planning-workflow-notes.md`](../project-planning-workflow-notes.md).
- Stack constraints: [`docs/implementation/tech-stack.md`](../../docs/implementation/tech-stack.md).
- Pitfalls: [`docs/implementation/appendices/tech-stack/pitfalls.md`](../../docs/implementation/appendices/tech-stack/pitfalls.md).
- Agent guardrails: [`AGENTS.md`](../../AGENTS.md).
- Skill required for watcher step: [`.claude/skills/filesystem-watching/SKILL.md`](../../.claude/skills/filesystem-watching/SKILL.md).

## Task Plan

### Task 17.1 -- Upgrade `notify` + `notify-debouncer-full`

**Purpose**: adopt `notify = "8"` and `notify-debouncer-full = "0.7"` together while preserving watcher semantics and test reliability.

**Work**:

- Update dependency versions in `Cargo.toml` (and lockfile via cargo).
- Compile-fix watcher/debouncer API changes:
  - `notify-types` rename impacts;
  - `FileIdCache` ownership trait updates;
  - any constructor/signature changes in debouncer wiring.
- Audit watcher behavior against expected event semantics under new dedup behavior (especially modify-after-create suppression behavior) and adjust tests only when behavior change is real and documented.
- Keep the existing debounce architecture; no custom debouncing.

**Files likely touched**:

- `Cargo.toml`
- `Cargo.lock`
- `src/watcher/mod.rs`
- watcher-related support modules/tests as needed (`tests/watch.rs`, any watcher unit tests)

**Tests**:

- `cargo check`
- focused watcher tests (`cargo test` target(s) touching watch/watcher behavior)
- full `cargo test`
- `cargo clippy -- -D warnings`

**Risk**: medium. Core watcher path is load-bearing; dedup behavior change can alter event-count assumptions.

### Task 17.2 -- Upgrade `axum` to 0.8

**Purpose**: adopt `axum = "0.8"` and reconcile HTTP/MCP-facing server code with axum 0.8 API changes.

**Work**:

- Update `axum` dependency and apply required migration edits across route handlers, response types, and server wiring.
- Verify compatibility of related dev/runtime dependencies (including `tower` in dev-deps if required by compile/test feedback).
- Verify/adjust `rust-toolchain.toml` for Rust 1.80 if current pin is lower.
- Keep runtime behavior and response contracts stable unless a change is required by axum API semantics.

**Files likely touched**:

- `Cargo.toml`
- `Cargo.lock`
- `rust-toolchain.toml` (if needed)
- `src/api/` modules
- `src/bin/hmnd.rs`
- HTTP/MCP integration test files as required

**Tests**:

- `cargo check`
- HTTP/MCP-focused tests
- full `cargo test`
- `cargo clippy -- -D warnings`

**Risk**: medium-high. Broad HTTP surface + MCP transport touchpoints.

### Task 17.3 -- Round 7 Verification Gate

**Purpose**: verify both upgrade slices are production-clean and documented.

**Work**:

- Run full quality gates:
  - `cargo fmt`
  - `cargo test`
  - `cargo clippy -- -D warnings`
  - `git diff --check`
- Run manual smoke checks:
  - watcher smoke: create/modify/delete in a watched vault and confirm live event flow remains intact;
  - HTTP smoke: health + filesystem/content search endpoints;
  - MCP Streamable HTTP smoke: confirm endpoint remains reachable/functional after axum upgrade.
- Update active planning docs only if scope/reality drifted (workplan or roadmap note).

**Risk**: medium. Integration-only failures may surface late.

## Files Likely Touched

- `Cargo.toml`
- `Cargo.lock`
- `rust-toolchain.toml` (possible)
- `src/watcher/` (possible)
- `src/api/` (possible)
- `src/bin/hmnd.rs` (possible)
- `tests/watch.rs` and other HTTP/MCP integration tests (possible)
- `notes/roadmap/step-17-workplan.md`

## Test Strategy

- Preserve existing behavioral expectations first; update assertions only for genuine upstream behavior changes.
- Use focused test runs after each upgrade slice, then full-suite gates at the end.
- For watcher behavior shifts due to upstream dedup fixes, require explicit note in task outcome summary/commit rationale.

## Non-Goals

- No new architecture layers or abstraction work.
- No new durable event storage/replay semantics.
- No scope expansion beyond the three candidate dependency upgrades.
- No silent dependency-churn bundle outside the planned upgrades.

## Definition Of Done

- [ ] `notify` pinned to major 8 and `notify-debouncer-full` pinned to major 0.7.
- [ ] Watcher path compiles and behavior remains correct under existing/updated tests.
- [ ] `axum` pinned to major 0.8 with HTTP/MCP surfaces compiling and passing tests.
- [ ] `rust-toolchain.toml` is compatible with dependency MSRV requirements (updated if needed).
- [ ] `cargo fmt`, `cargo test`, `cargo clippy -- -D warnings`, and `git diff --check` are clean.
- [ ] Manual watcher + HTTP + MCP smoke checks pass.
- [ ] No out-of-scope dependency or architecture work was introduced.

## Build-Phase Notes For Coordinator

- Wait for explicit human approval (`build` / `go` / `approved`) before starting build phase.
- At build start, read `notes/coordinator-playbook.md` **COORDINATOR** section only; do not read ORCHESTRATOR section.
- Execute COORDINATOR setup and per-task loop exactly.
- Create one Solo todo per workplan task (17.1, 17.2, 17.3).
- Spawn task agents named `step-17-task-MM` to execute tasks.
- Coordinator does orchestration/review/gating only; do not implement task code directly in coordinator phase.
