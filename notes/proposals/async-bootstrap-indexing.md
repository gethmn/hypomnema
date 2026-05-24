# Async Bootstrap Indexing — Daemon Responsiveness During Initial Scan

**Status**: Draft
**Date**: 2026-05-07

---

## Summary

`hmnd` currently blocks its HTTP listener for the entire duration of the initial vault scan. With a ~900-file Markdown vault, clients cannot connect at all until indexing finishes — there is no `/status`, no `/health`, no MCP, no error response. From the outside the daemon looks dead.

This proposal makes the listener bind immediately and runs the initial scan as a background task, while exposing an `Indexing | Ready | Errored` bootstrap state per vault through the existing `/status` surface. It does **not** change the data plane, the watcher invariant, or the on-disk schema.

It is a contained, near-term fix — explicitly *not* the larger architectural reshape (concurrent watcher-during-bootstrap, durable bootstrap state, push-based progress events) that remains deferred.

---

## Motivation

**Observed behavior (today).** Pointing `hmnd` at a real ~900-file vault: the process starts, logs `hmnd: starting daemon`, and then sits silent for the duration of the initial scan. `curl /status` from another shell hangs (connection refused) until the scan completes. The expected "indexing in progress" status that a user would assume from the async-everywhere posture of the codebase does not exist.

**Why this surprises.** The scanner already uses `spawn_blocking` for SQL work internally, so it is reasonable to expect the runtime to stay responsive. It does not, because the bootstrap *orchestration* is awaited inline — the listener binding sits **after** the await for every vault's full initial scan in `src/bin/hmnd.rs`.

**Real-world impact.** Any vault large enough that the initial scan exceeds a few hundred milliseconds is, from a client's perspective, a dead daemon during startup. This affects:

- First-run experience after `hmn vault create` against an existing notes directory.
- Daemon restart with multiple registered vaults.
- Any environment where embeddings are slow (cold model, network embedding endpoint).

---

## Current Baseline

### What was actually planned

Searching the existing specs/ADRs:

- `docs/decisions/0003-indexing-in-the-daemon.md` decides indexing happens in the daemon. **Silent on bootstrap-vs-serve ordering.**
- `docs/specs/vault-management.md` describes vault create as "start watcher + indexer, then return" — synchronously. The MVP implicitly assumed startup is fast.
- `docs/specs/mcp-streamable-http.md` explicitly defers progress notifications during long operations.
- `docs/specs/semantic-search.md` §263–271 introduces a search-response `hint: "semantic index is building"` for the "files exist but no chunks yet" case. This is the **only** indexing-aware status anywhere in the system, and it is reactive (search-side), not proactive.

So: a daemon-wide "indexing" state was never specified. The MVP simply assumed startup would not take long enough for clients to notice.

### What is implemented

- `src/bin/hmnd.rs:111-119` — `VaultManager::open(...)` is awaited.
- `src/bin/hmnd.rs:161` — only **after** the manager returns is the HTTP listener bound.
- `src/control_plane/manager.rs:1259-1358` — `spawn_runner_parts` runs the entire initial scan (`scanner.run().await`, line 1280-1283) before returning. Watcher only spawns afterward (line 1304-1339).
- The scanner (`src/indexer/mod.rs:83-150`) does use `spawn_blocking` for individual SQL transactions. That keeps the runtime healthy *within* the scan. It does not help that nothing else runs *until* the scan is done.

### Existing scaffolding a fix can build on

- `VaultStatus` enum (`src/vault_registry/mod.rs:70`) — persisted state: `Active | Paused | Errored`. The bootstrap state we need is **in-memory only** (per-process) and orthogonal; this enum is unchanged.
- `StatusResponse` (`src/api/types.rs`) — already returns aggregate `indexed_file_count` + `last_indexed_at`. Extending it additively keeps v0 clients working.
- `EventBus` (`src/control_plane/events.rs`) — could carry a `VaultBootstrapped` event for free if cheap.
- Per-vault shutdown joiner pattern (`manager.rs:1320-1326`) — reusable so the bootstrap task respects daemon-wide shutdown.
- Search-side `hint` (`docs/specs/semantic-search.md` §263) — the precedent that "search results during indexing return partial data with a hint" is already accepted by the project.

