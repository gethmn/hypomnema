# Step 2 Workplan — Scan + hash

**Roadmap step**: [Step 2 — Scan + hash](./roadmap.md#step-2--scan--hash)
**Status**: drafted, awaiting review
**Created**: 2026-04-25

---

## Goal recap

`hmnd` opens (or creates) `index.sqlite` in the data directory, walks the
configured vault, computes SHA-256 over the bytes of every `.md` file that
survives `ignore_patterns`, and persists `{path, size, mtime, content_hash}`
rows. Re-runs are idempotent — same vault state in, same rows out, no
duplicates, no spurious content-hash updates.

Two callers of the same scan code:

- `hmnd` (default action) runs the scan once at startup, then idles awaiting
  shutdown (no watcher yet — that's step 3).
- `hmnd scan` runs the scan once, prints a one-line summary, and exits 0.

Step 2 also locks the schema-migration mechanism we'll evolve through steps
3–7. No watcher, no outbox, no HTTP, no chunking, no embedding.

## Deferred-decision resolutions

The roadmap flagged four TBDs to be resolved here.

### 1. Auto-rescan-on-startup default

**Resolution**: the daemon **always** reconciles on startup. There is no fast
path that trusts the existing index.

The reconciliation is cheap on a clean restart because the scan stat-gates
hashing: for each indexed file, `stat` first; only re-hash when `(size, mtime)`
differs from the stored row. New files are hashed unconditionally. Files
present in the index but missing from disk are deleted from the index. The
result is auto-incremental: the cost scales with the change set, not the vault.

The `--rescan` flag from
[`reference/cli.md`](../reference/cli.md) is **deferred to a later step**
(it would force re-hashing every file regardless of stat — useful as a bitrot
sweep, not as a startup default). Step 2 does not implement it; the doc gets a
small edit to clarify "deferred" rather than "TBD."

**Why**: matching the architecture-overview's quality attribute ("Restart must
re-reconcile the index without corruption") is a v0 invariant. A "trust the
index" mode is a future optimization that this step can explicitly leave
unbuilt.

### 2. Default ignore-pattern set, including VCS awareness

**Resolution**: v0 honors `ignore_patterns` only — no `.gitignore` parsing, no
Mutagen-style `ignore_vcs_files` flag. Add `.git/**` to the default
`ignore_patterns` list so the most common case is covered without the user
asking.

Final default list:

```toml
ignore_patterns = [
  ".git/**",
  ".obsidian/**",
  ".trash/**",
  "*.sync-conflict-*",
  "**/*.tmp",
]
```

**Why**: v0 deliberately doesn't load `.gitignore` (per
[`vision.md`](../product/vision.md) open question and the "Future direction"
note already in [`reference/configuration.md`](../reference/configuration.md)).
Adding `.git/**` to defaults is one line; a user with `.git` inside the vault
gets the right behavior without per-vault configuration. Other VCS (`.svn`,
`.hg`) are skipped from defaults — rare in note vaults, easily added by the
user.

### 3. Symlink handling

**Resolution**: follow symlinks during the walk; reject any entry whose
canonicalized real path falls outside the canonicalized vault root. Loop
detection is delegated to `walkdir`'s built-in mechanism (it tracks
`(device, inode)` per directory entered when `follow_links(true)` is set, and
returns a loop error on cycles — log and skip).

