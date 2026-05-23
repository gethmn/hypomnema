# Hypomnema: Vision Document

**Version**: 0.3.0
**Date**: 2026-05-03
**Status**: Living vision; original v0 achieved

---

## Problem Statement

### The Scenario

An author keeps a growing directory of Markdown notes — observations, quotations, design explorations, meeting notes, reading summaries. AI agents and other tools would like to search this corpus to answer questions, pull quotes, and notice change. The agents need three distinct search shapes: *do I have notes on X?* (filesystem), *which file mentions Y exactly?* (content), *what in this vault is conceptually similar to Z?* (semantic).

Building this retrieval layer separately inside every consumer — every agent, every script, every skill — duplicates work and produces inconsistent quality. Equally, coupling the retrieval to any one consumer's internal format (Obsidian plugins, Iris's ContextRetriever, a one-off grep pipeline) means the retrieval doesn't travel.

### Why Existing Solutions Fall Short

| Solution | Limitation |
|----------|------------|
| Obsidian plugins | Tie retrieval to one editor; no headless access for agents or scripts |
| Per-consumer pipelines (Iris ContextRetriever, ad-hoc rag tools) | Each consumer rebuilds the same chunk/embed/search stack; quality drift |
| Full bidirectional sync (CRDT-backed PKBs) | Solves a much harder problem (Hexist, AFFiNE, Anytype); not needed for read-only search |
| Obsidian local REST API + generic MCP wrappers | Fails in recognizable ways — no semantic layer, noisy under sync-tool writes |

### The Gap

There was no small, generic, local service that makes a directory of notes searchable through three shapes (filesystem / content / semantic) over both HTTP and MCP, emits live change notifications for active subscribers, and never writes back into the watched directory. Hypomnema fills that gap for Markdown notes today. Future format and metadata work is tracked as current product boundaries rather than as v0 blockers.

---

## Product Vision

Hypomnema makes directories of notes searchable and reachable to any consumer — most commonly an AI agent connected via MCP — running locally on the user's machine. Internally it is a long-running local service (a daemon, `hmnd`) that watches registered vaults and maintains live indexes; the user-facing framing is the searchable substrate, not the process model. Today it indexes Markdown files; broader text-format coverage is a future product decision, not an active v0 restriction. Consumers can search the contents three ways:

1. **Filesystem search**: what files exist, what's in this directory, glob patterns
2. **Content search**: grep-shaped, which files contain this exact string
3. **Semantic search**: vector similarity over chunked content

It also emits live change events, so consumers can subscribe to "this file changed" notifications while they are connected and fall back to the index as the source of truth when they need to recover current state.

The watched directory is treated as the user's. Hypomnema reads from it. Hypomnema does not write to it. State that Hypomnema maintains (the index, vault registry, configuration, logs) lives in the daemon's own data directory, never under the watched path.

### Guiding Principles

1. **Read-only over the watched directory**: the daemon never creates, modifies, or deletes files under the watched path. This eliminates conflict resolution, atomic-write dance, ownership models, and every problem space that belongs to a CRDT-based bidirectional system.
2. **Three search modes as peers**: filesystem, content, and semantic answer different question shapes. An MCP server offering only one of these is incomplete in a way that's immediately obvious watching an agent try to work.
3. **Local everything**: local embedding model, local vector store, local event delivery. No cloud dependencies, no required external services. Non-negotiable for office deployments where data leaving the box may be a hard restriction; operationally simpler everywhere else.

---

## Core Concepts

### Hypomnema

From the ancient Greek ὑπόμνημα (plural *hypomnemata*) — a personal notebook of gathered material assembled from the outside world for later rereading. Marcus Aurelius's *Meditations* is the canonical example. Foucault revived the term for the practice of constituting oneself through accumulated external material. The fit to this project is near-literal: a hypomnema is a reachable substrate of things read, heard, or thought. That is exactly what this daemon builds from the user's vault.

### Vaults

