# Step 5 Workplan — HTTP Filesystem + Content Search (Shipping Gate)

**Roadmap step**: [Step 5 — HTTP filesystem + content search (shipping gate)](./roadmap-1.md#step-5--http-filesystem--content-search-shipping-gate)
**Status**: Shipped 2026-04-25
**Created**: 2026-04-25

---

## Goal recap

`hmnd` exposes an Axum HTTP server on `127.0.0.1:7777` that answers two
search shapes — `/search/filesystem` (glob over indexed paths) and
`/search/content` (substring/regex over indexed file bodies) — plus
liveness (`/health`) and a daemon-detail endpoint (`/status`). `hmn` ships
real implementations of `hmn search filesystem`, `hmn search content`, and
`hmn status` that reach the daemon over HTTP and render results as
human-readable text or `--json`.

The shipping criteria from the roadmap are:

1. With `hmnd` running against a real vault: `hmn search filesystem
   'notes/*.md'` returns matching files.
2. `hmn search content 'pgvector'` returns files (with line snippets, per
   spec) that contain the term.
3. `curl http://127.0.0.1:7777/health` returns 200.
4. `hmn status` shows: vault path, indexed file count, last indexed time,
   outbox file size.
5. Result shapes match what the specs describe; pagination is intentionally
   absent (truncate + flag, per spec).

Step 5 is the **first external surface**: every JSON shape this step ships
becomes a contract once an agent or skill wires up to it. The
deferred-decision resolutions below are written with that contract in mind.

Step 5 is **not** semantic search (step 7), not chunking/embedding (step 6),
not MCP (step 8), and not multi-vault implementation (post-v0). The
forward-compat work for multi-vault is a single-field doc + serde change
across four shapes (the three search responses and the outbox envelope) —
the implementation itself stays single-vault.

## Deferred-decision resolutions

The roadmap flagged five TBDs for this step. All resolve here, inline,
with rationale.

### 1. Multi-vault forward-compat `vault` field

**Resolution**: **adopt the lean** — every v0 wire shape that v0 consumers
will bind to gains an optional `vault` field, **always omitted in v0** (the
single-vault daemon never populates it), present in the spec / serde shape
so adding multi-vault later is additive. Applied uniformly across:

- `/search/filesystem` response — per-result `vault?: string`
- `/search/content` response — per-result `vault?: string`
- Outbox event envelope (`docs/specs/change-events.md`) — per-event
  `vault?: string` (this is the doc-only spec table flip the roadmap
  flagged)
- `/search/semantic` response (forward-doc only — spec gets the field
  added; step 7 inherits the contract)

**Why per-result, not top-level**: per-result is the only shape that stays
additive under both plausible multi-vault futures — (B) one daemon, many
vaults, fan-out queries return mixed results; and (C) hybrid where an
`hmn`/MCP router fans across daemons. Top-level `vault` would force a
breaking change if (B-fan-out) ever lands. Cost is one nullable field per
result entry; the field is `#[serde(skip_serializing_if = "Option::is_none")]`
so v0 wire bytes stay byte-identical to a no-vault world. Future hypothesis
(A) — "one daemon per vault, port-per-vault" — works with the field too;
the daemon just stays silent on it.

**Why apply now, not at step 6/7**: Step 5 is the inflection point — the
roadmap's call. Once consumers bind to vault-less shapes at the shipping
gate, adding the field later is a wire-shape change that breaks them. The
goal is "v0 ships the strongest forward-compat we will ever offer; the
field stays harmless for as long as it's needed." Mirrors the step-4
fsync-policy logic.

**What is *not* on**: `/health` and `/status` do not gain a `vault` field.
`/health` is daemon-scoped (liveness probe). `/status` is a daemon-detail
endpoint and, in a multi-vault future, would naturally restructure
("`vaults: [{path, file_count, ...}]`") rather than additively gain a
single field. Pre-allocating that restructure now is over-design; v0 ships
the single-vault `/status` shape and the spec note flags the future shape.
This keeps the wire-contract scope of "the field" tight: search responses
and outbox events only.

**Outbox envelope spec table flip** (the roadmap explicitly names this as a
doc-only change in this step): `docs/specs/change-events.md`'s data-schema
table gains a row:

| `vault` | string | no | Reserved; always absent in v0. Will carry the source vault identifier when multi-vault ships. Consumers must accept its absence and tolerate any string value. |

The outbox writer (`src/outbox/event.rs`'s `ChangeEvent`) gains the same
`#[serde(skip_serializing_if = "Option::is_none")] pub vault:
Option<String>` field. v0 always passes `None`. No code change beyond the
struct definition; v0 outbox lines are byte-identical pre- and post-flip.

### 2. Precise JSON response shapes

**Resolution**: lock the four shapes below. All examples shown with
forward-compat `vault` field; the field is omitted on the wire in v0.

#### `/search/filesystem` request (POST `application/json`)

```json
{
  "prefix": "notes/databases/",
  "glob": "**/*.md",
  "max_depth": 3,
  "limit": 100
}
```

All fields optional. `prefix` is a path-prefix (not a glob — see
[§ Deferred decision 4](#4-regex-vs-glob-behavior-boundaries) for the
boundary rule). `glob` uses `globset` syntax (already in tree from step 2).
`max_depth` counts path separators in the relative path; `0` matches only
top-level files. `limit` defaults to `100` per spec.

#### `/search/filesystem` response

```json
{
  "results": [
    {
      "path": "notes/databases/pgvector.md",
      "size": 4821,
      "mtime": "2026-04-22T14:31:08.123456Z",
      "content_hash": "sha256:abc123..."
    }
  ],
  "truncated": false
}
```

Field types: `path` string, `size` integer, `mtime` RFC3339 with µs
precision (passes through verbatim from the `files.mtime` column the
indexer wrote), `content_hash` string. Each result entry also carries
`vault?: string` (omitted in v0). Result ordering: ascending `path` (the
spec's "stable output" rule).

#### `/search/content` request

```json
{
  "query": "pgvector",
  "regex": false,
  "case_sensitive": false,
  "prefix": "notes/databases/",
  "include_matches": true,
  "max_matches_per_file": 5,
  "limit": 100
}
```

`query` is required; everything else optional with the spec's defaults.

#### `/search/content` response

```json
{
  "results": [
    {
      "path": "notes/databases/pgvector.md",
      "match_count": 7,
      "matches": [
        { "line": 12, "text": "pgvector supports HNSW and IVF indexes." },
        { "line": 45, "text": "Compared to pgvector, sqlite-vec trades..." }
      ]
    }
  ],
  "truncated": false
}
```

`match_count` is the total number of matches in the file (`>= matches.len()`
when `max_matches_per_file` truncates). Each result entry carries `vault?:
string`. The `matches` array is empty when the request had `include_matches
= false`. Result ordering: ascending `path` (matches the filesystem-search
rule; predictable for diff-style consumers).

The `text` field for each match is the **line that contains the start of
the match**, trimmed to a maximum of 240 bytes (truncated with a trailing
`…` if longer — single Unicode ellipsis, one char). 240 is a soft cap
chosen to keep responses concise without crowding out useful context;
agents can fetch the file via filesystem search and read the surrounding
prose if more is needed. `line` is 1-indexed.

#### `/health` response

```json
{ "status": "ok" }
```

200 OK with that body when the daemon is up. No `vault` field. No metrics.
Per the roadmap, "Health metrics beyond reachability" is out of scope.

#### `/status` response

```json
{
  "vault": "/home/user/Documents/vault",
  "indexed_file_count": 1247,
  "last_indexed_at": "2026-04-22T14:31:08.123456Z",
  "outbox": {
    "path": "/home/user/.local/share/hypomnema/outbox.jsonl",
    "size_bytes": 18432
  }
}
```

`vault` here is the watched path (not the multi-vault forward-compat field
— it cannot be omitted; the daemon always knows its vault). `indexed_file_count`
is `SELECT COUNT(*) FROM files`. `last_indexed_at` is `SELECT MAX(indexed_at)
FROM files` and is `null` when the index is empty. `outbox.size_bytes` is
`fs::metadata(path).map(|m| m.len()).unwrap_or(0)` — the outbox file may
not yet exist on a fresh daemon that hasn't seen its first event.

#### Error envelope

For any 4xx/5xx response from any endpoint:

```json
{ "error": { "code": "invalid_glob", "message": "unbalanced [ in pattern" } }
```

Codes are stable, lowercased, `snake_case`. Initial set: `invalid_glob`,
`invalid_regex`, `invalid_prefix`, `invalid_request` (catch-all for serde
deserialization failures), `internal` (500 with the inner anyhow chain
redacted to a generic message; the full chain lives in the daemon log).
HTTP status conveys broad class; `code` lets the caller distinguish narrow
cases. `message` is human-readable, single-line, no trailing punctuation.

**Why these shapes**: each one mirrors the spec's existing YAML example
exactly except for the additive `vault` field. The error envelope follows
the architecture overview's "JSON error body over HTTP" promise without
inventing a fancier error model. `application/json` request bodies (rather
than query-string parameters) keep the wire contract uniform across modes
and avoid query-string escaping pitfalls for regex/glob patterns. The
filesystem and content endpoints use POST for the same request-body
reason (`limit` and `query` over GET would force URL-escaping).

### 3. Phrase search across line boundaries

**Resolution**: **yes — substring and regex matching both operate over the
file's full byte content, line-agnostic.**

Implementation: in the content-search query path, the indexed body is
treated as a single string. For substring mode (default), `body.find(&q)`
or its case-insensitive equivalent runs over the whole string. For regex
mode, the compiled `regex::Regex` runs in `find_iter` over the whole
string; the regex flavor is Rust `regex` crate default (no DOTALL, but `\s`
matches `\n`, so `pgvector\s+supports` finds matches that span a soft-wrap
line break).

**Match rendering**: for each match, the response's `line` field is the
1-indexed line number of the **match's start** (computed by counting `\n`
bytes before the match offset). The `text` field is the line of the
match's start, trimmed per [§ Deferred decision 2](#2-precise-json-response-shapes).
A match that spans multiple lines is reported once, anchored at its start.

**Why**: Markdown prose soft-wraps. A user looking for "long phrase" in a
note where the phrase happens to wrap across two lines would otherwise see
zero matches and be silently misled. The spec's open question
([content-search.md line 86](../../../docs/specs/content-search.md)) leans yes for
exactly this reason. Implementation cost is low — *not* splitting by line
is the default; the alternative would require us to add splitting logic.
Closes the spec open question.

The reverse rule — "phrase searches must NOT span line boundaries" — would
require us to either reject regex queries containing whitespace (poor UX)
or split-then-search-per-line (inverts the natural shape and breaks regex
features). v0 is line-agnostic.

### 4. Regex vs. glob behavior boundaries

**Resolution**: **clean separation by endpoint.**
- `/search/filesystem`: glob only. The `glob` field uses `globset` syntax.
  No regex alternative in v0 — see [§ Deferred decision 5](#5-regex-alternative-to-glob).
- `/search/content`: substring (default) or regex (when `regex: true`). The
  `query` field is the pattern string in the chosen mode.

**Why a clean separation**: filesystem search answers "what files exist
under this path-shape." Globs are the path-shaped query language users
already know. Regex over paths is a niche need (e.g., `notes/[12]\d{3}-.*`)
that adds documentation surface (semantics for `^`/`$` anchoring,
case-sensitivity, depth interaction with `**`) without a v0 use case.
Content search answers "which files contain this string-shape." Substring
covers the common case; regex covers the next-most-common case (word
boundaries, alternation). No single endpoint should be overloaded with
both flavors.

**Boundary semantics**:

- *Substring matching* (default in `/search/content`) is **case-insensitive
  by default**, ASCII-folded. Toggle via `case_sensitive: true`. Unicode
  case folding is *not* applied — `é` does not match `É` in v0. Reason:
  Rust's standard `str::to_ascii_lowercase` is fast and predictable; full
  Unicode case folding would require `unicode-case-mapping` and surface
  Turkish-`I` style edge cases that are not v0 concerns. Spec already says
  "case-insensitive substring match" without claiming Unicode-aware folding;
  this nails it down.
- *Regex matching* (when `regex: true`) uses the Rust `regex` crate's
  default Unicode flavor. Case-sensitivity is a property of the pattern
  itself (`(?i)foo`); the request's `case_sensitive` flag is **ignored**
  when `regex: true` (the regex crate's case behavior wins, and re-wrapping
  the pattern in `(?i)` would conflict with patterns that explicitly set
  case mode inline). Document this behavior in the spec.
- *Glob matching* (`/search/filesystem`) honors the host filesystem's
  case-sensitivity: case-insensitive on macOS's default APFS, case-sensitive
  on Linux. The glob is matched against the relative `path` column as
  stored — paths inherit the indexer's case from the filesystem. Spec
  already says this; restated for the workplan.
- *`prefix` matching* (both endpoints) is a path-prefix string match (not
  a glob). Trailing `/` is normalized — `notes` and `notes/` both match
  `notes/...` and exclude `notesarchive/...`. Empty `prefix` matches
  everything. Absolute paths and `..` segments are rejected with
  `invalid_prefix`.

**Closes**: `prefix` semantics (was implicit in the spec); case-sensitivity
of substring matching; regex `case_sensitive` interaction.

### 5. Regex alternative to glob

**Resolution**: **no for v0.** `/search/filesystem` ships glob-only; the
spec open question ([filesystem-search.md line 92](../../../docs/specs/filesystem-search.md))
**stays open**. The workplan does not add a `regex: bool` field to the
filesystem-search request shape, since adding it later is additive (the
forward-compat is achieved by *not* shipping it now, not by pre-allocating
a flag).

**Why not now**: globs cover ~all v0 path-shape questions agents ask
("notes/databases/*", "**/2026-*.md", "drafts/**"). The path-regex use
case (e.g., catching all year-prefix paths from any directory) is rare
enough that it doesn't justify duplicating the spec's case-sensitivity /
depth / hidden-files semantics for a second matcher. The cost of deferring
is one open-question line in the spec.

**Why not pre-allocate the field**: a `regex?: string` field that is
"reserved for future use" creates ambiguity for v0 consumers (do I send
it? What happens if I do?). Adding the field later is a one-line spec
change and one-line struct change — additive in the same way the `vault`
field is. The forward-compat case for `vault` is sharper because consumers
already need to *read* it from responses; adding a field consumers *write*
is symmetric but optional. Defer.

### Resolved as part of this step (not pre-flagged in the roadmap)

#### A. Content storage schema (migration 0002)

The current `files` table (migration 0001) has columns `path, size, mtime,
content_hash, indexed_at`. Content search needs the file body in the store
— per
[`docs/specs/content-search.md`](../../../docs/specs/content-search.md): "File text is
stored inside the SQLite store as part of the indexer's work — content
search does not re-read files on every query." This shape was deferred
from step 2's schema work because nothing before step 5 needed it.

**Resolution**: add a new migration `0002` that runs:

```sql
ALTER TABLE files ADD COLUMN content TEXT NOT NULL DEFAULT '';
DELETE FROM files;
```

The `DELETE FROM files;` clears the table after the schema bump so the
next bulk-scan (which `hmnd` runs at startup) repopulates rows with
populated bodies. Without the delete, existing rows would stay at
`content = ''` indefinitely — the stat-gate in `upsert_file_in_tx` skips
re-reads when size + mtime match, so backfill via natural reindexing would
require either touching every file or special-casing the empty-body case
in the indexer.

**Why ALTER + DELETE rather than DROP + CREATE**: ALTER preserves the
schema-versioning niceness (one migration per change, monotonic
`PRAGMA user_version`). DROP + CREATE would lose the ability to
incrementally evolve the schema below the table. Cost of the DELETE — one
extra full scan on the next startup — is cheap and predictable.

**Why a separate `files` column rather than a separate `file_contents`
table or FTS5**: a separate table requires a JOIN on every content query
and saves zero space (one row per file, same primary key). FTS5 is
attractive for a tokenizer-aware future but commits us to tokenizer-shaped
semantics for substring matching that the spec describes as substring +
regex (neither tokenizer-shaped). v0 ships scan-everything substring/regex
matching; an FTS5 pre-filter is a future perf optimization, additive when
scale demands it. ADR-0007 already pins sqlite-vec as the v0
storage-extension story; FTS5 would be a second extension to load.

**UTF-8 handling**: file bodies are decoded with `String::from_utf8_lossy`
before storage (replacement char `U+FFFD` for invalid sequences). Markdown
is UTF-8 by spec; vault hygiene problems become searchable-but-noisy
rather than indexer crashes. The hash continues to be computed over the
**raw bytes** (`hash_file`'s current behavior) — lossy decode for storage,
lossless hash for change detection. Document this in
[`docs/specs/content-search.md`](../../../docs/specs/content-search.md) edge cases.

#### B. Indexer body-storage wiring

The current `upsert_file_in_tx` calls `hash::hash_file(abs)` which streams
bytes through SHA256 and returns the hash, discarding the bytes. For step
5, the indexer needs both the hash and the body.

**Resolution**: introduce a sibling `read_and_hash(abs) -> Result<(String,
String)>` (returns `(body_lossy_utf8, hash)`) that does **one filesystem
read** for both. Refactor `upsert_file_in_tx` to use it on the
`Inserted` and `Updated` (bytes-changed) paths. The stat-gate path
(`HashMatched` / `StatGateHit`) stays untouched — those branches do not
re-read the file, and the `DELETE FROM files;` in migration 0002 ensures
no row keeps an empty body across the upgrade.

`hash::hash_file` is **kept** for callers that only need the hash (its
unit tests, integration tests). The new function is `hash::read_and_hash`,
in the same module. The implementation reads the bytes once, computes the
hash incrementally with `sha2::Sha256`, and decodes lossily for the body.

The `INSERT INTO files` and `UPDATE files SET ...` SQL gain a `content`
column.

#### C. Content search query path

**Resolution**: in the search module, `/search/content` queries:
1. Filter the files table by `prefix` (path-prefix `LIKE` or `path
   GLOB`-shaped, but for prefix it's a `path >= ? AND path < ?` range scan
   — a sortable-prefix shape that lets SQLite use the primary-key index).
2. For each candidate row, run substring or regex match on the `content`
   column (in Rust, after retrieval).
3. Collect up to `max_matches_per_file` per file; total `match_count`
   is computed in full (not just the truncated count).
4. Apply `limit` over the file-level result set; emit `truncated` when
   `LIMIT + 1` rows would have matched.

The "scan candidates in Rust" approach is the v0 perf bet: vault sizes are
small enough that running a Rust regex over (say) 10 MB of total body
content is fine. FTS5 pre-filtering is a future optimization. Document
this trade-off in the workplan, not in the spec — it's an implementation
choice, not a contract.

#### D. `hmn` ↔ `hmnd` URL discovery

The CLI already accepts `--daemon-url`/`HYPOMNEMA_DAEMON_URL`/derives from
`config.http.bind`. Step 5 adds: when none is supplied and
`config.http.bind` is `host:port`, the client computes
`http://<host>:<port>` and uses that. (`bind` is intentionally a
socket-address string; the client's URL build is deterministic.) Localhost
binds (`127.0.0.1`) become `http://127.0.0.1:<port>`.

**Why a deterministic build**: it's the simplest mapping, and it matches
what `cli.md` already documents. No new TLS, no auth — both stay
out-of-scope for v0 per the architecture overview's "binds to localhost
only in v0" call.

**HTTPS / non-localhost**: `--daemon-url https://...` bypasses the
deterministic build. `reqwest` is configured with default TLS features
disabled; if a user points `hmn` at a non-localhost daemon, the build
will fail loudly at HTTPS time. Acceptable for v0; pure-TLS support lands
when v0+.

#### E. Default `max_matches_per_file` and `limit`

Per spec: `max_matches_per_file` defaults to `5`, `limit` defaults to
`100`. Workplan adopts these without revision.

#### F. Server lifecycle inside `hmnd`

The HTTP server runs as a sibling task to the watcher consumer task. The
shutdown signal (`shutdown_rx`) is shared. On SIGINT/SIGTERM:
1. Shutdown signal flips.
2. Both the consumer task and the HTTP server's `axum::serve(...)
   .with_graceful_shutdown(...)` future complete.
3. `run_daemon` joins both; the watcher's drop-ordering (already in place)
   keeps the debouncer alive until the consumer drains.
4. Daemon exits.

The HTTP server is bound to `config.http.bind`. Failure to bind (port in
use, permission denied) is fatal at startup — `run_daemon` returns Err and
the daemon exits 1, identical to outbox-open failure in step 4.

---

## Tasks (ordered, each independently mergeable)

Eight tasks. Each lands as its own commit. Step 4's retro left no
carry-over cleanup for step 5 to gate on; the first task here is real
schema work.

### Task 5.1 — Schema migration 0002: add `content` column + clear rows

**Files**:
- `src/store/schema.rs` — append a second migration string to
  `MIGRATIONS`:
  ```rust
  pub const MIGRATIONS: &[&str] = &[
      // 0001 — initial files table per step-2 workplan § Resolution 4.
      "CREATE TABLE files (
          path           TEXT PRIMARY KEY,
          size           INTEGER NOT NULL,
          mtime          TEXT    NOT NULL,
          content_hash   TEXT    NOT NULL,
          indexed_at     TEXT    NOT NULL
      ) STRICT;",
      // 0002 — content storage for grep-shaped queries per step-5 workplan
      // § Resolution A. The DELETE clears any rows present before the
      // schema bump so the next bulk scan repopulates with bodies.
      "ALTER TABLE files ADD COLUMN content TEXT NOT NULL DEFAULT '';
       DELETE FROM files;",
  ];
  ```

  The migration is run as `execute_batch`, which already handles two
  semicolon-separated statements correctly.

- Five new unit tests in `schema.rs`:
  - `migration_0002_adds_content_column` — fresh in-memory DB, run all
    migrations, assert `content` column exists via `PRAGMA table_info(files)`.
  - `migration_0002_clears_rows_from_pre_existing_db` — open in-memory DB,
    apply only migration 0001 (manually), insert one row, then call
    `apply_migrations` which advances to 0002, assert the row count is 0
    afterwards.
  - `content_column_is_not_null_with_empty_default` — fresh DB, after
    migrations, `INSERT INTO files (path, size, mtime, content_hash,
    indexed_at) VALUES (...)` without `content` succeeds and the row's
    `content` is `''`.
  - `content_column_accepts_arbitrary_utf8` — insert a row with
    `content = 'héllo café'`, read it back, assert byte-equal.
  - `migrations_advance_user_version_to_2` — fresh DB, after migrations,
    `PRAGMA user_version` is `2`.

**What lands**:
- One schema migration. No new module. No new dependencies.
- The `content` column is `TEXT NOT NULL DEFAULT ''`. STRICT mode
  enforcement is preserved — STRICT applies to the original CREATE TABLE
  statement, and ADD COLUMN inherits it.

**Why first**: every downstream task (5.2, 5.3, 5.4, 5.5, 5.6, 5.7) reads
or writes the `content` column. Landing the migration first means every
subsequent task starts with a tree that has the schema in place. The
DELETE statement only fires once per local DB upgrade; subsequent runs
see `user_version = 2` already and skip the migration.

**Skill applied**: `.claude/skills/rusqlite-in-async/` — the schema
migration runs inside `Store::open_blocking` (already a `spawn_blocking`
context).

### Task 5.2 — Indexer stores body content

**Files**:
- `src/indexer/hash.rs` — add a `read_and_hash` function alongside
  `hash_file`:
  ```rust
  pub fn read_and_hash(path: &Path) -> Result<(String, String)> {
      let bytes = fs::read(path)
          .with_context(|| format!("reading {}", path.display()))?;
      let hash = sha256_hex(&bytes);
      let body = String::from_utf8_lossy(&bytes).into_owned();
      Ok((body, format!("sha256:{hash}")))
  }
  ```
  `hash_file` keeps its current shape (streaming `BufReader` + `sha2`)
  for callers that only need the hash. `read_and_hash` reads the file
  fully into memory once — Markdown files are small enough that the
  streaming-vs-full-read cost difference is negligible at v0 vault
  sizes.

  Three or four new unit tests in `hash.rs`:
  - `read_and_hash_roundtrip` — write bytes, call `read_and_hash`, assert
    the body matches the bytes (modulo lossy UTF-8 — use only valid UTF-8
    in this test) and the hash matches `hash_file`'s output.
  - `read_and_hash_lossy_utf8_replaces_invalid_bytes` — write a file
    containing `b"\xFFvalid"`, call `read_and_hash`, assert the body
    starts with the U+FFFD replacement char and the hash matches the raw
    `hash_file` output.
  - `read_and_hash_handles_empty_file` — empty file, both fields are
    sensible (empty body, hash of empty bytes).

- `src/indexer/mod.rs` — `upsert_file_in_tx` is restructured to read the
  body alongside the hash on the `Inserted` and `Updated` (bytes-changed)
  branches:
  ```rust
  None => {
      let (body, hash) = hash::read_and_hash(abs)
          .with_context(|| format!("reading new file {rel}"))?;
      tx.execute(
          "INSERT INTO files (path, size, mtime, content_hash, content, indexed_at) \
           VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
          params![rel, size, mtime, hash, body, now_iso],
      )?;
      Ok(UpsertEffect::Inserted { hash })
  }
  ```
  And on the bytes-changed branch:
  ```rust
  let (body, hash) = hash::read_and_hash(abs)
      .with_context(|| format!("reading changed file {rel}"))?;
  if hash == prev.content_hash {
      // mtime-only update: still write content because lossy-decode rules
      // mean an mtime-touched file with broken bytes might have changed
      // its lossy decoding; safest to keep content in lockstep with the
      // last-read bytes. (Edge case; cost is one extra UPDATE per
      // mtime-only churn — bounded by the debouncer + hash gate.)
      tx.execute(
          "UPDATE files SET mtime = ?1, content = ?2, indexed_at = ?3 \
           WHERE path = ?4",
          params![mtime, body, now_iso, rel],
      )?;
      Ok(UpsertEffect::HashMatched)
  } else {
      tx.execute(
          "UPDATE files SET size = ?1, mtime = ?2, content_hash = ?3, content = ?4, indexed_at = ?5 \
           WHERE path = ?6",
          params![size, mtime, hash, body, now_iso, rel],
      )?;
      Ok(UpsertEffect::Updated { hash })
  }
  ```

  The stat-gate path (`StatGateHit`) is **unchanged** — the spec gate
  (size + mtime match → trust the row) still holds, and migration 0002's
  DELETE ensures no pre-existing row carries an empty body across the
  upgrade.

- Update existing `src/indexer/mod.rs` tests where the upsert sets the new
  column path but doesn't change behavior:
  - `scan_inserts_one_md_file`, `rerun_is_idempotent_on_unchanged_vault`,
    `editing_bytes_updates_content_hash`,
    `mtime_only_change_preserves_content_hash`,
    `deleting_a_file_removes_its_row` — all already use `Scanner::run`
    and don't read the `content` column directly. They keep passing
    without modification (the new column populates transparently).
  - Add **two new tests**:
    - `scan_populates_content_for_inserted_files` — write
      `b"# hello\n\nbody"`, scan, query the row's `content` column,
      assert it equals `"# hello\n\nbody"`.
    - `scan_populates_content_for_updated_files` — write `b"# v1"`, scan;
      overwrite with `b"# v2"`, scan again; assert the row's `content`
      column is now `"# v2"` (not `"# v1"`).

**What lands**:
- Body content populates on first index and on every bytes-changed update.
- `Scanner::reindex_path` and `Scanner::remove_path` keep their public
  signatures from step 4. The new column is internal to the upsert SQL.
- One new function in `hash`. One refactored function in `indexer::mod`.

**Why a separate task**: it's a contract change to the `files` row's data
shape (the writers know about a new column). Splitting the storage write
from the search-side reads (Task 5.3) means the storage change has its
own commit and tests; if a search query later reveals a write-path bug,
bisect lands on this commit.

**Skill applied**: `.claude/skills/rusqlite-in-async/` — same per-event
`spawn_blocking` discipline; the new SQL stays inside the existing
transactions.

### Task 5.3 — Search query module (filesystem + content)

**Files**:
- `src/lib.rs` — `pub mod search;`
- `src/search/mod.rs` (new) — module root, re-exports the two query
  shapes:
  ```rust
  mod filesystem;
  mod content;

  pub use filesystem::{search_filesystem, FilesystemQuery, FilesystemResult};
  pub use content::{search_content, ContentQuery, ContentResult, ContentMatch};
  ```
- `src/search/filesystem.rs` (new) — pure-data query types and the
  `search_filesystem` async function:
  ```rust
  #[derive(Debug, Clone, Default)]
  pub struct FilesystemQuery {
      pub prefix: Option<String>,    // normalized: empty or ends with '/'
      pub glob: Option<String>,
      pub max_depth: Option<usize>,
      pub limit: usize,              // resolved default 100
  }

  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct FilesystemResult {
      pub path: String,
      pub size: i64,
      pub mtime: String,
      pub content_hash: String,
  }

  pub async fn search_filesystem(
      pool: SqlitePool,
      q: FilesystemQuery,
  ) -> Result<(Vec<FilesystemResult>, bool /* truncated */)> { ... }
  ```

  The implementation:
  1. Validates `prefix` (rejects absolute or `..`-bearing values with
     `anyhow!("invalid_prefix: ...")` — the HTTP layer maps the
     `invalid_prefix` token to its error code).
  2. Compiles `glob` via `globset::Glob` (returns
     `anyhow!("invalid_glob: ...")` on failure).
  3. Builds a SQL query like
     `SELECT path, size, mtime, content_hash FROM files
      WHERE path >= ?1 AND path < ?2 ORDER BY path ASC LIMIT ?3` —
     where `?1` is the prefix (or `''`) and `?2` is the prefix's
     successor (`prefix` with the last byte incremented; for `''` the
     bound is `'\x7f'` or simply omitted). The prefix range scan uses the
     `path` primary-key index.
  4. Applies the in-Rust glob filter and `max_depth` filter.
  5. Truncates at `limit + 1`; sets `truncated = true` if the +1 row is
     present.

  The work runs inside `task::spawn_blocking` (per
  `.claude/skills/rusqlite-in-async/`). One pool-borrow per query.

- `src/search/content.rs` (new) — same shape:
  ```rust
  #[derive(Debug, Clone)]
  pub struct ContentQuery {
      pub query: String,
      pub regex: bool,
      pub case_sensitive: bool,
      pub prefix: Option<String>,
      pub include_matches: bool,
      pub max_matches_per_file: usize,   // resolved default 5
      pub limit: usize,                  // resolved default 100
  }

  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct ContentResult {
      pub path: String,
      pub match_count: usize,
      pub matches: Vec<ContentMatch>,
  }

  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct ContentMatch {
      pub line: usize,
      pub text: String,
  }

  pub async fn search_content(
      pool: SqlitePool,
      q: ContentQuery,
  ) -> Result<(Vec<ContentResult>, bool /* truncated */)> { ... }
  ```

  The implementation:
  1. Validates `prefix` (same as filesystem).
  2. Compiles a `regex::Regex` if `regex == true` (emits
     `anyhow!("invalid_regex: ...")` on failure). If `regex == false`,
     stores the query as a substring; if `case_sensitive == false`,
     pre-lowercases the query for ASCII-folded matching.
  3. SQL: `SELECT path, content FROM files WHERE path >= ?1 AND path < ?2
     ORDER BY path ASC` (no SQL-side body filter — the spec's substring
     match runs in Rust). The prefix range scan limits the rows scanned.
  4. For each row, runs the matcher over `content`. For substring mode:
     - case-insensitive: pre-lowercased body via `to_ascii_lowercase`,
       then `find_iter` via `body_lc.match_indices(&query_lc)`.
     - case-sensitive: `body.match_indices(&query)`.
     For regex mode: `re.find_iter(&body)`.
  5. For each match, computes `(line, text)` from the match's start byte:
     line is `1 + body[..start].matches('\n').count()`; text is the
     line containing `start`, trimmed to 240 bytes (UTF-8-safe via
     `floor_char_boundary`-ish logic — implemented manually since
     `floor_char_boundary` is unstable).
  6. Collects per-file results; emits when `match_count > 0`; truncates
     `matches` at `max_matches_per_file` while keeping the full
     `match_count`.
  7. Truncates the file-level result set at `limit + 1`; sets
     `truncated`.

  The work runs inside `task::spawn_blocking`.

- Twelve or so unit tests across the two files:
  - **Filesystem**:
    - `filesystem_returns_empty_when_index_is_empty`
    - `filesystem_returns_all_paths_when_no_filters`
    - `filesystem_glob_filter_matches_extension`
    - `filesystem_prefix_filter_excludes_outside_subdir`
    - `filesystem_max_depth_caps_descent`
    - `filesystem_truncated_when_more_than_limit`
    - `filesystem_invalid_glob_returns_invalid_glob_error`
    - `filesystem_invalid_prefix_returns_invalid_prefix_error`
    - `filesystem_results_are_sorted_ascending_by_path`
  - **Content**:
    - `content_substring_matches_case_insensitive_by_default`
    - `content_substring_case_sensitive_when_flag_set`
    - `content_regex_matches_alternation`
    - `content_regex_invalid_returns_invalid_regex_error`
    - `content_regex_ignores_case_sensitive_flag`
    - `content_match_count_reflects_full_count_not_truncated`
    - `content_matches_truncated_at_max_matches_per_file`
    - `content_truncated_at_file_limit`
    - `content_phrase_spans_line_boundary` (resolution 3 confirmation)
    - `content_match_text_trimmed_at_240_bytes`
    - `content_match_line_is_one_indexed`
    - `content_omits_matches_when_include_matches_false`
    - `content_invalid_prefix_returns_invalid_prefix_error`

  Each test uses an in-memory `Store` (via `Store::open` against
  `tempdir()`) seeded with a few rows via direct SQL inserts. No vault
  filesystem operations needed — the query path reads from the store, not
  from disk.

**What lands**:
- One new module (`src/search/`). All query work is async-friendly and
  uses `spawn_blocking` for SQL.
- New deps: `regex = "1"`. Already-in-tree: `globset`, `rusqlite`,
  `r2d2`, `tokio`.

**Why a separate task**: the query module is the load-bearing piece for
the shipping gate — every endpoint and every CLI command goes through
it. Splitting it from the HTTP layer (Task 5.4) means the matching logic
gets its own commit with its own tests; HTTP-shape concerns and
matching-shape concerns stay decoupled. Same shape as step 4 task 4.3
(indexer outcomes carrying content_hash before the consumer wired them
up).

**Skill applied**: `.claude/skills/rusqlite-in-async/` — every SQL call
goes through `task::spawn_blocking`; the `pool.get()` is inside the
closure.

### Task 5.4 — HTTP API: types, router, handlers (`/health`, `/status`, `/search/*`)

**Files**:
- `src/lib.rs` — `pub mod api;`
- `src/api/mod.rs` (new) — module root and the `Router` factory:
  ```rust
  pub mod types;
  mod health;
  mod search;
  mod status;

  pub use types::*;

  pub fn router(state: ApiState) -> Router { ... }

  #[derive(Clone)]
  pub struct ApiState {
      pub pool: SqlitePool,
      pub vault: PathBuf,
      pub outbox_path: PathBuf,
  }
  ```
- `src/api/types.rs` (new) — the request/response types with serde
  derives matching the JSON shapes in [§ Deferred decision 2](#2-precise-json-response-shapes).
  Includes the optional `vault` field on the search-result types:
  ```rust
  #[derive(Debug, Clone, Serialize)]
  pub struct FilesystemResultJson {
      pub path: String,
      pub size: i64,
      pub mtime: String,
      pub content_hash: String,
      #[serde(skip_serializing_if = "Option::is_none")]
      pub vault: Option<String>,
  }
  ```
  Same field on `ContentResultJson`. Status / health types do not get
  the field. Error envelope:
  ```rust
  #[derive(Debug, Clone, Serialize)]
  pub struct ErrorEnvelope {
      pub error: ErrorBody,
  }

  #[derive(Debug, Clone, Serialize)]
  pub struct ErrorBody {
      pub code: String,
      pub message: String,
  }
  ```
- `src/api/health.rs` (new) — one handler:
  ```rust
  async fn health() -> impl IntoResponse {
      Json(json!({ "status": "ok" }))
  }
  ```
- `src/api/status.rs` (new) — one handler that runs SQL inside
  `spawn_blocking`:
  ```rust
  async fn status(State(s): State<ApiState>) -> Result<Json<StatusResponse>, ApiError> {
      let pool = s.pool.clone();
      let (count, last_indexed) = task::spawn_blocking(move || -> Result<_> {
          let conn = pool.get()?;
          let count: i64 = conn.query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))?;
          let last: Option<String> = conn.query_row(
              "SELECT MAX(indexed_at) FROM files", [], |r| r.get(0)
          ).optional()?;
          Ok((count, last))
      })
      .await
      .context("spawn_blocking join error in status handler")??;
      let outbox_size = std::fs::metadata(&s.outbox_path).map(|m| m.len()).unwrap_or(0);
      Ok(Json(StatusResponse {
          vault: s.vault.display().to_string(),
          indexed_file_count: count as u64,
          last_indexed_at: last,
          outbox: OutboxStatus {
              path: s.outbox_path.display().to_string(),
              size_bytes: outbox_size,
          },
      }))
  }
  ```
- `src/api/search.rs` (new) — two handlers (`POST /search/filesystem`,
  `POST /search/content`) that:
  1. Deserialize the request body via `axum::Json`. On deserialization
     failure, return `400` with `code = "invalid_request"`.
  2. Resolve defaults (`limit`, `max_matches_per_file`) and normalize
     `prefix`.
  3. Call the matching `search::*` function from Task 5.3.
  4. Map the returned `Vec<FilesystemResult>` / `Vec<ContentResult>` to
     the JSON types — populating `vault: None` (the v0 always-omitted
     value).
  5. Map errors: `invalid_glob` / `invalid_regex` / `invalid_prefix` →
     400 with the matching error code; everything else → 500
     `internal`.

  The router shape:
  ```rust
  Router::new()
      .route("/health", get(health::health))
      .route("/status", get(status::status))
      .route("/search/filesystem", post(search::filesystem))
      .route("/search/content", post(search::content))
      .with_state(state)
  ```

  An `ApiError` enum implements `IntoResponse` to produce the error
  envelope. Two layers: error code → HTTP status (400 vs 500), error
  code → message string.

- Eight or so handler-level unit tests using `axum::body::to_bytes` and
  the router as a `tower::Service`:
  - `health_returns_200_with_status_ok`
  - `status_reports_zero_files_when_index_empty`
  - `status_reports_count_and_last_indexed_after_seeding`
  - `search_filesystem_returns_results_for_glob`
  - `search_filesystem_invalid_glob_returns_400_with_code`
  - `search_content_returns_results_with_matches`
  - `search_content_invalid_regex_returns_400_with_code`
  - `search_response_omits_vault_field_in_v0`

  Each test seeds an in-memory `Store`, builds the router, sends a
  `Request`, asserts on status + body. No real network socket; this is
  axum's standard "router-as-service" test pattern.

**What lands**:
- New module (`src/api/`). One `Router` factory plus four handlers.
- New deps: `axum = "0.7"`. (`serde`, `serde_json`, `tokio` already in
  tree.)
- Handlers do not yet bind to a port; that wiring is Task 5.5.

**Why a separate task**: the route map and request/response types are
the v0 contract. Landing them in their own commit means the contract
(JSON shapes, error envelope, status codes) lives in a single diff a
reviewer can read end-to-end. Wiring this surface into `hmnd`'s lifecycle
is a different concern; same shape as step 4 tasks 4.2 (writer surface)
vs. 4.4 (consumer wiring).

**Skill applied**: `.claude/skills/rusqlite-in-async/` — `/status` and
both search handlers go through `spawn_blocking`.

### Task 5.5 — Wire HTTP server into `hmnd` (graceful shutdown, `/status` outbox path)

**Files**:
- `src/bin/hmnd.rs` — extend `run_daemon`:
  1. After opening the outbox, build the `ApiState`:
     ```rust
     let api_state = api::ApiState {
         pool: store.pool(),
         vault: config.vault.0.clone(),
         outbox_path: outbox_path.clone(),
     };
     let app = api::router(api_state);
     ```
  2. Bind the listener:
     ```rust
     let listener = tokio::net::TcpListener::bind(&config.http.bind)
         .await
         .with_context(|| format!("binding HTTP server to {}", config.http.bind))?;
     tracing::info!(bind = %config.http.bind, "hmnd: http server listening");
     ```
  3. Spawn the server with graceful shutdown wired to the same
     `shutdown_rx` the watcher uses:
     ```rust
     let mut http_shutdown = shutdown_rx.clone();
     let http_handle = tokio::spawn(async move {
         let server = axum::serve(listener, app)
             .with_graceful_shutdown(async move {
                 let _ = http_shutdown.wait_for(|v| *v).await;
             });
         if let Err(e) = server.await {
             tracing::warn!(error = ?e, "hmnd: http server task ended with error");
         }
     });
     ```
  4. Update the startup banner to log the resolved bind address (already
     present from step 1; no change needed).
  5. After the consumer drains, await the http handle:
     ```rust
     let _ = http_handle.await;
     ```
     (Order: shutdown signal → consumer drains → consumer task exits →
     `drop(watcher_handle)` → http server has been awoken via the same
     signal in parallel; await it last so any in-flight requests
     complete.)

  Bind failure (port in use, permission denied) is fatal at startup —
  `run_daemon` returns Err and `dispatch` propagates to `main`, exit 1.
  This matches step 4's outbox-open-failure shape.

- One new integration-style unit-ish test in `src/bin/hmnd.rs` is *not*
  added (binaries are awkward to unit-test); end-to-end coverage is
  Task 5.7.

**What lands**:
- The HTTP server is **on** by default in `hmnd` no-subcommand mode.
- `hmnd scan` and `hmnd config-validate` are unchanged (they don't run
  the server).
- Graceful shutdown: SIGINT/SIGTERM → both watcher and HTTP server stop
  cleanly. In-flight HTTP requests complete; new connections are
  refused after the listener task exits.

**Why this task is medium risk** (not high): it composes the new API
(Task 5.4) with the existing watcher / outbox / shutdown plumbing. The
composition is mechanical; the only fresh invariant is "HTTP server
exits on the same shutdown signal as the watcher consumer," which axum's
`with_graceful_shutdown` already encapsulates. The bind-failure-is-fatal
rule is the same shape as outbox-open-failure (step 4).

**Manual smoke verification** (the task agent runs this before reporting
done):
1. Start `hmnd` against a small test vault.
2. `curl -s http://127.0.0.1:7777/health` → `{"status":"ok"}`.
3. `curl -s http://127.0.0.1:7777/status` → JSON with vault, count,
   last_indexed_at, outbox.
4. `curl -s -X POST http://127.0.0.1:7777/search/filesystem -H
   'content-type: application/json' -d '{"glob":"**/*.md"}'` → JSON
   results.
5. `curl -s -X POST http://127.0.0.1:7777/search/content -H
   'content-type: application/json' -d '{"query":"hello"}'` → JSON
   results.
6. SIGINT → daemon exits 0 with the existing "drain complete, exiting
   cleanly" log line.

### Task 5.6 — `hmn` HTTP client + commands

**Files**:
- `src/lib.rs` — `pub mod client;`
- `src/client.rs` (new) — typed reqwest wrapper:
  ```rust
  pub struct DaemonClient {
      base_url: String,
      http: reqwest::Client,
  }

  impl DaemonClient {
      pub fn from_config(config: &Config, override_url: Option<&str>) -> Result<Self> { ... }
      pub async fn health(&self) -> Result<HealthResponse> { ... }
      pub async fn status(&self) -> Result<StatusResponse> { ... }
      pub async fn search_filesystem(&self, q: &FilesystemRequest) -> Result<FilesystemResponse> { ... }
      pub async fn search_content(&self, q: &ContentRequest) -> Result<ContentResponse> { ... }
  }
  ```

  Default URL build: when `override_url` is `None`, parse
  `config.http.bind` as `host:port` and produce `http://<host>:<port>`.
  When `override_url` is `Some`, use it verbatim. Both env var
  (`HYPOMNEMA_DAEMON_URL`) and `--daemon-url` are surfaced through the
  CLI's existing `cli.daemon_url` field.

  Error mapping: HTTP non-2xx returns the daemon's error body parsed as
  `ErrorEnvelope`; the client returns `Err(anyhow!("{code}: {message}"))`.
  Connection refused / DNS errors return an `Err` with code-path
  context; the binary translates these to exit code 4 (per
  `cli.md`'s "Daemon not reachable").

  The reqwest client is built **without TLS features**:
  `reqwest = { version = "0.12", default-features = false, features = ["json"] }`.
  Localhost only is the v0 assumption (architecture overview); no TLS
  surface to maintain.

- The shared types between the daemon and client live in `src/api/types.rs`
  from Task 5.4. `client.rs` re-exports the request/response types it
  uses; serde does both the daemon's response serialization and the
  client's response deserialization.

- `src/bin/hmn.rs` — replace the placeholder `not implemented yet`
  branch with real handlers:
  ```rust
  #[tokio::main]
  async fn main() -> ExitCode {
      // ...config + logging unchanged...
      let runtime_result = match cli.command {
          Command::Search { mode } => match mode {
              SearchMode::Filesystem { query, prefix, limit } => {
                  cmd_search_filesystem(&config, cli.daemon_url.as_deref(),
                                         cli.json, query, prefix, limit).await
              }
              SearchMode::Content { query, prefix, limit } => {
                  cmd_search_content(&config, cli.daemon_url.as_deref(),
                                     cli.json, query, prefix, limit).await
              }
              SearchMode::Semantic { .. } => {
                  eprintln!("hmn: semantic search lands in step 7");
                  return ExitCode::from(1);
              }
          },
          Command::Status => cmd_status(&config, cli.daemon_url.as_deref(), cli.json).await,
      };
      // ...exit-code mapping...
  }
  ```

  `hmn` becomes a `#[tokio::main]` async binary in this task — the
  reqwest client is async and we need a runtime. The `tokio` crate is
  already a dependency.

  Output rendering:
  - `hmn search filesystem`: text mode prints one line per result
    (`<path>  <size> bytes  <mtime>`), with a trailing line
    `(truncated; raise --limit)` when `truncated == true`. JSON mode
    pretty-prints the response.
  - `hmn search content`: text mode prints one block per result —
    `<path> (<match_count> matches)` followed by indented match lines
    (`  <line>: <text>`), separated by blank lines. JSON mode
    pretty-prints.
  - `hmn status`: text mode prints a four-line block (vault, file
    count, last indexed, outbox size in human-readable form). JSON
    mode pretty-prints.

  For `--limit`: when omitted, the command sends no `limit` field (the
  daemon applies its default). Same for `--prefix`.

  Treatment of `SearchMode::Semantic`: stays a stub that exits 1 with a
  pointer-to-step-7 message (semantic search lands in step 7; the `hmn`
  CLI surface stays unchanged from step 1, only its `Filesystem` and
  `Content` arms light up).

- Six or so unit tests on `client.rs` (using `axum`'s test router as a
  mock daemon — same pattern as Task 5.4 tests, but flipped):
  - `client_default_url_builds_from_config_bind`
  - `client_override_url_takes_precedence`
  - `client_health_parses_response`
  - `client_search_filesystem_round_trips`
  - `client_search_content_round_trips`
  - `client_translates_400_to_anyhow_with_code`
  - `client_returns_connect_error_when_daemon_down`

**What lands**:
- The CLI does real work for the first time. `hmn search filesystem`,
  `hmn search content`, and `hmn status` are functional end-to-end.
- New deps: `reqwest = { version = "0.12", default-features = false,
  features = ["json"] }`. (`tokio` already in tree.)

**Why this is the second-to-last code task**: the CLI is the
human-visible artifact for the shipping gate. Landing it after the
daemon-side surface (Task 5.4) and after wiring (Task 5.5) means the
client is built against a known-good server. Same shape as step 1's
`hmn` skeleton landing after `hmnd`'s skeleton.

**Skill applied**: none specific to this task; reqwest + axum are the
standard idioms.

### Task 5.7 — Integration tests against live daemon

**Files**:
- `tests/http.rs` (new) — integration tests that spawn a real `hmnd`
  process (no, actually: spawn the in-process server via `tokio::spawn`
  on a free port — same shape as the unit tests in Task 5.4 but full
  end-to-end including reqwest):
  ```rust
  struct LiveDaemon {
      base_url: String,
      shutdown: watch::Sender<bool>,
      _join: JoinHandle<()>,
      _data_dir: TempDir,
      _vault_dir: TempDir,
  }

  async fn spawn_live_daemon() -> LiveDaemon { ... }
  ```

  The `spawn_live_daemon` helper:
  1. Creates tempdir vault + tempdir data_dir.
  2. Seeds the vault with a small fixture set (e.g., 4–5 files including
     nested dirs and a file with multi-line content).
  3. Builds a `Config` via `Config::load` (tempdir-rooted toml on disk —
     mirrors `tests/outbox.rs`'s pattern).
  4. Opens the `Store`, runs an initial scan.
  5. Builds the `ApiState` and `Router`.
  6. Binds to `127.0.0.1:0` (kernel-assigned port), captures the
     resolved port, builds `base_url`.
  7. Spawns `axum::serve(...).with_graceful_shutdown(...)` on a tokio
     task; returns the handle.

  Cases (mirrors the roadmap shipping criteria + a few extras):
  1. *Health endpoint reachable.* (Criterion 3.) `GET /health` →
     `{"status":"ok"}`, status 200.
  2. *Status endpoint reports vault, file count, last indexed, outbox.*
     (Criterion 4.) `GET /status` → JSON matches the seeded vault path;
     `indexed_file_count == 5`; `last_indexed_at` is non-null and parses
     as RFC3339; `outbox.size_bytes` is `0` (no events yet on a
     daemon that didn't run the watcher).
  3. *Filesystem search with glob returns matching files.* (Criterion 1.)
     `POST /search/filesystem` with `{"glob":"**/*.md"}` returns all 5
     fixtures; with `{"glob":"notes/*.md"}` returns only the nested
     ones.
  4. *Filesystem search with prefix narrows results.* `POST` with
     `{"prefix":"notes/"}` returns only `notes/...` paths.
  5. *Filesystem search with invalid glob returns 400 + code.*
     `{"glob":"["}` → 400 with `error.code = "invalid_glob"`.
  6. *Content search substring matches case-insensitive by default.*
     (Criterion 2.) Seed a file with the word `Pgvector`; query
     `{"query":"pgvector"}` returns it. With `{"query":"pgvector",
     "case_sensitive":true}` returns nothing.
  7. *Content search regex matches alternation.* `{"query":"foo|bar",
     "regex":true}` matches files containing either word.
  8. *Content search returns line + text for each match.* Verify
     `matches[0].line` is the 1-indexed line and `matches[0].text` is
     the line content.
  9. *Content search with invalid regex returns 400 + code.*
     `{"query":"(","regex":true}` → 400 with `error.code =
     "invalid_regex"`.
 10. *Content search phrase across line boundary matches.* (Resolution 3
     confirmation.) Seed a file with `"foo bar"` split across `\n`;
     query `{"query":"foo\\sbar","regex":true}` returns it.
 11. *Truncation flag is set when results exceed limit.* Seed 3 files;
     query with `{"glob":"**/*.md","limit":2}` returns 2 results +
     `truncated: true`.
 12. *Vault field absent in v0 wire bytes.* Parse the response body via
     `serde_json::Value`; assert that no result entry contains a `vault`
     key. (Resolution 1 confirmation.)
 13. *Graceful shutdown closes the listener.* Trigger `shutdown.send(true)`,
     await the join handle, assert subsequent `reqwest::get` returns a
     connection error.

- One **end-to-end smoke test using the actual `hmn` binary** in
  `tests/cli.rs` (new):
  - Build `hmn` via `cargo build --bin hmn` (the test depends on the
    binary being built; CI already runs `cargo build` before `cargo
    test`).
  - Spawn the live daemon as above.
  - Use `assert_cmd` or `std::process::Command` to invoke
    `target/debug/hmn --daemon-url http://127.0.0.1:<port> search
    filesystem '**/*.md'`. Assert exit code 0 and that stdout contains
    expected paths.
  - Same for `hmn search content 'pgvector'` and `hmn status`.
  - Same for `hmn --json status` and assert the output parses as JSON.

  Add `assert_cmd = "2"` as a dev-dependency for this test.
  Alternative: use `Command::new(env!("CARGO_BIN_EXE_hmn"))` directly,
  which avoids the new dev-dep — Cargo populates this env var at test
  build time. **Pick the env var path** — fewer deps, same outcome.

  Five test cases:
  1. `hmn_search_filesystem_text_mode` — text output contains expected
     paths.
  2. `hmn_search_filesystem_json_mode` — `--json` output parses; has
     `results`, `truncated`.
  3. `hmn_search_content_text_mode` — text output shows `path
     (<count> matches)` block.
  4. `hmn_status_text_mode` — output contains the vault path.
  5. `hmn_status_when_daemon_unreachable_exits_4` — `hmn` against an
     unbound URL exits 4 (per `cli.md`'s exit-code table).

- Existing tests that need updates:
  - `tests/skeleton.rs` — no change. `hmnd config-validate` and `hmnd
    scan` don't touch the HTTP server.
  - `tests/scan.rs` — no change. Scan-only flow.
  - `tests/watch.rs` and `tests/outbox.rs` — no change. They don't
    spawn the HTTP server.
  - `tests/config.rs` — no change. `[http].bind` parsing is already
    covered.

**Per-test layout**: each case in its own `#[tokio::test]` (for the HTTP
tests) or `#[test]` (for the CLI tests). Each test owns its own
tempdirs and live daemon — no shared fixture state.

**Footnote on flake**: same anti-flake rule as steps 3 and 4 — no
polling-loop helpers that hide timing. The tests don't depend on the
watcher's debouncer (the seeded vault is scanned once at startup; no
filesystem mutations during the test). The only timing concern is the
HTTP server's startup latency: bind-then-connect is synchronous
(`TcpListener::bind` returns when the socket is ready), so no settle
window is needed. Connection-refused errors after `shutdown.send(true)`
have a small race window — repeat-until-error in case 13 is acceptable
since the test asserts the error eventually surfaces.

**Cross-platform**: macOS + Linux covered. The `127.0.0.1:0`
kernel-assigned-port pattern works on both. The CLI test runs the
locally-built `hmn` binary (`CARGO_BIN_EXE_hmn`) which exists on both
platforms.

**3× consecutive flake-check** before reporting done (precedent: step 3
task 3.5, step 4 task 4.5).

### Task 5.8 — Reference docs reflect step-5 resolutions

**Files**:
- `docs/specs/filesystem-search.md`:
  - § Data Schema / Request: keep the existing YAML; add a note that the
    HTTP endpoint accepts the same fields as a JSON body via
    `POST /search/filesystem`.
  - § Data Schema / Response: add the `vault` field to the field table:
    | `vault` | string | no | Reserved; always absent in v0. Will carry the source vault identifier when multi-vault ships. |
  - § Edge Cases: append a `prefix semantics` subsection naming the
    path-prefix-not-glob rule (resolution 4).
  - § Open Questions: line 92 (regex alternative) **stays open** with a
    one-line resolution note: "v0 ships glob-only; see [step-5 workplan
    § Deferred decision 5](../roadmap/step-05-workplan.md#5-regex-alternative-to-glob)
    — no field added, additive when needed."
- `docs/specs/content-search.md`:
  - § Data Schema / Response: add `vault` field row (same shape as
    filesystem-search above).
  - § Semantics: tighten the existing prose:
    - "Default: case-insensitive substring match" → add "(ASCII-folded;
      Unicode case folding is not applied in v0)."
    - "Optional: regex mode (syntax TBD; likely Rust's regex crate
      flavor)" → tighten to "Optional: regex mode using the Rust `regex`
      crate's default Unicode flavor. The request's `case_sensitive`
      flag is ignored when `regex: true`; case-sensitivity is a
      property of the pattern (`(?i)foo`)."
    - Append: "Phrase searches span line boundaries — the matcher
      operates over the file's full byte content, not per-line. See
      [step-5 workplan § Deferred decision 3](../roadmap/step-05-workplan.md#3-phrase-search-across-line-boundaries)."
  - § Edge Cases: add a `Lossy UTF-8` subsection — invalid UTF-8 bytes
    are decoded with `String::from_utf8_lossy` before storage; matches
    against the lossy form are still surfaced.
  - § Open Questions: line 86 (phrase across lines) flips from `[ ]` to
    `[x]` with one resolution line: "Resolved in step 5 as line-agnostic
    matching. See [step-5 workplan § Deferred decision 3](../roadmap/step-05-workplan.md#3-phrase-search-across-line-boundaries)."
    Line 87 (frontmatter-only matches) **stays open**.
- `docs/specs/change-events.md`:
  - § Data Schema table: add a new row for the optional `vault` field
    (forward-compat). New row text is the one in [§ Deferred decision 1](#1-multi-vault-forward-compat-vault-field).
  - The example envelope (`### Event Envelope (minimum)`) is unchanged —
    v0 omits the field, so the on-the-wire example stays correct.
  - § Open Questions: lines 100, 101, 102 stay open per the roadmap. No
    new resolution lines.
- `docs/specs/semantic-search.md`:
  - § Data Schema / Response: add `vault` field row (forward-compat
    only; semantic search ships in step 7).
  - One paragraph after the response example: "The `vault` field is
    present in the response shape from step 5 onwards (added when the
    HTTP filesystem and content endpoints lit up); see [step-5 workplan
    § Deferred decision 1](../roadmap/step-05-workplan.md#1-multi-vault-forward-compat-vault-field)."
- `docs/architecture/overview.md`:
  - § Search API: append two short sentences. "Step 5 ships the HTTP
    surface: `/search/filesystem` and `/search/content` over POST,
    `/health` and `/status` over GET, all bound to
    `config.http.bind` (default `127.0.0.1:7777`). All four shapes plus
    the outbox envelope carry an optional `vault` field, omitted in v0,
    reserved for forward-compat with multi-vault."
  - § Communication Patterns / External Communication: update the
    inbound HTTP row to name the four endpoints explicitly.
- `docs/reference/cli.md`:
  - `hmn search` section: keep the existing prose; append a paragraph:
    "As of step 5, `hmn search filesystem` and `hmn search content` are
    functional. `hmn search semantic` continues to print 'lands in step
    7.' Output is human-formatted by default; pass `--json` to render
    the daemon's JSON response unchanged. When `truncated == true` the
    text mode prints `(truncated; raise --limit)` after the results."
  - `hmn status` section: append: "The output shows the daemon's vault
    path, indexed file count, last-indexed timestamp (or `—` when the
    index is empty), and outbox file size. Exit code 4 if the daemon is
    not reachable."
  - `hmnd` (no-subcommand) section: append: "Step 5 ships the HTTP
    server alongside the watcher. `/health` returns 200 OK; `/status`
    returns a JSON snapshot; `/search/filesystem` and `/search/content`
    accept POST with a JSON body. See the search specs for shapes."
- `docs/reference/configuration.md`:
  - `[http].bind` row: keep the existing prose; append: "Step 5 binds
    the Axum router on this address; failure to bind is fatal at
    daemon startup."
- `notes/roadmap/archive/roadmap-1.md`:
  - Step 5 gets `**Status**: shipped <date>` at the top of its section
    (filled in at the actual ship moment, not at workplan time).

**Why this task is last**: docs follow code, and the spec column flips
in particular benefit from having the implementation validate that
"per-result `vault` field with `skip_serializing_if` is a one-line
serde change, not a wire-shape regression" before locking the wording
in.

---

## Test strategy

**Unit tests** (`#[cfg(test)]`):
- `src/store/schema.rs` — five new tests on migration 0002 (Task 5.1).
- `src/indexer/hash.rs` — three new tests on `read_and_hash` (Task 5.2).
- `src/indexer/mod.rs` — two new tests on body-on-write (Task 5.2);
  existing tests pass unchanged.
- `src/search/filesystem.rs` — nine tests on the filesystem query (Task
  5.3).
- `src/search/content.rs` — thirteen tests on the content query (Task
  5.3).
- `src/api/types.rs` (or co-located) — light tests on serde
  round-tripping for the request/response types, especially the
  `vault` field's omit-when-`None` behavior (Task 5.4).
- `src/api/{health,status,search}.rs` — eight handler-level tests (Task
  5.4).
- `src/client.rs` — seven client-side tests (Task 5.6).

**Integration tests** (`tests/`):
- `tests/http.rs` (new) — thirteen end-to-end cases against a live
  in-process daemon (Task 5.7).
- `tests/cli.rs` (new) — five cases that invoke the built `hmn` binary
  via `CARGO_BIN_EXE_hmn` (Task 5.7).
- `tests/skeleton.rs`, `tests/scan.rs`, `tests/watch.rs`,
  `tests/outbox.rs`, `tests/config.rs` — no changes.

**Lint and format**: `cargo clippy --all-targets -- -D warnings` and
`cargo fmt --all -- --check` before review.

**Cross-platform**: macOS + Linux covered. Windows out of v0. No
`#[cfg(unix)]` gating in this step (no rename-shape tests; the HTTP
server is platform-agnostic).

**Anti-flake rule** (from steps 3 and 4): no polling-loop helpers that
hide timing. The HTTP tests don't depend on the debouncer; bind +
graceful-shutdown are deterministic. 3× consecutive flake-check on
Task 5.7 before reporting done.

---

## Definition of done

- [ ] `cargo run --bin hmnd` against a real vault binds to
      `127.0.0.1:7777` and serves `/health`, `/status`,
      `/search/filesystem`, `/search/content`.
- [ ] `curl -s http://127.0.0.1:7777/health` returns 200 with
      `{"status":"ok"}` (criterion 3).
- [ ] `hmn search filesystem 'notes/*.md'` against the running daemon
      prints matching files in human-formatted text (criterion 1).
- [ ] `hmn search content 'pgvector'` against the running daemon prints
      files with line snippets (criterion 2).
- [ ] `hmn status` against the running daemon prints vault path,
      indexed file count, last indexed time, outbox file size
      (criterion 4).
- [ ] `hmn --json` mode renders the daemon's JSON response unchanged
      (no human formatting); piping to `jq` works.
- [ ] Result shapes match what the specs describe; `truncated: true`
      surfaces when results exceed `limit`. No pagination (criterion 5).
- [ ] Every result entry on `/search/filesystem` and `/search/content`
      carries the `vault` field as `Option<String>` in the serde shape;
      v0 always-omitted on the wire (resolution 1 confirmation).
- [ ] The outbox `ChangeEvent` envelope carries `vault: Option<String>`
      in the serde shape; v0 always-omitted on the wire (resolution 1
      confirmation).
- [ ] Migration 0002 advances `PRAGMA user_version` to 2 and adds the
      `content` column with `DEFAULT ''` and `NOT NULL`. Existing rows
      are cleared by the migration.
- [ ] Indexer populates `content` on every `Inserted` and `Updated`
      branch. Stat-gate skips reading the file as before.
- [ ] Content body decoding is `String::from_utf8_lossy`; the hash
      stays computed over raw bytes (resolution A confirmation).
- [ ] Filesystem search is glob-only — no `regex` field on the request
      shape (resolution 5 confirmation).
- [ ] Content search defaults to case-insensitive ASCII-folded
      substring matching; regex mode ignores `case_sensitive` and uses
      the Rust `regex` crate's default Unicode flavor (resolution 4
      confirmation).
- [ ] Content phrase search across line boundaries returns matches
      (resolution 3 confirmation, verified by `tests/http.rs` case 10).
- [ ] All HTTP error responses use the `{ "error": { "code", "message"
      } }` envelope; codes are stable lowercased snake_case.
- [ ] Bind failure at startup is fatal (daemon exits 1).
- [ ] Graceful shutdown: SIGINT → both watcher and HTTP server stop
      cleanly within the existing drain window; the daemon exits 0.
- [ ] All file I/O on the outbox and store goes through
      `tokio::task::spawn_blocking` — verified by reading the diff.
- [ ] `cargo test`, `cargo clippy --all-targets -- -D warnings`,
      `cargo fmt --all -- --check` all pass.
- [ ] Reference docs reflect the resolutions:
      `docs/specs/filesystem-search.md`,
      `docs/specs/content-search.md`,
      `docs/specs/change-events.md`,
      `docs/specs/semantic-search.md`,
      `docs/architecture/overview.md`,
      `docs/reference/cli.md`,
      `docs/reference/configuration.md`.
- [ ] Roadmap marks Step 5 shipped with the date.
- [ ] Step 5 retrospective appended to
      `notes/project-planning-workflow-notes.md` (using the retro
      template).
- [ ] **End-of-round retrospective** appended to the same file —
      Step 5 is the last step in this roadmap round; the retro covers
      both step 5 and the round as a whole (per
      [`roadmap.md` § After step 5](./roadmap-1.md#after-step-5)).

---

## Cross-references

**Specs / decisions**:
- [`specs/filesystem-search.md`](../../../docs/specs/filesystem-search.md) — primary
  spec for `/search/filesystem`. Open question on line 92 (regex
  alternative) stays open per
  [§ Deferred decision 5](#5-regex-alternative-to-glob).
- [`specs/content-search.md`](../../../docs/specs/content-search.md) — primary
  spec for `/search/content`. Open question on line 86 (phrase across
  lines) resolved in this step; line 87 (frontmatter-only) stays open.
- [`specs/change-events.md`](../../../docs/specs/change-events.md) — gains the
  `vault` field row in this step's doc-update task. Open questions on
  lines 100, 101, 102 stay open.
- [`specs/semantic-search.md`](../../../docs/specs/semantic-search.md) — gains the
  `vault` field row in this step's doc-update task; the rest of the
  spec ships in step 7.
- [ADR-0004: Three search modes as peers](../../../docs/decisions/0004-three-search-modes-as-peers.md)
  — the v0 plan ships filesystem + content first, semantic in step 7;
  this step is the *peer* shape's first concrete realization.
- [ADR-0008: Two binaries (hmnd + hmn) in one crate](../../../docs/decisions/0008-two-binary-daemon-plus-cli.md)
  — `hmn`'s HTTP client is the first time this step lights up the
  hmn-only side of the binary split. Both binaries link the full
  library; the runtime restriction is an organizational concern.
- [ADR-0006: Outbox outside the watched directory](../../../docs/decisions/0006-outbox-outside-watched-directory.md)
  — the outbox `vault` field is forward-compat only; the field stays
  optional and omitted, so the ADR's read-only-vault invariant is
  unchanged.

**Reference docs (updated by this step)**:
- [Filesystem-search spec](../../../docs/specs/filesystem-search.md)
- [Content-search spec](../../../docs/specs/content-search.md)
- [Change-events spec](../../../docs/specs/change-events.md)
- [Semantic-search spec](../../../docs/specs/semantic-search.md)
- [Architecture overview](../../../docs/architecture/overview.md)
- [CLI reference](../../../docs/reference/cli.md)
- [Configuration reference](../../../docs/reference/configuration.md)

**Pitfalls touched** (from
[`docs/implementation/appendices/tech-stack/pitfalls.md`](../../../docs/implementation/appendices/tech-stack/pitfalls.md)):
- #1 *Blocking the async runtime with rusqlite* — every search handler
  goes through `spawn_blocking`. The pattern carries over from steps
  2–4.
- #5 *Putting state in the watched directory* — already enforced by the
  config validator; the HTTP server reads from the store, never writes
  to the vault.

**Skills applied**:
- `.claude/skills/rusqlite-in-async/` — every SQL call (search handlers,
  status handler, indexer body writes) goes through `spawn_blocking`.

**Skills that don't apply yet**:
- `filesystem-watching` — step 3 covered the watcher; step 5 doesn't
  introduce new `notify` sites.
- `sqlite-vec-extension` — step 6/7 (semantic search).
- `markdown-chunking` — step 6 (chunking).

---

## Out of scope (will not appear in this PR)

- **Pagination.** Specs prescribe truncate-and-flag; this step honors
  that. A future paginated mode is additive (new request fields,
  optional response fields) and not pursued in v0.
- **Frontmatter summaries in filesystem results** ([filesystem-search.md
  line 93](../../../docs/specs/filesystem-search.md)). Spec open question stays
  open.
- **Regex alternative to glob** ([filesystem-search.md line 92](../../../docs/specs/filesystem-search.md)).
  Spec open question stays open per
  [§ Deferred decision 5](#5-regex-alternative-to-glob).
- **Frontmatter-only-match distinguishing** ([content-search.md line
  87](../../../docs/specs/content-search.md)). Spec open question stays open.
- **Health metrics beyond reachability.** `/health` returns
  `{"status":"ok"}` and nothing else. A future endpoint can carry
  index-fresh-vs-stale, watcher-running, embedding-service-reachable
  signals; not now.
- **Multi-vault implementation.** The `vault` field in v0 is
  forward-compat scaffolding only; the daemon stays single-vault and
  the field is always omitted on the wire. Implementation is post-v0.
- **MCP transport.** Step 8.
- **Semantic search.** Step 7.
- **Chunking and embedding.** Step 6.
- **TLS / auth on the HTTP endpoint.** Architecture overview pins
  localhost-only; reqwest is built without TLS features. Future
  remote-access support lands when v0+.
- **FTS5 pre-filter for content search.** A future perf optimization;
  v0 ships scan-everything substring/regex matching. The vault sizes
  v0 targets (≤10k files) make this acceptable.
- **`tower-http` middleware (request logging, CORS, compression).**
  None added in v0. Slim deps; bring in when a specific need surfaces.
- **`hmnd` reload-config without restart.** Out of scope; restart on
  config change.
- **`hmn search semantic` real implementation.** Stubs out with a
  pointer-to-step-7 message; the existing CLI surface is preserved.

If review surfaces a strong reason to pull any of the above forward,
that's a roadmap revision per the
[mid-step roadmap revision](../../project-planning-workflow-notes.md#open-questions-about-the-workflow-itself)
open question.

---

## Net new dependencies

Three new runtime deps; one new dev-dep is intentionally **not** added
(see `tests/cli.rs` rationale in Task 5.7). Final list:

| Crate | Version | Purpose | Features |
|-------|---------|---------|----------|
| `axum` | `0.7` | HTTP server (router, handlers, JSON) | default |
| `reqwest` | `0.12` | HTTP client for `hmn` → `hmnd` | `default-features = false, features = ["json"]` (no TLS — localhost only) |
| `regex` | `1` | Regex mode for `/search/content` | default |

Already-in-tree:

| Crate | Pulled in by | Used here for |
|-------|--------------|---------------|
| `serde`, `serde_json` | step 1 | request/response serde |
| `tokio` | step 1 | async runtime, `spawn_blocking`, `TcpListener::bind` |
| `globset` | step 2 | filesystem-search glob matcher |
| `rusqlite`, `r2d2`, `r2d2_sqlite` | step 2 | search SQL |
| `chrono` | step 2 | already used for `mtime` and `last_indexed_at` formatting |
| `anyhow` | step 1 | error propagation |
| `tracing` | step 1 | request-path logs (one-line per request, info-level) |
| `tempfile` (dev) | step 2 | `tests/http.rs` and `tests/cli.rs` fixtures |

**Why not `tower` / `tower-http` directly**: axum 0.7 re-exports the
`Service`/`Layer` traits it needs; we add no middleware in v0. The
roadmap mentions `tower` and `tower-http` as candidates; the workplan
trims to the minimum. Bring them in additively when the first
middleware-shaped need lands (request-id, structured request logs,
compression).

**Why not `assert_cmd`**: `Command::new(env!("CARGO_BIN_EXE_hmn"))` is
the standard Cargo-blessed pattern for binary integration tests. Same
outcome, one fewer dep.

**Why `reqwest` without TLS features**: architecture overview pins
"binds to localhost only in v0." TLS would mean linking either OpenSSL
or rustls — both add substantial build time and binary size for
zero v0 benefit. A future v0+ remote-daemon mode is a config knob
away (`features = ["rustls-tls"]`), additive.

---

## Process dependencies

Step 4's retro left no playbook-level cleanups for step 5 to gate on.
Two open items in the playbook (in
[`notes/coordinator-playbook.md` § Open questions](../../coordinator-playbook.md))
will get fresh evidence from this step but do not need to be settled
before it starts:

1. **Coordinator context drift at higher task density.** Steps 1–4
   ran on 6–7 tasks each; step 5 has 8. Step 3's retro called this
   "the genuine stress test." If drift surfaces (forgotten
   forward notes, mis-scoped task prompts, stale scratchpad reads),
   note it in the step-5 retro for playbook follow-up.
2. **Forward-note volume scaling.** Step 4's retro shipped on three
   forward notes (4.2→4.4, 4.3→4.4, 4.4→4.5). Step 5 has more
   handoffs (5.1→5.2 schema-to-indexer; 5.3→5.4 query-to-handler;
   5.4→5.5 router-to-binary; 5.4→5.6 types-to-client; 5.5→5.7 wiring-to-test).
   The forward-note pattern's ceiling is unknown; if it starts to
   strain (notes go stale before consumption, or the next agent
   misses one), note it in the retro.

The orchestration shape this workplan assumes — coordinator drives
task agents through the rolling-context scratchpad, soft flags land
in `Forward note for Task M+1` paragraphs (or `Soft flag` blocks
addressed to the coordinator), idle-detection fires on per-task
completion — is the *current* shape and is expected to hold.

---

## End-of-round acknowledgement

Step 5 is the last step in this roadmap round; per
[`roadmap.md` § After step 5](./roadmap-1.md#after-step-5):
1. Tag the milestone in git.
2. Capture any ADRs that hardened during the build.
3. Write a short **end-of-round retrospective** into
   [`notes/project-planning-workflow-notes.md`](../../project-planning-workflow-notes.md) —
   on top of the regular per-step retro — answering: did the
   roadmap→workplan→build cadence work? What would we change for the
   next round (steps 6–8)?
4. Open a fresh roadmap doc for steps 6–8 (chunking + embedding,
   semantic search, MCP).

These are step-boundary actions, executed by the coordinator at the
end of step 5's build (per
[`coordinator-playbook.md` § Step boundary](../../coordinator-playbook.md)).
The workplan acknowledges them so the build does not surprise the
boundary; nothing to do here at workplan-write time.
