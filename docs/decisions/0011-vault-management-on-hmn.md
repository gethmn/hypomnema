# ADR-0011: Vault Management Lives on `hmn`

**Status**: proposed
**Date**: 2026-04-26
**Decision-Makers**: Beau Simensen

---

## Context

[ADR-0009](./0009-multi-vault-per-daemon.md) adopts multi-vault and
[ADR-0010](./0010-vault-definitions-as-runtime-state.md) adopts vault-
as-state with a control-plane API. The remaining placement question:
which CLI surface exposes the control-plane operations?

Three candidates:

1. **`hmn vault …`** — extend the existing CLI client. Fits ADR-0008's
   two-binary shape; `hmn` is already the "talk to a running `hmnd`"
   surface.
2. **`hmnd vault …`** — daemon binary handles its own configuration.
   Awkward: `hmnd` is otherwise the long-running daemon process; using
   it for short-lived "tell the daemon to do X and exit" mutates its
   identity.
3. **`hmndctl`** — a third binary, conventional for systemd-style
   control planes (`systemctl`, `kubectl` against `kubelet`). Adds a
   third binary that ADR-0008 explicitly settled was unnecessary.

Mutagen places its session-management commands on `mutagen` (its
user-facing CLI), not on a separate `mutagen-agent`-management binary —
direct precedent for option 1.

A secondary consideration: the MCP tool surface. If vault-management
operations are exposed as MCP tools, an agent (Claude Code, Iris) gains
the ability to create/list/pause/etc. vaults. The "agent ergonomics"
rationale of ADR-0004 (three search modes as peers) extends naturally:
vault management is just another set of operations the agent might
compose into work. Placing the operations on `hmn` (a process that
otherwise speaks HTTP to `hmnd`) means the underlying HTTP routes are
reused unchanged for MCP — same operations, two transports, no
duplication.

## Decision

Vault-management operations are implemented as `hmn` subcommands under
`hmn vault …`:

```
hmn vault create [--name NAME] PATH
hmn vault list
hmn vault status [NAME|ID]
hmn vault pause NAME|ID
hmn vault resume NAME|ID
hmn vault reset NAME|ID
hmn vault rename [NAME|ID] --name NEW_NAME
hmn vault rescan [NAME|ID]
hmn vault terminate NAME|ID
```

Each subcommand is a thin wrapper over the control-plane HTTP routes
defined in ADR-0010. Either a vault name or its surrogate ID may be
passed; the daemon resolves at request entry.

`hmnd scan` (the v0 standalone scan subcommand) is removed; its
behavior is subsumed by `hmn vault rescan [NAME|ID]`.

A third binary (`hmndctl`) is **not** introduced. The two-binary shape
of ADR-0008 is preserved.

The same control-plane operations are exposed as MCP tools — same
underlying handlers, same wire shapes. Which subset ships in which
round is a workplan-time decision; spec coverage is unconditional.

## Consequences

### Positive

- Two-binary shape preserved; ADR-0008 unchanged. No `hmndctl`
  proliferation.
- `hmn` is now the user's general-purpose interaction surface for
  Hypomnema — search, status, vault management. One binary to learn,
  one to install, one to alias.
- HTTP routes and MCP tools share handler code (the same pattern v0
  search already uses); adding MCP exposure of vault-management later
  is purely a tool-registration change, not a re-architecture.

### Negative

- `hmn` grows from a thin search/status client to a general daemon-
  interaction CLI. It remains thin (no embedded indexing, no
  watching), but its subcommand surface is materially larger.
- Vault-management as MCP tools is a write surface for agents; some
  deployments may want it disabled. Mitigated by per-tool gating in
  the MCP layer; default exposure is a workplan decision.

### Neutral

- ADR-0008's "`hmn` — thin client. Speaks HTTP to a running `hmnd` for
  most operations" remains accurate — `hmn` still speaks HTTP to a
  running `hmnd`. The set of "operations" simply grows. This is the
  reason this ADR *extends* rather than supersedes ADR-0008.

---

## Notes

- Extends [ADR-0008](./0008-two-binary-daemon-plus-cli.md). The two-
  binary shape and the "thin HTTP client" framing are preserved.
- Depends on [ADR-0009](./0009-multi-vault-per-daemon.md) and
  [ADR-0010](./0010-vault-definitions-as-runtime-state.md).
- Related to [ADR-0004](./0004-three-search-modes-as-peers.md) — agent
  ergonomics rationale (operations as peers an agent can compose)
  extends to vault management.

## Amendments

<!-- None yet -->
