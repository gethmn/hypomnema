# ADR-0002: Rust over Python

**Status**: accepted
**Date**: 2026-04-23
**Decision-Makers**: Beau Simensen

---

## Context

Hypomnema is a long-running local daemon that watches a directory, maintains persistent indexes (including a SQLite database with the sqlite-vec extension), serves an HTTP and an MCP endpoint, and writes to an append-only event log. The two realistic implementation languages were Python and Rust.

Python's appeal:
- Faster to v0 — mature ecosystems for filesystem watching (`watchdog`), embedding (`sentence-transformers`, `llama-cpp-python`), MCP (`mcp` Python SDK), HTTP (`fastapi`), and SQLite
- Shorter feedback loop during prototyping
- Familiar to most collaborators

Rust's appeal:
- Deployable as a single binary (important for the office scenario where Python install management is a real cost)
- Type system catches the whole class of bugs that haunt long-running services — `None`-shaped runtime errors, forgotten state transitions, concurrent mutation
- `tokio` + `axum` + `rmcp` (official Rust MCP SDK) + `rusqlite` + `notify` form a mature, well-fitting stack
- Expected project lifespan is years; upfront cost amortizes; the porting cost from a Python prototype would not realistically be paid

## Decision

Implement Hypomnema in Rust.

The tipping factors were (a) the office deployment scenario — a single binary plus the sqlite-vec extension `.so`/`.dylib`/`.dll` is far easier to install and maintain than a Python environment — and (b) the project's expected lifespan. The MCP Rust SDK (`rmcp`, official) is mature enough to remove the main practical objection (missing protocol support).

## Consequences

### Positive

- Single-binary deployment; no runtime installation of Python, venvs, or system libraries beyond the extension
- Compile-time guarantees about lifetimes, nullability, and thread safety — especially valuable for a process that must run for days without crashing
- Performance headroom: scanning, hashing, and embedding can utilize available cores without GIL contention

### Negative

- Slower initial velocity vs. Python; more time spent on `Result` plumbing, lifetimes, and trait bounds before anything runs
- Smaller pool of collaborators comfortable with the stack
- The rusqlite + tokio combination has a known blocking-in-async trap (see the `rusqlite-in-async` skill in `.claude/skills/`); every SQL call must go through `spawn_blocking`

### Neutral

- The design itself is language-agnostic; a Python port would be possible if the decision reversed, but the cost of writing the Rust version from scratch is expected to be less than the cost of porting a working Python version to Rust once it stopped being sufficient

---

## Notes

- Skills in `.claude/skills/rusqlite-in-async/` capture the most important language-level gotcha this choice introduces
- `docs/implementation/tech-stack.md` lists the concrete crate choices

## Amendments

<!-- None yet -->
