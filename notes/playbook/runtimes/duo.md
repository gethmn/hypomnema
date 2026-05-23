# Runtime Provider: Duo (partial overlay)

**Coverage:** `spawn-agent-at-tier`

**Requires base:** `solo`

Duo is a policy/orchestration layer that sits on top of Solo, not a Solo
replacement. From the Duo PRD
(`notes/proposals/archive/solo-orchestrator-companion-prd.md`):

> Solo remains the authority for process creation. The companion is a
> policy/orchestration layer, not a Solo replacement.

When Duo overlays Solo, Duo provides `spawn-agent-at-tier`. All other eight
capabilities resolve through the Solo base unchanged.

## Capability mapping

| Capability | Duo tool(s) |
|---|---|
| `spawn-agent-at-tier` | `mcp__duo__spawn_agent(tier, name?, purpose?)` |

Companion operations under the same capability:

- `mcp__duo__list_agent_tiers()` — discover available tier labels and their
  availability for the current project.
- `mcp__duo__resolve_agent_tool(tier)` — preview which tool would be spawned
  for a tier without creating a process. Use for diagnostics.

## What this overlay supersedes

When `duo` is active as an overlay, the following section of
[`solo.md`](./solo.md) is **superseded** and should not be applied:

- `solo.md` § Tier Resolver (Source of truth, Resolution order, Command-first
  classification, Name fallback, Spawn behavior, Return shape, Failure
  behavior). Duo owns tier resolution; Solo is consulted only by Duo
  internally for the concrete spawn.

The rest of `solo.md` — `identity`, `message-agent`, `coordination/*`,
`pause-until-signal`, `process-liveness`, `close-process`, the `mcp-cli`
anti-pattern, and Solo control-plane terminology — continues to apply.

## Why a base dependency

Duo's `spawn_agent` delegates the underlying process creation to Solo's
`spawn_process(kind="agent", agent_tool_id=...)`. A profile that pairs Duo
with a non-Solo base would have nothing for Duo to delegate to. Hence
`Requires base: solo`.

## Call shape

- Inputs:
  - `tier`: `small | medium | large` (per `capabilities.md` § Tier / Role
    Policy).
  - `name` (optional): role-scoped process name (e.g. `step-NN-coordinator`).
  - `purpose` (optional): one-line reason, recorded for auditability.
- Output: process metadata including resolved `tool_name`, `tier`, and a
  `selection_reason` explaining the classification.
- Failure: hard-fail when no enabled tool maps confidently to the requested
  tier. Do not silently substitute another tier. Diagnostics are returned in
  the failure payload.

## Control-plane abstraction

Callers continue to request capability tier rather than a runtime-specific
tool id. The control-plane form remains `/spawn-agent <tier>` — under the
Duo overlay it routes through `mcp__duo__spawn_agent` instead of the Solo
resolver.

## Project / process identity

Duo accepts explicit `project_id` and process identity arguments. As a
convenience, it may default to the caller's `SOLO_PROJECT_ID` and
`SOLO_PROCESS_ID` environment variables. Explicit arguments win when present.
