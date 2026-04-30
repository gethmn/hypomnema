# Step 16 Workplan -- Outbox Retirement Shipping Gate

**Status**: Shipped 2026-04-30

**Roadmap**: [`notes/roadmap/roadmap-6.md`](../roadmap-6.md) Step 16.

**Goal**: retire the durable JSONL outbox as Hypomnema's change-event contract, replace the runtime path with a live-only in-memory event stream, remove stale config/status/test/doc surfaces, and prove the new event shape with real filesystem changes.

## Workplan Decisions

### A. Replacement Event Model

Step 16 removes the durable outbox outright.

- No JSONL outbox file remains in the runtime.
- No compatibility shim tails, writes, migrates, reports, or preserves an outbox file.
- No narrower durable store is introduced in this step.
- The v0 event model is live-only: connected subscribers receive events from an in-memory broadcast channel; disconnected clients recover by querying current index state.
- A future durable/replayable stream remains possible, but it must be designed as a real event store with sequence/generation/retention semantics, not as consumers tailing daemon-owned files.

**Why**: `tests/outbox.rs` flakes are symptoms of the wrong center of gravity. The old tail-file model pretends to provide a durable subscription contract without replay invalidation, retention, bootstrap watermark, or reset semantics. The step should delete that surface instead of hardening stale tests around it.

### B. Consumer-Facing Compatibility

Step 16 ships the live event stream over the daemon HTTP control plane and the `hmn` CLI.

- HTTP: add a streaming endpoint that emits newline-delimited JSON event envelopes.
- CLI: add `hmn vault watch [NAME|ID] [--all]`, implemented as a thin streaming client over the HTTP endpoint.
- MCP: do **not** invent a pseudo-durable tool result. During implementation, verify the pinned `rmcp` version's practical streaming options. If there is no clean long-lived MCP framing in the current in-tree shape, the MCP `vault_watch` surface stays deferred and docs are narrowed accordingly. MCP hosts can still consume the HTTP endpoint through their host/runtime integration until a separate MCP-streaming workplan pins the transport shape.

**Compatibility rule**: removing the JSONL outbox is allowed to break consumers that tailed daemon-owned files directly. That was never the desired v0 contract after this step. The compatibility target is preserving a human/script event-tailing experience through `hmn vault watch`, not preserving the old file path.

### C. Spec Wording

`docs/specs/change-events.md` stays active, but it is narrowed to the live-only contract that this step actually ships.

- Keep the event envelope and live-only recovery model.
- Pin newline-delimited JSON as the HTTP/CLI v0 framing unless implementation discovers a concrete reason to prefer SSE. Default to NDJSON because it is simple for CLI piping and does not add browser-specific semantics.
- Move MCP streaming from "public v0 surface" to "deferred until rmcp framing is pinned" if Task 16.6 confirms no clean current implementation.
- Keep the future durable stream section as non-goal / future design guidance.

### D. Outbox State Placement ADR

ADR-0006 remains accepted. This step amends it without reversing it:

- The durable JSONL outbox is removed.
- The "daemon state outside watched vault" rule still applies to all remaining mutable daemon state.
- If a future durable event store lands, it must live outside watched vaults.

## Relevant Inputs

- Roadmap: [`notes/roadmap/roadmap-6.md`](../roadmap-6.md) § Step 16.
- ADR: [`docs/decisions/0006-outbox-outside-watched-directory.md`](../../docs/decisions/0006-outbox-outside-watched-directory.md).
- ADR: [`docs/decisions/0012-mcp-transport-stdio-v0.md`](../../docs/decisions/0012-mcp-transport-stdio-v0.md).
- ADR: [`docs/decisions/0013-mcp-transport-streamable-http.md`](../../docs/decisions/0013-mcp-transport-streamable-http.md).
- Spec: [`docs/specs/change-events.md`](../../docs/specs/change-events.md).
- Spec: [`docs/specs/vault-management.md`](../../docs/specs/vault-management.md) § `watch` and § MCP Tool Surface.
- Skill: [`.claude/skills/filesystem-watching/SKILL.md`](../../.claude/skills/filesystem-watching/SKILL.md).
- Skill: [`.claude/skills/rusqlite-in-async/SKILL.md`](../../.claude/skills/rusqlite-in-async/SKILL.md).

## Task Plan

### Task 16.1 -- Canonical Docs And Contract Cleanup

**Purpose**: pin the deletion decision in docs before code changes make the old surface disappear.

**Work**:

