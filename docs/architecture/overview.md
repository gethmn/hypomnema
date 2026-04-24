# Hypomnema: Architecture Overview

**Version**: 0.1.0
**Date**: 2026-04-23
**Status**: Draft

---

## System Context

### Overview

Hypomnema is a local daemon process. It reads one user-owned directory of Markdown files (the *vault*), maintains three indexes over that content (filesystem, content, semantic), and exposes search and subscription operations to consumers over HTTP and MCP. All state the daemon maintains — index, event log, config, logs — lives outside the watched directory in the daemon's own data directory.

### Context Diagram

```
┌─────────────────────────────────────────────────────────────────────┐
│                           User's Host                                │
│                                                                      │
│   ┌──────────────┐        ┌──────────────────┐    ┌──────────────┐  │
│   │ AI Agents    │◄──MCP─►│                  │    │ Watched      │  │
│   │ (Iris,       │        │                  │───►│ Vault        │  │
│   │  Claude Code)│        │                  │read│ (Markdown)   │  │
│   └──────────────┘        │                  │    └──────────────┘  │
│                           │   Hypomnema      │                      │
│   ┌──────────────┐        │   Daemon         │    ┌──────────────┐  │
│   │ HTTP clients,│◄─HTTP─►│   (hmn)          │    │ Daemon Data  │  │
│   │ hmn CLI      │        │                  │───►│ Dir          │  │
│   └──────────────┘        │                  │r/w │ (index +     │  │
│                           │                  │    │  outbox +    │  │
│                           │                  │    │  logs)       │  │
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
| Watched vault directory | Source of indexed content | Filesystem (read-only) |

### Consumers

Hypomnema has no awareness of its consumers. It exposes the same operations to all of them. Expected consumers:

| Consumer | Transport | Notes |
|----------|-----------|-------|
| AI agents (Iris, Claude Code, others) | MCP | Primary consumer shape |
| HTTP clients, skills.sh packages, ad-hoc scripts | HTTP | Same operations as MCP |
| `hmn` CLI | Calls the HTTP endpoint locally | Thin wrapper for humans |
| Event subscribers | Tail the JSONL outbox | No push, consumers poll the file |

See [ADR-0003](../decisions/0003-indexing-in-the-daemon.md) for why indexing happens in the daemon rather than in consumers, and [ADR-0004](../decisions/0004-three-search-modes-as-peers.md) for why all three search modes are first-class peers.

---

## Containers

### Container Diagram

```
┌────────────────────────────────────────────────────────────────────┐
│                          Hypomnema Daemon                           │
│                                                                     │
│   ┌──────────────┐        ┌──────────────┐                         │
│   │  Watcher     │───────►│  Indexer     │                         │
│   │  (notify +   │ change │  (scan, hash,│                         │
│   │  debouncer)  │ events │  chunk,      │                         │
│   └──────────────┘        │  embed)      │                         │
│                           └──────┬───────┘                         │
│                                  │                                  │
│                                  ▼                                  │
│   ┌──────────────┐        ┌──────────────┐   ┌──────────────┐      │
│   │  Search API  │◄──────►│  Store       │   │  Outbox      │      │
│   │  (Axum HTTP +│  read  │  (rusqlite + │   │  writer      │      │
│   │  rmcp MCP)   │        │  sqlite-vec) │   │  (JSONL)     │      │
│   └──────────────┘        └──────────────┘   └──────────────┘      │
│                                                                     │
└────────────────────────────────────────────────────────────────────┘
```

### Container Descriptions

| Container | Technology | Purpose |
|-----------|------------|---------|
| Watcher | `notify` + `notify-debouncer-full` | Detect Markdown file changes under the watched directory; filter out sync-conflict files; emit debounced change events |
| Indexer | pulldown-cmark, rusqlite, reqwest (to embedding service) | Walk the vault, compute content hashes, split files into heading-aware chunks, embed via local HTTP, persist to the store |
| Store | rusqlite + r2d2 + sqlite-vec | One SQLite file on disk: files table, chunks table (metadata), vec0 virtual table (embeddings). All three indexes (filesystem, content, semantic) live here. |
| Search API | Axum (HTTP) + rmcp (MCP) | Expose `search_filesystem`, `search_content`, `search_semantic` operations over two transports with identical semantics |
| Outbox writer | Plain file append | Append change events as JSONL to `~/.local/share/hypomnema/outbox.jsonl`; consumers tail the file |

---

## Key Components

### Watcher

Uses `notify` with `notify-debouncer-full` to coalesce the event storms that editors and sync tools produce around a single logical save. Events are filtered:
- Only `.md` files (no `.md.tmp`, no `.swp`, no hidden sidecars)
- Sync-conflict filenames (Syncthing `.sync-conflict-*`, Obsidian Sync conflict patterns, Dropbox conflicted copies) are dropped at the watcher, never indexed

Content-hash check: when the watcher observes a write, the indexer computes the file's content hash and compares against the stored hash. *No change in hash → no reindex, no outbox event.* This is the core defense against editor-save noise and sync-tool mtime churn. See [Pitfalls](../implementation/appendices/tech-stack/pitfalls.md) and the `.claude/skills/filesystem-watching/` skill.

### Indexer

Three responsibilities, run on each changed file:

1. **File-level**: upsert path, size, mtime, content hash into the files table
2. **Content**: the file text is stored as-is in the content index (grep-shaped queries operate on this)
3. **Semantic**: parse with pulldown-cmark, emit heading-aware chunks, embed each chunk via HTTP to the embedding service, store chunk + vector in sqlite-vec

Vec0 virtual tables do not update gracefully — the indexer *deletes and reinserts* all chunks for a file when the content hash changes. See the `.claude/skills/sqlite-vec-extension/` skill.

### Search API

All three operations are exposed identically over HTTP (Axum) and MCP (rmcp). The MCP endpoint is the expected primary interface for agents; the HTTP endpoint is the primary interface for everything else (CLI, scripts, skills).

The same SQL/vector query code backs both transports — transport is a thin layer over operations, not a fork.

### Outbox Writer

Each real change (file created, modified, deleted) produces one JSONL line in the outbox. Minimum envelope: `{event_type, path, content_hash, detected_at}`. The outbox lives in the daemon's data directory, never under the watched path — see [ADR-0006](../decisions/0006-outbox-outside-watched-directory.md).

Consumers subscribe by tailing the file. There is no push notification mechanism in v0; see the handoff's "Out of scope" for deferred fan-out work.

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
| Inbound | HTTP `/search/*` endpoints (default `127.0.0.1:7777`) | Human and script consumers |
| Inbound | MCP transport (stdio or socket, TBD) | Agent consumers |
| Outbound | Embedding service HTTP | Produce vectors for chunks and queries |
| Outbound | Outbox file (local filesystem) | Publish change events |

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

---

## Known Risks & Technical Debt

| Risk/Debt | Impact | Mitigation |
|-----------|--------|------------|
| Embedding model dimension mismatch between config and existing schema | Daemon starts on a stale index with wrong vector width; queries silently degrade or error | Bake dimension in at schema creation; fail loudly at startup if config disagrees (see `.claude/skills/sqlite-vec-extension/`) |
| Watcher event storms during sync-tool operations | Spurious reindexes; wasted CPU; sync-loop feedback | Debouncer + content-hash check + conflict-filename filter (see `.claude/skills/filesystem-watching/`) |
| Blocking the async runtime with rusqlite calls | Daemon deadlocks; search requests hang | All SQL via `spawn_blocking` without exception (see `.claude/skills/rusqlite-in-async/`) |
| Single-consumer event delivery (outbox tail) | Doesn't scale to remote consumers or push notifications | Deferred from v0; noted in handoff "Out of scope" |
| Model switching is a re-index | Migrating to a different embedding model is an operation, not a config flip | Documented; considered acceptable for v0 scope (see [ADR-0007](../decisions/0007-sqlite-vec-over-alternatives.md)) |

---

## Related Documents

- [Vision](../product/vision.md)
- [Decisions](../decisions/) — all eight ADRs are cross-referenced above
- [Specifications](../specs/) — per-search-mode and outbox specs
- [Implementation: Tech Stack](../implementation/tech-stack.md)
