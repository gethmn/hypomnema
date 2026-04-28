# Hypomnema: Architecture Overview

**Version**: 0.3.0
**Date**: 2026-04-28
**Status**: Draft

---

## System Context

### Overview

Hypomnema is a local daemon process. It reads one or more user-owned directories of Markdown files (*vaults*), maintains three indexes per vault (filesystem, content, semantic), and exposes search and subscription operations to consumers over HTTP and MCP. Vaults are runtime state — they are added, paused, and removed via a control-plane API ([ADR-0010](../decisions/0010-vault-definitions-as-runtime-state.md)) — not configured at startup. All state the daemon maintains — vault registry, per-vault indexes, per-vault event logs, daemon config, logs — lives outside the watched directories in the daemon's own data directory.

### Context Diagram

```
┌─────────────────────────────────────────────────────────────────────┐
│                           User's Host                                │
│                                                                      │
│   ┌──────────────┐        ┌──────────────────┐    ┌──────────────┐  │
│   │ AI Agents    │◄──MCP─►│                  │    │ Watched      │  │
│   │ (Iris,       │        │                  │───►│ Vaults       │  │
│   │  Claude Code)│        │                  │read│ (Markdown)   │  │
│   └──────────────┘        │                  │    └──────────────┘  │
│                           │   Hypomnema      │                      │
│   ┌──────────────┐        │   Daemon         │    ┌──────────────┐  │
│   │ HTTP clients,│◄─HTTP─►│   (hmnd)         │    │ Daemon Data  │  │
│   │ hmn CLI      │        │                  │───►│ Dir          │  │
│   └──────────────┘        │                  │r/w │ (vaults.     │  │
│                           │                  │    │  sqlite +    │  │
│                           │                  │    │  per-vault   │  │
│                           │                  │    │  state)      │  │
│                           └─────────┬────────┘    └──────────────┘  │
│                                     │                                │
│                                     ▼                                │
│                           ┌──────────────────┐                      │
│                           │ Embedding Service│                      │
│                           │ (TEI sidecar,    │                      │
│                           │  local HTTP)     │                      │
│                           └──────────────────┘                      │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

### External Dependencies

| System | Purpose | Protocol |
|--------|---------|----------|
| Embedding service (TEI / vLLM / anything OpenAI-API-shaped) | Produce 768-dim vectors for chunks and queries | HTTP (OpenAI-compatible) |
| sqlite-vec extension | Vector similarity search via SQLite | Dynamic library loaded in-process |
| Watched vault directories | Source of indexed content; one or more per daemon | Filesystem (read-only) |

### Consumers

Hypomnema has no awareness of its consumers. It exposes the same operations to all of them. Expected consumers:

| Consumer | Transport | Notes |
|----------|-----------|-------|
| AI agents (Iris, Claude Code, others) | MCP via stdio (`hmn mcp`) or HTTP (`/mcp` on `hmnd`) | Primary consumer shape. Two MCP transports ship: stdio via the `hmn mcp` CLI subcommand (a thin shim that translates tool calls into HTTP requests against `hmnd`) and Streamable HTTP at `/mcp` on `hmnd`'s existing Axum router (round 4) — the natural fit for browser-hosted hosts and remote-MCP scenarios where spawning `hmn mcp` is not an option. The deferred Unix-socket transport will join HTTP-MCP on `hmnd` when it ships. Both shipped transports serve the same twelve tools (3 search + 9 vault-management). See [ADR-0008 § Amendments](../decisions/0008-two-binary-daemon-plus-cli.md#amendments), [ADR-0012](../decisions/0012-mcp-transport-stdio-v0.md), and [ADR-0013](../decisions/0013-mcp-transport-streamable-http.md). Step 11 completed the vault tool surface with all nine ops: `vault_list` / `vault_status` (read) + `vault_create` / `vault_pause` / `vault_resume` / `vault_reset` / `vault_rename` / `vault_rescan` / `vault_terminate` (write, gated by `[mcp] enable_write_tools`), alongside the three search tools — twelve tools total. |
| HTTP clients, skills.sh packages, ad-hoc scripts | HTTP | Same operations as MCP |
| `hmn` CLI | Calls the HTTP endpoint locally; also serves the MCP-over-stdio shim via `hmn mcp` | Thin wrapper for humans (search, status, **and vault management** — [ADR-0011](../decisions/0011-vault-management-on-hmn.md)) and for agent hosts (the MCP shim) |
| Event subscribers | Tail the per-vault JSONL outbox | No push, consumers poll the file |

See [ADR-0003](../decisions/0003-indexing-in-the-daemon.md) for why indexing happens in the daemon rather than in consumers, and [ADR-0004](../decisions/0004-three-search-modes-as-peers.md) for why all three search modes are first-class peers. See [ADR-0009](../decisions/0009-multi-vault-per-daemon.md) for multi-vault per daemon.

---

## Containers

### Container Diagram

```
┌────────────────────────────────────────────────────────────────────┐
│                          Hypomnema Daemon                           │
│                                                                     │
│   ┌──────────────┐         ┌──────────────┐                        │
│   │  Vault       │◄───────►│ Control-plane│                        │
│   │  Registry    │ mutate  │ API          │                        │
│   │  (vaults.    │         │ (HTTP)       │                        │
│   │   sqlite)    │         └──────────────┘                        │
│   └──────┬───────┘                                                 │
│          │ drives lifecycle                                         │
│          ▼                                                          │
│   ┌──────────────┐        ┌──────────────┐    ┌──────────────┐     │
│   │  Watcher *   │───────►│  Indexer *   │    │  Outbox *    │     │
│   │  (notify +   │ change │  (scan, hash,│    │  writer      │     │
│   │  debouncer)  │ events │  chunk,      │    │  (JSONL,     │     │
│   └──────────────┘        │  embed)      │    │  per vault)  │     │
│                           └──────┬───────┘    └──────────────┘     │
│                                  │                                  │
│                                  ▼                                  │
│   ┌──────────────┐        ┌──────────────┐                         │
│   │  Search API  │◄──────►│  Store *     │                         │
│   │  (Axum HTTP) │  read  │  (rusqlite + │                         │
│   │              │ x-vault│  sqlite-vec, │                         │
│   │              │ fan-out│  per vault)  │                         │
│   └──────▲───────┘        └──────────────┘                         │
│          │                                                          │
│   * = one instance per active vault                                 │
│          │                                                          │
└──────────┼──────────────────────────────────────────────────────────┘
           │ HTTP (loopback)
           │
       ┌───┴────────┐
       │ hmn mcp    │  rmcp MCP server over stdio (per-session, spawned
       │ (CLI shim) │  by the agent host: Claude Code, Iris). Forwards
       └────────────┘  every tool call to hmnd over loopback HTTP. The
                       deferred Unix-socket MCP transport will live in
                       hmnd; see ADR-0012.