- Update `docs/specs/change-events.md`:
  - live-only is the only v0 contract;
  - HTTP/CLI use NDJSON streaming;
  - no `since`, replay, retained log, or file-tailing semantics;
  - `stream_lagged` is a control event from bounded live channels, not durable replay metadata;
  - MCP streaming is either explicitly deferred or pinned only if the implementation verifies a real rmcp shape before Task 16.6.
- Update `docs/specs/vault-management.md`:
  - keep `watch` as CLI/HTTP live subscription;
  - remove or narrow `vault_watch` claims if MCP streaming is not implemented in this step;
  - make `rescan` wording say "publish live change events" instead of "re-emit outbox events."
- Update `docs/architecture/overview.md`:
  - replace durable-outbox language with "watcher/indexer publishes to live event bus after the content-hash gate."
- Update `docs/reference/configuration.md` and `docs/reference/cli.md`:
  - remove `storage.outbox_file`;
  - remove status output references to outbox path/size;
  - document `hmn vault watch` only if Task 16.5 ships it.
- Amend `docs/decisions/0006-outbox-outside-watched-directory.md`:
  - record JSONL outbox removal;
  - preserve the data-dir placement rule for any future durable store.

**Files likely touched**:

- `docs/specs/change-events.md`
- `docs/specs/vault-management.md`
- `docs/architecture/overview.md`
- `docs/reference/configuration.md`
- `docs/reference/cli.md`
- `docs/decisions/0006-outbox-outside-watched-directory.md`

**Tests**: doc-only; run `rg -n "outbox|outbox_file|JSONL|vault_watch" docs` and verify remaining hits are historical or intentionally deferred.

**Risk**: medium. The docs already contain some forward-looking live-stream claims; this task must distinguish "contract shipped now" from "future MCP framing."

### Task 16.2 -- Event Types And Live Bus

**Purpose**: replace the `outbox` module with runtime-neutral event types and an in-memory bus.

**Work**:

- Add a new module, likely `src/events.rs` or `src/events/mod.rs`.
- Move the reusable event concepts out of `src/outbox/event.rs`:
  - `EventType::{Created, Modified, Deleted}`;
  - file-change event payload;
  - RFC3339 microsecond `detected_at`.
- Update the wire shape to match `change-events.md`:
  - file events serialize with top-level `"type": "file_changed"`;
  - `event_type` remains `created|modified|deleted`;
  - `vault` remains the surrogate vault ID only;
  - `path`, `content_hash`, `detected_at` remain as documented.
- Add a stream-control event type for lag:
  - top-level `"type": "stream_lagged"`;
  - `vault?: string`;
  - `missed?: u64`;
  - `action: "resync_required"`;
  - `detected_at`.
- Add a small live bus around `tokio::sync::broadcast`:
  - one daemon-level sender;
  - bounded capacity, pinned as a named constant;
  - `publish(StreamEvent)` ignores "no subscribers" and logs real send failures only if any exist;
  - `subscribe()` returns a receiver used by HTTP streams.

**Files likely touched**:

- `src/events.rs` or `src/events/mod.rs`
- `src/lib.rs`
- `src/outbox/event.rs` (deleted or moved)
- `src/outbox/mod.rs` (deleted)
- `src/outbox/writer.rs` (deleted)

**Tests**:

- Unit tests for JSON serialization of file events and lag events.
- Unit test that unknown `vault_name` is not present in file events.
- Unit test for publish-without-subscribers succeeding.
- Unit test for lag event construction if practical without racing broadcast internals.

**Risk**: medium. Event wire shape changes are user-facing and shared by HTTP/CLI.

### Task 16.3 -- Watcher And Manager Runtime Rewire

**Purpose**: remove the durable writer from the watcher/indexer pipeline and publish live events after real index changes.

**Work**:

- Change `watcher::run_consumer` to accept an event publisher instead of `Outbox`.
- Preserve the existing order:
  1. watcher translates debounced filesystem events;
  2. `Scanner` applies the index change;
  3. only `Inserted`, `Updated`, or successful `Removed` publishes an event;
  4. `HashUnchanged`, `NotPresent`, and filtered paths remain silent.
- Preserve rescan behavior:
  - `run_rescan` walks vault paths and drives the same `apply_event` path;
  - unchanged files stay silent;
  - reset/rebuild plus rescan can publish changed files via the cleared hash gate.
- Thread the live bus through `VaultManager::open`, `spawn_runner_for_row`, and `spawn_runner_parts`.
- Remove `Outbox::open`, `outbox_path`, and `storage.outbox_file` from runner construction.
- Update `VaultEntry` so it no longer carries `outbox_path`.
- Keep all SQLite calls inside existing `Scanner` / `Store` spawn_blocking boundaries. Do not add direct SQL in async watcher code.
- Keep notify/debouncer behavior unchanged. Do not reimplement debouncing.

