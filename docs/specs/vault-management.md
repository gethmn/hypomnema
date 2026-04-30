# Vault Management Specification

**Version**: 1.2.0
**Date**: 2026-04-30
**Status**: Approved

---

## Overview

A Hypomnema daemon owns zero or more **vaults** — directories on disk whose contents the daemon watches, indexes, and exposes via search. Vault lifecycle operations (`create`, `list`, `status`, `pause`, `resume`, `reset`, `rename`, `rescan`, `terminate`) and live event subscription (`watch`) are exposed by `hmnd` over an HTTP control plane, the `hmn` CLI, and MCP tools — same handlers, three transports where transport framing allows. The `watch` operation is available via CLI and HTTP; MCP `vault_watch` is deferred pending rmcp long-lived streaming support.

Authoritative vault state lives in `<data_dir>/vaults.sqlite`. Each vault gets a per-vault subdirectory at `<data_dir>/vaults/<vault_id>/` holding its own `index.sqlite` and `meta.toml`. Operations on the same vault are serialized; operations on different vaults run in parallel. Search queries fan out across all currently-active vaults by default and can be narrowed by name or surrogate ID.

This spec covers the full intended surface. Per the project's LDS rule that specs cover the full intended surface, every operation is fully specified here even when implementation phases across multiple workplans:

