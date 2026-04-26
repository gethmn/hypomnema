# Hypomnema Roadmap — Initial Implementation Kick-Off

**Scope**: First five of the eight steps enumerated in [`docs/implementation/tech-stack.md`](../implementation/tech-stack.md). Step 5 is the **shipping gate** — a usable daemon with HTTP-based filesystem and content search. Steps 6–8 (chunking + embedding, semantic search, MCP) are deliberately out of scope for this round and will get their own roadmap when this one ships.

**Status**: Not started. Repo is near-greenfield (binary stubs, empty `lib.rs`, foundational deps in `Cargo.toml`).

**Process**: Each step gets a short workplan (`step-NN-workplan.md` in this directory) created **just before** that step is implemented. TBDs flagged in the docs are resolved at or before the step that needs them. See [`notes/project-planning-workflow-notes.md`](../../notes/project-planning-workflow-notes.md) for the working description of the planning process itself.

---

## Step 1 — Skeleton

**Status**: shipped 2026-04-25

**Goal**: `hmnd` starts from a TOML config, initializes tracing, handles SIGINT/SIGTERM, exits cleanly. `hmn` parses CLI args via clap (`--help`, `--version` work).

**Shipping criteria**:
- `cargo run --bin hmnd -- --config <path>` logs config summary and watched vault path, then idles
- Ctrl+C exits 0 with a clean shutdown log line
- `hmn --help` renders without error
- `cargo test` passes (at least one smoke test exists)

**Deferred decisions to resolve here**:
- CLI subcommand naming ([`vision.md` line 115](../product/vision.md))
- TOML config schema (top-level keys, defaults, validation)
- Default logging verbosity per module ([`vision.md` line 113](../product/vision.md))

**New deps**: none beyond what's already in `Cargo.toml`.

**Risk**: low. Pure setup.

---

## Step 2 — Scan + hash

**Status**: shipped 2026-04-25

**Goal**: On startup, `hmnd` walks the configured vault, SHA256-hashes each `.md` file, persists `{path, size, mtime, content_hash}` rows to `index.sqlite` at `~/.local/share/hypomnema/`.

**Shipping criteria**:
- Against a test vault, `index.sqlite` has exactly one row per `.md` file with correct hashes
- Re-running is idempotent — no duplicate rows
- Modifying a file's bytes updates its `content_hash`; modifying only `mtime` does not
- All SQL goes through `tokio::task::spawn_blocking` (per `.claude/skills/rusqlite-in-async`)

**Deferred decisions**:
- Auto-rescan-on-startup default ([`vision.md` line 116](../product/vision.md))
- Default ignore-pattern set, including VCS awareness ([`vision.md` line 117](../product/vision.md))
- Symlink handling ([`specs/filesystem-search.md` line 91](../specs/filesystem-search.md))
- SQLite schema migration strategy

**New deps**: `rusqlite`, `r2d2`, `r2d2_sqlite`, `walkdir`, `sha2`, `chrono`, `globset` (pulled forward from step 5).

**Risk**: medium. First exercise of the `spawn_blocking` pattern. Schema design lands here and constrains everything downstream.

---

## Step 3 — Watcher

**Status**: shipped 2026-04-26

**Goal**: `notify` + `notify-debouncer-full` watch the vault. On debounced events, re-hash changed files; if the new hash differs from the stored hash, update the row. Sync-conflict filenames filtered at the watcher boundary (per `.claude/skills/filesystem-watching`).

**Shipping criteria**:
- Editing a `.md` file in the vault updates its row's `content_hash`
- Dropping a `*.sync-conflict-*` file produces no DB write
- Deleting a watched file removes its row
- Saving a file without changing bytes produces zero DB writes (mtime-only changes are ignored)
- The daemon survives a sustained editor save loop without runaway CPU

**Deferred decisions**:
- Debounce window tuning (start with skill's recommended default)
- Rename-as-distinct-event vs. delete+create pair ([`specs/change-events.md` line 98](../specs/change-events.md))

**New deps**: `notify`, `notify-debouncer-full`.

**Risk**: medium-high. Editor save patterns and sync-tool event storms are the biggest landmines in this entire project.

---

## Step 4 — Outbox

**Status**: shipped 2026-04-26

**Goal**: On each real change (post hash-gate), append a JSONL line to `~/.local/share/hypomnema/outbox.jsonl`: `{event_type, path, content_hash, detected_at}`. Event types: `created`, `modified`, `deleted`.

**Shipping criteria**:
- Editing a watched file appends one JSONL line with `event_type: "modified"`
- mtime-only touch appends nothing
- Deleting a file appends `event_type: "deleted"` with the last known content_hash
- `tail -f outbox.jsonl` works as a consumer interface end-to-end
- Outbox file is never written under the watched vault directory

**Deferred decisions**:
- fsync policy: per-event vs. periodic ([`specs/change-events.md` line 97](../specs/change-events.md))
- Rename handling ([`specs/change-events.md` line 98](../specs/change-events.md))

**Explicitly out of shipping-gate scope**:
- Outbox rotation ([line 99](../specs/change-events.md))
- Consumer byte-offset checkpoint API ([line 100](../specs/change-events.md))

**New deps**: none (`serde_json` already present).

**Risk**: low. Thin layer on top of step 3.

---

## Step 5 — HTTP filesystem + content search (shipping gate)

**Goal**: Axum server on `127.0.0.1:7777` exposes `/search/filesystem`, `/search/content`, and `/health`. `hmn search filesystem <glob>` and `hmn search content <query>` hit the daemon and print results. `hmn status` reports daemon reachability and basic index stats.

**Shipping criteria**:
- With `hmnd` running against a real vault: `hmn search filesystem 'notes/*.md'` returns matching files
- `hmn search content 'pgvector'` returns files (with line snippets, per spec) that contain the term
- `curl http://127.0.0.1:7777/health` returns 200
- `hmn status` shows: vault path, indexed file count, last indexed time, outbox file size
- Result shapes match what the specs describe; pagination is intentionally absent (truncate + flag, per spec)

**Deferred decisions**:
- Precise JSON response shapes for `/search/filesystem` and `/search/content`
- Regex vs. glob behavior boundaries
- Phrase search across line boundaries ([`specs/content-search.md` line 86](../specs/content-search.md))
- Regex alternative to glob ([`specs/filesystem-search.md` line 92](../specs/filesystem-search.md))

**Explicitly out of shipping-gate scope**:
- Pagination (specs prescribe truncate + flag)
- Frontmatter summaries in filesystem results ([`specs/filesystem-search.md` line 93](../specs/filesystem-search.md))
- Health metrics beyond basic reachability

**New deps**: `axum`, `tower`, `tower-http`, `reqwest` (for `hmn`), `regex`. (`globset` already landed in step 2.)

**Risk**: medium. First external surface. JSON shapes become a contract once agents wire up to them.

---

## After step 5

When step 5 ships:
1. Tag the milestone in git
2. Capture any ADRs that hardened during the build
3. Write a short retrospective into [`notes/project-planning-workflow-notes.md`](../../notes/project-planning-workflow-notes.md) on what worked and what didn't about the roadmap→workplan process
4. Open a fresh roadmap doc for steps 6–8 (chunking + embedding, semantic search, MCP)
