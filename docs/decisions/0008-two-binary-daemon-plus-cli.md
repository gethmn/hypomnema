# ADR-0008: Two Binaries (hmnd + hmn) in One Crate

**Status**: accepted
**Date**: 2026-04-23
**Decision-Makers**: Beau Simensen

---

## Context

Hypomnema needs an executable surface for two distinct roles:

1. A long-running daemon that owns the watched directory, the SQLite store, the sqlite-vec extension, the HTTP and MCP servers, and the background indexing workers.
2. A short-lived client invoked from a terminal or a script that wants to search the vault, check daemon status, or drive the control surface.

Two shapes fit the bill:

- **One binary that internally dispatches** on subcommand (e.g., `hmn start` launches the daemon, `hmn search` queries a running one). This was the shape earlier docs hinted at when they said "the CLI binary is `hmn`."
- **Two separate binaries** with distinct names — `hmnd` for the daemon, `hmn` for the client — built from a single `hypomnema` crate with shared code in `src/lib.rs`.

Both cost roughly the same in Cargo. The choice is positioning, not engineering.

## Decision

Ship two binaries from one crate:

- **Crate:** `hypomnema` — the name in `Cargo.toml`, on crates.io, in prose.
- **Daemon binary:** `hmnd` — long-running process. Owns the watcher, DB connections, HTTP + MCP servers, indexing workers.
- **CLI binary:** `hmn` — thin client. Speaks HTTP to a running `hmnd` for most operations; used over stdio for the MCP surface by invoking `hmnd --mcp-stdio` (final flag shape TBD). Never indexes or watches directly.
- **Shared code:** `src/lib.rs` exports everything both binaries need — config loading, shared types, the typed HTTP client, the server machinery.

No workspace split. No feature flags. No transport abstraction. Both binaries link against the full dependency graph; the daemon's deps dominate and are paid either way. If binary size ever becomes a real concern, gate the heavy deps behind features — but not before.

## Consequences

### Positive

- **Unix convention.** `sshd` / `ssh`, `dockerd` / `docker`, `systemd` / `systemctl`. A `d`-suffixed daemon and a short client is a shape admins and developers already know how to read in `ps aux`, in systemd units, and in documentation. Hypomnema is positioned as a proper long-running daemon with a systemd-style lifecycle; the conventional binary shape tells that story without a paragraph of prose.
- **Binary weight.** `hmnd` pulls the heavy dependency graph (tokio full, axum, rmcp, sqlite-vec loading, embedding HTTP client, notify + debouncer). `hmn` pulls a small subgraph — `reqwest` + `serde` + `clap`. Users who run `hmn search` every day get a fast, small binary; the daemon can be as heavy as it needs to be.
- **MCP clarity.** When an agent host (Claude Code, Iris, others) launches the MCP server, it wants a specific executable. `hmnd --mcp-stdio` reads as "the daemon, running over stdio instead of HTTP." Both modes are the daemon; that framing is cleaner as its own binary than as a subcommand of a conflated CLI.
- **Name question moot.** With two binaries, neither matches the crate name — the crate carries the project name, the binaries carry functional names. The "should the binary match the crate name?" question that dogs single-binary projects doesn't arise.

### Negative

- Two binaries cost roughly twice the link time of one. Mitigated by Cargo's incremental build caching and by `hmn`'s small subgraph. Unlikely to matter until release-build CI becomes a bottleneck.
- Two surfaces to test in release CI instead of one.
- Newcomers looking for "the entry point" have to know to look in `src/bin/` — `src/main.rs` no longer exists.

### Neutral

- This is a source-layout decision, not a runtime-architecture one. Nothing about the watcher, indexer, event stream, or search pipelines changes. The v0 step order in [`implementation/tech-stack.md`](../implementation/tech-stack.md) still holds; each step just has a daemon-side implementation (owned by `hmnd`) and, where applicable, a thin CLI-side wrapper (owned by `hmn`).
- Not a workspace split. A workspace would formalize the daemon/client boundary by splitting into separate crates — explicitly excluded in the tech stack as "not until a second consumer demands reuse of the library." Two binaries in one crate is a strictly lighter-weight structure than a workspace and earns none of a workspace's ceremony.

---

## Notes

