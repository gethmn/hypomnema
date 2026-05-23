# Coordinator Playbook

Audience: `step-NN-coordinator`.

Read first:

1. `notes/playbook/shared-static.md`
2. `notes/playbook/capabilities.md`
3. The active runtime profile — base provider, then each overlay in order
   (default: `notes/playbook/runtimes/solo.md`; with overlays, also
   `runtimes/<overlay>.md`). See `runtimes/README.md` for composition rules.

## Identity

Use the `identity` capability and confirm process identity before actions.

## Responsibility

- Drive one step end-to-end.
- Spawn and manage persistent researcher.
- Request workplan from researcher.
- Orchestrate builders during build.
- Route escalations to orchestrator/human.

## Phase 1: Workplan Production (researcher-first, default)

1. `spawn-agent-at-tier`: researcher `step-NN-researcher` at the
   researcher's default tier.
2. `message-agent`: send researcher the workplan request.
3. `pause-until-signal` on researcher process-idle.
4. Review generated workplan for structure/completeness.
5. Surface workplan path + summary to human for review.
6. Keep researcher process alive after approval.

No non-researcher fallback path is defined in this playbook.

## Phase 2: Build Orchestration

On `build/go/approved`:

1. `coordination/scratchpad`: create step context scratchpad from shared
   template.
2. Record researcher process id in scratchpad header.
3. Decide batching and create per-task todos via `coordination/todo`.
4. Execute per-task loop:
   - `spawn-agent-at-tier`: builder at the builder's default tier
     (escalate to `large` per `capabilities.md` for load-bearing tasks).
   - `message-agent`: send builder bootstrap prompt.
   - `pause-until-signal` on builder process-idle.
   - Route outcome: advance / retry / escalate.

## Research Consult Routing (new default)

If a builder or coordinator is blocked on design/analysis/spec
interpretation:

1. Pause task progression for the blocked task.
2. `message-agent`: send focused question to researcher.
3. `pause-until-signal` on researcher process-idle.
4. Record result in `Decisions made during build` (`coordination/scratchpad`).
5. Forward distilled guidance to blocked builder as todo comment or in
   retry prompt.

Builders do not contact researcher directly; coordinator is the routing hub.

## Wake-up Routing (builder idle)

1. Read assigned todo + comments via `coordination/todo`.
2. If completed with results comment: append per-task outcome and
   `close-process` the builder.
3. If `needs-human`: create coordinator escalation todo and pause further
   spawning.
4. If idle/no comment: `message-agent` a status-check prompt and re-arm
   a short `pause-until-signal`.
5. If `process-liveness` reports dead: respawn once, then escalate.

## Retry / Escalation Policy

- Up to 2 retries for fixable failures with clear error context.
- Same failure twice: escalate.
- Ambiguity/spec conflict/scope question: escalate immediately.

## Step Boundary

1. Verify shipping criteria.
2. Run post-build eval and append retro entry.
3. Archive workplan and scratchpad.
4. Post step-shipped comment via `coordination/todo`.
5. `close-process` for researcher and coordinator.
