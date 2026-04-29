# Amp Code Review — 2026-04-28

Findings from a full-tree code review run via Amp on round-4-shipped `v0.3.0`. The review surfaced ten items not already covered by [`backlog.md`](backlog.md). The three load-bearing items (search-error classification, `path_under` / `paths_equal` canonicalize handling, embedding-skipped consumer signal) were promoted to backlog § Round-6 candidates as research-and-confirm items pending a future roadmap. The remaining seven are captured here for future reference; promotion to backlog or a roadmap step happens if/when one becomes load-bearing.

> **Status**: Captured. None blocking. Clippy passes with `-D warnings`; tests green. The rules in [`AGENTS.md`](../AGENTS.md) (spawn_blocking discipline, no in-vault state, no rolled-own debouncing) are uniformly upheld.

---

## Carry-forward items (not yet promoted)

### 1. Watcher `RenameMode::Any` / `RenameMode::Other` — undirected-rename consumer test gap

[`src/watcher/translate.rs`](../src/watcher/translate.rs) lines 74-80 treat `RenameMode::Any`/`RenameMode::Other` (single path, no direction) as `Upsert` and rely on the consumer's `MissingFromDisk` path in [`watcher::apply_event`](../src/watcher/mod.rs) to follow up with a `remove` when the upsert reveals the file is gone. The translate-layer comment promises this; the consumer-level integration tests in [`tests/watch.rs`](../tests/watch.rs) and `watcher/mod.rs` § tests don't cover it. Worth a regression test that fires an `Upsert` for a non-existent path and asserts a `Deleted` outbox event lands.

### 2. Outbox `sync_data` runs inside the file mutex

[`src/outbox/writer.rs`](../src/outbox/writer.rs) lines 42-52 (`Outbox::append`) take the `Mutex<File>` and run `writeln!` *and* `sync_data` while holding it. `sync_data` is a syscall that can take milliseconds; concurrent appenders serialize on it. With one consumer per vault this is rarely contended, but the existing `concurrent_appends_all_land` test only verifies correctness, not throughput. If the manager ever issues parallel rescans into the same outbox the lock-during-fdatasync pattern will surface. Fix shape if/when needed: write into a buffer outside the mutex, then take it briefly to swap into the file and call `sync_data` on a clone of the file handle.

### 3. `validate_dimension` reads the `chunks_vec` schema with a regex

[`src/store/mod.rs`](../src/store/mod.rs) lines 143-160 parses `embedding FLOAT[<dim>]` out of `sqlite_master.sql` with a regex. This works because we control the migration SQL, but it couples startup validation to the exact CREATE statement spelling. A future migration that spells the column differently (whitespace, casing, an inline comment) breaks `Store::open` at startup with a "could not locate `embedding FLOAT[<dim>]`" error. Two cheap mitigations: (a) pin the spelling with a code comment that points back to the regex, or (b) drive validation off a probe `vec_len()` call from sqlite-vec instead of parsing the schema text.

### 4. Lifecycle / op_lock contract is reviewer-enforced, not type-enforced

[`src/control_plane/manager.rs`](../src/control_plane/manager.rs): every lifecycle-mutating op is required to take `runner.op_lock` before touching `runner.lifecycle`. The contract lives only in code review. A future op that forgets the lock will produce a hard-to-reproduce torn-write. Fix shape if/when needed: colocate `lifecycle` + `op_lock` in a dedicated struct that only exposes a single `with_op_lock(|lifecycle| ...)` method, or remove `op_lock` and put the lifecycle behind a single `Mutex<Option<RunnerLifecycle>>` whose ownership semantics naturally serialize.

### 5. `RescanResponse.row` reflects pre-acceptance state

[`src/control_plane/manager.rs`](../src/control_plane/manager.rs) `rescan` (around line 1030) fetches the `VaultRow` *before* signalling the consumer, so a `rename` that lands between fetch and response returns a stale name. `pause` / `resume` re-fetch post-update, but rescan specifically uses the pre-update row. Either re-fetch post-signal, or document that the row reflects the moment of acceptance, not of completion.

### 6. `Config::default_for_smoke_test` is `#[cfg(test)]` only

[`src/config.rs`](../src/config.rs) lines 428-440. Used by the in-crate unit tests in `src/watcher/mod.rs` § tests; not visible to integration tests in [`tests/`](../tests/), which build their own `Config` instead. Either drop the `#[cfg(test)]` and rename to something less smoke-specific (e.g. `Config::for_vault_path`), or add a comment explaining the in-crate-only intent. Annotation-footgun rather than a bug.

### 7. Stale doc comment on `ControlPlaneError::VaultErrored`

[`src/control_plane/manager.rs`](../src/control_plane/manager.rs) lines 69-74 says "reserved for step 11", but the variant is now used by `resume`/`reset` (lines ~663, ~700, ~937). Trivial — drop the "reserved" framing and describe the live use.

---

## Format note for future reviews

This file is the catch-all for "noticed during a code review, worth recording, not yet load-bearing enough for the backlog." Each entry: (1) the file + line region, (2) the concrete shape of the issue, (3) a sketched fix-shape so a future reader doesn't need to re-derive it. Promotion path: when an item becomes load-bearing (a bug surfaces, a workplan touches the same surface, an architectural decision depends on it), it moves to [`backlog.md`](backlog.md) as a roadmap-candidate item and this file's entry gets a "**Promoted to backlog YYYY-MM-DD**" strikethrough.
