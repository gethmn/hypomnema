# Capability Contract

This is the agnostic seam between the playbook's role model and any concrete
orchestration runtime. Role and shared docs reference operations by the
capability verbs defined here. A **runtime binding** under
`notes/playbook/runtimes/<runtime>.md` maps each capability to concrete tools
(today: `runtimes/solo.md`).

Read order for any role:

1. `notes/playbook/shared-static.md` (role invariants, naming, tags, template)
2. `notes/playbook/capabilities.md` (this file)
3. `notes/playbook/runtimes/<active-runtime>.md` (concrete tool mapping)

A new orchestration layer is supported by adding a `runtimes/<name>.md` that
implements one or more of the capabilities below. Layer A (role + shared
docs) must not change.

Runtime providers compose: a session uses a profile of one complete base
provider plus zero or more partial overlays. A capability resolves through
the overlays first; the base provides anything no overlay claims. See
[`runtimes/README.md`](./runtimes/README.md) for the composition rules.

## Capabilities

Each entry names the abstract operation, its semantics, and the inputs/outputs
the role model expects. No tool names appear here.

### `identity`

Confirm own process identity at activation. Adopt the role-scoped process name
defined in `shared-static.md` § Naming Conventions if not already set.

- Inputs: none.
- Output: confirmed process identifier and assigned role-scoped name.

### `spawn-agent-at-tier`

Create a new agent process for a role at a requested capability tier.

- Inputs:
  - `tier`: one of `small`, `medium`, `large` (see Tier / Role Policy below).
  - `name`: role-scoped process name (e.g. `step-NN-coordinator`).
  - `purpose`: one-line reason, recorded for auditability.
  - optional `strategy`: `deterministic` (default) or `random` selection among
    qualified candidates.
  - optional `exclude_ids`: candidates to skip (e.g. for retry).
- Output:
  - `process_id`, `tier`, `tool_name`, `selection_reason`,
    `alternatives_considered`.
- Failure: hard-fail with an actionable message when no candidate qualifies
  for the requested tier. Do not silently substitute another tier.

Callers request capability tier rather than a runtime-specific tool id.

### `message-agent`

Deliver a bootstrap or follow-up prompt to a previously spawned agent.

- Inputs: target `process_id`, message body.
- Output: confirmation of delivery.

### `coordination/todo`

The durable coordination surface for task state, escalations, and human
hand-offs.

- Operations: create, read, comment, tag, untag, complete, query by tag.
- Required tags are defined in `shared-static.md` § Tags.
- Naming patterns (task title, escalation title) are defined in
  `shared-static.md` § Naming Conventions.

### `coordination/scratchpad`

Rolling step-context record. Created from the template in `shared-static.md`
§ Scratchpad Template. Used for batching plans, decisions made during build,
escalations, and per-task outcomes.

- Operations: create, read, append.
- Scratchpads are not a substitute for live task state — query
  `coordination/todo` for the current status of tasks.

### `coordination/kv`

Small shared key/value state for cross-process flags and pointers.

- Operations: set, get, delete.
- Use sparingly; todos and scratchpads are the primary coordination surfaces.

### `pause-until-signal`

Pause the calling agent and resume on a signal so the next action picks up
automatically without polling.

- Signal kinds:
  - `duration`: resume after a fixed interval (one-shot or repeating).
  - `process-idle`: resume when one or all watched processes finish their
    current task.
  - `port-bound`: resume when a service port becomes ready (used for service
    readiness, not worker idle).
- On resume, the signal payload appears as a fresh user turn for the paused
  agent.

### `process-liveness`

Determine whether a target agent process is alive, idle, or dead. Used by
the coordinator and orchestrator wake-up routing.

- Inputs: target `process_id`.
- Output: a state from `{alive-working, alive-idle, dead}` plus last-activity
  metadata when available.

### `close-process`

Terminate an ephemeral agent process (typically a completed or escalated
builder, or a researcher at step boundary).

- Inputs: target `process_id`.
- Output: confirmation of termination.

## Tier / Role Policy (balanced default)

The capability tiers `small`, `medium`, `large` are agnostic. A runtime
binding decides which concrete tools satisfy each tier.

| Role | Default tier | Escalation tier |
|---|---|---|
| Orchestrator | `small` | — |
| Coordinator | `small` | `medium` when handling active escalations / retries |
| Researcher | `large` | `medium` allowed for low-risk narrow-decision steps |
| Builder | `medium` | `large` for medium-high/high-risk and load-bearing/novel tasks |

Additional guardrail: if a Builder hits repeated same-shape failures on
`medium`, the next attempt should use `large` or escalate.

### Load-bearing builder escalation

Use `large` for builder tasks that touch load-bearing or high-risk areas:

- SQLite and async runtime boundaries
- file watching and debouncing
- MCP protocol behavior
- cross-cutting refactors
- schema / indexing changes
- novel or hard-to-reverse implementation paths

Using a stronger model for these tasks is cheaper than debugging subtle
orchestration or runtime failures later.

## Selection Strategy (agnostic)

When more than one concrete tool qualifies for a requested tier within a
runtime binding:

- Default `deterministic`: sort qualified candidates by a stable identifier
  ascending and pick the first.
- Optional `random`: choose uniformly from qualified candidates. Use only
  when intentional spread is desired.

The mapping from `command` / `name` tokens to tiers, the failure shape, and
the return shape are runtime-bound concerns and live in
`runtimes/<runtime>.md`.
