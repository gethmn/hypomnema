# Runtime Provider: Solo (complete base)

**Coverage:** `identity`, `spawn-agent-at-tier`, `message-agent`,
`coordination/todo`, `coordination/scratchpad`, `coordination/kv`,
`pause-until-signal`, `process-liveness`, `close-process` (all nine).

**Role:** complete base provider. May be used alone (default profile) or
paired with overlays (see [`./README.md`](./README.md)).

Concrete tool mapping for the capabilities defined in
`notes/playbook/capabilities.md`, plus Solo-specific operational guidance.
This is the default base provider for the playbook.

You are operating inside Solo MCP. Use Solo tools for process control,
todos, scratchpads, timers, KV, and process status. Keep process names
explicit and role-scoped.

## Capability Mapping

| Capability | Solo tool(s) |
|---|---|
| `identity` | `mcp__solo__whoami()`, `mcp__solo__rename_process(...)` |
| `spawn-agent-at-tier` | `mcp__solo__list_agent_tools()` + `mcp__solo__spawn_process(kind="agent", agent_tool_id=N)` via the resolver below (control-plane abstraction: `/spawn-agent <tier>`) |
| `message-agent` | `mcp__solo__send_input(process_id=…, input=…)` |
| `coordination/todo` | `mcp__solo__todo_create / todo_get / todo_list / todo_update / todo_complete / todo_comment_create / todo_add_tag / todo_remove_tag` |
| `coordination/scratchpad` | `mcp__solo__scratchpad_write / scratchpad_read / scratchpad_append` |
| `coordination/kv` | `mcp__solo__kv_set / kv_get / kv_delete` |
| `pause-until-signal` (duration) | `mcp__solo__timer_set` |
| `pause-until-signal` (process-idle) | `mcp__solo__timer_fire_when_idle_any / timer_fire_when_idle_all` |
| `pause-until-signal` (port-bound) | `mcp__solo__wait_for_bound_port` |
| `process-liveness` | `mcp__solo__get_process_status(process_id=…)` |
| `close-process` | `mcp__solo__close_process` / `mcp__solo__stop_process` |

Notes:

- When a Solo timer fires, Solo injects its `body` into the agent's PTY as a
  fresh user turn — no polling loop needed.
- Use `timer_fire_when_idle_*` for worker quiet periods. Use
  `wait_for_bound_port` for service readiness, not worker idle.

## Anti-pattern: Do not use `mcp-cli` from bash

❌ **Wrong**: Do not call `mcp-cli solo ...` from bash scripts or Monitor
commands.

```bash
# WRONG — do not do this
mcp-cli solo get_process_output --process-name orchestrator
mcp-cli solo spawn_process kind=agent agent_tool_id=3
```

✅ **Right**: Use the Solo MCP tool interface directly.

```
mcp__solo__get_process_output(process_name="orchestrator")
mcp__solo__spawn_process(kind="agent", agent_tool_id=3)
```

**Why**: `mcp-cli` is for CLI usage outside of MCP; inside an agent you have
direct access to the MCP tools. Calling `mcp-cli` from bash introduces shell
escaping issues and slower poll loops. Instead:

- **One-shot queries**: call the MCP tool directly.
- **Polling/waiting**: use `Monitor` with a bash loop that checks local
  conditions (file existence, exit codes), not `mcp-cli` calls; or use
  `timer_fire_when_idle_*` for process idle detection.
- **Coordination**: use `kv_*`, `todo_*`, `scratchpad_*` instead of
  bash-based state files.

## Solo control-plane terminology

Use this language consistently in prompts, comments, and playbook updates:

- `process`: the runtime instance managed by Solo (agent or terminal).
- `agent process`: a process spawned with `kind="agent"` (used for
  orchestrator/coordinator/researcher/builder roles).
- `terminal process`: an interactive shell process (when shell execution is
  needed).
- `spawn`: create a new process via Solo MCP.
- `agent_tool_id`: the runtime/tool selection used when spawning an agent
  process. This is a Solo database detail — callers express intent via the
  capability tier (`small`, `medium`, `large`) rather than a literal id.

When in doubt, refer to units as **processes**, then qualify as **agent
process** or **terminal process** for clarity.

## Tier Resolver

> **Superseded by overlays.** When an overlay claims `spawn-agent-at-tier`
> (e.g. [`./duo.md`](./duo.md)), this entire section is superseded for the
> active session and should not be applied; the overlay owns tier
> resolution. The rest of this file continues to apply.

The control-plane abstraction is `/spawn-agent <tier>`, optionally
`/spawn-agent <tier> --name <process-name> --purpose "<short reason>"`. The
resolver implements `spawn-agent-at-tier` from `capabilities.md`.

### Source of truth

Use Solo `list_agent_tools()` as the source of truth for available runtimes.
The current response shape includes:

- `id`: Solo `agent_tool_id` used by `spawn_process(kind="agent", agent_tool_id=N)`
- `name`: human-readable tool name
- `command`: command Solo will execute
- `tool_type`: runtime family such as `codex`, `opencode`, or `generic`
- `enabled`: whether the tool is enabled

Do not require a project-maintained static `agent_tool_id` mapping. IDs are
operational details and may change as tools are added, removed, or renamed.

### Resolution order

1. Query `mcp__solo__list_agent_tools()`.
2. Drop tools where `enabled != true`.
3. Drop tools whose `id` is listed in `exclude_ids`.
4. Classify remaining tools by `command` tokens.
5. Use `name` tokens only as a fallback or tie signal.
6. Fail with a clear error if no confident candidates exist.

### Command-first classification

Prefer model/runtime tokens found in `command` over display names.

Initial token policy (case-insensitive):

- `small`: `haiku`, `mini`, `flash`, `fast`, `cheap`, `small`
- `medium`: `sonnet`, `standard`, `medium`, `default`, `gpt-5.2`, `gpt-5.3-codex`, `gpt-5.4`
- `large`: `opus`, `flagship`, `max`, `large`, `gpt-5.5`

If a token appears in both `command` and `name`, the `command` match wins.
If `command` is ambiguous or unclassifiable, `name` may be used as a
fallback, but the `selection_reason` must say that name fallback was used.

### Name fallback

Name-based fallback exists for compatibility, not as the main contract.

Examples of useful fallback tokens:

- `small`: `haiku`, `mini`, `flash`, `fast`, `cheap`, `small`
- `medium`: `sonnet`, `standard`, `medium`, `default`
- `large`: `opus`, `flagship`, `pro`, `max`, `large`

Treat `pro` as a weak large-tier signal. Prefer stronger command/model
tokens when present.

### Spawn behavior

- Spawn an **agent process** with `kind="agent"` and the selected
  `agent_tool_id`.
- Return the created process metadata.
- Do not send a bootstrap message unless explicitly requested by the
  caller — bootstrap delivery is the `message-agent` capability.

### Return shape

Return:

- `process_id`
- `agent_tool_id`
- `tool_name`
- `tool_type`
- `command`
- `tier`
- `selection_reason`
- `alternatives_considered` (present even when empty)

### Failure behavior

Hard-fail with an actionable message when:

- No enabled tools map confidently to the requested tier.
- Multiple tools produce conflicting classification signals that cannot be
  resolved deterministically.
- Spawn fails after selecting a candidate.

Failure message must include: requested tier, discovered tools, enabled
tools after filtering, excluded IDs (if any), classification source used
(`command`, `name`, or none), and the token policy checked.

Do not silently fall back to an arbitrary enabled tool.