```

### Container Descriptions

| Container | Technology | Purpose |
|-----------|------------|---------|
| Vault Registry | rusqlite | Authoritative list of registered vaults: id, name, path, status, created_at, last_error. Lives at `<data_dir>/vaults.sqlite`. Daemon reconciles on startup, including an orphan-subdir cleanup pass under `<data_dir>/vaults/`. See [ADR-0010](../decisions/0010-vault-definitions-as-runtime-state.md). |
| Vault Manager / Control-plane API | Axum (HTTP) + rmcp (MCP) | Expose vault lifecycle operations. Per-vault async `op_lock` serializes ops on the same vault; ops on different vaults run in parallel. Step 11 ships the full nine-operation surface — `create` / `list` / `status` / `pause` / `resume` / `reset` / `rename` / `rescan` / `terminate` — over both transports. MCP write tools are gated by `[mcp] enable_write_tools`. See [ADR-0011](../decisions/0011-vault-management-on-hmn.md) and [`docs/specs/vault-management.md`](../specs/vault-management.md). |
| Watcher (per vault) | `notify` + `notify-debouncer-full` | Detect Markdown file changes under one watched directory; filter out sync-conflict files; emit debounced change events. One instance per active vault. |
| Indexer (per vault) | pulldown-cmark, rusqlite, reqwest (to embedding service) | Walk one vault, compute content hashes, split files into heading-aware chunks, embed via local HTTP, persist to that vault's store. One instance per active vault. |
| Store (per vault) | rusqlite + r2d2 + sqlite-vec | One SQLite file per vault at `<data_dir>/vaults/<vault_id>/index.sqlite`: files table, chunks table (metadata), vec0 virtual table (embeddings). All three indexes (filesystem, content, semantic) per vault live here. |
| Search API | Axum (HTTP) in `hmnd`; rmcp (MCP) via `hmn mcp` stdio shim **and** the `/mcp` route on `hmnd`'s Axum router (round 4) | Expose `search_filesystem`, `search_content`, `search_semantic` over three transports with identical semantics. `hmnd` binds HTTP and now also serves Streamable-HTTP MCP at `/mcp` (in-process tool execution via the shared `HypomnemaBackend` trait — no DaemonClient HTTP shim); `hmn mcp` continues to serve stdio MCP as a per-session shim that forwards to `hmnd` over HTTP. The deferred Unix-socket MCP transport will join HTTP-MCP on `hmnd` when it ships — see [ADR-0012](../decisions/0012-mcp-transport-stdio-v0.md) and [ADR-0013](../decisions/0013-mcp-transport-streamable-http.md). Cross-vault fan-out by default. |
| Outbox writer (per vault) | Plain file append | Append change events as JSONL to `<data_dir>/vaults/<vault_id>/outbox.jsonl`; consumers tail the per-vault file. One instance per active vault. |

---

## Key Components

### Vault Registry

The registry is the daemon's authoritative list of vaults. Schema (illustrative; finalized in [`docs/specs/vault-management.md`](../specs/vault-management.md)):

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

The registry lives at `<data_dir>/vaults.sqlite` (a top-level SQLite file alongside the per-vault subdirectories). On startup, the daemon reads the registry, verifies each vault's path exists and `<data_dir>/vaults/<id>/` is present, and starts a watcher + indexer pair for each `active` vault. Vaults whose path is missing transition to `errored` with a recorded `last_error`; the daemon stays running and other vaults continue to serve.

Control-plane mutations against the registry are serialized per-vault (operations on different vaults run in parallel — see § Vault Manager / Control Plane below for the per-vault async-mutex shape). Atomic write semantics on `vaults.sqlite` ensure crash safety; a corrupt registry causes the daemon to refuse serving until the file is restored.

See [ADR-0009](../decisions/0009-multi-vault-per-daemon.md) and [ADR-0010](../decisions/0010-vault-definitions-as-runtime-state.md).

### Vault Manager / Control Plane

The Vault Manager owns the runtime state behind the registry: a map of active vault entries (each holding the watcher + indexer + outbox handles, the per-vault store handle, a `tokio::sync::watch` shutdown sender, a `tokio::sync::watch::Sender<u64>` rescan trigger, and the per-vault `op_lock`) plus the registry connection itself. The daemon's `ApiState` holds a single `Arc<VaultManager>`; every HTTP handler that touches vault state goes through this Arc.

**Per-vault `op_lock` shape.** Each `VaultRunner` carries a `tokio::sync::Mutex` keyed by surrogate ID — the per-vault `op_lock` introduced in step 10 and acquired for the first time in step 11 by the lifecycle ops. Operations against the same vault (e.g. two concurrent `pause` requests for the same name, or `pause` + `terminate` racing) serialize on this lock. Operations against different vaults run in parallel — there is no daemon-wide lock on control-plane mutations. Search handlers take an `Arc` clone of the vault's runner before going async, so an in-flight search is never blocked by a `terminate` and never aborted by one (the Arc keeps the runner alive for the duration of the search; the next search after `terminate` no longer sees the vault). Acquisition pattern: clone the runner `Arc` from the runner-map under a short read-lock window, then `.op_lock.lock().await` outside the runner-map lock — control-plane ops never hold the runner-map write-lock across `await`.

**Interior-mutability shape on `VaultEntry`.** `VaultRunner.entry` is `std::sync::RwLock<Arc<VaultEntry>>`; readers (search, status, list) clone the inner `Arc<VaultEntry>` under a short read-lock and then operate on the snapshot. Writers (`rename`, `resume`, `reset`) `replace_entry(Arc<VaultEntry>)` with a freshly built entry under the same lock — readers in flight at the moment of the swap see the previous `Arc` for the rest of their query. This is what lets `rename` change the user-facing name without disturbing in-flight searches and lets `resume` install a fresh lifecycle into a runner whose error state is being cleared.

**Rescan signal channel.** Each runner's lifecycle exposes a `tokio::sync::watch::Sender<u64>` (mirroring the existing shutdown-channel pattern). `rescan` signals it via `send_modify(|v| *v = v.wrapping_add(1))`; the watcher's consumer task selects on the rescan receiver alongside the shutdown receiver and the change-event channel. On every rescan tick the watcher walks the vault directory and pushes synthetic `Modified` events through the same indexer pipeline, where `decide_upsert` decides per-file whether the `content_hash` has actually drifted before emitting an outbox event. Plain rescan on a fully-indexed vault therefore emits few or zero events; cold-start emission for every file is the `reset --rebuild` path (which clears every `files.content_hash` first via the `decide_upsert` empty-hash bypass). Rescan on a paused or errored vault is a silent no-op signal-side — the `rescan_initiated_at` is still returned to the caller, but no signal is sent.

**Lifecycle**. Step 11 ships the full nine-operation surface against this manager:

- **`create`** canonicalizes the requested path, validates it (path exists, is a directory, does not place `<data_dir>` under itself), inserts a registry row with a fresh UUIDv7 ID and the configured `default_vault_name` when no `--name` was supplied, creates `<data_dir>/vaults/<id>/`, opens the per-vault `index.sqlite`, writes `meta.toml`, and spawns the watcher + indexer + outbox writer. Errors map to the spec's HTTP envelope catalog (`vault_path_conflict` 409, `vault_name_conflict` 409, `vault_path_invalid` 422).
- **`pause`** acquires the `op_lock`, transitions `active → paused`, drains the lifecycle (cooperative shutdown signalled via the watch channel; 30s `LIFECYCLE_DRAIN_TIMEOUT` cap), and leaves the runner in the map with `lifecycle = None`. Search handlers see the runner under a `paused` status and report it under `partial_results.skipped`. Idempotent on already-paused.
- **`resume`** acquires the `op_lock`, transitions `paused → active` or `errored → active` (the latter only when the underlying error is no longer present — e.g. the vault path is reachable again), and installs a fresh lifecycle into the existing runner via `spawn_runner_parts`. Dual-path: a runner may be in the map (paused case) or absent (errored-at-startup case where reconcile skipped it); the absent-runner path validates path-accessible, builds the runner-pair, and inserts under the runner-map write-lock.
- **`reset`** acquires the `op_lock`, clears `last_error`, drains the lifecycle, and re-spawns. With `rebuild: true`, additionally runs the rebuild SQL transactionally between drain and re-spawn (`DELETE FROM chunks_vec; DELETE FROM chunks; UPDATE files SET content_hash = ''`); the fresh runner that comes up after re-spawn is what re-embeds against the cleared content hashes. Same dual-path as `resume` for the errored-at-startup case.
- **`rename`** acquires the `op_lock`, runs a single `UPDATE vaults SET name = ?` against the registry, rewrites `meta.toml` (using the same `legacy_state_migration::write_meta_toml` helper the create path uses), and `replace_entry`s the `VaultEntry` with one carrying the new name. The surrogate ID is unchanged; outbox events continue to carry the same `vault` ID; in-flight searches see the previous name for the rest of their query.
- **`rescan`** acquires the `op_lock` (briefly), bumps the rescan-channel counter, and returns the row + `rescan_initiated_at` immediately. The actual walk runs asynchronously inside the watcher's consumer task.
- **`terminate`** signals the per-vault `watch::Sender<bool>` to drain the runner cooperatively (30s timeout; on timeout the consumer task is force-cancelled via the `JoinHandle::abort_handle()` extracted before the timeout), removes the per-vault subdirectory under `<data_dir>/vaults/<id>/`, and deletes the registry row. The vault path's own files are never touched.

**Reconciliation on startup**. `VaultManager::open` walks the registry and starts a runner pair for each `active` row (vaults whose path is now missing or unreadable transition to `errored` with a recorded `last_error`; the daemon stays running). A second pass reconciles `<data_dir>/vaults/<id>/` directories that have no matching registry row — the orphan-subdir cleanup pass — best-effort, with `WARN`-level logging on failure. Step 9 already handled the inverse (registry row present but per-vault subdirectory missing — that case recreates the subdirectory at open time); step 10 adds the orphan-subdir half so a crash mid-`terminate` doesn't leave the daemon shipping disk space it can't address.

**Closest-name hints**. `vault_not_found` envelopes carry an optional `hint` field naming the closest existing vault by Levenshtein distance ≤ 3 over current registry names. The distance routine is inlined; no new crate dependency.

**Control-plane HTTP routes** (the full nine ops, all shipped by step 11):

- `POST /vaults` — create
- `GET /vaults` — list
- `GET /vaults/{id_or_name}` — get one
- `POST /vaults/{id_or_name}/pause` — pause
- `POST /vaults/{id_or_name}/resume` — resume
- `POST /vaults/{id_or_name}/reset` — reset (optional `{rebuild: bool}` body)
- `POST /vaults/{id_or_name}/rename` — rename (`{new_name: string}` body)
- `POST /vaults/{id_or_name}/rescan` — rescan (response carries `rescan_initiated_at`)
- `DELETE /vaults/{id_or_name}` — terminate

The same operations are exposed as MCP tools using the same handlers and wire shapes; step 11 ships the seven write tools (`vault_create` / `vault_pause` / `vault_resume` / `vault_reset` / `vault_rename` / `vault_rescan` / `vault_terminate`) and the two read tools (`vault_list` / `vault_status`), all gated as a single set by `[mcp] enable_write_tools`. The CLI surface is `hmn vault …` per [ADR-0011](../decisions/0011-vault-management-on-hmn.md).

### Watcher

Uses `notify` with `notify-debouncer-full` to coalesce the event storms that editors and sync tools produce around a single logical save. Events are filtered:
- Only `.md` files (no `.md.tmp`, no `.swp`, no hidden sidecars)
- Sync-conflict filenames (Syncthing `.sync-conflict-*`, Obsidian Sync conflict patterns, Dropbox conflicted copies) are dropped at the watcher, never indexed

Content-hash check: when the watcher observes a write, the indexer computes the file's content hash and compares against the stored hash. *No change in hash → no reindex, no outbox event.* This is the core defense against editor-save noise and sync-tool mtime churn. See [Pitfalls](../implementation/appendices/tech-stack/pitfalls.md) and the `.claude/skills/filesystem-watching/` skill.

Step 3 ships the implementation: `notify-debouncer-full` feeds a translation layer into a bounded `tokio::mpsc` channel; a consumer task drives `Scanner::reindex_path` / `Scanner::remove_path` until shutdown drains the channel.

Step 11 wires a second control input alongside the change-event channel: a `tokio::sync::watch::Sender<u64>` rescan-trigger (mirroring the existing shutdown-channel pattern). The consumer task selects on the rescan receiver, the shutdown receiver, and the change-event channel; on every rescan tick it walks the vault directory and pushes synthetic `Modified` events into the same indexer pipeline, where `decide_upsert` decides per-file whether `content_hash` has actually drifted before emitting an outbox event. The control-plane `rescan` op signals this channel via `send_modify(|v| *v = v.wrapping_add(1))`.

### Indexer

Three responsibilities, run on each changed file:

1. **File-level**: upsert path, size, mtime, content hash into the files table
2. **Content**: the file text is stored as-is in the content index (grep-shaped queries operate on this)
3. **Semantic**: split frontmatter off the top, walk the body with pulldown-cmark, emit heading-aware chunks (size targets, frontmatter handling, and heading-stack edge cases live in the `.claude/skills/markdown-chunking/` skill), embed each chunk via HTTP to the embedding service, and persist chunk metadata + vector blob into the per-vault `chunks` and `chunks_vec` tables in a single SQL transaction (delete-then-reinsert per the `.claude/skills/sqlite-vec-extension/` skill).

Vec0 virtual tables do not update gracefully — the indexer *deletes and reinserts* all chunks for a file when the content hash changes.

Step 6 shipped this path. The vec0 dimension is baked at schema-creation time (currently `768`); the *distance metric* is also schema-baked as `cosine` (migration 0004 in step 7; see [ADR-0007 § Amendments](../decisions/0007-sqlite-vec-over-alternatives.md#amendments)). A config↔schema mismatch fails the daemon at startup with a structured error. At index time, embedding-service unavailability — transport error, HTTP `5xx`, or a wrong-dimension response — skips that file's chunks, logs an `ERROR`, and leaves `files.content_hash` unadvanced so the next watcher event or rescan retries; the daemon stays responsive throughout. See [`docs/reference/configuration.md`](../reference/configuration.md#embedding) for the embedding knobs and [`docs/specs/semantic-search.md`](../specs/semantic-search.md) for the query-time surface.

### Search API

All three operations are exposed identically over three transports: HTTP `/search/*` on `hmnd`'s Axum router (CLI, scripts, skills), stdio MCP via the `hmn mcp` shim (process-per-session, agent hosts that spawn subprocesses), and HTTP MCP at `/mcp` on `hmnd`'s same Axum router (round 4 — agent hosts that prefer or require a network endpoint, including browser-hosted hosts). The MCP transports are the expected primary interface for agents; the HTTP `/search/*` endpoint is the primary interface for everything else. See [`docs/specs/mcp-streamable-http.md`](../specs/mcp-streamable-http.md) and [ADR-0013](../decisions/0013-mcp-transport-streamable-http.md) for the HTTP-MCP wire shape and trust-posture-inheritance posture.

The same SQL/vector query code backs all three transports — transport is a thin layer over operations, not a fork. The `HypomnemaBackend` trait formalizes the surface: two impls (`DaemonClient` HTTP shim used by `hmn mcp`; `InProcessBackend` direct calls used by `hmnd`'s `/mcp`) call the same handlers from different framings.

Step 5 shipped the HTTP surface: `/search/filesystem` and `/search/content` over POST, `/health` and `/status` over GET, all bound to `config.http.bind` (default `127.0.0.1:7777`). All three search responses carry per-result `vault` (surrogate ID) and `vault_name` (point-in-time display label) fields — round-3 step 9 (the per-vault internal refactor) populates both on every result. The `vault_name` field is for display ergonomics only; it never appears in the outbox (outbox events carry `vault` ID only — names rot, the durable log doesn't). `/health` is daemon-scoped. `/status` is **representative-only in step 9**: with N=1 active vault its response is byte-identical to v0.1.0 (single-vault file count + last-indexed timestamp); with N≥2 it sums file counts, takes `MAX(last_indexed_at)` across vaults, and reports the registry-list-first vault as a representative — the cross-vault `vaults: [{...}]` array shape lands in step 10. See [ADR-0009](../decisions/0009-multi-vault-per-daemon.md).

Step 7 shipped the third route: `POST /search/semantic`. The handler embeds the natural-language query via the same local embedding client the indexer uses, runs a kNN MATCH against `chunks_vec`, and returns a top-level envelope of `{ results, hint? }` per [`docs/specs/semantic-search.md`](../specs/semantic-search.md). Embedding-service failures at query time (transport error, HTTP 5xx, or a vector whose dimension disagrees with the schema) are mapped to a new error envelope code `embedding_unavailable` (HTTP 503) — the daemon never crashes due to embedding-service issues, anywhere in the runtime. See [step-6 workplan § Build-time amendment 3](../roadmap/step-06-workplan.md) for the load-bearing precedent at index time and [step-7 workplan § Resolution E](../roadmap/step-07-workplan.md#e-error-envelope-code-for-embedding-service-unavailable) for the query-time complement.

**Cross-vault search semantics (step 10).** Search handlers fan out across the active vaults (concurrently, via `tokio::join_all` over per-vault async tasks), each task taking an `Arc` clone of the target vault's runner before going async. The default scope is every `active` vault; passing a request-side `vaults: [...]` filter (or `--vaults` on the CLI) narrows the fan-out subset. Unknown names appear in the response's `partial_results.failed` array with `code: "vault_not_found"` and the search proceeds; an empty filter array returns `invalid_request`.

Per-mode merge:

- **Filesystem and content** modes concatenate per-vault result slices, then re-sort globally by `path` ascending with the surrogate vault ID as the lexicographic tie-break, then truncate at the request's `limit`.
- **Semantic** mode merges per-vault top-K candidates by score descending with the surrogate vault ID as the tie-break, then re-truncates at `limit` preserving cross-vault top-N.

Each result carries the surrogate `vault` (id) and the point-in-time `vault_name` (display label, never durable — the outbox carries `vault` only).

**Partial results** (`partial_results: { skipped: [...], failed: [...] }`) is an additive envelope field present only when at least one in-scope vault was paused, errored, or hit a runtime error mid-query — and omitted when all in-scope vaults completed successfully. Paused / errored vaults inside the default scope are reported in `skipped`; per-vault search errors that are recoverable (storage class) land in `failed`. Request-validation errors (`invalid_glob`, `invalid_regex`, `invalid_prefix`) bubble out as the top-level error envelope rather than landing in `failed`; they're cross-vault-consistent and should fail loud. Full semantics — including pagination across N independent indexes, streaming response shapes, multi-model-embedding-per-vault — are pinned in [`docs/specs/vault-management.md` § Cross-Vault Search Semantics](../specs/vault-management.md#cross-vault-search-semantics) with the deferrals noted; pagination and streaming are round-4+ candidates.

### Outbox Writer

Each real change (file created, modified, deleted) produces one JSONL line in that vault's outbox. Envelope: `{vault, event_type, path, content_hash, detected_at}` — the leading `vault` field is the surrogate ID and is present on every line as of round-3 step 9 (the per-vault internal refactor). The outbox lives in the daemon's data directory under the per-vault subdirectory (`<data_dir>/vaults/<vault_id>/outbox.jsonl`), never under the watched path — see [ADR-0006](../decisions/0006-outbox-outside-watched-directory.md) (amended 2026-04-26 to formalize the multi-vault layout).

Step 4 shipped the implementation: the watcher's consumer task — the same one that drives `Scanner::reindex_path` / `Scanner::remove_path` — emits one JSONL line per real change to the vault's outbox, with per-event `sync_data`.

Consumers subscribe by tailing the per-vault file. The outbox `vault` field carries the surrogate vault ID only — there is no `vault_name` field on outbox events (names are mutable; durable logs need stable identifiers). The peer `vault_name` field on synchronous search responses lives only in those responses, never in the outbox. There is no push notification mechanism in v0; see the handoff's "Out of scope" for deferred fan-out work.

---

## Communication Patterns

### Internal Communication

| From | To | Method | Purpose |
|------|----|--------|---------|
| Watcher | Indexer | In-process channel (tokio mpsc or similar) | Pass debounced change events |
| Indexer | Store | rusqlite calls wrapped in `spawn_blocking` | Persist file/chunk/vector rows (see `.claude/skills/rusqlite-in-async/`) |
| Indexer | Outbox writer | Direct file append | Record real changes |
| Search API | Store | rusqlite calls wrapped in `spawn_blocking` | Answer queries |

### External Communication

| Direction | Endpoint | Purpose |
|-----------|----------|---------|
| Inbound | HTTP `/health`, `/status`, `/search/filesystem`, `/search/content`, `/search/semantic` (default `127.0.0.1:7777`) | Human and script consumers |
| Inbound | HTTP `POST /vaults`, `GET /vaults`, `GET /vaults/{id}`, `POST /vaults/{id}/{pause,resume,reset,rename,rescan}`, `DELETE /vaults/{id}` | Vault lifecycle control plane (full nine-op surface as of step 11) |
| Inbound | MCP over stdio (via `hmn mcp`) | Agent consumers that spawn subprocesses (Claude Code, Iris) — search + vault-management tools, twelve total |
| Inbound | MCP over HTTP (`/mcp` on the same listener as `/search/*` and `/vaults/*`) | Agent consumers that prefer or require a network endpoint (browser-hosted hosts, remote MCP) — same twelve-tool surface as stdio MCP, in-process tool execution, Origin-validation defense against DNS rebinding |
| Inbound | MCP over Unix socket (deferred — listener on `hmnd` when shipped) | Forward-compat per ADR-0012 § Decision 2; not bound in v1 |
| Outbound | Embedding service HTTP | Produce vectors for chunks and queries |
| Outbound | Per-vault outbox files (local filesystem) | Publish change events |

---

## Cross-Cutting Concerns

### Security

Hypomnema binds to localhost only in v0. No authentication on the HTTP endpoint beyond that (local-only assumption). The daemon reads the vault; it never writes. There is no upload path, no config-endpoint mutation, no privileged operation.

### Logging & Observability

`tracing` + `tracing-subscriber`, logs to stdout and optionally a file in the daemon's data directory. Defaults: `info` at the daemon level, `warn` for `notify` (which is chatty), `error` for `tokio`. A `/health` route is pre-allocated in step 5 of the v0 plan; shape TBD.

### Error Handling

`anyhow::Result` throughout. No `thiserror` until there's a public library API to stabilize. Errors that reach the top of the search API return a JSON error body over HTTP and an MCP error over MCP; the transports handle serialization identically.

---

## Quality Attributes

| Attribute | Requirement | How Achieved |
|-----------|-------------|--------------|
| Crash safety | Restart must re-reconcile the index without corruption | Content-hash-based reconciliation on startup; SQLite's built-in durability; outbox is append-only |
| Sync-tool resilience | Running on a Syncthing / Dropbox / Obsidian Sync vault must not cause spurious reindexes or sync loops | Content-hash check before any reindex; all state outside the watched dir ([ADR-0006](../decisions/0006-outbox-outside-watched-directory.md)); conflict-filename filter |
| Local-only | No required outbound network traffic beyond the (possibly-local) embedding service | All components local-first ([ADR-0005](../decisions/0005-local-everything.md)) |
| Deployability | Two self-contained binaries plus one extension file | Rust statically-linked build ([ADR-0002](../decisions/0002-rust-over-python.md)); daemon (`hmnd`) and CLI client (`hmn`) ship together ([ADR-0008](../decisions/0008-two-binary-daemon-plus-cli.md)); sqlite-vec as a `.so`/`.dylib`/`.dll` ([ADR-0007](../decisions/0007-sqlite-vec-over-alternatives.md)) |
| Agent ergonomics | An agent can compose filesystem → content → semantic searches naturally | All three as peer MCP operations ([ADR-0004](../decisions/0004-three-search-modes-as-peers.md)) |
| Vault isolation | Cross-vault state leakage prevented; per-vault terminate is safe | Per-vault data subdirectory (`<data_dir>/vaults/<id>/`); vault-id-keyed schema; control-plane operations serialized per-vault ([ADR-0009](../decisions/0009-multi-vault-per-daemon.md), [ADR-0010](../decisions/0010-vault-definitions-as-runtime-state.md)) |

---

## Known Risks & Technical Debt

| Risk/Debt | Impact | Mitigation |
|-----------|--------|------------|
| Embedding model dimension mismatch between config and existing schema | Daemon starts on a stale index with wrong vector width; queries silently degrade or error | Bake dimension in at schema creation; fail loudly at startup if config disagrees (see `.claude/skills/sqlite-vec-extension/`) |
| Embedding service unavailable or slow at index time | Indexer stalls; naive retry hammers the service; risk of zero-vector poisoning | Explicit reqwest timeout; skip-and-retry on failure (never insert placeholder vectors); daemon remains responsive to search queries while embedding is unreachable (see [Pitfalls](../implementation/appendices/tech-stack/pitfalls.md) #10) |
| Watcher event storms during sync-tool operations | Spurious reindexes; wasted CPU; sync-loop feedback | Debouncer + content-hash check + conflict-filename filter (see `.claude/skills/filesystem-watching/`) |
| Blocking the async runtime with rusqlite calls | Daemon deadlocks; search requests hang | All SQL via `spawn_blocking` without exception (see `.claude/skills/rusqlite-in-async/`) |
| Single-consumer event delivery (outbox tail) | Doesn't scale to remote consumers or push notifications | Deferred from v0; noted in handoff "Out of scope" |
| Model switching is a re-index | Migrating to a different embedding model is an operation, not a config flip | Documented; considered acceptable for v0 scope (see [ADR-0007](../decisions/0007-sqlite-vec-over-alternatives.md)) |
| Concurrent control-plane operations on same vault | Race in registry mutations or per-vault state | Operations on the same vault are serialized at the daemon; operations on different vaults run in parallel ([ADR-0010](../decisions/0010-vault-definitions-as-runtime-state.md)) |
| Vault registry corruption | `vaults.sqlite` partial write or filesystem damage | Atomic write semantics for control-plane mutations; daemon refuses to serve on read failure until the file is restored |
| Pagination across N independent indexes | Each per-vault index has its own SQL pagination cursor; combining them across vaults requires either a global cursor (heavy) or a fan-in re-merge per page (state-ful). Step 10 ships unpaginated cross-vault search; large multi-vault result sets must use `--limit`. | Deferred to round 4+ per [`docs/specs/vault-management.md` § Open Questions](../specs/vault-management.md#open-questions); wire shape stays forward-compat (request-side cursor field is reserved). |

---

## Related Documents

- [Vision](../product/vision.md)
- [Decisions](../decisions/) — all eight ADRs are cross-referenced above
- [Specifications](../specs/) — per-search-mode and outbox specs
- [Implementation: Tech Stack](../implementation/tech-stack.md)
