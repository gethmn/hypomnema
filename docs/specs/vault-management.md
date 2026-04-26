# Vault Management Specification

**Version**: 0.1.0
**Date**: 2026-04-26
**Status**: Draft

---

> **File Location**: `docs/specs/vault-management.md`
>
> This is an outline produced from the LDS canon exploration of multi-vault adoption. Detailed sections are filled in by `spec-generator` post-canon-settlement; the outline below names every section the full spec must cover (per the rule that LDS specs cover the full intended surface, with workplan-time phasing for implementation).

---

## Overview

Vault lifecycle operations exposed by `hmnd`: `create`, `list`, `status`, `pause`, `resume`, `reset`, `rename`, `rescan`, `terminate`. Authoritative state lives in `<data_dir>/vaults.sqlite`; per-vault subdirectories at `<data_dir>/vaults/<id>/` hold each vault's `index.sqlite`, `outbox.jsonl`, and `meta.toml`. Operations are exposed over the HTTP control plane, the `hmn` CLI, and MCP tools — same handlers, three transports.

**Related Documents**:
- [ADR-0009: Multi-Vault per Daemon](../decisions/0009-multi-vault-per-daemon.md)
- [ADR-0010: Vault Definitions Are Runtime State, Not Configuration](../decisions/0010-vault-definitions-as-runtime-state.md)
- [ADR-0011: Vault Management Lives on `hmn`](../decisions/0011-vault-management-on-hmn.md)
- [Architecture: Vault Registry / Control-plane API](../architecture/overview.md)

---

## Behavior

### Vault Lifecycle State Machine

States: `nonexistent` → `active` ↔ `paused`; `active|paused` → `errored`; any → `terminated` (terminal). Triggers: `create` / `pause` / `resume` / `reset` / `rename` / `rescan` / `terminate` / startup-reconcile.

### Identifier Model

- Surrogate **ID**: opaque, immutable, generated at create-time. Format pinned in spec finalization (candidates: `vault_<base32>`, UUIDv7, ULID).
- **Name**: user-facing label, mutable, unique within the daemon.
- The daemon resolves either at request entry; name takes precedence on collision (collision is impossible by uniqueness — but the rule is documented).
- A configurable **default name** (`default_vault_name` in `config.toml`, default `"default"`) is used when a control-plane command omits the vault selector.

### Operations

Each operation gets a sub-section with full request/response shape, validation rules, and error catalog. Names and one-line semantics:

- **create**: validate path; canonicalize; reject if path is already registered or under `data_dir`; allocate ID; create per-vault subdirectory; insert registry row; start watcher + indexer.
- **list**: read registry; return `[{id, name, path, status, file_count, last_indexed_at, ...}]`.
- **status (id|name)**: single-vault detail.
- **pause / resume**: mutate registry status; signal watcher/indexer to stop/start; index untouched.
- **reset**: clear `last_error`; restart watcher + indexer; preserve index unless `--rebuild` is passed.
- **rename**: registry UPDATE (id ↔ new name); per-vault data unchanged; surrogate ID unchanged.
- **rescan**: force full reconciliation against vault contents; outbox events emitted as if from cold start.
- **terminate**: stop watcher + indexer; remove registry row; remove per-vault subdirectory; never touch the vault path's own files.

### Cross-Vault Search Semantics

*Settled* (this spec):
- Default scope: all currently active vaults (treatment of paused / errored vaults is open below)
- Per-result `vault` (id) + `vault_name` (name) disambiguate origin
- Request-side `vaults: string[]` filter selects a named subset (by name or ID)
- Identical wire-shape across filesystem-search, content-search, semantic-search

