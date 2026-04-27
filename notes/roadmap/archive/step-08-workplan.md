# Step 8 Workplan — MCP wrapper (round-2 shipping gate)

**Step**: 8 of 8 (round 2 of 2). Final step of round 2 — see [`roadmap-2.md`](./roadmap-2.md) for the round and [`step-07-workplan.md`](./step-07-workplan.md) for the immediately prior step (whose `search_filesystem` / `search_content` / `search_semantic` HTTP surfaces this step wraps over MCP). This is the round-2 shipping gate; on shipping the full **after-step-8 boundary ritual** runs (milestone tag, ADRs, per-step + end-of-round retros — see [`roadmap-2.md`](./roadmap-2.md) § "After step 8").

**Status**: Shipped 2026-04-27.

**Round-2 lessons carrying forward** (from [`notes/project-planning-workflow-notes.md`](../../project-planning-workflow-notes.md) § Step 6 retro and § Step 7 retro):

- Risk grade is honest: step 8 is **medium-high overall**, with an unevenly-distributed risk surface. The pure transport-wiring tasks (Tasks 8.1–8.3) are medium; the agent-integration manual test (Task 8.5, criterion 2 — Claude Code or Iris in the loop) is the load-bearing risk and is *qualitatively new* — round 1 and round 2 step 6/7 had no analogous external-host-in-the-loop dependency.
- All five deferred decisions from the roadmap are resolved here at workplan-write time per round-1 lesson 2 ("pull deferred decisions forward into workplan-time"). One is a workplan-time **scope rescope of shipping criterion 4** (deferring the Unix-socket transport to a follow-on); see Resolution D and the rescoped criterion in [§ Shipping criteria (rescoped)](#shipping-criteria-rescoped) below.
- Self-review for prose accuracy (round-1 boundary heuristic) runs after the first draft. This workplan is projected to land near or above the ~1000-line threshold given the new external library (`rmcp`) — the heuristic fires; results in [§ Self-review for prose accuracy](#self-review-for-prose-accuracy).
- Coordinator-spawned in-build follow-up (the Task 6.4r1 pattern, see step-6 retro § Build-time amendments) remains available; the act-now decision rule from step 6 stands.
- Manual smoke verification is a per-step investment that paid off in steps 5, 6, and 7. Step 8's wiring task (Task 8.3) and the agent-integration task (Task 8.5) are the two natural smoke points; the latter is the load-bearing test for the round shipping gate.
- No in-tree skill covers `rmcp` / MCP transport patterns. Tasks 8.1–8.3 instead cite `rmcp` upstream docs at task time, with an explicit syntax-verification gate (Resolution C and Task 8.1 prose). If `rmcp`-shaped patterns prove load-bearing for round 3 or beyond, write a skill at this round's boundary per the step-7 retro recommendation.

---

## Goal recap

`hmn mcp` (the resolved flag shape and binary placement — see Resolution A) starts an MCP server over stdio that exposes the three search operations as MCP tools: `search_filesystem`, `search_content`, `search_semantic` (matching ADR-0004 — see Resolution B). Tool input shapes are derived from the existing `src/api/types.rs` request types via `schemars::JsonSchema` (Resolution C). Tool output shapes are the same `*SearchResponse` types serialized as MCP `structured_content`. Errors map to MCP `structured_error` with the same `{ "error": { "code": "...", "message": "..." } }` envelope HTTP returns.

The `hmn mcp` process is a **thin CLI shim that translates MCP tool calls to HTTP requests** against a separately-running `hmnd` daemon (Resolution D, stdio half). It opens **no** SQLite store, loads **no** sqlite-vec extension, runs **no** watcher, holds **no** embedding client of its own — every search request is forwarded to the daemon, which already owns those substrates. This is the structural reason MCP-stdio belongs on `hmn` and not `hmnd` (see Resolution A and the ADR-0008 inconsistency it resolves): the shim is exactly what `hmn` already is — a thin HTTP client of the daemon — just with stdio MCP transport instead of the CLI's human-stdout transport.

Claude Code (or another MCP-capable agent) configured to invoke `hmn mcp` can call each of the three tools and receive results that match the spec response shapes. The agent-integration manual test (Task 8.5, criterion 2) is the round-2 shipping gate's load-bearing test.

The Unix-socket transport (`mcp.transport = "socket"`) is **deferred to a follow-on workplan** (Resolution D, socket half). The `mcp.transport` config knob continues to parse and validate; non-`stdio` values produce a clear "not yet implemented" warning at `hmnd` daemon startup but do not crash the daemon. This is a workplan-time rescope of shipping criterion 4 — see [§ Shipping criteria (rescoped)](#shipping-criteria-rescoped). Authentication on the socket transport (Resolution E) is documented forward-compat but not implemented. When socket lands, it lives in the `hmnd` daemon (long-lived listener) — keeping the binary split clean: stdio = `hmn` (short-lived adapter); socket = `hmnd` (long-lived listener).

---

## Deferred-decision resolutions

The five TBDs from [`roadmap-2.md`](./roadmap-2.md) § Step 8 are resolved below (A–E). Two additional fall-out resolutions surfaced during task decomposition (F and G) — they are not in the roadmap but are workplan-time decisions worth naming.

### A. Final flag shape and binary placement: `hmnd --mcp-stdio` vs. `hmnd mcp-stdio` vs. `hmn mcp` vs. env-var

**Resolution**: subcommand on `hmn`, not `hmnd`. The shape is `hmn mcp`, a new variant of the existing `Command` enum in [`src/cli.rs`](../../../src/cli.rs) (parallel to the existing `Command::Search` and `Command::Status`). The previously-documented `hmnd --mcp-stdio` flag form is dropped; the documentation in [`docs/reference/cli.md`](../../../docs/reference/cli.md) (lines 54, 62, 77) and [ADR-0008](../../../docs/decisions/0008-two-binary-daemon-plus-cli.md) § Decision is amended in Task 8.5 to resolve a pre-existing internal inconsistency in the ADR (see "ADR-0008 inconsistency" below).

The agent host's MCP configuration shape — already a one-liner — becomes:

```json
{ "command": "hmn", "args": ["mcp"] }
```

versus the would-have-been variants:

```json
{ "command": "hmnd", "args": ["--mcp-stdio"] }   // ADR-0008 line 40 framing
{ "command": "hmnd", "args": ["mcp-stdio"] }     // workplan v1 (subcommand-on-hmnd)
```

**Why subcommand over flag**: three reasons, in descending weight.

1. *clap subcommand model is the existing pattern.* `hmn`'s `Command` enum already has `Search { mode }` and `Status`. Adding `Mcp` is the natural extension. A flag-form `--mcp` would be the first `hmn` flag that *replaces* the default-mode operation (which is to require a subcommand) rather than *modifies* it.
2. *Per-mode help is cleaner.* `hmn mcp --help` can document mcp-specific options (currently none, but the slot exists for future `--transport`, per-tool gating, etc.) without polluting `hmn --help`. Round 3 (vault-management MCP tools) is likely to benefit.
3. *Future transport options compose cleanly under a subcommand.* If a `socket`-side CLI dial-in mode ever ships (`hmn mcp --transport socket`), it's a flag on this subcommand. Under a flag-form `--mcp`, the same future option would be `--mcp --mcp-transport socket` — gnarly.

**Why `hmn` over `hmnd`**: this is the canon-touching half of the resolution.

Under [Resolution D](#d-connection-lifecycle-stdio-process-per-connection-vs-socket-long-lived) the stdio MCP process is a **thin HTTP shim**: opens no SQLite, loads no sqlite-vec extension, runs no watcher, holds no embedding client, owns no daemon-state. It is structurally a `reqwest` + `serde` + `clap` + `rmcp` process. That dependency profile is `hmn`'s, not `hmnd`'s. ADR-0008 § Decision line 38 reads: "`hmn` is the user's general-purpose interaction surface for Hypomnema." An agent talking via MCP is using Hypomnema; the shim that bridges agent-stdio to daemon-HTTP is exactly what `hmn` already is — just with a stdio MCP transport instead of the human-stdout transport `hmn search` and `hmn status` use.

When the socket transport ships (deferred — see Resolution D), the *running daemon* (`hmnd`) binds the socket and accepts MCP connections directly. That's a `hmnd` feature in the same way HTTP is. Stdio is a CLI feature; socket is a daemon feature. Each transport lives in the binary that matches its lifetime: short-lived adapter (`hmn mcp`) vs. long-lived listener (`hmnd`'s socket-MCP).

**ADR-0008 inconsistency this resolves**: ADR-0008 § Decision is internally inconsistent on this point. Line 29 reads:

> **CLI binary:** `hmn` — thin client. Speaks HTTP to a running `hmnd` for most operations; **used over stdio for the MCP surface by invoking `hmnd --mcp-stdio`** (final flag shape TBD).

— which names `hmn` as the MCP surface but invokes `hmnd`. Line 40 commits to `hmnd`:

> When an agent host (Claude Code, Iris, others) launches the MCP server, it wants a specific executable. `hmnd --mcp-stdio` reads as "the daemon, running over stdio instead of HTTP." Both modes are the daemon.

Line 40's rationale ("the daemon running over stdio") was implicitly assuming a self-contained-process implementation (Model Y: stdio-MCP opens SQLite directly, runs as its own daemon-shaped process). Resolution D explicitly rejects Model Y in favor of the thin-HTTP-shim model (Model X). Under Model X, line 40's framing is literally false — the shim is *not* the daemon, it's an HTTP client of the daemon. The ADR's TBD escape hatch ("final flag shape TBD") authorizes the binary-placement clarification at workplan time. Task 8.5 amends ADR-0008 to commit to `hmn mcp` and to update the "binary weight" prose (line 39) acknowledging that `hmn` now pulls `rmcp` and `schemars` (see [Resolution F](#f-tool-result-content-shape--structured_content-vs-content-vs-both-and-where-rmcp-lives-in-the-dep-graph)).

**Subcommand suffix — `mcp` vs. `mcp-stdio`**: the bare `mcp` form. Stdio is implied as the v0 transport; if a future `--transport socket` flag ever lands (where `hmn mcp` dials the daemon's deferred-socket transport and bridges to the user's stdio), it's a flag on this same subcommand, not a sibling subcommand. Avoids the `mcp-stdio` / `mcp-socket` / `mcp-…` proliferation.

**How to apply**: in [`src/cli.rs`](../../../src/cli.rs), add `Command::Mcp` to the `Command` enum (line 30–38). Route in [`src/bin/hmn.rs`](../../../src/bin/hmn.rs) `main()` (line 38–87) to a new `cmd_mcp(&config, override_url)` function. The new function lives in `src/bin/hmn.rs` (top-level, parallel to `cmd_search_filesystem` / `cmd_search_content` / `cmd_search_semantic` / `cmd_status`); it constructs an `rmcp` server over stdio and `await`s it. Body sketch in Task 8.3.

**References**: [ADR-0008 § Decision](../../../docs/decisions/0008-two-binary-daemon-plus-cli.md#decision) (lines 29 and 40 — the inconsistency this resolution settles); [`docs/reference/cli.md`](../../../docs/reference/cli.md) lines 54–77 (currently documents `--mcp-stdio` on `hmnd`; gets fully restructured in Task 8.5 to put MCP on the `hmn` section); [`src/cli.rs:30-38`](../../../src/cli.rs) (existing `Command` enum); [`src/bin/hmn.rs:38-87`](../../../src/bin/hmn.rs) (existing dispatch).

### B. MCP tool names

**Resolution**: keep `search_filesystem` / `search_content` / `search_semantic` — exactly as named in [ADR-0004 § Decision](../../../docs/decisions/0004-three-search-modes-as-peers.md#decision) and the architecture overview ([`docs/architecture/overview.md`](../../../docs/architecture/overview.md) line 114). No flatter or shorter naming.

**Why**: ADR-0004 is the canon and was written specifically with the MCP tool surface in mind. The verb_noun ordering matches MCP convention (`read_file`, `search_files`, `list_directory` in the upstream `@modelcontextprotocol/server-filesystem` and similar reference implementations). Renaming at MCP-time would create a wire-shape divergence between HTTP (`/search/filesystem`) and MCP (`filesystem_search` or similar) for no upside; agents reading the architecture overview would see one set of names in prose and a different set in their tool list.

The flatter-naming alternative (e.g. `filesystem_search`, `content_search`, `semantic_search` — noun-verb) was considered and rejected: it would break consistency with both ADR-0004 (canon) and the verb_noun MCP convention.

**How to apply**: in the `#[tool_router(server_handler)] impl HypomnemaMcpServer` block (Task 8.2), the three methods are named `search_filesystem`, `search_content`, `search_semantic`. The `#[tool(description = "...")]` macro on each takes a description string but the tool *name* derives from the method name. Verify at task time that rmcp 1.5.0's `#[tool]` macro doesn't apply any name-mangling (snake_case → camelCase, etc.); upstream's calculator example (`fn sum`, `fn sub`) uses the method name verbatim.

**References**: [ADR-0004 § Decision](../../../docs/decisions/0004-three-search-modes-as-peers.md#decision); [`docs/architecture/overview.md`](../../../docs/architecture/overview.md) line 114; rmcp 1.5.0 calculator example (`examples/servers/src/common/calculator.rs` upstream).

### C. Tool parameter schemas — derive vs hand-author

**Resolution**: derive from the existing `src/api/types.rs` request shapes. Add `#[derive(schemars::JsonSchema)]` (re-exported as `rmcp::schemars::JsonSchema` per the upstream calculator example) alongside the existing `#[derive(serde::Deserialize)]` on:

- `FilesystemQueryJson` (existing — `src/api/types.rs:3-13`)
- `ContentQueryJson` (existing — `src/api/types.rs:31-46`)
- `SemanticQueryJson` (existing — `src/api/types.rs:73-82`)

Per-field `#[schemars(description = "...")]` annotations are added alongside the existing `#[serde(...)]` attributes — agents read these descriptions at tool-list time to understand each parameter. Suggested descriptions track the spec language in [`docs/specs/filesystem-search.md`](../../../docs/specs/filesystem-search.md), [`docs/specs/content-search.md`](../../../docs/specs/content-search.md), and [`docs/specs/semantic-search.md`](../../../docs/specs/semantic-search.md); concrete strings drafted in Task 8.1.

Response types (`FilesystemSearchResponse`, `ContentSearchResponse`, `SemanticSearchResponse` and their per-result shapes) **also** receive `#[derive(schemars::JsonSchema)]` so the same types can be used to populate tool *output* schemas if rmcp 1.5.0 supports tool-output-schema declaration. (rmcp 1.5.0's `#[tool]` macro takes only a description in the calculator example; the agent verifies at Task 8.1 time whether the macro accepts an output-schema attribute, and if not, the response derives are still useful for client-side typing in `tests/mcp.rs`.)

**Why**: deriving from the HTTP request types is the only structurally-honest answer. The HTTP and MCP wire shapes are the *same* operations — that's the load-bearing claim of ADR-0004. Hand-authoring would create two type definitions to maintain in lockstep; any drift is a wire-shape bug. Deriving keeps them aligned at compile time: any future field added to `FilesystemQueryJson` automatically flows to the MCP tool schema with no manual sync.

The `#[schemars(description = ...)]` annotations are the ergonomic-only addition. They have no effect on serde behavior and don't change the runtime wire shape; agents see them as the parameter descriptions in `tools/list` responses. Agent ergonomics is one of ADR-0004's load-bearing rationales — descriptions that say "Vault-relative path prefix to scope results to a subdirectory" are materially better than agents inferring from field names alone.

Re-exporting `schemars::JsonSchema` via `rmcp::schemars` (the path used in upstream's calculator example) avoids a direct top-level `schemars` dependency in `Cargo.toml`. The agent verifies at Task 8.1 time that rmcp 1.5.0's `schemars` feature flag exposes the `JsonSchema` derive macro through the `rmcp::schemars` re-export. If the re-export path differs (or rmcp 1.5.0 changed it post-calculator-example), the agent corrects to the upstream-verified path and notes the verified syntax in the task's results comment.

**How to apply**: edit `src/api/types.rs` (Task 8.1) — add `#[derive(schemars::JsonSchema)]` (or the verified upstream path) to the three request types and the three response/per-result types. Add `#[schemars(description = "...")]` per field on the request types only (response fields are agent-consumed but don't need descriptions for tool-schema purposes). Add a unit test that calls `schemars::schema_for!(FilesystemQueryJson)` and asserts the schema's `properties` object contains the expected field names; one such test per request type.

**References**: rmcp 1.5.0 upstream `examples/servers/src/common/calculator.rs` (`#[derive(... schemars::JsonSchema)]` on input structs, `rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router}` import path); [ADR-0004 § Decision](../../../docs/decisions/0004-three-search-modes-as-peers.md#decision).

### D. Connection lifecycle: stdio (process-per-connection) vs socket (long-lived)

**Resolution**: ship stdio fully; defer the socket transport to a follow-on workplan.

**Stdio half (shipped here)**: process-per-connection. The `hmn mcp` subprocess serves exactly one MCP session — the host's stdio. Process startup is fast (no SQLite open, no extension load, no scan, no daemon-state initialization); exit happens when the host closes stdin. Inside the process, the MCP server holds a `DaemonClient` (the existing HTTP client from `src/client.rs`) configured against `http://${config.http.bind}` (or `--daemon-url` / `HYPOMNEMA_DAEMON_URL` override — both already supported by `hmn` today), and translates MCP tool calls into HTTP requests against the running `hmnd`. Per-tool-call HTTP latency is loopback-only (sub-millisecond on commodity hardware); the overhead is acceptable for v0. This is the same wiring shape `hmn search filesystem` already uses; the only difference is the input/output transport (stdio MCP instead of CLI args / stdout text).

If the daemon is not reachable at tool-call time, the tool returns a `CallToolResult::structured_error` with a synthesized envelope `{ "error": { "code": "daemon_unreachable", "message": "<configured URL> did not respond: <transport detail>" } }`. The MCP client (Claude Code, Iris) sees this as a structured tool-call error; the agent can branch on `error.code` if it cares.

**Socket half (deferred)**: out of scope for this workplan. The `mcp.transport = "socket"` config value continues to parse and validate; if set, the `hmnd` daemon's startup logs `WARN`: `"mcp.transport = \"socket\" is not implemented in v0; only mcp.transport = \"stdio\" is shipped via the `hmn mcp` subcommand on the CLI binary. The socket file at <path> is NOT bound."` The daemon does not crash; HTTP and the watcher run normally. Future-step note in Task 8.5's documentation: when the socket transport ships, the long-lived shape is "the daemon binds a `tokio::net::UnixListener` at `mcp.socket`, accepts connections, and spawns a per-connection task that wires the `(AsyncRead, AsyncWrite)` pair from `UnixStream::into_split()` into the same `HypomnemaMcpServer.serve(...)` entry point used by stdio." The socket-MCP server lives in `hmnd` (long-lived listener); stdio-MCP lives in `hmn` (short-lived adapter). Same `HypomnemaMcpServer` impl in `src/mcp/`; different transport wiring per binary.

**Why defer the socket half**: four reasons.

1. *The agent-integration test (criterion 2) is stdio-shaped.* Claude Code and Iris invoke MCP servers via stdio process spawns; their MCP configuration is a `command` + `args` shape that runs a binary and pipes its stdio. A socket transport would require the agent host to dial the socket, which neither Claude Code nor Iris currently does for local servers. Shipping stdio fully is the load-bearing path to the round-2 shipping gate.
2. *Socket adds daemon-startup/shutdown complexity.* An accept-loop integrated with the existing `shutdown::install()` channel; per-connection task spawning; cleanup of the socket file on shutdown to avoid `EADDRINUSE` on restart; mode-0600 permission handling. Each of these is small individually; bundled they push the workplan's task count to 7+ and expand the medium-risk surface.
3. *The two transports would have asymmetric tool implementations.* Stdio = HTTP shim (out-of-process; talks to the daemon over loopback HTTP). Socket = in-daemon (talks to `src/search/` directly via the daemon's existing `ApiState`). Two test surfaces for one wire-level behavior. Round 3 (multi-vault) can resolve the asymmetry by introducing a shared backend abstraction; doing it now without a concrete second consumer is speculative.
4. *Round 1 also rescoped its shipping gate at workplan time.* Step 5 deliberately did not ship MCP, even though the v0 step plan named MCP at the gate; the round-1 retro recorded this rescope as the right call. Same precedent: ship the load-bearing transport, document the deferred half, leave the forward path explicit.

**Decision rule for un-deferring**: when a concrete consumer needs socket-MCP — and that consumer has named the use case (e.g. "round 3 multi-vault MCP tools that mutate state under high call rates," or "an agent host that prefers persistent connections") — pull the socket transport up at that workplan. The implementation is well-scoped: `tokio::net::UnixListener` accept-loop + per-connection rmcp service + permission setting + shutdown integration.

**How to apply (stdio half)**: see Tasks 8.2 and 8.3. The `HypomnemaMcpServer` struct holds a `DaemonClient`; each tool method calls `self.client.search_*(...).await` and maps the result; `run_mcp_stdio()` constructs the server and calls `.serve(stdio()).await?` per the rmcp upstream calculator example.

**How to apply (socket half — deferred warning)**: in the new `run_mcp_stdio()` function, **do not** read `config.mcp.transport`. (The subcommand is its own mode; the config knob is for the long-running daemon.) Inside `run_daemon()` (the default mode), after the existing initial-setup block (around line 89 today) and before the watcher spawn (line 122 today), insert a warning emission if `config.mcp.transport != "stdio"`:

```rust
if config.mcp.transport != "stdio" {
    tracing::warn!(
        configured = %config.mcp.transport,
        socket = %config.mcp.socket.0.display(),
        "mcp.transport = {:?} is not implemented in v0; only stdio via the `hmn mcp` subcommand \
         is shipped. The socket file is NOT bound. To use MCP, invoke `hmn mcp` from the agent host.",
        config.mcp.transport,
    );
}
```

The warning is one-shot at startup; no runtime behavior change. The warning lives in `hmnd::run_daemon()` because `mcp.transport` is the long-running daemon's config — the deferred socket transport, when it lands, will be a daemon-side feature. Test in Task 8.3.

**References**: [ADR-0008 § Decision](../../../docs/decisions/0008-two-binary-daemon-plus-cli.md#decision); [`roadmap-2.md`](./roadmap-2.md) § Step 8 deferred decision 4; round-1 retro § "End-of-round retrospective" point 1 ("the system held"); [`notes/project-planning-workflow-notes.md`](../../project-planning-workflow-notes.md) § Step 7 retro § Notes (last bullet, "step 8's MCP transport is structurally similar to step 5's filesystem/content handlers; the load-bearing risk is the agent-integration test").

### E. Authentication on the socket transport

**Resolution**: not applicable in v0 because the socket transport is deferred (Resolution D). The forward-compat decision is recorded here so a future workplan can pull it up without re-litigating: when the socket transport ships, authentication is **filesystem permissions only** — the socket file at `mcp.socket` is created with mode `0600` (owner read/write only). No token, no challenge-response, no TLS.

**Why filesystem permissions**: the trust boundary is the user's home directory. Anyone who can read the socket file at `~/.local/share/hypomnema/mcp.sock` already has access to `~/.config/hypomnema/config.toml` (which contains the daemon URL and any embedding-service `api_key`) and `~/.local/share/hypomnema/index.sqlite` (which contains every chunk's text). Adding token auth would be theater — the keys would have to live in the same trust boundary as the socket they protect.

The HTTP server is loopback-only with no auth (`docs/reference/configuration.md` line 103); mode-0600 on the socket is the structural equivalent (loopback for sockets is "same user only"). The trust model is consistent across transports.

**Why no token**: tokens add a configuration burden, a key-rotation question, and a place for misconfiguration (writing the token to disk world-readable, sharing it in environment variables that leak via `ps`, etc.). For a local daemon serving a local user, mode-0600 is sufficient and self-evidently auditable (`ls -l ~/.local/share/hypomnema/mcp.sock` is the audit).

**How to apply**: deferred. When the socket transport ships, the implementation pattern is:

```rust
let listener = tokio::net::UnixListener::bind(&config.mcp.socket.0)?;
// Set mode 0600 on the socket file (Unix only).
#[cfg(unix)]
{
    use std::os::unix::fs::PermissionsExt;
    let perms = std::fs::Permissions::from_mode(0o600);
    std::fs::set_permissions(&config.mcp.socket.0, perms)?;
}
```

(Verify against current tokio + std semantics at the future task time.)

**References**: [`docs/reference/configuration.md`](../../../docs/reference/configuration.md) line 103 ("Loopback-only by default; v0 does not implement auth"); [`docs/decisions/0005-local-everything.md`](../../../docs/decisions/0005-local-everything.md) § Decision (the local-everything trust boundary).

### Resolved as part of this step (not pre-flagged in the roadmap)

#### F. Tool result content shape — `structured_content` vs `content` vs both

**Resolution**: ship `CallToolResult::structured(serde_json::to_value(response)?)` only, populating `structured_content`. Do **not** also populate the `content` text-content field with a redundant text rendering of the JSON. Errors map to `CallToolResult::structured_error(envelope_value)` with the same `{ "error": { "code": "...", "message": "..." } }` envelope shape HTTP returns.

**Why**: rmcp 1.5.0's `CallToolResult` has both `content: Vec<Content>` and `structured_content: Option<Value>` as independent fields; the `structured(value)` constructor sets only `structured_content`. MCP clients that consume `structured_content` (the modern path) get fully-typed JSON they can parse with their existing decoders. Clients that pre-date `structured_content` and rely on text-rendered `content` are out of scope for v0 — the target hosts (Claude Code, Iris) are built against current MCP and consume `structured_content` natively.

The error shape mirrors HTTP intentionally. The same code-token-prefix mapping HTTP uses (`invalid_glob`, `invalid_regex`, `invalid_prefix`, `invalid_request`, `embedding_unavailable`, `internal`) flows through unchanged: the daemon returns a JSON error envelope; the MCP shim deserializes and re-emits via `structured_error`. No new error codes are introduced at the MCP layer.

The `daemon_unreachable` synthesized error (Resolution D) is **new at the MCP layer** — there's no HTTP analogue because HTTP can't return errors when the daemon isn't running (the connect itself fails). The MCP shim creates this envelope shape locally when the underlying `reqwest::Error` indicates a connect failure (`is_connect_error()` from `src/client.rs:92-98`). The message includes the configured daemon URL.

**Backwards-compat trap to avoid**: do not invent any per-tool result wrapper. The wire shape an MCP client sees is identical to the HTTP client's wire shape (modulo the MCP envelope around it). A `SemanticSearchResponse` round-trips through MCP with no field renaming, no field reordering, no per-tool serialization — `serde_json::to_value(response)` is the entire conversion.

**How to apply**: in each tool method (Task 8.2), the body shape is roughly:

```rust
match self.client.search_filesystem(&input).await {
    Ok(resp) => Ok(CallToolResult::structured(serde_json::to_value(resp)?)),
    Err(err) => Ok(CallToolResult::structured_error(error_envelope_from_anyhow(&err))),
}
```

Where `error_envelope_from_anyhow` parses the `anyhow::Error`'s `Display` to recover the `{code, message}` shape. The existing `decode_response` in `src/client.rs:73-90` already produces an `anyhow!("{}: {}", env.error.code, env.error.message)` chain; the MCP shim does the inverse (split on the first `": "`). For connect errors (per `is_connect_error()`), synthesize `{ code: "daemon_unreachable", message: format!("{} did not respond: {err:#}", self.client.base_url()) }`.

**References**: rmcp 1.5.0 `CallToolResult` (`structured` / `structured_error` constructors); `src/client.rs:73-98` (existing HTTP error decoding); [ADR-0004 § Decision](../../../docs/decisions/0004-three-search-modes-as-peers.md#decision) (HTTP and MCP are peers — same wire shapes).

#### G. Logging during `hmn mcp` mode — stderr-only

**Resolution**: when `hmn mcp` runs, `tracing` log output is redirected to `stderr` only. **Stdout is reserved for the MCP transport** — any byte that lands on stdout outside the rmcp framing is a protocol violation that breaks the MCP session.

Concretely: the `cmd_mcp()` function (or `main()` itself, before any other initialization) configures tracing with `tracing_subscriber::fmt().with_writer(std::io::stderr).with_ansi(false)` *before* constructing the rmcp server. The default tracing-subscriber writer is stdout; left unchanged, any `tracing::info!` from the DaemonClient request, the config-load path, or any other path would corrupt the MCP transport.

**Why**: the rmcp upstream calculator example (`examples/servers/src/calculator_stdio.rs`) explicitly does this: `tracing_subscriber::fmt().with_writer(std::io::stderr).with_ansi(false).init()`. The pattern is load-bearing for stdio MCP servers — it's not optional.

**How to apply**: the existing [`src/logging.rs::init`](../../../src/logging.rs) takes a `BinaryKind` parameter (currently `Hmnd` or `Hmn`) and uses `tracing_subscriber::fmt().try_init()` — which silently no-ops if a subscriber is already installed (`init_is_idempotent_within_a_process` test confirms). The fmt builder defaults to stdout. To support stderr-only for the new `hmn mcp` mode, add a `BinaryKind::HmnMcp` variant whose branch in `init`'s fmt-builder chain calls `.with_writer(std::io::stderr).with_ansi(false)` before `.try_init()`. In `src/bin/hmn.rs::main()` (line 27 today), select the variant based on the parsed `cli.command` *before* calling `logging::init` — so the MCP-mode subscriber is the first one installed.

```rust
// In src/bin/hmn.rs::main, after `Cli::parse()` and `Config::load`:
let kind = match &cli.command {
    Command::Mcp => BinaryKind::HmnMcp,
    _ => BinaryKind::Hmn,
};
if let Err(e) = logging::init(&config.logging, cli.verbose, kind) {
    eprintln!("hmn: error: {e:#}");
    return ExitCode::from(1);
}
```

The verbose-flag bumping behavior should match `BinaryKind::Hmn`'s existing logic (the `compose_filter` branch in `src/logging.rs:53-67`); only the writer differs. The agent picks the cleanest exact shape at task time — likely a small refactor inside `init` to share the filter-composition path with `Hmn` while branching only on the fmt-builder writer/ansi config.

**`with_ansi(false)`**: also recommended — ANSI color codes in stderr aren't a protocol violation but are noise for non-TTY parents (which is every MCP host). Match the calculator example.

**How to apply**: see Task 8.3. The wiring change is in `run_mcp_stdio()` at the top of the function body, before the DaemonClient or rmcp server is constructed.

**References**: rmcp 1.5.0 upstream `examples/servers/src/calculator_stdio.rs` (`with_writer(std::io::stderr).with_ansi(false)`); `src/logging.rs::init` (existing logger — verify at Task 8.3 time whether stderr redirection is already supported or needs a small extension).

---

## Tasks (ordered, each independently mergeable)

Five tasks. Each landing as its own commit per the round-1/round-2 convention (one task = one commit; per-commit results comment includes the SHA).

### Task 8.1 — `rmcp` dependency + `JsonSchema` derives on `src/api/types.rs`

**Files**:
- `Cargo.toml` (extend) — add the `rmcp` dependency:
  ```toml
  rmcp = { version = "1.5", features = ["server", "transport-io", "macros", "schemars"] }
  ```
  The agent verifies at task time:
  1. The exact `1.5.x` patch version on crates.io (`cargo search rmcp` or the [crates.io](https://crates.io/crates/rmcp) page); pin to the latest 1.5.x stable.
  2. Whether `schemars::JsonSchema` is re-exported as `rmcp::schemars::JsonSchema` (per the upstream calculator example) or if a direct `schemars` dependency is required. If the latter, add `schemars = "1.0"` (or whichever version rmcp expects) to `[dependencies]`. Document the verified path in the task's results comment.
  3. That `transport-io` is the correct feature flag for stdio (the upstream README says `transport-io`; the rmcp 1.5.0 docs on docs.rs may use a slightly different name — verify against the actual feature list).
- `Cargo.lock` (auto-extended by `cargo build`).
- `src/api/types.rs` (extend) — add `#[derive(schemars::JsonSchema)]` (or the verified upstream path) to:
  - `FilesystemQueryJson` (line 3) — input
  - `ContentQueryJson` (line 31) — input
  - `SemanticQueryJson` (line 73) — input
  - `FilesystemSearchResponse` (line 16), `FilesystemResultJson` (line 22) — output (typed clients consume)
  - `ContentSearchResponse` (line 53), `ContentResultJson` (line 58), `ContentMatchJson` (line 67) — output
  - `SemanticSearchResponse` (line 84), `SemanticResultJson` (line 91) — output
  - `ErrorEnvelope` (line 121), `ErrorBody` (line 126) — for the synthesized `daemon_unreachable` envelope and any structured_error consumers

  Add `#[schemars(description = "...")]` per field on the **input** types only. Drafted descriptions:

  ```rust
  // FilesystemQueryJson
  #[schemars(description = "Vault-relative path prefix to scope results to a subdirectory (e.g. \"notes/databases/\"). Trailing `/` is normalized; absolute paths and `..` segments are rejected.")]
  pub prefix: Option<String>,
  #[schemars(description = "Glob pattern over vault paths (e.g. \"**/*.md\"). Single-pattern only; v0 does not support multi-pattern unions.")]
  pub glob: Option<String>,
  #[schemars(description = "Maximum directory depth to descend, relative to `prefix` (or vault root if no prefix). Unbounded if omitted.")]
  pub max_depth: Option<usize>,
  #[schemars(description = "Maximum number of results. Defaults to 100; results beyond this are truncated and the response carries `truncated: true`.")]
  pub limit: Option<usize>,

  // ContentQueryJson
  #[schemars(description = "Substring or regex to match against file contents. ASCII case-insensitive by default; see `case_sensitive` and `regex`.")]
  pub query: String,
  #[schemars(description = "If true, `query` is interpreted as a Rust regex pattern (anchors, character classes, etc.). Catastrophic backtracking is not possible — Rust's `regex` crate is linear-time. When true, `case_sensitive` is ignored; embed `(?i)` in the pattern instead.")]
  pub regex: bool,
  #[schemars(description = "If true, match query case-sensitively. Ignored when `regex` is true.")]
  pub case_sensitive: bool,
  #[schemars(description = "Vault-relative path prefix to scope results to a subdirectory.")]
  pub prefix: Option<String>,
  #[schemars(description = "If true, response includes per-line match details for each file (line number + matching text). Defaults to true.")]
  pub include_matches: bool,
  #[schemars(description = "Maximum match details returned per file when `include_matches` is true. Defaults to 5; `match_count` always reports the full match count.")]
  pub max_matches_per_file: Option<usize>,
  #[schemars(description = "Maximum number of result files. Defaults to 100.")]
  pub limit: Option<usize>,

  // SemanticQueryJson
  #[schemars(description = "Natural-language query. Embedded via the daemon's configured embedding service and compared against indexed chunk vectors by cosine similarity.")]
  pub query: String,
  #[schemars(description = "Vault-relative path prefix to scope results to a subdirectory.")]
  pub prefix: Option<String>,
  #[schemars(description = "Maximum number of result chunks. Defaults to 100.")]
  pub limit: Option<usize>,
  #[schemars(description = "Filter results to those whose cosine similarity score is >= this value, in [0.0, 1.0]. Out-of-range values are clamped. Defaults to 0.0 (no filter).")]
  pub min_similarity: Option<f32>,
  ```

  Description prose may be tightened at task time; the agent's editorial latitude is to keep them spec-faithful.

- `src/lib.rs` (touch — likely no change) — `pub mod api;` is already there.

**Tests in `src/api/types.rs::tests`** (new file scope inside `types.rs` — the existing module has no tests today; add a `#[cfg(test)] mod tests {}` block):
- `filesystem_query_json_schema_has_expected_properties` — `let s = schemars::schema_for!(FilesystemQueryJson);` → assert the JSON has `properties.prefix`, `properties.glob`, `properties.max_depth`, `properties.limit`. Assert each `description` is non-empty.
- `content_query_json_schema_has_query_required_others_optional` — schema's `required` array contains `"query"` only; the others are not required.
- `semantic_query_json_schema_has_min_similarity` — assert `properties.min_similarity` exists.
- `filesystem_search_response_json_schema_serializes` — sanity-check the response derive: a `schema_for!` call doesn't panic and produces a non-empty schema.

**What lands**:
- `rmcp` and (if needed) `schemars` are on the dependency graph.
- The existing wire shapes carry `JsonSchema` derives. No runtime behavior change; pure type-level addition.
- Per-field descriptions exist for the three request types — agent ergonomics is in place for Task 8.2's tool wiring.

**Why a separate task**: dependency additions and pure-type derive additions are the lowest-risk change shape; isolating from the MCP-server-construction task (8.2) keeps each commit's surface narrow. The agent's verification of upstream rmcp syntax (the `schemars` re-export path, the feature flags, the version pin) is the load-bearing thing here; doing it before any MCP code is written prevents a Task 8.2 rewrite if the verification surfaces drift.

**Risk: medium.**
- *Why medium*: new external dependency; the upstream-syntax verification (rmcp version, schemars re-export path, feature flags) is the load-bearing surface. The `schemars` crate has had a recent major version bump (1.0); whether rmcp 1.5.0 pins to schemars 1.x or 0.8 affects the import path and any type-level differences.
- *Mitigation*: tests assert each schema-generation property explicitly. The agent verifies the upstream feature-flag list against the rmcp 1.5.0 docs at task start. If the feature names differ from the workplan literal, the agent corrects and notes the verified syntax in the task's results comment. Forward note to Task 8.2 if the verified `schemars` import path is not `rmcp::schemars::JsonSchema`.

### Task 8.2 — `src/mcp/` module + `HypomnemaMcpServer` with three tools wired to `DaemonClient`

**Files**:
- `src/mcp/mod.rs` (new) — module entry point. Exports:
  ```rust
  mod server;
  pub use server::{HypomnemaMcpServer, daemon_unreachable_envelope};
  ```
  Plus a `pub async fn serve_stdio(server: HypomnemaMcpServer) -> Result<()>` that calls `server.serve(rmcp::transport::stdio()).await?` and `service.waiting().await?` per the upstream calculator example. (The exact entry path — `rmcp::ServiceExt::serve` vs `rmcp::serve_server` — is verified at task time against rmcp 1.5.0; the calculator example uses `service.serve(stdio()).await?` via the `ServiceExt` import.)

- `src/mcp/server.rs` (new) — the load-bearing module. Implements:
  ```rust
  use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};
  use rmcp::model::CallToolResult;
  use serde_json::json;

  use crate::api::types::{
      ContentQueryJson, FilesystemQueryJson, SemanticQueryJson,
  };
  use crate::client::DaemonClient;

  #[derive(Clone)]
  pub struct HypomnemaMcpServer {
      pub client: DaemonClient,
  }

  #[tool_router(server_handler)]
  impl HypomnemaMcpServer {
      #[tool(description = "List vault files matching a path prefix and/or glob. \
                            Cheapest of the three search modes; the typical first step \
                            when exploring an unfamiliar vault. See docs/specs/filesystem-search.md.")]
      async fn search_filesystem(
          &self,
          Parameters(input): Parameters<FilesystemQueryJson>,
      ) -> CallToolResult {
          match self.client.search_filesystem(&input).await {
              Ok(resp) => CallToolResult::structured(
                  serde_json::to_value(resp).expect("response is JSON-serializable")
              ),
              Err(err) => CallToolResult::structured_error(envelope_from_anyhow(&self.client, &err)),
          }
      }

      #[tool(description = "Search file contents by substring or regex. Grep-shaped — \
                            answers \"which files contain this exact phrase?\". \
                            See docs/specs/content-search.md.")]
      async fn search_content(
          &self,
          Parameters(input): Parameters<ContentQueryJson>,
      ) -> CallToolResult {
          // mirror shape
      }

      #[tool(description = "Semantic search via cosine similarity over indexed chunk \
                            embeddings. Answers \"what in this vault is conceptually \
                            similar to this idea?\". See docs/specs/semantic-search.md.")]
      async fn search_semantic(
          &self,
          Parameters(input): Parameters<SemanticQueryJson>,
      ) -> CallToolResult {
          // mirror shape
      }
  }
  ```

  Plus the helpers:

  ```rust
  fn envelope_from_anyhow(client: &DaemonClient, err: &anyhow::Error) -> serde_json::Value {
      // Connect failure (daemon unreachable) — synthesize the envelope.
      if crate::client::is_connect_error(err) {
          return daemon_unreachable_envelope(client.base_url(), err);
      }
      // HTTP error envelope already passed through from decode_response —
      // err's Display is "<code>: <message>". Split on the first ": ".
      let display = format!("{err:#}");
      let (code, message) = match display.split_once(": ") {
          Some((c, m)) => (c.to_string(), m.to_string()),
          None => ("internal".to_string(), display),
      };
      json!({ "error": { "code": code, "message": message } })
  }

  pub fn daemon_unreachable_envelope(url: &str, err: &anyhow::Error) -> serde_json::Value {
      json!({
          "error": {
              "code": "daemon_unreachable",
              "message": format!("{} did not respond: {:#}", url, err),
          }
      })
  }
  ```

  The agent verifies at task time:
  1. Whether the rmcp 1.5.0 `#[tool_router(server_handler)]` macro accepts `async fn` directly or needs `#[async_trait]` framing. Upstream calculator uses sync `fn` returning `String`; our methods are `async fn` returning `CallToolResult`. The macro is type-generic but the framing may differ.
  2. Whether the `Parameters<T>` wrapper requires `T: schemars::JsonSchema + serde::DeserializeOwned + Send + Sync + 'static` or some narrower bound. The existing `*QueryJson` types satisfy `Deserialize`, `Clone`, `Default` (some), and after Task 8.1 also `JsonSchema`.
  3. Whether the tool method's return-type contract is `CallToolResult`, `Result<CallToolResult, McpError>`, or some `Into<CallToolResult>` — verify against rmcp 1.5.0 docs.
  4. Whether `tool_router(server_handler)` auto-implements `ServerHandler` or if a separate `#[tool_handler]` block is required (the upstream README mentioned both).
  5. The exact `rmcp::transport::stdio` import — verify against rmcp 1.5.0; the calculator example imports `rmcp::{ServiceExt, transport::stdio}` and calls `Calculator.serve(stdio()).await?`.

- `src/lib.rs` (extend) — add `pub mod mcp;`.

**Tests in `src/mcp/server.rs::tests`** (new):

The tests below run against an in-process mock HTTP server (a `tokio::net::TcpListener` + a tiny axum router or a hand-rolled hyper service). The mock returns canned responses for `/search/{filesystem,content,semantic}` and `/health`; per-test the canned responses vary to exercise success and each error class.

- `mcp_search_filesystem_round_trips_success` — mock returns 200 with a known `FilesystemSearchResponse`; the tool method returns `CallToolResult::structured(_)` whose JSON matches the response.
- `mcp_search_content_round_trips_success` — same for content.
- `mcp_search_semantic_round_trips_success` — same for semantic.
- `mcp_search_semantic_propagates_embedding_unavailable_envelope` — mock returns 503 with `{"error":{"code":"embedding_unavailable","message":"..."}}`; tool method returns `CallToolResult::structured_error` whose JSON has `error.code == "embedding_unavailable"`.
- `mcp_search_filesystem_propagates_invalid_glob_envelope` — mock returns 400 `invalid_glob`; assert structured_error with `error.code == "invalid_glob"`.
- `mcp_search_filesystem_synthesizes_daemon_unreachable_for_connect_error` — bind a TCP listener, drop it (port now closed), construct `HypomnemaMcpServer` against that URL, call `search_filesystem`; assert structured_error with `error.code == "daemon_unreachable"` and the message includes the configured URL.
- `daemon_unreachable_envelope_shape` — pure-helper test: `daemon_unreachable_envelope("http://x.invalid", &anyhow!("connection refused"))` produces the expected JSON shape.

**What lands**:
- A new `src/mcp/` module with the three tools wired to `DaemonClient`.
- Error mapping from HTTP error envelopes to MCP `structured_error`.
- The `daemon_unreachable` synthesized error code is the new wire-level addition (Resolution F).

**Why a separate task**: this is the load-bearing pure-logic surface for step 8 (parallel to Task 7.2 in step 7). Splitting from the dep+derives task (8.1) keeps the bisect window tight; splitting from the binary-wiring task (8.3) keeps the `src/bin/hmnd.rs` change small and the unit tests focused on the tool/error contract.

**Risk: medium.**
- *Why medium*: the `tool_router`+`tool` macro shape is the new contract surface; the upstream-syntax verification (Resolution C, items in the verification list above) is the most error-prone surface. The DaemonClient wiring is well-trodden (the same client `hmn` uses; the same error decoding).
- *Mitigation*: the seven tests cover happy-path + each error class. The mock HTTP server pattern is reused from existing test patterns (e.g. `tests/embedding.rs` `StubServer`). Agent verifies upstream rmcp syntax at task start; corrections are noted in the results comment and forwarded to Task 8.3.

### Task 8.3 — `hmn mcp` subcommand wiring (CLI side) + non-stdio `mcp.transport` warning (daemon side) + stderr-only logging

This task touches **both binaries** by intent: `hmn` gains the `mcp` subcommand and the stdio MCP entry point; `hmnd` gains the non-stdio-warning emission. The split is a direct consequence of Resolution A — the stdio MCP shim is a CLI concern; the deferred socket transport (and its forward-compat config knob) is a daemon concern.

**Files**:
- `src/cli.rs` (extend) — add `Command::Mcp` variant. The current enum (line 30–38) has `Search { mode }` and `Status`. Insert:
  ```rust
  /// Serve the MCP surface over stdio against a running `hmnd` daemon.
  /// Intended to be invoked by MCP-capable agent hosts (Claude Code,
  /// Iris). Process exits when its parent (the host) closes stdin.
  Mcp,
  ```
  Add a clap-parse test alongside the existing `parses_status`, `parses_search_filesystem_with_query`, etc.:
  ```rust
  #[test]
  fn parses_mcp_subcommand() {
      let cli = Cli::try_parse_from(["hmn", "mcp"]).expect("parses");
      assert!(matches!(cli.command, Command::Mcp));
  }
  ```

- `src/bin/hmn.rs` (extend) — three changes:
  1. *Pre-init `BinaryKind` selection* (Resolution G). Before the existing `logging::init` call (line 27 today), select the kind based on the subcommand so the stderr-only subscriber is the first one installed (the existing `try_init()` is silently idempotent — first init wins):
     ```rust
     let kind = match &cli.command {
         Command::Mcp => BinaryKind::HmnMcp,
         _ => BinaryKind::Hmn,
     };
     if let Err(e) = logging::init(&config.logging, cli.verbose, kind) {
         eprintln!("hmn: error: {e:#}");
         return ExitCode::from(1);
     }
     ```
  2. *Dispatch arm* in the `match cli.command` (line 38–87 today). Add an `Mcp` arm:
     ```rust
     Command::Mcp => cmd_mcp(&config, cli.daemon_url.as_deref()).await,
     ```
  3. *New top-level function* `cmd_mcp`, parallel to `cmd_search_filesystem` / `cmd_search_content` / `cmd_search_semantic` / `cmd_status`:
     ```rust
     async fn cmd_mcp(config: &Config, override_url: Option<&str>) -> Result<()> {
         let client = DaemonClient::from_config(config, override_url)
             .context("constructing DaemonClient for mcp subcommand")?;
         let server = hypomnema::mcp::HypomnemaMcpServer { client };
         hypomnema::mcp::serve_stdio(server)
             .await
             .context("serving MCP over stdio")
     }
     ```
     The function honors `--daemon-url` / `HYPOMNEMA_DAEMON_URL` via `cli.daemon_url` (already wired by `Cli`'s global flags at `src/cli.rs:16-17`). The `--json` global flag is meaningless for MCP (the protocol is JSON by construction); not consulted.

  Note: `cmd_mcp` does **not** emit any output on stdout via `println!`, `print_json`, or any helper — the rmcp `serve()` call owns stdout exclusively from the moment it's invoked. The existing `is_connect_error` exit-code-4 mapping at the bottom of `main()` (line 92–98) does not apply to `cmd_mcp`'s success path: a connect error inside an MCP tool call is reported via `structured_error` to the host, not as a process exit code. If `serve_stdio` itself errors before any tool call (e.g. transport setup failure), the existing `eprintln!("hmn: error: ...")` + ExitCode::from(1) path applies as it does for any other subcommand.

- `src/bin/hmnd.rs` (extend, smaller change) — *only* the non-stdio `mcp.transport` warning emission, per Resolution D. No subcommand change to `hmnd`.

  Inside `run_daemon()` (around line 89, after `tracing::debug!(?config, "hmnd: full configuration")` and before the `Store::open` call at line 91), insert:
  ```rust
  if config.mcp.transport != "stdio" {
      tracing::warn!(
          configured = %config.mcp.transport,
          socket = %config.mcp.socket.0.display(),
          "mcp.transport = {:?} is not implemented in v0; only stdio via the `hmn mcp` \
           subcommand on the CLI binary is shipped. The socket file is NOT bound. \
           To use MCP, invoke `hmn mcp` from the agent host.",
          config.mcp.transport,
      );
  }
  ```
  No other `hmnd` change. The daemon does not link or reach into `src/mcp/` in v0 — see [Resolution F](#f-tool-result-content-shape--structured_content-vs-content-vs-both-and-where-rmcp-lives-in-the-dep-graph) for the dep-graph arrangement.

- `src/logging.rs` (touch — small extension per Resolution G).
  - Add a `BinaryKind::HmnMcp` variant alongside `Hmnd` and `Hmn` (line 9–13).
  - In `compose_filter` (line 43–68), the new variant shares `Hmn`'s filter-composition logic (both are CLI-side; the only difference is the writer/ansi config in `init`):
    ```rust
    BinaryKind::HmnMcp => {
        let bumped = level_str(bump(Level::WARN, verbose));
        format!("error,hypomnema={bumped},hmn={bumped}")
    }
    ```
  - In `init` (line 15–41), branch the fmt-builder on the variant. For `HmnMcp`, force stderr regardless of the json-format env-var (the MCP protocol is JSON anyway; json-formatted-tracing on stderr is fine). The cleanest shape is a small refactor of `init`'s body to handle three variants — agent's editorial latitude.

  ```rust
  // Sketch — exact shape at task agent's discretion:
  match binary {
      BinaryKind::HmnMcp => {
          let _ = tracing_subscriber::fmt()
              .with_env_filter(env_filter)
              .with_writer(std::io::stderr)
              .with_ansi(false)
              .try_init();
      }
      BinaryKind::Hmnd | BinaryKind::Hmn => {
          // existing stdout / json-format conditional from lines 29-38
      }
  }
  ```

- `docs/reference/cli.md` (no change here — full update in Task 8.5).

**Tests**:
- `src/cli.rs::tests` (extend) — the `parses_mcp_subcommand` test above.
- `src/logging.rs::tests` (extend) — add cases for the new variant:
  ```rust
  #[test]
  fn hmn_mcp_filter_matches_hmn() {
      let s = compose_filter(&default_cfg(), 0, BinaryKind::HmnMcp, None);
      assert_eq!(s, "error,hypomnema=warn,hmn=warn");
  }

  #[test]
  fn composed_directive_parses_for_hmn_mcp() {
      for v in 0u8..=3 {
          let directive = compose_filter(&default_cfg(), v, BinaryKind::HmnMcp, None);
          EnvFilter::try_new(&directive)
              .unwrap_or_else(|e| panic!("directive {directive:?} failed to parse: {e}"));
      }
  }
  ```
  Also extend the existing `composed_directive_parses_as_envfilter` and `bumped_directive_parses_as_envfilter` tests to include `BinaryKind::HmnMcp` in their `for binary in [...]` loops.

  The "writer is stderr" property is tested implicitly by manual smoke (the unit-test layer can't easily assert which writer is bound to a global subscriber).

**Manual smoke verification** (per the round-1/2 precedent — see step-7 retro § Notes "Manual smoke verification"):

The agent runs through three smoke paths and documents transcripts in the task's results comment.

1. *Healthy path*: a real `hmnd` is running against a temp vault with seeded files. Spawn `hmn mcp` from a shell with stdio piped to a small Python or `cargo run` driver. The driver issues:
   - `{"jsonrpc":"2.0","method":"initialize","params":{"protocolVersion":"...","capabilities":{},"clientInfo":{"name":"smoke","version":"0"}},"id":1}`
   - `{"jsonrpc":"2.0","method":"tools/list","params":{},"id":2}`
   - `{"jsonrpc":"2.0","method":"tools/call","params":{"name":"search_filesystem","arguments":{"glob":"**/*.md"}},"id":3}`

   Assert the response to `tools/list` contains three tools named `search_filesystem`, `search_content`, `search_semantic`, each with the expected description and `inputSchema`. Assert the `tools/call` response carries a `structured_content` JSON matching `FilesystemSearchResponse` shape. **Critically**: assert that `hmn`'s stderr contains the tracing logs (DaemonClient request lines, etc.) and stdout contains *only* MCP framing — no log lines leaking into stdout.

2. *Daemon-unreachable path*: with no `hmnd` running, spawn `hmn mcp` and issue a `tools/call`. Assert the response carries `structured_error` with `error.code == "daemon_unreachable"` and the URL appears in the message.

3. *Non-stdio mcp.transport warning path*: write a config with `mcp.transport = "socket"`, start `hmnd` (default mode), confirm the daemon's stderr (or log file) carries the `WARN`-level message about socket transport not being implemented; confirm the daemon does NOT crash and HTTP `/health` returns 200. *(This path exercises the `hmnd` side of the split task.)*

Smoke verification documented in the task's results comment, with transcripts.

**What lands**:
- `hmn mcp` subcommand exists and serves an MCP server over stdio.
- Stderr-only logging is enforced for the `hmn mcp` mode (`BinaryKind::HmnMcp`).
- Non-stdio `mcp.transport` config values produce a clear `WARN`-level log at `hmnd` startup but do not crash.

**Why a separate task**: this is the wiring task (parallel to Task 7.3 in step 7). Splitting from the pure-logic mcp module (8.2) keeps the bisect anchor tight; the manual smoke is the load-bearing safety net for the wiring layer; bundling it into the integration-tests task (8.4) would conflate the unit-test layer with the live-binary layer. The two-binary split inside this task (CLI subcommand vs. daemon warning) is a single conceptual change ("v0 ships stdio MCP; the daemon knows it") and ships as one commit.

**Risk: medium-high.**
- *Why medium-high*: the logging-writer split is the most error-prone surface (stdout corruption breaks MCP). The clap subcommand routing is mechanical but every wiring task in this project has surfaced at least one prose-accuracy slip (per the round-1 retro). The non-stdio `mcp.transport` warning placement (early enough that operators see it, late enough that startup tracing is initialized) is a small judgment call. The two-binary touch (both `src/bin/hmn.rs` and `src/bin/hmnd.rs`) doubles the wiring surface vs. step 7's single-binary tasks.
- *Mitigation*: manual smoke runs *all three* paths (not just the happy path) — the daemon-unreachable and warning paths catch wiring slips that unit tests miss. Stderr-only logging is verified in smoke (the smoke driver observes stderr separately and confirms it carries logs while stdout carries only MCP framing). Forward note to Task 8.4 if any logging-init pattern surfaces that the integration tests need to reuse.

### Task 8.4 — Integration tests against a live daemon and the `hmn mcp` subprocess

**Files**:
- `tests/mcp.rs` (new) — integration tests that:
  1. Spawn a live `hmnd` daemon against a temp vault (reuse `tests/cli.rs` and `tests/embedding.rs` `LiveDaemon` patterns; the daemon needs the existing stub embedding service infra from step 6).
  2. Spawn the `hmn mcp` binary as a subprocess (`std::process::Command::new(env!("CARGO_BIN_EXE_hmn")).arg("mcp").arg("--daemon-url").arg(&live_daemon.base_url).stdin(piped).stdout(piped).stderr(piped).spawn()`) configured against the live daemon's URL via the existing `--daemon-url` global flag.
  3. Pipe MCP protocol messages over stdin and read responses from stdout.

  Rather than pulling in a full MCP client crate (which would mean re-doing rmcp's client-side feature flag verification), the tests issue **JSON-RPC framed messages directly** over stdio per the MCP protocol's framing rules. The framing is simple — JSON-RPC 2.0 with newline-delimited JSON or Content-Length headers depending on the rmcp transport's framing. The agent verifies the rmcp 1.5.0 stdio framing at task time (the upstream `examples/clients/` dir is the reference) and writes a small test helper that handles framing, single-request/single-response.

  Suggested test cases:
  - `mcp_initialize_returns_server_info` — send `initialize`, assert response carries `serverInfo.name == "hypomnema"` (or whatever rmcp 1.5.0's server identification carries).
  - `mcp_tools_list_advertises_three_tools` — send `tools/list`, assert response's `tools` array has three entries with expected names.
  - `mcp_tools_list_includes_input_schemas_with_descriptions` — same, drill into `tools[0].inputSchema.properties.prefix.description` and assert it's non-empty (sanity check on Task 8.1's descriptions making it through the schema-generation pipeline).
  - `mcp_call_search_filesystem_returns_structured_content` — seed the vault with two `.md` files; wait for the watcher cycle; send `tools/call` for `search_filesystem` with `{"glob":"**/*.md"}`; assert the `structured_content` response carries an array of two paths.
  - `mcp_call_search_content_returns_structured_content` — seed a file with a known phrase; call `search_content` for that phrase; assert the structured_content carries the file's path with `match_count >= 1`.
  - `mcp_call_search_semantic_returns_structured_content_with_hint` — index a vault normally (stub embedder up, watcher cycle complete, chunks land); then truncate `chunks_vec` and `chunks` directly via SQL while leaving `files` populated (per step-7 workplan § Build-time amendments item 1's corrected hint-reproduction recipe); call `search_semantic`; assert structured_content has `hint == "semantic index is building"`.
  - `mcp_call_with_invalid_glob_returns_structured_error` — call `search_filesystem` with `{"glob":"[unterminated"}`; assert structured_error with `error.code == "invalid_glob"`.
  - `mcp_call_against_dead_daemon_returns_daemon_unreachable` — spawn `hmn mcp --daemon-url http://127.0.0.1:<closed-port>` (bind a TCP listener, drop it to obtain a closed port); call `search_filesystem`; assert structured_error with `error.code == "daemon_unreachable"`.
- `tests/embedding.rs` (no change) — reuse the existing stub-embedding-service pattern by spawning the LiveDaemon with a stub embedder. Or, more cleanly, share a small test-helper crate (out of scope for v0; the pattern of duplicate `spawn_live_daemon` across test files is acceptable per the existing `tests/cli.rs` precedent).

The test helper for MCP framing lives inline in `tests/mcp.rs` (no new shared `tests/common/` mod; the project doesn't currently use one).

**Anti-flake budget**: 3× consecutive flake-check clean per the round-1/round-2 precedent (matching Task 6.6 and Task 7.5). Subprocess + watcher debounce + MCP framing introduces three timing axes; flakes on the watcher debounce window are the most likely failure mode (mirroring step 5's filesystem/content integration tests). The agent runs `cargo test --test mcp` three times locally without flakes before reporting green.

**What lands**:
- Eight integration tests covering MCP protocol round-trips against `hmn mcp`.
- The end-to-end MCP path is exercised against a real subprocess, not just unit tests.
- The structured-content and structured-error response shapes are validated at the wire level.

**Why a separate task**: integration test surface deserves its own commit and bisect anchor (parallel to Task 7.5 precedent). The MCP framing helper is the new test-side machinery; isolating from the wiring task keeps each commit's surface clean.

**Risk: medium.**
- *Why medium*: subprocess timing + MCP framing + watcher debounce are three timing axes; the rmcp 1.5.0 stdio framing format (newline-delimited JSON vs Content-Length headers) is the load-bearing protocol verification. The 3× flake-check budget is the safety net.
- *Mitigation*: the helper for MCP framing is small and self-contained; verified against rmcp 1.5.0 upstream at task start. Tests use the existing `LiveDaemon` and stub-embedding patterns. Forward note to Task 8.5 if the rmcp framing format is unusual or has any quirks worth recording for the agent-integration test.

### Task 8.5 — Reference docs + ADR amendments + manual agent-integration test (round-2 shipping gate)

**Files**:
- `docs/reference/cli.md` (substantial restructure) — the `--mcp-stdio` flag prose currently lives under the `hmnd` section (lines 54, 62, 77). It must be **removed from the `hmnd` section entirely** and **added as a new `hmn mcp` subcommand under the `hmn` section**. Concrete edits:
  - Line 54 (`hmnd` Usage): remove `[--mcp-stdio]` from the synopsis. The `hmnd` default-mode usage becomes `hmnd [--rescan]` only.
  - Line 62 (`hmnd` Options table): remove the `--mcp-stdio` row entirely.
  - Line 77 (`hmnd` Example block): remove the `hmnd --mcp-stdio` example.
  - In the **`hmn`** section (the second half of cli.md), add a new subcommand subsection alongside `hmn search …` and `hmn status`:
    ```
    #### `hmn mcp`

    Serve the MCP surface over stdio. Intended for MCP-capable agent
    hosts (Claude Code, Iris) that launch the binary as a child process
    and communicate via stdio. Translates MCP tool calls into HTTP
    requests against a running `hmnd`.

    **Usage**: `hmn mcp [--daemon-url URL]`

    **Behavior**:
    - Reads MCP messages from stdin; writes MCP messages to stdout.
    - All tracing/log output goes to stderr (stdout is reserved for
      the MCP transport).
    - Tool calls are forwarded to `hmnd` over HTTP using the same
      `DaemonClient` machinery as `hmn search …`.
    - If `hmnd` is not reachable, tool calls return a structured MCP
      error with `error.code = "daemon_unreachable"`.
    - Process exits when stdin is closed by the parent.

    **Example MCP host configuration**:
    ```json
    { "command": "hmn", "args": ["mcp"] }
    ```
    ```
- `docs/reference/configuration.md` (touch) — line 113–114 (the `[mcp]` table) — replace the existing `transport` and `socket` row descriptions to clarify v0 state: "v0 implements only `transport = \"stdio\"` via the **`hmn mcp` subcommand on the CLI binary**. `transport = \"socket\"` parses and validates but is not yet bound; the `hmnd` daemon emits a `WARN` log at startup when set to a non-`stdio` value. See [step-08 workplan § Resolution D](../roadmap/step-08-workplan.md#d-connection-lifecycle-stdio-process-per-connection-vs-socket-long-lived) for the deferral rationale and the binary-placement reasoning."
- `docs/architecture/overview.md` (touch) — line 60 (Consumers table — agents row) and line 110 (Control-plane API — Axum + rmcp) — clarify the rmcp mention to indicate that v0 ships MCP via `hmn mcp` (CLI-side stdio shim) and that the daemon's socket-MCP transport is deferred. The Search API row (line 114) gets a similar forward-compat note.
- `docs/decisions/0008-two-binary-daemon-plus-cli.md` § Amendments (line 64) — add a substantial amendment that resolves the line-29-vs-line-40 inconsistency this workplan settled:
  ```markdown
  ### 2026-MM-DD: MCP-over-stdio lives on `hmn`, not `hmnd`

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
  for the full reasoning. The two-binary shape and the "thin client"
  framing of the ADR are otherwise preserved — this amendment clarifies
  *which* thin client owns the MCP surface, not whether the project
  uses thin clients.
  ```
- `docs/specs/filesystem-search.md`, `docs/specs/content-search.md`, `docs/specs/semantic-search.md` (touch) — verify each spec's "Behavior" section reads correctly for both HTTP and MCP transports (the existing prose says "via HTTP or MCP" or similar; round-2 step 7 left this language in place for semantic). No structural changes expected; the spec wire shapes are by construction the same across transports per ADR-0004 and Resolution C.
- New ADR (likely **ADR-0012** — verify next ADR number against `docs/decisions/`) — *MCP transport: stdio on `hmn` in v0; socket on `hmnd` deferred*. Captures Resolution D's deferral and the forward-compat decision (Resolution E) for socket auth via filesystem permissions, plus Resolution A's binary placement. The ADR is short (one page) — context (the deferred decisions from roadmap-2.md and the ADR-0008 inconsistency), decision (stdio on `hmn`, socket on `hmnd` deferred), consequences (positive: each transport in the binary that matches its lifetime, focused shipping gate; negative: agents need to read both ADR-0008's amendment and this ADR to fully understand the v0 surface; neutral: `mcp.transport` config knob continues to parse). Cross-references ADR-0004, ADR-0008.
- `notes/project-planning-workflow-notes.md` (extend at boundary) — Task 8.5 doesn't write the retro itself, but the boundary ritual (post-shipping, before the round-2 retro) appends:
  - Step 8 retrospective (per the retro template; coordinator + human collaborate per the playbook).
  - End-of-round-2 retrospective (after the per-step retro). The end-of-round retro answers: *did the roadmap → workplan → build cadence still work at higher risk?* and *what surprised us about embedding/sqlite-vec/MCP that the docs did not predict?* (per `roadmap-2.md` § "After step 8" line 98).
- `notes/roadmap/archive/roadmap-2.md` (touch) — mark step 8 as `**Status**: shipped <date>`; add the link to step-08-workplan.md and the relevant retro section.

**Manual agent-integration test (the round-2 shipping gate's load-bearing test — criterion 2 of `roadmap-2.md` § Step 8)**:

This test runs at boundary, after the unit and integration tests are green and before the retro is written. It is the load-bearing verification that step 8's wire shape works against a real MCP host.

**Procedure**:

1. Build `hmnd` and `hmn` in release mode (or the dev-shell-equivalent). Confirm the binaries are at known paths.
2. Configure a test vault with three to five seeded `.md` files (suggested: copy a subset of `docs/specs/*.md` into a temp directory). Start a long-running `hmnd` against this vault.
3. **Configure Claude Code's MCP client to invoke `hmn mcp`**:
   ```json
   {
     "mcpServers": {
       "hypomnema": {
         "command": "/path/to/hmn",
         "args": ["mcp"]
       }
     }
   }
   ```
   (Exact configuration shape verified against current Claude Code docs at test time.)
4. Restart Claude Code (or hot-reload the MCP server connection per its UX).
5. In Claude Code, observe the three Hypomnema tools appearing (`search_filesystem`, `search_content`, `search_semantic`). Confirm tool descriptions and parameter schemas appear correctly.
6. **Exercise each tool** through Claude Code's tool-call UX:
   - `search_filesystem` with `{"glob": "**/*.md"}` — expect a list of the seeded files
   - `search_content` with `{"query": "<a known phrase from one of the files>"}` — expect the matching file in results
   - `search_semantic` with `{"query": "<a paraphrase of a concept in one of the files>"}` — expect a relevant chunk
7. **Exercise an error path**: stop the long-running `hmnd`. Issue a tool call. Expect Claude Code to render a `daemon_unreachable` error.
8. Record transcripts (Claude Code's tool-call panel screenshots are sufficient; or the raw MCP protocol exchanges if the inspector is available) and attach to Task 8.5's results comment.

**Optional second host (Iris)**: if the Iris adapter from `docs/hypomnema-handoff.md` § "Iris-side integration" is in a state where it can be configured against an MCP server, run the same test against Iris. v0 doesn't require Iris coverage — Claude Code is the primary target — but a second host validates that the MCP wire shape isn't accidentally Claude-Code-specific.

**Pass criteria** (all must hold):
- All three tools list correctly with descriptions and schemas.
- Each tool round-trips through Claude Code with a structured result.
- The error path (daemon down) produces a structured error visible in Claude Code's UX with `daemon_unreachable` as the code.
- No stdout-pollution incidents (no malformed-message errors from Claude Code).

**Failure handling**: if any criterion fails, the failure is treated as a `coordinator-only` soft flag at minimum. The decision rule from step 6's Task 6.4r1 precedent (act-now vs defer-to-boundary) applies: if the failure is a real bug with a small, well-bounded fix and no downstream task covers it, the coordinator may spawn a surgical follow-up Task 8.Nr1. Otherwise it routes to the boundary retro.

**What lands**:
- All documentation reflects what step 8 actually built (subcommand, stderr logging, daemon_unreachable error, stdio-only transport).
- The new ADR (likely 0012) records the stdio-shipped/socket-deferred decision.
- The agent-integration manual test passes; transcripts attached.
- Step 8 retro and end-of-round retro append to `notes/project-planning-workflow-notes.md`.
- `roadmap-2.md` § Step 8 marked shipped.

**Why a separate task**: doc-only by design (parallel to Tasks 5.8, 6.7, 7.6); lands at the boundary so any soft-flag-to-coordinator from earlier tasks (e.g. wording corrections or rmcp-syntax verifications surfaced during 8.1–8.4) can be incorporated. The manual agent-integration test is bundled here because it is the round-shipping-gate's load-bearing verification — it wouldn't make sense to ship in any earlier task, and it's structurally documentation-shaped (transcripts + a result comment).

**Risk: low for docs; medium for the agent test.**
- *Doc risk*: low. Sync docs to code surface.
- *Agent-test risk*: medium. New external host dependency (Claude Code's MCP UX); rmcp 1.5.0 protocol-version compatibility with whatever Claude Code's MCP client speaks; the structured_content rendering UX is at Claude Code's discretion (it might render structured JSON differently than expected, but as long as the data is there the wire-level test passes). Mitigation: the integration tests in Task 8.4 already validate the wire-level behavior; the agent test is verifying the host's *consumption* of that behavior, not the daemon's production of it.

---

## Test strategy

**Unit tests** (`#[cfg(test)]`):
- `src/api/types.rs::tests` — ~4 cases for the `JsonSchema` derives (Task 8.1).
- `src/mcp/server.rs::tests` — 7 cases for the tool methods + error mapping (Task 8.2): three success paths, two HTTP-error-envelope-pass-through, one connect-error-synthesis, one helper-shape.
- `src/cli.rs::tests` — 1 case for the `hmn mcp` clap subcommand parse (Task 8.3).
- `src/logging.rs::tests` — 2 cases for the new `BinaryKind::HmnMcp` variant (filter shape + envfilter parse), plus extension of the existing `composed_directive_parses_*` loops to include the new variant (Task 8.3).

**Integration tests** (`tests/`):
- `tests/mcp.rs` — 8 cases for the live-`hmnd`-plus-`hmn-mcp`-subprocess round trips (Task 8.4). Reuses the existing `LiveDaemon` pattern from `tests/cli.rs` / `tests/embedding.rs`. 3× consecutive flake-check budget at task close.

**Manual smoke** (Task 8.3):
- Three paths exercised via a small driver against `hmn mcp` (or against `hmnd` directly for the warning path): healthy, daemon-unreachable, non-stdio-mcp-transport-warning. Documented in the task's results comment with transcripts.

**Manual agent-integration** (Task 8.5):
- Configure Claude Code's MCP client to invoke `hmn mcp`; exercise each of the three tools and the error path through Claude Code's UX. Transcripts attached. This is the round-2 shipping gate's load-bearing test.

**Lint and format**:
- `cargo clippy --all-targets -- -D warnings` clean.
- `cargo fmt --check` clean.

**Cross-platform notes**:
- Stdio MCP works identically on macOS, Linux, and Windows (no Unix-specific syscalls in v0).
- The deferred socket transport would be Unix-only (no `tokio::net::UnixListener` on Windows in stable tokio); when it ships, the `mcp.socket` config will be Unix-only and the parse should reject it on Windows or no-op silently. v0 doesn't ship the socket binding so this isn't a concern.
- The rmcp dependency must build on macOS, Linux (the project's two reference platforms per the existing `flake.nix` shape) and Windows (the project's third declared platform per `docs/reference/configuration.md` line 53–55). Verify at Task 8.1 time that the `transport-io` feature flag is platform-portable.

**Anti-flake rules** (carried forward from round 1 and round 2):
- Do **not** introduce a polling-loop helper that hides timing in `tests/mcp.rs` — flakes on a non-deterministic boundary are signal, per the step-3 retro and steps 6/7 precedent.
- Subprocess teardown in `tests/mcp.rs` must explicitly close stdin (causes the child to exit cleanly per Resolution G's stdio-handling contract); waiting on the child with a timeout is the safety net for any hang.
- The watcher-debounce window in any seed-and-search test follows the existing `tests/embedding.rs` pattern of a poll-the-API-shape timeout (e.g. retry the `search_filesystem` call until results show up or 5 seconds elapse — the wait is on observable state, not a fixed sleep).

---

## Definition of done

- [ ] `rmcp` (and `schemars` if the upstream re-export path doesn't suffice) is on the dependency graph at the verified version (Task 8.1).
- [ ] `JsonSchema` derives are on the three `*QueryJson` request types and the response/per-result types in `src/api/types.rs` (Task 8.1).
- [ ] Per-field `#[schemars(description = "...")]` annotations exist on the three request types' fields (Task 8.1).
- [ ] `src/mcp/` module exists with `HypomnemaMcpServer` struct holding a `DaemonClient` and three `#[tool]` methods named `search_filesystem`, `search_content`, `search_semantic` (Task 8.2).
- [ ] Tool methods return `CallToolResult::structured(_)` on HTTP success and `CallToolResult::structured_error(_)` on HTTP error (Task 8.2).
- [ ] Connect failures (daemon not reachable) map to a synthesized `daemon_unreachable` error envelope (Task 8.2).
- [ ] `hmn mcp` subcommand parses correctly (in `src/cli.rs`) and invokes `serve_stdio(server)` over stdio from `src/bin/hmn.rs::cmd_mcp` (Task 8.3).
- [ ] Stderr-only logging is enforced for the `hmn mcp` mode via `BinaryKind::HmnMcp` in `src/logging.rs` (Task 8.3).
- [ ] `mcp.transport != "stdio"` in the `hmnd` daemon produces a `WARN`-level log at startup but does not crash (Task 8.3).
- [ ] Manual smoke verification documented in Task 8.3's results comment with transcripts (healthy, daemon-unreachable, non-stdio-warning paths).
- [ ] `cargo test --test mcp` passes 3× consecutively without flakes (Task 8.4).
- [ ] All eight integration cases in `tests/mcp.rs` cover the documented MCP wire shapes (Task 8.4).
- [ ] All step-1 through step-7 tests still pass (no regression on filesystem/content/semantic search, HTTP plumbing, indexer, schema, outbox, watcher).
- [ ] `cargo clippy --all-targets -- -D warnings` clean; `cargo fmt --check` clean.
- [ ] `docs/reference/cli.md` reflects the `hmn mcp` subcommand (moved out of the `hmnd` section into the `hmn` section; Task 8.5).
- [ ] `docs/reference/configuration.md` § `[mcp]` notes the v0-stdio-via-`hmn`-only state with link to this workplan (Task 8.5).
- [ ] `docs/decisions/0008-two-binary-daemon-plus-cli.md` § Amendments records both the subcommand resolution AND the binary-placement (`hmn` not `hmnd`) resolution that settles the line-29-vs-line-40 inconsistency (Task 8.5).
- [ ] New ADR (likely `0012-mcp-transport-stdio-v0.md`) records the stdio-on-`hmn` shipped / socket-on-`hmnd` deferred decision (Task 8.5).
- [ ] Step 8 retrospective and end-of-round-2 retrospective appended to `notes/project-planning-workflow-notes.md` (per the retro template; coordinator + human collaborate per the playbook).
- [ ] `notes/roadmap/archive/roadmap-2.md` § Step 8 marked `**Status**: shipped <date>`.
- [ ] Manual agent-integration test (Claude Code via MCP) passes per the Task 8.5 procedure; transcripts attached.
- [ ] **Round-2 milestone tag created in git** (likely `v0.1.0` or `v0`, per `roadmap-2.md` § "After step 8" line 96). The exact tag name is decided at boundary.
- [ ] No fall-out resolutions or in-build TBDs left undocumented (workplan and code agree at the end; soft flags routed to coordinator at boundary).

---

## Cross-references

**Skills (load-bearing)**:
- *No in-tree skill covers `rmcp` / MCP transport.* The rmcp upstream docs at https://docs.rs/rmcp/latest/rmcp/ and the upstream `examples/servers/` directory are the agent's references. If `rmcp`-shaped patterns become load-bearing for round 3 or beyond, write a skill at the round-2 boundary per the step-7 retro recommendation.
- [`.claude/skills/rusqlite-in-async/`](../../../.claude/skills/rusqlite-in-async/SKILL.md) — not directly relevant to step 8 (the MCP shim doesn't touch SQLite — that's the daemon's job, accessed via HTTP). Carrying forward as a project-wide standing reference.

**ADRs**:
- [`docs/decisions/0001-adopt-layered-documentation-system.md`](../../../docs/decisions/0001-adopt-layered-documentation-system.md) — the LDS framing under which the new ADR-0012 (Resolution D) lands.
- [`docs/decisions/0004-three-search-modes-as-peers.md`](../../../docs/decisions/0004-three-search-modes-as-peers.md) — the canonical names for the three tools (Resolution B); the canonical claim that HTTP and MCP are peers (Resolutions C and F).
- [`docs/decisions/0005-local-everything.md`](../../../docs/decisions/0005-local-everything.md) — the local-everything trust boundary that justifies mode-0600 socket auth (Resolution E, deferred).
- [`docs/decisions/0008-two-binary-daemon-plus-cli.md`](../../../docs/decisions/0008-two-binary-daemon-plus-cli.md) — the daemon-shape framing; amended in Task 8.5 with the subcommand resolution (Resolution A).
- [`docs/decisions/0011-vault-management-on-hmn.md`](../../../docs/decisions/0011-vault-management-on-hmn.md) — extends ADR-0008's `hmn`-as-CLI claim; informs the round-3 forward-compat thinking on what the *next* MCP tool surface looks like.

**Specs**:
- [`docs/specs/filesystem-search.md`](../../../docs/specs/filesystem-search.md), [`docs/specs/content-search.md`](../../../docs/specs/content-search.md), [`docs/specs/semantic-search.md`](../../../docs/specs/semantic-search.md) — the three response/error wire shapes the MCP tools round-trip. No structural changes expected; minor doc updates possible at Task 8.5.

**Prior workplans / retros**:
- [`notes/roadmap/archive/step-07-workplan.md`](./step-07-workplan.md) — the immediately prior step. The `search_*` HTTP handlers this MCP shim wraps; the `embedding_unavailable` error envelope (step-7 Resolution E) flows through unchanged. Step 7's retro recommended a workplan-time decision on flag shape and tool names (both pulled forward here in Resolutions A and B).
- [`notes/roadmap/archive/step-06-workplan.md`](./step-06-workplan.md) — the chunking + embedding substrate. Not directly touched by step 8; cited for context.
- [`notes/roadmap/archive/step-05-workplan.md`](./step-05-workplan.md) — the HTTP/CLI/error-envelope shape this step's MCP layer mirrors. The error-envelope token-prefix mapping in `src/api/error.rs` (Resolution F's pass-through) is the load-bearing precedent.
- [`notes/roadmap/archive/step-01-workplan.md`](./step-01-workplan.md) — line 82–84 establishes the existing `[mcp]` config block. Resolution D's "non-stdio transport produces a warning" preserves the config knob's parse-and-validate behavior unchanged.
- [`notes/project-planning-workflow-notes.md`](../../project-planning-workflow-notes.md) § Step 6 retro and § Step 7 retro — three patterns feeding into this step: (a) coordinator-spawned in-build follow-up for cross-task design tensions (act-now decision rule); (b) manual smoke verification on medium-high-risk wiring tasks (Task 8.3 here); (c) workplan-prose accuracy heuristic at ~1000-line threshold (this workplan is at or above the threshold; full self-review ran).

**Roadmap and tech-stack**:
- [`notes/roadmap/archive/roadmap-2.md`](./roadmap-2.md) § Step 8 — the contract this workplan resolves. Shipping criterion 4 is rescoped at workplan time per Resolution D; see [§ Shipping criteria (rescoped)](#shipping-criteria-rescoped) below for the full rescoped list.
- [`docs/implementation/tech-stack.md`](../../../docs/implementation/tech-stack.md) — semantic search composes the same SQLite + sqlite-vec stack steps 1–6 built; the MCP wrapper composes the HTTP surface step 5/7 built. No new top-level components.

---

## Shipping criteria (rescoped)

The roadmap's [§ Step 8](./roadmap-2.md#step-8--mcp-wrapper-round-shipping-gate) lists five shipping criteria. Four are unchanged; the fourth is rescoped at workplan time per Resolution D. The full rescoped list:

1. **`hmn mcp`** *(rescoped from `hmnd --mcp-stdio` per Resolution A — both the flag-vs-subcommand half and the binary-placement half)* starts an MCP server over stdio with the three tools advertised.
2. **Claude Code (or another MCP-capable agent)** can invoke each tool against the running daemon and get back results that match the spec response shapes. *(Unchanged — load-bearing manual test in Task 8.5.)*
3. **The `vault` forward-compat field** passes through MCP responses identically to HTTP responses (omitted in v0; serde shape preserved). *(Unchanged — automatic given Resolution F: `serde_json::to_value(response)` round-trips the field.)*
4. *(Rescoped.)* **The `mcp.transport` config knob continues to parse and validate; non-`stdio` values produce a clear `WARN`-level log at daemon startup but do not crash. The Unix-socket transport is not bound; it is documented as a follow-on workplan with Resolution E (filesystem-permissions auth) recorded forward-compat.** *(Original criterion 4: "Unix-socket transport is wired (per the existing `mcp.transport = "socket"` config) but stdio is the default." Rescope rationale in Resolution D.)*
5. **The MCP error mapping mirrors HTTP's error-envelope codes.** *(Unchanged — Resolution F pass-through; the new `daemon_unreachable` code is a structural addition for connect failures, not a deviation from HTTP's codes.)*

The rescope is a workplan-time decision; the human approves at workplan-review time. The retro records whether the rescope was the right call (i.e. did any consumer want socket-MCP sooner than round 3?).

---

## Out of scope (will not appear in this PR)

- **Unix-socket MCP transport (in `hmnd`)** — Resolution D defers. The daemon does not bind any socket in v0; `mcp.transport = "socket"` produces a startup warning. Future workplan (likely round 3 or a v0+ follow-on) implements `tokio::net::UnixListener` accept-loop with mode-0600 permissions per Resolution E. The socket transport will live in `hmnd` (long-lived listener) — preserving the binary-placement split: stdio on `hmn`, socket on `hmnd`.
- **Vault-management MCP tools** — round 3 work per ADR-0011. Out of v0; the workplan-time decision on which subset ships is round 3's concern.
- **MCP write tools** (any tool that mutates state) — out of v0 entirely; the three search tools are read-only by construction. Round 4+ if a write surface ever lands.
- **Per-tool gating / authorization** — out of v0. The local-loopback trust model means all three tools are equally available; future deployments that want to restrict (e.g. semantic search disabled for compliance reasons) will pull this up at a later workplan.
- **MCP resources, prompts, sampling** — rmcp 1.5.0 supports these protocol features; v0 ships only tools. The three search operations are tools by design (per ADR-0004); future feature surfaces (e.g. exposing the outbox as an MCP resource subscription) are post-v0 concerns.
- **Compile-time bundling of `rmcp`** — release-packaging concern; out of v0. The crate is a runtime dependency at the v0 stage.
- **Iris adapter integration test** — optional in Task 8.5's manual agent-integration test; v0 requires only Claude Code coverage. Iris coverage is a forward-compat strengthener, not a shipping criterion.
- **MCP client-side functionality** — `rmcp` supports both client and server roles; v0 uses only the server features (`hmn mcp` is an MCP *server* — it serves tools to its parent agent host; it is *not* an MCP client of anything). `hmn`'s normal subcommands (`search`, `status`) speak HTTP to `hmnd` per ADR-0008, not MCP; no rmcp client-feature flags are enabled in `Cargo.toml`.
- **Output schemas in tool descriptors** — if rmcp 1.5.0's `#[tool]` macro doesn't accept an output-schema attribute (verification at Task 8.1), v0 ships without explicit output schemas. The response types still derive `JsonSchema` (so client-side typing is possible), but the tool's MCP descriptor will only carry `inputSchema`. Adding `outputSchema` is additive when rmcp supports it.

---

## Net new dependencies

- **`rmcp` ≈ 1.5.x** with features `["server", "transport-io", "macros", "schemars"]` (verified at Task 8.1). Pulls in: `tokio` (already in tree), `serde` (already in tree), `serde_json` (already in tree), `schemars` (transitive — re-exported via `rmcp::schemars` per upstream calculator example), and rmcp-internal crates.
- **`schemars`** *(direct dependency — only if the `rmcp::schemars` re-export doesn't suffice; verified at Task 8.1)*. The upstream calculator example uses the re-export path, so the direct dependency is likely unnecessary.

The dependency-graph weight increase is modest (rmcp's `transport-io` feature avoids pulling in the HTTP-streamable transport's reqwest-based client). Both binaries link against the lib crate, so both *technically* see the dependency, but **only `hmn` actively imports `hypomnema::mcp`** (added by Task 8.2 and used by Task 8.3's `cmd_mcp`). `hmnd` does not reach into the mcp module in v0; the linker should strip the unused rmcp symbols from `hmnd`'s release build via standard dead-code elimination. If a binary-size measurement at boundary shows `hmnd`'s release artifact growing materially (>1 MB), the follow-up is to feature-gate the mcp module behind an `mcp` cargo feature that only `hmn`'s build activates — a small `Cargo.toml` change. The premature optimization is to feature-gate now without measuring; the workplan's call is "measure first, gate only if measurement justifies it."

---

## Process dependencies

- The long-running `hmnd` daemon must be running (and reachable at the configured `http.bind`) for `hmn mcp` to serve any tool calls. The daemon-unreachable error envelope (`code: "daemon_unreachable"`) handles the case where it isn't; the agent's UX makes the failure visible.
- The sqlite-vec extension binary (operator-provisioned per step 6) and the embedding service (per step 7) are required only by the long-running `hmnd` daemon, not by the `hmn mcp` shim. The shim has no SQLite or embedding dependency in its own process; failures of those substrates surface as HTTP error envelopes from the daemon, which the shim passes through unchanged.
- The pre-existing outbox flake under `cargo nextest run` fail-fast cancellation (carried forward from steps 6 and 7's boundaries) is not caused by this step and is not blocking — the agent runs tests with `--no-fail-fast` if it surfaces. Step 8 doesn't introduce any new flake-prone surfaces beyond the watcher-debounce timing reused from steps 5/7.
- The `flake.nix` sqlite-vec dylib provisioning question (carried forward from step 6's boundary) is not blocking step 8 either — `hmn mcp` doesn't load the extension (it's `hmn`-side, not `hmnd`-side), and the long-running daemon's setup is unchanged from step 7. The boundary may revisit `flake.nix` provisioning as a separate follow-on commit.

---

## Self-review for prose accuracy

This workplan came in at ~1000+ lines, at or above the round-1-boundary heuristic threshold. Full self-review ran with focus on the new external-library claims (rmcp 1.5.0 API shape, schemars derive path, feature-flag names) and the cross-resolution consistency.

**Claims that were re-checked**:

- *rmcp 1.5.0 is the latest version and was released around 2026-04-16* — confirmed against docs.rs and the rust-sdk GitHub repo's latest release tag at the time of writing. The exact patch version is verified at Task 8.1 time (the workplan literal `1.5` accommodates 1.5.x patch updates without re-edit).
- *The feature flags `["server", "transport-io", "macros", "schemars"]` are the right set for a stdio MCP server with macro-driven tool registration and JsonSchema-derived input schemas* — confirmed against the upstream README (`features = ["server"]` shown there is the *minimum*; the full set including `transport-io`, `macros`, `schemars` is documented in the rmcp 1.5.0 docs.rs introduction). Task 8.1 verifies the exact feature names against the current rmcp 1.5.x feature list — if `transport-io` is renamed (e.g. to `transport-stdio`) or split, the agent corrects.
- *The `#[tool_router(server_handler)]` + `#[tool(description)]` + `Parameters<T>` pattern with `serde::Deserialize + schemars::JsonSchema` on the input struct* — confirmed against the upstream `examples/servers/src/common/calculator.rs` and `examples/servers/src/calculator_stdio.rs`. The exact handler return-type contract (whether the methods return `String`, `CallToolResult`, or `Result<CallToolResult, _>`) is verified at Task 8.2 time per the upstream macro implementation. The calculator example uses sync `fn` returning `String`; this workplan's tools are async `fn` returning `CallToolResult` — the macro is type-generic enough to support both, but the agent verifies before committing.
- *`rmcp::ServiceExt::serve(stdio()).await?` followed by `service.waiting().await?`* — confirmed against the upstream `calculator_stdio.rs` example. The exact import path (`rmcp::transport::stdio` vs `rmcp::transport::io::stdio`) is verified at Task 8.2/8.3 time; the calculator example imports `transport::stdio` directly.
- *`CallToolResult::structured(value)` and `structured_error(value)` constructors set only `structured_content`, leaving `content` empty* — confirmed against the rmcp 1.5.0 docs.rs `CallToolResult` struct page (the four named constructors are `success`, `error`, `structured`, `structured_error`).
- *Stderr-only logging is the upstream pattern for stdio MCP servers* — confirmed against `calculator_stdio.rs` (`with_writer(std::io::stderr).with_ansi(false)`). Pattern is load-bearing, not optional.
- *The existing `src/client.rs::DaemonClient` and `decode_response` flow already produce `anyhow::Error` chains whose Display starts with `"<code>: <message>"`* — confirmed by reading `src/client.rs:73-90`. The MCP shim's `envelope_from_anyhow` helper inverts this format (split on `": "`).
- *The existing `is_connect_error()` helper in `src/client.rs:92-98` correctly identifies connect failures via `reqwest::Error::is_connect()`* — confirmed by reading the source.
- *The `[mcp]` config block in `src/config.rs::McpConfig` (line 87–101) defaults `transport = "stdio"` and `socket = "~/.local/share/hypomnema/mcp.sock"`* — confirmed by reading the source. Resolution D's non-stdio warning preserves this struct unchanged.
- *Step-7's `embedding_unavailable` error envelope flows through `decode_response` as an `anyhow::Error` whose Display is `"embedding_unavailable: <detail>"`* — confirmed against `src/api/error.rs:35-54` and `src/client.rs:81-90`.
- *Claude Code's MCP client configuration shape (`mcpServers` JSON object with `command` + `args` keys)* — workplan literal is the well-documented shape for Claude Code; Task 8.5 verifies against current Claude Code docs at test time. If the schema has changed, the agent corrects.

**Workplan-internal consistency**:

- The flag shape and binary placement `hmn mcp` (subcommand on `hmn`, not `hmnd`) appear consistently in Resolution A, Goal recap, Resolution D's stdio half, Resolution G (logging), Tasks 8.3–8.5, Definition of Done, Cross-references, the rescoped shipping criterion 1, and the documentation Tasks 8.5 prescribe.
- The non-stdio `mcp.transport` warning emission (Resolution D, socket half) is consistently described as a `hmnd` daemon-side concern in Resolution D, Task 8.3 (split between `hmn` for the subcommand and `hmnd` for the warning), Definition of Done, and the rescoped shipping criterion 4.
- The error code `daemon_unreachable` appears consistently in Resolutions D and F, Task 8.2's helper signature, Task 8.4 integration test, the agent-integration test failure path in Task 8.5, the Definition of Done, and the cli.md doc update.
- The `structured_content` / `structured_error` MCP wire shape (Resolution F) appears consistently in Task 8.2's tool-method shape, Task 8.4's integration tests, the Definition of Done, and the rescoped shipping criterion 5.
- The deferred socket transport (Resolution D) is named consistently as a future `hmnd`-side feature in Resolution E (auth), Resolution D (lifecycle), Tasks 8.3 (warning), 8.5 (ADR-0012), Out of Scope, Process Dependencies, and the rescoped shipping criterion 4.
- The `tools/list` advertising the three `search_*` tools (Resolution B) appears in Task 8.4's integration test and the Task 8.5 manual agent-integration test.
- The dependency-graph claim — `rmcp` is required for `hmn`'s build (not `hmnd`'s) — appears consistently in Resolution A's "binary weight clarification" prose, the ADR-0008 amendment in Task 8.5, Net New Dependencies, and the rescoped shipping criteria's implicit assumption about which binary the agent host invokes.

**Residual ambiguities flagged for the agent at task time**:
- Upstream rmcp 1.5.x exact patch version + feature-flag names (Task 8.1 verifies; agent corrects + reports in results comment if any differ).
- Whether `schemars::JsonSchema` is re-exported as `rmcp::schemars::JsonSchema` (Task 8.1 verifies; if direct dependency is needed, the agent adds `schemars = "<version>"` to `Cargo.toml`).
- Whether the `#[tool_router(server_handler)]` macro auto-implements `ServerHandler` or requires a separate `#[tool_handler]` block (Task 8.2 verifies).
- The exact tool-method return-type contract — `CallToolResult` vs `Result<CallToolResult, McpError>` vs `impl Into<CallToolResult>` (Task 8.2 verifies against rmcp 1.5.0 macro impl).
- Whether the `#[tool]` macro accepts an output-schema attribute (Task 8.1 verifies; if not, the response types still derive JsonSchema for client-side use, but the tool descriptor carries `inputSchema` only).
- The exact stdio-framing format used by `rmcp::transport::stdio()` — newline-delimited JSON-RPC vs Content-Length headers (Task 8.4 verifies for the integration-test framing helper).
- Whether `src/logging.rs` already supports stderr-redirected output or needs a small extension (Task 8.3 verifies).
- The exact next ADR number (likely 0012; verified against `docs/decisions/`) at Task 8.5 time.
- The exact MCP-server-configuration shape Claude Code currently expects (Task 8.5 verifies at test time).

These are *narrow* escape hatches; the verification gates are in the tasks themselves, not deferred to soft-flag. If the agent's verification finds something the workplan didn't anticipate (e.g. rmcp 1.5.0 has an `inspectable_handler` requirement that the macros don't auto-generate, or Claude Code now requires a different MCP version), that's a `coordinator-only` soft flag worth surfacing at task close.

---

## Build-time amendments

(Empty — to be appended during the build phase per the round-2 precedent. The build-time amendments section in step-7-workplan.md § Build-time amendments is the structural template; entries here will record any workplan-prose slips, implementation choices the workplan deliberately did not pin, and any cross-task design tensions surfaced via the manual smoke or agent-integration tests.)
