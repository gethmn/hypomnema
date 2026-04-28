# Step 9 Workplan — Per-vault internal refactor + registry foundation

**Step**: 9 of 11 (round 3 of 3). First step of round 3 — see [`roadmap-3.md`](./roadmap-3.md) for the round and [`archive/step-08-workplan.md`](./archive/step-08-workplan.md) for the immediately prior step (the round-2 shipping gate). Step 9 is the structural-foundation step for round 3; step 10 lights up the user-visible vault control plane on top of this foundation.

**Status**: Shipped 2026-04-28. See `notes/project-planning-workflow-notes.md` § Step 9 for the retrospective.

**Round-2 lessons carrying forward** (from [`notes/project-planning-workflow-notes.md`](../project-planning-workflow-notes.md) § End-of-round retrospective):

- **MSRV cross-check** runs at workplan self-review for any new top-level crate. Step 9 introduces one new dep candidate (`uuid` for surrogate IDs — see Resolution A); MSRV is verified against `rust-toolchain.toml` (currently 1.88.0 since round-2 step 8) before the build starts.
- **Manual smoke verification** is load-bearing for medium-high-risk wiring tasks (round-2 4-of-4 wiring tasks paid off). Step 9's wiring task is **9.5** (daemon startup-sequence rewrite + reconcile + migration); smoke is bundled into 9.5 per the round-2 step-7 task-7.3 / step-8 task-8.3 precedent.
- **Behavior-preservation as the load-bearing gate**: the round-2 step-7 schema-amendment shape (later step touches earlier-step schema) and step-8 brand-identity reversal both showed that any drift in shipped behavior surfaces fast through smoke + integration tests. Step 9's whole goal is "no observable behavior change in single-vault operation"; treat any test regression as a hard stop, not a soft flag.
- **Forward-note prediction-vs-observation** check applies to predictions about library / OS behavior. Step 9's main external library is `notify` / `notify-debouncer-full` (already battle-tested through round 1 step 3 — predictions are low-risk) and `rusqlite` for the new `vaults.sqlite` (also battle-tested). The fresh territory is filesystem-rename behavior across platforms during the v0-state migration (Resolution B).
- **Skills carrying forward**: [`rusqlite-in-async`](../../.claude/skills/rusqlite-in-async/SKILL.md) (every SQL site in the new `vault_registry` module + per-vault store wraps `spawn_blocking`); [`filesystem-watching`](../../.claude/skills/filesystem-watching/SKILL.md) (per-vault watchers all share the round-1 notify + debouncer pattern, including sync-conflict filtering at the boundary).

---

## Goal recap

`hmnd` is internally per-vault. A new `vault_registry` module owns `<data_dir>/vaults.sqlite` (the authoritative vault registry per [ADR-0010](../../docs/decisions/0010-vault-definitions-as-runtime-state.md)). Per-vault subdirectories live at `<data_dir>/vaults/<vault_id>/` and contain `index.sqlite`, `outbox.jsonl`, and `meta.toml`. The store, indexer, watcher, and outbox modules are refactored from "one global instance" to "N per-vault instances keyed by vault_id." Daemon startup reads the registry, reconciles per-vault state against the filesystem, and spawns one watcher + indexer per `active` vault.

**No new user-visible surface ships in step 9.** No new HTTP routes, no new CLI subcommands, no new MCP tools. The four search/event spec amendments (Solo todo 64) and the vault-management.md fleshout (Solo todo 65) are explicitly **deferred to step 10's workplan-write phase**, where they get pulled forward at the moment cross-vault search semantics need pinning.

The migration story for an existing v0.1.0 deployment: on first startup with an empty `vaults.sqlite`, a pre-existing `<data_dir>/index.sqlite`, and a top-level `[vault]` config block, the daemon auto-creates one vault using `default_vault_name` (default `"default"`) and the legacy path; `index.sqlite` and `outbox.jsonl` are atomically renamed into the new per-vault subdirectory. No re-index. The legacy `[vault]` block is accepted-and-warned at startup; the daemon translates it into a registry row at first migration and ignores it on subsequent starts.

The shipping gate is **behavior preservation**: every existing integration test (`tests/scan.rs`, `tests/watch.rs`, `tests/outbox.rs`, `tests/embedding.rs`, `tests/mcp.rs`, plus the skeleton/config tests) passes unchanged. Tests use the legacy `[vault]` config shape (or a step-9-equivalent), the daemon auto-creates a single vault under the hood, and the search/outbox surface returns identical results to v0.1.0 against the same vault contents.

---

## Deferred-decision resolutions

The six TBDs from [`roadmap-3.md`](./roadmap-3.md) § Step 9 are resolved below (A–F).

### A. Surrogate ID format

**Resolution**: **UUIDv7 string-form via the `uuid` crate**, stored as TEXT in `vaults.sqlite` and used as the per-vault subdirectory name. User-facing display contexts (logs, future `hmn vault list` output in step 10) format as `vault_<uuid>` for readability and disambiguation; storage and filesystem paths use the bare canonical UUID form (`018f3a7c-9b4e-7d2a-95f1-c8a6e3b2d1f0`).

```toml
# Cargo.toml addition
uuid = { version = "1.10", features = ["v7", "serde"] }
```