- **Step 10** ships `create`, `list`, `status`, `terminate` plus the cross-vault search refinements.
- **Step 11** (round-3 follow-on) ships `pause`, `resume`, `reset`, `rename`, `rescan` against the same wire shapes.
- **v0 event-stream amendment** ships `watch` as a live-only change-event subscription over CLI and HTTP. MCP `vault_watch` is deferred pending rmcp streaming support (see [change-events.md](./change-events.md#mcp-subscription)).

**Related Documents**:
- [ADR-0009: Multi-Vault per Daemon](../decisions/0009-multi-vault-per-daemon.md)
- [ADR-0010: Vault Definitions Are Runtime State, Not Configuration](../decisions/0010-vault-definitions-as-runtime-state.md)
- [ADR-0011: Vault Management Lives on `hmn`](../decisions/0011-vault-management-on-hmn.md)
- [filesystem-search.md](./filesystem-search.md) · [content-search.md](./content-search.md) · [semantic-search.md](./semantic-search.md) · [change-events.md](./change-events.md)
- [Architecture: Vault Registry / Control-plane API](../architecture/overview.md)

---

## Behavior

### Vault Lifecycle State Machine

```
            create                pause
nonexistent ─────► active ◄──────────────► paused
                    │ ▲                       │
                    │ │ resume                │
            err     │ │ (clears last_error)   │ err
                    ▼ │                       ▼
                  errored ◄─────────────── errored
                    │
                    │ reset (clears last_error;
                    │        restarts watcher+indexer)
                    ▼
                  active
                    │
                    │ terminate (any state)
                    ▼
              terminated (terminal — registry row removed)
```

| State | Description | Transitions |
|---|---|---|
| `active` | Watcher + indexer running; vault answers searches and emits live change events. | → `paused` (on `pause`); → `errored` (on runtime error); → `terminated` (on `terminate`). |
| `paused` | Watcher + indexer stopped; index preserved; vault is silently skipped from default search scope. | → `active` (on `resume`); → `terminated` (on `terminate`). |
| `errored` | Runtime error rendered the vault unsable (path inaccessible, schema mismatch, etc.); `last_error` populated. | → `active` (on successful `reset`); → `errored` (on `reset` that fails again); → `terminated` (on `terminate`). |
| `terminated` | Registry row removed; per-vault subdirectory removed; vault path's own files untouched. | Terminal. The same `name` and `path` may be reused for a fresh `create` (with a new surrogate ID). |

Startup-reconcile applies the same transitions: a row whose path is no longer accessible enters `errored`; a row whose per-vault subdirectory is missing has the subdirectory recreated and remains `active`; an orphan per-vault subdirectory whose registry row is missing is removed.

### Identifier Model

- **Surrogate ID** (`vault_id`): immutable, generated at `create`-time as a **UUIDv7** (time-ordered, monotonic-leaning, 128-bit). Stored as the canonical UUID string form (e.g., `01951f6c-7c3b-7a2e-8c1d-1a2b3c4d5e6f`) in `vaults.id` and in event payloads. Display contexts (CLI tables, log lines) may decorate with a `vault_` prefix (`vault_01951f6c-…`) for visual distinction; the wire / storage form is always the bare UUID.
- **Name**: user-facing label, mutable via `rename`, **unique within the daemon**. Must be a non-empty string of `[A-Za-z0-9_-]` characters (no path separators, no whitespace; CLI-friendly).
- **Either is accepted as a `name_or_id` argument** to control-plane operations and search-side `vaults` filters. The daemon resolves at request entry; **names take precedence** on collision (collision is impossible by uniqueness, but the resolution rule is documented).
- The daemon never mints names — operators supply them, or they fall back to the configured default (see § Default Name Resolution).

### Default Name Resolution

The configured `default_vault_name` (in `config.toml`, default `"default"`) is used when a control-plane command omits the vault selector or when a `create` request omits `name`.

| `default_vault_name` | Behavior on `create` without `--name` | Behavior on `status` / `terminate` / etc. without `<target>` |
|---|---|---|
| Non-empty string (e.g., `"default"`, `"personal"`) | New vault is created with this name (or `409 vault_name_conflict` if already taken). | The default-named vault is selected. |
| Empty string `""` | **Rejected** with `422 vault_path_invalid` (`"name is required when default_vault_name is empty"`). | **Rejected** by the CLI / API with `vault_not_found`-style validation: every command must specify a name or ID. |

Operators who want strict explicit-naming set `default_vault_name = ""`. The daemon never picks a name on the operator's behalf in that mode.

### Operations

Each operation is invoked over HTTP, CLI, or MCP. The wire shapes in § Data Schema § Control-Plane HTTP Wire Shapes are canonical; the CLI and MCP transports are thin shims.

| Operation | Ships in | One-line semantics |
|---|---|---|
| `create` | Step 10 | Validate path; canonicalize; reject if path is already registered or if `data_dir` is under it; mint UUIDv7; create per-vault subdirectory; insert registry row; start watcher + indexer. |
| `list` | Step 10 | Read registry; return all vaults with status, file count, last-indexed timestamp, last error. |
| `status` (id\|name) | Step 10 | Single-vault detail; same fields as `list` plus per-vault storage/status metadata. |
| `pause` | Step 11 | Stop the vault's watcher + indexer; transition `active → paused`; index untouched. Idempotent on already-paused. |
| `resume` | Step 11 | Restart watcher + indexer; transition `paused → active` or `errored → active` if the underlying error is resolved (`reset` is the explicit-recovery path; `resume` from `errored` is a convenience). Clears `last_error` on success. |
| `reset` | Step 11 | Force-clear `last_error`; restart watcher + indexer. With `--rebuild`, also drop and rebuild the per-vault `chunks` + `chunks_vec` tables (keeps `files`). |
| `rename` | Step 11 | Registry UPDATE: `id ↔ new name`. Per-vault data unchanged; surrogate ID unchanged. Live change events continue to carry the surrogate ID, not the name. |
| `rescan` | Step 11 | Force full reconciliation against the vault's directory contents; live change events emitted for files whose content hash changes. |
| `watch` | v0 event stream | Subscribe to live change events for one vault or all active vaults; no replay / no `since` in v0. |
| `terminate` | Step 10 | Stop watcher + indexer; remove registry row; remove the per-vault subdirectory at `<data_dir>/vaults/<id>/`; **never** touch the vault path's own files. |

#### `create`

**Request**: `{name?: string, path: string}`. `path` must be absolute or expandable (leading `~`); the daemon `canonicalize`s before the uniqueness check. `name` defaults to `default_vault_name` if omitted.

**Validation**:
- Reject `path` if `canonicalize` fails (path doesn't exist, is unreadable, or contains symlink loops) → `422 vault_path_invalid`.
- Reject if `data_dir` is under the canonicalized vault path (would cause the daemon to watch its own state files) → `422 vault_path_invalid`.
- Reject if `path` is already registered (under any name) → `409 vault_path_conflict`.
- Reject if `name` is already in use (under any path) → `409 vault_name_conflict`.
- Reject if `name` is omitted and `default_vault_name = ""` → `422 vault_path_invalid`.

**Flow**:
1. Acquire the manager's runner-write-lock.
2. Validate request (above).
3. Mint UUIDv7 vault id.
4. Insert registry row with `status = active`, `last_error = NULL`, `created_at = now()`.
5. Create `<data_dir>/vaults/<id>/`; write `meta.toml` with the registry row's fields.
6. Construct the per-vault `Store`; start watcher + indexer.
7. Insert into the runner map.
8. Release the runner-write-lock.
9. Return the inserted row in the response.

**Crash safety**: if the daemon crashes between step 4 (registry row inserted) and step 5 (subdirectory created), next-startup reconcile sees a registry row with no subdirectory and recreates it. Conversely, if a previous `terminate` crashed between deleting the row and removing the subdirectory, that orphan subdirectory is removed at startup before fresh `create`s run.

**Response** (`200 OK`): single vault row (see § Data Schema § Control-Plane HTTP Wire Shapes).

#### `list`

**Request**: no body.

**Flow**: read all rows from `vaults.sqlite`; for each row, augment with file-count and last-indexed-at from the per-vault `index.sqlite` (cached in the runner; cheap).

**Response** (`200 OK`): `{vaults: [VaultRow, ...]}`.

#### `status`

**Request**: path-parameterized — `GET /vaults/{name_or_id}`.

**Validation**:
- If `name_or_id` doesn't resolve, return `404 vault_not_found` with a closest-name hint computed via Levenshtein on the registry's name list (omit hint if no candidate is within distance 3).

**Response** (`200 OK`): single `VaultRow` plus per-vault storage/status metadata.

#### `pause` (step 11)

**Request**: `POST /vaults/{name_or_id}/pause`, no body.

**Flow**: take the per-vault `op_lock` (see § Concurrency); transition status `active → paused`; signal watcher + indexer to drain (max 30s); leave the per-vault `index.sqlite` in place.

**Response** (`200 OK`): updated `VaultRow` with `status: "paused"`.

**Idempotent on `paused`**: returns `200 OK` with the existing row unchanged.

#### `resume` (step 11)

**Request**: `POST /vaults/{name_or_id}/resume`, no body.

**Flow**: take the per-vault `op_lock`; transition `paused → active` or `errored → active` (where applicable; `errored → active` only if the underlying error is no longer present — e.g., the path is reachable again); restart watcher + indexer; clear `last_error` on success.

**Response** (`200 OK`): updated `VaultRow` with `status: "active"`.

#### `reset` (step 11)

**Request**: `POST /vaults/{name_or_id}/reset`, optional body `{rebuild?: boolean}` (default `false`).

**Flow**: take the per-vault `op_lock`; clear `last_error`; restart watcher + indexer. If `rebuild: true`, additionally drop and rebuild the `chunks` + `chunks_vec` tables in the per-vault `index.sqlite` (preserves `files` rows; re-embeds at startup).

**Response** (`200 OK`): updated `VaultRow`.

#### `rename` (step 11)

**Request**: `POST /vaults/{name_or_id}/rename`, body `{new_name: string}`.

**Validation**:
- `new_name` matches `[A-Za-z0-9_-]{1,}`.
- `new_name` is not already in use → `409 vault_name_conflict`.

**Flow**: take the per-vault `op_lock`; `UPDATE vaults SET name = ?` on the matching row; rewrite the per-vault `meta.toml`. The surrogate ID is unchanged; live change events continue to carry the same `vault` ID.

**Response** (`200 OK`): updated `VaultRow` with the new `name`.

#### `rescan` (step 11)

**Request**: `POST /vaults/{name_or_id}/rescan`, no body.

**Flow**: take the per-vault `op_lock`; force a full directory walk; publish live change events only for files whose content hash differs from the stored value. A fully indexed, unchanged vault may emit few or zero events. Consumers that need current state must query the index.

**Response** (`200 OK`): updated `VaultRow` plus a `rescan_initiated_at` timestamp; the rescan itself is asynchronous.

#### `watch` (v0 event stream)

**Request**: long-lived streaming subscription by vault selector. CLI shape is `hmn vault watch [NAME|ID] [--all]`; HTTP framing is pinned in [change-events.md](./change-events.md). MCP `vault_watch` is deferred.

**Flow**: resolve the requested vault or active vault set; subscribe the client to the daemon's live in-memory change event bus; stream newline-delimited event envelopes until disconnect, daemon shutdown, vault termination, or stream error.

**Response**: live stream of change-event envelopes. No replay and no `since` argument in v0.

**Errors**: `404 vault_not_found` when a requested selector does not resolve.

#### `terminate`

**Request**: `DELETE /vaults/{name_or_id}`, no body.

**Validation**:
- If `name_or_id` doesn't resolve → `404 vault_not_found`.

**Flow**:
1. Acquire the manager's runner-write-lock.
2. Resolve `name_or_id` → `vault_id`.
3. Remove from the runner map; signal watcher + indexer shutdown via the runner's `WatcherShutdownHandle`; await drain (max 30s — beyond which we force-stop and continue).
4. Delete the registry row.
5. Remove `<data_dir>/vaults/<id>/` (using `std::fs::remove_dir_all`; same-filesystem assumption inherited from step 9).
6. Release the runner-write-lock.

**Crash safety**: if the daemon crashes between step 4 and step 5, the orphan per-vault subdirectory is harmless; startup-reconcile detects "subdirectory without registry row" and removes it.

**Important**: `terminate` does **not** touch the vault path's own files. The user's vault directory (e.g., `~/personal-vault`) is untouched. Only the daemon's per-vault state under `<data_dir>/vaults/<id>/` is removed.

**Response** (`200 OK`): `{terminated: true, id: "<surrogate_id>"}`.

### Cross-Vault Search Semantics

The four search/event specs ([filesystem-search.md](./filesystem-search.md), [content-search.md](./content-search.md), [semantic-search.md](./semantic-search.md), [change-events.md](./change-events.md)) cross-reference this section for cross-vault execution semantics. Eight resolutions, applied uniformly.

#### Result ordering — filesystem-search and content-search

**Global path-ascending across all in-scope vaults.** The merge step interleaves results by `path` (ascending, byte-lexicographic). Identical paths across two vaults break ties by `vault_id` (UUIDv7 → creation-time-stable, so the same query against the same content returns the same order across daemon restarts).

For N=1 (single-vault), this is identical to v0/step-9 behavior — single slice already path-sorted. For N≥2, the cross-vault default is "as if you had one big vault" semantically, with `vault` + `vault_name` per result for origin disambiguation. Operators can split or merge vaults without confusing consumers about ordering.

#### Result ordering — semantic-search

**Score-descending across all in-scope vaults.** Cosine similarity is bounded `[0.0, 1.0]` ([semantic-search.md § Score conversion](./semantic-search.md#score-conversion)) and comparable across same-model embeddings, so no cross-vault score normalization is needed. Identical scores break ties by `vault_id`.

**Same-embedding-model assumption**: all vaults use the daemon-wide embedding model and dimension. The embedding service is configured per-daemon, not per-vault, and `chunks_vec`'s dimension is migration-baked (see [ADR-0007](../decisions/0007-sqlite-vec-over-alternatives.md)). Multi-model-embedding-per-vault is round-4+; until then, cross-vault score comparison is sound by construction.

#### `limit` semantics

**Gather all per-vault results (each vault contributes up to `limit`), merge by the mode-specific ordering, truncate to global `limit`.**

The per-vault `truncated` flag is preserved into the merged response's `truncated` (any per-vault truncation **OR** post-merge truncation → global `truncated: true`).

Worst-case cost is `N_vaults * limit` rows in memory pre-merge. For typical limits (100) and typical vault counts (1–5), this is a few hundred rows — negligible. Per-vault budgeting (each vault gets `limit / N`) is rejected because the all-matches vault under-fills its share when another vault has no matches; global `limit` doesn't have this issue.

**Edge cases**: `limit = 0` is `invalid_request`. `limit > 1000` is rejected at request validation (defense against runaway memory).

#### Fan-out execution

**Sequential per-vault iteration; gather-then-respond.** Step 9's pattern in `src/api/search.rs::filesystem`/`content`/`semantic` already iterates vaults sequentially; step 10 keeps that shape. For typical vault counts (1–5) on a single host, sequential per-vault SQLite reads complete in tens of milliseconds; the parallelism gain from tokio-task fan-out is marginal and brings partial-failure-during-spawn questions and per-vault timeout semantics that aren't worth the complexity for round 3.

Streaming responses (chunked HTTP / SSE / NDJSON) and async-with-completion (job-id + poll) are deferred to round 4+. The trigger to revisit: a deployment surfaces with N≥10 vaults or measured vault-search latency that begs for parallelism.

#### `vaults` filter (cross-cutting)

All three search request types accept `vaults?: Vec<String>`:

- `vaults: None` (or omitted) → query all currently active vaults.
- `vaults: Some([])` → request validation error (`invalid_request: vaults filter must be non-empty`).
- `vaults: Some([...])` → query only the named subset; each entry is matched against `name` first, then against `id`. Unknown names produce `partial_results.failed` entries with `code: "vault_not_found"`. Paused/errored vaults in the subset are skipped per the rules below.

#### Partial-failure handling

**Silent-skip plus a `partial_results` diagnostic on the response envelope.** When a per-vault search errors (vault-side database error, vault disappeared mid-query, etc.), the daemon logs the error and continues; the merged response carries a non-empty `partial_results` field listing which vaults were skipped or failed and why.

Wire shape (added to all three search response envelopes):

```yaml
results: [...]
truncated: false
partial_results:                           # OMITTED when no skips/failures
  skipped:
    - vault: "<id>"
      vault_name: "<name>"
      status: "paused" | "errored"
      reason: "vault is paused" | "vault is errored: <last_error>"
  failed:
    - vault: "<id>"
      vault_name: "<name>"
      code: "vault_search_failed" | "vault_not_found" | ...
      message: "<short detail>"
```

The `partial_results` field is present **only when at least one vault was skipped or failed**. Empty arrays are not emitted; the field itself is absent in the all-success / all-active case. Wire-bytes additive: v0/step-9 consumers parsing `results` / `truncated` see exactly the same fields when no skip/fail happens. No bumped-major-version breaking change.

`skipped` is for *intentional* exclusions (paused/errored vault). `failed` is for *unexpected* runtime errors. Distinguishing them gives consumers the signal they need without overloading one channel.

#### Paused vault inclusion in default scope

**Silent skip plus `partial_results.skipped` diagnostic.** Default scope (`/search/...` with no `vaults` filter) does not query paused vaults; each paused vault that would have been queried is added to `partial_results.skipped` with `status: "paused"` and `reason: "vault is paused"`. Pause is user-initiated; the user's intent is "stop querying this vault until I resume."

When the request includes `vaults: [...]` and the named subset includes paused vaults, the same skip-and-diagnose treatment applies: filtering names a vault, paused state still skips it, the consumer learns via `partial_results`.

Step 10 doesn't ship `pause`/`resume` (those are step 11), but the registry already supports the `paused` state from step 9. The skip behavior must work in step 10 to be ready for step 11's user surface — and to handle paused-row test fixtures.

#### Errored vault inclusion in default scope

**Silent skip plus `partial_results.skipped` diagnostic with the registry's `last_error` propagated.** Same treatment as paused, with `status: "errored"` and `reason: "vault is errored: <last_error>"`. The `last_error` text is operator-supplied / runtime-supplied diagnostic (e.g., `"vault path /home/foo no longer accessible"`) — propagate verbatim so consumers can act on it.

If `last_error` is `NULL` in the registry (which shouldn't happen for an `errored`-status row but is permitted by the schema), use a static fallback `"vault is errored (no last_error recorded)"`.

### Concurrency

**Per-vault async-mutex.** A `VaultManager` struct owns an `RwLock<HashMap<VaultId, Arc<VaultRunner>>>`; each `VaultRunner` carries the runner's immutable `VaultEntry` plus a `tokio::sync::Mutex<()>` (`op_lock`) that serializes vault-scoped operations.

```rust
pub struct VaultManager {
    registry: Arc<VaultRegistry>,
    runners: Arc<RwLock<HashMap<VaultId, Arc<VaultRunner>>>>,
    config: Arc<Config>,
    embedder: Arc<dyn Embedder>,
    embedding_dimension: u32,
    data_dir: PathBuf,
}

pub struct VaultRunner {
    entry: Arc<VaultEntry>,
    op_lock: tokio::sync::Mutex<()>,
    shutdown: WatcherShutdownHandle,
}
```

- **Read-side** (`active_vaults()` for search handlers): read-lock on `runners`, clone-and-filter `Arc<VaultEntry>`s for active vaults. No mutex acquisition per search; per-search overhead is the read-lock acquire/release plus the Arc clones.
- **Write-side** (`create`, `terminate`): write-lock on `runners` for the create-or-remove operation itself; the registry insert/delete and per-vault subdirectory creation/removal happen inside the write-lock window. Operations on different vaults that mutate runner-set composition serialize by virtue of the outer write-lock, but they are short.
- **Per-vault op-side** (`pause`, `resume`, `reset`, `rename`, `rescan` — step 11): take the per-vault `op_lock` while keeping a read-lock on the outer map. Operations on the same vault serialize; operations on different vaults run in parallel.

`tokio::sync::Mutex` (not `std::sync::Mutex`) is required because async control-plane ops await across the mutex boundary (registry SQL via `spawn_blocking`, fs ops, watcher shutdown).

This shape implements ADR-0010's invariants ("operations on the same vault are serialized; operations on different vaults run in parallel") with two well-understood synchronization primitives. Alternatives considered and rejected:

- **One actor task per vault, channels for ops**: more complex; vault ops are infrequent, so per-op tokio-task-startup cost is irrelevant.
- **Single channel, dispatch by vault id**: loses the natural read-side parallelism of search handlers.

### MCP Tool Surface

The lifecycle operations plus the live event subscription are exposed as MCP tools, naming-mirroring the HTTP control plane:

| Tool | Trust posture | Ships in |
|---|---|---|
| `vault_list` | Read-only | Step 10 |
| `vault_status` | Read-only | Step 10 |
| `vault_create` | Write (gated) | Step 10 |
| `vault_terminate` | Write (gated) | Step 10 |
| `vault_pause` | Write (gated) | Step 11 |
| `vault_resume` | Write (gated) | Step 11 |
| `vault_reset` | Write (gated) | Step 11 |
| `vault_rename` | Write (gated) | Step 11 |
| `vault_rescan` | Write (gated) | Step 11 |
| `vault_watch` | Read-only, long-lived | **deferred** — pending rmcp streaming support |

**Gating**: a single config key `[mcp] enable_write_tools: bool` (default `true`) controls whether the write tools are advertised. When `false`, only the read-only tools are listed in the MCP `tools/list` response and `tools/call` against any write tool returns a structured `write_tools_disabled` error.

**Why single-flag over per-tool gating**:
- All write tools mutate vault registry; they share the same trust posture. Per-tool gating fragments config without ergonomic gain.
- Default-on matches the round-2 trust posture (localhost-only daemon by default; agents already trusted to invoke search tools that read every file in every vault).
- Future write tools inherit the same gate. No config-key-explosion across rounds.
- Operators wanting strict opt-out get a single-line config edit (`[mcp] enable_write_tools = false`).

Both stdio MCP (the `hmn mcp` subcommand, [ADR-0012](../decisions/0012-mcp-transport-stdio-v0.md)) and HTTP MCP (the `/mcp` endpoint on `hmnd`, [ADR-0013](../decisions/0013-mcp-transport-streamable-http.md)) serve this same tool surface; the `[mcp] enable_write_tools` flag governs both transports identically. `vault_watch` is not shipped in v0: `rmcp` 1.5.0 does not expose a server-side API for long-lived push streaming from a tool call. MCP clients can consume live change events via the daemon's HTTP watch endpoints (`GET /vaults/{id}/watch`, `GET /events/watch`). See [change-events.md](./change-events.md#mcp-subscription) for the deferred MCP streaming design note.

### Compose-Style Declarative Layer (deferred)

An optional `<data_dir>/hmnd-compose.toml` file is contemplated as an additive feature in a future round. The reconciler would be additive: vaults listed in the file but not in state are created at startup; vaults in state but not in the file are left alone. The file does **not** destroy vaults — state remains canonical (per ADR-0010).

The file format and merging rules are pinned at the workplan that ships this layer. Step 10 / step 11 ship without it; the spec describes the surface so future workplans can pull it without canon rewrites.

---

## Data Schema

### Registry — `<data_dir>/vaults.sqlite`

Single CREATE-TABLE schema; no migrations module (per step-9 Resolution D — the registry's ergonomic shape is stable enough that schema-version-negotiation lives in a `meta` row instead of a migrations runner).

```sql
CREATE TABLE IF NOT EXISTS vaults (
    id          TEXT PRIMARY KEY NOT NULL,    -- UUIDv7 string form
    name        TEXT NOT NULL UNIQUE,
    path        TEXT NOT NULL UNIQUE,         -- absolute, canonicalized
    status      TEXT NOT NULL
                CHECK (status IN ('active', 'paused', 'errored')),
    created_at  TEXT NOT NULL,                -- ISO-8601 UTC
    last_error  TEXT
);

CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);
-- meta.schema_version = "1" at step 9; bump at any future schema change.
```

| Column | Type | Required | Notes |
|---|---|---|---|
| `id` | TEXT (UUIDv7) | yes | Stored as the canonical UUID string (e.g., `01951f6c-7c3b-7a2e-8c1d-1a2b3c4d5e6f`). Sortable lexicographically by creation time. |
| `name` | TEXT | yes | `[A-Za-z0-9_-]+`. Mutable via `rename`. Unique within the daemon. |
| `path` | TEXT | yes | Absolute, canonicalized at create-time. Unique within the daemon. |
| `status` | TEXT | yes | `active` \| `paused` \| `errored`. CHECK constraint at the schema level. |
| `created_at` | TEXT | yes | ISO-8601 UTC; set at row insert. |
| `last_error` | TEXT | no | Free-form diagnostic text; populated on `errored`-state transitions. |

### Per-Vault Layout

```
<data_dir>/vaults/<vault_id>/
  index.sqlite      # files, chunks, chunks_vec for this vault
  meta.toml         # human-readable copy of the registry row
```

`<vault_id>` is the canonical UUID string form (no `vault_` prefix in the path).

`meta.toml` is an operator-readable copy of the row, refreshed on `create` / `rename`. Reading it does not affect daemon behavior — the registry's row is canonical.

```toml
# Example meta.toml
id = "01951f6c-7c3b-7a2e-8c1d-1a2b3c4d5e6f"
name = "personal"
path = "/Users/operator/Notes"
status = "active"
created_at = "2026-04-27T18:42:11Z"
```

### Control-Plane HTTP Wire Shapes

#### `VaultRow` — common response fragment

```yaml
id: "01951f6c-7c3b-7a2e-8c1d-1a2b3c4d5e6f"
name: "personal"
path: "/Users/operator/Notes"
status: "active"             # one of active | paused | errored
created_at: "2026-04-27T18:42:11Z"
last_error: null             # string when status == errored; null otherwise
file_count: 1247             # included in list / status responses; cached from per-vault store
last_indexed_at: "2026-04-27T19:00:00Z"  # null until first indexing pass completes
```

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | string (UUIDv7) | yes | Surrogate ID. |
| `name` | string | yes | Mutable display name. |
| `path` | string | yes | Absolute, canonicalized vault path. |
| `status` | string enum | yes | `active` \| `paused` \| `errored`. |
| `created_at` | ISO-8601 UTC | yes | When the vault row was inserted. |
| `last_error` | string\|null | no | Diagnostic text; non-null only when `status == errored`. |
| `file_count` | integer | no | From per-vault `index.sqlite`'s `files` table; absent when the vault is `errored` and the index isn't readable. |
| `last_indexed_at` | ISO-8601 UTC \| null | no | Most recent successful index-pass completion; `null` for fresh vaults pre-completion. |

#### `POST /vaults` — `create`

**Step 10** ✅

Request:
```yaml
name: "personal"      # optional; defaults to default_vault_name
path: "/Users/operator/Notes"   # required
```

Response `200 OK`: single `VaultRow`.

Errors: `409 vault_path_conflict`, `409 vault_name_conflict`, `422 vault_path_invalid`.

#### `GET /vaults` — `list`

**Step 10** ✅

No request body.

Response `200 OK`:
```yaml
vaults:
  - <VaultRow>
  - <VaultRow>
```

#### `GET /vaults/{name_or_id}` — `status`

**Step 10** ✅

Response `200 OK`:
```yaml
<VaultRow>
storage_path: "<data_dir>/vaults/<id>/"
```

Errors: `404 vault_not_found` (with `closest_name_hint?` in message).

#### `POST /vaults/{name_or_id}/pause` — `pause`

**Step 11** (specified for forward-compat; ships in step 11)

No request body.

Response `200 OK`: updated `VaultRow` with `status: "paused"`.

Errors: `404 vault_not_found`.

Idempotent on already-paused: `200 OK` with the existing row.

#### `POST /vaults/{name_or_id}/resume` — `resume`

**Step 11** (specified for forward-compat; ships in step 11)

No request body.

Response `200 OK`: updated `VaultRow` with `status: "active"` and `last_error: null`.

Errors: `404 vault_not_found`, `503 vault_errored` (when `errored → active` transition is attempted but the underlying error is not resolved).

#### `POST /vaults/{name_or_id}/reset` — `reset`

**Step 11** (specified for forward-compat; ships in step 11)

Request:
```yaml
rebuild: false           # optional; default false. When true, drops and rebuilds chunks + chunks_vec.
```

Response `200 OK`: updated `VaultRow`.

Errors: `404 vault_not_found`.

#### `POST /vaults/{name_or_id}/rename` — `rename`

**Step 11** (specified for forward-compat; ships in step 11)

Request:
```yaml
new_name: "my-notes"
```

Response `200 OK`: updated `VaultRow` with the new `name`.

Errors: `404 vault_not_found`, `409 vault_name_conflict`, `422 vault_path_invalid` (if `new_name` doesn't match the name regex).

#### `POST /vaults/{name_or_id}/rescan` — `rescan`

**Step 11** (specified for forward-compat; ships in step 11)

No request body.

Response `200 OK`:
```yaml
<VaultRow>
rescan_initiated_at: "2026-04-27T19:30:00Z"
```

Errors: `404 vault_not_found`.

#### `DELETE /vaults/{name_or_id}` — `terminate`

**Step 10** ✅

No request body.

Response `200 OK`:
```yaml
terminated: true
id: "01951f6c-7c3b-7a2e-8c1d-1a2b3c4d5e6f"
```

Errors: `404 vault_not_found`.

### Search-Side Cross-References

The search and event specs cross-reference this spec for cross-vault behavior:

- [filesystem-search.md](./filesystem-search.md) — per-result `vault` (id) + `vault_name` (name); request-side `vaults?: string[]`; response-envelope `partial_results?`.
- [content-search.md](./content-search.md) — same shape.
- [semantic-search.md](./semantic-search.md) — same shape; ordering is score-desc with same-embedding-model assumption.
- [change-events.md](./change-events.md) — live-only events; per-event `vault` (id) only; **no** `vault_name`.

---

## Examples

### Fresh install with default name

```sh
$ hmn vault create ~/personal-vault
# default_vault_name = "default" (config default)
# → vault id minted as UUIDv7; row inserted with name "default"
{
  "id": "01951f6c-7c3b-7a2e-8c1d-1a2b3c4d5e6f",
  "name": "default",
  "path": "/Users/operator/personal-vault",
  "status": "active",
  "created_at": "2026-04-27T18:42:11Z",
  "last_error": null
}
```

### Named create

```sh
$ hmn vault create --name=personal ~/personal-vault
$ hmn vault create --name=work ~/work-vault
$ hmn vault list
# → both vaults active, sorted by created_at
```

### Cross-vault content search

```sh
$ hmn search content "pgvector"
# → results from all active vaults; each result carries vault + vault_name
# → ordering: global path-asc with vault-id tie-break
```

### Filtered search

```sh
$ hmn search content "pgvector" --vaults personal,work
# → results from named subset only; unknown names show in partial_results.failed
```

### Rename then continue watching events

```sh
$ hmn vault rename personal --new-name=my-notes
# → registry row's name updated; surrogate ID unchanged
# → consumer watching by surrogate ID sees no disruption
```

### Terminate then recreate with same name

```sh
$ hmn vault terminate personal --yes
# → registry row removed; per-vault subdir removed; vault path's own files untouched

$ hmn vault create --name=personal ~/different-path
# → fresh UUIDv7; fresh subdir; fresh index from cold start
```

### MCP write-tool gating

```toml
# config.toml
[mcp]
enable_write_tools = false
```

```
# In the MCP client:
$ tools/list
# → vault_list, vault_status only; vault_create / vault_terminate / etc. not advertised

$ tools/call vault_create {"name": "x", "path": "/tmp/x"}
# → structured error: write_tools_disabled
```

---

## Edge Cases

### Path collision

Path already registered under a different name: `409 vault_path_conflict`. Operator either uses the existing vault or terminates it before creating again.

### `data_dir` under any vault path

Rejected at startup-reconcile and at `create`-time with `422 vault_path_invalid`. The daemon would otherwise watch its own state files, causing infinite reindex loops.

### Vault path becomes inaccessible at runtime

Vault enters `errored` (registry's status updated; `last_error` populated with the OS error text). Other vaults continue serving. Search responses include the errored vault in `partial_results.skipped` with `status: "errored"`. Recovery: operator resolves the access issue and runs `hmn vault reset <name>` (step 11) — or `hmn vault terminate` + `hmn vault create` with a corrected path.

### Daemon crash mid-create

Per-vault subdirectory may exist without registry row, OR registry row may exist without subdirectory. Startup-reconcile drops orphan subdirs and recreates missing subdirs; the inverse case (row + missing subdir → recreate) was already handled in step 9. The orphan-subdir-without-row case is handled in the step-10 reconcile pass extension.

### Daemon crash mid-terminate

Symmetric to the above: registry row may be deleted while subdirectory remains. Startup-reconcile detects "subdirectory without registry row" and removes it.

### Concurrent operations on same vault

Serialized via the per-vault `op_lock` (step 11 ops) or the outer runner-write-lock (step 10's `create` / `terminate`). Second op waits; if both are `terminate`, the second receives `404 vault_not_found` (the first removed it).

### Concurrent operations on different vaults

Run in parallel by design. Two `create`s against different paths/names complete concurrently (they take the outer write-lock briefly but don't otherwise block each other). Two step-11 ops on different vaults take their respective per-vault `op_lock`s independently.

### Default-name collision

Existing vault with name = `default_vault_name` + new `create` without `--name` → `409 vault_name_conflict`. Operator passes `--name=<other>` or terminates the existing default vault first.

### `default_vault_name = ""` (strict explicit-naming mode)

Every command must specify a name or ID; the daemon never resolves a default. `create` without `--name` returns `422 vault_path_invalid`; `status` / `terminate` without `<target>` returns the same.

### Cross-vault path collision in search results

Two vaults may contain a file at the same vault-relative path (e.g., `notes/index.md` in both). Both rows appear in cross-vault search results; ordering breaks ties by `vault_id`. Operators wanting a single-vault answer should use `--vaults`.

### Paused vault in default search scope

Silently skipped; appears in `partial_results.skipped` with `status: "paused"`. Searching with `--vaults personal` against a paused `personal` vault produces the same skip-and-diagnose treatment.

### Errored vault in default search scope

Silently skipped; appears in `partial_results.skipped` with `status: "errored"` and `reason: "vault is errored: <last_error>"`. The `last_error` text propagates verbatim.

### Vault terminated mid-search

Race: a search request iterates active vaults, takes Arc clones, and a `terminate` runs concurrently. The Arc clone keeps the runner alive for the duration of the search; the search returns successfully. The next search after `terminate` no longer sees that vault.

### Live event stream lag

A slow `watch` subscriber may miss events if the in-memory channel overflows. The stream reports lag when the runtime can detect it; the consumer must re-query the index for current state. There is no replay in v0.

### Vault count: 0

Daemon starts cleanly with no vaults registered. Search responses return empty `results` and (when `vaults: None`) no `partial_results` (no vault was skipped or failed; the in-scope set is empty). Operators add the first vault via `hmn vault create`.

### Registry corrupt at startup

Daemon refuses to start; logs the `registry_corrupt` error. Operator restores `vaults.sqlite` from backup. Per-vault subdirectories are unchanged and can be re-registered after the registry is restored (each subdir has a `meta.toml` with the row's pre-corruption state, though the daemon currently does not auto-import from `meta.toml` — round-4+).

---

## Error Handling

| Error Condition | HTTP Status | `code` | Notes |
|---|---|---|---|
| Vault not found | 404 | `vault_not_found` | Message includes the requested name/ID and (if name) a closest-name hint computed via Levenshtein on the name list. Hint omitted if no candidate within distance 3. |
| Path already registered | 409 | `vault_path_conflict` | Message includes the existing vault's name. |
| Name already in use | 409 | `vault_name_conflict` | Message includes the existing vault's path. Also surfaces when default-name resolution collides on `create` without `--name`. |
| Path invalid (canonicalize fails, not absolute, contains `..` after canonicalization, etc.) | 422 | `vault_path_invalid` | Message describes the validation failure. |
| `data_dir` is under any vault path | 422 | `vault_path_invalid` | Spec-required edge case. |
| Vault path doesn't exist at create-time | 422 | `vault_path_invalid` | Path must exist; nonexistent paths are configuration error, not `errored` state (which is for previously-good paths that became inaccessible). |
| Vault is `errored`; op requires active state | 503 | `vault_errored` | Step 10's four ops (create/list/status/terminate) are status-agnostic. Step 11's `pause`/`resume`/`reset`/`rescan` may emit this for ops that require a non-errored vault. |
| Vault op conflict (non-blocking) | 409 | `vault_op_conflict` | Reserved for non-blocking conflict cases (e.g., `terminate` racing with an in-flight `create` where waiting would deadlock). Step 10 should rarely emit this; step 11's `pause`-during-`rescan` is the more likely surface. |
| Registry corrupt / read failure | 500 | `registry_corrupt` | Operator restores from backup; daemon refuses to serve until restored. |
| Default-name collision (auto-resolution → existing vault) | 409 | `vault_name_conflict` | Operator passes `--name` or terminates the existing default vault. |
| Search-side: `vaults` filter is empty array | 422 | `invalid_request` | Filter must be non-empty if specified; absent (`None`) is the all-active default. |
| MCP write tool invoked when gated off | — (MCP envelope) | `write_tools_disabled` | Structured error envelope returned by the tool body. Read-only tools unaffected. |

The error envelope shape `{"error": {"code": "<code>", "message": "<human text>"}}` matches the v0/round-2 shape used by the search APIs.

---

## Integration Points

### With Watcher / Indexer

Per-vault instances. Lifecycle is driven by control-plane mutations: `create` constructs and starts; `pause` / `terminate` signal shutdown via `WatcherShutdownHandle`; `resume` / `reset` reconstruct. The watcher and indexer never communicate across vaults.

### With Change Events

Per-vault and all-vault live subscriptions use the daemon's in-memory event bus. Each event carries the surrogate `vault` ID (never `vault_name`). See [change-events.md](./change-events.md).

### With Search API

Cross-vault by default; `vaults` filter restricts. Cross-vault execution semantics are pinned in this spec (§ Cross-Vault Search Semantics) and cross-referenced from each search spec to avoid duplication.

### With MCP

Same operations as HTTP, registered as tools. Read-only tools always advertised; write tools gated by `[mcp] enable_write_tools` (default `true`).

### With CLI

`hmn vault {create|list|status|pause|resume|reset|rename|rescan|terminate}` subcommands, thin shims over the HTTP control plane via `DaemonClient`. Confirmation prompts on destructive ops (`terminate`, `reset --rebuild`, `rescan`); skipped with `--yes` for non-interactive use.

### With Daemon Startup

`VaultManager::open(...)` is called once at daemon startup; it consumes the active-vault snapshot from the registry's `list_active()`, constructs a `VaultRunner` per row, and runs the reconcile pass (recreate missing subdirs; remove orphan subdirs; mark inaccessible-path rows as `errored` with `last_error` populated).

### With Configuration

`config.toml` provides `default_vault_name` (used by `create` / `status` / `terminate` when target is omitted) and `[mcp] enable_write_tools` (gates write tool advertisement). Vault definitions themselves live in runtime state, never in config (per ADR-0010). The legacy `[vault]` config block is soft-deprecated; the daemon migrates it into a registry row on first startup and warns until the operator removes the block (per step 9 Resolution C).

---

## Implementation Notes

- **Phasing**: step 10 ships `create`, `list`, `status`, `terminate` plus the cross-vault search refinements and write-tool gating. Step 11 ships `pause`, `resume`, `reset`, `rename`, `rescan` against the same wire shapes documented here.
- **`VaultManager` is the central refactor**: step 10's first wiring task replaces `ApiState.vaults: Arc<Vec<VaultEntry>>` with `ApiState.vault_manager: Arc<VaultManager>`. Search handlers call `vault_manager.active_vaults()` instead of iterating `s.vaults`. This preserves search behavior for N=1 while opening the door to dynamic vault count.
- **`vaults.sqlite` schema**: single `CREATE TABLE` plus a `meta` row for `schema_version`. No migrations module — the registry's shape is small enough that schema-version-negotiation in `meta` is sufficient, and any future change rebuilds rather than migrates (the registry is rebuildable from per-vault `meta.toml`s in principle, though the auto-import path is round-4+).
- **UUIDv7 for surrogate IDs**: time-ordered, monotonic-leaning, 128-bit. Sortable by `created_at` lexicographically (the time prefix dominates), so iterating the registry in id-order matches creation-order in practice.
- **`tokio::sync::Mutex` (not std)** is required for the per-vault `op_lock` because async ops await across the lock boundary.
- **No streaming / no pagination in round 3**: search responses are gather-then-respond. Streaming and cursor-based pagination are deferred to round 4+.

---

## Open Questions

- [ ] Pagination / cursor across N independent indexes: cursor stability under concurrent indexing, cross-vault cursor encoding, equivalence of `limit + cursor` and `limit * page`. Round 3 ships `truncated: bool` only; round-4+ design.
- [ ] Streaming response shapes (chunked HTTP / SSE / NDJSON) for high-vault-count deployments. Trigger to revisit: a deployment surfaces with N≥10 vaults or measured latency that begs for streaming.
- [ ] Compose-style declarative layer (`<data_dir>/hmnd-compose.toml`) — file format, merging rules, additive-reconciler semantics. Round-3 step 11 decides whether to ship.
- [ ] Multi-model-embedding-per-vault — relaxes the same-embedding-model assumption that semantic-search's cross-vault score-desc ordering relies on. Requires either score normalization or per-vault top-K with re-ranking.
- [ ] Cross-platform rename safety for the legacy-state migration (step-9 boundary follow-up).
- [ ] Cross-vault live event stream semantics — should `watch --all` include vaults created after subscription time or only the active set at subscription start?
- [ ] Auto-import of orphan per-vault subdirectories on registry restore: detect a subdir whose `meta.toml` has no registry row and re-register from the meta file. Round-4+; round-3 reconcile removes orphan subdirs.

---

## Revision History

| Version | Date | Changes |
|---------|------|---------|
| 0.1.0 | 2026-04-26 | Initial outline, seeded from ADR-0009 / ADR-0010 / ADR-0011. Cross-vault search semantics deliberately under-specified; round-3 workplan resolves. |
| 1.0.0 | 2026-04-27 | Fleshed from outline; commits step-10 workplan resolutions. Status: Draft → Approved. Pinned: UUIDv7 ID format; eight cross-vault search semantics resolutions; per-vault async-mutex concurrency; MCP single-flag write-tool gating; full HTTP error catalog; full operations specification (step 10 ships 4; step 11 ships 5). Removed resolved Open Questions; preserved round-4+ items. |
| 1.1.0 | 2026-04-28 | Step 12 workplan: small wording amendment to § MCP Tool Surface naming both stdio MCP and HTTP MCP transports as peers serving the same tool list. No behavioral change. |
| 1.2.0 | 2026-04-30 | Added `watch` / `vault_watch` as a live-only event subscription and removed JSONL outbox semantics from the public v0 contract. |
| 1.3.0 | 2026-04-30 | Task 16.6 rmcp verification: marked `vault_watch` as deferred in MCP tool surface table and overview; updated `watch` operation request description and overview sentence to note MCP deferral; confirmed HTTP/CLI watch is the v0 streaming surface. |
