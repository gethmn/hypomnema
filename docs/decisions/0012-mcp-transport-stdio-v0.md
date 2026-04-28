# ADR-0012: MCP transport — stdio on `hmn` in v0; socket on `hmnd` deferred

**Status**: accepted
**Date**: 2026-04-27
**Decision-Makers**: Beau Simensen

---

## Context

Step 8 (the round-2 shipping gate) wires Hypomnema's three search operations as MCP tools so that MCP-capable agent hosts (Claude Code, Iris, others) can invoke them through their own tool-call UX. The roadmap entry for step 8 left five decisions deferred:

- **A.** Final flag shape and binary placement: `hmnd --mcp-stdio` vs `hmnd mcp-stdio` vs `hmn mcp` vs env var.
- **B.** MCP tool names.
- **C.** Tool parameter schemas — derive vs hand-author.
- **D.** Connection lifecycle: stdio (process-per-connection) vs socket (long-lived).
- **E.** Authentication on the socket transport.

Resolutions B (kept the `search_filesystem` / `search_content` / `search_semantic` names from [ADR-0004](./0004-three-search-modes-as-peers.md)) and C (derived JSON schemas from the existing `*QueryJson` request types via `schemars::JsonSchema`, re-exported as `rmcp::schemars`) were narrow technical calls captured in the workplan and the implementing commits — they don't require an ADR.

