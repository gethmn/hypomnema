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
| AI agents (Iris, Claude Code, others) | MCP via stdio (`hmn mcp`) | Primary consumer shape. v0 ships MCP through the `hmn mcp` CLI subcommand (a thin stdio shim that translates MCP tool calls into HTTP requests against `hmnd`); the deferred Unix-socket transport will live in `hmnd` when it ships. See [ADR-0008 § Amendments](../decisions/0008-two-binary-daemon-plus-cli.md#amendments) and [ADR-0012](../decisions/0012-mcp-transport-stdio-v0.md). v0 exposes search tools only; vault-management MCP tools land in round 3. |
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
| Vault Registry | rusqlite | Authoritative list of registered vaults: id, name, path, status, created_at, last_error. Lives at `<data_dir>/vaults.sqlite`. Daemon reconciles on startup. See [ADR-0010](../decisions/0010-vault-definitions-as-runtime-state.md). |
| Control-plane API | Axum (HTTP); MCP deferred | Expose vault lifecycle operations (create / list / status / pause / resume / reset / rename / rescan / terminate). v0 ships the HTTP surface only; vault-management MCP tools are round-3 work. See [ADR-0011](../decisions/0011-vault-management-on-hmn.md). |
| Watcher (per vault) | `notify` + `notify-debouncer-full` | Detect Markdown file changes under one watched directory; filter out sync-conflict files; emit debounced change events. One instance per active vault. |
| Indexer (per vault) | pulldown-cmark, rusqlite, reqwest (to embedding service) | Walk one vault, compute content hashes, split files into heading-aware chunks, embed via local HTTP, persist to that vault's store. One instance per active vault. |
| Store (per vault) | rusqlite + r2d2 + sqlite-vec | One SQLite file per vault at `<data_dir>/vaults/<vault_id>/index.sqlite`: files table, chunks table (metadata), vec0 virtual table (embeddings). All three indexes (filesystem, content, semantic) per vault live here. |
| Search API | Axum (HTTP) in `hmnd`; rmcp (MCP) via `hmn mcp` stdio shim | Expose `search_filesystem`, `search_content`, `search_semantic` over two transports with identical semantics. v0 binds HTTP in `hmnd` and serves MCP from `hmn mcp` (a per-session stdio shim that forwards to `hmnd` over HTTP). The deferred Unix-socket MCP transport will live in `hmnd` when it ships — see [ADR-0012](../decisions/0012-mcp-transport-stdio-v0.md). Cross-vault fan-out by default. |
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

Control-plane mutations against the registry are serialized per-vault (operations on different vaults run in parallel). Atomic write semantics on `vaults.sqlite` ensure crash safety; a corrupt registry causes the daemon to refuse serving until the file is restored.

See [ADR-0009](../decisions/0009-multi-vault-per-daemon.md) and [ADR-0010](../decisions/0010-vault-definitions-as-runtime-state.md).

### Control-plane API

Axum routes under `/vaults` mirror the operations defined in ADR-0010:

- `POST /vaults` — create
- `GET /vaults`, `GET /vaults/{id}` — list, get
- `POST /vaults/{id}/{pause|resume|reset|rename|rescan}` — lifecycle
- `DELETE /vaults/{id}` — terminate

The same operations are exposed as MCP tools (same handlers, same wire shapes). Which subset ships in which round is a workplan-time decision; spec coverage is unconditional. The CLI surface is `hmn vault …` per [ADR-0011](../decisions/0011-vault-management-on-hmn.md).

### Watcher

Uses `notify` with `notify-debouncer-full` to coalesce the event storms that editors and sync tools produce around a single logical save. Events are filtered:
- Only `.md` files (no `.md.tmp`, no `.swp`, no hidden sidecars)
- Sync-conflict filenames (Syncthing `.sync-conflict-*`, Obsidian Sync conflict patterns, Dropbox conflicted copies) are dropped at the watcher, never indexed

Content-hash check: when the watcher observes a write, the indexer computes the file's content hash and compares against the stored hash. *No change in hash → no reindex, no outbox event.* This is the core defense against editor-save noise and sync-tool mtime churn. See [Pitfalls](../implementation/appendices/tech-stack/pitfalls.md) and the `.claude/skills/filesystem-watching/` skill.

Step 3 ships the implementation: `notify-debouncer-full` feeds a translation layer into a bounded `tokio::mpsc` channel; a consumer task drives `Scanner::reindex_path` / `Scanner::remove_path` until shutdown drains the channel.

### Indexer

Three responsibilities, run on each changed file:

1. **File-level**: upsert path, size, mtime, content hash into the files table
2. **Content**: the file text is stored as-is in the content index (grep-shaped queries operate on this)
3. **Semantic**: split frontmatter off the top, walk the body with pulldown-cmark, emit heading-aware chunks (size targets, frontmatter handling, and heading-stack edge cases live in the `.claude/skills/markdown-chunking/` skill), embed each chunk via HTTP to the embedding service, and persist chunk metadata + vector blob into the per-vault `chunks` and `chunks_vec` tables in a single SQL transaction (delete-then-reinsert per the `.claude/skills/sqlite-vec-extension/` skill).

Vec0 virtual tables do not update gracefully — the indexer *deletes and reinserts* all chunks for a file when the content hash changes.

Step 6 shipped this path. The vec0 dimension is baked at schema-creation time (currently `768`); the *distance metric* is also schema-baked as `cosine` (migration 0004 in step 7; see [ADR-0007 § Amendments](../decisions/0007-sqlite-vec-over-alternatives.md#amendments)). A config↔schema mismatch fails the daemon at startup with a structured error. At index time, embedding-service unavailability — transport error, HTTP `5xx`, or a wrong-dimension response — skips that file's chunks, logs an `ERROR`, and leaves `files.content_hash` unadvanced so the next watcher event or rescan retries; the daemon stays responsive throughout. See [`docs/reference/configuration.md`](../reference/configuration.md#embedding) for the embedding knobs and [`docs/specs/semantic-search.md`](../specs/semantic-search.md) for the query-time surface.

### Search API

All three operations are exposed identically over HTTP (Axum) and MCP (rmcp). The MCP endpoint is the expected primary interface for agents; the HTTP endpoint is the primary interface for everything else (CLI, scripts, skills).

The same SQL/vector query code backs both transports — transport is a thin layer over operations, not a fork.

Step 5 shipped the HTTP surface: `/search/filesystem` and `/search/content` over POST, `/health` and `/status` over GET, all bound to `config.http.bind` (default `127.0.0.1:7777`). All three search responses carry per-result `vault` (surrogate ID) and `vault_name` (point-in-time display label) fields — round-3 step 9 (the per-vault internal refactor) populates both on every result. The `vault_name` field is for display ergonomics only; it never appears in the outbox (outbox events carry `vault` ID only — names rot, the durable log doesn't). `/health` is daemon-scoped. `/status` is **representative-only in step 9**: with N=1 active vault its response is byte-identical to v0.1.0 (single-vault file count + last-indexed timestamp); with N≥2 it sums file counts, takes `MAX(last_indexed_at)` across vaults, and reports the registry-list-first vault as a representative — the cross-vault `vaults: [{...}]` array shape lands in step 10. See [ADR-0009](../decisions/0009-multi-vault-per-daemon.md).

Step 7 shipped the third route: `POST /search/semantic`. The handler embeds the natural-language query via the same local embedding client the indexer uses, runs a kNN MATCH against `chunks_vec`, and returns a top-level envelope of `{ results, hint? }` per [`docs/specs/semantic-search.md`](../specs/semantic-search.md). Embedding-service failures at query time (transport error, HTTP 5xx, or a vector whose dimension disagrees with the schema) are mapped to a new error envelope code `embedding_unavailable` (HTTP 503) — the daemon never crashes due to embedding-service issues, anywhere in the runtime. See [step-6 workplan § Build-time amendment 3](../roadmap/step-06-workplan.md) for the load-bearing precedent at index time and [step-7 workplan § Resolution E](../roadmap/step-07-workplan.md#e-error-envelope-code-for-embedding-service-unavailable) for the query-time complement.

**Cross-vault search semantics (step-9 internal pre-staging)**. Search handlers iterate `registry.list_active()` and execute each per-vault query against that vault's `Store`. Filesystem and content modes concatenate per-vault result slices in **registry-list order** and apply the request's `limit` to the concatenated list. Semantic mode merges per-vault top-K candidates by score (descending) and re-truncates at `limit`, preserving cross-vault top-N. For N=1 — the only state reachable from a single legacy `[vault]` migration — the wire output is byte-identical to v0.1.0 modulo the populated `vault` and `vault_name` fields. The full cross-vault semantics — ordering across vaults beyond registry-list order, pagination across N independent indexes, fan-out execution model, partial-failure handling, treatment of paused/errored vaults, semantic global top-N — are pinned in step 10's workplan-write phase.

**Spec-text amendment forward-pointer**. The four search and event spec docs ([`docs/specs/filesystem-search.md`](../specs/filesystem-search.md), [`docs/specs/content-search.md`](../specs/content-search.md), [`docs/specs/semantic-search.md`](../specs/semantic-search.md), and [`docs/specs/change-events.md`](../specs/change-events.md)) still describe the v0.1.0 "always absent in v0" wording for the per-result `vault` and `vault_name` fields — step 9 ships ahead of those specs. The amendments land in step 10's workplan-write phase; until then, the integration tests in `tests/multi_vault_internal.rs` are the authoritative source of truth for the populated wire shape.

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
| Inbound | HTTP `POST /vaults`, `GET /vaults`, `GET /vaults/{id}`, `POST /vaults/{id}/{op}`, `DELETE /vaults/{id}` | Vault lifecycle control plane |
| Inbound | MCP transport (stdio or socket, TBD) | Agent consumers (search + vault management) |
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
| Cross-vault search semantics partially specified | Step 9 pins minimal cross-vault shape (filesystem/content concatenate by registry-list order; semantic merges by score and re-truncates). Full semantics — ordering beyond registry-list, pagination, fan-out execution, partial-failure, paused/errored treatment, semantic global top-N — not yet pinned | Wire shapes are forward-compat; full resolution lands in step 10's workplan-write phase ([ADR-0009](../decisions/0009-multi-vault-per-daemon.md), [`docs/specs/vault-management.md` § Open Questions](../specs/vault-management.md#open-questions)) |

---

## Related Documents

- [Vision](../product/vision.md)
- [Decisions](../decisions/) — all eight ADRs are cross-referenced above
- [Specifications](../specs/) — per-search-mode and outbox specs
- [Implementation: Tech Stack](../implementation/tech-stack.md)
