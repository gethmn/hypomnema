# Step 4 Workplan — Outbox

**Roadmap step**: [Step 4 — Outbox](./roadmap.md#step-4--outbox)
**Status**: drafted, awaiting review
**Created**: 2026-04-26

---

## Goal recap

`hmnd` (default action) emits a durable change-event stream as JSONL to
`~/.local/share/hypomnema/outbox.jsonl`. The outbox is fed by the same
`WatchEvent` stream the indexer consumes — so outbox emission is downstream of
the content-hash gate and only "real changes" reach the file. Consumers tail
the file (`tail -f outbox.jsonl`) and see one JSON object per line.

The shipping criteria from the roadmap are:

1. Editing a watched file appends one JSONL line with `event_type: "modified"`.
2. mtime-only touch appends nothing.
3. Deleting a file appends `event_type: "deleted"` with the last known
   `content_hash`.
4. `tail -f outbox.jsonl` works as a consumer interface end-to-end.
5. The outbox file is never written under the watched vault directory.

Step 4 is **outbox-only**: no rotation, no consumer byte-offset checkpoint
API, no push notifications, no webhooks. Per the roadmap, those are
explicitly out of shipping-gate scope.

The outbox attaches to the same consumer task that drives the indexer in
step 3 (`watcher::run_consumer`). It does not introduce a new long-running
task or a new channel.

## Deferred-decision resolutions

The roadmap flagged two TBDs for this step.

### 1. fsync policy: per-event vs. periodic

**Resolution**: **per-event `sync_data`** (`fdatasync` on Linux/macOS).
After each JSONL line is written, the outbox writer calls
`File::sync_data()` inside the same `spawn_blocking` closure that performed
the write, and only returns `Ok(())` to the caller after the sync returns.

**Why**: durability of the change-event stream is the outbox's job — it is
the only place a consumer can recover events from. The realistic event rate
is bounded by user save rate plus rare sync-tool bursts (the debouncer +
hash-gate already coalesce both); we are not in a regime where one
`fdatasync` per JSONL line — small file, sequential append — bottlenecks the
daemon. The roadmap's risk-grade for this step is "low," and the simplest
durability shape is also the strongest.

The spec's existing crash-safety promise ([§ Edge Cases / Crash during
write](../specs/change-events.md#edge-cases)) — "the daemon on restart picks
up from the end-of-file; no duplicate is emitted for events that made it
through before the crash" — is consistent with per-event durability. A
periodic-fsync alternative would require either a flush-on-shutdown contract
(adding lifetime surface area to the writer) or accept silent loss of the
last N events on crash; both are strictly worse for v0.

`sync_data` (rather than `sync_all`) is the right call because the file's
metadata changes (size, mtime) are not load-bearing for consumers — only the
appended bytes are. `sync_data` skips the metadata flush and is measurably
faster on both ext4 and APFS.

If profiling under a real sync-tool storm later shows per-event sync as a
hot spot, the upgrade path is a hybrid (sync after N events or M ms,
whichever first). That is a strict superset of per-event sync, so v0 ships
the strongest durability we will ever offer; relaxing it later is easier
than tightening it.

### 2. Rename handling

**Resolution**: **two JSONL lines per rename** — confirms the v0 shape
already resolved at the watcher boundary in
[step 3 § Deferred decision 2](./step-03-workplan.md#2-rename-as-distinct-event-vs-deletecreate-pair).

The watcher decomposes a rename into `WatchEvent::Remove(old) +
WatchEvent::Upsert(new)` before the events leave the translation layer. The
indexer then reports a `Removed{previous_hash}` outcome for `old` and an
`Inserted{content_hash}` outcome for `new` (or `Updated{content_hash}` if
`new` was already known under that path — rare but possible). The outbox
serializes whatever the indexer produced: one `deleted` line, one `created`
(or `modified`) line. No coalescing into a single `renamed` event.

**Why**: this matches the language [step 3 tightened in
`docs/specs/change-events.md` line 42](../specs/change-events.md): "v0
behavior, confirmed in step 3: renames are observed as a `deleted` +
`created` pair. Fused rename detection remains open (line 98)." Step 4 has
nothing new to decide here — the watcher already produced the v0 shape; the
outbox just serializes it.

The line 98 open question (fused `renamed` event type) stays open. A v1
implementation would change the watcher's translation layer and add a new
event type to the outbox envelope; that path is documented but not pursued.

### Resolved as part of this step (not pre-flagged in the roadmap): `content_hash` on `deleted` events

The roadmap shipping criterion 3 says deletes append "with the last known
content_hash." The current spec column note
([change-events.md table](../specs/change-events.md#data-schema)) says
content_hash is "yes for create/modify, null for delete." These are in
mild conflict; resolving in favour of the roadmap.

**Resolution**: `deleted` events include the prior `content_hash` from the
indexed row, when known. Cost is zero — the indexer already has to read the
row before deleting it, so the prior hash is in hand at outbox-emit time.
Value is non-zero — agents that maintain a content-hash-keyed cache can
correlate a `deleted` event with the cached version they had; forensic
queries can answer "what version of this file was last seen alive?".

The spec's column note flips to "yes when known; always for create/modify;
for delete, the last known hash from the index" in Task 4.6. The JSON
example in the spec gains a populated `content_hash` on a delete-shaped
illustration (or stays as a modified-shaped one, with one new line of prose
clarifying the delete case).

The rare edge case where the daemon truly has no prior hash for a deleted
path — a `WatchEvent::Remove` for a path that was never indexed — emits
nothing to the outbox at all (the indexer returns `RemoveOutcome::NotPresent`
and the outbox-emission branch is conditional on a real change). So the
schema never has to express "deleted with null hash" in practice.

---

## Tasks (ordered, each independently mergeable)

Six tasks. Each lands as its own commit. Step 3's retro left no carry-over
cleanup for step 4 to gate on (only playbook-level cleanups, called out in
[§ Process dependencies](#process-dependencies) below); the first task here
is real outbox work.

### Task 4.1 — `ChangeEvent` type + serde envelope

**Files**:
- `src/lib.rs` — `pub mod outbox;`
- `src/outbox/mod.rs` (new, stub for now) — module declaration:
  ```rust
  mod event;
  pub use event::{ChangeEvent, EventType};
  // step-4 task 4.2 lands the Outbox writer here.
  ```
  Stub keeps the module compilable at task boundary.
- `src/outbox/event.rs` (new) — the `ChangeEvent` struct and `EventType`
  enum:
  ```rust
  #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
  pub struct ChangeEvent {
      pub event_type: EventType,
      pub path: String,
      #[serde(skip_serializing_if = "Option::is_none")]
      pub content_hash: Option<String>,
      pub detected_at: String, // RFC3339 UTC, µs precision
  }

  #[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
  #[serde(rename_all = "lowercase")]
  pub enum EventType { Created, Modified, Deleted }

  impl ChangeEvent {
      pub fn now(event_type: EventType, path: String, content_hash: Option<String>) -> Self {
          Self {
              event_type,
              path,
              content_hash,
              detected_at: chrono::Utc::now()
                  .to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
          }
      }
  }
  ```
- Five or so unit tests:
  - `EventType` round-trips through serde_json with lowercased discriminants
    (`"created"`, `"modified"`, `"deleted"`).
  - `ChangeEvent` with `Some` hash serializes the `content_hash` field;
    with `None` the field is **omitted** rather than emitted as `null` —
    this keeps the line shorter and consumer parsing simpler. (Confirmed
    against the resolved spec column note: "yes when known" implies "when
    not known, the field is absent.")
  - Round-trip: `serde_json::from_str(serde_json::to_string(&ev)?)?` equals
    `ev` for all three event types.
  - `ChangeEvent::now` produces a `detected_at` parseable by
    `chrono::DateTime::parse_from_rfc3339`.

**What lands**:
- One pure-data module, no I/O.
- No new dependencies. `serde`, `serde_json`, and `chrono` are already in
  `Cargo.toml` from steps 1–2.
- `chrono`'s `serde` feature is **not** enabled — `detected_at` is a
  `String` produced via the same `to_rfc3339_opts(SecondsFormat::Micros,
  true)` call already used in `src/indexer/mod.rs:173` for `now_iso`.
  Storing as a String avoids pulling chrono's `serde` feature flag.

**Why first**: pure-logic types move first so Task 4.2 (the writer) has the
envelope ready to serialize. Splitting also keeps the type's tests isolated
from the file-handling tests; when an integration test fails, "the envelope
is wrong" vs "the writer is wrong" is one bisect step apart.

### Task 4.2 — `Outbox` writer with per-event `sync_data`

**Files**:
- `src/outbox/mod.rs` (replace stub) — `pub use event::{ChangeEvent,
  EventType}; mod writer; pub use writer::Outbox;`
- `src/outbox/writer.rs` (new) — the `Outbox` struct:
  ```rust
  pub struct Outbox {
      path: PathBuf,
      file: Arc<Mutex<std::fs::File>>,
  }

  impl Outbox {
      pub async fn open(path: PathBuf) -> Result<Self> {
          let path_for_blocking = path.clone();
          let file = task::spawn_blocking(move || {
              std::fs::OpenOptions::new()
                  .create(true)
                  .append(true)
                  .open(&path_for_blocking)
                  .with_context(|| format!("opening outbox at {}", path_for_blocking.display()))
          })
          .await
          .context("spawn_blocking join error in Outbox::open")??;
          Ok(Self { path, file: Arc::new(Mutex::new(file)) })
      }

      pub async fn append(&self, event: ChangeEvent) -> Result<()> {
          let file = self.file.clone();
          task::spawn_blocking(move || -> Result<()> {
              let line = serde_json::to_string(&event).context("serializing change event")?;
              let mut g = file.lock().expect("outbox mutex poisoned");
              writeln!(*g, "{line}").context("writing outbox line")?;
              g.sync_data().context("fdatasync on outbox")?;
              Ok(())
          })
          .await
          .context("spawn_blocking join error in Outbox::append")?
      }

      pub fn path(&self) -> &Path { &self.path }
  }
  ```
- Six or so unit tests:
  - Open → append three events → reopen the file with `std::fs::read_to_string`
    → assert three lines, each parsing back to the input.
  - Open on an existing non-empty file preserves prior contents (manually
    pre-write a line, open, append, assert both lines present).
  - Open on a path whose parent directory does not exist returns an error
    (does not panic).
  - Concurrent `append` calls from multiple tasks — even though the consumer
    is sequential in production, this verifies the `Mutex` discipline is
    correct: spawn 10 tasks each calling `append` once, await all, assert 10
    lines on disk. (Order is not asserted.)
  - `path()` returns the path passed to `open`.
  - One test that explicitly drops the `Outbox` and re-opens at the same
    path and appends one more line; assert the prior contents survive.

**What lands**:
- One new module with one public type. All file I/O via `task::spawn_blocking`.
- `std::sync::Mutex` (not tokio's) is fine because the lock is only ever
  held inside the `spawn_blocking` closure — never across an `await`.
- No new dependencies.

**Why a separate task**: the writer's bare surface — open + append, with the
fsync choice — is the load-bearing piece a reviewer should evaluate on its
own merits. Separating it from the consumer wiring (Task 4.4) means the
fsync decision lives in its own commit with its own tests. Same shape as
step 3 Task 3.2.

**Skill applied**: `.claude/skills/rusqlite-in-async/` — the same
`spawn_blocking` discipline the project applies to rusqlite I/O extends to
synchronous file I/O. Outbox writes go through the same gate.

### Task 4.3 — Indexer outcomes carry `content_hash`

**Files**:
- `src/indexer/mod.rs` — extend the outcome enums:
  ```rust
  pub enum ReindexOutcome {
      Inserted { content_hash: String },
      Updated { content_hash: String },
      HashUnchanged,
      MissingFromDisk,
  }

  pub enum RemoveOutcome {
      Removed { previous_hash: String },
      NotPresent,
  }
  ```
- `single_file_blocking` already computes `hash` inside `upsert_file_in_tx`
  for the `Inserted` and `Updated` branches. Thread it back out of
  `UpsertEffect`:
  ```rust
  enum UpsertEffect {
      Inserted { hash: String },
      Updated { hash: String },
      HashMatched,    // bulk-scan path doesn't need the hash
      StatGateHit,
  }
  ```
  Bulk-scan path (`run_blocking`) discards the hash by pattern-matching:
  `UpsertEffect::Inserted { .. } => report.inserted += 1`. Single-file path
  forwards the hash into `ReindexOutcome::Inserted { content_hash: hash }`.
- `remove_blocking` — currently runs `DELETE FROM files WHERE path = ?` and
  decides the outcome from the affected-rows count. Change to **read the
  prior hash inside the same transaction before the DELETE**:
  ```rust
  let tx = conn.transaction()?;
  let prior: Option<String> = tx.query_row(
      "SELECT content_hash FROM files WHERE path = ?1",
      params![rel],
      |row| row.get(0),
  ).optional()?;
  let n = tx.execute("DELETE FROM files WHERE path = ?1", params![rel])?;
  tx.commit()?;
  Ok(match (n, prior) {
      (0, _) => RemoveOutcome::NotPresent,
      (_, Some(h)) => RemoveOutcome::Removed { previous_hash: h },
      (_, None) => RemoveOutcome::NotPresent, // defensive: row read failed
  })
  ```
  Two-statement transaction is the right shape here (read-then-delete on the
  primary key); the `RETURNING` clause would be slightly slimmer but adds a
  rusqlite version constraint we don't need.
- Update the existing single-file unit tests to match the new outcome
  shapes:
  - `reindex_path_inserts_new_file` → assert
    `outcome == ReindexOutcome::Inserted { content_hash: <expected> }` where
    `<expected>` is `"sha256:" + hex` of the file bytes.
  - `reindex_path_returns_updated_when_bytes_change` → assert the new hash.
  - `remove_path_removes_present_row` → assert
    `outcome == RemoveOutcome::Removed { previous_hash: <expected> }`.
- Add three new tests:
  - `reindex_path_carries_inserted_hash` — pure assertion that the returned
    hash matches `hash_file(&path)`.
  - `reindex_path_carries_updated_hash` — same after a bytes-changed write.
  - `remove_path_carries_prior_hash` — insert a known file, remove, assert
    the prior hash matches what was inserted.

**What lands**:
- Same indexer logic, slightly richer return types so the outbox writer can
  fill in `content_hash` without a second DB lookup.
- The bulk-scan `ScanReport` is unchanged. Hash is only carried where
  consumers need it (the watcher's per-event consumer).

**Why a separate task**: this is a contract change to two public enums on
`Scanner`. Splitting it from the consumer wiring (Task 4.4) lets the indexer
change land with its own tests, matching step 3's task 3.3 pattern
(single-file ops landed before the watcher consumed them).

**Skill applied**: `.claude/skills/rusqlite-in-async/` — same per-event
`spawn_blocking` plus per-event short transactions. The new SELECT-before-
DELETE pattern stays inside one transaction.

### Task 4.4 — Wire `Outbox` into `run_consumer`; emit only on real changes

**Files**:
- `src/watcher/mod.rs` — extend `run_consumer`'s signature with an
  `Outbox` parameter (required, not `Option`):
  ```rust
  pub async fn run_consumer(
      mut rx: mpsc::Receiver<WatchEvent>,
      scanner: Scanner,
      outbox: Outbox,
      mut shutdown_rx: watch::Receiver<bool>,
  ) { ... }
  ```
  - Re-shape `apply_event` to take `&Outbox` and to emit one
    `outbox.append(...)` per real change. Sketch:
    ```rust
    async fn apply_event(ev: WatchEvent, scanner: &Scanner, outbox: &Outbox) {
        match ev {
            WatchEvent::Upsert(rel) => match scanner.reindex_path(&rel).await {
                Ok(ReindexOutcome::Inserted { content_hash }) => {
                    emit(outbox, EventType::Created, rel, Some(content_hash)).await;
                }
                Ok(ReindexOutcome::Updated { content_hash }) => {
                    emit(outbox, EventType::Modified, rel, Some(content_hash)).await;
                }
                Ok(ReindexOutcome::HashUnchanged) => { /* spec invariant: no event */ }
                Ok(ReindexOutcome::MissingFromDisk) => {
                    // existing remove-follow-up branch, but now if the
                    // follow-up reports Removed{prev_hash}, emit a deleted
                    // line.
                    match scanner.remove_path(&rel).await {
                        Ok(RemoveOutcome::Removed { previous_hash }) => {
                            emit(outbox, EventType::Deleted, rel, Some(previous_hash)).await;
                        }
                        Ok(RemoveOutcome::NotPresent) => { /* nothing was indexed; no event */ }
                        Err(e) => tracing::warn!(rel, error = ?e, "watcher: remove follow-up failed"),
                    }
                }
                Err(e) => tracing::warn!(rel, error = ?e, "watcher: upsert failed"),
            },
            WatchEvent::Remove(rel) => match scanner.remove_path(&rel).await {
                Ok(RemoveOutcome::Removed { previous_hash }) => {
                    emit(outbox, EventType::Deleted, rel, Some(previous_hash)).await;
                }
                Ok(RemoveOutcome::NotPresent) => { /* never indexed; no event */ }
                Err(e) => tracing::warn!(rel, error = ?e, "watcher: remove failed"),
            },
        }
    }

    async fn emit(outbox: &Outbox, event_type: EventType, rel: String, hash: Option<String>) {
        let ev = ChangeEvent::now(event_type, rel.clone(), hash);
        if let Err(e) = outbox.append(ev).await {
            tracing::warn!(rel, error = ?e, "watcher: outbox append failed");
        }
    }
    ```
  - **Order matters**: index update happens first, outbox append second. A
    failed outbox append logs `warn` and the consumer continues — the index
    is already updated; the outbox simply has a missing line for that one
    event. We do not roll back the index on outbox failure (the spec
    already accepts that consumers may have to skip bad/missing lines —
    [§ Edge Cases / Crash during write](../specs/change-events.md#edge-cases)).
  - The drain branch (`drain_remaining`) is updated to take `&Outbox` too,
    so events still in the channel at shutdown also get persisted.
- `src/bin/hmnd.rs` — `run_daemon` extended:
  1. After the initial scan, before `spawn_watcher`: open the outbox.
     ```rust
     let outbox_path = config.storage.data_dir.0.join(&config.storage.outbox_file);
     let outbox = Outbox::open(outbox_path.clone())
         .await
         .context("opening outbox")?;
     ```
  2. Pass `outbox` into `watcher::run_consumer(rx, scanner, outbox, shutdown_rx.clone())`.
  3. Startup banner gains one new field: `outbox = %outbox_path.display()`.
  4. The drop-ordering comment around `watcher_handle` stays unchanged. The
     outbox is dropped along with the consumer task at the end of
     `run_daemon`; the `Arc<Mutex<File>>` releases the file handle, the
     daemon-side append surface goes silent.

**What lands**:
- The outbox is **on** in default `hmnd`. Every real change (post hash-gate)
  appends one JSONL line.
- `hmnd scan` is unchanged. `Command::ConfigValidate` is unchanged. Only
  the no-subcommand `run_daemon` path opens the outbox and emits events.
- An outbox-open failure at startup is fatal (returns Err, daemon exits 1).
  An outbox-append failure mid-run logs `warn` and the consumer continues —
  same robustness shape as the watcher.

**Why this task is medium risk** (not high): it composes the new outbox
(Task 4.2), the richer indexer outcomes (Task 4.3), and the existing watcher
consumer under the same async runtime. The composition is structurally
mechanical — the consumer was already a sequential apply-then-log loop, and
the outbox emit is one more `await` per branch. The only fresh invariant is
"emit only on real changes" (the `HashUnchanged` and `NotPresent` branches
must not emit), which is enforced by exhaustive matching on the outcome
enums and verified by Task 4.5's case 2 (mtime-only touch produces zero
outbox lines).

### Task 4.5 — Integration tests against tempdir vault + outbox

**Files**:
- `tests/outbox.rs` (new) — integration tests that exercise the live
  watcher + outbox against a real tempdir vault. Each test:
  1. Builds a `Fixture` (mirrors `tests/watch.rs`'s pattern) — tempdir
     vault, tempdir data_dir, `Config` constructed via `Config::load`.
  2. Opens the `Store`, runs an initial `Scanner::run`.
  3. Constructs the `Outbox` against `data_dir.path().join("outbox.jsonl")`.
  4. Calls `watcher::spawn_watcher` with a short debounce (50 ms).
  5. Spawns the same `run_consumer` shape the daemon uses, now with the
     outbox handle.
  6. Performs filesystem operations, awaits a settle window
     (`tokio::time::sleep` for `2 × debounce` — the precedent from
     `tests/watch.rs`).
  7. Reads the outbox file with `std::fs::read_to_string`, splits into
     lines, parses each line with `serde_json::from_str::<ChangeEvent>`,
     and asserts.

- `tests/watch.rs` — existing tests need a small update: `run_consumer`'s
  signature gains an `Outbox` parameter, so each test constructs an
  `Outbox::open(data_dir.path().join("outbox.jsonl")).await?` and ignores
  what gets written. No assertion changes; just one extra fixture line per
  test. Nine tests, nine one-line additions.

**Cases (`tests/outbox.rs`, mirrors the roadmap's shipping criteria + a few
extras)**:

1. *Edit existing file emits one `modified` line.* (Criterion 1.) Write
   `# v1`, scan, then write `# v2 longer`, settle, read outbox: exactly
   one new line, `event_type == "modified"`, `path == "hello.md"`,
   `content_hash` matches `sha256(b"# v2 longer")`.
2. *mtime-only touch emits nothing.* (Criterion 2.) Write `# stable`,
   scan, then `set_modified` without changing bytes, settle, read outbox:
   zero lines.
3. *Delete emits one `deleted` line with the prior hash.* (Criterion 3.)
   Write `# bye`, scan to capture the row + hash, delete the file, settle,
   read outbox: exactly one new line, `event_type == "deleted"`, `path ==
   "bye.md"`, `content_hash == "sha256:..."` matching the bytes that were
   indexed.
4. *Create emits one `created` line.* Write a brand-new `notes/new.md`,
   settle, read outbox: one line, `event_type == "created"`,
   forward-slash `path`, hash matches.
5. *`tail -f` shape: re-reading the outbox file across multiple writes
   surfaces each line.* (Criterion 4.) Open the file with
   `std::fs::OpenOptions::new().read(true)`, capture an offset; perform a
   change; settle; read from the prior offset; assert the new bytes are
   exactly the new JSONL line. Repeat for a second change. This is the
   semantic the spec promises (§ Consumer Subscription); make it concrete
   in a test.
6. *Outbox path is in `data_dir`, not the vault.* (Criterion 5.) Assert
   `outbox.path() == data_dir.path().join("outbox.jsonl")` and that
   `vault_dir.path().join("outbox.jsonl").exists() == false` after several
   writes. Backstop is the existing config validator (rejects `data_dir`
   under `vault`); this case verifies the runtime behaviour matches the
   structural rule.
7. *Rename produces two lines: `deleted` then `created`.* (Confirms
   deferred decision 2.) Write `notes/a.md`, scan, `std::fs::rename` to
   `notes/b.md`, settle, read outbox: exactly two new lines. The
   `deleted` line carries the prior hash; the `created` line carries the
   new hash. Order is `deleted` first, then `created`, per the watcher's
   translation order. Gated on `#[cfg(unix)]` per step 3's precedent.
8. *Sync-conflict file emits zero outbox lines.* Drop a `My Note
   .sync-conflict-202604.md`, settle, read outbox: zero new lines. Same
   for `(conflicted copy 2026-04-26).md`. The watcher's filter intercepts
   before the indexer is called; verifies the boundary still holds with
   the outbox added.
9. *Sustained 50-write loop emits at most 1–2 lines.* Tight loop: write
   the same bytes 50 times to one file with no `tokio::time::sleep`
   between writes; settle (use `4 × debounce` per step 3's
   `sustained_save_loop_*` precedent); read outbox. Assert ≤ 2 lines and
   that the final-line `content_hash` matches the bytes on disk. The
   debouncer + hash-gate eat the rest.

**Per-test layout**: each case lives in its own `#[tokio::test]` and uses
its own tempdirs. No shared fixture state. Helper at the top of the file:
```rust
fn read_events(path: &Path) -> Vec<ChangeEvent> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}
```

**Footnote on flake**: same anti-flake rule as step 3 — no polling-loop
helpers that hide timing; if a case flakes once on CI, double its settle
window for that case rather than the default. Flakes on a non-deterministic
boundary are signal.

**Cross-platform**: `#[cfg(unix)]` gating on the rename test (case 7); all
other tests are platform-agnostic. macOS + Linux covered; Windows out of
v0.

### Task 4.6 — Reference docs reflect step-4 resolutions

**Files**:
- `docs/specs/change-events.md`:
  - Table row for `content_hash` flips from "yes for create/modify, null
    for delete" to "yes when known (always for create/modify; for delete,
    the last known hash from the index)". Add one prose line directly
    below the table: "When the daemon has no prior record for a deleted
    path — a rare race where the watcher reports a delete on a path that
    was never indexed — the outbox emits no event for it; the schema
    therefore never expresses delete-without-hash in practice."
  - Open question on line 97 (fsync policy) flips from `[ ]` to `[x]` with
    one resolution line: "Resolved in step 4 as per-event `sync_data`. See
    [step-4 workplan § Deferred decision 1](../roadmap/step-04-workplan.md#1-fsync-policy-per-event-vs-periodic)."
  - Open question on line 98 (rename detection) **stays open**. The line
    was already tightened in step 3; nothing new to say in step 4.
  - Open questions on lines 99 (rotation) and 100 (consumer byte-offset
    checkpoint) stay open per the roadmap's "explicitly out of
    shipping-gate scope" call.

- `docs/architecture/overview.md`:
  - § Outbox Writer: keep existing prose; append one short sentence: "Step
    4 ships the implementation: the watcher's consumer task — the same one
    that drives `Scanner::reindex_path` / `Scanner::remove_path` — emits
    one JSONL line per real change to `outbox.jsonl`, with per-event
    `sync_data`."

- `docs/reference/cli.md`:
  - `hmnd` (no-subcommand) section: append one line to the existing prose:
    "After the watcher applies an indexer outcome, the outbox writer
    appends a JSONL line for each real change. Tail
    `~/.local/share/hypomnema/outbox.jsonl` to subscribe; see [the
    change-events spec](../specs/change-events.md) for envelope shape."

- `docs/reference/configuration.md`:
  - § Storage `outbox_file` row: keep the existing default; append one
    prose line to the section: "The outbox file is created at daemon
    startup if missing. Consumers that tail it should reopen on `ENOENT` or
    inode change — see [the change-events spec § Edge
    Cases](../specs/change-events.md#edge-cases)."

- `docs/roadmap/roadmap.md`:
  - Step 4 gets `**Status**: shipped <date>` at the top of its section
    (filled in at the actual ship moment, not at workplan time).

**Why this task is last**: docs follow code, and the spec column flip in
particular benefits from having the implementation validate that
"content_hash on deletes is free and useful" before locking the wording in.

---

## Test strategy

**Unit tests** (`#[cfg(test)]`):
- `src/outbox/event.rs`: serde round-trip, lowercased discriminants, `None`
  hash field omission (Task 4.1).
- `src/outbox/writer.rs`: open + append + read-back, preserves prior
  contents on reopen, fails gracefully on missing parent dir, concurrent
  appends serialize via the mutex (Task 4.2).
- `src/indexer/mod.rs`: extended for the new outcome shapes — three new
  tests on top of the existing single-file ones (Task 4.3).

**Integration tests** (`tests/`):
- `tests/outbox.rs` (new) — nine cases enumerated in Task 4.5, each in its
  own tempdir.
- `tests/watch.rs` — small fan-out to add `Outbox::open(...)` + new
  signature to each existing test; no new test cases.
- `tests/skeleton.rs` — no new cases. `hmnd scan` doesn't touch the outbox;
  the no-subcommand outbox path is exercised by `tests/outbox.rs`.
- `tests/config.rs` — no new cases. `outbox_file` default and parse tests
  already in place from step 1.

**Lint and format**: `cargo clippy --all-targets -- -D warnings` and `cargo
fmt --all -- --check` before review.

**Cross-platform**: macOS + Linux covered. Rename test (case 7) is
`#[cfg(unix)]`-gated. The fsync path uses `File::sync_data`, which is
stable cross-platform in std (`fsync`/`fdatasync` on Unix,
`FlushFileBuffers` on Windows when we eventually add it).

---

## Definition of done

- [ ] Editing a `.md` file in the vault appends exactly one outbox line
      with `event_type: "modified"` and the file's new content_hash
      (criterion 1).
- [ ] mtime-only touch appends nothing (criterion 2).
- [ ] Deleting a file appends `event_type: "deleted"` with the last known
      content_hash (criterion 3).
- [ ] `tail -f outbox.jsonl` works end-to-end — verified by
      `tests/outbox.rs` case 5 (re-read across writes from a captured
      offset) (criterion 4).
- [ ] The outbox file is in `data_dir`, not under the vault — verified by
      `tests/outbox.rs` case 6 plus the existing config validator
      (criterion 5).
- [ ] Renaming a file produces two outbox lines (`deleted` + `created`),
      each with the appropriate hash (deferred decision 2 confirmation).
- [ ] All file I/O on the outbox goes through `tokio::task::spawn_blocking`
      — verified by reading the diff (no `std::fs::write` /
      `File::write_all` outside a `spawn_blocking` closure).
- [ ] Per-event `sync_data` is called inside the same `spawn_blocking`
      closure as the write — verified by reading the diff (deferred
      decision 1 confirmation).
- [ ] `HashUnchanged` reindex outcomes and `NotPresent` remove outcomes
      emit no outbox events — verified by `tests/outbox.rs` case 2 and the
      lack of an explicit `NotPresent` test (the test for case 8 covers
      this implicitly, since sync-conflict files yield `NotPresent` if the
      filter ever leaks).
- [ ] `cargo test`, `cargo clippy --all-targets -- -D warnings`,
      `cargo fmt --all -- --check` all pass.
- [ ] `docs/specs/change-events.md`, `docs/architecture/overview.md`,
      `docs/reference/cli.md`, `docs/reference/configuration.md` reflect
      the resolutions.
- [ ] Roadmap marks Step 4 shipped with the date.
- [ ] Step 4 retrospective appended to
      `notes/project-planning-workflow-notes.md` (using the retro
      template).

---

## Cross-references

**Specs / decisions**:
- [`specs/change-events.md`](../specs/change-events.md) — primary spec for
  this step. Table row for `content_hash` updated in Task 4.6 (deleted
  events now carry prior hash); line 97 (fsync policy) resolved in Task
  4.6; lines 98–100 stay open per the roadmap.
- [ADR-0006: Outbox outside watched dir](../decisions/0006-outbox-outside-watched-directory.md)
  — already enforced by the config validator (step 1) and re-asserted by
  this step's runtime: the outbox writer opens its file inside `data_dir`,
  by construction.
- [ADR-0003: Indexing in the daemon](../decisions/0003-indexing-in-the-daemon.md)
  — the outbox is the change-event projection of "indexing in the daemon."

**Reference docs (updated by this step)**:
- [Change-events spec](../specs/change-events.md) — table + open-question
  resolution.
- [Architecture overview](../architecture/overview.md) — § Outbox Writer
  one-line confirmation.
- [CLI reference](../reference/cli.md) — `hmnd` no-subcommand prose.
- [Configuration reference](../reference/configuration.md) — § Storage
  outbox_file note.

**Pitfalls touched** (from
[`docs/implementation/appendices/tech-stack/pitfalls.md`](../implementation/appendices/tech-stack/pitfalls.md)):
- #1 *Blocking the async runtime with rusqlite* — extended in spirit: the
  same `spawn_blocking` discipline applies to outbox file I/O.
- #5 *Putting state in the watched directory* — already enforced by the
  config validator; this step writes the outbox file outside the vault, by
  construction.

**Skills applied**:
- `.claude/skills/rusqlite-in-async/` — single-file ops are
  `spawn_blocking` with per-event short transactions; the new
  SELECT-before-DELETE pattern in `remove_blocking` stays inside one
  transaction.

**Skills that don't apply yet**:
- `filesystem-watching` — step 3 covered the watcher; step 4 does not
  introduce new `notify` sites.
- `sqlite-vec-extension` — step 6.
- `markdown-chunking` — step 6.

---

## Out of scope (will not appear in this PR)

- **Outbox rotation.** Roadmap explicitly out-of-shipping-gate-scope; spec
  open question 99 stays open.
- **Consumer byte-offset checkpoint API.** Roadmap explicitly
  out-of-shipping-gate-scope; spec open question 100 stays open.
- **Push notifications, webhooks, in-process callbacks.** Spec § Consumer
  Subscription explicitly says "no push, no webhook, no in-process callback
  in v0." Consumers tail the file.
- **Fused rename detection.** v0 emits `deleted` + `created` per the
  step-3 resolution; spec open question 98 stays open.
- **Periodic-fsync mode as a config option.** Per [§ Deferred decision
  1](#1-fsync-policy-per-event-vs-periodic): if profiling later shows
  per-event sync as a hot spot, the upgrade path is a hybrid. v0 does not
  expose a config knob.
- **Outbox truncation/recovery on external truncation.** Spec edge-case
  section already says the daemon recreates the file on next event;
  consumer-side reopen is the expected recovery. v0 does not add daemon-side
  detection of external truncation or re-create-on-the-fly.
- **The HTTP server, `/health`, `/search/*`.** Step 5.
- **Outbox file size in `hmn status`.** That's a step-5 concern (`hmn
  status` lands then). The current cli.md row already names "outbox size"
  in the status response shape; no surface to add now.

If review surfaces a strong reason to pull any of the above forward, that's
a roadmap revision per the
[mid-step roadmap revision](../../notes/project-planning-workflow-notes.md#open-questions-about-the-workflow-itself)
open question.

---

## Net new dependencies

None. All required types and functions come from crates already in
`Cargo.toml`:

| Crate | Already in tree from | Used here for |
|-------|----------------------|---------------|
| `serde`, `serde_json` | step 1 (config), step 1 (handoff) | `ChangeEvent` envelope serialization |
| `chrono` (no default features) | step 2 | `detected_at` timestamp formatting |
| `tokio` (full features) | step 1 | `spawn_blocking` for outbox I/O |
| `anyhow` | step 1 | error propagation |
| `tempfile` (dev) | step 2 | `tests/outbox.rs` fixtures |

Step 4 is the first roadmap step that adds zero new dependencies. Confirms
the roadmap's "low risk — thin layer on top of step 3" call.

---

## Process dependencies

Three playbook-level cleanups were called out in step 3's retro and are
being worked on by a separate playbook-edits agent in parallel with this
workplan:

1. **Soft-flag forwarding contract** — promote the `Forward note for Task
   M+1` paragraph pattern from observed convention to documented playbook
   contract.
2. **Idle-detection retirement** — the playbook's open question on
   `timer_fire_when_idle_any` reliability has 14/14 clean fires across
   steps 1–3 and is ready to be closed.
3. **Soft-flag-to-coordinator vs. soft-flag-to-task-agent** — distinguish
   the two cases in the playbook's TASK AGENT § Reporting section.

**Step 4 does not block on any of these.** If the playbook has shipped its
edits by the time the coordinator role for step 4 starts, the coordinator
follows the new contract verbatim. If not, the coordinator follows the
in-tree playbook as it stands and notes any friction in the step 4 retro.
The orchestration shape this workplan assumes — coordinator drives task
agents through the rolling-context scratchpad, soft flags land in `Forward
note for Task M+1` paragraphs, idle-detection fires on per-task completion
— is the *current* shape and will not break under either playbook
revision.

The workplan does not lock the coordinator into specific phrasings or
section names from the playbook; it describes work in terms of code,
tests, and docs. Coordinator-level orchestration choices are deferred to
the playbook's authority at the moment of execution.