Resolutions A, D, and E are the canon-touching ones. They also resolved a pre-existing internal inconsistency in [ADR-0008](./0008-two-binary-daemon-plus-cli.md) about which binary owns the MCP-over-stdio surface (line 29 said `hmn`; line 40 said `hmnd`). The ADR-0008 inconsistency is settled in the [ADR-0008 § Amendments](./0008-two-binary-daemon-plus-cli.md#amendments) entry; this ADR records the v0 transport-shape decision and its forward-compat plan for socket auth.

A fourth decision surfaced during the build (not pre-flagged in the roadmap):

- **Brand identity in the MCP `serverInfo`.** rmcp's `Implementation::from_build_env()` macro auto-derives `serverInfo.name = "rmcp"` and `serverInfo.version = "1.5.0"` from rmcp's own crate metadata, not Hypomnema's. MCP hosts surface `serverInfo.name` to users in their tool-listing UX, so the auto-derived value would mislabel the server.

## Decision

**Stdio MCP ships in v0; socket MCP deferred.**

1. **Stdio transport on `hmn`.** The MCP-over-stdio surface is the `hmn mcp` subcommand on the CLI binary. The `hmn mcp` process is a thin HTTP shim — it opens no SQLite, loads no sqlite-vec extension, runs no watcher, holds no embedding client. Every tool call is forwarded to the running `hmnd` over loopback HTTP using the same `DaemonClient` machinery `hmn search …` uses. Process exits when the agent host closes stdin. (Resolution A and Resolution D, stdio half.)

2. **Socket transport deferred to a follow-on workplan.** The `mcp.transport` config knob continues to parse and validate. Setting `mcp.transport = "socket"` in v0 produces a clear `WARN`-level log at `hmnd` startup but does not crash; the socket file at `mcp.socket` is **not** bound. When socket transport ships, it lives in `hmnd` (long-lived listener) — keeping the binary-placement split clean: stdio = `hmn` (short-lived adapter); socket = `hmnd` (long-lived listener). (Resolution D, socket half — this is a workplan-time rescope of the original step-8 shipping criterion 4; see [step-08 workplan § Shipping criteria (rescoped)](../roadmap/step-08-workplan.md#shipping-criteria-rescoped).)

   **HTTP MCP transport ships in round 4** ([ADR-0013](./0013-mcp-transport-streamable-http.md)). The full v0-deferred MCP transport landscape is now stdio-on-`hmn` (this ADR § Resolution 1, shipped round 2), HTTP-on-`hmnd` (ADR-0013, shipped round 4), socket-on-`hmnd` (this ADR § Resolution 2, deferred). HTTP-MCP mounts on `hmnd`'s existing Axum router at `/mcp`; the v0 stdio-shipped / socket-deferred decision recorded here is unchanged.

3. **Forward-compat for socket auth: filesystem permissions only.** When the socket transport ships, the socket file is created with mode `0600` (owner read/write only). No token, no challenge-response, no TLS. The trust boundary is the user's home directory: anyone who can read the socket file already has access to the daemon's config (`~/.config/hypomnema/config.toml`, which contains the daemon URL and any embedding-service `api_key`) and the index (`~/.local/share/hypomnema/vaults/<id>/index.sqlite`, which contains every chunk's text). Adding token auth would be theater. (Resolution E, recorded forward-compat — not implemented in v0.)

4. **Brand identity: `serverInfo.name = "hypomnema"`.** The `#[tool_handler(name = "hypomnema")]` attribute on `impl ServerHandler for HypomnemaMcpServer` in `src/mcp/server.rs` overrides rmcp's auto-derived `Implementation::from_build_env()`. The `version` field auto-fills from `env!("CARGO_PKG_VERSION")` via the rmcp 1.5.0 macro's behavior when `name` is provided without an explicit `version` — so the MCP `serverInfo.version` tracks the Hypomnema crate version with no further bookkeeping. (Brand-identity override; recorded here because it directly shapes how MCP hosts label the tool surface.)

This ADR amends [ADR-0008](./0008-two-binary-daemon-plus-cli.md) — see [ADR-0008 § Amendments](./0008-two-binary-daemon-plus-cli.md#amendments) for the binary-placement reasoning that resolves the ADR-0008 line-29-vs-line-40 inconsistency. It supersedes nothing else.

## Consequences

### Positive

- **Each transport's binary matches its lifetime.** Stdio MCP is per-session (the agent host spawns `hmn mcp` and closes stdin to terminate); the binary that serves it (`hmn`) is the project's per-invocation client. Socket MCP, when it ships, will be a long-lived listener (the daemon binds the socket and accepts connections); the binary that serves it (`hmnd`) is the project's long-lived process. The placement clarifies what each binary is for instead of conflating both.
- **`hmnd` does not link `rmcp` in v0.** The daemon's dependency graph stays narrower than it would have if step 8 had landed MCP on `hmnd` directly. `hmn` grows by `rmcp` (with `["server", "transport-io", "macros", "schemars"]`); `hmnd` is unchanged on the MCP axis. (Net new dependencies for v0 are recorded in [step-08 workplan § Net new dependencies](../roadmap/step-08-workplan.md#net-new-dependencies).)
- **Focused round-2 shipping gate.** Deferring socket transport keeps the round-2 ship small and load-bearing on the agent-integration test (Claude Code in the loop, criterion 2). Socket would have added an accept-loop integrated with daemon shutdown, per-connection task spawning, mode-0600 permission handling, and asymmetric tool implementations (stdio = HTTP shim, socket = in-daemon direct call) — bundled, those push step 8 from medium-high to high risk and delay the load-bearing manual test.
- **Brand identity in MCP host UX.** Users see "hypomnema" in their MCP host's tool listing instead of "rmcp 1.5.0," matching the project name they configured.
- **Decision rule for un-deferring is explicit.** When a concrete consumer needs socket-MCP — e.g. round 3 multi-vault MCP tools that mutate state under high call rates, or an agent host that prefers persistent connections — the socket transport pulls up at that workplan. The implementation is well-scoped: `tokio::net::UnixListener` accept-loop + per-connection rmcp service + permission setting + shutdown integration. No premature abstraction in v0.

### Negative

- **Two ADRs to read for the full v0 MCP picture.** Agents reading [ADR-0008](./0008-two-binary-daemon-plus-cli.md) get the two-binary shape; this ADR layers in the per-transport split. The cross-reference from [ADR-0008 § Amendments](./0008-two-binary-daemon-plus-cli.md#amendments) plus this ADR's link back keeps the trail navigable but doubles the read.
- **The `mcp.transport` config knob exists in v0 without doing anything.** Operators who set `mcp.transport = "socket"` see a clear startup warning but no functionality. The alternative — removing the knob until socket ships — would have churned the config schema for forward-compat reasons that don't earn it; the warning is the right shape.
- **Brand-identity override is rmcp-version-specific.** The `#[tool_handler(name = "...")]` attribute syntax depends on rmcp 1.5.x's macro shape (verified at step-8 build time against `rmcp-macros-1.5.0`). If a future rmcp major version changes the attribute syntax, the override needs revisiting at the upgrade workplan.

### Neutral

- **MCP error envelopes mirror HTTP's.** The `daemon_unreachable` code is new at the MCP layer (synthesized when the underlying `reqwest::Error` indicates a connect failure — there is no HTTP analogue because HTTP can't return errors when the daemon isn't running). The other codes (`invalid_glob`, `invalid_regex`, `invalid_prefix`, `invalid_request`, `embedding_unavailable`, `internal`) flow through unchanged via `decode_response` → `envelope_from_anyhow` → `CallToolResult::structured_error`.
- **Stdout reservation for the MCP transport is load-bearing.** When `hmn mcp` runs, all `tracing` output is redirected to stderr via `BinaryKind::HmnMcp` in `src/logging.rs`. Any byte that lands on stdout outside the rmcp framing is a protocol violation that breaks the MCP session. This is a concrete operational invariant, not a stylistic choice.
- **No MCP write tools, prompts, resources, sampling.** v0 ships read-only search tools only. Round 3+ may pull up vault-management MCP tools per [ADR-0011](./0011-vault-management-on-hmn.md); other rmcp 1.5.0 protocol features (prompts, resource subscriptions, sampling) are post-v0 concerns.

---

## Notes

- Amends [ADR-0008: Two Binaries (hmnd + hmn) in One Crate](./0008-two-binary-daemon-plus-cli.md) — see its § Amendments entry for the binary-placement reasoning that resolves the line-29-vs-line-40 inconsistency. This ADR is the formal record of the v0 transport-shape decision the amendment refers to.
- Extends [ADR-0004: Three Search Modes as Peers](./0004-three-search-modes-as-peers.md) — the canonical names (`search_filesystem` / `search_content` / `search_semantic`) and the canonical claim that HTTP and MCP are peers (same wire shapes, same operations). The `Resolution C` schema-derive choice in step 8 makes the "peers" claim load-bearing at compile time: the `*QueryJson` request types and `*SearchResponse` response types serve both transports without divergence.
- Extends [ADR-0005: Local Everything](./0005-local-everything.md) — the local-everything trust boundary is what justifies mode-0600 socket auth (Resolution E) over token auth. The HTTP server's loopback-only / no-auth posture (configuration.md § `[http]`) is the structural equivalent for a different transport; mode-0600 on the socket is "loopback for sockets is same-user-only."
- Related to [ADR-0011: Vault Management Lives on `hmn`](./0011-vault-management-on-hmn.md). Vault-management MCP tools are round-3 work; when they land, they extend the MCP tool surface this ADR establishes (they don't change its transport shape).

## Amendments

### 2026-04-28: HTTP-MCP transport added in round 4 (ADR-0013)

[ADR-0013](./0013-mcp-transport-streamable-http.md) introduces Streamable
HTTP MCP on `hmnd` as the round-4 shipping gate — the third standard
MCP transport, mounted on `hmnd`'s existing Axum router at `/mcp`. The
v0 stdio-shipped / socket-deferred decision recorded in this ADR is
unchanged; ADR-0013 adds the third transport without superseding the
present scope. The "each transport's binary matches its lifetime"
principle applies unchanged: stdio-MCP → `hmn` (short-lived adapter);
HTTP-MCP → `hmnd` (long-lived listener); socket-MCP (deferred) → `hmnd`
(long-lived listener). Brand identity (`serverInfo.name = "hypomnema"`,
this ADR § Resolution 4) and the `HypomnemaBackend` trait extraction
that underpins both `hmn`-side and `hmnd`-side MCP servers are recorded
in ADR-0013 + [`docs/specs/mcp-streamable-http.md`](../specs/mcp-streamable-http.md).
