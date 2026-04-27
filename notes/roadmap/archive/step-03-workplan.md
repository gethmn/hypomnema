# Step 3 Workplan — Watcher

**Roadmap step**: [Step 3 — Watcher](./roadmap-1.md#step-3--watcher)
**Status**: Shipped 2026-04-25
**Created**: 2026-04-25

---

## Goal recap

`hmnd` (default action) watches the configured vault while it runs. When
`notify-debouncer-full` reports a change for a `.md` file under the vault that
survives the relevance and ignore filters, the indexer re-hashes the file and
updates the corresponding row in `index.sqlite`. When a watched file disappears,
the row is deleted. Files that survive the filesystem filters but whose bytes
have not changed produce zero DB writes (the content-hash gate from step 2 is
re-used).

The shipping criteria from the roadmap are:

1. Editing a `.md` file in the vault updates its row's `content_hash`.
2. Dropping a `*.sync-conflict-*` file produces no DB write.
3. Deleting a watched file removes its row.
4. Saving a file without changing bytes produces zero DB writes
   (mtime-only changes are ignored).
5. The daemon survives a sustained editor save loop without runaway CPU.

Step 3 is **index-only**: no outbox, no JSONL, no event-shape decisions beyond
the internal channel. The outbox is step 4 and lands on top of this step's
single-file reindex helpers without changing them.

The watcher runs as part of `hmnd`'s default `run_daemon` path. `hmnd scan`
remains the one-shot reconcile from step 2; nothing about its surface changes.

## Deferred-decision resolutions

The roadmap flagged two TBDs for this step.

### 1. Debounce window tuning

**Resolution**: bump the default `watcher.debounce_ms` from `400` (set
defensively in step 1) to **`500`**, matching the
[`filesystem-watching` skill's](../../../.claude/skills/filesystem-watching/SKILL.md)
recommended default. Keep it configurable; do not tune it speculatively further.

**Why**: the roadmap explicitly directs this step to "start with skill's
recommended default." The skill's reasoning is that 500ms is short enough that
interactive use does not feel laggy, long enough to coalesce most editor save
patterns. The 100ms delta from the step-1 defensive default is small
behaviorally but matters as a documented choice — once the watcher is live, the
default is what every user inherits, and the skill is the project's load-bearing
source of truth on watcher-pattern decisions.

The existing config-default test (`tests/config.rs:55`) and config sample
fixture (`tests/config.rs:255`) assert `debounce_ms == 400`; both flip to
`500` in Task 3.1 alongside `default_debounce_ms()` and the configuration
reference. Any user who needs the older value can set it explicitly; the skill
notes that sync-tool bursts may justify 1–2s, surfaced as a config-doc note
rather than a default change.

### 2. Rename-as-distinct-event vs. delete+create pair

**Resolution**: in v0, treat renames as a **delete + create pair** at the
watcher boundary. The watcher's translation layer decomposes
`notify-debouncer-full`'s rename-shaped events into our internal
`WatchEvent::Remove(old_rel_path) + WatchEvent::Upsert(new_rel_path)` pair
before pushing them onto the channel. The downstream consumer sees two events
and processes them as two independent updates against the index.

**Why**: this matches the existing language in
[`docs/specs/change-events.md`](../../../docs/specs/change-events.md) (line 42:
"Renames are observed as a `deleted` + `created` pair in v0; fused rename
detection is an open question") and avoids introducing a third
`WatchEvent::Renamed` variant whose only consumer in v0 would be the indexer
treating it as `Remove(old)` then `Upsert(new)` anyway. Step 4's outbox will
inherit this naturally — it serializes whatever the watcher emits — so the
spec's `created`/`modified`/`deleted` event-type set holds without an
additional `renamed` value.

A fused `renamed` event is genuinely useful for consumers that want to
preserve identity across a rename (semantic-search cache invalidation, agent
memory of "this is the same file"). That is a legitimate v1 feature; the
delete+create decomposition does not block it. When fused renames land
later, the watcher's translation layer is the single place to change, and
the spec's open question on line 98 of `change-events.md` becomes the place
to document the final shape.

The corresponding spec line — already correct — gets a one-word tightening
in Task 3.6 to upgrade "v0" from a parenthetical aside to an explicit "v0
behavior, fused renames are a v1 open question" sentence with a back-link to
this workplan.

---

## Tasks (ordered, each independently mergeable)

Six tasks. Each lands as its own commit. Step 2's retro left no carry-over
cleanup for step 3 to gate on; the first task here is real watcher work.

### Task 3.1 — Watcher filter helpers + bump default debounce

**Files**:
- `src/config.rs` — `default_debounce_ms()` returns `500` instead of `400`.
- `tests/config.rs` — flip the two `400` literals to `500`.
- `docs/reference/configuration.md` — change the `debounce_ms` row's default
  to `500`; update the sample TOML block; add one prose line that sync tools
  occasionally justify bumping to 1–2s.
- `src/lib.rs` — `pub mod watcher;`
- `src/watcher/mod.rs` (new, stub for now) — module declaration only:
  `mod filter;` and a `// step-3 task 3.2 lands the Watcher type here.` line
  so the module compiles before Task 3.2.
- `src/watcher/filter.rs` (new) — three pure functions:
  - `is_relevant_path(path: &Path) -> bool` — returns `false` for paths whose
    components include any dotfile-prefixed segment (`.git/`, `.obsidian/`,
    `.trash/`, leading-dot files), and `false` for paths whose extension is
    not `md`. Matches the skill's `is_relevant` example.
  - `is_sync_conflict(path: &Path) -> bool` — matches the three sync-tool
    conflict-name patterns from the skill: Syncthing
    `*.sync-conflict-*`, Obsidian Sync `(conflicted copy *)`, Dropbox
    `(<device>'s conflicted copy)`.
  - `vault_relative(canonical_vault: &Path, abs_path: &Path) -> Option<String>`
    — strips the canonical vault prefix, emits a forward-slash-joined
    `String`, returns `None` on prefix-mismatch or non-UTF-8.
  - Eight or so unit tests covering each helper's true / false branches.

**What lands**:
- One config default change (400→500) with tests updated.
- Three filter helpers, fully unit-testable without `notify`.
- Stub `src/watcher/mod.rs` so `cargo build` is green at task boundary.

**Why first**: pure-logic helpers move first so Task 3.2 (which adds the
`notify` deps and the threading model) has the filter primitives ready to
compose into the watcher's translation layer. Splitting them out also keeps
the test surface of the filter logic separate from the test surface of the
event pipeline — when an integration test fails, "the filter is wrong" vs
"the channel is wrong" is one bisect step apart.

**Note on filter scope**: `is_sync_conflict` is **complementary** to the
existing `ignore_patterns` globset, not redundant. The default globset only
catches Syncthing's `*.sync-conflict-*` shape; Obsidian Sync and Dropbox
patterns include parentheses and would not match the globset without each user
adding their own patterns. The helper closes that gap so a fresh-install user
gets all three sync-tool conflict shapes filtered without configuration.

### Task 3.2 — `notify` + `notify-debouncer-full` deps; `Watcher` module

**Files**:
- `Cargo.toml` — add `notify = "6"`, `notify-debouncer-full = "0.3"`.
- `src/watcher/mod.rs` (replace stub) — public surface:
  - `pub enum WatchEvent { Upsert(String), Remove(String) }` — both variants
    carry a vault-relative forward-slash path. Already-filtered: every event
    on the channel is for a `.md` file under the vault that survived
    `is_relevant_path`, the configured `ignore_patterns` globset, and
    `is_sync_conflict`.
  - `pub struct Watcher { _debouncer: ... }` — owns the debouncer handle so it
    is kept alive for the daemon's lifetime. Drop = stop watching.
  - `pub fn spawn_watcher(vault: &Path, ignores: GlobSet, debounce: Duration,
    buffer: usize) -> Result<(Watcher, mpsc::Receiver<WatchEvent>)>`.
- `src/watcher/translate.rs` (new) — `pub(super) fn translate(events:
  Vec<DebouncedEvent>, ctx: &TranslateCtx) -> Vec<WatchEvent>`. Pure function
  over the debouncer's `DebouncedEvent` slice + a `TranslateCtx` (canonical
  vault, ignore globset reference). Handles:
  - `EventKind::Create(_)` and `EventKind::Modify(ModifyKind::Data(_))` →
    `Upsert(rel)` after filter pass.
  - `EventKind::Remove(_)` → `Remove(rel)` after filter pass.
  - `EventKind::Modify(ModifyKind::Name(RenameMode::Both))` (with both paths
    in `event.paths`) → `Remove(old_rel) + Upsert(new_rel)` per
    [§ Deferred decision 2](#2-rename-as-distinct-event-vs-deletecreate-pair).
  - `EventKind::Modify(ModifyKind::Name(RenameMode::From | To | Any))` —
    same decomposition where applicable; otherwise treat as a single-path
    Upsert/Remove based on which direction notify reported.
  - `EventKind::Modify(ModifyKind::Metadata(_))` → drop (mtime/permission
    bumps without data change; the content-hash gate would catch them in the
    consumer anyway, but dropping early avoids a syscall storm).
  - `EventKind::Access(_)` and `EventKind::Other` → drop.
  - Unknown / future variants → drop with a `tracing::trace!` log.
  - Unit tests against synthetic `DebouncedEvent` values for each branch.

**What lands**:
- Two new runtime crates. `notify` is the de facto Rust filesystem-watching
  crate; `notify-debouncer-full` is its companion that the skill mandates.
  Both MIT/Apache. Together they pull in `crossbeam-channel` (light) and a
  small set of OS-shim crates already common in tree.
- The watcher runs the `notify` callback on a thread `notify` owns. Per the
  skill ("Don't do reindex work inside the callback"), the callback's only
  job is: call `translate`, then `tx.blocking_send(events)` for each event in
  the resulting `Vec`. No I/O, no SQL.
- Channel: `tokio::sync::mpsc::channel(buffer)`. Default buffer = 256
  (twice the skill's 128 because we may emit two events per rename and the
  channel carries individual `WatchEvent`s rather than batches). Buffer size
  exposed as a parameter to `spawn_watcher` so tests can pin it; the
  daemon-side caller in Task 3.4 uses the default.
- Errors from `notify` go to `tracing::warn!` per the skill's example.

**Why a separate task**: this is the dependency-introduction commit. Keeping
it apart from Task 3.3's indexer changes lets a reviewer evaluate the new
crates and the `Watcher` shape on their own merits before consuming them. It
is also the highest-risk task in this step (the project's biggest landmines
per the roadmap live in the watcher); isolating it makes a bisect easy if
something breaks two tasks later.

**Open subtle point — symlinks**: notify's default behavior does not follow
symlinks, so a symlinked-in file edited externally will not produce an event
the watcher sees. This diverges from the step-2 walker (which follows
symlinks via `WalkDir::follow_links(true)`). Acceptable in v0: the daemon
already re-scans on startup (step 2's resolution 1), so out-of-tree symlink
changes get picked up on next restart. Documented as a known v0 limitation in
the watcher module's doc-comment; not a deferred decision worth promoting.

**Skill applied**: `.claude/skills/filesystem-watching/`. Every code site
that touches `notify` or `notify-debouncer-full` cross-references one of the
skill's named patterns or anti-patterns.

### Task 3.3 — Indexer single-file `reindex_path` + `remove_path`

**Files**:
- `src/indexer/mod.rs` — add two methods on `Scanner`:
  - `pub async fn reindex_path(&self, rel: &str) -> Result<ReindexOutcome>`
  - `pub async fn remove_path(&self, rel: &str) -> Result<RemoveOutcome>`
- `src/indexer/mod.rs` — add the outcome enums:
  - `pub enum ReindexOutcome { Inserted, Updated, HashUnchanged, MissingFromDisk }`
  - `pub enum RemoveOutcome { Removed, NotPresent }`
- A small private helper `single_file_blocking(...)` that opens a single
  short-lived transaction (one `BEGIN`/`COMMIT` per event), reads the
  existing row by `path`, runs the same `(size, mtime)` stat-gate then hash
  comparison the bulk scan uses, and writes the appropriate
  `INSERT`/`UPDATE`/`DELETE`. Branches mirror the bulk-scan logic so the two
  paths agree by construction.
- `MissingFromDisk` is the right thing to return when a file disappears
  between the watcher event and the indexer call (race window is real but
  small): the consumer in Task 3.4 treats it as a soft `Remove` — issue a
  `remove_path` follow-up and proceed.
- Unit tests on `Scanner` mirroring the bulk-scan tests but exercising the
  single-file methods: insert-then-reindex-no-bytes-changed →
  `HashUnchanged`; reindex of a new file → `Inserted`; reindex with bytes
  flipped → `Updated`; reindex of a missing path → `MissingFromDisk`;
  remove of a present path → `Removed`; remove of an absent path →
  `NotPresent`.

**What lands**:
- Two new public methods on `Scanner`. Both use `tokio::task::spawn_blocking`
  same as `Scanner::run`. The private `single_file_blocking` shares the
  `(size, mtime)`-gate / hash-then-compare logic with the bulk scan via a
  small helper extracted from `run_blocking` so the two cannot drift.
- `ScanReport` is unchanged (bulk scan remains the only thing that produces
  one). The watcher's consumer in Task 3.4 keeps a cumulative counter set
  scoped to the watcher loop; it is not a `ScanReport`.

**Decisions encoded here (not new ADRs)**:
- One short-lived transaction per event. The bulk scan wraps the whole walk
  in one transaction (atomic reconciliation); the watcher cannot — events
  arrive over time, and holding a transaction open for the daemon's lifetime
  blocks the bulk scan path and the future search-API queries. Per-event
  `BEGIN`/`COMMIT` is the right shape; WAL + `synchronous=NORMAL` (already
  set in step 2) keeps the cost low.
- The `MissingFromDisk` race is handled by the consumer (Task 3.4), not the
  indexer. Letting the indexer return a precise outcome instead of papering
  over the race keeps the two callers (events vs. tests) honest.

**Why a separate task**: the indexer changes are pure logic that compose with
the watcher; building them in isolation, with their own tests, makes the
Task 3.4 wiring task small and obvious.

**Skill applied**: `.claude/skills/rusqlite-in-async/` (per-event
`spawn_blocking` plus per-event short transactions).

### Task 3.4 — Wire watcher into `hmnd run_daemon`

**Files**:
- `src/bin/hmnd.rs` — `run_daemon` extended:
  1. Initial scan (existing; no change).
  2. Construct the `GlobSet` once for the watcher (same compiled set the
     scanner uses).
  3. Call `watcher::spawn_watcher(&config.vault.0, ignores,
     Duration::from_millis(config.watcher.debounce_ms), 256)?` — hold onto
     the returned `Watcher` for the daemon's lifetime.
  4. Spawn a `tokio::spawn` consumer task that owns the receiver, the
     `Scanner` handle, and a shutdown receiver clone. The task loop:
     - `tokio::select!` between `rx.recv()` and the shutdown signal.
     - On `WatchEvent::Upsert(rel)` → `scanner.reindex_path(&rel).await` →
       log outcome at `debug` (or `warn` on `MissingFromDisk` followed by
       `scanner.remove_path(&rel)`).
     - On `WatchEvent::Remove(rel)` → `scanner.remove_path(&rel).await` →
       log outcome at `debug`.
     - On error from a single event: log at `warn`, continue. Per-event
       errors do not kill the consumer task.
     - On shutdown branch firing: drain whatever is already in the channel
       (best-effort, time-boxed to one second), then break.
  5. Drop the `Watcher` after the consumer finishes draining so the
     debouncer thread shuts down cleanly. This happens implicitly when
     `run_daemon` returns; the explicit drop ordering is documented in a
     comment.
  6. Existing shutdown receiver-await line stays; the unchanged
     "drain complete, exiting cleanly" log line still fires.
- `src/bin/hmnd.rs` startup banner gets one new field:
  `debounce_ms = %config.watcher.debounce_ms`.

**What lands**:
- The watcher is **on** in default `hmnd`. Every editor save in the vault now
  results in a debounced upsert; every file deletion results in a row removal.
- Backpressure: the channel has a finite buffer. If the consumer is slower
  than the watcher (large bulk import, sync-tool storm), `blocking_send` in
  the notify thread will block until the consumer drains. Per the skill,
  this is the desired backpressure shape — log it (one `warn` line per N
  blocked sends), do not bump the buffer to "fix" it.
- `hmnd scan` and `hmnd config-validate` are unchanged. Only the default
  (no-subcommand) path gets the watcher.

**Why this task is medium-high risk**: it composes the new dependencies
(Task 3.2), the new indexer methods (Task 3.3), the existing shutdown
plumbing, and the existing scanner under one async runtime. The skill's
"Don't do reindex work in the callback" rule is enforced here by spawning
the consumer task; testing that this composition holds up under a sustained
save loop is shipping criterion 5 and is exercised in Task 3.5.

### Task 3.5 — Integration tests against a real tempdir vault

**Files**:
- `tests/watch.rs` (new) — integration tests that exercise the live
  watcher against a real tempdir vault. Each test:
  1. Builds a `Fixture` (similar to `tests/scan.rs`) — tempdir vault,
     tempdir data_dir, `Config` constructed via `Config::load`.
  2. Opens the `Store`, runs an initial `Scanner::run` (mirrors what
     `run_daemon` does at startup).
  3. Calls `watcher::spawn_watcher(...)` with a short debounce (50ms) so
     tests do not have to wait long.
  4. Spawns the same consumer task shape as the daemon (factored into a
     `pub(crate)` helper in `src/bin/hmnd.rs` or, more naturally, hoisted
     into `hypomnema::watcher::run_consumer` so the test does not depend on
     the binary). Recommended: hoist into the lib.
  5. Performs a filesystem operation, awaits a deterministic settle window
     (`tokio::time::sleep` for `2 × debounce`), then queries the store and
     asserts.
  6. Drops the watcher; cleanup is implicit.

**Cases (mirrors the roadmap's shipping criteria + a few extras)**:
1. Edit an existing `.md` file → row's `content_hash` updates; row count
   unchanged. (Criterion 1.)
2. Drop a `My Note.sync-conflict-202604.md` file into the vault → no row
   appears for it; existing row count unchanged. (Criterion 2.)
3. Drop a `(conflicted copy 2026-04-25).md` file → no row appears.
   (Skill's Obsidian / Dropbox conflict-pattern coverage.)
4. Delete an existing `.md` file → its row disappears. (Criterion 3.)
5. `set_modified` on a file without changing bytes → row's
   `content_hash` stays identical; row count unchanged. (Criterion 4.)
6. Create a brand-new `.md` file in a nested subdirectory →
   row appears with the right vault-relative path
   (forward-slash, no leading `./`).
7. Move a file from `notes/a.md` to `notes/b.md` (Unix `rename`) →
   `notes/a.md` row gone, `notes/b.md` row present. Verifies the
   delete+create rename decomposition resolved in
   [§ Deferred decision 2](#2-rename-as-distinct-event-vs-deletecreate-pair).
8. Drop a `.git/HEAD.md` file → no row appears. (Verifies the watcher
   honors the same default `ignore_patterns` as the scanner.)
9. Sustained save loop: in a tight `for _ in 0..50` write the same bytes
   to the same file, settle, assert exactly one or two rows updated and
   no `MissingFromDisk` from the consumer. Smoke test for criterion 5;
   the assertion is "test completes within 5 seconds and the row's
   `content_hash` matches the final-write content."

**Footnote on flake**: the settle window of `2 × debounce` is a balance
between fast tests and false-negative flakes. If a test flakes once on CI,
double the settle window for that test rather than the default. Do not
introduce a polling-loop helper that hides the timing — flakes on a
non-deterministic boundary are signal.

**Cross-platform**: tests use Unix-only operations
(`std::os::unix::fs::symlink`, `std::os::unix::fs::rename` flavors) where
applicable, gated on `#[cfg(unix)]` per step 2's pattern. macOS + Linux
covered; Windows out of v0.

### Task 3.6 — Reference docs reflect step-3 resolutions

**Files**:
- `docs/reference/configuration.md`
  - `[watcher]` table: `debounce_ms` default flips `400` → `500`. Sample
    TOML block updates the literal. New short prose line: "Sync tools that
    burst-write across more than the debounce window may justify
    `debounce_ms = 1000` or `2000`; do not raise it speculatively — the
    watcher logs backpressure, raise it when you see those logs."
- `docs/reference/cli.md`
  - `hmnd` (no subcommand) section: tighten "starts the watcher" by adding
    one line: "Implemented in step 3; the watcher runs for the daemon's
    lifetime, debounces filesystem events, and updates the index in place
    for files whose content hash changed."
- `docs/specs/change-events.md`
  - Line 42 parenthetical "(Renames are observed as a `deleted` + `created`
    pair in v0; fused rename detection is an open question.)" is upgraded
    to a full sentence with a back-link to this workplan's deferred
    decision 2: "v0 behavior, confirmed in step 3: renames are observed as
    a `deleted` + `created` pair. Fused rename detection remains open
    (line 98)."
  - The corresponding open question on line 98 stays open.
- `docs/architecture/overview.md`
  - § Watcher: keep the existing prose; add one short line confirming
    step 3 ships the implementation behind it.
- `notes/roadmap/archive/roadmap-1.md`
  - Step 3 gets `**Status**: shipped <date>` at the top of its section
    (filled in at the actual ship moment, not at workplan time).

---

## Test strategy

**Unit tests** (`#[cfg(test)]`):
- `src/watcher/filter.rs`: pure-logic coverage of `is_relevant_path`,
  `is_sync_conflict`, `vault_relative` (Task 3.1).
- `src/watcher/translate.rs`: synthetic `DebouncedEvent` values for each
  notify variant the translator handles, plus the rename
  decomposition (Task 3.2).
- `src/indexer/mod.rs`: extended with single-file `Scanner::reindex_path` /
  `remove_path` cases (Task 3.3).
- `src/config.rs`: existing `default_debounce_ms` test passes with the new
  value (Task 3.1).

**Integration tests** (`tests/`):
- `tests/watch.rs` (new) — nine cases enumerated in Task 3.5, each in its
  own tempdir.
- `tests/skeleton.rs` — no new cases. (`hmnd` default action gains the
  watcher but its skeleton-level surface — process exits cleanly on
  shutdown — is unchanged from step 2.)
- `tests/config.rs` — two `400` → `500` literal flips (Task 3.1); no new
  cases.

**No new test for**: bulk-scan behavior. Step 2's tests already cover the
hash-gate logic that the watcher's single-file methods reuse.

**Lint and format**: `cargo clippy --all-targets -- -D warnings` and
`cargo fmt --all -- --check` before review.

**Cross-platform**: tests run on macOS (developer machine) and Linux (when
CI lands). `notify` has a per-OS backend (FSEvents on macOS, inotify on
Linux); the integration tests exercise both implicitly. Windows is out of
v0 scope; rename / symlink tests gate on `#[cfg(unix)]` per step 2.

---

## Definition of done

- [ ] Editing a `.md` file in the vault updates its row's `content_hash`
      (shipping criterion 1).
- [ ] A `*.sync-conflict-*` file dropped in the vault produces no DB write;
      same for `(conflicted copy *)` (Obsidian/Dropbox patterns) (criterion
      2 + skill-driven extras).
- [ ] Deleting a watched file removes its row (criterion 3).
- [ ] Saving a file without changing bytes produces zero DB writes — the
      stat-gate or hash-gate intercepts before any `UPDATE` (criterion 4).
- [ ] A 50-iteration tight save loop completes in well under 5 seconds and
      produces exactly the expected final-state row (criterion 5).
- [ ] Renaming a file decomposes into `Remove(old) + Upsert(new)` and the
      DB reflects exactly the new path (deferred decision 2).
- [ ] `hmnd` (no subcommand) starts the watcher and the consumer task on
      top of the existing scan + idle path; SIGINT exits 0 with the
      drain-complete log line.
- [ ] All filesystem-changing SQL goes through `tokio::task::spawn_blocking`
      via the existing `Scanner` plumbing — verified by reading the diff.
- [ ] No reindex work runs inside the `notify-debouncer-full` callback (the
      callback only translates and `blocking_send`s) — verified by reading
      `src/watcher/mod.rs`.
- [ ] `cargo test`, `cargo clippy --all-targets -- -D warnings`,
      `cargo fmt --all -- --check` all pass.
- [ ] `docs/reference/configuration.md`, `docs/reference/cli.md`, and
      `docs/specs/change-events.md` reflect the resolutions.
- [ ] Roadmap marks Step 3 shipped with the date.
- [ ] Step 3 retrospective appended to
      `notes/project-planning-workflow-notes.md` (using the retro template).

---

## Cross-references

**Specs / decisions**:
- [`specs/change-events.md`](../../../docs/specs/change-events.md) — line 42 (rename
  v0 behavior, tightened in Task 3.6); line 98 (fused-rename open question,
  unchanged).
- [ADR-0003: Indexing in the daemon](../../../docs/decisions/0003-indexing-in-the-daemon.md)
  — the watcher is the streaming half of "indexing in the daemon."
- [ADR-0006: Outbox outside watched dir](../../../docs/decisions/0006-outbox-outside-watched-directory.md)
  — the watcher reads from the vault and writes only to the data-dir
  SQLite file, by construction.

**Reference docs (updated by this step)**:
- [Configuration reference](../../../docs/reference/configuration.md)
- [CLI reference](../../../docs/reference/cli.md)
- [Change-events spec](../../../docs/specs/change-events.md)
- [Architecture overview](../../../docs/architecture/overview.md)

**Pitfalls touched**:
- #1 *Blocking the async runtime with rusqlite* — single-file ops also use
  `spawn_blocking`; per-event short transactions, not a long-held one.
- #2 *Watcher event storms during editor saves and sync operations* —
  the central pitfall of this step. Mitigated by `notify-debouncer-full`
  per the skill; verified by Task 3.5 case 9 (sustained save loop).
- #3 *Spurious re-indexing from mtime-only change detection* — single-file
  path uses the same `(size, mtime)`-then-hash gate as the bulk scan.
- #4 *Sync-conflict files* — closed by Task 3.1's `is_sync_conflict`
  helper at the watcher boundary. Step 2 covered partial filtering via
  `ignore_patterns`; this step covers the broader pattern set the globset
  cannot reach.

**Skills applied**:
- `.claude/skills/filesystem-watching/` — every site that touches `notify`
  or `notify-debouncer-full` cross-references one of the skill's named
  patterns or anti-patterns. The skill is canonical for this step.
- `.claude/skills/rusqlite-in-async/` — single-file ops are `spawn_blocking`
  with per-event short transactions; the skill's pattern enforced.

**Skills that don't apply yet**:
- `sqlite-vec-extension` — step 6.
- `markdown-chunking` — step 6.

---

## Out of scope (will not appear in this PR)

- The outbox JSONL writer and any `outbox.jsonl` plumbing (step 4).
- Any new event-type values beyond what `WatchEvent` needs internally.
  In particular, no `WatchEvent::Renamed` (deferred decision 2).
- The HTTP server, `/health`, `/search/*` (step 5).
- `--rescan` flag (step 2 deferred it; still deferred).
- Self-write event suppression — v0 is read-only; the skill calls this out
  as a later-phase concern.
- Health-endpoint conflict-file metric. The `is_sync_conflict` filter
  drops conflict files; counting them and surfacing the count is a
  step-5 / health-endpoint concern.
- Channel buffer auto-tuning. The buffer is a fixed-default constant;
  raising it adaptively would mask backpressure rather than report it.
- A new `tokio::sync::Notify`-style "indexer is idle" signal for tests
  to await on. The integration tests use a deterministic settle window
  (`2 × debounce`); this is sufficient for v0 and avoids the test
  surface depending on internal indexer state.

If review surfaces a strong reason to pull any of the above forward,
that's a roadmap revision per the
[mid-step roadmap revision](../../project-planning-workflow-notes.md#open-questions-about-the-workflow-itself)
open question.

---

## Net new dependencies

| Crate | Where | Why |
|-------|-------|-----|
| `notify` | runtime | OS-native filesystem watching (FSEvents / inotify / ReadDirectoryChangesW) |
| `notify-debouncer-full` | runtime | Coalesces editor / sync-tool event storms per the skill — required, never roll your own |

Two crates, both in the roadmap's "step 3 deps" line, both MIT/Apache.
Transitive surface: `crossbeam-channel` (already a common transitive in
tree via `tracing`), and a small set of OS-shim crates from `notify`'s
backend selection. No `regex-syntax` movement, no `serde` feature flips.

`tempfile` is already in `[dev-dependencies]` from step 2; the new
integration test file in Task 3.5 reuses it without a Cargo.toml change.
