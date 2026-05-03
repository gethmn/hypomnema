# Step 21 — Health Endpoint and VCS-Aware Ignores — Workplan

**Round**: 10 (v0 Polish)
**Status**: Draft (workplan-ready handoff)
**Authored**: 2026-05-02
**Source**: `notes/roadmap/roadmap-10.md`; Solo scratchpad #17 "v0 Polish Round Scope" (tasks 1 and 2)

---

## Executive Summary

Step 21 ships two additive, read-only operational improvements in a single round-10 gate:

1. **Task 21.1 — Health Endpoint**: Fill the existing `GET /health` route slot in `src/api/health.rs` with a real probe handler that returns a structured status reflecting watcher state, database connectivity, and (when configured) embedding-service reachability. Pure new handler; no shared-state writes.
2. **Task 21.2 — VCS-Aware Ignores**: Layer `.gitignore` parsing onto the existing watcher event filter and initial-scan path so a Git-managed vault doesn't silently index `.git/`, `node_modules/`, `target/`, etc. Existing `watcher.ignore_patterns` config stays authoritative on conflict (operator override).

Tasks are independent: they share no code paths, no migration, and no transport surface. They ship together for round-10 gate convenience.

**Risk profile**: Low. Health is a new HTTP-only handler (no MCP surface, no DB writes). VCS-ignores is an additive filter at one boundary (`walk_vault` + `watcher::filter`); the existing `globset`-based path is preserved as the override layer.

**One new daemon dependency** for human review at workplan-ready handoff: the `ignore` crate (Decision 2.1 below).

---

## Intra-Step Sequencing

**Recommendation: Health-first, then VCS-ignores. Build sequentially in a single builder thread, not in parallel.**

Reasoning:

