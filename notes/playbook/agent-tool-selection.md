# Agent Tool Selection And Spawn

This document defines how tier-based agent spawning works in Solo MCP.

## Goal

Callers should request capability (`small`, `medium`, `large`) rather than hard-coding `agent_tool_id` values.

Primary interface:

- `/spawn-agent <tier>`

Optional expanded form:

- `/spawn-agent <tier> --name <process-name> --purpose "<short reason>"`

## Inputs

Required:

- `tier`: one of `small`, `medium`, `large`

Optional:

- `name`: process name to use at spawn time
- `purpose`: one-line reason for auditability
- `strategy`: `deterministic` (default) or `random`
- `exclude_ids`: list of `agent_tool_id` values to avoid

## Resolution Order

Resolve candidate tools in this order:

1. `notes/agent-runtimes.md` explicit mapping (if present)
2. Runtime name suffix inference
3. Fail with a clear error if no candidates exist

### Suffix Inference

- `small`: `-haiku`, `-flash`, `-mini`, `-cheap`, `-small`
- `medium`: `-sonnet`, `-medium`, `-default`
- `large`: `-opus`, `-pro`, `-max`, `-large`

Matching is case-insensitive and suffix-based.

## Selection Strategy

Default: `deterministic`

- Sort matching candidates by `agent_tool_id` ascending.
- Select first candidate.

Optional: `random`

- Choose uniformly from matching candidates.

Deterministic selection is the default for reproducibility. Use `random` only when intentional spread is desired.

## Spawn Behavior

- Spawn an **agent process** with `kind="agent"` and the selected `agent_tool_id`.
- Return the created process metadata.
- Do not send a bootstrap message unless explicitly requested by the caller.

## Return Shape

Return:

- `process_id`
- `agent_tool_id`
- `tool_name`
- `tier`
- `selection_reason`

Optional:

- `alternatives_considered`

## Failure Behavior

Hard-fail with an actionable message when:

- No registered tools map to requested tier.
- Spawn fails after selecting a candidate.

Failure message must include:

- requested tier
- discovered tools
- mapping source used (`notes/agent-runtimes.md` vs suffix)

## Role Policy (Balanced Default)

- `Orchestrator`: `small`
- `Coordinator`: `small` default, `medium` when handling active escalations/retries
- `Researcher`: `large` default, `medium` allowed for low-risk narrow-decision steps
- `Builder`: `medium` default, `large` for medium-high/high-risk and load-bearing/novel tasks

Additional guardrail:

- If a `Builder` hits repeated same-shape failures on `medium`, next attempt should use `large` or escalate.