The user's directories of Markdown files. Hypomnema watches one or more vaults; each vault has a name and a generated surrogate identifier. Hypomnema reads from a vault recursively but never writes to user-authored files. Frontmatter is stored as raw content and stripped only for semantic chunking; it is not parsed into structured metadata. Wikilinks, backlinks, and tags are not indexed specially today. Obsidian vaults are implicitly supported since any directory of `.md` files works.

### Vault Lifecycle

Vaults are runtime state, not configuration. They are created, paused, resumed, reset, renamed, rescanned, and terminated via a control-plane API exposed on `hmn vault …` and over HTTP/MCP. The daemon's data directory holds the authoritative vault registry; configuration only specifies daemon-level behavior (HTTP bind, embedding endpoint, default vault name, watcher tuning, log levels). See [ADR-0010](../decisions/0010-vault-definitions-as-runtime-state.md).

### Indexes

Hypomnema maintains three indexes over the vault:
- **Filesystem index**: paths, sizes, mtimes
- **Content index**: the file text itself
- **Semantic index**: heading-aware chunks, embedded via a local model (nomic-embed-text-v1.5 / 768 dims), stored in sqlite-vec

### Change Events

Live file-change notifications emitted after the watcher/indexer confirms a real indexed change. These events are not durable and are not replayed; consumers use them as invalidation hints and re-query the index for current truth when they connect, reconnect, or detect stream loss. A future replayable stream requires an explicit event-store design with sequence numbers, stream generations, retention, and reset semantics.

### Consumer

Anything that calls Hypomnema: AI agents over MCP (Iris, Claude Code), HTTP clients, the `hmn` CLI, scripts. Hypomnema has no awareness of its consumers; it exposes the same operations to all of them.

---

## Current Product Boundaries

The original v0 gate is complete, so "not in v0" is no longer a reason to stop a discussion. Use these labels instead.

### Shipped

| Area | Current status | Where tracked |
|---|---|---|
| Filesystem search | Shipped over HTTP, CLI, and MCP | [filesystem-search.md](../specs/filesystem-search.md) |
| Content search | Shipped, including ranked FTS mode | [content-search.md](../specs/content-search.md) |
| Semantic search | Shipped with Markdown heading-aware chunking, sqlite-vec, payload budgeting, and document/chunk granularity | [semantic-search.md](../specs/semantic-search.md) |
| Content retrieval | Shipped over HTTP, CLI, and MCP | [content-retrieval.md](../specs/content-retrieval.md) |
| Multi-vault lifecycle | Shipped: create, list, status, pause, resume, reset, rename, rescan, terminate | [vault-management.md](../specs/vault-management.md) |
| Live change events | Shipped over CLI/HTTP as live-only streams | [change-events.md](../specs/change-events.md) |
| MCP request/response tools | Shipped over stdio and Streamable HTTP | [mcp-streamable-http.md](../specs/mcp-streamable-http.md) |

### Not Implemented Yet / Backlog Candidates