**Files likely touched**:

- `src/watcher/mod.rs`
- `src/control_plane/manager.rs`
- `src/control_plane/runner.rs`
- `src/api/mod.rs`
- `src/lib.rs`

**Tests**:

- Update watcher unit/integration tests to subscribe to the live bus instead of reading a file.
- Preserve coverage for:
  - create -> `created`;
  - edit -> `modified`;
  - delete -> `deleted`;
  - mtime-only touch -> no event;
  - sync-conflict file -> no event;
  - rename -> deleted + created where the platform exposes that shape.
- Anti-flake stance: tests may poll a receiver with explicit timeouts, but should not hide event-ordering bugs by blindly retrying assertions against final state.

**Risk**: high. This is the core behavior deletion and replacement. It touches watcher, indexer outcomes, control-plane runner lifecycle, and test fixtures.

### Task 16.4 -- HTTP Watch Endpoint

**Purpose**: expose the live event bus through the daemon control plane.

**Work**:

- Add HTTP streaming route(s), defaulting to:
  - `GET /vaults/{name_or_id}/watch` for one vault;
  - `GET /events/watch?all=true` or equivalent for all active vaults if a separate aggregate route is cleaner.
- Prefer `GET /vaults/{name_or_id}/watch` plus `GET /events/watch` for all-active-vault subscriptions if route ergonomics stay simple.
- Resolve selector at subscription time:
  - unknown selector -> `404 vault_not_found`;
  - paused/errored selector -> either stream no file events or return a documented error. Prefer documented error only if current control-plane semantics already provide one cleanly.
- For all-active subscriptions, pin v0 behavior to the active vault set at subscription time. Vaults created after subscription start are not included unless a later spec amendment says otherwise.
- Stream NDJSON:
  - each file event as one JSON line;
  - lag detection emits `stream_lagged` as one JSON line and keeps streaming where possible;
  - disconnect stops the task without affecting the daemon.
- Filtering happens at stream time by vault ID.
- Use `tokio_stream` only if already available transitively and appropriate; otherwise use axum's existing streaming body utilities without adding a new crate. No new dependency unless the implementation proves it is necessary and asks first.

**Files likely touched**:

- `src/api/mod.rs`
- `src/api/vaults.rs`
- `src/api/types.rs`
- `src/client.rs`
- possibly `Cargo.toml` only if a new streaming helper crate is unavoidable; expected answer is no new crate.

**Tests**:

- HTTP handler/integration test: subscribe, write a real `.md` file, assert one NDJSON `file_changed` event with the vault ID and path.
- HTTP test: unknown vault selector returns `404 vault_not_found`.
- HTTP test: `--all`/aggregate subscription receives events from two active vaults and filters out inactive vaults according to the pinned v0 rule.
- HTTP lag test only if deterministic with a small channel capacity injection; otherwise cover lag at the unit level in Task 16.2 and document why an integration lag test would be artificial.

**Risk**: medium-high. Long-lived HTTP bodies add cancellation and timing edges, but the endpoint is a thin subscriber over the in-memory bus.

### Task 16.5 -- CLI `hmn vault watch`

**Purpose**: preserve the practical tailing experience without preserving file tailing.

**Work**:

- Add `hmn vault watch [NAME|ID] [--all]`.
- Implement it as a thin client over the HTTP streaming endpoint.
- Default selector behavior:
  - `hmn vault watch NAME|ID` watches that vault;
  - `hmn vault watch` resolves the daemon config's `default_vault_name`;
  - `hmn vault watch --all` watches the active set at subscription time.
- Output each received NDJSON line to stdout unchanged.
- Ensure stderr carries errors and user-facing diagnostics; stdout is reserved for event lines.
- Exit when:
  - the daemon closes the stream;
  - the user interrupts the process;
  - the selected vault is terminated and the stream ends;
  - the HTTP request fails.
- Update rescan prompt/user-facing text from "outbox events" to "live change events."
- Remove `render_status_text` outbox output.

**Files likely touched**:

- `src/bin/hmn.rs`
- `src/cli.rs`
- `src/client.rs`
- `tests/cli.rs`

**Tests**:

- CLI parser/render tests for `hmn vault watch`.
- CLI integration smoke: spawn daemon, start `hmn vault watch <name>` subprocess, write a file, read one JSON line from stdout, assert `type=file_changed`.
- CLI error test for unknown vault selector if the existing CLI test harness can cover it without brittle subprocess timing.

