# Shared Playbook Instructions

These instructions are shared across all roles: `Orchestrator`, `Coordinator`,
`Researcher`, and `Builder`. The role model defined here is runtime-agnostic;
it relies on a capability contract that any orchestration layer can satisfy.

## Runtime Binding

Operations are referenced by **capability verb**, never by a concrete tool
name. The verbs and their semantics live in
[`notes/playbook/capabilities.md`](./capabilities.md).

Concrete tool mappings live in **runtime providers** under
`notes/playbook/runtimes/`. A session uses a **profile**: one complete base
provider plus zero or more partial overlays. A capability resolves through
the overlays left-to-right; the first overlay that claims it wins; otherwise
the base provides it. See [`runtimes/README.md`](./runtimes/README.md) for
the composition rules and the conflict / dependency rules.

The **default profile** is:

```
base: solo
overlays: []
```

A session may override the default in its bootstrap prompt. For example, to
route `spawn-agent-at-tier` through Duo while keeping Solo for everything
else:

```
base: solo
overlays: [duo]
```

Every role reads, in order:

1. This file.
2. `capabilities.md`.
3. The active profile's base, then each overlay in order
   (default: `runtimes/solo.md`; with overlays, also `runtimes/<overlay>.md`).
4. The role-specific playbook (`orchestrator.md`, `coordinator.md`,
   `researcher.md`, `builder.md`).

Confirm own identity (capability `identity`) early. Keep process names
explicit and role-scoped. Use `coordination/todo` as the primary durable
coordination surface; use `coordination/scratchpad` for rolling context, not
as a replacement for todo state. Use `pause-until-signal` instead of polling
loops.

## Tiered Spawning

Callers request capability tier (`small`, `medium`, `large`) via the
`spawn-agent-at-tier` capability — never a runtime-specific tool id. The
tier/role policy and load-bearing escalation criteria are defined in
`capabilities.md`. The active runtime binding decides which concrete tools
satisfy each tier.

## Role Invariants

- `Orchestrator` and `Coordinator` are never the same process.
- `Coordinator` and `Researcher` are separate processes.
- `Builder` processes are ephemeral and scoped to task execution.
- `Researcher` is long-lived for the lifetime of a step and remains available
  for consultation during build.

## Naming Conventions

| Thing | Pattern | Example |
|---|---|---|
| Coordinator process | `step-NN-coordinator` | `step-01-coordinator` |
| Researcher process | `step-NN-researcher` | `step-01-researcher` |
| Builder process | `step-NN-builder-MM` (or `-MM-r1`) | `step-01-builder-03` |
| Step context scratchpad | `step-NN-context` | `step-01-context` |
| Step todo | `Step N · Task M — <one-line>` | `Step 1 · Task 3 — Logging init` |
| Escalation todo | `[ESCALATION step-NN/builder-MM] <summary>` | `[ESCALATION step-01/builder-03] EnvFilter strategy unclear` |

## Tags

- `roadmap`
- `step-NN`
- `task`
- `needs-human`
- `escalation`
- `coordinator-context`

## Scratchpad Template

```markdown
# Step N — Rolling Context

**Coordinator**: <process name and id>
**Researcher**: <process name and id>
**Workplan**: notes/roadmap/step-NN-workplan.md
**Build started**: <ISO timestamp>

## Batching plan

| Batch | Tasks | Rationale |
|---|---|---|
| (filled by coordinator at setup) | | |

> **Live task status**: query the `coordination/todo` capability
> (tag `step-NN`) rather than maintaining a status table here.

## Decisions made during build

(append during build)

## Escalations

(append as escalations occur)

## Per-task outcomes

(append one outcome paragraph per task completion)
```
