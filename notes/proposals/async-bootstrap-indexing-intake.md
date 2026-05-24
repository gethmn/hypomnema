# Async Bootstrap Indexing — Intake

**Status**: draft
**Date**: 2026-05-07
**Intake inputs**:

- `notes/proposals/async-bootstrap-indexing.md` — proposal (only planning input)

---

## Summary

`hmnd` currently blocks its HTTP listener until the initial vault scan completes, making the daemon appear dead from a client's perspective for the entire scan duration. This intake covers a contained fix: bind the listener immediately, run the initial scan as a background task, and expose an in-memory `Indexing | Ready | Errored` bootstrap state per vault through the existing `/status` surface. No data-plane change, no schema change, no new dependency.

## Source Inputs

| Source | Type | Role in intake |
|---|---|---|
| `notes/proposals/async-bootstrap-indexing.md` | proposal | primary |

## Candidate Outcomes

- Outcome: Daemon HTTP listener responds immediately on startup regardless of vault size
  - Source: proposal § Motivation, § Verification Plan item 1
  - User-visible result: `curl /status` returns within 1 s of `hmnd: http server listening` on a 900-file vault
  - Verification signal: connection-refused during scan no longer reproducible

- Outcome: Clients can observe per-vault indexing progress via `/status`
  - Source: proposal § Option A — Status surface
  - User-visible result: `/status` per-vault `bootstrap` block reports `state`, `files_seen`, `files_indexed`, `started_at`
  - Verification signal: `files_indexed` rises on polling; state transitions `indexing → ready`

- Outcome: Search queries return partial results during bootstrap (no crash, no 503)
  - Source: proposal § Option A — Search behavior during bootstrap
  - User-visible result: content/semantic queries mid-scan return whatever is indexed so far
  - Verification signal: no panic, no 5xx on queries issued before `bootstrap.state == "ready"`

- Outcome: `hmn vault create` returns promptly; newly created vault enters background indexing
  - Source: proposal § Option A, § Files Likely Touched (`create_vault` ~line 532)
  - User-visible result: CLI returns immediately; vault appears in `/status` with `bootstrap.state == "indexing"`
  - Verification signal: symmetric to startup case

- Outcome: Daemon shuts down cleanly during an in-progress scan
  - Source: proposal § Error Shape, § Verification Plan item 4
  - User-visible result: SIGTERM during indexing exits within shutdown timeout; no orphaned tasks
  - Verification signal: clean shutdown log; no partial-state corruption

## Proposed Roadmap Shape

### Evaluation: single-step vs. two-step

The proposal's first open question asks whether the API change (`StatusResponse` extension) and the control-plane change (`spawn_runner_parts` split) warrant two separate steps for review hygiene. **Single step is the right call.** The control-plane change produces an in-memory `bootstrap_state` that has no observable effect until the API layer exposes it; the API change is meaningless without the underlying state. Neither ships a useful, verifiable increment on its own. The only argument for splitting — reducing per-PR diff size — is not compelling enough given the tight coupling. Proceed as one step.

---

### Step N — Async Bootstrap Indexing

**Goal**: Move the initial vault scan to a background task so the HTTP listener binds immediately and clients can observe per-vault bootstrap state via `/status`.

**Shipping criteria**:

- [ ] HTTP listener responds within 1 s of process start on a ~900-file vault (connection-refused regression gone)
- [ ] `/status` returns a per-vault `bootstrap` block: `state` (`"indexing"` | `"ready"` | `"errored"`), `started_at`, `files_seen`, `files_indexed`, `message`
- [ ] `bootstrap.state` transitions to `"ready"` on scan completion; transitions to `"errored"` with message on failure
- [ ] Top-level `/status` legacy fields (`indexed_file_count`, `last_indexed_at`) continue to populate (back-compat)
- [ ] Content and semantic search queries during bootstrap return partial results without panic or 5xx
- [ ] `hmn vault create` returns promptly; vault enters background indexing symmetrically
- [ ] SIGTERM during indexing exits cleanly within existing shutdown timeout; per-vault joiner pattern reused
- [ ] `VaultStatus` enum and on-disk schema are **not** changed (bootstrap state is in-memory only)
- [ ] `wait_for_bootstrap` helper added so existing tests continue to work with one-line additions
- [ ] `cargo test` and `cargo clippy -- -D warnings` clean

**Deferred decisions resolved in this step**:

- Decision: Whether `VaultManager::open` can return before the initial scan completes
  - Source: proposal § Why This Is Not a "Major Architecture Change"; ADR-0003 silent on ordering
  - Why this step: This is the load-bearing design decision of the round; confirmed safe because the store opens before the scan and handlers query the store directly

- Decision: Whether `spawn_runner_parts` should be split into synchronous and background phases
  - Source: proposal § Option A design, `manager.rs:1259-1358`
  - Why this step: The structural refactor is the fix; workplan must define the two-phase contract explicitly

- Decision: Whether per-vault `bootstrap` field is additive on the wire (not a breaking change)
  - Source: proposal § Option A — Status surface; legacy top-level fields kept
  - Why this step: Additive extension confirmed; workplan should note that clients ignoring unknown fields continue to work

**New deps**:

- (none)

**Risk**: low-medium

The blocking is isolated to one location (`spawn_runner_parts` inline await). The store is already open before the scan, so installing a `VaultEntry` before the scan completes is architecturally valid. The main execution risk is the test migration: many tests assume the vault is fully indexed when `open()` returns, and the `wait_for_bootstrap` helper needs a sound implementation (watch channel or poll loop). Error paths (scan panic, shutdown mid-scan) are well-specified in the proposal's error shape table.

**Source coverage**:

- `notes/proposals/async-bootstrap-indexing.md`: Motivation, Option A, Verification Plan items 1–6, Error Shape table, Files Likely Touched

## Coverage Map

| Source item | Proposed step | Status | Notes |
|---|---|---|---|
| proposal § Motivation — listener blocked during scan | Step N | planned | Core defect being fixed |
| proposal § Option A — two-phase split of `spawn_runner_parts` | Step N | planned | Load-bearing change |
| proposal § Option A — `BootstrapState` enum (Indexing/Ready/Errored) | Step N | planned | In-memory, per-process only |
| proposal § Option A — per-vault `bootstrap` block in `StatusResponse` | Step N | planned | Additive wire extension |
| proposal § Option A — back-compat legacy top-level fields | Step N | planned | Explicit requirement |
| proposal § Option A — symmetric `create_vault` async-bootstrap | Step N | planned | Prevents re-introducing the bug via CLI path |
| proposal § Option A — search partial results during bootstrap | Step N | planned | No code change required for correctness; optional hint widening deferred |
| proposal § Option A — `wait_for_bootstrap` test helper | Step N | planned | Visibility TBD (see OQ-2) |
| proposal § Option A — optional `VaultBootstrapped` event | Step N | deferred | Explicitly optional; see deferred items |
| proposal § Option A — optional hint widening on `Indexing` state | Step N | deferred | Proposal marks optional; see OQ-5 |
| proposal § Option A — `/health` bootstrap exposure | out-of-scope | out-of-scope | Proposal explicitly excludes unless trivial |
| proposal § Option B — push-based progress events | deferred | deferred | Future round if it earns it |
| proposal § Option C — concurrent watcher-during-bootstrap | deferred | deferred | "Major architecture change"; future round |
| proposal § Option D — durable bootstrap state across restarts | deferred | deferred | Not needed for responsiveness fix |
| proposal § Files Not Touched — `VaultStatus`, `Scanner`, schema | out-of-scope | out-of-scope | Explicitly excluded; no schema change |

## Deferred / Out-of-Scope Items

- Item: Option B — push-based progress events on the SSE bus
  - Source: proposal § Option B
  - Reason: Not required for the responsiveness fix; useful for a future TUI/UI
  - Revisit trigger: When a TUI or live-progress consumer is prioritized

- Item: Option C — concurrent watcher-during-bootstrap
  - Source: proposal § Option C
  - Reason: Architectural complexity (event queueing during scan); the "files modified during long scan" hole is currently theoretical
  - Revisit trigger: When a vault large enough that scan duration causes observable missed-event window is reported, or when concurrent-watcher architecture is planned

- Item: Option D — durable bootstrap state across restarts
  - Source: proposal § Option D
  - Reason: Not needed for responsiveness fix; adds schema complexity
  - Revisit trigger: When daemon restart UX (resume vs restart scan) becomes a user need

- Item: `/health` bootstrap exposure (degraded while indexing)
  - Source: proposal § Proposed Direction exclusion list
  - Reason: Out of scope unless an existing health-check pattern makes it trivial; no health-check pattern currently exists for this shape
  - Revisit trigger: If a health-check consumer (load balancer, orchestrator) needs degraded signaling