**Risk**: medium. Subprocess + streaming + watcher debounce has several timing axes; keep the smoke narrow and time-bounded.

### Task 16.6 -- MCP Streaming Decision And Cleanup

**Purpose**: keep the MCP surface honest instead of shipping a fake long-lived tool.

**Work**:

- Verify the currently pinned `rmcp` version's server-side support for long-lived tool streaming, resource subscriptions, or server notifications over stdio and Streamable HTTP.
- If a clean implementation is small and fits the step:
  - add `vault_watch` with the same live-only semantics;
  - keep it read-only and not gated by `enable_write_tools`;
  - add narrow MCP tests for advertised schema and one stream smoke if the transport supports it without flake-prone harnessing.
- If no clean implementation exists:
  - do not add `vault_watch` to `src/mcp/server.rs`;
  - remove or narrow docs that currently claim it ships;
  - add a short "MCP streaming deferred" note to `docs/specs/change-events.md` and `docs/specs/vault-management.md`;
  - make sure `tools/list` expectations in tests do not include `vault_watch`.

**Files likely touched**:

- `src/mcp/server.rs`
- `src/mcp/backend.rs`
- `src/mcp/backend_in_process.rs`
- `tests/mcp.rs`
- `docs/specs/change-events.md`
- `docs/specs/vault-management.md`
- `docs/reference/cli.md`

**Tests**:

- If deferred: tests assert the existing MCP request/response tool surface is unchanged except for stale prose cleanup.
- If shipped: add focused tool/stream tests and run `cargo test --test mcp` 3x consecutively.

**Risk**: medium-high if shipped; low if deferred. The workplan default is defer unless rmcp makes the correct shape obvious.

### Task 16.7 -- Remove Status, Config, Migration, And Fixture Leftovers

**Purpose**: finish excising the durable outbox surface from configuration, status, migration, and fixtures.

**Work**:

- Remove `StorageConfig.outbox_file`, `default_outbox_file`, config docs, config tests, and any sample TOML references.
- Remove `OutboxStatus` and `StatusResponse.outbox`.
- Update `/status` and `hmn status` output to report only current index status.
- Update legacy migration:
  - stop moving `outbox.jsonl` as a first-class migration artifact;
  - if needed, leave old files alone or ignore them during migration;
  - do not delete arbitrary legacy files unless the behavior is already clearly scoped and tested.
- Update `VaultEntry` construction in tests and helpers.
- Remove `src/outbox/` and `pub mod outbox`.
- Delete `tests/outbox.rs` or replace it with live-event tests under a better name such as `tests/change_events.rs`.

**Files likely touched**:

- `src/config.rs`
- `src/api/types.rs`
- `src/api/status.rs`
- `src/bin/hmn.rs`
- `src/client.rs`
- `src/legacy_state_migration.rs`
- `src/api/tests.rs`
- `src/control_plane/tests.rs`
- `src/mcp/backend_in_process.rs`
- `tests/config.rs`
- `tests/cli.rs`
- `tests/mcp.rs`
- `tests/watch.rs`
- `tests/vault_control_plane.rs`
- `tests/multi_vault_internal.rs`
- `tests/outbox.rs` (delete or replace)

**Tests**:

- Existing config tests updated to reject no removed key unless compatibility policy says unknown keys are tolerated. Do not keep `outbox_file` as a no-op config key unless a concrete compatibility need appears during implementation.
- Status API tests updated for the new wire shape.
- CLI status tests updated for the new text output.
- Migration tests updated so legacy outbox files are not treated as active runtime state.

**Risk**: medium. This is broad compile-fix cleanup across many fixtures.

### Task 16.8 -- Docs, Search Sweep, And Shipping Gate Verification

**Purpose**: prove the repo no longer tells or tests the old durable outbox story.

**Work**:

- Run repository-wide searches:
  - `rg -n "outbox|outbox_file|outbox.jsonl|tail -f|JSONL" src tests docs notes`
  - remaining `notes/roadmap/archive`, retros, historical ADR revision entries, and backlog references may remain if they are clearly historical;
  - no active source, active specs, active reference docs, or tests should describe JSONL outbox as a live runtime surface.
- Run focused tests:
  - event module unit tests;
  - watcher/live-event tests;
  - HTTP watch tests;
  - CLI watch smoke;
  - MCP tests if Task 16.6 touched the MCP surface.
