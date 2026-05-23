# AGENTS.md

Agent guide for the Hypomnema project. Read this before touching code.

## What Hypomnema is

> *“a material memory of things read, heard, or thought”*
> — Foucault, on the hypomnemata of the ancient Greeks

Hypomnema makes a directory of notes searchable and reachable programmatically — from the command line (`hmn`) and from AI agents over MCP — running locally on the user's machine. It exposes three search shapes (filesystem, content, semantic) and a live change-event stream to any consumer. Internally it's a long-running local service (`hmnd`); the user-facing framing is the searchable substrate, not the process model. The original v0 gate is complete and the project has shipped beyond it; current boundaries and backlog candidates live in [docs/product/vision.md](docs/product/vision.md) and [notes/backlog.md](notes/backlog.md). It's deliberately agnostic: the vault it watches doesn't have to be Obsidian, and the consumer doesn't have to be any specific agent.

The name comes from the ancient Greek *hypomnema* — a personal notebook of gathered external material kept for rereading. The fit is near-literal: this daemon makes a directory of notes reachable as an accumulated substrate you return to. The crate is `hypomnema`; it ships two binaries — `hmnd` (the daemon) and `hmn` (a thin CLI client that speaks to a running `hmnd`). See [ADR-0008](docs/decisions/0008-two-binary-daemon-plus-cli.md). During early design the project was carried under the working name `mdkb`, which may still appear in older notes.

Current implementation is read-only over user-authored vault files: watch registered vaults, index what’s there, serve search/retrieval queries, manage vault lifecycle, and emit live change events. There is no write-to-vault surface, no ownership model enforcement, no bidirectional sync, and no format spec for bridge-managed files unless a future accepted design explicitly adds one.

## Current guardrails and escalation points

These are not "v0 forbids discussion" rules. They are areas where changes need an explicit design/spec update before implementation because they affect project boundaries or long-lived data contracts:

- Writing to user-authored vault files
- Bridge-managed file conventions such as `iris_type`, `iris_id`, or `hmn_id`
- Ownership-model boundaries (`vault_root` vs `vault_path`)
- Conflict resolution, three-way merge, last-known-synced tracking, or bidirectional sync
- Durable/replayable event history (`subscribe since X`, retained event logs, stream generations)
- Structured frontmatter/tag/backlink indexing
- Semantic indexing beyond Markdown
- A workspace split into multiple crates
- Custom error types via `thiserror`
- Abstract traits for swappable backends (embedding providers, vector stores)

If a change feels like it needs one of these, surface the design decision instead of introducing it quietly.

## The load-bearing rules

**Never block the async runtime with SQLite.** rusqlite is synchronous. Any call that touches a connection goes inside `tokio::task::spawn_blocking`. Mixing them produces deadlocks that will waste hours to diagnose. See `.claude/skills/rusqlite-in-async/`.

**Never put Hypomnema state inside the watched vault.** The index, registry, cache, logs, and any future durable event store all live in the daemon’s data directory, never under the watched path. A synced vault (Syncthing, Dropbox, Obsidian Sync) will mangle any file that grows or mutates frequently. The one-way rule: read from the vault, write elsewhere.

**Never write to user-authored files.** The daemon doesn’t write to the vault today. If a future design adds writes, writes get restricted to a designated subdirectory at the service layer, not at the caller layer.

**Don’t roll your own file-watcher debouncing.** Use `notify` with `notify-debouncer-full`. Editor saves and sync-tool writes produce event storms that any naive approach will get wrong. See `.claude/skills/filesystem-watching/`.

## Project layout

The canonical layout lives in [docs/implementation/tech-stack.md#project-structure](docs/implementation/tech-stack.md#project-structure).

## Core crate stack

The canonical crate list lives in [docs/implementation/tech-stack.md#core-dependencies](docs/implementation/tech-stack.md#core-dependencies).

## Error handling

`anyhow::Result` for anything application-level. Propagate with `?`. Attach context with `.context("what you were doing")` at boundaries where the surrounding operation is meaningful.

Do not introduce `thiserror` or custom error types unless there is a library API or cross-module contract that consumers need to pattern-match on.

## Async patterns

Any SQLite operation goes in `spawn_blocking`. No exceptions. See the `rusqlite-in-async` skill.

Network calls (reqwest to the embedding service, axum handlers) run directly on the runtime.

If you hit a trait-bound error involving async on a trait that already exists for non-abstraction reasons (framework traits like `axum::FromRequest`, `rmcp` handler traits, etc.), reach for `#[async_trait]` on that one trait. Don’t spend an hour fighting the raw language feature. This is an escape hatch for existing traits — not permission to design trait-based abstractions without a concrete second implementation.

Cancellation: every long-running task (watcher loop, live event stream, indexer worker) responds to a shutdown signal. Pass a `tokio::sync::watch` or `CancellationToken` through; don’t `std::process::exit`.

## Testing

Unit tests in `#[cfg(test)]` modules next to the code.

Integration tests in `tests/`. Feature tests that spawn the daemon and hit it over HTTP belong there.

`tempfile::TempDir` for anything that touches the filesystem. Never write tests that assume a specific absolute path.

Before declaring a feature done: `cargo test` and `cargo clippy -- -D warnings`. Clippy is friend.

## Workflow commands

```
cargo check                          # during iteration
cargo watch -x check                 # continuous check on save
cargo test                           # unit + integration tests
cargo clippy -- -D warnings          # lint, warnings as errors
cargo fmt                            # format
cargo run --bin hmnd -- [args]       # run the daemon in the foreground
cargo run --bin hmn  -- <subcommand> # run the CLI client against a running hmnd
```

## Orchestration sessions

When the user opens a message with one of the shorthand prompts below, treat it as a request to enter the orchestration playbook — load @notes/playbook/README.md for the full prompts, role flow, and entrypoint details, then use the long-form prompt that matches.

- orchestrator status
- orchestrator intake-proposal
- orchestrator start-next-round
- orchestrator continue

## Historical v0 step order

The original v0 step order lives in [docs/implementation/tech-stack.md#original-v0-implementation-order-completed](docs/implementation/tech-stack.md#original-v0-implementation-order-completed). It is historical context, not an active gate. New work should follow current specs, ADRs, and backlog items.

## When to ask vs when to proceed

Proceed: implementation questions within a defined step, naming, factoring within a module, test shape.

Ask: scope changes (adding a dependency, splitting a module, introducing an abstraction), design questions that affect more than one step, anything that would be hard to reverse.

## Related skills

- `.claude/skills/rusqlite-in-async/` — the spawn_blocking pattern and why
- `.claude/skills/sqlite-vec-extension/` — loading the extension and vector table patterns
- `.claude/skills/filesystem-watching/` — notify + debouncer + sync-tool gotchas
- `.claude/skills/markdown-chunking/` — pulldown-cmark event-driven chunking

## Related pitfalls

- `docs/implementation/appendices/tech-stack/pitfalls.md` — catalog of named hazards; each entry maps to a skill or to a rule in this file.

## Related design docs

- `docs/hypomnema-handoff.md` — consolidated orientation and scope; lists the historical `iris-vault-bridge-*` documents that live in the Iris project as design groundwork.
