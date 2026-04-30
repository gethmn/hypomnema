# Hypomnema Roadmap -- Round 6: Outbox Retirement

**Scope**: retire the durable outbox surface that has been carrying the `tests/outbox.rs` flake history, and simplify the surrounding change-event plumbing as far as the round can do cleanly. Step 14 from round 5 was deferred because the likely next move is removing the outbox entirely; this round makes that the primary focus instead of treating it as a flake-hunting exercise.

The exact replacement shape for any consumer-visible event delivery is intentionally left workplan-time. If the round discovers that a small residual event surface still needs to exist, the workplan will pin that behavior; otherwise, the goal is to delete the durable outbox path and retire the stale tests and docs around it.

**Status**: Not started. Round 5 shipped `v0.4.0` on 2026-04-29. Workplans are created just before each step is implemented, per the round-1 through round-5 cadence.

**Process**: Same as rounds 1--5. This round is a single-step round: one workplan (`step-16-workplan.md`) written immediately before implementation. Deferred decisions are pulled forward to workplan-time. The orchestration shape (orchestrator + per-step coordinator + ephemeral task agents, see [`notes/coordinator-playbook.md`](../coordinator-playbook.md)) carries forward unchanged.

**Round-5 lessons feeding into this round** (see [`notes/project-planning-workflow-notes.md`](../project-planning-workflow-notes.md) § End-of-round retrospective for full text):

- **CHANGELOG ritual is now load-bearing** -- the round-boundary checklist explicitly updates repo-root `CHANGELOG.md`. Keep that boundary habit, but do not expand it into release automation.
- **Round-5's outbox deferral was deliberate** -- the flake was not ignored; it was deferred because outbox removal looked like the better next move. Round 6 should treat that as the primary hypothesis.
- **Round-level scope stays useful** -- the round-5 maintenance round stayed small and coherent. Round 6 should keep the same discipline: one outbox-retirement shipping gate, not a grab bag of unrelated follow-ons.
- **Manual smoke still matters on wiring-shaped deletions** -- if the round keeps any consumer-visible event behavior, verify it with a real-file-change smoke pass rather than trusting unit tests alone.
- **Skills carrying forward**: `filesystem-watching` is the most likely to matter here, because this round is likely to touch watcher/outbox event paths. `rusqlite-in-async`, `markdown-chunking`, and `sqlite-vec-extension` are likely irrelevant unless a hidden dependency surfaces.

**Specs likely to be amended or retired this round**:

- **`docs/specs/change-events.md`** -- likely the main spec touchpoint if the durable outbox path is removed or narrowed.
- **`docs/architecture/overview.md`** -- update the high-level dataflow if outbox persistence disappears or changes shape.
- **`docs/reference/configuration.md`** -- only if a setting related to outbox behavior or event delivery changes.
- **`notes/project-planning-workflow-notes.md`** -- append the round-6 retrospective when the round ships.
- **No new spec is anticipated**. This looks like a deletion/simplification round, not a feature-addition round.

**Implementation surface across the round**:

- Likely `src/outbox/` removal or substantial simplification.
- Possible follow-up changes in `src/watcher/`, `src/indexer/`, and any HTTP / CLI / MCP surface that currently exposes outbox tailing or event assumptions.
- `tests/outbox.rs` is the first likely retirement target; nearby integration tests may need to be rewritten to assert the new, simpler behavior.
- Docs and runbooks that still assume durable outbox tailing will need to be reconciled with shipped reality.
- No new top-level crate additions anticipated.

---

## Phasing

Two steps keeps the deletion work bisectable:

| Step | Contents | Risk |
|------|----------|------|
| 16 | Outbox retirement shipping gate: remove or simplify the durable outbox path, retire or rewrite stale tests, reconcile docs/runbooks/spec references, and prove the new event-delivery shape with smoke | High |

Step 16 is the uncertain step and the round shipping gate. If the round discovers that change-event delivery needs to remain durable in some narrower form, that decision belongs in the step-16 workplan. The round should still stay focused on the outbox retirement hypothesis rather than broadening into unrelated backlog items.

---

## Step 16 -- Outbox retirement (round shipping gate)

**Status**: Shipped 2026-04-30.

**Goal**: remove the durable outbox path or simplify it to the smallest remaining shape that still fits the round's chosen event-delivery contract, then finish the cleanup required to make that shape shippable. The workplan will decide whether the consumer-facing surface becomes ephemeral, narrower, or disappears entirely. The key outcome is that the old durable outbox tailing model is no longer the daemon's center of gravity.

**Shipping criteria**:

- The chosen outbox replacement shape is pinned in the workplan and implemented consistently in the core modules.
- The old durable outbox writer / tailing path is gone or clearly reduced to a trivial compatibility shim.
- The flake-prone tests that depended on the old shape are retired or rewritten so they no longer encode the old durable contract.
- Docs and runbooks no longer describe a stale durable outbox tailing model.
- Search and file-watching behavior still pass the existing non-outbox tests.

**Deferred decisions to resolve at workplan-time**:

- **Replacement event model** -- ephemeral only, narrower durable form, or complete removal. This is the load-bearing question.
- **Consumer-facing compatibility** -- whether any CLI / HTTP / MCP surface must preserve an event-tailing experience or whether it can be retired outright.
- **Spec wording** -- whether `change-events.md` is amended, retired, or narrowed.

**Risk**: high. This is behavior deletion, not an additive refactor. The implementation may force coordinated changes across watcher, indexer, tests, and docs, so the shipping gate needs to absorb both core deletion and cleanup.

---

## Notes on the round-6 shipping gate

The round-6 shipping gate is:

1. The durable outbox path is gone or intentionally reduced to the smallest pinned replacement shape.
2. `tests/outbox.rs` no longer encodes the old durable-outbox contract.
3. The docs and runbooks match the new behavior.
4. A real-file-change smoke proves the new event-delivery shape.
5. The broader test suite stays green.
6. Round tag: likely `v0.5.0` if this becomes the next shipping gate.

After the gate hits, round 6 archives alongside its step workplan, and round 7's roadmap is written when the human picks the next focus from the backlog.