*Open* (resolved at round-3 workplan; see Open Questions):
- Result ordering / merge across vaults (per-vault-then-concat? interleaved by stable sort key? score-ranked? differs per search mode)
- Pagination / cursor across N independent indexes
- Fan-out execution: synchronous wait-for-all vs. streaming partial results vs. async-with-completion
- `limit` semantics: global limit applied after merge, per-vault limit, or proportional
- Partial-failure handling: fail-whole-query, return-partial-with-warning, silent-skip — and how the wire shape signals it
- Paused vault inclusion in default scope (likely silent skip, but unspecified)
- Errored vault inclusion in default scope (likely silent skip with diagnostic, but unspecified)
- Semantic-search global top-N: per-vault top-K then merge with score normalization, or alternatives

### Compose-Style Declarative Layer (deferred-to-workplan)

An optional `<data_dir>/hmnd-compose.toml` file is contemplated as an additive feature. The reconciler is additive: vaults listed in the file but not in state are created at startup; vaults in state but not in the file are left alone. The file does **not** destroy vaults. State remains canonical.

The file format and merging rules are pinned at the workplan that ships this layer; the spec describes the surface so future workplans can pull it without canon rewrites.

---

## Data Schema

### Registry — `<data_dir>/vaults.sqlite`

`vaults(id PK, name UNIQUE, path UNIQUE, status, created_at, last_error)` — see ADR-0010 for the illustrative DDL. ID format pinned in spec finalization.

### Per-Vault Layout

```
<data_dir>/vaults/<vault_id>/
  index.sqlite      # files, chunks, chunks_vec for this vault
  outbox.jsonl      # per-vault append-only event log
  meta.toml         # human-readable copy of registry row (id, name, path, status)
```

### Control-Plane HTTP Wire Shapes

- `POST /vaults` — request `{name?, path}`; response `{id, name, path, status, ...}`
- `GET /vaults` — response `{vaults: [{...}]}`
- `GET /vaults/{id}` — response `{id, name, path, status, file_count, last_indexed_at, last_error?, ...}`
- `POST /vaults/{id}/{op}` — response shape varies by op
- `DELETE /vaults/{id}` — response `{terminated: true}`

### Search-Side Cross-References

- `filesystem-search.md`, `content-search.md`, `semantic-search.md` — add per-result `vault` (id) and `vault_name` (name); add request-side `vaults?: string[]` filter
- `change-events.md` — add `vault` (id); **no** `vault_name` field (outbox is durable; names rot)

### MCP Tool Surface (Full)

Tool names mirror the operations: `vault.create`, `vault.list`, `vault.status`, `vault.pause`, `vault.resume`, `vault.reset`, `vault.rename`, `vault.rescan`, `vault.terminate`. Tool descriptions, parameter schemas, and response shapes mirror the HTTP control plane.

---

## Examples

- Fresh install: `hmn vault create ~/personal-vault` → creates a vault named `default` (or per `default_vault_name`).
- Named create: `hmn vault create --name=personal ~/personal-vault`.
- Rename: `hmn vault rename personal --name my-vault`.
- Cross-vault content search: `hmn search content "pgvector"` → results from all active vaults; each result carries `vault` and `vault_name`.
- Filtered search: `hmn search content "pgvector" --vaults personal,work`.
- Terminate then recreate: `hmn vault terminate foo && hmn vault create --name=foo ~/foo`.

---

## Edge Cases

- **Path collision** (path already registered under different name): reject with `409 vault_path_conflict`.
- **`data_dir` under any vault path**: reject at startup-reconcile and at create-time with `422 vault_path_invalid`.
- **Vault path becomes inaccessible at runtime**: vault enters `errored`; other vaults continue; `last_error` recorded.
- **Daemon crash mid-create**: per-vault subdirectory may exist without registry row → reconcile drops orphan subdirs at next startup.
- **Concurrent operations on same vault**: serialized per-vault.
- **Concurrent operations on different vaults**: parallel.
- **Default-name collision**: existing default vault + new create without `--name` → reject with `409 vault_name_conflict`.
- **`default_vault_name = ""`**: every command must specify a name or ID; the daemon never resolves a default.

---

## Error Handling

