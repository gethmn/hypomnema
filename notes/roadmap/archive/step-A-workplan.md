# Step A Workplan -- `notify` + `notify-debouncer-full` upgrade

**Step**: A of 2 (round 7: Dependency Upgrade Round). See [`roadmap-7.md`](./roadmap-7.md) for the round framing and sequencing. This step lands the watcher-stack major-version bumps together: `notify` 6 -> 8 and `notify-debouncer-full` 0.3 -> 0.7.

**Status**: Shipped 2026-05-01.

**Goal recap**

- Upgrade `notify` from `6.1.1` to `8.2.0`.
- Upgrade `notify-debouncer-full` from `0.3.2` to `0.7.0`.
- Keep the watcher module compiling cleanly.
- Preserve watcher semantics unless the new debouncer behavior changes an observable test expectation.
- Prove the upgrade with a real-file-change smoke pass.

**Relevant current surface**

- `Cargo.toml` currently pins `notify = "6"` and `notify-debouncer-full = "0.3"`.
- `src/watcher/mod.rs` constructs a `notify-debouncer-full` debouncer and translates events into `WatchEvent` values.
- `src/watcher/translate.rs` owns the event-kind mapping, including rename decomposition and relevance filtering.
- `tests/watch.rs` exercises the end-to-end watcher path against a live daemon-backed consumer loop.

**Deferred decisions to resolve while building**

- Whether the `notify-debouncer-full` 0.7 `FileIdCache` trait/ownership changes require a code adjustment in `src/watcher/mod.rs`, or whether the existing `FileIdMap` usage still satisfies the new bounds.
- Whether `notify-debouncer-full` 0.7's `Modify`-after-`Create` suppression changes any observable event counts in `tests/watch.rs`.
- Whether the step needs any docs/spec cleanup beyond a short note in the step results comment.

**Skill**

- `filesystem-watching` is the primary skill for this step.

**Risk**

- Medium. The watcher is load-bearing, but the upgrade surface is narrow and the version jumps are well defined.

---

## Tasks

### Task A.1 -- Bump watcher dependencies and fix compile breakage

Update `Cargo.toml` to `notify = "8"` and `notify-debouncer-full = "0.7"`, then resolve any compiler errors in `src/watcher/` and the immediate callsites. Verify the code still uses the same high-level shape: debounced notify events, translation at the boundary, blocking send on the channel, no extra I/O or async work in the callback.

**Deliverable**: a compiling watcher stack with the upgraded dependency pins.

**Expected files**: `Cargo.toml`, `Cargo.lock`, `src/watcher/mod.rs`, and any narrow follow-on edits in `src/watcher/translate.rs` or nearby callsites.

### Task A.2 -- Reconcile watcher tests and smoke the event pipeline

Run the watcher integration tests and update only the assertions that genuinely changed because of the new debouncer behavior. Then run a real-file-change smoke pass against a live daemon to confirm create / modify / delete still flow through the consumer surface.

**Deliverable**: green watcher tests plus a verified real-file-change smoke.

**Expected files**: `tests/watch.rs` only if the debouncer change alters observable expectations.

---

## Test strategy

- `cargo check` after the dependency bump to surface the compile inventory quickly.
- `cargo test --test watch` to cover the watcher integration surface if a targeted rerun is enough to isolate failures.
- `cargo test` before closing the step.
- Real-file-change smoke against a live daemon after the watcher tests are green.
- `cargo clippy -- -D warnings` before declaring the step done.

## Definition of done

- `Cargo.toml` and `Cargo.lock` pin the watcher stack at the upgraded versions.
- The watcher module compiles cleanly with no unreviewed workaround.
- Existing watcher and watch integration tests pass, with any changed assertion explicitly justified.
- A live-file smoke confirms the debounced event pipeline still behaves as expected.
- No unrelated polish or docs work sneaks into the step.

## Cross-references

- [`notes/roadmap/roadmap-7.md`](./roadmap-7.md) -- round framing and sequencing.
- [`notes/coordinator-playbook.md`](../coordinator-playbook.md) -- coordinator/task-agent contract.
- [`src/watcher/mod.rs`](/Users/beausimensen/Code/hypomnema/src/watcher/mod.rs) -- the debouncer integration point.
- [`tests/watch.rs`](/Users/beausimensen/Code/hypomnema/tests/watch.rs) -- watcher integration coverage.

## Out of scope

- `axum` upgrade work belongs to Step B.
- Any unrelated dependency bumps outside the watcher stack.
- Spec or docs rewrites unless the upgrade produces a concrete behavior change that needs recording.

## Net new dependencies

- None beyond the requested version upgrades.

## Process dependencies

- Step A should finish before Step B starts so the dependency diffs stay bisectable.
- If the upgraded debouncer changes observable behavior, record that in the step results comment before moving on.
