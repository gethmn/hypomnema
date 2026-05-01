# AGENTS.md

Agent guide for the Hypomnema project. Read this before touching code.

## What Hypomnema is

> *“a material memory of things read, heard, or thought”*
> — Foucault, on the hypomnemata of the ancient Greeks

Hypomnema is a local daemon that indexes a Markdown directory and exposes search (filesystem, content, semantic) and change events to any consumer — most commonly an AI agent connected via MCP. It’s deliberately agnostic: the vault it watches doesn’t have to be Obsidian, and the consumer doesn’t have to be any specific agent.

The name comes from the ancient Greek *hypomnema* — a personal notebook of gathered external material kept for rereading. The fit is near-literal: this daemon makes a directory of notes reachable as an accumulated substrate you return to. The crate is `hypomnema`; it ships two binaries — `hmnd` (the daemon) and `hmn` (a thin CLI client that speaks to a running `hmnd`). See [ADR-0008](docs/decisions/0008-two-binary-daemon-plus-cli.md). During early design the project was carried under the working name `mdkb`, which may still appear in older notes.

The project is in **v0** scope. v0 is read-only: watch a directory, index what’s there, serve search queries, emit change events. No writes to the vault. No ownership model enforcement. No bidirectional sync. No format spec for bridge-managed files.

All of the above are real and planned. None are in v0.

## What not to build

A partial list of things that belong to later phases and should not appear in v0:

- Writing to the vault (any path under the watched directory)
- An `iris_type` / `hmn_id` frontmatter convention for bridge-managed files
- Ownership-model boundaries (`vault_root` vs `vault_path`)
- Conflict resolution, three-way merge, last-known-synced tracking
- Durable/replayable event history (`subscribe since X`, retained event logs, stream generations)
- A workspace split into multiple crates
- Custom error types via `thiserror`
- Abstract traits for swappable backends (embedding providers, vector stores)

If a change feels like it needs one of these, flag it and ask rather than introduce it quietly.

## The load-bearing rules

**Never block the async runtime with SQLite.** rusqlite is synchronous. Any call that touches a connection goes inside `tokio::task::spawn_blocking`. Mixing them produces deadlocks that will waste hours to diagnose. See `.claude/skills/rusqlite-in-async/`.

**Never put Hypomnema state inside the watched vault.** The index, registry, cache, logs, and any future durable event store all live in the daemon’s data directory, never under the watched path. A synced vault (Syncthing, Dropbox, Obsidian Sync) will mangle any file that grows or mutates frequently. The one-way rule: read from the vault, write elsewhere.

**Never write to user-authored files.** In v0 the daemon doesn’t write to the vault at all, so this is automatic. If that changes in a later phase, writes get restricted to a designated subdirectory at the service layer, not at the caller layer.

**Don’t roll your own file-watcher debouncing.** Use `notify` with `notify-debouncer-full`. Editor saves and sync-tool writes produce event storms that any naive approach will get wrong. See `.claude/skills/filesystem-watching/`.

## Project layout

The canonical layout lives in [docs/implementation/tech-stack.md#project-structure](docs/implementation/tech-stack.md#project-structure).

## Core crate stack

The canonical crate list lives in [docs/implementation/tech-stack.md#core-dependencies](docs/implementation/tech-stack.md#core-dependencies).

## Error handling

`anyhow::Result` for anything application-level. Propagate with `?`. Attach context with `.context("what you were doing")` at boundaries where the surrounding operation is meaningful.

Do not introduce `thiserror` or custom error types unless there is a library API that consumers need to pattern-match on — which there isn’t in v0.

## Async patterns

Any SQLite operation goes in `spawn_blocking`. No exceptions. See the `rusqlite-in-async` skill.

Network calls (reqwest to the embedding service, axum handlers) run directly on the runtime.

If you hit a trait-bound error involving async on a trait that already exists for non-abstraction reasons (framework traits like `axum::FromRequest`, `rmcp` handler traits, etc.), reach for `#[async_trait]` on that one trait. Don’t spend an hour fighting the raw language feature. This is an escape hatch for existing traits — not permission to design trait-based abstractions (see "What not to build").

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

## Playbook control

For orchestration sessions, use these one-liners:

- `orchestrator status`
- `orchestrator start-next-round`
- `orchestrator continue`

For full prompts and role-flow details, load @notes/playbook/README.md.

## The v0 step order

The canonical v0 step order lives in [docs/implementation/tech-stack.md#implementation-priority](docs/implementation/tech-stack.md#implementation-priority). If a PR touches step N, it must not start building step N+1 opportunistically.

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