| Area | Current status | Where tracked |
|---|---|---|
| Structured frontmatter | Raw file content is indexed; frontmatter is not parsed into fields | Search specs open questions / future proposal |
| Tags | Inline `#tag` and `frontmatter.tags` are searchable only as raw text | Future proposal |
| Wikilinks/backlinks | Not parsed or indexed as a graph | Future proposal |
| Durable/replayable events | Not shipped; current streams are live-only | [change-events.md](../specs/change-events.md) |
| MCP `vault_watch` | Deferred pending long-lived MCP streaming design/support | [change-events.md](../specs/change-events.md#mcp-subscription) |
| Unix-socket MCP | Config parses, but no socket listener is bound | [ADR-0012](../decisions/0012-mcp-transport-stdio-v0.md) |
| Semantic indexing beyond Markdown | Not shipped; requires file discovery and chunking strategy changes | [semantic-search.md](../specs/semantic-search.md) |
| Pagination/cursors and streaming search responses | Not shipped; current responses are bounded gather-then-return | Search specs and [vault-management.md](../specs/vault-management.md) |

### Still Product Boundaries Unless Reopened

- **User-authored vault writes**: no file creation, modification, deletion, or atomic-write logic under the watched path unless a future accepted design adds a write surface.
- **Bidirectional sync**: conflict resolution, three-way merge, last-known-synced tracking, and escalation belong to a different product shape unless explicitly reopened.
- **Bridge-managed file ownership model**: no `vault_root` / `vault_path` distinction or `iris_id` / `hmn_id` convention today.
- **Multi-instance coordination**: each daemon is independent.
- **Obsidian lock-in**: Obsidian motivated the project, but Hypomnema stays generic; Obsidian-specific metadata can be discussed as additive indexing behavior, not as a project dependency.

---

## Original v0 Completion Record

The original v0 gate is complete. The implementation has also shipped several post-v0 capabilities, especially multi-vault lifecycle and content retrieval.

- [x] A fresh install can index vaults, serve all three search types over HTTP and MCP, and emit live change events to active subscribers.
- [x] The watcher handles editor saves and sync-tool writes without re-indexing unchanged files.
- [x] Consumers can run `hmn vault watch` or subscribe over HTTP and receive live real-change notifications while connected.
- [x] The daemon keeps mutable state outside watched vaults and uses SQLite transactions for index consistency.
- [x] An agent connected via MCP can perform "do I have notes on X" → "show me the directory" → "which file mentions Y" workflows.

Important evolved decisions:

- The early JSONL outbox idea is no longer the public event contract. The shipped contract is a live event bus exposed through CLI/HTTP streams; durable replay remains a separate future design.
- MCP `vault_watch` is not shipped because the current rmcp tool-call shape is request/response rather than server-push streaming.
- Multi-vault support started as post-v0 work and has already shipped.

---

## Open Questions

Live questions should be treated as normal future product work, not as v0 scope blockers:

- [ ] Should structured frontmatter become a first-class index, and if so which fields are generic versus format-specific?
- [ ] Should tags from inline `#tag` and frontmatter `tags` share one normalized tag index?
- [ ] Should wikilinks/backlinks be parsed into a graph, and should that graph be exposed through search, retrieval metadata, or a separate endpoint?
- [ ] Should semantic indexing expand beyond Markdown, and what chunking strategy applies to plain text or other formats?
- [ ] Should live events become durable/replayable, and where should the event store live?
- [ ] Should MCP grow a streaming `vault_watch` surface once the transport shape is clear?
- [ ] Should search add pagination/cursors or streaming response shapes for large multi-vault deployments?
- [ ] Should watcher filtering honor `.gitignore` / `.dockerignore`, or is explicit `ignore_patterns` enough?

---

## Glossary

| Term | Definition |
|------|------------|
| **Hypomnema** | The daemon itself; also the Greek ancestor term — a notebook of accumulated external material for later rereading |
| **hmn** | The CLI binary name. Pronunciation of the project name: *hi-POM-nih-muh* (English) / *hoo-POM-nay-mah* (Greek) |
| **Vault** | The watched directory of Markdown files (term inherited from Obsidian; used here generically) |
| **Consumer** | Anything that calls Hypomnema's search or subscribes to its live change stream (agents via MCP, HTTP clients, CLI, scripts) |
| **Change stream** | Live, non-durable event notifications emitted to connected subscribers after real indexed changes |
| **Chunk** | A heading-aware slice of a Markdown file's content, embedded and stored in the semantic index |
| **sqlite-vec** | The SQLite extension used for vector storage; one file on disk, no separate process |
| **Iris** | One consumer of Hypomnema, not a dependency. Hypomnema has no Iris-specific code. |
| **Hexist** | The author's CRDT-based system that, when it arrives, will handle the bidirectional-sync problem Hypomnema deliberately does not solve |

---

## Related Documents

- [Architecture Overview](../architecture/overview.md) — how the containers fit together
- [Key Decisions](../decisions/) — the load-bearing choices
- [Implementation: Tech Stack](../implementation/tech-stack.md) — crate list and original v0 step plan
- [Project Handoff](../hypomnema-handoff.md) — full origin story and design-space context
