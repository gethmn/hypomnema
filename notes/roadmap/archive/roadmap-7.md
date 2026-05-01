# Hypomnema Roadmap -- Round 7: Dependency Upgrade Round

**Scope**: land the three open Dependabot PRs that are genuinely breaking upgrades rather than mechanical version bumps: `notify` / `notify-debouncer-full`, then `axum`. The round is intentionally focused on dependency upgrades only. It is not a place to opportunistically absorb unrelated polish work.

**Status**: Shipped 2026-05-01. Round 6 shipped `v0.5.0` on 2026-04-30. Workplans are created just before each step is implemented, per the established round cadence.

**Process**: Same as rounds 1-6. This round is split into two steps. Each step gets a short workplan (`step-A-workplan.md`, `step-B-workplan.md`) created immediately before that step is built. Deferred decisions are pulled forward to workplan-time. The orchestration shape (orchestrator + per-step coordinator + ephemeral task agents, see [`notes/coordinator-playbook.md`](../coordinator-playbook.md)) carries forward unchanged.

**Why a dedicated round**:

- **These are breaking major-version upgrades, not one-line bumps.** `axum 0.7 -> 0.8` touches the HTTP and MCP surface. `notify 6 -> 8` and `notify-debouncer-full 0.3 -> 0.7` affect the watcher path and carry their own API and correctness changes.
- **The watcher and HTTP surfaces are both load-bearing.** Mixing both upgrade tracks into a larger polish round would make regressions harder to bisect and would widen the blast radius of any failure.
- **The debouncer bump is not just churn.** `notify-debouncer-full 0.7` fixes a real correctness issue around event deduplication during debounce windows. That makes the watcher upgrade worth isolating even if the public API changes are small.
- **Round discipline stays the priority.** Keep the round bisectable, keep the diff focused, and avoid bundling unrelated backlog items into the same step.

**Skills carrying forward**:

- `filesystem-watching` is the primary skill for Step A.
- No dedicated skill is expected for Step B.

**New deps**: none beyond the version bumps already implied by the open Dependabot PRs.

---

## Phasing

Two steps, ordered by blast radius:

| Step | Contents | Risk |
| ---- | -------- | ---- |
| A | `notify` + `notify-debouncer-full` upgrade | Medium |
| B | `axum` upgrade | Medium-high |

Step A ships first because it touches the watcher, which is core to Hypomnema, but its API footprint is narrower than axum's. Step B follows after Step A so the two upgrade surfaces do not overlap in the same diff.

---

## Step A -- `notify` + `notify-debouncer-full` upgrade

**Goal**: bump `notify` from `6.1.1` to `8.2.0` and `notify-debouncer-full` from `0.3.2` to `0.7.0` together. The watcher module compiles clean, existing watcher tests pass, and a real-file-change smoke confirms the debounced event pipeline still behaves correctly.

**Key changes to absorb**:

- `notify-types` 2.0 rename, which changes import paths in `src/watcher/`
- `FILE_NOTIFY_INFORMATION` alignment fix on Windows
- `FileIdCache` ownership-trait changes in `notify-debouncer-full` 0.7
- `FileIdCache` flexible-handle support introduced in `notify-debouncer-full` 0.6
- `notify-debouncer-full` 0.7 correctness fix around suppressing `Modify` events immediately after `Create` unless the event is a rename

**Shipping criteria**:

- `Cargo.toml` pins `notify = "8"` and `notify-debouncer-full = "0.7"`
- `cargo check` and `cargo clippy -- -D warnings` are clean
- `cargo test` passes without changing watcher assertions unless the new debouncer behavior genuinely changes observable event counts
- If test expectations change, the step makes that change explicit and justifies it in the commit message
- A real-file-change smoke pass still confirms create/modify/delete events propagate through the consumer surface

**Deferred decisions to resolve at workplan-time**:

- Whether the `FileIdCache` trait change needs a Hypomnema code change or whether the existing implementation already satisfies the new bounds
- Whether the debouncer's `Modify`-after-`Create` suppression changes any expected event counts in `tests/watch.rs`

**Risk**: medium. The watcher is load-bearing, but the upgrade surface is well-scoped. The main failure mode is a non-trivial `FileIdCache` adjustment.

---

## Step B -- `axum` upgrade

**Goal**: bump `axum` from `0.7.9` to `0.8.9`. The HTTP search endpoints, MCP Streamable HTTP transport, health endpoint, and error types compile clean under the new API. The full test suite passes. A smoke pass against the live daemon confirms the HTTP and MCP surfaces still respond correctly.

**Key changes to absorb**:

- `axum` 0.8 API changes across the HTTP surface
- MSRV bump to 1.80
- `IntoResponse` tuple behavior should be verified against 0.8.9 specifically
- `tower` dev-dependency compatibility may need a version bump

**Shipping criteria**:

- `Cargo.toml` pins `axum = "0.8"`
- `cargo check` and `cargo clippy -- -D warnings` are clean
- `cargo test` passes across HTTP, CLI, and MCP integration tests
- A smoke pass against a live daemon verifies `/health`, `/search/*`, `/vaults/*`, and `/mcp`
- `rust-toolchain.toml` is compatible with axum 0.8's MSRV requirement

**Deferred decisions to resolve at workplan-time**:

- Exact axum 0.8 callsite inventory, after reading the migration guide and the first `cargo check` errors
- Whether `tower = "0.5"` is needed in dev-dependencies
- Whether `rust-toolchain.toml` needs an explicit bump

**Risk**: medium-high. Axum touches the entire HTTP and MCP surface, so the blast radius is wider than Step A. The migration guide should keep the work tractable, but the workplan needs to inventory the actual callsites before the build starts.

---

## Notes on the round-7 shipping gate

The round-7 shipping gate is:

1. `notify` and `notify-debouncer-full` are upgraded together and the watcher surface remains correct.
2. `axum` is upgraded and the HTTP + MCP surface remains correct.
3. Existing tests stay green, with any assertion changes called out explicitly if the new library behavior warrants them.
4. Real-file-change smoke coverage remains intact for the watcher path.
5. Live-daemon smoke coverage remains intact for the HTTP and MCP path.
6. Round tag: likely `v0.6.0` if this becomes the next shipping gate.

After the gate hits, round 7 archives alongside its step workplans, and round 8's roadmap is written when the human picks the next focus from the backlog.

---

## Out of scope for round 7

These stay in [`notes/backlog.md`](../backlog.md) and are explicitly not part of this dependency round:

- Release automation and cross-compilation
- OSSF Scorecard and CodeQL
- Windows CI matrix
- Any dependency bumps beyond the three open Dependabot PRs that motivated this round
- Any unrelated polish or feature work that would widen the step blast radius