---

## Why This Is Not a "Major Architecture Change"

The blocking is concentrated in **one** place: `spawn_runner_parts` awaits `scanner.run()` inline. The store opens *before* the scan runs, so a `VaultEntry` with a usable `Arc<Store>` can be installed in the manager *before* the scan starts. Search/status handlers already query the store directly — they will simply see partial data during indexing, which is the desired behavior.

No data-plane refactor. No schema change. No new dependency. No new abstraction.

---

## Design Options

### Option A — Background bootstrap, pull-only status (recommended)

Split `spawn_runner_parts` into two phases:

1. **Synchronous phase** (must finish before `VaultManager::open` returns):
   - Open `Store`.
   - Construct `VaultEntry` with a new in-memory `bootstrap_state: Arc<RwLock<BootstrapState>>` set to `Indexing { started_at, files_seen: 0, files_indexed: 0 }`.
   - Insert the entry into the manager's runners map.
2. **Background phase** (`tokio::spawn`, parented under daemon shutdown):
   - Run `scanner.run().await` (unchanged internally).
   - Update progress counters at low frequency (every N files or M ms).
   - On success, flip `bootstrap_state` to `Ready` and *then* spawn watcher + consumer (preserves the invariant that the consumer never observes events against an unindexed store).
   - On failure, flip to `Errored(msg)` and emit on the event bus.

`VaultManager::open` returns immediately once entries are inserted. `run_daemon` proceeds to bind the HTTP listener while indexing continues in the background.

**Status surface.** Extend `StatusResponse` additively:

```jsonc
{
  "vaults": [
    {
      "name": "...",
      "path": "...",
      "bootstrap": {
        "state": "indexing",            // "indexing" | "ready" | "errored"
        "started_at": "...",
        "files_seen": 412,
        "files_indexed": 198,
        "message": null
      },
      "indexed_file_count": 198,
      "last_indexed_at": "..."
    }
  ],
  // legacy top-level fields kept for back-compat
  "vault": "...",
  "indexed_file_count": 198,
  "last_indexed_at": "..."
}
```

Top-level fields stay populated as today (sum across vaults). v0 clients keep working.

**Search behavior during bootstrap.** No code change required for correctness — partial results are returned. The existing semantic-search `hint` already covers the "no chunks yet" case. *Optional:* widen the hint to fire whenever `bootstrap_state == Indexing`.

**`hmn vault create`.** Same change applies symmetrically (`manager.rs::create_vault`, ~line 532) so creating a vault from the CLI does not reintroduce blocking.

### Option B — Push-based progress events on the SSE bus

Same as Option A plus a `VaultBootstrapProgress { files_indexed, total_estimate }` event stream. Useful for a future TUI/UI but not required for the bug. **Recommendation: defer.**

### Option C — Concurrent watcher-during-bootstrap

Start the watcher *before* the initial scan completes; consumer queues events for files the scan has not visited yet. Solves the (currently theoretical) "files modified during a 30-minute scan get missed until next event" hole. This is the "major architecture change" the user is open to deferring. **Recommendation: defer.**

### Option D — Durable bootstrap state across restarts

Persist scan progress so a restart mid-scan resumes instead of restarting. Not needed for the responsiveness fix. **Recommendation: defer.**

---

## Proposed Direction

**Ship Option A as a single-step round.** It is contained, additive on the wire, preserves the watcher invariant, and gives clients the responsiveness and visibility the user expected on day one.

Explicitly **not** in scope for this round (each is a candidate for a future round if it earns it):

- Option B — progress events.
- Option C — concurrent watcher-during-bootstrap.
- Option D — durable bootstrap state.
- Adding `bootstrap` to `/health`. (Out of scope unless an existing health-check pattern makes it trivial.)
- Persisting `bootstrap_state` to `vaults.sqlite`. In-memory only.

---

## Files Likely Touched