- Item: Persist `bootstrap_state` to `vaults.sqlite`
  - Source: proposal § Proposed Direction exclusion list
  - Reason: In-memory only is sufficient for the fix; persistence adds schema migration cost with no v0 benefit
  - Revisit trigger: If durable restart-resume becomes a requirement (overlaps Option D)

- Item: Optional `VaultBootstrapped` event on the event bus
  - Source: proposal § Option A (optional), § Open Questions item 4
  - Reason: Proposal marks this optional and it is not required for the fix; adding it increases scope without a clear consumer
  - Revisit trigger: If an event consumer needs bootstrap completion notification (overlaps Option B partially)

- Item: Widen semantic-search `hint` to fire on `Indexing` state
  - Source: proposal § Option A — Search behavior, § Open Questions item 5
  - Reason: Proposal marks optional; current hint fires on zero-chunks which covers the observable case; widening is a quality improvement, not a correctness fix
  - Revisit trigger: Low-cost follow-up in the same round if workplan author judges it a one-liner; otherwise next touch of semantic-search

## Open Questions

- Question: Is `wait_for_bootstrap` a `pub` method on `VaultManager` or test-only?
  - Why it matters: If `pub`, it enables `hmn vault status --wait` (blocking flag for scripting). If test-only, the public API stays minimal but the capability is unavailable to CLI callers.
  - Blocks roadmap? no
  - Suggested owner: workplan author; recommend starting test-only and promoting if `hmn vault status --wait` is scoped

- Question: Should `/health` reflect degraded state while any vault is bootstrapping?
  - Why it matters: Load balancers and orchestrators may use `/health` to gate traffic; a daemon that answers health checks but returns partial search results could confuse consumers
  - Blocks roadmap? no
  - Suggested owner: workplan author; proposal already excludes this unless trivial

- Question: Should the optional `VaultBootstrapped` event ship in this round?
  - Why it matters: Adding it now is cheaper than retrofitting later; but it has no consumer in v0 and adds test surface
  - Blocks roadmap? no
  - Suggested owner: workplan author; recommend deferring unless trivially cheap after the core change

- Question: Should the semantic-search `hint` be widened to fire on `Indexing` state?
  - Why it matters: Current hint fires only on zero-chunks; a vault mid-index with some chunks would not hint. Widening makes the hint more accurate during bootstrap.
  - Blocks roadmap? no
  - Suggested owner: workplan author; low-risk one-liner if the `bootstrap_state` is reachable from the search handler path

- Question: How is `bootstrap_state` progress updated during the scan — scanner callbacks, channel, or periodic task?
  - Why it matters: The scanner (`src/indexer/mod.rs`) currently has no progress-callback interface. Updating `files_indexed` counter requires either adding a callback/channel to `Scanner::run` or wrapping the scan call in a polling task. This is an interface design choice with implications for how much `scanner.rs` changes.
  - Blocks roadmap? no — but the workplan must choose a mechanism before implementation begins
  - Suggested owner: workplan author; recommend an `Arc<AtomicU64>` counter injected into the scanner (minimal interface change, no new abstraction)

- Question: The proposal says the proposal-recommended single-step round is the right shape — is there a review-hygiene argument to split the API extension and control-plane change into two steps?
  - Why it matters: Diff size and reviewer focus
  - Blocks roadmap? no
  - Suggested owner: orchestrator/human; **intake recommends single step** (see Proposed Roadmap Shape evaluation above); the two changes are not independently verifiable

## Recommendation

Proceed to:

- [x] Draft/update `notes/roadmap/roadmap-N.md`
- [x] Draft/update `notes/roadmap/step-NN-workplan.md`
- [ ] Refine planning inputs first

**Rationale**: The proposal is complete and well-scoped. The problem is confirmed (real-world 900-file vault, specific line references), the root cause is isolated (`spawn_runner_parts` inline await at `manager.rs:1280-1283`), the design is fully specified (two-phase split, in-memory `BootstrapState`, additive `StatusResponse`, error shape, verification plan), and no open question blocks implementation. No new dependencies are required. The only workplan authoring decision needed before coding is the `files_indexed` progress-update mechanism (OQ-5 above); this is answerable within the workplan without further planning input. Start the round.

## Human Review Notes

(append review decisions here)
