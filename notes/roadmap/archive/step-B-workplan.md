# Step B Workplan -- `axum` upgrade

**Step**: B of 2 (round 7: Dependency Upgrade Round). See [`roadmap-7.md`](./roadmap-7.md) for the round framing and sequencing. This step lands the HTTP / MCP surface upgrade: `axum` 0.7 -> 0.8.

**Status**: Shipped 2026-05-01.

**Goal recap**

- Upgrade `axum` from `0.7.9` to `0.8.9`.
- Keep the HTTP search, health, MCP Streamable HTTP, and client surfaces compiling cleanly.
- Preserve the existing response shapes and router topology unless the migration guide / compiler force a change.
- Prove the upgrade with the full test suite and a live-daemon smoke pass.

**Relevant current surface**

- `Cargo.toml` currently pins `axum = "0.7"`.
- `rust-toolchain.toml` must be checked against axum 0.8's MSRV requirement before the build is declared done.
- `src/api/` contains the HTTP search, health, status, watch, vault, and MCP-HTTP routing surfaces.
- `src/mcp/server.rs` builds the MCP backend and has the densest axum callsite cluster.
- `src/bin/hmnd.rs` hosts the daemon listener and graceful-shutdown path.
- `src/client.rs` and the integration tests under `tests/` exercise the live axum listener path.

**Deferred decisions to resolve while building**

- Exact axum 0.8 callsite inventory after reading the migration guide and the first `cargo check` errors.
- Whether `tower = "0.5"` is needed in dev-dependencies for axum 0.8 compatibility.
- Whether `rust-toolchain.toml` needs an explicit bump to satisfy axum 0.8's MSRV.
- Whether any tuple / `IntoResponse` callsites need local adjustment under 0.8.9 specifically.

**Skill**

- No dedicated skill is expected for this step. Standard HTTP / async / axum migration work applies.

**Risk**

- Medium-high. Axum touches the full HTTP and MCP surface, so the blast radius is wider than Step A. The migration guide should keep the work tractable, but the workplan needs to inventory the actual callsites before the build starts.

---

## Tasks

### Task B.1 -- Bump axum and absorb compile breakage

Update `Cargo.toml` to `axum = "0.8"`, then resolve the compiler errors surfaced by the migration in `src/api/`, `src/mcp/server.rs`, `src/bin/hmnd.rs`, `src/client.rs`, and any narrow follow-on callsites. Check `rust-toolchain.toml` and the dev-dependencies for compatibility while you are in the compile loop.

**Deliverable**: a compiling HTTP / MCP surface with the upgraded axum pin and any necessary toolchain / dependency adjustments called out explicitly.

**Expected files**: `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml` if the MSRV must move, `src/api/*`, `src/mcp/server.rs`, `src/bin/hmnd.rs`, `src/client.rs`, and narrow follow-on edits if the migration guide requires them.

### Task B.2 -- Reconcile tests and smoke the live HTTP / MCP surface

Run the HTTP, CLI, and MCP integration tests, update only the assertions that genuinely changed because of the axum upgrade, and then run a live-daemon smoke pass against `/health`, `/search/*`, `/vaults/*`, and `/mcp` to confirm the daemon still responds correctly.

**Deliverable**: green integration tests plus a verified live-daemon smoke.

**Expected files**: `tests/http.rs`, `tests/cli.rs`, `tests/mcp.rs`, `tests/mcp_http.rs`, `tests/embedding.rs`, `tests/vault_control_plane.rs`, and any other test file whose assertion shape truly changes because of axum 0.8.

---

## Test strategy

- Read the axum 0.8 migration guide before touching the compile-fix loop.
- Run `cargo check` immediately after the dependency bump to surface the inventory quickly.
- Run the relevant HTTP / MCP integration tests incrementally while adjusting callsites.
- Run `cargo test` before closing the step.
- Run `cargo clippy -- -D warnings` before declaring the step done.
- Run a live-daemon smoke against the `/health`, `/search/*`, `/vaults/*`, and `/mcp` surfaces after the tests are green.

## Definition of done

- `Cargo.toml` and `Cargo.lock` pin axum at the upgraded version.
- The HTTP, MCP, and client surfaces compile cleanly with no unreviewed workaround.
- Existing HTTP, CLI, and MCP integration tests pass, with any changed assertion explicitly justified.
- A live-daemon smoke confirms the HTTP and MCP surfaces still behave as expected.
- If the upgrade requires an MSRV bump, `rust-toolchain.toml` records it explicitly.

## Cross-references

- [`notes/roadmap/roadmap-7.md`](./roadmap-7.md) -- round framing and sequencing.
- [`notes/coordinator-playbook.md`](../coordinator-playbook.md) -- coordinator / task-agent contract.
- [`src/api/mod.rs`](/Users/beausimensen/Code/hypomnema/src/api/mod.rs) -- HTTP router entrypoint.
- [`src/api/mcp_http.rs`](/Users/beausimensen/Code/hypomnema/src/api/mcp_http.rs) -- Streamable HTTP MCP mount point.
- [`src/mcp/server.rs`](/Users/beausimensen/Code/hypomnema/src/mcp/server.rs) -- MCP server / route wiring.
- [`src/bin/hmnd.rs`](/Users/beausimensen/Code/hypomnema/src/bin/hmnd.rs) -- daemon listener and graceful shutdown.
- [`tests/http.rs`](/Users/beausimensen/Code/hypomnema/tests/http.rs) -- HTTP integration coverage.
- [`tests/mcp_http.rs`](/Users/beausimensen/Code/hypomnema/tests/mcp_http.rs) -- Streamable HTTP integration coverage.

## Out of scope

- `notify` / `notify-debouncer-full` work belongs to Step A and is already complete.
- Any unrelated dependency bumps outside axum and immediately required compatibility changes.
- Spec or docs rewrites unless the migration produces a concrete behavior change that needs recording.

## Net new dependencies

- None beyond the requested axum version bump, unless the migration guide forces a narrow compatibility bump such as `tower`.

## Process dependencies

- Step A must be closed before Step B starts so the round diffs stay bisectable.
- If the upgraded axum surface changes observable behavior, record that in the step results comment before closing the step.
