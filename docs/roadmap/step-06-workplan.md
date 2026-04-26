# Step 6 Workplan — Chunking and embedding

**Step**: 6 of 8 (round 2 of 2). First step of round 2 — see [`roadmap-2.md`](./roadmap-2.md) for the round and [`roadmap.md`](./roadmap.md) for round 1.

**Status**: workplan written 2026-04-26; awaiting review before build.

**Round-1 lessons carrying forward** (from [`notes/project-planning-workflow-notes.md`](../../notes/project-planning-workflow-notes.md) end-of-round retro):

- Risk grade is honest: step 6 is **high** (new external contract via the embedding service; new SQLite extension; immutable schema dimension).
- Five deferred decisions from the roadmap are pulled forward into this workplan-write phase rather than left to build-time. Three additional fall-out resolutions surfaced during task design (table naming, `chunk_index` column, `heading_path` storage shape) and are also resolved here.
- Self-review for prose accuracy ran after the first draft per the heuristic added at the round boundary; results in [§ Self-review for prose accuracy](#self-review-for-prose-accuracy).
- Soft-flag-to-coordinator includes the new *workplan-prose accuracy* shape (added pre-step-6; see playbook edits at the round boundary). Task agents should use it if they catch prose drift in this workplan against the resolved decisions.

---

## Goal recap

On each real change to a watched file, `hmnd` parses the changed file with `pulldown-cmark`, splits it into heading-aware chunks (per the [`markdown-chunking`](../../.claude/skills/markdown-chunking/SKILL.md) skill), embeds each chunk via HTTP to a local OpenAI-compatible embedding service (default: a local TEI sidecar at `http://127.0.0.1:8080/v1/embeddings`), and persists the chunk metadata to a `chunks` table and the vector to a sibling `chunks_vec` virtual table (per the [`sqlite-vec-extension`](../../.claude/skills/sqlite-vec-extension/SKILL.md) skill).

The vec0 dimension is baked at schema creation. Mismatch between `config.embedding.dimension` and the schema-baked value fails the daemon at startup with a structured error.

Embedding-service unavailability does not crash the daemon; the file's chunk + vec rows are skipped, the change is logged, and the daemon stays responsive to filesystem and content search queries.

The semantic search HTTP handler (`/search/semantic`) is **out of scope** — that ships in step 7. Step 6 produces the data step 7 will query.

---

## Deferred-decision resolutions

The five TBDs from [`roadmap-2.md`](./roadmap-2.md) § Step 6 are resolved below, plus three fall-out resolutions surfaced during task decomposition.

### 1. Embedding-service contract (timeout, retry, batch size, on-failure)

**Resolution**:

- **Timeout**: 30s for the embed request. Surfaced in config as `embedding.timeout_ms` (default `30000`).
- **Retry policy**: at most one retry on a transport-level failure (connection refused, connection reset, request timeout) or on HTTP `5xx`. Backoff before retry: 250ms. No retry on `4xx` — those are the daemon's bug, not the service's. Surfaced in config as `embedding.max_retries` (default `1`). Set to `0` to disable retries entirely.
- **Batch size**: `1` for v0 (one chunk per request). Surfaced in config as `embedding.batch_size` (default `1`). Why-1: TEI and Ollama both accept arrays via the OpenAI-compatible `input` field, but batching is an optimization, not a correctness concern. v0 ships the simpler shape; a future step can promote to batching if real-world chunk volume justifies the SQL/HTTP coordination cost.
- **On-failure behavior**: skip-and-log. After exhausting retries, the file's chunks are not written, an `ERROR` log is emitted with `path`, error class, and HTTP status (if any), and the indexer continues with the next event. The `files` row's `content_hash` is **not** updated when chunking fails (so the next scan or watcher event will retry). The `chunks` and `chunks_vec` rows for that file remain whatever they were before (stale or absent — both are acceptable v0 states; the spec calls out that semantic search may return empty for files that haven't been embedded yet).

**Why**: shipping criterion 4 in [`roadmap-2.md`](./roadmap-2.md#step-6--chunking-and-embedding) names skip-and-log as the load-bearing behavior. Skip is preferable to queueing because (a) v0 has no durable retry queue (the outbox is for `ChangeEvent`, not for failed embeddings), and (b) the source of truth is the vault — the next scan or any future watcher event for that file will naturally retry. Queueing adds a failure mode (queue full, queue drop policy, queue persistence) that doesn't carry its weight at v0.

**How to apply**: the embedding client returns a typed error on failure. The indexer's per-file pipeline catches that error class specifically (HTTP / transport), logs it, leaves the `files` row untouched (no `content_hash` advance), and returns. Other error classes (chunking panic, SQL error) propagate normally as they would in step 2's scanner.

**References**: [`docs/specs/semantic-search.md`](../specs/semantic-search.md) § Edge Cases — Embedding service unavailable (line 92–94) describes the *query-time* response (HTTP 503; that's step 7's concern). This step's resolution covers the *index-time* response.

### 2. Chunk size cap and overflow rule

**Resolution**: target `~500` tokens per chunk; hard cap `~800` tokens. v0 approximates tokens as `bytes / 4` per the [`markdown-chunking`](../../.claude/skills/markdown-chunking/SKILL.md) skill (§ Size targets). The cap is enforced as a byte threshold (`3200` bytes) to avoid per-chunk tokenizer setup; the threshold is checked at paragraph boundaries within the current chunk's heading scope. When the running chunk's byte length crosses the target threshold (`2000` bytes), the chunker breaks at the next paragraph end. If the cap (`3200` bytes) is reached without a paragraph end (e.g. one very long paragraph), break at the cap regardless — finishing the current chunk at the next event boundary that pulldown-cmark surfaces (paragraph end, list-item end, blockquote end, code-block end). Code blocks are preserved as a unit per the skill's "Tests to write" line ("Code blocks spanning many lines → preserved as a unit, not split mid-block") — they will not trigger a mid-block break.

The thresholds are constants in `src/chunk.rs`, not config knobs:

```rust
const CHUNK_TARGET_BYTES: usize = 2000;
const CHUNK_HARD_CAP_BYTES: usize = 3200;
```

**Why**: matches the skill's documented defaults for `nomic-embed-text-v1.5` (which has an 8192-token context but degrades on very short and very long inputs). Constants rather than config knobs because (a) the threshold pair is a pair — they need to move together, not independently — and (b) v0 should not invite per-vault tuning before we have a corpus to tune against. A future step can promote to config when there's evidence one default doesn't fit.

**How to apply**: the chunker holds a running `usize` byte-length counter for the current chunk. After each `Event::End(Tag::Paragraph)`, the counter is checked against `CHUNK_TARGET_BYTES`. If exceeded, the current chunk closes and a new chunk starts on the next event. If the counter exceeds `CHUNK_HARD_CAP_BYTES` mid-paragraph, the next event-boundary that closes any block-level container forces a chunk break.

**References**: [`markdown-chunking`](../../.claude/skills/markdown-chunking/SKILL.md) § Size targets and § Tests to write.

### 3. Frontmatter handling on chunks

**Resolution**: frontmatter is split off the top of the file before pulldown-cmark parses, per the skill's `split_frontmatter` helper. The frontmatter content is **not** stored as a chunk and **not** parsed for fields in step 6. The body (content after the frontmatter) is what the chunker walks. If the file is frontmatter-only (no body), zero chunks are produced.

**Why**: frontmatter is structural metadata (tags, title, dates) — useful for filtering, not for semantic search over prose. Chunking it would inject low-signal vectors that compete with body chunks. Field extraction (e.g. surfacing `tags` as a separate index column) is a follow-on feature — out of scope here. The minimum viable behavior to satisfy shipping criteria 1–4 is: split frontmatter off, never see it again.

**How to apply**: `src/chunk.rs::chunk_file()` calls `split_frontmatter()` first, threads only the body into the event walker. The returned `Chunk` records have `start_byte` / `end_byte` offsets into the **original file** (not the body slice) so step 7's "jump to this location" projection lands in the right place when the file has frontmatter.

**References**: [`markdown-chunking`](../../.claude/skills/markdown-chunking/SKILL.md) § Frontmatter first (lines 14–28) and § Tests to write line "Frontmatter only, no body → zero chunks."

### 4. Dimension lock-in mechanism

**Resolution**: the `chunks_vec` virtual table's `embedding FLOAT[<dim>]` clause is the source of truth for the schema-baked dimension. At startup, after `apply_migrations()` returns, the daemon probes:

```sql
SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'chunks_vec'
```

…and parses the integer out of `embedding FLOAT[<dim>]` in the returned CREATE statement. This integer is compared against `config.embedding.dimension`. On mismatch, the daemon exits with an `anyhow::Error` whose message names both values and the resolution (re-index from scratch by deleting the database file, or change the config). On match, startup proceeds.

The probe lives in `src/store/mod.rs::Store::open()` after `apply_migrations()` (or in a dedicated `validate_schema()` helper called from `Store::open()`).

**Why**: `PRAGMA` introspection is the simplest and most direct route — no probe rows polluting the vector table, no parsing fragility around the vec0 binary format. The `sqlite_master.sql` text is stable across SQLite versions for `CREATE VIRTUAL TABLE`. The parser is a small regex (`embedding FLOAT\[(\d+)\]`); the regex crate is already in tree from step 5.

**How to apply**: implement `Store::validate_dimension(expected: u32) -> Result<()>`. Called once during `Store::open()`. Never re-checked after startup — the schema is immutable per ADR-0007.

**References**: [`docs/decisions/0007-sqlite-vec-over-alternatives.md`](../decisions/0007-sqlite-vec-over-alternatives.md) lines 27–28 (dimension is schema-level commitment); [`docs/specs/semantic-search.md`](../specs/semantic-search.md) § Model dimension mismatch (line 96–98) (fail loudly at startup); [`sqlite-vec-extension`](../../.claude/skills/sqlite-vec-extension/SKILL.md) § Smells last bullet ("Mismatched embedding dimensions between config and schema — fail loudly at startup if these disagree").

### 5. sqlite-vec extension binary location

**Resolution**: a config knob `embedding.extension_path` (type `ConfigPath`, default expands per platform) plus an env-var override `HYPOMNEMA_VEC_EXT_PATH` that, if set, takes precedence. The default path is `~/.local/share/hypomnema/sqlite-vec.<ext>` where `<ext>` is `dylib` on macOS, `so` on Linux, `dll` on Windows. The path is read at pool-init time inside `with_init`; the extension is loaded once per connection. If the file is missing at startup, the daemon exits with a structured error naming the config path and the env-var override mechanism.

**Why**: shipping pattern parallels existing config (`mcp.socket` uses the same `~/.local/share/hypomnema/...` shape); the env-var override is the smallest cost mechanism for development workflows where the binary lives in `target/debug/` rather than installed. Hardcoding violates the skill smell ("Hardcoded extension path in source code"). Bundling the extension in the Cargo build via `sqlite-vec-loadable` or similar is out of scope for v0 — it adds build-system complexity that doesn't carry weight until release packaging is itself a concern.

**How to apply**: extend `EmbeddingConfig` in `src/config.rs` with `extension_path: ConfigPath` (default via `default_embedding_extension_path()`). At pool build time (`build_pool()` in `src/store/pool.rs`), pass the resolved path into `with_init`'s closure; resolve env-var override before passing. The resolution helper lives next to the other path-resolution helpers in `src/config.rs`.

**References**: [`sqlite-vec-extension`](../../.claude/skills/sqlite-vec-extension/SKILL.md) § Loading the extension (lines 14–29); [`src/config.rs`](../../src/config.rs) `ConfigPath` and `expand_tilde` precedent (used for `mcp.socket`).

### Resolved as part of this step (not pre-flagged in the roadmap)

#### A. Vector table named `chunks_vec` (not `chunk_vectors`)

**Resolution**: the vec0 virtual table is named `chunks_vec` everywhere in code and SQL. The metadata table is `chunks`.

**Why**: roadmap-2 line 22 and [`docs/specs/semantic-search.md`](../specs/semantic-search.md) line 28 both use `chunks_vec`. The [`sqlite-vec-extension`](../../.claude/skills/sqlite-vec-extension/SKILL.md) skill currently uses `chunk_vectors` (line 49) — this is the outlier. Per the LDS authority order (spec > skill), the spec wins and the skill is the one that should be re-aligned later (a small skill-author edit, not blocking step 6). The workplan body uses `chunks_vec` consistently throughout.

**How to apply**: migration 0003 creates `chunks_vec`. Insert/delete patterns in `src/indexer/mod.rs` reference `chunks_vec`. The semantic search query in step 7 (`SELECT ... FROM chunks_vec v JOIN chunks c ...`) inherits this name.

#### B. `chunk_index` column on the `chunks` table

**Resolution**: the `chunks` table includes a column `chunk_index INTEGER NOT NULL` — the 0-based ordinal of the chunk within its file (i.e. the first chunk emitted by the chunker is `chunk_index = 0`, the second is `1`, etc.). The chunker assigns this during emission; the indexer writes it into the row. There is a `UNIQUE (file_path, chunk_index)` constraint so the (file, ordinal) tuple is the natural deduplication key.

**Why**: [`docs/specs/semantic-search.md`](../specs/semantic-search.md) lines 39 and 77 expose `chunk_index` as a public response field. Computing it at query time (`ROW_NUMBER() OVER (PARTITION BY file_path ORDER BY start_byte)`) is brittle (relies on `start_byte` ordering for ties; expensive on large vaults). Storing it as a column is `O(N)` extra space per chunk for `O(1)` query-time projection — clearly the right side of the trade-off.

**How to apply**: chunker emits `Vec<Chunk>` where each `Chunk` has `chunk_index: u32` set as it iterates. Indexer transaction inserts the column directly. Step 7's `/search/semantic` handler projects this column straight to the response.

#### C. `heading_path` stored as slash-separated TEXT

**Resolution**: the `chunks.heading_path` column is `TEXT NOT NULL` storing the slash-separated heading breadcrumb (e.g. `"Architecture/Load-bearing rules"`), per the [`markdown-chunking`](../../.claude/skills/markdown-chunking/SKILL.md) skill (line 46). Empty segments (the H3-without-H2 edge case) are encoded as empty strings between slashes (e.g. `"Setup//Prereqs"`). The HTTP and MCP layers in step 7 split this string on `/` to produce the spec's array shape (line 40: `["Architecture", "Containers"]`).

**Why**: TEXT is cheaper than a JSON-encoded array column (no parser per row at query time; no JSON1 dependency on the read path). Slash-separated round-trips losslessly to the array form when paired with explicit empty-segment handling. The skill's "decide once and be consistent" guidance (line 64) applies — pinned here.

**How to apply**: chunker computes the slash-joined string from its heading-stack vector. Indexer writes the string. Step 7 projects with `path.split('/')` (handling the leading-empty-segment case explicitly).

---

## Tasks (ordered, each independently mergeable)

Seven tasks. Each landing as its own commit per the round-1 convention (one task = one commit; per-commit results comment includes the SHA).

### Task 6.1 — Migration 0003 + sqlite-vec extension load in pool

**Files**:
- `Cargo.toml` (extend) — add `sqlite-vec` (or its bundled-loading shim, see § Net new dependencies for the exact crate selection); add `bytemuck = "1"` in dependencies if not already present (used by 6.4 for the `f32 → u8` cast).
- `src/config.rs` (extend) — add `extension_path: ConfigPath` to `EmbeddingConfig` with default `default_embedding_extension_path()` resolving to `~/.local/share/hypomnema/sqlite-vec.<platform-ext>`.
- `src/store/pool.rs` (extend) — `build_pool()` takes an additional `extension_path: &Path` parameter; the `with_init` closure loads the extension via `unsafe { conn.load_extension_enable()?; conn.load_extension(path, None)?; conn.load_extension_disable()?; }` per the [`sqlite-vec-extension`](../../.claude/skills/sqlite-vec-extension/SKILL.md) skill (lines 16–27). The `load_extension` rusqlite feature gets enabled in `Cargo.toml`'s `rusqlite` features list.
- `src/store/schema.rs` (extend) — add migration 0003 to the `MIGRATIONS` array. Migration 0003 SQL:
  ```sql
  CREATE TABLE chunks (
      id            INTEGER PRIMARY KEY,
      file_path     TEXT    NOT NULL,
      chunk_index   INTEGER NOT NULL,
      heading_path  TEXT    NOT NULL,
      content       TEXT    NOT NULL,
      content_hash  TEXT    NOT NULL,
      start_byte    INTEGER NOT NULL,
      end_byte      INTEGER NOT NULL,
      created_at    TEXT    NOT NULL,
      UNIQUE (file_path, chunk_index)
  ) STRICT;
  CREATE INDEX idx_chunks_file_path ON chunks(file_path);
  CREATE VIRTUAL TABLE chunks_vec USING vec0(
      chunk_id INTEGER PRIMARY KEY,
      embedding FLOAT[768]
  );
  ```
  The `768` is the schema-baked dimension per resolution 4. Note: `STRICT` only applies to regular tables; vec0 virtual tables don't accept it.
- `src/store/mod.rs` (extend) — `Store::open()` accepts an `extension_path: &Path` (or threads through `EmbeddingConfig`); after `apply_migrations()` returns, calls `validate_dimension(expected_dim: u32)` which probes `sqlite_master.sql` and parses the dimension out of the CREATE statement. `validate_dimension` is also exported as a `pub(crate)` helper for tests.
- `src/lib.rs` (touch) — re-export any new types if `Store::open()` signature changes.

**What lands**:
- Pool init loads the sqlite-vec extension on every connection (pool size is `8` per existing constant in `src/store/pool.rs:9`).
- Database file at v3 includes `chunks` (regular STRICT table with the column shape above) and `chunks_vec` (vec0 virtual table, dimension 768).
- New unit tests in `src/store/schema.rs::tests`:
  - `migrations_advance_user_version_to_3` — adapts the existing `migrations_advance_user_version_to_2` test (line 178 of current schema.rs).
  - `migration_0003_creates_chunks_table` — `PRAGMA table_info(chunks)` shape assertion.
  - `migration_0003_creates_chunks_vec` — `sqlite_master.sql` contains `chunks_vec USING vec0`.
  - `migration_0003_chunks_vec_dimension_is_768` — probe the dimension via the validate helper, assert 768.
  - `chunks_unique_constraint_on_file_path_chunk_index` — INSERT two rows with the same `(file_path, chunk_index)`, expect the second to fail with a UNIQUE violation.
- New tests in `src/store/mod.rs` (or a new `validate.rs` if cleaner):
  - `validate_dimension_matches` — happy path, no error.
  - `validate_dimension_mismatch_errors_with_path_and_values` — config says `512`, schema is `768`, error message mentions both numbers and the path-to-resolution.

**Why a separate task**: schema lock-in deserves its own commit, tests, and bisect anchor. The extension-load and the migration are coupled (the migration uses vec0 syntax that requires the extension already loaded on the connection running `apply_migrations`); they ship together. The dimension validation is small enough to ride along — splitting it out would create a one-commit task.

**Risk: high.**
- *Why high*: the schema is immutable for the life of the database file (per ADR-0007). Wrong column names, wrong types, wrong index choices, or a mistake in the vec0 syntax mean every existing user's database would need to be deleted and re-indexed when the fix lands. Test coverage of every column in the new table is the load-bearing safety net.
- *Mitigation*: migration tests assert each column is present and correctly typed (`PRAGMA table_info`); a separate test asserts the UNIQUE constraint behaves; the dimension probe round-trips through schema.

### Task 6.2 — Chunking module (`src/chunk.rs`, new)

**Files**:
- `Cargo.toml` (extend) — add `pulldown-cmark = "0.10"` (or whatever the current major) under `[dependencies]`.
- `src/chunk.rs` (new) — implements:
  - `pub fn split_frontmatter(content: &str) -> (Option<&str>, &str)` per the skill (lines 18–28).
  - `pub struct Chunk { pub chunk_index: u32, pub heading_path: String, pub content: String, pub content_hash: String, pub start_byte: usize, pub end_byte: usize }` — the in-memory chunk representation. `content_hash` is the SHA-256 of `content` (chunk text only, not the whole file) per the skill (line 49).
  - `pub fn chunk_file(file_content: &str) -> Vec<Chunk>` — splits frontmatter, walks pulldown-cmark events on the body, returns chunks with the resolution-2 size targets and resolution-3 frontmatter behavior.
  - Internal helpers: `HeadingStack` (Vec<String> tracking H1..H6 per the skill's edge-case rule for orphan H3 — pin behavior in a doc comment), `Chunker` (the event-walk state machine).
- `src/lib.rs` (touch) — `pub mod chunk;` and re-export `Chunk` if used by `indexer`.

**What lands**:
- Pure-logic chunking module. Does **not** call `spawn_blocking` — chunking is CPU over strings and runs on the async runtime per the [`markdown-chunking`](../../.claude/skills/markdown-chunking/SKILL.md) skill smell line 89 ("Chunking inside `spawn_blocking`").
- Comprehensive unit-test coverage of the nine cases enumerated in the skill (§ Tests to write, lines 70–80). Each test gets a self-contained Markdown literal and asserts the resulting `Vec<Chunk>` shape. Also:
  - `chunk_index_is_zero_based_and_contiguous` — emit several chunks, assert `chunk_index == 0, 1, 2, ...`.
  - `byte_offsets_account_for_frontmatter` — frontmatter present; first body chunk's `start_byte` reflects the original file offset, not the body-slice offset.
  - `orphan_h3_uses_documented_behavior` — the documented edge case behavior (pin in code: H3 fills H2 slot with empty string; `heading_path` becomes e.g. `"Setup//Prereqs"`).
  - `code_block_not_split_mid_block` — a code block long enough to cross `CHUNK_TARGET_BYTES`; assert it stays whole.

**Why a separate task**: the chunker is the load-bearing input shape for everything downstream. Splitting from the embedding/store path keeps the unit-test surface tight and the bisect window clean.

**Risk: medium.**
- *Why medium*: pulldown-cmark's event sequencing has documented quirks (CommonMark edge cases, table extension, fenced vs. indented code blocks). The skill's smells call out regex-based chunking and blank-line splitting; we're avoiding both, but event handling has its own foot-guns. The nine skill test cases plus the four task-specific ones (above) cover the concrete edge cases the skill has identified.
- *Mitigation*: every behavior decision (target/cap thresholds, orphan-H3 stack-fill, frontmatter handling) has a test; the `Chunker` state machine is simple enough that any real bug surfaces in one of those tests.

### Task 6.3 — Embedding client (`src/embedding.rs`, new)

**Files**:
- `src/embedding.rs` (new) — implements:
  - `pub struct EmbeddingClient { http: reqwest::Client, endpoint: String, model: String, api_key: String, timeout: Duration, max_retries: u8, batch_size: u8 }` — built once per daemon start; cheap to clone (`reqwest::Client` is `Arc`-internal).
  - `pub fn new(cfg: &EmbeddingConfig) -> Result<Self>` — builds with `reqwest::Client::builder().timeout(...).build()`.
  - `pub async fn embed(&self, text: &str) -> Result<Vec<f32>, EmbeddingError>` — sends `{"model": ..., "input": [text]}` to `endpoint`, parses `{"data": [{"embedding": [...]}]}`, returns the float vector. Honors the resolved retry policy (one retry on transport / 5xx with 250ms backoff). Note: in v0 only the single-input shape is exercised since `batch_size = 1`; `embed_batch()` can land in a future step if/when `batch_size > 1` matters.
  - `pub enum EmbeddingError { Transport(reqwest::Error), Status { code: u16, body: String }, BodyParse(serde_json::Error), DimensionMismatch { expected: u32, actual: u32 } }` — typed so the indexer can distinguish "service unavailable, skip-and-log" from "JSON parse failure, bug in our code or service".
- `src/lib.rs` (touch) — `pub mod embedding;` and re-export `EmbeddingClient` if `hmnd` constructs it.

**What lands**:
- Async client, no `spawn_blocking` (HTTP calls belong on the runtime per the [`sqlite-vec-extension`](../../.claude/skills/sqlite-vec-extension/SKILL.md) skill smell line 111).
- Optional `Authorization: Bearer <api_key>` header when `api_key` is non-empty.
- Unit tests using `TcpListener::bind("127.0.0.1:0")` to stub the embedding service (pattern from `tests/http.rs` and `tests/cli.rs`):
  - `embed_returns_vector_for_200` — stub returns a fixed-shape JSON body; client returns the `Vec<f32>`.
  - `embed_retries_once_on_503` — stub returns 503 then 200; client returns the vector after one retry.
  - `embed_retries_once_on_connection_refused` — stub closes after accept; client retries and (per the test) the second attempt also fails → returns `Transport`.
  - `embed_does_not_retry_on_4xx` — stub returns 400; client returns `Status { code: 400, ... }` after one attempt.
  - `embed_dimension_mismatch_classified` — stub returns a vector of wrong length; client returns `DimensionMismatch`. (The client knows the expected dimension because it's threaded through from config.)
  - `embed_honors_timeout` — stub accepts and never responds; client returns `Transport` (a `reqwest::Error::is_timeout()`-classified one) after the timeout window.

**Why a separate task**: separates network-shape concerns (timeout / retry / status / parse) from indexer-coordination concerns (when to call, how to handle the error). Step 5's `src/client.rs` (the CLI's daemon client) is intentionally **not** extended here — it's the wrong direction (CLI → daemon), and the embedding client lives entirely inside the daemon process.

**Risk: medium.**
- *Why medium*: retry/timeout interaction with `reqwest::Client` is the most error-prone surface. The unit tests stub timing (a 503-then-200 stub, a slow stub for timeout testing) and the typed error enum forces explicit classification at the call site.
- *Mitigation*: tests cover each branch of the retry/error matrix; the typed error enum is the contract the indexer in 6.4 reads.

### Task 6.4 — Indexer integration: chunk → embed → write transaction

**Files**:
- `src/indexer/mod.rs` (extend) — `Scanner` gains an `EmbeddingClient` field. The per-file pipeline (`reindex_path()` and the bulk-scan loop) extends to: read file → split frontmatter → chunk body → embed each chunk async → spawn_blocking the SQL write transaction (delete-and-reinsert). On embedding failure (the `EmbeddingError::Transport` and `Status { code: 5xx }` classes), log and skip; do not advance `files.content_hash` for that file.
- `src/indexer/hash.rs` (touch) — no changes expected; chunk-hash computation lives in `src/chunk.rs`.
- `src/store/` (touch — likely `src/store/chunks.rs` new, or extend an existing module) — `pub fn rewrite_chunks_for_file(conn, file_path, chunks_with_embeddings)` doing delete-then-insert in one transaction per the [`sqlite-vec-extension`](../../.claude/skills/sqlite-vec-extension/SKILL.md) skill (lines 78–88). Uses `bytemuck::cast_slice::<f32, u8>` for the vector blob (skill line 72).
- New tests in `src/indexer/mod.rs::tests`:
  - `reindex_writes_chunks_for_simple_file` — using a real Store (in-memory or temp file) and a stub embedding client, assert `chunks` rows materialized.
  - `reindex_replaces_chunks_for_modified_file` — first reindex writes 2 chunks; second reindex (after content change) writes 3 chunks; assert old 2 are gone.
  - `reindex_skips_on_embedding_transport_error` — stub client returns `Transport`; assert no `chunks` rows written, no `files.content_hash` advance, error logged at `ERROR` level (use the `tracing-test` pattern from earlier steps if available, else the test passes when the row inspection confirms skip-and-log behavior).
  - `reindex_zero_chunks_for_frontmatter_only_file` — frontmatter-only fixture; assert zero `chunks` rows written; `files.content_hash` advances normally (the file is "indexed", just empty).

**What lands**:
- The async embedding call sits between the chunk computation (sync) and the SQL write (`spawn_blocking`). The pipeline shape:
  ```rust
  // pseudocode
  let body = read_file(...).await?;          // tokio fs
  let chunks = chunk_file(&body);            // sync, runs on runtime
  let embeddings = futures::stream::iter(&chunks)
      .then(|c| client.embed(&c.content))
      .try_collect::<Vec<_>>()
      .await
      .map_err(...)?;                        // skip-and-log on transport/5xx
  let chunks_with_vecs = chunks.into_iter().zip(embeddings).collect();
  spawn_blocking(move || {
      store.rewrite_chunks_for_file(&path, &chunks_with_vecs)?;
      // and update files.content_hash + files row in same tx
  }).await??;
  ```
- The `files` row update and the `chunks` rewrite should land in the **same** SQL transaction so a crash mid-way leaves the database consistent (either both old, or both new).

**Why a separate task**: composes 6.1 (storage), 6.2 (chunking), and 6.3 (embedding) at the indexer boundary. The async/blocking dance is the foot-gun the [`rusqlite-in-async`](../../.claude/skills/rusqlite-in-async/SKILL.md) skill names — embedding goes on the async runtime, SQL goes in `spawn_blocking`, chunking stays on the runtime. Splitting from the prior tasks keeps each commit focused on one concern.

**Risk: high.**
- *Why high*: this is where step 6's foot-guns concentrate. The async/blocking boundary is well-described by [`rusqlite-in-async`](../../.claude/skills/rusqlite-in-async/SKILL.md) but easy to get wrong (calling `spawn_blocking` from inside a blocking context, or holding a blocking SQL handle across an `.await`). The transactional semantics — `files` update + `chunks` rewrite in one tx — are the load-bearing crash-safety story.
- *Mitigation*: read the [`rusqlite-in-async`](../../.claude/skills/rusqlite-in-async/SKILL.md) skill at the start of the task. Apply the existing pattern from `src/store/mod.rs::Store::open()` and `Scanner::reindex_path()` (step 2/3/4 precedent). Tests cover the skip-and-log path explicitly — the most common foot-gun (silently advancing content_hash on embedding failure) is gated by an explicit assertion.

### Task 6.5 — Wire into `hmnd` (extension load, dimension validation, embedding-service health probe)

**Files**:
- `src/bin/hmnd.rs` (extend) — startup ordering becomes:
  1. Parse config.
  2. `Store::open(&cfg.daemon.data_dir, &cfg.daemon.index_file, &cfg.embedding)` — extended signature threads the embedding config through. Internally: `build_pool` loads the extension via `with_init` (extension-binary-missing errors surface here); `apply_migrations()` runs against a connection that already has the extension loaded (so the vec0 syntax in migration 0003 works); `validate_dimension(cfg.embedding.dimension)` runs after migrations and exits with a structured error on mismatch.
  3. Build `EmbeddingClient::new(&cfg.embedding)`.
  4. Health probe: `embed_health_probe(&client)` — sends a one-token test request (e.g. `embed("ping")`) at startup. **Does not** fail the daemon if the probe returns `Transport` or `Status { code: 5xx }` — the round-trip would let downstream watcher events skip-and-log naturally; the probe's purpose is to log a one-liner (`INFO`: "embedding service reachable, vector length matches dimension" or `WARN`: "embedding service not reachable at startup; chunking will skip-and-log per file").
  5. Build `Scanner` with the embedding client; spawn watcher consumer; spawn HTTP server.
  6. Wait for shutdown signal.

  Note: the existing `Store::open(data_dir: &Path, index_file: &str)` signature (`src/store/mod.rs:26`) gains parameters per Task 6.1; the rest of the call-sites (currently just `hmnd` and tests) pass them through. Whether the new shape takes `&EmbeddingConfig` or three separate parameters is a Task 6.1 implementation choice; the workplan does not pin it.
- `src/api/health.rs` (touch, optional) — extend `HealthResponse` with a field `embedding_reachable: Option<bool>` if it cleanly fits the existing shape from step 5 (this is a soft choice — agents may reasonably defer this to step 7 with a forward note; the test surface in 6.6 does not depend on it).
- Manual smoke verification: per round-1's step-5 task 5.5 precedent, the agent runs the daemon against a tempdir vault and confirms with `curl` (filesystem search still works; `/status` still works; logs show the embedding probe result; deleting and re-creating a Markdown file produces `chunks` rows).

**What lands**:
- Daemon starts cleanly when the extension is present and the embedding service is reachable.
- Daemon starts cleanly (with a `WARN` log) when the embedding service is unreachable; non-semantic features (filesystem and content search) continue working.
- Daemon **fails to start** with a structured error in two specific cases: extension binary missing (resolution 5), schema dimension ≠ config dimension (resolution 4).
- Existing step-5 tests keep passing (no regression on filesystem/content search).

**Why a separate task**: composes all prior step-6 tasks behind the daemon's startup contract. Manual smoke verification mirrors the round-1 step-5 task 5.5 precedent — a single end-to-end pass against a real filesystem catches the kinds of bugs unit tests miss (path resolution at runtime, signal handling, log shape).

**Risk: medium-high.**
- *Why medium-high*: startup ordering is sensitive — extension load must happen on every connection (in `with_init`), but the dimension validation must happen *after* migrations have run on a connection that already has the extension loaded. If the order is wrong the daemon fails to start in confusing ways (e.g. "no such function: vec0" if a migration tries to use vec0 before extension load).
- *Mitigation*: the `with_init` pattern handles this naturally — every checkout from the pool already has the extension loaded, and the migration runner uses a checked-out connection. Tests in 6.1 cover this implicitly (migrations succeed → extension was loaded on the connection that ran them). Manual smoke is the final gate.

### Task 6.6 — Integration tests against live daemon + stub embedding service

**Files**:
- `tests/embedding.rs` (new) — integration tests that spin up:
  - A tempdir vault with seed `.md` files.
  - A stub embedding service via `TcpListener::bind("127.0.0.1:0")` returning a deterministic 768-dim vector for any input. Stub lifetime is the test scope.
  - A live `hmnd`-style stack (Store + Scanner + watcher) wired against the stub. Pattern: copy from `tests/http.rs` how the live daemon is composed.

  Cases:
  - `editing_a_watched_file_writes_chunks_to_db` — create file, wait for watcher debounce + indexer cycle, query `SELECT count(*) FROM chunks WHERE file_path = ?`, assert nonzero.
  - `chunk_count_matches_chunker_for_known_fixture` — write a fixture with 3 H2-separated sections; assert exactly 3 `chunks` rows; cross-check by running `chunk_file(...)` directly on the fixture and asserting equality.
  - `chunks_vec_row_per_chunks_row` — assert `(SELECT count(*) FROM chunks_vec) == (SELECT count(*) FROM chunks)`.
  - `embedding_service_unavailable_skips_file_and_keeps_daemon_responsive` — stub closes after accept (or returns 503 always); the watched file's chunks rows are absent; the daemon's HTTP server still responds to `/health` (regression check from step 5).
  - `editing_existing_file_replaces_chunks` — create file with 2 chunks, edit to have 3 chunks, assert old 2 chunks are gone and the 3 new chunks have fresh `created_at` timestamps.
  - `dimension_mismatch_at_startup_fails_loudly` — open a Store with `expected_dim = 512` against a fresh database; assert `Store::open()` returns `Err` whose `Display` mentions both `512` and `768` and the path-to-resolution; this is unit-level but lands here for orchestration consistency.
- 3× consecutive flake-check budget (per round-1 step-3 task 3.5 and step-5 task 5.7 precedent — run `cargo test --test embedding` three times locally without flakes before reporting green).

**What lands**:
- Six new integration tests, all using the existing `TcpListener::bind` stub-service pattern from steps 5.7 (cli.rs) and 5.7 (http.rs).
- The tests live in `tests/embedding.rs` rather than `tests/chunk.rs` because they exercise the **wired** path (chunker + client + indexer + store), not the chunker in isolation. Pure-chunker tests live in `src/chunk.rs::tests`.

**Why a separate task**: integration test surface deserves its own commit and bisect anchor; cross-references between the stub-service helper and the test fixtures are easier to audit when isolated.

**Risk: medium.**
- *Why medium*: stub-service timing introduces flake potential (the stub's `TcpListener::accept()` loop racing with `reqwest`'s connection pool); the 3× flake budget is the safety net.
- *Mitigation*: copy the stub-service helper from `tests/cli.rs` rather than reinventing; document the timing trade-offs the round-3 step-3 retro identified ("Do not introduce a polling-loop helper that hides the timing — flakes on a non-deterministic boundary are signal").

### Task 6.7 — Reference docs reflect step-6 resolutions

**Files**:
- `docs/reference/configuration.md` (extend) — add `embedding.extension_path`, `embedding.timeout_ms`, `embedding.max_retries`, `embedding.batch_size`. Document the env-var override `HYPOMNEMA_VEC_EXT_PATH`. Update the embedding section example TOML.
- `docs/reference/cli.md` (touch, if any) — no changes expected; `hmnd` flags don't change in step 6.
- `docs/architecture/overview.md` (touch) — extend the indexing flow diagram or prose to include the chunk → embed → store path. Cross-reference the [`markdown-chunking`](../../.claude/skills/markdown-chunking/SKILL.md) and [`sqlite-vec-extension`](../../.claude/skills/sqlite-vec-extension/SKILL.md) skills.
- `docs/specs/semantic-search.md` (touch) — flip the `chunk_id` line (line 37) from "(primary key)" to specifically reference the `chunks.id` column; add a one-line note that semantic search proper ships in step 7 but the data substrate it reads from ships in step 6.

**What lands**:
- Documentation is consistent with what step 6 actually built. The configuration reference enumerates every new knob with its default and a brief description.

**Why a separate task**: doc-only by design; lands at the boundary so any soft-flag-to-coordinator from earlier tasks (e.g. wording corrections) can be incorporated.

**Risk: low.**

---

## Test strategy

**Unit tests** (`#[cfg(test)]`):
- `src/chunk.rs::tests` — nine cases from the [`markdown-chunking`](../../.claude/skills/markdown-chunking/SKILL.md) skill plus the four task-specific cases enumerated in Task 6.2. Pure logic; no async; no I/O.
- `src/embedding.rs::tests` — six cases from Task 6.3 using `TcpListener::bind` stub. Async (`#[tokio::test]`); no SQL.
- `src/store/schema.rs::tests` — five new cases for migration 0003 plus the dimension-validation case from Task 6.1. Synchronous SQLite; in-memory connections.
- `src/indexer/mod.rs::tests` — four new cases for the chunk-embed-store pipeline from Task 6.4. Uses an in-memory or temp Store and a stub `EmbeddingClient`.

**Integration tests** (`tests/`):
- `tests/embedding.rs` — six cases from Task 6.6. Spins up a live Store + Scanner + watcher + stub embedding service per test. 3× consecutive flake-check budget per the round-1 anti-flake rule.

**Lint and format**:
- `cargo clippy --all-targets -- -D warnings` clean.
- `cargo fmt --check` clean.

**Cross-platform notes**:
- The sqlite-vec extension binary is platform-specific (`.dylib` / `.so` / `.dll`). Tests that load the extension must use a path resolved at test-fixture-setup time — likely an env-var pointing at the developer's local `sqlite-vec.dylib`. CI will need the extension binary in a known path; that's a CI concern (not blocking step 6 — the agent should flag if CI integration becomes load-bearing).
- All other tests are platform-neutral. macOS + Linux CI matrices remain the primary targets per round-1 precedent; Windows is out of v0.

**Anti-flake rules** (carried forward from round 1):
- Do **not** introduce a polling-loop helper that hides timing in `tests/embedding.rs` — flakes on a non-deterministic boundary are signal, per the step-3 retro.
- The stub service's `TcpListener::accept()` loop runs on a `tokio::spawn`; the test's `Drop` shuts it down explicitly. No global state between tests.

---

## Definition of done

- [ ] Migration 0003 lands; `chunks` and `chunks_vec` exist in the schema; `user_version` advances to `3` (criterion 1, 3).
- [ ] `chunks_vec` dimension is `768`; mismatch with config fails the daemon at startup with both numbers and resolution path in the error message (criterion 5).
- [ ] Editing a watched `.md` file results in fresh `chunks` rows for that file (criterion 1); the chunk count matches what `chunk_file()` emits in unit tests (criterion 2); a `chunks_vec` row exists for every `chunks` row at dimension 768 (criterion 3).
- [ ] Embedding-service unavailability does not crash the daemon; the affected file's `chunks` rows are skipped (not partially written), and `/health` plus `/search/filesystem` and `/search/content` continue to respond (criterion 4).
- [ ] All step-5 tests still pass (no regression on filesystem/content search or HTTP plumbing).
- [ ] All new unit and integration tests pass; `cargo clippy --all-targets -- -D warnings` clean; `cargo fmt --check` clean.
- [ ] Manual smoke verification per Task 6.5 documented in the task's results comment.
- [ ] `docs/reference/configuration.md` documents every new `embedding.*` knob with default and brief description, plus the `HYPOMNEMA_VEC_EXT_PATH` env-var override.
- [ ] Step 6 retrospective appended to [`notes/project-planning-workflow-notes.md`](../../notes/project-planning-workflow-notes.md) following the retro template.
- [ ] `docs/roadmap/roadmap-2.md` § Step 6 marked `**Status**: shipped <date>`.
- [ ] No fall-out resolutions or in-build TBDs left undocumented (workplan and code agree at the end; soft flags routed to coordinator at boundary).

---

## Cross-references

**Skills (load-bearing)**:
- [`.claude/skills/markdown-chunking/`](../../.claude/skills/markdown-chunking/SKILL.md) — Task 6.2's contract; cited at every `Event::*` decision in `src/chunk.rs` and at the size-target/cap thresholds.
- [`.claude/skills/sqlite-vec-extension/`](../../.claude/skills/sqlite-vec-extension/SKILL.md) — Tasks 6.1 and 6.4; cited at the extension load (`with_init`), the schema split (`chunks` vs. `chunks_vec`), the delete-and-reinsert pattern, and the `bytemuck::cast_slice` blob conversion. *Note*: skill currently calls the vec0 table `chunk_vectors` (line 49); workplan resolution A pins it to `chunks_vec`. The skill should be re-aligned in a follow-on edit.
- [`.claude/skills/rusqlite-in-async/`](../../.claude/skills/rusqlite-in-async/SKILL.md) — Task 6.4's load-bearing contract for the async/blocking boundary; cited at every `spawn_blocking` call site and at the embedding-call site (which deliberately stays on the async runtime).

**ADRs**:
- [`docs/decisions/0003-indexing-in-the-daemon.md`](../decisions/0003-indexing-in-the-daemon.md) — chunking and embedding live in the daemon, not delegated.
- [`docs/decisions/0005-local-everything.md`](../decisions/0005-local-everything.md) — embedding service is local (TEI / Ollama / vLLM); no cloud calls.
- [`docs/decisions/0007-sqlite-vec-over-alternatives.md`](../decisions/0007-sqlite-vec-over-alternatives.md) — sqlite-vec is the vector store; dimension is schema-level; delete-and-reinsert on change.

**Specs**:
- [`docs/specs/semantic-search.md`](../specs/semantic-search.md) — pinned the public chunk shape (`chunk_index`, `heading_path`, `text`, `score`, `file_path`, plus the v0-omitted `vault` forward-compat field). Step 6 produces the data shape this spec will consume in step 7. Edge-case § Embedding service unavailable describes the *query-time* behavior (not built here).

**Prior workplans / retros**:
- [`docs/roadmap/step-05-workplan.md`](./step-05-workplan.md) — closest-shape precedent (1685 lines, five deferred decisions, first-external-surface step). This workplan mirrors its section structure verbatim.
- [`notes/project-planning-workflow-notes.md`](../../notes/project-planning-workflow-notes.md) end-of-round retro — three round-1 lessons feeding into this round (risk grading honesty, pull-forward of deferred decisions, workplan-prose accuracy review heuristic).

**Roadmap and tech-stack**:
- [`docs/roadmap/roadmap-2.md`](./roadmap-2.md) § Step 6 — the contract this workplan resolves.
- [`docs/implementation/tech-stack.md`](../implementation/tech-stack.md) — naming the chunk + embed step as "the step most likely to surprise you" (round-2 framing); risk grade in this workplan is honest about that.

---

## Out of scope (will not appear in this PR)

- `/search/semantic` HTTP handler — that's step 7. Step 6 produces the data step 7 will query.
- `hmn search semantic <query>` CLI command — step 7.
- MCP wrapper for any search — step 8.
- Frontmatter field extraction (e.g. surfacing `tags` as a queryable index) — out of v0 scope; round-1 noted "frontmatter fields don't go on the files row currently" and step 6 preserves that.
- Re-index on dimension change — ADR-0007 names the path as "drop the database file and re-index from scratch"; step 6 fails loudly at startup but does not auto-rebuild.
- Embedding batching (`batch_size > 1`) — config knob exists; v0 default is 1; promotion happens when there's evidence batching matters.
- Bundled extension loading via a Cargo feature (`sqlite-vec-loadable` or similar) — release-packaging concern; out of v0.
- Outbox notifications for chunk-level changes — outbox carries file-level `ChangeEvent` per step 4; chunk-level granularity is a future feature if a consumer ever asks for it.
- Reranking, hybrid search, adjacent-chunk context — open questions in the spec (§ Open Questions); not v0.

---

## Net new dependencies

| Crate | Where | Why |
|---|---|---|
| `pulldown-cmark` (latest 0.x) | `[dependencies]` | Streaming Markdown event parser for Task 6.2. |
| `sqlite-vec` (or its loadable shim — see note) | `[dependencies]` *or* path-loaded at runtime | Vector storage extension for SQLite, loaded via `rusqlite::Connection::load_extension`. The exact crate name and integration shape (Cargo dep vs. runtime-loaded `.dylib`) is a Task 6.1 implementation choice — both ship a `vec0` virtual table; the choice is between bundling the extension binary (Cargo) or relying on a path config (runtime). Resolution 5 pins the runtime-load approach; the Cargo entry, if present, is for compile-time discovery only. The agent must verify which crate / artifact gets used in v0 against the upstream sqlite-vec project and confirm in the task's results comment. |
| `bytemuck` (1.x) | `[dependencies]` | Used by Task 6.4 to cast `&[f32]` to `&[u8]` without copy when writing the embedding blob into `chunks_vec`. Per the `sqlite-vec-extension` skill (line 72). Already-in-tree check: the agent should `cargo tree | grep bytemuck` in 6.1 — if it's already a transitive dep, no `Cargo.toml` change is needed; otherwise, add it. |
| `rusqlite` features | `[dependencies.rusqlite].features` | Add `load_extension` to the existing feature list. This is a feature flip on an existing dep, not a new dep. |

The `reqwest` dep is already in tree from step 5 (`src/client.rs`); Task 6.3 reuses it.

---

## Process dependencies

- Boundary cleanups landed pre-workplan (commits at the round-1/round-2 boundary): playbook edit (a) added the workplan-prose-accuracy soft-flag shape; playbook edit (b) retired the coordinator-context-drift open question; workflow-notes edit (c) added the self-review-for-prose-accuracy heuristic to Phase B. Step 6's build does **not** depend on any further playbook or workflow-notes edits.
- The skill text in [`.claude/skills/sqlite-vec-extension/SKILL.md`](../../.claude/skills/sqlite-vec-extension/SKILL.md) line 49 currently uses `chunk_vectors`; resolution A pins the table name to `chunks_vec` per the spec/roadmap. The skill should be re-aligned in a follow-on edit, but step 6's build does not block on it — the workplan body is the operative reference for the table name during the build phase.

---

## Self-review for prose accuracy

This workplan came in at ~460 lines, under the ~1000-line threshold of the new Phase B heuristic in [`notes/project-planning-workflow-notes.md`](../../notes/project-planning-workflow-notes.md). The heuristic didn't formally fire, but a voluntary accuracy spot-check was done anyway given the density of external-library claims (pulldown-cmark, sqlite-vec, rusqlite features, reqwest). Results below.

**Claims that were re-checked**:

- pulldown-cmark "streaming parser that emits events" — confirmed against the [`markdown-chunking`](../../.claude/skills/markdown-chunking/SKILL.md) skill (line 12) which is the load-bearing reference.
- pulldown-cmark "doesn't handle frontmatter natively" — confirmed against the skill (line 16); also matches widely-known behavior.
- sqlite-vec extension API "loaded into a standard SQLite connection at runtime" — confirmed against the [`sqlite-vec-extension`](../../.claude/skills/sqlite-vec-extension/SKILL.md) skill (line 8).
- `vec0` "virtual tables have limited column support" — confirmed against the skill (line 33).
- `MATCH` operator and `k` pseudo-column "are sqlite-vec idioms" with "exact syntax against upstream docs" — confirmed against the skill (lines 100–105). Step 7 will exercise this; step 6 only writes vectors.
- `bytemuck::cast_slice` "convert `&[f32]` to `&[u8]` without copying" — confirmed against the skill (line 59).
- `reqwest::Client` "is `Arc`-internal; cheap to clone" — confirmed against widespread reqwest documentation; matches the existing `src/client.rs` usage from step 5.
- `rusqlite::Connection::load_extension` "needs the `load_extension` feature enabled" — confirmed against the skill (line 14) and rusqlite's `Cargo.toml` feature surface.

**Workplan-internal consistency**:

- The dimension `768` appears in: roadmap-2 § Step 6, ADR-0007, the spec (line 27), the [`sqlite-vec-extension`](../../.claude/skills/sqlite-vec-extension/SKILL.md) skill (line 51), `default_embedding_dimension()` in `src/config.rs` (line 144), and migration 0003 SQL above. All consistent.
- The vec0 table name `chunks_vec` (resolution A): used consistently throughout the workplan body, the migration SQL, the cross-references, and the test cases. The skill's `chunk_vectors` is called out as the outlier in resolution A and § Cross-references.
- Configuration shape: every new knob (`extension_path`, `timeout_ms`, `max_retries`, `batch_size`) appears in resolutions 1 and 5, in Task 6.5, in the DoD, and in Task 6.7's doc updates. No knob mentioned without a default.

**One residual ambiguity flagged for the agent at task time**:
- The `sqlite-vec` Cargo crate's exact name and the runtime-load-vs-Cargo-bundle shape. Task 6.1 and § Net new dependencies both flag this as an implementation choice the agent verifies against the upstream sqlite-vec project. This is a *narrow* prose-accuracy escape hatch — the verification gate is in the task itself, not deferred to soft-flag.