- `src/bin/hmnd.rs` — verify ordering still works (likely no change beyond a comment; the manager now returns fast).
- `src/control_plane/manager.rs` — load-bearing change. Split `spawn_runner_parts`; add `bootstrap_state` to `VaultEntry`; spawn the scan + watcher-start sequence as a background task. Audit all `spawn_runner_for_row` callers (open, resume, reset, create).
- `src/control_plane/manager.rs::create_vault` (~line 532) — symmetric async-bootstrap behavior for the `hmn vault create` path.
- `src/api/types.rs` — extend `StatusResponse` additively (per-vault block + bootstrap field).
- `src/api/status.rs` — populate the per-vault `bootstrap` block by reading from the manager.
- `src/control_plane/events.rs` — *optional* `VaultBootstrapped` event for completion/failure.
- Tests: `src/control_plane/tests.rs`, `src/api/tests.rs`, `tests/vault_control_plane.rs`, `tests/mcp_http.rs`, `tests/cli.rs` — many assume "indexed" when `open()` returns. Add a `vault_manager.wait_for_bootstrap().await` helper so test semantics stay one line away from current behavior.

## Files Not Touched

- `src/vault_registry/mod.rs` — no change to `VaultStatus`. Bootstrap state is per-process, in-memory.
- `src/indexer/mod.rs` — `Scanner::run` unchanged. It already uses `spawn_blocking` correctly.
- Schema files (`src/store/schema.rs`, registry migrations) — no schema change.

---

## Error Shape

| Condition | Surface | Behavior |
|---|---|---|
| Scan task panics | `bootstrap_state = Errored(msg)`; event-bus error event | `/status` reports `bootstrap.state = "errored"` with `message`. Daemon stays up. Vault is queryable (returns whatever was indexed before the panic). |
| Daemon shuts down mid-scan | Bootstrap task is cancelled via the per-vault joiner | Same shutdown timeout as today. No partial-state corruption — each per-file insert is its own `spawn_blocking` transaction. |
| Embedding endpoint unavailable mid-scan | Existing scanner error path | `bootstrap_state = Errored(...)` with the underlying message. Operator can `hmn vault rescan` after fixing the endpoint. |

---

## Verification Plan

1. **The actual bug.** Point `hmnd` at the user's ~900-file vault. Within 1s of `hmnd: http server listening`, `curl localhost:<port>/status` should return `bootstrap.state == "indexing"`. (Today: connection refused for the duration of the scan.)
2. **Bootstrap completion.** Poll `/status`; observe `files_indexed` rising and the state flipping to `"ready"`.
3. **Partial search during bootstrap.** Issue content/semantic queries mid-scan; expect partial results, no panic, no 503.
4. **Shutdown mid-scan.** SIGTERM during indexing; daemon exits within shutdown timeout, no orphaned tasks.
5. **`hmn vault create`** against a 900-file directory while daemon is up; CLI returns promptly, vault appears in `/status` with `bootstrap.state == "indexing"`.
6. **`cargo test` and `cargo clippy -- -D warnings`** clean. Tests that depend on a fully indexed vault use the new `wait_for_bootstrap` helper.

---

## Open Questions

- [ ] Is a single-step round the right shape, or should "API change" (`StatusResponse` extension) and "control-plane change" (`spawn_runner_parts` split) be two steps for review hygiene?
- [ ] Should `wait_for_bootstrap` be a `pub` method on `VaultManager` (useful for `hmn vault status` blocking flag), or test-only?
- [ ] Should the `bootstrap` block also appear on `/health` (degraded while any vault is indexing), or is `/status` sufficient?
- [ ] Should the optional `VaultBootstrapped` event ship in this round, or is it Option B and deferred?
- [ ] Should the `hint` in semantic-search be widened to fire on `Indexing` state (currently fires only on zero chunks), or is that a separate touch-up?

---

## Related Documents

- [`docs/decisions/0003-indexing-in-the-daemon.md`](../../docs/decisions/0003-indexing-in-the-daemon.md)
- [`docs/specs/vault-management.md`](../../docs/specs/vault-management.md)
- [`docs/specs/semantic-search.md`](../../docs/specs/semantic-search.md) — `hint` precedent
- [`docs/specs/mcp-streamable-http.md`](../../docs/specs/mcp-streamable-http.md) — deferred progress notifications
- `src/bin/hmnd.rs:71-195` — startup sequence
- `src/control_plane/manager.rs:1234-1358` — `spawn_runner_for_row` / `spawn_runner_parts`
- `src/api/status.rs` — current `/status` handler