| Error Condition | Code | Message | Recovery |
|---|---|---|---|
| Vault not found | `404 vault_not_found` | "no vault named <X>" + closest-name hint | Caller specifies a valid name or ID |
| Path already registered | `409 vault_path_conflict` | "<path> is already vault <name>" | Use the existing vault or terminate-then-create |
| Name already in use | `409 vault_name_conflict` | "<name> is already in use" | Pick another name or rename the existing vault |
| Path invalid | `422 vault_path_invalid` | reason | Caller fixes path |
| Vault errored | `503 vault_errored` | "vault <X> is in errored state" | `hmn vault reset <X>` |
| Registry corrupt | `500 registry_corrupt` | "vaults.sqlite read failed" | Operator restores from backup; daemon refuses to serve until restored |

(Plus per-op errors inherited from the four search/event specs once they're amended.)

---

## Integration Points

- **With Watcher / Indexer**: per-vault instances; lifecycle driven by control-plane mutations.
- **With Outbox**: per-vault outbox file under `<data_dir>/vaults/<id>/outbox.jsonl`.
- **With Search API**: cross-vault by default; `vaults` filter restricts.
- **With MCP**: same operations as HTTP, registered as tools.

---

## Implementation Notes

- Round 3 of the roadmap implements this; v0 (rounds 1–2) ships single-vault. Workplan time chooses whether to ship the full surface in one round or phase (e.g., create/list/status/terminate first; pause/reset/rename/rescan + Compose layer in a follow-on workplan).
- ID format and Compose-file format pinned at workplan time.
- Cross-vault search semantics (ordering, pagination, fan-out, partial failure) are explicitly under-specified in this draft. The round-3 workplan resolves them inline; the spec gets a follow-on amendment at that time. Until resolved, single-vault behavior of each search mode (per its own spec) is the documented behavior.

---

## Open Questions

- Result ordering across vaults for filesystem-search: cross-vault default (interleaved ascending path? per-vault then concat with stable vault ordering?). Today's spec is "ascending path"; lifting that to multi-vault is non-obvious.
- Result ordering across vaults for content-search: same question; today's spec is per-vault path-sorted.
- Pagination / cursor across N independent indexes: stable cursor design when each vault has its own row ordering. Possible shapes: opaque cursor encoding per-vault offsets; per-vault paginated then merge per page; require sort-key-based cursors.
- Fan-out execution model: gather-then-respond (simplest, but slowest vault blocks response) vs. streaming (chunked HTTP / SSE / NDJSON) vs. async-with-completion (job ID + poll).
- `limit` semantics across vaults: does `limit=25` mean (a) each vault returns ≤25, daemon merges top-25; (b) proportional split; (c) global budget? (a) is correct under any stable ordering but expensive.
- Partial-failure handling: one vault errors mid-query — fail whole, return partial with warning, silent skip. Wire-shape signal for partial results (e.g., `truncated_due_to: ["vault_x"]`).
- Paused vault inclusion in default scope: silent skip is the natural default; document it.
- Errored vault inclusion in default scope: same default; needs a wire-shape diagnostic so consumers know coverage was incomplete.
- Semantic-search global top-N: cosine similarity scores are per-index; merging requires no cross-index normalization in practice (cosine is bounded), but the ranking property "global top-N" requires fetching per-vault top-N and re-sorting — which interacts with the `limit`-semantics question above.
- ID format: `vault_<base32>` vs. UUIDv7 vs. ULID.
- Default-name semantics when `default_vault_name = ""` (must always be specified).
- MCP tool gating: should `vault.create` / `vault.terminate` be gated separately from read-only ops?
- Compose-file format and merging rules.

---

## Revision History

| Version | Date | Changes |
|---------|------|---------|
| 0.1.0 | 2026-04-26 | Initial outline, seeded from ADR-0009 / ADR-0010 / ADR-0011. Cross-vault search semantics deliberately under-specified; round-3 workplan resolves. |
