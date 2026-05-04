# Implementation: Hypomnema Technology Stack

**Version**: 0.2.0
**Date**: 2026-04-26
**Status**: Draft

---

> **File Location**: `docs/implementation/tech-stack.md`
>
> Appendix content lives in `docs/implementation/appendices/tech-stack/`.

---

This document defines the technology stack for Hypomnema and orders the v0 implementation work.

---

## Stack Overview

Hypomnema is a Rust project that ships two binaries from one crate: `hmnd` (the long-running daemon) and `hmn` (a thin CLI client that talks to a running `hmnd`). Both are built from the single `hypomnema` crate with shared code in `src/lib.rs`. The runtime is Tokio; the HTTP server is Axum; the MCP transport is the official `rmcp` crate; persistence is `rusqlite` with sqlite-vec statically linked into the daemon and registered at startup. Filesystem watching is `notify` + `notify-debouncer-full`. Markdown parsing is `pulldown-cmark`.

See [ADR-0002: Rust over Python](../decisions/0002-rust-over-python.md) for the language choice, [ADR-0008: Two Binaries (hmnd + hmn) in One Crate](../decisions/0008-two-binary-daemon-plus-cli.md) for the binary shape, and the ADRs they reference for the major library choices.

### Core Dependencies

```toml
# Cargo.toml (representative — actual versions pinned at step 1)

[package]
name = "hypomnema"
version = "0.1.0"
edition = "2024"

[lib]
# src/lib.rs — shared code for both binaries

[[bin]]
name = "hmnd"                 # daemon: watcher, HTTP + MCP servers, indexing
path = "src/bin/hmnd.rs"

[[bin]]
name = "hmn"                  # CLI client: speaks HTTP to a running hmnd
path = "src/bin/hmn.rs"

[dependencies]
# async runtime
tokio = { version = "1", features = ["full"] }

# HTTP server
axum = "0.7"

# MCP (official Rust SDK)
rmcp = "*"   # pin at step 8

# SQLite + sqlite-vec (statically linked)
rusqlite = { version = "0.31", features = ["bundled"] }
sqlite-vec = "=0.1.10-alpha.3"
r2d2 = "0.8"
r2d2_sqlite = "0.24"

# filesystem watching
notify = "6"
notify-debouncer-full = "0.3"

# Markdown parsing
pulldown-cmark = "0.10"

# HTTP client (talking to embedding service)
reqwest = { version = "0.12", features = ["json"] }

# serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"

# logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# error handling
anyhow = "1"

# CLI
clap = { version = "4", features = ["derive"] }
```

### Future Dependencies (Add When Needed)

```toml
# Added when a real second implementation demands abstraction
# async-trait = "…"

# Added when the crate gains a public library API
# thiserror = "…"
```

---

## Dependency Rationale

### Runtime and Transports

| Package | Purpose |
|---------|---------|
| **tokio** | Async runtime. Drives the HTTP server, the MCP transport, the watcher event loop, and the embedding HTTP client |
| **axum** | HTTP server framework. Small, tokio-native, natural fit for the search endpoints |
| **rmcp** | Official Rust MCP SDK. Its maturity is the specific factor that removed the main objection to Rust (per [ADR-0002](../decisions/0002-rust-over-python.md)) |

### Storage

| Package | Purpose |
|---------|---------|
| **rusqlite** (`bundled`) | Blocking SQLite access. `bundled` ships SQLite inside the binary. |
| **sqlite-vec** (Rust crate, static extension build) | Vector store. The crate compiles the sqlite-vec C amalgamation into the binary and exposes `sqlite3_vec_init`; the daemon registers it via `sqlite3_auto_extension` at process startup so every connection in the pool sees the extension automatically. See [ADR-0007](../decisions/0007-sqlite-vec-over-alternatives.md) (and its 2026-05-03 Amendment) and `.claude/skills/sqlite-vec-extension/`. |
| **r2d2** + **r2d2_sqlite** | Blocking connection pool. Connections are checked out inside `spawn_blocking` so the async runtime never blocks on SQL. See `.claude/skills/rusqlite-in-async/`. |

### Watching and Parsing

| Package | Purpose |
|---------|---------|
| **notify** + **notify-debouncer-full** | Filesystem watching with event coalescing. Never roll our own — editor saves and sync-tool operations produce event storms the debouncer is built to handle. See `.claude/skills/filesystem-watching/` |
| **pulldown-cmark** | Event-driven Markdown parser. Used for heading-aware chunking, not for rendering. See `.claude/skills/markdown-chunking/` |