- Run full quality gate:
  - `cargo fmt`
  - `cargo test`
  - `cargo clippy -- -D warnings`
  - `git diff --check`
- Manual smoke:
  - start `hmnd` against a temp vault;
  - run `hmn vault watch <name>`;
  - write/edit/delete a `.md` file;
  - capture one or more NDJSON events;
  - verify no outbox file is created under `<data_dir>/vaults/<id>/` and none is created under the watched vault.

**Risk**: medium-high. The manual smoke is the round shipping gate: it proves the replacement is real, not just a deleted test file.

## Files Likely Touched

Runtime:

- `src/events.rs` or `src/events/mod.rs` (new)
- `src/outbox/` (delete)
- `src/lib.rs`
- `src/config.rs`
- `src/watcher/mod.rs`
- `src/control_plane/manager.rs`
- `src/control_plane/runner.rs`
- `src/api/mod.rs`
- `src/api/vaults.rs`
- `src/api/status.rs`
- `src/api/types.rs`
- `src/client.rs`
- `src/bin/hmn.rs`
- `src/cli.rs`
- `src/mcp/server.rs`
- `src/mcp/backend.rs`
- `src/mcp/backend_in_process.rs`
- `src/legacy_state_migration.rs`

Tests:

- `tests/outbox.rs` (delete or replace)
- `tests/watch.rs`
- `tests/cli.rs`
- `tests/mcp.rs`
- `tests/config.rs`
- `tests/multi_vault_internal.rs`
- `tests/vault_control_plane.rs`
- `src/api/tests.rs`
- `src/control_plane/tests.rs`
- `src/mcp/server.rs` unit tests
- `src/mcp/backend_in_process.rs` unit tests

Docs:

- `docs/specs/change-events.md`
- `docs/specs/vault-management.md`
- `docs/architecture/overview.md`
- `docs/reference/configuration.md`
- `docs/reference/cli.md`
- `docs/decisions/0006-outbox-outside-watched-directory.md`

## Test Strategy

- Unit tests pin event JSON and bus behavior.
- Watcher/live-event tests replace `tests/outbox.rs` JSONL assertions.
- HTTP tests prove subscription, selector errors, and all-active filtering.
- CLI tests prove `hmn vault watch` can receive a real file-change event from a running daemon.
- Existing search, vault lifecycle, MCP request/response, config, and migration tests are updated only where they referenced the removed outbox surface.
- Full gate is `cargo test` plus `cargo clippy -- -D warnings`.
- Anti-flake convention remains: use explicit timeouts around streaming reads; do not mask event-ordering bugs with indefinite polling or retries.

## Non-Goals

- No writes to the watched vault.
- No durable event store.
- No replay / `since` / retained history.
- No stream generations or sequence numbers.
- No custom error type crate.
- No new backend abstraction beyond the small event bus needed for this step.
- No broad MCP streaming architecture unless rmcp's existing shape makes it a small, verified addition.

## Definition Of Done

- [ ] The durable JSONL outbox writer and module are removed.
- [ ] No runtime code opens, appends, tails, reports, migrates, or configures an outbox file as active state.
- [ ] `storage.outbox_file` is gone.
- [ ] `/status` and `hmn status` no longer report outbox path/size.
- [ ] The watcher/indexer path publishes real live events after content-hash-confirmed changes.
- [ ] HTTP live watch works for at least one vault selector.
- [ ] `hmn vault watch` streams NDJSON events from a running daemon.
- [ ] MCP streaming is either implemented with verified rmcp framing or explicitly deferred in active docs and tests.
- [ ] `tests/outbox.rs` no longer encodes the old durable contract.
- [ ] Active docs describe one model: live-only change events, index-as-source-of-truth recovery, durable replay deferred.
- [ ] `cargo fmt`, `cargo test`, `cargo clippy -- -D warnings`, and `git diff --check` are clean.
- [ ] Manual smoke proves a real file change produces a live event and no outbox file is created.

## Build-Phase Notes For Coordinator

- Do not read `notes/coordinator-playbook.md` until the human approves the workplan and says build/go/approved.
- In build phase, read only the COORDINATOR section first. The TASK AGENT section can wait until task-agent prompts are needed. Do not read the ORCHESTRATOR section.
- Likely task split:
  - 16.1 docs contract cleanup;
  - 16.2 event module and bus;
  - 16.3 watcher/manager rewire;
  - 16.4 HTTP watch endpoint;
  - 16.5 CLI watch;
  - 16.6 MCP decision;
  - 16.7 broad cleanup/tests;
  - 16.8 verification/docs sweep.
