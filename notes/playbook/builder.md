# Builder Playbook

Audience: `step-NN-builder-MM`.

Read first:

1. `notes/playbook/shared-static.md`
2. `notes/playbook/capabilities.md`
3. The active runtime profile — base provider, then each overlay in order
   (default: `notes/playbook/runtimes/solo.md`; with overlays, also
   `runtimes/<overlay>.md`). See `runtimes/README.md` for composition rules.

## Identity

Use the `identity` capability and confirm you are the assigned builder
process.

## Responsibility

- Execute assigned task (or assigned batch).
- Report outcomes in todo comments.
- Commit task-scoped changes.
- Stop after completion or escalation.

## Startup Sequence

1. Read this file.
2. Read assigned todo(s) with comments via `coordination/todo`.
3. Read step context via `coordination/scratchpad`.
4. Read assigned workplan section.

## Reporting Contract (mandatory)

On success:

1. Run required quality gates.
2. Commit with `Step N · Task M: <summary>`.
3. Add todo results comment with files touched, tests, commit sha, decisions
   (via `coordination/todo`).
4. Mark todo complete.
5. Stop; wait for coordinator close.

## Soft Flags (optional)

Use soft flags for bounded judgment calls.

Audience values:

- `next-builder`
- `coordinator-only`
- `both`

Include:

- `Soft flag` summary
- Audience
- What you decided
- Trade-off
- Downstream impact (if applicable)

## Escalation

Escalate immediately when blocked by ambiguity/spec conflict/missing
requirement/scope explosion.

1. Add `needs-human` tag to blocked todo (`coordination/todo`).
2. Comment with blocker + options.
3. Do not mark todo complete.
4. Stop.

## Research Requests

Builders do not directly message researcher. Route research needs through
coordinator via escalation or status comment.
