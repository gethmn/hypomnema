# Hypomnema: Vision Document

**Version**: 0.1.0
**Date**: 2026-04-23
**Status**: Draft

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

There is no small, generic, local daemon that indexes a directory of Markdown and exposes three search shapes (filesystem / content / semantic) over both HTTP and MCP, emits a durable change-event stream for subscribers, and never writes back into the watched directory.

---

## Product Vision

Hypomnema is a local daemon that watches a directory of Markdown files and indexes them so that any consumer — most commonly an AI agent connected via MCP — can search the contents three ways:

1. **Filesystem search**: what files exist, what's in this directory, glob patterns
2. **Content search**: grep-shaped, which files contain this exact string
3. **Semantic search**: vector similarity over chunked content

It also emits change events to a durable local log, so consumers can subscribe to "this file changed" notifications without polling.

The watched directory is treated as the user's. Hypomnema reads from it. Hypomnema does not write to it. State that Hypomnema maintains (the index, the event log, configuration, logs) lives in the daemon's own data directory, never under the watched path.

### Guiding Principles

1. **Read-only over the watched directory**: the daemon never creates, modifies, or deletes files under the watched path. This eliminates conflict resolution, atomic-write dance, ownership models, and every problem space that belongs to a CRDT-based bidirectional system.
2. **Three search modes as peers**: filesystem, content, and semantic answer different question shapes. An MCP server offering only one of these is incomplete in a way that's immediately obvious watching an agent try to work.
3. **Local everything**: local embedding model, local vector store, local outbox. No cloud dependencies, no required external services. Non-negotiable for office deployments where data leaving the box may be a hard restriction; operationally simpler everywhere else.

---

## Core Concepts

### Hypomnema

From the ancient Greek ὑπόμνημα (plural *hypomnemata*) — a personal notebook of gathered material assembled from the outside world for later rereading. Marcus Aurelius's *Meditations* is the canonical example. Foucault revived the term for the practice of constituting oneself through accumulated external material. The fit to this project is near-literal: a hypomnema is a reachable substrate of things read, heard, or thought. That is exactly what this daemon builds from the user's vault.

### Watched Directory (the Vault)

The user's directory of Markdown files. Hypomnema reads from it recursively but never writes to it. Frontmatter is read but not interpreted. Wikilinks are not parsed in v0. Obsidian is a supported vault format by accident — any directory of `.md` files works.

### Indexes

Hypomnema maintains three indexes over the vault:
- **Filesystem index**: paths, sizes, mtimes
- **Content index**: the file text itself
- **Semantic index**: heading-aware chunks, embedded via a local model (nomic-embed-text / 768 dims), stored in sqlite-vec

### Outbox

An append-only JSONL event log in the daemon's data directory (`~/.local/share/hypomnema/` or platform equivalent) — never inside the watched vault. Consumers subscribe to change events by tailing this file. A constantly-growing file inside a Syncthing or Dropbox directory creates pathological sync behavior; keeping state out of the synced path is a principle that generalizes.

### Consumer

Anything that calls Hypomnema: AI agents over MCP (Iris, Claude Code), HTTP clients, the `hmn` CLI, scripts. Hypomnema has no awareness of its consumers; it exposes the same operations to all of them.

---

## Non-Goals

What Hypomnema explicitly does NOT do. These are real, planned, and preserved as design groundwork — but not in v0:

- **Writes to the vault**: No file creation, no modification, no atomic-write logic. The daemon does not write under the watched path at all.
- **The ownership model** (`vault_root` / `vault_path` distinction): Not needed when there's no write path to enforce boundaries on.
- **Format spec for bridge-managed files**: No `iris_id` / `hmn_id` frontmatter convention, no recognition of "bridge-owned" files.
- **Conflict resolution**: No three-way merge, no last-known-synced tracking, no escalation. Read-only systems don't have conflicts.
- **Multi-consumer event delivery beyond outbox tailing**: No webhooks, no in-process callbacks, no fan-out beyond "consumers tail the JSONL file."
- **Multi-directory support**: One watched directory per daemon instance.
- **Multi-instance coordination**: Each daemon is independent.
- **Obsidian-specific behavior**: Obsidian is the vault format that motivated this project, but the design assumes nothing about Obsidian. Wikilinks aren't parsed. Tags aren't indexed specially. Frontmatter isn't interpreted.
- **Bidirectional sync** (the original full vault-bridge scope): Belongs to a CRDT-based system (Hexist, AFFiNE, Anytype, Logseq in transition). Hypomnema is the smaller generic thing that fell out of asking "what would still be useful even without the bidirectional half?" — the answer was: probably enough to live as its own project.

---

## Success Criteria

v0 is done when:

- [ ] A fresh install can index a vault, serve all three search types over HTTP and MCP, and emit change events to an outbox.
- [ ] The watcher correctly handles editor saves and sync-tool writes without re-indexing unchanged files.
- [ ] A consumer (Iris or any other) can subscribe to changes by tailing the outbox and get a clean stream of real changes.
- [ ] The daemon survives a crash without corrupting its index or outbox; restart re-reconciles cleanly.
- [ ] An agent connected via MCP can perform "do I have notes on X" → "show me the directory" → "which file mentions Y" without surprises.

---

## Open Questions

Things deliberately not decided yet, to be settled in early code:

- [ ] Exact event envelope schema for the outbox. Start minimal (`{event_type, path, content_hash, detected_at}`), grow as features land.
- [ ] Configuration file format and location. TOML at `~/.config/hypomnema/config.toml` is the reasonable default; confirm during the skeleton step.
- [ ] Logging verbosity defaults. Probably `info` at the daemon level, `warn` for `notify`, `error` for `tokio`.
- [ ] Health and metrics endpoint shape. Out of scope for v0 but worth pre-allocating a `/health` route for easy expansion.
- [ ] CLI subcommand naming. `hmn start`, `hmn scan`, `hmn search`, `hmn status` is one obvious shape; could change.
- [ ] Whether the daemon should auto-rescan on startup or trust the existing index. Probably: rescan and reconcile, but make it skippable for fast restarts.

---

## Glossary

| Term | Definition |
|------|------------|
| **Hypomnema** | The daemon itself; also the Greek ancestor term — a notebook of accumulated external material for later rereading |
| **hmn** | The CLI binary name. Pronunciation of the project name: *hi-POM-nih-muh* (English) / *hoo-POM-nay-mah* (Greek) |
| **Vault** | The watched directory of Markdown files (term inherited from Obsidian; used here generically) |
| **Consumer** | Anything that calls Hypomnema's search or subscribes to its outbox (agents via MCP, HTTP clients, CLI, scripts) |
| **Outbox** | The append-only JSONL event log in the daemon's data directory |
| **Chunk** | A heading-aware slice of a Markdown file's content, embedded and stored in the semantic index |
| **sqlite-vec** | The SQLite extension used for vector storage; one file on disk, no separate process |
| **Iris** | One consumer of Hypomnema, not a dependency. Hypomnema has no Iris-specific code. |
| **Hexist** | The author's CRDT-based system that, when it arrives, will handle the bidirectional-sync problem Hypomnema deliberately does not solve |

---

## Related Documents

- [Architecture Overview](../architecture/overview.md) — how the containers fit together
- [Key Decisions](../decisions/) — the load-bearing choices
- [Implementation: Tech Stack](../implementation/tech-stack.md) — crate list and v0 step plan
- [Project Handoff](../hypomnema-handoff.md) — full origin story and design-space context
