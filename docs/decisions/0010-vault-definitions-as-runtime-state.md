# ADR-0010: Vault Definitions Are Runtime State, Not Configuration

**Status**: proposed
**Date**: 2026-04-26
**Decision-Makers**: Beau Simensen

---

## Context

[ADR-0009](./0009-multi-vault-per-daemon.md) adopts multi-vault per
daemon (option B from vision's menu). That ADR settles the wire-shape
and storage-layout questions but leaves a separable question open: are
vaults defined in *configuration* (a `[[vaults]]` array in
`config.toml`, edit-and-restart semantics) or in *runtime state* (a
control-plane API mutates a daemon-owned registry; `hmnd` reconciles on
startup)?

The original cost framing in vision priced multi-vault as "schema
mutation across config, CLI, specs, event stream." The user's framing
distinguishes the two:

> "Which port does `hmnd` listen on? Configuration. Which directories
> are currently watched? State!"

Mutagen and Docker are both precedent: the daemon's listening behavior
is config; the *things being managed* (sync sessions, containers, here
*vaults*) are state, mutated via a control-plane API.

The cost of vault-as-state is daemon-side: an authoritative vault
registry, idempotent lifecycle operations, control-plane HTTP routes,
and a startup reconciliation step. The benefit is operational: one TOML
file plus runtime mutations rather than reload-on-config-change
semantics, GitOps-style declarative provisioning available later as an
*additive* layer over the API rather than a parallel source of truth.

## Decision

Vault definitions are **runtime state**, not configuration. The
authoritative store is `<data_dir>/vaults.sqlite`, a top-level registry
managed by the daemon. Configuration in `config.toml` describes only
daemon-level behavior (HTTP bind, embedding endpoint, default vault
name, watcher tuning, log levels) — never which vaults exist.

Vault lifecycle operations are mutations against the registry, exposed
as control-plane API endpoints:

- `POST /vaults` — create
- `GET /vaults`, `GET /vaults/{id}` — list, get
- `POST /vaults/{id}/pause` — pause indexer/watcher; vault remains
  registered
- `POST /vaults/{id}/resume` — counterpart to pause
- `POST /vaults/{id}/reset` — clear error state and reinitialize
- `POST /vaults/{id}/rename` — single-row rename; index unchanged
- `POST /vaults/{id}/rescan` — force full reconciliation
- `DELETE /vaults/{id}` — terminate; remove registry row and
  per-vault data subdirectory; never touches the vault's own files

The CLI surface for these operations lives on `hmn` per
[ADR-0011](./0011-vault-management-on-hmn.md).

Vault registry schema (illustrative, finalized in vault-management
spec):

```sql
CREATE TABLE vaults (
  id              TEXT PRIMARY KEY,        -- surrogate ID, opaque to users
  name            TEXT NOT NULL UNIQUE,    -- user-facing label, mutable
  path            TEXT NOT NULL UNIQUE,    -- absolute, canonicalized
  status          TEXT NOT NULL,           -- 'active' | 'paused' | 'errored'
  created_at      TEXT NOT NULL,           -- ISO-8601 µs UTC
  last_error      TEXT
);
```

Storage layout under the daemon data directory:

```
<data_dir>/
  vaults.sqlite               -- authoritative registry
  vaults/
    <vault_id>/
      index.sqlite            -- files, chunks, chunks_vec for this vault
      meta.toml               -- human-readable copy of registry row
  hmnd.pid
  logs/
```

Reconciliation: on daemon startup, the daemon reads `vaults.sqlite`,
verifies each vault's path exists and `<data_dir>/vaults/<id>/` is
present, and starts a watcher + indexer for each `active` vault.
Vaults whose path is missing enter `errored` state with a recorded
`last_error`; the daemon stays running and other vaults continue to
serve.

Concurrency: control-plane operations on the same vault are serialized.
Operations on different vaults run in parallel.

Idempotency: `terminate` followed by `create` with the same name and
path is supported and cheap (the registry row is gone; the per-vault
subdirectory is gone; the create operation builds both anew).

A future declarative layer (e.g., an `hmnd-compose.toml` file) is
contemplated as an additive feature: it would call the control-plane
API on startup to ensure listed vaults exist (additive only — does
not destroy vaults missing from the file). State remains canonical.
The vault-management spec describes the surface; the round-N workplan
decides when it ships.

## Consequences

### Positive

- Operational ergonomics match Mutagen / Docker — `hmn vault create
  ~/personal-vault` is more natural than "edit TOML; restart daemon."
- Vault rename is a single registry UPDATE; the per-vault
  subdirectory's name is the surrogate ID, which never changes. No
  data movement.
- Vault terminate is cheap: stop the watcher/indexer; remove the
  per-vault subdirectory; delete the registry row.
- Configuration drift between "what's in TOML" and "what's running"
  cannot occur — there is no TOML source of truth for vaults.
- An additive Compose-style declarative layer can land later without
  touching any of the runtime semantics.

### Negative

- Daemon startup-sequence rewrite: load and reconcile registry before
  serving. Slightly more complexity than "read TOML, start watcher."
- Authoritative state in a SQLite file requires atomic write semantics
  for control-plane mutations, and a recovery path if the file is
  corrupted.
- "First-run" UX: a fresh `hmnd` has zero vaults until the user runs
  `hmn vault create`. Mitigated by the configurable default-name
  convention and good first-run docs.

### Neutral

- The vault-management API is a write surface. v0 had only the
  read-side search API; this is the daemon's first user-mutable
  state. Authentication is still localhost-only per the existing
  cross-cutting security stance.

---

## Notes

- Extends [ADR-0009](./0009-multi-vault-per-daemon.md) — without
  multi-vault, vault-as-state is moot; without vault-as-state,
  multi-vault could still be `[[vaults]]` in TOML.
- Related to [ADR-0006](./0006-outbox-outside-watched-directory.md) —
  per-vault subdirectory under the daemon data dir is the realization
  of ADR-0006's predicted layout.
- Related to [ADR-0008](./0008-two-binary-daemon-plus-cli.md) — the
  control-plane API is exposed on the same HTTP server as search;
  CLI surface placement is in ADR-0011.

## Amendments

<!-- None yet -->