### HTTP Client

| Package | Purpose |
|---------|---------|
| **reqwest** | HTTP client for the embedding service (TEI / vLLM / any OpenAI-compatible endpoint) |

### Serialization, Logging, Errors, CLI

| Package | Purpose |
|---------|---------|
| **serde** / **serde_json** / **toml** | Config file + event envelopes + wire formats |
| **tracing** + **tracing-subscriber** | Structured logging with per-module level filters |
| **anyhow** | Error handling. No `thiserror` until there's a public library API to stabilize errors for. |
| **clap** (`derive`) | Argument parsing for both binaries — `hmnd`'s daemon flags and `hmn`'s client subcommands |

### Intentionally Excluded

| Package | Reason |
|---------|--------|
| **thiserror** | Until there's a public library API, `anyhow::Result` everywhere is fine. |
| **async-trait / tower abstractions over embedding, vector store, transport** | Pick one of each concretely, build against it, refactor when a *second* real use case demands an abstraction. Premature abstraction in a v0 daemon is a trap. |
| **Workspace split into multiple crates** | Single crate with `lib` + two `bin` targets (`hmnd` and `hmn`). Two binaries in one crate is not a workspace — it keeps the shared library next to both consumers. Split into a workspace only when a second consumer demands reuse of the library as its own crate. See [ADR-0008](../decisions/0008-two-binary-daemon-plus-cli.md). |

These exclusions are explicit v0 scope boundaries, not permanent prohibitions. See the "Out of scope" section of [hypomnema-handoff.md](../hypomnema-handoff.md) for the full list.

---

## Architecture Patterns

### spawn_blocking for rusqlite

Every SQL call goes through `tokio::task::spawn_blocking`. No exceptions. The async runtime has a small fixed thread pool; blocking it on SQLite I/O deadlocks the daemon under load. Captured in detail in `.claude/skills/rusqlite-in-async/`.

### Content-hash gating

The watcher fires on any filesystem event under the watched directory. The indexer computes the file's content hash and compares against the stored hash. No change → no reindex, no live change event. This is the primary defense against editor-save noise and sync-tool mtime churn. Captured in `.claude/skills/filesystem-watching/`.

### Delete-and-reinsert for vec0

sqlite-vec's vec0 virtual table does not update rows gracefully. When a file's content hash changes, the indexer deletes all chunks and their vectors for that file and reinserts the new set. Captured in `.claude/skills/sqlite-vec-extension/`.

### Heading-aware Markdown chunking via pulldown-cmark events

Regex or blank-line-based chunk boundaries miss the semantic structure of prose. pulldown-cmark's event stream makes heading boundaries explicit. Captured in `.claude/skills/markdown-chunking/`.

---

## Project Structure

```
hypomnema/
├── Cargo.toml
├── Cargo.lock
├── src/
│   ├── lib.rs           # shared library: config, types, HTTP client, server machinery
│   ├── bin/
│   │   ├── hmnd.rs      # daemon entry: watcher, HTTP + MCP servers, indexing workers
│   │   └── hmn.rs       # CLI client entry: speaks HTTP to a running hmnd
│   ├── config.rs        # TOML config load + validation
│   ├── store/           # rusqlite + sqlite-vec (hmnd only)
│   │   ├── mod.rs
│   │   ├── schema.rs    # migrations; vec0 dimension baked in
│   │   └── pool.rs      # r2d2 pool, WAL + synchronous=NORMAL pragmas
│   ├── vault_registry/  # vaults.sqlite + reconciliation (hmnd only; round 3+)
│   ├── control_plane/   # vault lifecycle handlers — HTTP + CLI + MCP (round 3+)
│   ├── watcher/         # notify + debouncer + conflict filter (hmnd only)
│   ├── indexer/         # scan, hash, chunk, embed, persist (hmnd only)
│   ├── search/          # filesystem / content / semantic query impls (hmnd only)
│   ├── api/
│   │   ├── http.rs      # axum routes (hmnd)
│   │   └── mcp.rs       # rmcp server (hmnd)
│   ├── client.rs        # typed reqwest client against the daemon (used by hmn)
│   ├── events.rs        # live change event bus (hmnd only)
│   └── embedding.rs     # reqwest client to the embedding service (hmnd only)
├── tests/               # integration tests
└── docs/                # this documentation
```