**Why UUIDv7 over alternatives** (the three candidates from [`docs/specs/vault-management.md` § Open Questions](../../docs/specs/vault-management.md#open-questions)):

1. *Time-ordered out of the box.* Lexicographic sort of UUIDv7 strings = creation-time sort, which is the natural default ordering for `hmn vault list` (step 10). UUIDv4 sorts randomly; ULID sorts time-ordered but its base32 encoding is less universally recognized than UUID.
2. *Standard, no custom encoder needed.* `vault_<base32>` (the spec's own first candidate) requires hand-rolling a Crockford-base32 encoder + an OS-entropy source glue; UUIDv7 is one well-known crate. The `uuid` crate's MSRV is 1.63 (well below the project's 1.88), zero transitive deps for `v7` feature beyond `getrandom`, and sub-1KB code size.
3. *Filesystem-safe across all platforms.* The hyphen-separated 36-char form has no Windows-reserved characters, no path-separator concerns. The `vault_` prefix is reserved for user-facing display contexts only; subdirectory names use the bare UUID for shorter paths (Windows `MAX_PATH` is the only concern, and 36 chars + the `vaults/` prefix is well under 260).

**MSRV cross-check (round-2 lesson)**: `cargo info uuid` confirms `uuid = "1.10.x"` requires `rust-version = "1.63"`, well below the project's 1.88.0 pin. No toolchain bump needed.

**How to apply**: add `uuid = { version = "1.10", features = ["v7", "serde"] }` to `Cargo.toml` in Task 9.1. The `vault_registry` module exports a `VaultId` newtype wrapping `String` (or the `uuid::Uuid` type directly behind the newtype) with `VaultId::new()` returning a fresh UUIDv7. Display impl uses the bare canonical form; user-facing CLI prefix (`vault_<uuid>`) is a step-10 concern.

**References**: [`docs/specs/vault-management.md` § Open Questions](../../docs/specs/vault-management.md#open-questions); [`Cargo.toml`](../../Cargo.toml); upstream `uuid` crate v7 docs.

### B. Migration of existing v0 single-vault state

**Resolution**: **atomic move-and-reconcile**. On first startup against pre-existing v0 state:

1. Daemon detects (a) `<data_dir>/index.sqlite` exists, (b) `<data_dir>/vaults.sqlite` is empty or absent, (c) a top-level `[vault]` config block is present.
2. Daemon creates `<data_dir>/vaults.sqlite` (Resolution D below) and inserts one row using `default_vault_name` (Resolution C) + the legacy `vault.path`.
3. Daemon `mkdir -p <data_dir>/vaults/<new_id>/` and atomically renames the four legacy files:
    - `<data_dir>/index.sqlite` → `<data_dir>/vaults/<new_id>/index.sqlite`
    - `<data_dir>/index.sqlite-wal` → `<data_dir>/vaults/<new_id>/index.sqlite-wal` (if present)
    - `<data_dir>/index.sqlite-shm` → `<data_dir>/vaults/<new_id>/index.sqlite-shm` (if present)
    - `<data_dir>/outbox.jsonl` → `<data_dir>/vaults/<new_id>/outbox.jsonl` (if present)
4. Daemon writes `<data_dir>/vaults/<new_id>/meta.toml` with the registry row's contents (id, name, path, status, created_at) for human readability per ADR-0010.
5. Daemon starts watcher + indexer for the migrated vault. No re-index needed; the moved index is intact.

**Crash safety**: the four `rename(2)` operations are atomic per-file on POSIX. The cross-file consistency window is "registry row inserted, but index files not yet moved." On crash within that window, next-startup reconcile sees:
- Registry has the row (vault_id `X`, path `P`, status `active`).
- `<data_dir>/vaults/<X>/index.sqlite` does not exist.
- `<data_dir>/index.sqlite` still exists.
→ Reconcile detects this state, retries the move idempotently, and proceeds.

If the registry insert succeeded but the legacy `index.sqlite` was already moved by a prior partial run (e.g. crash between step 3 and step 5), reconcile sees:
- Registry has the row.
- `<data_dir>/vaults/<X>/index.sqlite` exists.
- `<data_dir>/index.sqlite` does not exist.
→ Reconcile treats this as already-migrated; just spawns watcher + indexer.

**Why move-and-reconcile over wipe-and-rescan**:
- *No re-index pause on upgrade.* For typical vault sizes (low thousands of files) the re-index is sub-minute, but for larger vaults (tens of thousands of files + chunks + embeddings) it can be many minutes. Move avoids the pause entirely.
- *No data loss.* Outbox history is preserved across the migration (the `outbox.jsonl` move keeps every event since the daemon was first started).
- *Implementation cost is low.* Four atomic renames + the reconcile-detect logic. The renames are stdlib `std::fs::rename` calls.
- *Crash safety is locally reasoned.* Each rename is atomic; the cross-file consistency window has a single deterministic recovery action.

**Why not wipe-and-rescan** (the candidate alternative): destructive, surprising for users, loses outbox history, requires a re-index pause. Reserved as an opt-in future tool (`hmn vault rebuild` post-step-11) for when an index is actually corrupted.

**How to apply**: in Task 9.5, the daemon-startup-sequence-rewrite includes a `legacy_state_migration::run_if_needed()` call before the main reconcile loop. The function is idempotent and crash-safe; the test in Task 9.7 exercises both fresh-from-scratch and migration-from-v0-state paths.

**References**: [ADR-0010 § Reconciliation](../../docs/decisions/0010-vault-definitions-as-runtime-state.md), `std::fs::rename` POSIX semantics.

### C. Top-level `[vault]` config key removal vs deprecation

**Resolution**: **accept-and-warn-and-translate**. The top-level `[vault]` config key continues to parse for backwards-compatibility; its presence at startup logs a deprecation WARN; its `path` field is consumed by the legacy-state migration in Resolution B.

- `default_vault_name: String` is added as a new top-level config key (default `"default"`). Used for naming the auto-migrated legacy vault and (in step 10) for resolving control-plane commands that omit the vault selector.
- After the first successful migration, `vaults.sqlite` is the source of truth; the `[vault]` block in `config.toml` is redundant but not destructive.
- WARN logged at every startup until the operator removes the block: `WARN: top-level [vault] config block is deprecated; vaults are now managed via the registry. Remove the [vault] block from <config-path> when convenient.`

**Why accept-and-warn over hard-remove**:
- Existing v0.1.0 deployments don't break on upgrade.
- The translation logic is small (already needed for the legacy-state migration in Resolution B).
- The persistent WARN gives operators a clear signal without forcing an upgrade-day edit.
- Hard-remove can ship in round 4+ once the round-3 surface stabilizes; the round-3 boundary retro is the natural moment to revisit this.

**Validation rules**:
- If both `[vault]` and a populated `vaults.sqlite` exist at startup, the registry wins; `[vault]` is logged as redundant-and-ignored.
- If `[vault]` is present and `vault.path` is missing or empty, error at startup (matches the v0.1.0 contract — was previously `vault.path` required).
- `default_vault_name` cannot be empty unless explicitly set to `""` (which then requires every step-10+ control-plane command to specify a name or ID; documented in the vault-management spec's edge cases).

**How to apply**: in `src/config.rs`, the existing `[vault]` deserializer stays intact; add a new `default_vault_name: Option<String>` (resolved to `"default"` if absent) at the top level. The startup-sequence in Task 9.5 reads both and feeds them into the legacy-state migration.

**References**: [`src/config.rs`](../../src/config.rs); [`docs/reference/configuration.md`](../../docs/reference/configuration.md).

### D. `vaults.sqlite` schema migration strategy

**Resolution**: **single CREATE TABLE at first run; no migrations module** until a schema change is actually needed. Round-3 is too early to commit to a `vaults.sqlite` migration framework — the registry schema is small, slow-moving, and won't change shape until a concrete need arises.

```sql
-- vaults.sqlite, applied at first run
CREATE TABLE IF NOT EXISTS vaults (
  id          TEXT PRIMARY KEY NOT NULL,        -- UUIDv7, per Resolution A
  name        TEXT NOT NULL UNIQUE,             -- user-facing label
  path        TEXT NOT NULL UNIQUE,             -- absolute, canonicalized
  status      TEXT NOT NULL                     -- 'active' | 'paused' | 'errored'
              CHECK (status IN ('active', 'paused', 'errored')),
  created_at  TEXT NOT NULL,                    -- ISO-8601 µs UTC
  last_error  TEXT
);
```

A `schema_version` row in a `meta` key-value table is added (single row, value `"1"`) so a future schema change can detect the legacy schema and apply migrations. The migrations module itself is deferred until that future change.

**Why single-CREATE-TABLE over the round-1 migrations framework**:
- The `index.sqlite` migration framework (round-1 migrations 0001-0004) is justified by chunks/chunks_vec / a 4-step evolution. `vaults.sqlite` is one table that hasn't changed shape since the canon was written.
- Adding the framework now without a concrete second-migration use-case is speculative complexity. Add it when migration #2 lands.
- The `meta(key, value)` table with `schema_version: "1"` costs ~5 lines of code now and unlocks the framework when needed.

**How to apply**: in Task 9.1, the `vault_registry::open(data_dir)` constructor checks for `vaults.sqlite`; if absent, creates it with the schema above + the `meta` table seeded with `schema_version=1`. If present, asserts `schema_version=1` (and errors with a clear message if not — operator is on a future version that downgraded).

**References**: round-1 migration patterns at `src/store/schema.rs`; ADR-0010's illustrative schema.

### E. Reconcile behavior for errored-or-paused legacy auto-create + zero-vaults daemon behavior

**Resolution**: two distinct cases.

**Case 1 — legacy `[vault]` exists, but path is inaccessible at first startup**:
- Auto-create the registry row with the legacy path.
- Set `status = "errored"` and `last_error = "vault path <P> not accessible: <io-error-text>"`.
- Daemon continues to start; no watcher or indexer for the errored vault.
- Search returns empty results from this vault (the fan-out shape in Resolution F handles the no-active-vaults case naturally).

This matches the spec's lifecycle state machine ([`docs/specs/vault-management.md` § Vault Lifecycle State Machine](../../docs/specs/vault-management.md#vault-lifecycle-state-machine)) — `errored` is a vault state, not a daemon-fail state.

**Case 2 — zero registered vaults AND no legacy `[vault]` config**:
- Daemon starts cleanly; logs WARN: `WARN: no vaults registered. The daemon is idle; populate vaults.sqlite or restore the legacy [vault] config.`
- HTTP/MCP search endpoints return empty results (consistent with "empty vault → empty results"; no special error path).
- This state should be vanishingly rare in practice (only triggers on operator-wiped-config + operator-wiped-registry); step 10's `hmn vault create` will be the natural recovery path.

**Why warn-and-idle over error-and-exit**:
- A fresh install (no config, no registry) shouldn't fail to start — round-1's "skeleton starts and idles cleanly" pattern is the project's foundational UX.
- Operators can recover by editing config OR (post-step-10) running `hmn vault create`.
- Error-and-exit risks bricking a fresh install for an operator who hasn't yet read the upgrade docs.

**How to apply**: in Task 9.5, the startup-sequence-rewrite handles both cases as branches of the same reconcile pass. Tests in Task 9.7 cover both (legacy-config-with-bad-path triggers Case 1; no-config-and-empty-registry triggers Case 2).

**References**: [`docs/specs/vault-management.md` § Operations](../../docs/specs/vault-management.md), [ADR-0010 § Reconciliation](../../docs/decisions/0010-vault-definitions-as-runtime-state.md).

### F. Search fan-out shape (pre-staging cross-vault path)

**Resolution**: introduce a thin per-active-vault iteration in the search handlers. For N=1 (the only state reachable in step 9 absent step-10's `hmn vault create`), behavior is identical to v0.1.0. The fan-out machinery is the same pattern step 10 uses for true multi-vault.

Pseudocode shape (replaces the v0.1.0 single-store-handle pattern):

```rust
async fn handle_search_filesystem(state: ApiState, req: FilesystemQueryJson) -> Result<...> {
    let active_vaults = state.registry.list_active().await?;
    let mut all_results = Vec::new();
    for vault in &active_vaults {
        let vault_results = state.stores.get(&vault.id).search_filesystem(&req).await?;
        for r in vault_results {
            all_results.push(annotate_with_vault(r, vault.id, &vault.name));
        }
    }
    // step-9: N=1, so all_results is just the single vault's slice.
    // step-10: N>=1, with merge/sort/limit semantics resolved per the step-10 workplan.
    Ok(merge_and_truncate(all_results, req.limit))
}
```

**For step 9**:
- The merge_and_truncate function is a passthrough for N=1 (identical truncate-at-limit behavior to v0.1.0).
- Per-result `vault` (id) and `vault_name` are populated on every result. v0 wire bytes change from `vault: <absent>` to `vault: <id>` and `vault_name: <name>`. **This is a wire-shape change**, but it is the same change the round-3 outline commits to in shipping criteria — the fields are documented in the four search specs as v0-forward-compat scaffolding (always-absent in v0); step 9 turns "always-absent" into "always-present-with-the-default-vault-id."
- The four search/event spec amendments (Solo todo 64) are **NOT** in step 9's scope. Spec text continues to say "always absent in v0" through step 9's ship date; step 10's workplan-write phase amends the specs to reflect the populated state. **Until step 10 amends specs, the step-9 daemon's wire output is technically ahead of its specs.** This is acceptable for a single-step boundary because (a) the wire shape itself is forward-compat with v0 consumers (the field was already optional in spec table form), and (b) round 3 is post-v0, where between-round wire changes don't break shipping commitments. Document the gap in Task 9.7's results and at the step-9 boundary retro.
- The outbox writer also starts emitting `vault: <id>` on every event line. Same forward-compat-by-spec, same step-9-ahead-of-spec footnote.

**For step 10** (preview, not in step-9 scope): the `merge_and_truncate` step is where the cross-vault semantics resolutions land — ordering, pagination, fan-out execution model, partial-failure handling, paused/errored vault inclusion, semantic-search global top-N. The step-9 implementation should structure the iteration to make those plug-in points obvious (per-vault future, gather, then merge — not interleaved-in-place).

**How to apply**: in Task 9.5 (or its successor — the search handler refactor), introduce the `ApiState.stores: HashMap<VaultId, Arc<Store>>` shape and the fan-out loop. Test the N=0, N=1, and N=2 cases (the N=2 case is reachable in tests via direct registry insertion, even though the user surface for N=2 doesn't ship until step 10).

**References**: existing `src/api/handlers.rs`, [`docs/specs/filesystem-search.md` § Behavior](../../docs/specs/filesystem-search.md), [`docs/specs/content-search.md` § Behavior](../../docs/specs/content-search.md), [`docs/specs/semantic-search.md` § Behavior](../../docs/specs/semantic-search.md).

---

## Self-review for prose accuracy

Per the round-1 boundary heuristic (and round-2 step-8 § Self-review), workplans projected to land near or above ~1000 lines warrant a self-review pass for testable claims about external library semantics. This workplan is projected at ~700–900 lines (smaller than step-8's ~1100); the heuristic doesn't strictly fire, but the agent runs a voluntary spot-check on the three claims of this shape:

1. **`std::fs::rename` is atomic on POSIX** (Resolution B) — verified against [POSIX `rename(2)`](https://pubs.opengroup.org/onlinepubs/9699919799/functions/rename.html) for same-filesystem moves. The legacy-state migration assumes `<data_dir>/index.sqlite` and `<data_dir>/vaults/<id>/index.sqlite` are on the same filesystem (both under `<data_dir>`); this is true unless the operator has done an unusual mount-bind setup. Document the same-filesystem assumption in Task 9.5.
2. **`uuid` crate v7 feature emits time-ordered IDs that sort lexicographically by creation time** (Resolution A) — verified at task time against [`uuid` v1.10 docs](https://docs.rs/uuid/latest/uuid/) (`Uuid::now_v7()` documentation). Task 9.1 confirms with a unit test that two consecutive `VaultId::new()` calls produce IDs where the first sorts before the second.
3. **SQLite `CHECK (status IN ('active', 'paused', 'errored'))` constraint is enforced on UPDATE** (Resolution D) — basic SQLite behavior, but worth a unit test in Task 9.1 that an UPDATE with an invalid status returns an error.

The cross-platform-rename claim (Windows uses `MoveFileEx` semantics with `MOVEFILE_REPLACE_EXISTING`, which is also atomic at the file level) is documented but not exercised in step 9; macOS / Linux are the supported platforms per round-1's framing. If a Windows user ships through this round, the migration path needs review at the time of report.

---

## Tasks

The 8-task decomposition follows the round-1/2 pattern (default-not-batch; 8-task density matches step 5 and step 6). Each task ships its own commit per the playbook's TASK AGENT § Reporting; risk grades and dependencies noted at each task header.

### Task 9.1 — `src/vault_registry/` module + `vaults.sqlite` schema + CRUD

**Risk**: medium. Pure-logic surface (new module, no external state beyond a SQLite file); test surface is well-bounded. **Load-bearing for downstream tasks 9.2-9.5** — every later task uses `VaultId` and the registry interface.

**Scope**:
- New module: `src/vault_registry/{mod.rs,schema.rs}`.
- New top-level Cargo dep: `uuid = { version = "1.10", features = ["v7", "serde"] }` (Resolution A; MSRV verified at task time).
- Public surface (sketch):
  ```rust
  pub struct VaultRegistry { pool: r2d2::Pool<...> }
  pub struct VaultId(String);  // UUIDv7
  pub struct VaultRow { id: VaultId, name: String, path: PathBuf, status: VaultStatus, created_at: DateTime<Utc>, last_error: Option<String> }
  pub enum VaultStatus { Active, Paused, Errored }
  
  impl VaultRegistry {
    pub fn open(data_dir: &Path) -> Result<Self, ...>;             // opens vaults.sqlite, creates schema if absent
    pub async fn list(&self) -> Result<Vec<VaultRow>, ...>;
    pub async fn list_active(&self) -> Result<Vec<VaultRow>, ...>;
    pub async fn get_by_id(&self, id: &VaultId) -> Result<Option<VaultRow>, ...>;
    pub async fn get_by_name(&self, name: &str) -> Result<Option<VaultRow>, ...>;
    pub async fn insert(&self, row: VaultRow) -> Result<(), ...>;  // INSERT (id, name, path, status, created_at)
    pub async fn update_status(&self, id: &VaultId, status: VaultStatus, last_error: Option<&str>) -> Result<(), ...>;
    // No delete in step 9 — terminate is a step-10/11 control-plane operation.
  }
  ```
- Schema per Resolution D (single CREATE TABLE + `meta(key, value)` with `schema_version=1`).
- Per the [`rusqlite-in-async`](../../.claude/skills/rusqlite-in-async/SKILL.md) skill: every public async method wraps `spawn_blocking` around the `rusqlite` call; no async fn calls `rusqlite` directly.

**Tests** (in-module unit tests):
- `vaults_sqlite_created_with_schema_at_first_open` — open against an empty data_dir; verify table + meta row.
- `open_against_existing_schema_succeeds` — open twice; second open is a no-op.
- `open_against_wrong_schema_version_errors` — manually set `meta.schema_version = "2"`; assert open errors clearly.
- `insert_and_list_roundtrip` — insert two rows; list returns both in creation order (UUIDv7 lexicographic = creation-time).
- `insert_with_duplicate_name_errors` — UNIQUE constraint on `name`.
- `insert_with_duplicate_path_errors` — UNIQUE constraint on `path`.
- `update_status_to_invalid_value_errors` — CHECK constraint on `status` (per § Self-review item 3).
- `list_active_filters_paused_and_errored` — insert one of each; assert `list_active` returns only the active one.
- `vault_id_new_returns_time_ordered_uuids` — two consecutive `VaultId::new()` calls; assert lexicographic order matches creation order (per § Self-review item 2).

**Files touched**: `Cargo.toml`, `Cargo.lock`, `src/lib.rs` (`pub mod vault_registry;`), `src/vault_registry/mod.rs` (new), `src/vault_registry/schema.rs` (new).

**Dependencies**: none upstream; this is the foundation task.

### Task 9.2 — `src/store/` per-vault refactor

**Risk**: medium-high. Refactor of the existing public store API; behavior must be preserved for existing callers (indexer, search handlers, smoke tests). **Load-bearing for tasks 9.3, 9.5, 9.6, 9.7.**

**Scope**:
- Refactor `src/store/` so that opening a `Store` is keyed by vault_id + per-vault data directory.
- The path shape changes from `<data_dir>/index.sqlite` (v0.1.0) to `<data_dir>/vaults/<vault_id>/index.sqlite`.
- Public-API impact: the `Store::open(config)` constructor signature changes from "open the global store" to "open a per-vault store given a vault_id + data_dir." Callers (currently the daemon's startup code, the indexer, search handlers) update to pass a vault_id.
- The four migrations (0001-0004) of the existing schema continue to apply per-vault; no schema change in step 9.
- Connection pool: each per-vault `Store` holds its own `r2d2::Pool<SqliteConnectionManager>` (the round-1/2 pattern). The daemon's startup-sequence (Task 9.5) constructs a `HashMap<VaultId, Arc<Store>>`.

**Tests** (in-module unit tests + existing integration tests):
- All existing `src/store/` unit tests pass with the per-vault path shape.
- New unit test: `two_stores_at_different_vault_ids_are_independent` — open two stores; insert different files; verify isolation.

**Files touched**: `src/store/mod.rs`, `src/store/schema.rs`, `src/store/files.rs`, `src/store/chunks.rs`, any other module-internal files that reference the path shape.

**Dependencies**: 9.1 (uses `VaultId`).

### Task 9.3 — `src/indexer/` per-vault refactor

**Risk**: medium. Refactor follows the same shape as 9.2; the indexer takes a per-vault Store handle.

**Scope**:
- Refactor `src/indexer/` so the indexer is constructed per-vault: `Indexer::new(vault_id, store, embedder, config)` (or its post-resolution shape).
- The per-vault indexer owns its own `tokio::sync::watch` channel for shutdown signaling (consistent with the round-1 step-3 pattern).
- The `EmbeddingClient` (or its post-step-6 shape) is shared across vaults — one per daemon, multiple indexer instances point to it. This matches the [ADR-0009 § Negative consequences](../../docs/decisions/0009-multi-vault-per-daemon.md) note that the embedding service is shared-pool-tuned in multi-vault setups.
- Per-vault outbox writer (Task 9.4) is wired to the indexer at construction.

**Tests** (in-module unit tests):
- All existing `src/indexer/` unit tests pass with the per-vault construction.
- New unit test: `indexer_writes_to_correct_per_vault_store` — construct two indexers, two stores, verify writes go to the right one.

**Files touched**: `src/indexer/mod.rs`, any other module-internal files.

**Dependencies**: 9.1, 9.2.

### Task 9.4 — `src/watcher/` + `src/outbox/` per-vault refactor

**Risk**: medium-high. The watcher refactor touches the project's biggest landmines (per the round-1 step-3 retro): debouncer behavior, sync-conflict patterns, backpressure. Per-vault means N concurrent watchers + N concurrent outbox writers; coordination across them is the new failure surface.

**Scope**:
- Refactor `src/watcher/` so a `Watcher` is constructed per-vault: `Watcher::new(vault_id, vault_path, indexer_tx, config)`.
- Per-vault outbox writer in `src/outbox/`: `OutboxWriter::new(vault_id, outbox_path)` writing to `<data_dir>/vaults/<vault_id>/outbox.jsonl`.
- The `notify` watcher itself is per-vault (one watcher per vault root); no shared watcher across vaults.
- The debouncer is per-vault (preserves the round-1 invariants — no cross-vault event coalescing).
- Sync-conflict filtering at the watcher boundary remains per-vault per the [`filesystem-watching` skill](../../.claude/skills/filesystem-watching/SKILL.md).
- The outbox event shape gains a `vault: <id>` field on every event; `vault_name` is **not** added (per ADR-0009 — outbox is durable, names rot).
- The per-vault `OutboxWriter` flushes to its own file with the same `sync_data` per-event policy as v0.1.0 (round-1 step-4 resolution).

**Tests** (in-module unit tests + existing integration tests via 9.7):
- All existing `src/watcher/` and `src/outbox/` unit tests pass per-vault.
- New unit test: `two_watchers_emit_to_separate_outboxes` — construct two watchers + two outbox writers; verify event isolation.
- New unit test: `outbox_event_carries_vault_id` — assert the JSON line includes `"vault":"<id>"`.

**Files touched**: `src/watcher/mod.rs`, `src/watcher/translate.rs`, `src/outbox/mod.rs`, `src/outbox/event.rs`, `src/outbox/writer.rs`.

**Dependencies**: 9.1.

### Task 9.5 — Daemon startup-sequence rewrite + reconcile + legacy-state migration + manual smoke

**Risk**: medium-high. Composes 9.1-9.4 + new reconcile + new migration. **Manual smoke verification is load-bearing here** per the round-2 step-7 / step-8 precedent for medium-high-risk wiring tasks.

**Scope**:
- Rewrite `src/bin/hmnd.rs::main` to use the new sequence:
  1. Parse config (existing).
  2. Open `vault_registry` (Task 9.1).
  3. Run legacy-state migration if needed (Resolution B): detect pre-existing `<data_dir>/index.sqlite` + populated `[vault]` config + empty `vaults.sqlite`; insert one row, atomic-rename the four legacy files into `<data_dir>/vaults/<id>/`, write `meta.toml`.
  4. Reconcile registry against filesystem (Resolution E): for each registered vault, verify `<data_dir>/vaults/<id>/` exists and `path` is accessible; transition to `errored` with `last_error` if not.
  5. For each `active` vault, construct `Store` (9.2) + `Indexer` (9.3) + `Watcher` + `OutboxWriter` (9.4) + start the per-vault background tasks.
  6. Construct `ApiState` with `HashMap<VaultId, Arc<Store>>` for fan-out (Resolution F).
  7. Start Axum HTTP server (existing) with the fan-out-aware handlers.
- The fan-out shape in search handlers (Resolution F) lands here. Handler iteration over `state.registry.list_active()`, per-vault search, gather, merge_and_truncate. For N=1 the merge is a passthrough.
- The outbox event emission in the indexer (per Task 9.4) emits `vault: <id>` on every event line. (This is a wire-shape addition; spec text amendment lands in step 10 per Resolution F's "step-9 ahead of spec" footnote.)

**Manual smoke verification** (per round-2 lessons; bundled into this task):
1. **Fresh-install smoke**: empty `<data_dir>`, no `[vault]` config. Daemon starts, logs the `WARN: no vaults registered` line (Resolution E Case 2), idles. `curl http://127.0.0.1:7777/health` returns 200. `curl http://127.0.0.1:7777/search/filesystem -d '{}'` returns `{"results":[],"truncated":false}`.
2. **Legacy-config smoke**: empty `<data_dir>` aside from a pre-existing `<data_dir>/index.sqlite` populated from a v0.1.0 run, plus a top-level `[vault]` config block. Daemon starts, runs legacy-state migration, logs the deprecation WARN, idles. `<data_dir>/vaults/<uuid>/index.sqlite` exists; `<data_dir>/index.sqlite` does not. `curl http://127.0.0.1:7777/search/filesystem` returns the v0.1.0 result set unchanged (modulo the new `vault`/`vault_name` fields per Resolution F).
3. **Errored-vault smoke**: legacy `[vault]` config points to a non-existent path. Daemon starts, auto-creates the registry row in `errored` status, logs `last_error`, idles. Search returns empty results (no panic, no error response).
4. **Crash-recovery smoke** (manual SIGKILL): start daemon mid-migration (insert a sleep into the legacy-state migration for testing); SIGKILL during the `rename` window; restart; verify reconcile picks up cleanly. (This is hand-instrumented; the test in Task 9.7 exercises the same path with deterministic state setup.)

Document each smoke run's transcript in the task's results comment per the round-2 step-7/8 precedent.

**Files touched**: `src/bin/hmnd.rs`, `src/api/handlers.rs` (fan-out refactor), `src/api/state.rs` (or its equivalent — `ApiState` construction), `src/lib.rs` (re-export `vault_registry`).

**Dependencies**: 9.1, 9.2, 9.3, 9.4.

### Task 9.6 — Search response wiring: `vault` (id) + `vault_name` populated

**Risk**: low. Mostly serde plumbing on top of 9.5's fan-out shape.

**Scope**:
- In the four search response types (`FilesystemSearchResponse`, `ContentSearchResponse`, `SemanticSearchResponse`, plus per-result shapes), confirm the `vault` field continues to serialize when populated and that a new `vault_name` field is added.
- The wire shape goes from `vault: <absent>` (v0.1.0) to `vault: <id>, vault_name: <name>` (step 9 onward). Per Resolution F: this is a forward-compat-by-spec-table change but spec text amendment is deferred to step 10.
- The same plumbing is also exercised on outbox events (per Task 9.4), but the outbox carries `vault` only — no `vault_name`.

**Tests** (extend existing unit tests in `src/api/types.rs` and `src/outbox/event.rs`):
- `filesystem_search_response_serializes_vault_and_vault_name` — populated case.
- `content_search_response_serializes_vault_and_vault_name` — populated case.
- `semantic_search_response_serializes_vault_and_vault_name` — populated case.
- `outbox_event_serializes_vault_id_only` — assert `vault_name` is absent.

**Files touched**: `src/api/types.rs`, `src/outbox/event.rs`, possibly `src/api/handlers.rs` if the annotation site is in handlers.

**Dependencies**: 9.5.

### Task 9.7 — Integration tests + behavior-preservation gate

**Risk**: medium. Wide-surface integration testing; flake budget is tight because the per-vault refactor introduces new timing axes (multiple concurrent watchers, multiple SQLite databases, fan-out merge in search handlers).

**Scope**:
- All existing integration tests (`tests/scan.rs`, `tests/watch.rs`, `tests/outbox.rs`, `tests/embedding.rs`, `tests/mcp.rs`, plus skeleton/config tests) pass unchanged. Test fixtures use the legacy `[vault]` config shape; the daemon auto-creates one vault under the hood; tests don't need to know about vaults.
- New integration test file: `tests/multi_vault_internal.rs` (or similar). Tests exercise the per-vault internals via direct registry manipulation (since step 10 is the user surface). Tests:
  - `two_vaults_index_in_isolation` — directly insert two registry rows; confirm two stores, two outboxes; modify a file in vault A; confirm only vault A's outbox emits an event.
  - `cross_vault_search_returns_intermingled_results_with_vault_id` — set up two vaults, insert distinct files in each; query `/search/filesystem`; confirm results include both vaults' files with correct `vault` and `vault_name` annotations.
  - `legacy_state_migration_preserves_index` — set up `<data_dir>/index.sqlite` and `<data_dir>/outbox.jsonl` with v0.1.0-shape data; start daemon; confirm migration completes and the migrated index is intact.
  - `migration_idempotent_under_crash_window` — simulate the crash-mid-rename state (registry row inserted, `<data_dir>/index.sqlite` still exists, target subdir missing); restart daemon; confirm reconcile completes cleanly.
  - `errored_vault_returns_empty_search_results` — insert a registry row with a non-existent path; start daemon; query search; assert empty results, no panic.
- 3× consecutive flake-check clean run per the round-1/2 anti-flake convention.

**Files touched**: `tests/multi_vault_internal.rs` (new), possibly small adjustments to existing test helpers in other `tests/*.rs` files if the daemon-startup invocation shape changed.

**Dependencies**: 9.1-9.6.

### Task 9.8 — Reference docs reflect step-9 resolutions

**Risk**: low. Doc-only by design; lands at boundary so any forward-noted soft-flag corrections from earlier tasks can be incorporated.

**Scope**:
- `docs/reference/configuration.md`: document `default_vault_name` (default `"default"`); document the `[vault]` deprecation warning; document the per-vault data layout (`<data_dir>/vaults/<id>/`).
- `docs/reference/cli.md`: no new CLI in step 9, but the existing `hmn` invocations work against per-vault internals — verify no doc claims about single-vault-only behavior remain.
- `docs/architecture/overview.md`: update § Storage and § Search API to describe the per-vault shape; note that cross-vault search semantics are pinned in step 10.
- **NOT** in scope: `docs/specs/filesystem-search.md`, `docs/specs/content-search.md`, `docs/specs/semantic-search.md`, `docs/specs/change-events.md` — these are amended in step 10's workplan-write per Solo todo 64. The "step-9 daemon is ahead of its specs" gap is documented in Task 9.7's results comment for boundary review, not closed in step 9.
- **NOT** in scope: `docs/specs/vault-management.md` — fleshed in step 10 via Solo todo 65.

**Files touched**: `docs/reference/configuration.md`, `docs/reference/cli.md`, `docs/architecture/overview.md`.

**Dependencies**: 9.1-9.7. Lands last.

---

## Shipping criteria

The step ships when **all** of these hold:

- [ ] All existing integration tests pass unchanged: `tests/scan.rs`, `tests/watch.rs`, `tests/outbox.rs`, `tests/embedding.rs`, `tests/mcp.rs`, plus skeleton/config tests. Tests use the legacy `[vault]` config shape; the daemon auto-creates one vault under the hood.
- [ ] `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test` all green.
- [ ] Fresh `hmnd` against the legacy `[vault]` config produces exactly one row in `vaults.sqlite`, exactly one subdirectory under `<data_dir>/vaults/<id>/` with `index.sqlite`, `outbox.jsonl`, `meta.toml`.
- [ ] `hmn search filesystem`, `hmn search content`, `hmn search semantic`, `hmn status`, and outbox tailing all return identical results to v0.1.0 against the same vault contents (modulo the new populated `vault` and `vault_name` fields in search responses; outbox events gain `vault` only).
- [ ] Per-result `vault` and `vault_name` are populated on every search response (confirmed via integration tests in Task 9.7).
- [ ] Outbox events carry `vault` (id) on every line; no `vault_name` field on outbox lines.
- [ ] Legacy-state migration runs idempotently and is crash-safe under the rename-window kill scenario (confirmed in Task 9.7's `migration_idempotent_under_crash_window` test).
- [ ] `default_vault_name` config key works (default `"default"`); top-level `[vault]` block is accepted-and-warned at startup.
- [ ] All four manual smoke scenarios in Task 9.5 produce the documented outputs.
- [ ] 3× consecutive flake-check clean run on `cargo test`.
- [ ] Reference docs (configuration, cli, architecture) reflect the per-vault layout; spec amendments are explicitly **deferred to step 10** with a one-line forward-pointer in the relevant doc sections.
- [ ] One commit per task per the playbook (Task 9.5 may use the round-2-step-8 two-commit pattern for separability if the legacy-state migration is naturally separable from the startup-sequence rewrite).

## Step boundary follow-ups (anticipated)

- **Spec amendments deferred to step 10**: the four search/event specs say "always absent in v0" through step 9's ship date but the daemon emits populated `vault` and `vault_name` fields. Step 10's workplan-write phase (Solo todo 64 + 65) closes the gap.
- **Cross-platform rename safety**: Resolution B assumes same-filesystem renames; cross-mount setups would error. Document the assumption in `docs/reference/configuration.md`. If a Windows user surfaces, revisit at the report time.
- **Wipe-and-rebuild as a future tool**: Resolution B explicitly defers the wipe path to a future `hmn vault rebuild` (post-step-11) for when an index is corrupted. Note for backlog.
- **Hard-removal of `[vault]` config block**: Resolution C accepts-and-warns; round-3 boundary retro is the natural moment to schedule the hard-removal in round 4+.