**Why**: matches the v0 spec line in
[`specs/filesystem-search.md`](../specs/filesystem-search.md) ("symlinks within
the vault are followed; symlinks pointing outside the vault are not"). The
defensive "outside-the-vault" check uses `fs::canonicalize` per file entry —
modest cost, well worth the safety. Files that fail canonicalization (broken
symlinks) are logged at `warn` and skipped; they don't fail the scan.

### 4. SQLite schema migration strategy

**Resolution**: `PRAGMA user_version` + a Rust-side ordered migration list
(`MIGRATIONS: &[&str]`). On store open: read `user_version`; for each
migration with index ≥ that version, run it inside a transaction; bump
`user_version` to `MIGRATIONS.len()` on success. No new dependency.

Step 2 ships migration **0001** (initial `files` table). Future steps add
migrations by appending strings; never edit a migration after it ships.

**Why**: avoids the dep weight of `refinery` / `rusqlite_migration` for what is
in v0 a five-or-six-migration project. The pattern is well-trodden and trivial
to test (open a fresh DB → user_version goes from 0 → N; reopen the same DB →
no migrations run, user_version stays N). If we hit a migration that needs
data transformation rather than DDL, the entry can be a `fn(&Transaction) ->
Result<()>` instead of a `&str` — cheap to upgrade later.

Schema for migration 0001:

```sql
CREATE TABLE files (
    path           TEXT PRIMARY KEY,    -- vault-relative, forward-slash
    size           INTEGER NOT NULL,
    mtime          TEXT    NOT NULL,    -- ISO-8601 UTC, µs precision
    content_hash   TEXT    NOT NULL,    -- "sha256:" + 64 hex chars
    indexed_at     TEXT    NOT NULL     -- when this row was last (re)written
) STRICT;
```

`STRICT` is opt-in column-type enforcement; cheap to use, catches typos.
No secondary index in step 2 — `path` is the primary key; lookups by prefix
(filesystem search) land in step 5 with the index it needs.

`mtime` is stored as ISO-8601 text so it sorts correctly under TEXT comparison
and is grep-friendly when debugging the DB. The
[`specs/filesystem-search.md`](../specs/filesystem-search.md) response shape
also describes ISO-8601, so storage and wire match.

`content_hash` carries the `sha256:` prefix per
[`specs/change-events.md`](../specs/change-events.md) so the value can be
forwarded to the outbox unchanged in step 4.

---

## Tasks (ordered, each independently mergeable)

Each task lands as its own commit. Task 2.1 has no dependencies on the others
and is a pure cleanup (step-1 retro follow-up); the rest are sequential.

### Task 2.1 — Lift binary-target tracing workaround into `compose_filter`

**Files**:
- `src/logging.rs`
- `src/bin/hmnd.rs`
- `src/bin/hmn.rs`

**What lands**:
- `compose_filter` extends each `BinaryKind`'s default directive to include the
  binary crate's own target name:
  - `Hmnd` → `hypomnema={lvl},hmnd={lvl},notify={n},tokio={t}`
  - `Hmn`  → `error,hypomnema={lvl},hmn={lvl}`
- Verbose-bumps apply to both the `hypomnema` and the binary-name target.
- Remove the per-call `target: "hypomnema::hmnd"` / `target: "hypomnema::hmn"`
  tags in the binaries; plain `tracing::info!("…")` from `src/bin/hmnd.rs` now
  rides the filter.
- Existing unit tests in `src/logging.rs` get updated for the new directive
  shape; one new test asserts the bumped directive is parseable as `EnvFilter`.

**Why first**: step-1 retro identified this as the gating cleanup before the
indexer's many `tracing::*!` call-sites land. With the structural fix in
place, step 2's scan code can use `tracing::info!` / `debug!` / `warn!` from
modules under `src/store/` and `src/indexer/` without needing per-call
target-tagging. (Both modules' targets fall under `hypomnema::*`, which is
already in the filter — the binary-target lift is specifically for the binary
crates' own log-sites in `hmnd.rs` / `hmn.rs`.)

### Task 2.2 — `globset`-backed ignore matcher; default list adds `.git/**`

**Files**:
- `Cargo.toml` — add `globset = "0.4"`
- `src/config.rs` — `default_ignore_patterns()` adds `.git/**`; new method
  `WatcherConfig::compiled_ignores() -> Result<GlobSet>` that compiles the
  pattern list into a `GlobSet` once at config load.
- `tests/config.rs` — assert the compiled `GlobSet` matches representative
  paths and rejects non-matching ones.

**What lands**:
- `globset` dependency, MIT/Apache, ~5 KLOC, transitively pulls `regex-syntax`
  (already in tree via `tracing-subscriber`).
- `WatcherConfig::compiled_ignores()` — returns a compiled `GlobSet`. Errors on
  invalid pattern, with the pattern in the error message.
- Default list: `[".git/**", ".obsidian/**", ".trash/**", "*.sync-conflict-*",
  "**/*.tmp"]`.
- Test cases: `.git/objects/abc` matches, `.obsidian/workspace.json` matches,
  `notes/foo.md` does not match, `notes/foo.md.tmp` matches (via `**/*.tmp`),
  `My Note .sync-conflict-202604.md` matches.

**Why a separate task**: ignore-matching is consumed by the walker in 2.4 and
will be consumed again by the watcher in step 3. Landing the matcher first as a
config-side concern keeps walker code clean.

**Roadmap update note**: roadmap-step-2 listed deps as
`rusqlite, r2d2, r2d2_sqlite, walkdir, sha2`. `globset` is being pulled forward
from step 5 because step 2 honors `ignore_patterns`. Net step-2 deps are six
(those five + `globset`); roadmap step 5's "globset, regex" line should be
amended at the step-2 boundary to drop `globset`. Flagged in § Out of scope.

### Task 2.3 — `store` module: pool, schema migrations, smoke open

**Files**:
- `Cargo.toml` — add `rusqlite = { version = "0.31", features = ["bundled"] }`,
  `r2d2 = "0.8"`, `r2d2_sqlite = "0.24"`. (`load_extension` feature is *not*
  added in step 2 — sqlite-vec lands in step 6.)
- `src/lib.rs` — `pub mod store;`
- `src/store/mod.rs` — public surface: `Store::open(data_dir, index_file) ->
  Result<Store>`, holds an `r2d2::Pool<SqliteConnectionManager>`, exposes
  `pool() -> Pool` and `path() -> &Path`.
- `src/store/pool.rs` — pool builder with WAL + `synchronous=NORMAL` PRAGMAs
  via `with_init`; pool size = 8.
- `src/store/schema.rs` — `MIGRATIONS: &[&str]` (one entry — the `files`
  table); `apply_migrations(conn: &mut Connection) -> Result<()>` reads
  `user_version`, runs pending migrations in transactions, bumps the pragma.
- `src/store/mod.rs` unit tests:
  - fresh DB → `user_version` goes 0 → 1.
  - re-open same DB → no migrations run (idempotent).
  - the `files` table exists after open.
  - WAL mode is set.

**What lands**:
- Working store-open path that all subsequent indexer code consumes.
- Migration framework that future steps grow by appending one string.
- All SQL goes through `tokio::task::spawn_blocking` — even the open path
  (called from async `main` in step 2.5).

**Why a separate task**: this is the schema-design moment the roadmap flagged
as medium-risk. Splitting it from the indexer keeps the schema change visible
in its own commit and gives the indexer task a stable foundation.

**Skill applied**: `.claude/skills/rusqlite-in-async/`. The store module owns
the spawn_blocking discipline; callers receive results that have already
crossed the async boundary.

### Task 2.4 — `indexer` module: scan + hash

**Files**:
- `Cargo.toml` — add `walkdir = "2.5"`, `sha2 = "0.10"`,
  `chrono = { version = "0.4", default-features = false, features = ["std",
  "clock"] }` for ISO-8601 mtime formatting (no serde feature — we render to
  string, not deserialize).
- `src/lib.rs` — `pub mod indexer;`
- `src/indexer/mod.rs` — public surface: `Scanner::new(config: &Config, store:
  &Store) -> Result<Scanner>`, `async fn run(&self) -> Result<ScanReport>`.
- `src/indexer/walk.rs` — wraps `walkdir::WalkDir::new(vault).follow_links(true)`,
  filters to `.md` files and out via the compiled `GlobSet` from 2.2. Yields a
  Vec of `(vault_relative_path, abs_path, size, mtime)` tuples. Logs and skips
  entries whose canonicalized real path is not under the canonical vault.
- `src/indexer/hash.rs` — `pub fn hash_file(path: &Path) -> Result<String>`
  reads the file in a 64 KiB buffer, feeds `sha2::Sha256`, returns
  `"sha256:" + hex`. Path-not-found is a hard error; the caller decides
  what to do.
- `src/indexer/mod.rs` orchestrates:
  1. Walk the vault (in `spawn_blocking`).
  2. Load all rows from `files` table into a `HashMap<String, FileRow>`
     (path → stored size/mtime/content_hash).
  3. For each found file:
     - if not in map → hash it, INSERT row.
     - if in map and `(size, mtime)` matches → no work (skip-stat-gate).
     - if in map but `(size, mtime)` differs → hash; if hash matches, UPDATE
       only `(mtime, indexed_at)`; if hash differs, UPDATE all four.
  4. For each path in the map but not in the walk → DELETE row.
- `ScanReport`: `{ inserted: usize, updated: usize, hash_unchanged: usize,
  deleted: usize, skipped_outside_vault: usize, walk_errors: usize,
  duration: Duration }`. Returned to caller for the one-line summary log.

**Decisions encoded here (not new ADRs — too small)**:
- mtime-only churn updates the row's `(mtime, indexed_at)` but not
  `content_hash`. Avoids re-hashing on every scan when a sync tool stamped a
  new mtime onto unchanged bytes. Step 3's watcher will use the same gate to
  decide whether to emit an outbox event.
- The DB transaction wraps **the whole scan's writes**, not per-file. Atomic
  reconciliation: either the scan completes and the DB reflects the new
  vault state, or it fails and the previous state persists. Acceptable
  because step-2 vaults are small (handfuls to thousands of files); large-vault
  batching is a step-6+ concern (it'll matter once chunks land).
- Vault-relative paths are stored with `/` separators (post-`strip_prefix`,
  any path components are joined with `/`). Unix native, Windows TBD when
  Windows lands.
- Files with non-UTF-8 paths are logged and skipped. The DB column is `TEXT`;
  storing OsString would cost more than it saves for a v0 use case.

**Why this task is medium-high risk**:
- First time the project exercises `tokio::task::spawn_blocking` in a real
  workload (the skill's pattern lands for real here).
- The schema design from 2.3 gets its first non-trivial caller; if any column
  shape is wrong, this is the task that finds out.
- Symlink + canonicalization edge cases interact with the test fixtures.

**Skill applied**: `.claude/skills/rusqlite-in-async/`.

### Task 2.5 — Wire scan into `hmnd` (default action + `scan` subcommand)

**Files**:
- `src/bin/hmnd.rs`

**What lands**:
- Default action (`run_daemon`):
  - opens the store
  - constructs the scanner
  - awaits `scanner.run()`
  - logs the `ScanReport` at `info` (`"hmnd: scan complete: inserted=X
    updated=Y hash_unchanged=Z deleted=W in N.NNs"`)
  - awaits the shutdown receiver (unchanged from step 1)
  - on shutdown: logs the same drain-complete line as step 1
- `Command::Scan` no longer bails — it does everything `run_daemon` does
  except the awaits-shutdown bit. Returns 0 on a successful scan, 1 on error.
- `Command::ConfigValidate` unchanged.
- The scan path in both modes shares one helper (`do_scan(&Config) ->
  Result<ScanReport>`) so the daemon and the subcommand don't drift.

**No exit-code changes**: configuration error → 3, runtime error → 1, clap
error → 2. Same as step 1.

### Task 2.6 — Integration tests against a real tempdir vault

**Files**:
- `Cargo.toml` — add `tempfile = "3.10"` to `[dev-dependencies]`.
  ([Step-1 workplan flagged this](./step-01-workplan.md#task-16--smoke-tests--justfile-sanity)
  as a deferred dep — landing it now in the dev-only slot.)
- `tests/scan.rs` (new) — integration tests run against the built `hmnd scan`
  binary OR call `Scanner` directly via `hypomnema::*`. Calling directly is
  simpler and lets us inspect the SQLite file with rusqlite from the test;
  go that route.
- `tests/skeleton.rs` — extend with two cases:
  - `hmnd scan --config <tmp>` exits 0 against a vault with one `.md` file.
  - `hmnd scan --config <tmp>` exits 0 against an empty vault.
- `tests/config.rs` — extend `data_dir`-under-`vault` rejection with one
  case for the `.git/**` default pattern matching.

**What `tests/scan.rs` covers** (mirrors the roadmap's shipping criteria):
1. Single `.md` file → exactly one row, hash matches `sha256(bytes)`.
2. Add a second `.md` file, run scan again → two rows, no duplicate.
3. Edit a file's bytes, run scan → row's `content_hash` updated.
4. Touch a file's mtime only (`filetime` crate would help — see footnote)
   → `content_hash` unchanged in the row; `mtime` updated.
5. Delete a file from disk, run scan → row removed.
6. Hidden directory `.obsidian/foo.md` → not indexed.
7. `.git/HEAD` exists in the vault → not indexed (covered by default
   `.git/**`).
8. `*.sync-conflict-*.md` file → not indexed.
9. `notes/dir/file.md.tmp` → not indexed.
10. Symlink inside the vault to another file inside the vault → indexed once
    (under its symlink-name path).
11. Symlink inside the vault pointing **outside** the vault → not indexed;
    `skipped_outside_vault` counter ≥ 1 in the `ScanReport`.

**Footnote on mtime tests**: `std::fs::set_modified` is stable since 1.75;
no `filetime` dep needed. Tests using it create a file, scan, then call
`File::open(path).and_then(|f| f.set_modified(SystemTime::now() +
Duration::from_secs(1)))` to bump mtime without changing bytes.

**`tempfile` is dev-only**: doesn't ship in the binaries; no v0 production
weight.

### Task 2.7 — Reference docs reflect step-2 resolutions

**Files**:
- `docs/reference/configuration.md`
  - default `ignore_patterns` block adds `.git/**`.
  - Validation rules section adds: "the daemon scans + reconciles on every
    startup; this is the only mode in v0."
- `docs/reference/cli.md`
  - `--rescan` row: change "TBD …" to "deferred (forces re-hashing every file
    regardless of stat; not implemented in v0)."
  - `hmnd scan` row gains a one-line note: "implemented in step 2; reconciles
    the index against the vault, prints a one-line summary, exits 0."
- `docs/specs/filesystem-search.md`
  - § Edge Cases / Symlinks: tighten "v0: symlinks within the vault are
    followed" with one line confirming canonicalization-based outside-vault
    rejection.
- `docs/roadmap/roadmap.md`
  - At step boundary, add `**Status**: shipped <date>` to Step 2.
  - Step 5's `New deps:` line drops `globset` (it landed in step 2).

---

## Test strategy

**Unit tests** (`#[cfg(test)]`):
- `src/store/schema.rs`: pure migration-application against an in-memory
  connection.
- `src/indexer/hash.rs`: known-input hashing (`hash_file` against a fixture
  produces a known sha256).
- `src/indexer/walk.rs`: walking a tempdir with mixed file types + ignore
  patterns produces the expected entry set.
- `src/config.rs`: extended for `compiled_ignores()`.
- `src/logging.rs`: extended for the binary-target lift in 2.1.

**Integration tests** (`tests/`):
- `tests/scan.rs` (new) — eleven cases enumerated in 2.6, each in its own
  tempdir.
- `tests/skeleton.rs` — two new cases for `hmnd scan` exit codes.
- `tests/config.rs` — one new case for `.git/**` ignore matching.

**Cross-platform note**: tests run on macOS (developer machine) and Linux (CI,
when added). Symlink tests use Unix `std::os::unix::fs::symlink`. Windows is
out of v0 scope; tests that touch symlinks gate on `#[cfg(unix)]`.

**Lint and format**: `cargo clippy --all-targets -- -D warnings` and `cargo fmt
--all -- --check` before review.

---

## Definition of done

- [ ] Task 2.1 cleanup lands; no `target: "hypomnema::hmnd"` strings remain in
      either binary file.
- [ ] Against a tempdir vault containing one `.md` file, `hmnd scan` produces
      exactly one row in `index.sqlite` with the correct sha256.
- [ ] Re-running `hmnd scan` is idempotent (row count unchanged, no
      content_hash flips).
- [ ] Editing a file's bytes and re-scanning updates that file's
      `content_hash`; touching only mtime does not.
- [ ] Deleting a file from disk and re-scanning removes its row.
- [ ] `.obsidian/`, `.trash/`, `.git/`, `.tmp`, and `.sync-conflict-*` files
      do not appear in the `files` table.
- [ ] Symlink to file inside the vault is indexed; symlink pointing outside is
      not (`ScanReport.skipped_outside_vault > 0`).
- [ ] `hmnd` (no subcommand) runs the same scan, then idles awaiting shutdown;
      SIGINT exits 0 with the drain-complete log line.
- [ ] All SQL goes through `tokio::task::spawn_blocking` — verified by reading
      the diff (no rusqlite call appears outside a blocking closure).
- [ ] `cargo test`, `cargo clippy --all-targets -- -D warnings`,
      `cargo fmt --all -- --check` all pass.
- [ ] `docs/reference/configuration.md` and `docs/reference/cli.md` reflect
      the resolutions; `docs/specs/filesystem-search.md` symlink line is
      tightened.
- [ ] Roadmap marks Step 2 shipped with the date; Step 5's deps line is
      amended.
- [ ] Step 2 retrospective appended to
      `notes/project-planning-workflow-notes.md` (using the retro template).

---

## Cross-references

**Specs / decisions**:
- [ADR-0003: Indexing in the daemon](../decisions/0003-indexing-in-the-daemon.md)
  — this step is the first concrete instance.
- [ADR-0006: Outbox outside watched dir](../decisions/0006-outbox-outside-watched-directory.md)
  — `data_dir`-under-`vault` rejection (already enforced in step 1; the SQLite
  file lands in `data_dir` per the same invariant).
- [`specs/filesystem-search.md`](../specs/filesystem-search.md) — the response
  schema this step's `files` table feeds (step 5).
- [`specs/change-events.md`](../specs/change-events.md) — `content_hash` format
  ("sha256:" prefix) used in the row.

**Reference docs (updated by this step)**:
- [Configuration reference](../reference/configuration.md)
- [CLI reference](../reference/cli.md)
- [Filesystem search spec](../specs/filesystem-search.md) (small symlink edit)

**Pitfalls touched**:
- #1 *Blocking the async runtime with rusqlite* — first real exercise.
- #3 *Spurious re-indexing from mtime-only change detection* — addressed by
  the stat-gate + hash-then-compare loop.
- #4 *Sync-conflict files* — partial coverage via default `ignore_patterns`;
  watcher-side filtering lands in step 3.
- #5 *Putting state in the watched directory* — already enforced in step 1's
  config validation; this step writes the SQLite file outside the vault, by
  construction.

**Skills applied**:
- `.claude/skills/rusqlite-in-async/` — every SQL call site checked against the
  spawn_blocking pattern.

**Skills that don't apply yet**:
- `filesystem-watching` — step 3.
- `sqlite-vec-extension` — step 6 (load_extension feature is *not* added
  here).
- `markdown-chunking` — step 6.

---

## Out of scope (will not appear in this PR)

- The watcher (`notify` + `notify-debouncer-full`).
- The outbox.
- HTTP / MCP servers.
- The `/health` endpoint.
- Chunking, embedding, the embedding HTTP client, the vec0 virtual table,
  any `load_extension` work.
- A `--rescan` flag (deferred — see Resolution 1).
- File-content storage in SQLite (the `content` column for content search
  lands as a step-5 migration).
- `.gitignore` parsing or any VCS-aware ignore behavior beyond `.git/**` in
  defaults.
- Windows path handling.
- Multi-vault support.

If review surfaces a strong reason to pull any of the above forward, that's a
roadmap revision — see the
[mid-step roadmap revision](../../notes/project-planning-workflow-notes.md#open-questions-about-the-workflow-itself)
open question.

---

## Net new dependencies

| Crate | Where | Why |
|-------|-------|-----|
| `globset` | runtime | ignore-pattern matching (pulled forward from step 5) |
| `rusqlite` (`bundled`) | runtime | the store; `bundled` ships SQLite in-binary |
| `r2d2` | runtime | connection pool |
| `r2d2_sqlite` | runtime | r2d2 manager for rusqlite |
| `walkdir` | runtime | vault traversal with loop-detected symlink follow |
| `sha2` | runtime | content hashing |
| `chrono` (no default features) | runtime | ISO-8601 mtime formatting |
| `tempfile` | dev | tempdir vaults in integration tests |

Eight crates, all in the roadmap's "step 2 deps" list except `globset`
(pulled forward) and `chrono` (formatting helper, called out above) and
`tempfile` (dev-only). No new transitive surprises beyond what
`tracing-subscriber` already brings.
