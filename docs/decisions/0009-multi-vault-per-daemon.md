# ADR-0009: Multi-Vault per Daemon

**Status**: proposed
**Date**: 2026-04-26
**Decision-Makers**: Beau Simensen

---

## Context

`docs/product/vision.md` originally listed "Multi-directory support: One
watched directory per daemon instance" as a Non-Goal, with a parallel
Open Question enumerating three options for adding multi-vault support
later:

- **A**: One `hmnd` per vault; consumers route by host/port
- **B**: One `hmnd` covers many vaults
- **C**: Hybrid — daemon stays single-vault; `hmn`/MCP gain a
  registry/router that fans across daemons

The v0 forward-compat work in step 5 (the per-result `vault?: string`
field on every search response and outbox event) was added precisely so
adoption of B or C later would be additive rather than a versioned
migration.

After v0 shipped (2026-04-26), the operational reality of the "one
instance per vault" patterns (A and C) became the deciding factor:

- Multiple sets of configuration files or systemd units
- Multiple `hmnd` PID files, multiple data directories without a unified
  ownership story
- Consumers (CLI, MCP) become host/port-aware to reach the right vault
- Provisioning a new machine becomes scripted-startup-of-N-daemons

These costs land on the *user* and *consumer* side, while the cost of
B (schema mutation across config / CLI / specs / outbox) lands once on
the *daemon* side and is paid for by the v0 forward-compat work.

A separate, larger framing emerged alongside this — that vault
*definitions* are runtime state rather than configuration, and the
daemon should expose a Mutagen-shaped control-plane API for managing
them. That decision is captured in [ADR-0010](./0010-vault-definitions-as-runtime-state.md);
it depends on B but is independently arguable.

## Decision

Hypomnema adopts **option B**: one `hmnd` instance manages N vaults.
The vision Non-Goal "Multi-directory support: One watched directory per
daemon instance" is removed; the corresponding Open Question is
resolved.

Wire-shape implications:

- Per-result `vault: <id-string>` (the surrogate vault identifier).
  Populated whenever a vault is involved; v0 omitted; v0-compatible
  consumers continue to ignore the field.
- Search responses additionally carry per-result optional
  `vault_name?: string` — point-in-time-accurate display ergonomics.
  Outbox events carry `vault` (id) only and **do not** carry
  `vault_name` — outbox is durable, names can rot.
- `/health` and `/status` are not per-vault; `/status` returns a
  `vaults: [{...}]` array.

Search runs across all currently *active* vaults by default; the
per-result `vault` field disambiguates origin. An optional request-side
`vaults` filter restricts to a named subset (by name or ID). The
detailed semantics of cross-vault search — fan-out execution model,
result ordering and merge across independent indexes, pagination /
cursor across N vaults, per-vault `limit` enforcement, partial-failure
handling, and inclusion of paused / errored vaults — are deferred to
the vault-management spec's Open Questions and resolved at round-3
workplan time. The wire-shape decisions made here (per-result `vault`,
`vault_name` on transient responses, request-side `vaults` filter) are
forward-compat with any reasonable resolution of those semantics.

Storage layout: `<data_dir>/vaults/<vault_id>/` holds each vault's
`index.sqlite`, `outbox.jsonl`, and `meta.toml`; a top-level
`<data_dir>/vaults.sqlite` is the authoritative vault registry. Per-
vault state isolates vault lifecycle from peers (terminate = stop
watcher/indexer + remove the per-vault subdirectory).

Implementation lands as round 3 of the roadmap (`docs/roadmap/roadmap-2.md`),
after step 8's MCP wrapper. The v0 single-vault implementation continues
to ship through steps 6–8 unchanged; round 3 refactors the watcher /
indexer / store modules to be per-vault and adds the control-plane API
described in ADR-0010.

## Consequences

### Positive

- Single config; single PID; single port; single data-dir ownership
  model. Users and consumers get one daemon to run, configure, and
  reach.
- The forward-compat field shipped in step 5 starts being populated as
  designed; v0 wire bytes are unchanged for any consumer that runs
  against a single-vault daemon.
- Per-result `vault` and `vault_name` make result origin self-describing
  regardless of the cross-vault execution model chosen later. The
  questions of *how* cross-vault search merges, paginates, handles
  partial failure, and treats paused/errored vaults are deferred to
  round-3; the wire-shape commitments here are compatible with any
  resolution of those questions.
- Per-vault data subdirectory makes `vault terminate` safe and cheap
  (`rm -rf <data_dir>/vaults/<id>/` is the failure-mode floor).
- Provisioning a new machine is "install hmnd, run `hmn vault create
  ...` for each vault" — scriptable but not requiring N daemons.

### Negative

- The daemon is no longer single-context. Per-vault watcher/indexer
  state, vault-id-keyed schema, and concurrent multi-vault indexing
  are all new internal complexity. Mitigated by the per-vault state
  isolation in storage.
- `/status` restructures to `vaults: [{...}]` — not additive. v0
  consumers parsing `/status` for a single-vault shape will need
  trivial updates.
- N vaults indexing simultaneously share one embedding service; rate-
  limiting becomes shared-pool tuning rather than per-vault tuning.

### Neutral

- Step 5's `vault?: string` field decision (per-result, not top-level,
  for additive forward-compat under both B and C) was the right call.
  This ADR makes it load-bearing; ADR-0011 (CLI placement) and
  ADR-0010 (vault-as-state) build on it.
- The proposal's "Mutagen-shaped" operational ergonomics
  (`create / list / pause / ...`) are not part of *this* ADR — they
  belong to ADR-0010.

---

## Notes

- Resolves the multi-vault forward-compat Open Question in
  `docs/product/vision.md`.
- Predicted by [ADR-0006](./0006-outbox-outside-watched-directory.md)'s
  Consequences (the "Multiple vault directories could share a daemon
  data-dir scheme without collision" line) — see ADR-0006's amendment
  dated 2026-04-26 ratifying the predicted layout.
- Extended by [ADR-0010](./0010-vault-definitions-as-runtime-state.md)
  (vault definitions as state) and
  [ADR-0011](./0011-vault-management-on-hmn.md) (CLI placement).
- Related to [ADR-0008](./0008-two-binary-daemon-plus-cli.md) — the
  two-binary shape is preserved; `hmn` gains vault-management
  subcommands per ADR-0011.

## Amendments

<!-- None yet -->