- Extends [ADR-0002: Rust over Python](./0002-rust-over-python.md). The single-binary-deployment appeal of Rust was a tipping factor there. That still holds: "single binary" in the Rust sense means "statically linked, no runtime install, ship the file" — not "only one binary in total." Hypomnema now ships two files (plus the sqlite-vec extension), each of which is still a single self-contained executable.
- Extends [ADR-0003: Indexing in the Daemon, Not in the Consumer](./0003-indexing-in-the-daemon.md). That ADR established that Hypomnema is a daemon with an HTTP / MCP surface, not a library consumers link against. This ADR names the executable that *is* the daemon (`hmnd`) and the executable that *reaches* the daemon (`hmn`).
- No abstract "transport" trait in v0. The CLI's HTTP client and the daemon's HTTP server can both reach for `reqwest` and `axum` respectively, talk over localhost, and be done. When a second real consumer demands a more formal boundary, that's when abstraction earns its keep.
- Extended by [ADR-0011: Vault Management Lives on `hmn`](./0011-vault-management-on-hmn.md) — `hmn`'s subcommand surface grows to include vault-management operations (`hmn vault …`); the two-binary shape is preserved.

## Amendments

### 2026-04-27: MCP-over-stdio lives on `hmn`, not `hmnd`

ADR-0008 § Decision was internally inconsistent on the MCP-over-stdio
binary placement. Line 29 named `hmn` as "the CLI ... used over stdio
for the MCP surface" but invoked `hmnd --mcp-stdio`. Line 40 committed
to `hmnd` with the rationale "the daemon, running over stdio instead
of HTTP — both modes are the daemon."

Step 8's workplan resolved the TBD ("final flag shape TBD" on line 29)
as a clap subcommand on `hmn`: **`hmn mcp`**. Rationale:

1. The stdio MCP process under step 8's Resolution D is a thin HTTP
   shim — it opens no SQLite, loads no sqlite-vec extension, runs no
   watcher, holds no embedding client. It is not "the daemon"; it is
   a CLI client of the daemon, just with stdio MCP transport instead
   of human-stdout transport. Line 40's "both modes are the daemon"
   framing was based on an alternative implementation (Model Y:
   stdio-MCP opens SQLite directly, runs as its own daemon-shaped
   process) that Resolution D rejected on WAL-contention and
   operational-simplicity grounds.
2. `hmn`'s ADR-0008-line-38 framing — "the user's general-purpose
   interaction surface for Hypomnema" — fits MCP exactly: an agent
   calling MCP tools is using Hypomnema. The shim that bridges
   agent-stdio to daemon-HTTP is structurally identical to what
   `hmn search …` already does, modulo the input/output transport.
3. When the deferred socket transport ships (post-v0), it lives in
   `hmnd` (long-lived listener — that *is* a daemon feature). Stdio
   on `hmn`, socket on `hmnd`. Each transport's binary matches its
   lifetime.

**Binary weight clarification (amends ADR-0008 § Consequences →
"Binary weight" line 39)**: `hmn`'s dependency graph in v0 grows by
`rmcp` (with `["server", "transport-io", "macros", "schemars"]`) and
`schemars`. `hmnd` does **not** link `rmcp` in v0 — the daemon does
not bind any MCP transport itself until socket transport lands. The
original "small subgraph (reqwest + serde + clap)" framing is
superseded by "small subgraph + rmcp's stdio-server dep set."

See [step-08 workplan § Resolution A](../roadmap/step-08-workplan.md#a-final-flag-shape-and-binary-placement-hmnd---mcp-stdio-vs-hmnd-mcp-stdio-vs-hmn-mcp-vs-env-var)
and [§ Resolution F](../roadmap/step-08-workplan.md#f-tool-result-content-shape--structured_content-vs-content-vs-both-and-where-rmcp-lives-in-the-dep-graph)
for the full reasoning, and [ADR-0012](./0012-mcp-transport-stdio-v0.md)
for the formal record of stdio-shipped / socket-deferred. The
two-binary shape and the "thin client" framing of the ADR are
otherwise preserved — this amendment clarifies *which* thin client
owns the MCP surface, not whether the project uses thin clients.

### 2026-04-28: HTTP-MCP joins socket-MCP as the second `hmnd`-resident MCP transport ([ADR-0013](./0013-mcp-transport-streamable-http.md), round 4)

The "each transport's binary matches its lifetime" principle from the 2026-04-27 amendment applies unchanged: stdio-MCP → `hmn` (short-lived adapter); HTTP-MCP → `hmnd` (long-lived listener); socket-MCP (deferred) → `hmnd` (long-lived listener). No decision change to the two-binaries shape; `hmnd` simply gains a second MCP transport mount alongside the deferred Unix socket.