- **Health is genuinely small** (a single handler, one new struct, one daemon-start `Instant` captured in `main`). It can land in a few hours and will serve as a warmup that re-acquaints the builder with the API surface (`ApiState`, `spawn_blocking` discipline) before they touch the watcher.
- **VCS-ignores is the higher-risk surface** (correctness of indexed file set is load-bearing) and benefits from undivided attention. Sequencing it second means the builder isn't context-switching between an HTTP handler and `notify` event filtering.
- **Parallel build is not worth the coordination cost** — both tasks ship in one gate and the round is already small. Two ephemeral builders for a 1–2 hour task plus a 2–3 hour task is more orchestration overhead than throughput win.
- **Health-first also de-risks the round**: if VCS-ignores hits an unexpected snag (e.g. `ignore` crate doesn't expose a per-path predicate cleanly), we can still ship Task 21.1 alone and re-scope Task 21.2 to a follow-up round without losing the health endpoint.

If the coordinator decides to parallelize anyway, the two tasks are mechanically independent — they don't gate each other.

---

## Deferred Decisions — Resolved

### Task 21.1 deferred decisions

#### Decision 1.1: Per-vault snapshot inclusion

**Status**: ✅ Resolved
**Question**: Does the `/health` response body carry a per-vault array, or stay summary-only?
**Resolution**: **Summary-only for v0.5.0.** Body is `{ status, vaults_active, vaults_errored, uptime_seconds }` plus an `embedding` sub-object when embeddings are configured (see Decision 1.2). No `vaults: [...]` array.

**Why**:
- The existing `GET /status` endpoint (in `src/api/status.rs`) already exposes vault-level detail (path, indexed file count, last-indexed-at). A health probe and a status probe are different idioms — orchestration layers want a single boolean-shaped readiness call; humans/operators reach for `/status` (or `hmn status`) when they want the per-vault breakdown.
- Per-vault detail in `/health` would tempt orchestration layers to parse it and compute their own readiness logic, which is a layering inversion. Keep `/health` summary-only and let it be the single source of truth for "is the daemon serving traffic right now."
- If a real use case for per-vault health surfaces post-v0 (e.g. a multi-tenant control plane), it can be added behind an opt-in query parameter (`/health?detail=vaults`) in a future small round.

**Captured in**: `docs/specs/health-endpoint.md` (response schema section).

#### Decision 1.2: Embedding-service health policy

**Status**: ✅ Resolved
**Question**: When embeddings are configured for at least one vault, what does an unreachable embedding service map to — `degraded` or `unhealthy`?
**Resolution**: **`degraded`, never `unhealthy`.** The daemon still serves filesystem search, content search, and content retrieval without embeddings. Only embedding-dependent operations (semantic search) degrade.

**Status mapping ladder** (highest precedence wins, top-down):

| Signal | Status | HTTP |
| ------ | ------ | ---- |
| Watcher loop has crashed (not running) | `unhealthy` | 503 |
| DB probe (`SELECT 1` on any active vault) fails | `unhealthy` | 503 |
| Any vault is in `Errored` state | `degraded` | 503 |
| Embeddings configured AND embedding endpoint unreachable | `degraded` | 503 |
| Otherwise | `healthy` | 200 |

**Embedding probe shape**:
- Run **only when** at least one vault has embeddings actually configured (i.e. `EmbeddingConfig` is present and at least one runner has embeddings enabled). If embeddings are off for the whole daemon, this signal is skipped — an unconfigured optional service must not flap the endpoint.
- Probe is a single `HEAD` (or cheap `GET`) request to the embedding endpoint with a **500ms hard timeout**. No retries, no exponential backoff — health probes must respond fast.
- Result is **not cached** for v0; every `/health` call probes fresh. If this turns out to be expensive in dogfood (orchestrators that probe every 5s), a future small round can add a 1-second TTL cache. Keep the v0 implementation dumb and obviously correct.

**Why `degraded` not `unhealthy`**: `unhealthy` (especially mapped to a k8s liveness probe failure) tells the orchestrator to **restart** the pod. Restarting `hmnd` because the embedding service went down would create a thundering-herd reconnect storm without solving anything; the embedding service is the actual broken thing. `degraded` (503 but distinct status string) tells a readiness probe "don't send me work" while leaving liveness alone.

**Captured in**: `docs/specs/health-endpoint.md` (signal table + probe semantics).

#### Decision 1.3: Spec home

**Status**: ✅ Resolved
**Resolution**: **New file `docs/specs/health-endpoint.md`.** Use the existing `docs/specs/_template.md` shape.

**Why**: The endpoint is operationally distinct from any existing spec (which all describe data-plane surfaces — search, content retrieval, change events, vault management). Folding `/health` into `vault-management.md` or `mcp-streamable-http.md` would muddy those specs without buying anything. The new file is short (one wire shape, one signal table) but stands on its own.

---

### Task 21.2 deferred decisions

#### Decision 2.1: Crate choice — `ignore` vs `gitignore`

**Status**: ✅ Resolved (pending human review at workplan-ready handoff)
**Resolution**: **`ignore` crate** (BurntSushi, used by `ripgrep`).

Trade-off analysis:

| | `ignore` (used by ripgrep) | `gitignore` |
| --- | --- | --- |
| Hierarchical `.gitignore` semantics (root + nested) | ✅ Built-in via `WalkBuilder` | ❌ Single-file parsing only; would have to compose manually |
| Negation (`!important.log`) | ✅ Full Git semantics | ⚠️ Limited |
| `.git/` auto-exclusion | ✅ Default | ❌ Must be added by hand |
| Symlink handling parity with our existing walker | ✅ Configurable | ⚠️ Less control |
| Surface area / dep weight | Larger (depends on `globset` which we already pull in, plus `walkdir` which we also already pull in) | Smaller |
| Maintenance | Active (BurntSushi) | Lower activity |
| Per-path predicate (without re-walking) | ✅ `Gitignore::matched_path_or_any_parents` and `WalkBuilder::filter_entry` are both usable | ⚠️ Less ergonomic |

The shipping criteria require nested `.gitignore` semantics, negation patterns, and a per-path predicate that the watcher event filter can call (since watcher events arrive after the walk has already happened). `gitignore` would force us to either (a) re-parse and compose nested files by hand or (b) restrict to root-only `.gitignore` and lose nested semantics. Neither is acceptable.

`ignore` brings transitive deps we already have (`globset`, `walkdir`, `regex`); the marginal cost is `ignore` itself plus `same-file` and `crossbeam-utils`. This is well within the round's "one new dep" budget.

**Specifically we will use**: `ignore::gitignore::GitignoreBuilder` to construct a per-vault matcher that loads the vault-root `.gitignore` plus any nested `.gitignore` files discovered during the initial scan. The matcher is then exposed as a method on a small `VcsIgnore` newtype that both `walk_vault` and the watcher's event-filter layer call. We do **not** use `WalkBuilder` to drive the initial scan (that would force a parallel walker abstraction); we keep `walkdir::WalkDir` and call the matcher predicate ourselves. This minimizes blast radius into the existing scan path.

**Human review note**: Per AGENTS.md "ask vs proceed", the orchestrator pre-approved the **category** (one `.gitignore`-parsing crate) but the specific crate is a workplan-time decision the human will see. If the human rejects `ignore` in favor of `gitignore`, we re-scope Task 21.2 to root-only `.gitignore` and document nested-`.gitignore` support as a follow-up.

**Captured in**: `Cargo.toml` (new dep), `docs/specs/vault-ignores.md` (semantics).

#### Decision 2.2: Opt-out config knob

**Status**: ✅ Resolved
**Resolution**: **Introduce `respect_gitignore: bool` (default `true`)** under the existing `[watcher]` config section. Lives next to `ignore_patterns`.

```toml
[watcher]
debounce_ms = 500
ignore_patterns = [".git/**", ".obsidian/**", ...]
respect_gitignore = true   # new in v0.5.0
```

**Why introduce the knob**:
- Some operators run `hmnd` against vaults where `.gitignore` is **not** aligned with desired index scope. Example: a vault checked into Git mainly for backup, with `.gitignore` lines that exclude generated docs the operator actually wants searchable. Forcing them to choose between deleting `.gitignore` lines (which breaks Git workflow) and writing inverse `ignore_patterns` overrides (which is fragile) is operator-hostile.
- The default-on stance still gives the high-value behavior (Git-managed vaults work right out of the box) without locking operators in.

**Default `true`** matches the round-10 philosophy: out-of-the-box ergonomics for the most common case.

**Captured in**: `src/config.rs` (`WatcherConfig`), `docs/specs/vault-ignores.md`.

#### Decision 2.3: Conflict-resolution semantics — how daemon config wins

**Status**: ✅ Resolved
**Resolution**: **Daemon `ignore_patterns` is evaluated AFTER VCS ignore as an override layer with allowlist semantics via negation.** Concretely:

1. The compiled `globset::GlobSet` from `WatcherConfig::compiled_ignores()` is split into two tiers at config-load time:
   - **Exclude patterns**: any pattern that does NOT start with `!` (the existing default `.git/**`, `.obsidian/**`, etc.).
   - **Re-include patterns**: any pattern that starts with `!` (a new operator-facing affordance).
2. **Path inclusion algorithm** (per file path, applied identically in `walk_vault` and the watcher event filter):
   ```
   if config_reinclude_patterns.is_match(path):
       INCLUDE                                  # operator override wins
   elif config_exclude_patterns.is_match(path):
       EXCLUDE                                  # operator-set hard exclusion
   elif vcs_ignore.matched_path_or_any_parents(path).is_ignore():
       EXCLUDE                                  # .gitignore says no
   else:
       INCLUDE
   ```
3. **`.git/` is unconditionally excluded** at step 0, before any of the above runs (defensive default; never indexed regardless of config).

**Worked example**: vault has `.gitignore` with `.env*`. Operator wants `.env.example` indexed. They add `!.env.example` to `watcher.ignore_patterns`. Result: `.env.example` is included (config re-include wins); `.env`, `.env.local`, etc. remain excluded (no override; `.gitignore` applies).

**Why negation patterns rather than an allowlist field**:
- `globset::GlobSet` already supports the pattern surface; we just need to interpret leading `!` ourselves at config-load time. No new config shape needed.
- Operators familiar with `.gitignore` already know `!` semantics; reusing them is least-surprise.
- An explicit `allowlist: [...]` field would be a parallel surface to `ignore_patterns` and double the cognitive load.

**Migration concern**: The existing `WatcherConfig::compiled_ignores()` does NOT currently treat leading `!` specially — `globset::Glob::new("!foo")` will likely return an error or treat `!` as a literal. Builder must implement the split at config-load time, **not** push `!`-prefixed patterns into `globset`. The split happens in `WatcherConfig::compiled_ignores()` (or a new sibling method like `compile_ignores_split() -> (GlobSet, GlobSet)`).

**Captured in**: `docs/specs/vault-ignores.md` (precedence chain + worked example).

#### Decision 2.4: Nested-`.gitignore` re-evaluation on edit

**Status**: ✅ Resolved
**Resolution**: **Restart-required for v0.5.0.** A `.gitignore` itself edited at runtime does NOT trigger re-evaluation of the affected subtree. Document in spec; revisit if dogfood surfaces a real use case.

**Why**:
- The vast majority of `.gitignore` edits happen at vault setup time, before `hmnd` is running, or are followed by a logical "I changed how my vault is organized" pause where the operator can `hmn vault rescan` (or restart `hmnd`) themselves.
- Implementing live re-evaluation correctly requires (a) detecting `.gitignore` writes/deletes, (b) recomputing the nested matcher for the affected subtree, (c) reconciling the now-different inclusion set against the existing index (re-walking to find newly-includable files; deleting newly-excludable ones). This is non-trivial and out of scope for a polish round.
- `hmn vault rescan` is the documented escape hatch and already exists.

**Spec language**: "Editing a `.gitignore` file at runtime does not re-evaluate the affected subtree. Run `hmn vault rescan <vault>` (or restart the daemon) to apply the change."

**Captured in**: `docs/specs/vault-ignores.md` (limitations section).

#### Decision 2.5: Spec home

**Status**: ✅ Resolved
**Resolution**: **New file `docs/specs/vault-ignores.md`.** Use the existing `docs/specs/_template.md` shape.

**Why**: The precedence chain (config re-include → config exclude → VCS ignore → default include, plus the always-excluded `.git/`) is non-trivial enough to warrant a dedicated spec. Folding it into `vault-management.md` (which is about the lifecycle of vault registration) or a watcher-internal doc would obscure it. A short standalone file is the clearest home for operators who are debugging "why isn't my file indexed."

**Cross-references**: `vault-ignores.md` should be linked from `docs/specs/vault-management.md` (under "ignoring files") and from any future README section describing operator config.

---

## Task Breakdown

### Task 21.1 — Health Endpoint

#### Tier 1: Daemon-start instant capture

**Task 1.1**: Capture daemon-start `Instant` at boot

- Location: `src/bin/hmnd.rs` (the `main` entrypoint, before `Server::serve`)
- Add: `let started_at = std::time::Instant::now();`
- Plumb `started_at` into `ApiState` (extend the struct in `src/api/mod.rs`)
- No new background task; this is a single value captured once
- Owner: Builder
- Criterion: ✅ `ApiState` exposes `started_at: Instant`; `uptime_seconds` derived as `started_at.elapsed().as_secs()`

#### Tier 2: Response type

**Task 2.1**: Replace placeholder `HealthResponse` in `src/api/types.rs`

- Replace the existing one-field `HealthResponse { status: String }` with the v0.5.0 shape:
  ```rust
  #[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
  pub struct HealthResponse {
      pub status: String,           // "healthy" | "degraded" | "unhealthy"
      pub vaults_active: u64,
      pub vaults_errored: u64,
      pub uptime_seconds: u64,
      #[serde(default, skip_serializing_if = "Option::is_none")]
      pub embedding: Option<EmbeddingHealth>,
  }

  #[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
  pub struct EmbeddingHealth {
      pub status: String,           // "healthy" | "degraded"
      pub endpoint: String,         // configured endpoint (no secrets in the URL)
  }
  ```
- `embedding` is `None` when no vault has embeddings configured
- Owner: Builder
- Criterion: ✅ Type compiles; serde round-trip test passes

#### Tier 3: Handler implementation

**Task 3.1**: Implement signal collection in `src/api/health.rs`

- Replace placeholder handler with a state-aware one:
  ```rust
  pub(crate) async fn health(State(s): State<ApiState>) -> impl IntoResponse {
      let snapshot = collect_health(&s).await;
      let status_code = match snapshot.status.as_str() {
          "healthy" => StatusCode::OK,
          _ => StatusCode::SERVICE_UNAVAILABLE,
      };
      (status_code, Json(snapshot))
  }
  ```
- `collect_health` gathers the four signals per the ladder in Decision 1.2:
  1. Watcher liveness — query `VaultManager` for whether each active runner's watcher task is alive (extend `VaultManager` with a `watcher_alive(&VaultId) -> bool` method if needed; this should be cheap, no I/O).
  2. DB probe — for each active vault, `spawn_blocking { conn.query_row("SELECT 1", [], |r| r.get::<_, i64>(0)) }`. Treat any error as DB-unhealthy.
  3. Vault status counts — `vaults_active` and `vaults_errored` from `VaultManager`.
  4. Embedding probe — only if `EmbeddingConfig` is present and at least one runner has embeddings enabled; `reqwest::Client::head(endpoint).timeout(Duration::from_millis(500)).send().await`.

- Owner: Builder
- Criterion: ✅ Handler returns correct status code + body for healthy and degraded paths

**Task 3.2**: Wire the route

- Verify the route is mounted in `src/api/mod.rs` (it should be — the slot already exists)
- Confirm it's NOT gated by `[mcp] enable_write_tools` (it shouldn't be — it's a GET on the HTTP API, not an MCP tool)
- Owner: Builder
- Criterion: ✅ `curl http://localhost:8080/health` returns the new shape

**Task 3.3**: Negative fingerprint — runtime not blocked

- `rg "Connection::open" src/api/health.rs` → no direct synchronous DB opens
- All DB calls in `collect_health` are inside `tokio::task::spawn_blocking`
- Embedding probe uses `reqwest` (async-native; no blocking)
- Owner: Builder (gate-time verification)
- Criterion: ✅ Code review confirms no sync DB calls in async context

#### Tier 4: Testing

**Task 4.1**: Unit test — healthy path

- Fixture: in-process `ApiState` with one active vault, watcher alive, DB present, no embeddings configured
- Call handler; expect `200 OK` with `status: "healthy"`, `vaults_active: 1`, `vaults_errored: 0`, `embedding: None`
- Owner: Builder
- Criterion: ✅ Test passes

**Task 4.2**: Unit test — degraded path (errored vault)

- Fixture: one active vault + one errored vault stub (via `VaultManager::for_tests` with errored row)
- Call handler; expect `503` with `status: "degraded"`, `vaults_errored: 1`
- Owner: Builder
- Criterion: ✅ Test passes

**Task 4.3**: Unit test — degraded path (embedding endpoint unreachable)

- Fixture: one vault with embeddings configured pointing to `http://127.0.0.1:1` (will fail fast)
- Call handler; expect `503` with `status: "degraded"`, `embedding.status == "degraded"`
- Owner: Builder
- Criterion: ✅ Test passes

**Task 4.4**: Manual test fixture — uptime increments

- Manual recipe in spec: start `hmnd`, hit `/health` twice with a 2-second pause, confirm `uptime_seconds` increases
- Owner: Builder (manual; documented in spec)
- Criterion: ✅ Recipe documented and runnable

#### Tier 5: Documentation (Task 21.1)

**Task 5.1**: Author `docs/specs/health-endpoint.md`

- Sections per `_template.md`:
  - Purpose: HTTP probe for orchestration layers (k8s readiness, systemd external checks)
  - Wire shape: response schema with field descriptions
  - Signal ladder (the table from Decision 1.2)
  - Status codes (`200` healthy, `503` degraded/unhealthy)
  - Out of scope: per-vault detail (use `/status`), MCP tool surface, `/metrics` (separate future round)
  - Cross-references: `docs/specs/_template.md`, `src/api/health.rs`
- Owner: Builder
- Criterion: ✅ Spec lands; references match source

---

### Task 21.2 — VCS-Aware Ignores

#### Tier 1: Dependency + config

**Task 1.1**: Add `ignore` crate dependency

- `Cargo.toml`: add `ignore = "0.4"` (latest 0.4.x as of writing)
- Run `cargo build` to confirm no transitive-dep conflicts
- Owner: Builder
- Criterion: ✅ Build green

**Task 1.2**: Extend `WatcherConfig` with `respect_gitignore`

- Location: `src/config.rs`, `WatcherConfig` struct
- Add field: `#[serde(default = "default_respect_gitignore")] pub respect_gitignore: bool`
- Default function returns `true`
- Update the `Default` impl
- Owner: Builder
- Criterion: ✅ Round-trip TOML test: missing field defaults to `true`; explicit `false` honored

**Task 1.3**: Split `compiled_ignores` into exclude + re-include tiers

- Location: `src/config.rs`, replace `WatcherConfig::compiled_ignores(&self) -> Result<GlobSet>` with `compiled_ignores_split(&self) -> Result<CompiledIgnores>` returning a struct:
  ```rust
  pub struct CompiledIgnores {
      pub exclude: GlobSet,        // patterns NOT starting with '!'
      pub reinclude: GlobSet,      // patterns starting with '!' (with the '!' stripped)
  }
  ```
- Logic: iterate `ignore_patterns`; for each pattern, if it starts with `!`, strip and push into `reinclude` builder; otherwise push into `exclude` builder.
- Keep the old method as a thin shim that returns only `exclude` for any caller that doesn't need the split (or migrate all callers; check via `rg "compiled_ignores"`).
- Owner: Builder
- Criterion: ✅ Unit test: `["**/target/**", "!important.log"]` produces an exclude set matching `target/foo` and a re-include set matching `important.log`

#### Tier 2: VCS ignore matcher

**Task 2.1**: Build `VcsIgnore` newtype

- New module: `src/watcher/vcs_ignore.rs` (mod-level; reused by `walk_vault`)
- API:
  ```rust
  pub struct VcsIgnore {
      matchers: Vec<(PathBuf, ignore::gitignore::Gitignore)>,
      // ordered: vault root first, then nested directories deeper down
  }

  impl VcsIgnore {
      /// Build by walking the vault root once, loading every `.gitignore` found.
      /// `vault` is the canonical vault root.
      pub fn build(vault: &Path) -> Result<Self>;

      /// Returns true if `rel_path` (vault-relative, forward-slash) is ignored
      /// by any applicable `.gitignore` in the matcher chain.
      pub fn is_ignored(&self, rel_path: &str, is_dir: bool) -> bool;

      /// Empty matcher — equivalent to "no .gitignore present".
      pub fn empty() -> Self;
  }
  ```
- Implementation notes:
  - Use `ignore::gitignore::GitignoreBuilder::new(vault_root).add(vault_root.join(".gitignore"))` for the root matcher.
  - For nested `.gitignore` files, walk the vault once with `walkdir` (or piggyback on the existing scan) to collect them. Each nested `.gitignore` becomes a separate `Gitignore` rooted at its own directory.
  - `is_ignored` consults matchers from deepest-applicable to shallowest, returning the first definitive match (Git semantics: the most-specific rule wins; nested negation can re-include).
  - `ignore::gitignore::Gitignore::matched_path_or_any_parents` already handles the "any parent directory ignored" case.
- Owner: Builder
- Criterion: ✅ Unit tests for: root-only `.gitignore`, nested `.gitignore`, negation pattern (`!important.log`), `.git/` always-excluded, no-`.gitignore` case (empty matcher returns `false`)

**Task 2.2**: Build the unified path-inclusion predicate

- New helper (could live in `src/watcher/filter.rs` or a new `src/watcher/inclusion.rs`):
  ```rust
  pub struct InclusionFilter {
      pub config: CompiledIgnores,
      pub vcs: VcsIgnore,
      pub respect_gitignore: bool,
  }

  impl InclusionFilter {
      pub fn includes(&self, rel_path: &str, is_dir: bool) -> bool {
          // Always exclude .git/
          if rel_path == ".git" || rel_path.starts_with(".git/") {
              return false;
          }
          // Config re-include wins (operator override)
          if self.config.reinclude.is_match(rel_path) {
              return true;
          }
          // Config exclude
          if self.config.exclude.is_match(rel_path) {
              return false;
          }
          // VCS ignore (only if enabled)
          if self.respect_gitignore && self.vcs.is_ignored(rel_path, is_dir) {
              return false;
          }
          true
      }
  }
  ```
- This is the single source of truth that both `walk_vault` and the watcher event filter call. **No duplicated rule application.**
- Owner: Builder
- Criterion: ✅ Unit tests covering each branch of the precedence chain

#### Tier 3: Wire into the walker

**Task 3.1**: Update `walk_vault` to use `InclusionFilter`

- Location: `src/indexer/walk.rs`
- Change signature: `pub fn walk_vault(vault: &Path, filter: &InclusionFilter) -> Result<WalkOutcome>`
- Replace the existing `if ignores.is_match(&rel_str)` line with `if !filter.includes(&rel_str, false)`.
- Update all callers (the indexer, any tests) to construct `InclusionFilter { config, vcs: VcsIgnore::build(vault)?, respect_gitignore }`.
- Existing unit tests in `walk.rs` should continue to pass (they pass an empty `GlobSet` today; equivalent is `InclusionFilter` with empty `config` and `VcsIgnore::empty()`).
- Owner: Builder
- Criterion: ✅ Existing walk tests pass; new tests cover `.gitignore`-driven exclusion

**Task 3.2**: Update watcher event filter to use `InclusionFilter`

- Location: `src/watcher/filter.rs` and the translate path in `src/watcher/translate.rs`
- The watcher already filters incoming events through a `GlobSet`; replace that call with `InclusionFilter::includes`.
- The `InclusionFilter` is constructed once at watcher spawn (`spawn_watcher`) and held in the translate context. **VcsIgnore is built once at watcher start** (per Decision 2.4: restart-required for `.gitignore` edits).
- Owner: Builder
- Criterion: ✅ Watcher integration test: a vault with `.gitignore` excluding `node_modules/` does not emit watcher events for files inside `node_modules/`

#### Tier 4: Wiring through `WatcherConfig`

**Task 4.1**: Plumb `respect_gitignore` from config into `InclusionFilter` construction

- All sites that construct an `InclusionFilter` (walker, watcher) need access to `WatcherConfig`.
- Pass through the existing config plumbing in `VaultManager::open` / `spawn_watcher`.
- Owner: Builder
- Criterion: ✅ Setting `respect_gitignore = false` in TOML disables VCS-aware filtering at runtime

#### Tier 5: Testing

**Task 5.1**: Unit tests — `VcsIgnore` semantics

- Already specified under Task 2.1 above. Covers root, nested, negation, `.git/`, empty.
- Owner: Builder
- Criterion: ✅ All cases pass

**Task 5.2**: Unit tests — `InclusionFilter` precedence

- Test matrix:
  - `.git/foo` → excluded (always)
  - Path matching only `.gitignore` → excluded
  - Path matching `.gitignore` AND config re-include → included
  - Path matching `.gitignore` AND config exclude → excluded
  - Path matching only config exclude → excluded
  - Path matching nothing → included
  - With `respect_gitignore = false`: `.gitignore` matches are ignored
- Owner: Builder
- Criterion: ✅ All cases pass

**Task 5.3**: Integration test — non-trivial `.gitignore`

- Fixture: tempdir vault with this `.gitignore`:
  ```
  node_modules/
  target/
  *.log
  !important.log
  ```
- Files seeded:
  - `notes/a.md` (kept)
  - `node_modules/pkg/index.md` (excluded by directory rule)
  - `target/build.md` (excluded by directory rule)
  - `debug.log` → not `.md`, irrelevant; instead seed `notes/debug.log.md` (kept; `*.log` doesn't match `.md`)
    - Actually a better fixture: `notes/debug.log` (not indexed because not `.md` regardless), and `notes/foo.md` containing the negation case is awkward — simpler test: use the precedence test from Task 5.2 to cover negation
- Run `walk_vault` and assert exactly the expected file set is returned
- Owner: Builder
- Criterion: ✅ Test passes

**Task 5.4**: Integration test — watcher event filter matches `.gitignore`

- Fixture: vault with `.gitignore` excluding `target/`
- Spawn watcher; create `target/built.md`; assert no `Upsert` event arrives within the debounce window + a small grace
- Then create `notes/kept.md`; assert one `Upsert` event arrives
- Owner: Builder
- Criterion: ✅ Test passes; no events leak from ignored paths

**Task 5.5**: Manual test fixture — operator override via negation pattern

- Documented recipe in spec:
  1. Create vault with `.gitignore` containing `.env*`
  2. Configure `watcher.ignore_patterns = [".git/**", "!.env.example"]`
  3. Create `.env`, `.env.local`, `.env.example`
  4. Verify only `.env.example` is indexed (config re-include beats `.gitignore`)
- Owner: Builder
- Criterion: ✅ Recipe runs end-to-end and matches documented behavior

#### Tier 6: Negative fingerprints

**Task 6.1**: `notify` callback still does only translation

- `rg "blocking_send|spawn_blocking" src/watcher/` → `InclusionFilter::includes` is called inside the translate callback but does **only in-memory glob matching** (no I/O, no SQL).
- `VcsIgnore::is_ignored` is pure `globset` lookups against pre-loaded matchers; no FS reads on the hot path.
- Owner: Builder (gate-time verification)
- Criterion: ✅ Code review confirms no I/O on the watcher callback path

**Task 6.2**: No duplicate rule application between walker and watcher

- `rg "InclusionFilter|VcsIgnore" src/` shows both `walk_vault` and the watcher use the same struct with the same `includes` method
- Owner: Builder (gate-time verification)
- Criterion: ✅ Single shared predicate confirmed

#### Tier 7: Documentation (Task 21.2)

**Task 7.1**: Author `docs/specs/vault-ignores.md`

- Sections per `_template.md`:
  - Purpose: VCS-aware path filtering layered behind operator-set `ignore_patterns`
  - Precedence chain (the algorithm from Decision 2.3, with the worked example)
  - Configuration: `watcher.respect_gitignore` and `watcher.ignore_patterns` (including new `!`-negation semantics)
  - Defaults: `respect_gitignore = true`; `.git/` always excluded
  - Limitations:
    - Editing `.gitignore` at runtime requires `hmn vault rescan` or daemon restart (Decision 2.4)
    - Symlink behavior unchanged from existing walker (`WalkDir::follow_links(true)` for the initial scan; `notify` does not follow symlinks for live events)
    - `.dockerignore` and other VCS variants not supported in v0
  - Cross-references: `docs/specs/vault-management.md`, `src/watcher/`, `src/indexer/walk.rs`
- Owner: Builder
- Criterion: ✅ Spec lands; precedence example is correct and runnable

**Task 7.2**: Cross-link from `vault-management.md`

- Add a one-liner under whatever section covers vault config / file scope, pointing to `vault-ignores.md`
- Owner: Builder
- Criterion: ✅ Cross-link lands

---

### Cross-task: build + test verification

**Task X.1**: `cargo test` green
- All new tests pass; all existing tests pass (no regressions)
- Criterion: ✅ `cargo test` exits 0

**Task X.2**: `cargo clippy -- -D warnings` clean
- No new warnings introduced by either task
- Criterion: ✅ `cargo clippy` exits 0

**Task X.3**: `cargo fmt` clean
- Criterion: ✅ `cargo fmt --check` exits 0

**Task X.4**: Append round-10 retro entry to `notes/project-planning-workflow-notes.md`
- Brief retro: what shipped, what (if anything) we learned about the round-10 polish-shape format
- Owner: Coordinator (post-gate)
- Criterion: ✅ Retro entry appended

---

## Negative Fingerprints (Builder Verifies at Gate)

### Task 21.1 (Health)

- [ ] `rg "Connection::open|conn\.execute|pool\.get" src/api/health.rs` — every match is inside a `spawn_blocking { ... }` closure
- [ ] `rg "\.await" src/api/health.rs` — every match is on a `spawn_blocking::join` or `reqwest` future, not on a sync `rusqlite` call
- [ ] `rg "/health" src/api/mod.rs` — route is mounted; `enable_write_tools` is NOT referenced in the same context
- [ ] `rg "tools/list|tool_router" src/mcp/` — does NOT include any `health` tool registration (HTTP-only per round scope)

### Task 21.2 (VCS Ignores)

- [ ] `rg "GlobSet" src/watcher/ src/indexer/walk.rs` — call sites go through `InclusionFilter`, not raw `GlobSet::is_match` against config patterns
- [ ] `rg "compiled_ignores" src/` — no caller of the old single-tier method outside `compiled_ignores_split` (or whatever bridge we keep)
- [ ] `rg "GitignoreBuilder|gitignore::" src/watcher/vcs_ignore.rs` — single source of `.gitignore` parsing; no other module instantiates a matcher
- [ ] Watcher callback path (`src/watcher/translate.rs`) does NOT call `fs::read`, `Path::exists`, or any I/O — only pure predicate evaluation
- [ ] `rg "respect_gitignore" src/` — config field flows from `WatcherConfig` → `InclusionFilter` construction sites; no orphan reads
- [ ] `.git/` exclusion is enforced in `InclusionFilter::includes`, not (only) in `.gitignore` itself — confirmed by a unit test that omits `.git/` from `.gitignore` and still excludes the path

---

## Spec Locations (Summary)

| Spec | File | Justification (one line) |
| --- | --- | --- |
| Health endpoint | `docs/specs/health-endpoint.md` (new) | Operationally distinct from data-plane specs; small but standalone |
| Vault ignores | `docs/specs/vault-ignores.md` (new) | Non-trivial precedence chain warrants dedicated home for operators debugging "why isn't my file indexed" |

Both files use `docs/specs/_template.md` as the shape.

---

## New Daemon Dependencies (For Human Review at Workplan-Ready Handoff)

**Single new crate**: `ignore = "0.4"` (BurntSushi, used by `ripgrep`)

- **Why this crate**: full hierarchical `.gitignore` semantics, robust negation handling, automatic `.git/` exclusion, ergonomic per-path predicate via `Gitignore::matched_path_or_any_parents`. The smaller `gitignore` crate would force us to either lose nested-`.gitignore` support or hand-roll the composition layer.
- **Transitive cost**: `ignore` depends on `globset` and `walkdir` (both already in our tree), plus `same-file` and `crossbeam-utils`. Marginal weight is low.
- **What we use**: `ignore::gitignore::{Gitignore, GitignoreBuilder}`. We do NOT pull in `ignore::WalkBuilder`; we keep `walkdir::WalkDir` driving the initial scan and call the matcher predicate ourselves.
- **Maintenance**: actively maintained; same author as `ripgrep` and `walkdir`.

If the human review prefers `gitignore` (smaller, simpler), the fallback plan is to scope Task 21.2 down to root-only `.gitignore` support and document nested-`.gitignore` as a follow-up. The precedence chain, config knob, and override semantics all stay the same.

No other new dependencies for either task. Health endpoint reuses `axum`, `serde`, `reqwest` (already in the tree).

---

## Shipping Criteria Coverage

Cross-checked against `notes/roadmap/roadmap-10.md` Step 21 shipping criteria.

### Task 21.1 — Health Endpoint

| Roadmap criterion | Workplan task(s) |
| --- | --- |
| `GET /health` implemented in `src/api/health.rs` | 3.1, 3.2 |
| Response body shape | 2.1 |
| Status mapping (200/503) | 3.1 |
| Watcher / DB / embedding signals | 3.1 (signal collection); ladder per Decision 1.2 |
| No new background tasks; uptime from boot Instant | 1.1 |
| Read-only; not gated by `enable_write_tools` | 3.2 |
| No MCP tool surface | Negative fingerprint (no MCP registration) |
| Negative fingerprint: handler doesn't block runtime | 3.3 |
| Spec | 5.1 |
| `cargo test` green; `cargo clippy` clean | X.1, X.2 |
| Manual fixture: healthy + degraded paths | 4.1, 4.2, 4.3, 4.4 |
| Per-vault snapshot (deferred decision 1.1) | Resolved → summary-only |
| Embedding semantics (deferred decision 1.2) | Resolved → `degraded` not `unhealthy` |
| Spec home (deferred decision 1.3) | Resolved → new file |

### Task 21.2 — VCS-Aware Ignores

| Roadmap criterion | Workplan task(s) |
| --- | --- |
| Vault-root `.gitignore` parsed; applied to watcher + initial scan | 2.1, 3.1, 3.2 |
| Nested `.gitignore` honored | 2.1 |
| Documented + tested precedence chain | 2.2, 5.2, 7.1 |
| Daemon config wins on conflict | Decision 2.3 (negation override); 2.2; 5.2 |
| `.git/` always excluded | 2.2 (algorithm step 0); 5.1 |
| Symlink behavior preserved + documented | 7.1 (limitations section) |
| Default-on for vaults with `.gitignore` | Decision 2.2 (`respect_gitignore = true`); 1.2 |
| Shared ignore-evaluation logic (no duplication) | 2.2 (`InclusionFilter` is the single predicate); 6.2 |
| Negative fingerprint: representative `.gitignore` works | 5.3, 5.4 |
| Spec | 7.1, 7.2 |
| `cargo test` green; `cargo clippy` clean | X.1, X.2 |
| Manual fixture: non-trivial `.gitignore` + negation | 5.3, 5.5 |
| Crate choice (deferred decision 2.1) | Resolved → `ignore` |
| Opt-out knob (deferred decision 2.2) | Resolved → `respect_gitignore` boolean |
| Conflict semantics (deferred decision 2.3) | Resolved → `!`-negation in `ignore_patterns` |
| Nested-`.gitignore` re-eval (deferred decision 2.4) | Resolved → restart-required for v0 |
| Spec home (deferred decision 2.5) | Resolved → new file |

### Cross-task

| Roadmap criterion | Workplan task(s) |
| --- | --- |
| Both ship in single round-10 gate; don't gate each other | Sequencing section (health-first, sequential) |
| Workplan picks sequencing rationale | Sequencing section |
| Round-10 retro appended | X.4 |

---

## Recommended Builder Batching

Single builder, sequential:

1. **Batch H1** (Health): Tasks 1.1, 2.1, 3.1, 3.2, 3.3, 4.1–4.4, 5.1 — all of Task 21.1
2. **Gate H1**: Verify Task 21.1 negative fingerprints + test pass; pause for any course-correction before moving on
3. **Batch V1** (VCS-ignores foundations): Tasks 1.1, 1.2, 1.3, 2.1, 2.2 — dep, config, matchers, predicate
4. **Batch V2** (Wire-up): Tasks 3.1, 3.2, 4.1 — walker, watcher, plumbing
5. **Batch V3** (Tests + spec): Tasks 5.1–5.5, 6.1, 6.2, 7.1, 7.2
6. **Final gate**: Cross-task verification (X.1–X.4); coordinator surfaces to human for round-10 sign-off

---

## Notes for Coordinator and Builder

- **Deferred decisions are resolved**; no blocking questions remain. The one item that wants explicit human eyes at workplan-ready handoff is the `ignore` crate dep (Decision 2.1).
- **Health is the warm-up**; build it first to re-acquaint with `ApiState` and `spawn_blocking` discipline before touching the watcher.
- **VCS-ignores' load-bearing piece is `InclusionFilter`** — the single predicate consumed by both walker and watcher. Don't duplicate the rules in the watcher's translate layer; call the predicate.
- **`!` in `ignore_patterns`** is a new operator-facing affordance. The existing default patterns are all positive excludes, so no migration concern, but the spec needs to document the new semantics clearly.
- **`VcsIgnore::build` happens once per vault at watcher spawn** — it's not on the hot event path. Editing `.gitignore` at runtime is documented as restart-required.
- **Manual testing is critical for Task 21.2** — a non-trivial `.gitignore` with a negation pattern is the highest-signal fixture; run it before gate review.

---

## Version History

| Date | Author | Change |
|------|--------|--------|
| 2026-05-02 | Researcher (step-21-researcher) | Initial workplan from Round 10 roadmap |