No `src/main.rs` — both entry points live under `src/bin/`. Modules tagged "hmnd only" above are linked into the `hmnd` binary via `lib.rs` but are unreachable at runtime from `hmn`; the distinction is organizational, not a build-time boundary (both binaries link the full library — see [ADR-0008](../decisions/0008-two-binary-daemon-plus-cli.md)).

---

## Implementation Priority

Eight steps, dependency-ordered, each independently useful as a stopping point. Each is a unit of work that can ship; if the next step proves hard, the previous one remains useful on its own.

### Phase 1: Skeleton & Scan

1. **Skeleton** — Daemon starts, reads config, logs what it's watching, exits cleanly on SIGINT.
2. **Scan + hash** — Walk the directory, compute content hashes, store in SQLite. Re-runs are deterministic.
3. **Watcher** — `notify` + `notify-debouncer-full`, filtered to `.md` files, with the content-hash check that distinguishes "OS noticed a write" from "content changed."

### Phase 2: Events & Search

4. **Live change events** — Publish real change events to connected subscribers.
5. **Filesystem and content search over HTTP** — List/glob and grep, exposed via Axum. CLI built against these. **Useful enough to dogfood for ordinary find/grep work.**

### Phase 3: Semantic & MCP

6. **Chunking and embedding** — pulldown-cmark heading-aware chunking, embed via TEI, store in sqlite-vec. The step most likely to surprise you.
7. **Semantic search** — Query → embed → vector search → return chunks with metadata.
8. **MCP wrapper** — Same operations, MCP transport via `rmcp`. Test against an actual agent (Claude Code, Iris).

If a step is hard, ship the previous one and keep using it. Step 5 (filesystem + content over HTTP) is the natural early shipping gate — it is genuinely valuable on its own, and reaching it validates the whole indexing-and-watch skeleton.

> **Multi-vault support** (per [ADR-0009](../decisions/0009-multi-vault-per-daemon.md), [ADR-0010](../decisions/0010-vault-definitions-as-runtime-state.md), [ADR-0011](../decisions/0011-vault-management-on-hmn.md)) is **post-v0**; it lands as **round 3** of the roadmap. The eight steps above remain single-vault. Round 3 introduces `src/vault_registry/` and `src/control_plane/`, refactors the watcher / indexer / store / event-stream modules to be per-vault-aware, and ships the vault-management spec (`docs/specs/vault-management.md`).

---

## Testing Strategy

### Unit Tests

- Pure functions get unit tests (chunker, config validator, conflict-filename filter)
- Unit tests avoid the filesystem and the database; use `tempfile` + `:memory:` when they can't

### Integration Tests

- Each step of the implementation plan has at least one integration test that exercises the end-to-end flow for that step
- Integration tests use a real temp directory, a real SQLite file, and a stubbed embedding endpoint
- The watcher tests run against simulated editor saves and simulated sync-tool writes to verify the content-hash gate works

### CI

Tests run under `cargo-nextest` locally and in CI. The GitHub Actions pipeline (`.github/workflows/ci.yml`) runs `fmt`, `clippy`, and `nextest` on every push to `main` and every PR, across Ubuntu and macOS. See [`docs/specs/ci-pipeline.md`](../specs/ci-pipeline.md) for the full pipeline specification.

---

## Pitfalls

The handoff names eight pitfalls that agents writing Hypomnema code need to know. Most are captured in the `.claude/skills/` directory as loadable skills; a full catalog lives in the appendix:

- [Pitfalls catalog](./appendices/tech-stack/pitfalls.md)

---

## Appendices

- [Pitfalls](./appendices/tech-stack/pitfalls.md) — Named hazards that have corresponding skills or AGENTS.md sections

---

## Related Documents

- [Vision](../product/vision.md)
- [Architecture Overview](../architecture/overview.md)
- [Decisions](../decisions/) — all eight ADRs inform this tech stack
- [Specifications](../specs/) — what the phases above are implementing

---

## Revision History

| Version | Date | Changes |
|---------|------|---------|
| 0.1.0 | 2026-04-23 | Initial draft, seeded from project handoff "Crate stack" and "v0 step plan" sections |
| 0.2.0 | 2026-04-26 | Multi-vault adoption (ADR-0009 / ADR-0010 / ADR-0011): added `vault_registry/` and `control_plane/` modules to the project structure; added forward-pointer note that multi-vault implementation is round 3, post-v0. v0 step plan unchanged. |
