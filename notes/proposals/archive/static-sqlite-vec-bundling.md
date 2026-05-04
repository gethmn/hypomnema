# Static sqlite-vec Bundling Specification

**Version**: 0.1.0
**Date**: 2026-05-02
**Status**: Draft

---

## Overview

Static sqlite-vec bundling makes Hypomnema's semantic index runtime self-contained: a user who installs the `hypomnema` crate gets runnable `hmn` and `hmnd` binaries without separately downloading `sqlite-vec.so`, `sqlite-vec.dylib`, or `sqlite-vec.dll`.

The vector-store choice does not change. Hypomnema still uses sqlite-vec inside the daemon's SQLite connection pool, with the same `chunks_vec` schema, dimension validation, and delete-then-reinsert indexing behavior. This spec changes only how sqlite-vec reaches each SQLite connection: from operator-provisioned loadable extension to statically linked extension registered by the process before store connections open.

**Related Documents**:
- [ADR-0002: Rust over Python](../../docs/decisions/0002-rust-over-python.md)
- [ADR-0005: Local Everything](../../docs/decisions/0005-local-everything.md)
- [ADR-0007: sqlite-vec over Lance, qdrant, and Other Vector Stores](../../docs/decisions/0007-sqlite-vec-over-alternatives.md)
- [ADR-0008: Two Binaries (hmnd + hmn) in One Crate](../../docs/decisions/0008-two-binary-daemon-plus-cli.md)
- [Architecture: Store](../../docs/architecture/overview.md#system-components)
- [Reference: Configuration](../../docs/reference/configuration.md#embedding)
- [CI Pipeline](../../docs/specs/ci-pipeline.md)

---

## Behavior

### Normal Flow

1. `cargo install hypomnema` builds the crate and installs both binaries exposed by the package: `hmnd` and `hmn`.
2. The build compiles sqlite-vec into the Hypomnema binary link graph through a pinned Rust dependency that vendors/builds the sqlite-vec C source.
3. Before any store connection pool is created, Hypomnema registers sqlite-vec's static init function with SQLite.
4. Each new rusqlite connection opened by the store can create and query `vec0` virtual tables without `load_extension` and without any filesystem path to a native sqlite-vec library.
5. `hmnd` startup, vault creation, migrations, semantic indexing, and semantic search behave as they did with the dynamic extension.
6. The daemon no longer fails startup because `~/.local/share/hypomnema/sqlite-vec.<ext>` is missing.

### State Machine

**State Machine**: N/A - this feature is a packaging and store-initialization change. The daemon and vault lifecycle states are unchanged.

---

## Data Schema

### Cargo Dependency Shape

```yaml
package: hypomnema
dependencies:
  rusqlite:
    features:
      - bundled
  sqlite-vec:
    version: "=0.1.10-alpha.3"
    purpose: "static sqlite-vec C extension build and init symbol"
removed_dependency_behavior:
  rusqlite_load_extension_feature: "not required by default"
```

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `package` | string | yes | `hypomnema` | The Cargo package installed by users. |
| `dependencies.rusqlite.features` | array of strings | yes | `["bundled"]` | SQLite remains bundled into the Rust build. `load_extension` is not required for the default path once sqlite-vec is static. |
| `dependencies.sqlite-vec.version` | exact semver string | yes | none | The upstream sqlite-vec Rust crate version. Pin exactly while sqlite-vec remains pre-v1. |
| `dependencies.sqlite-vec.purpose` | string | yes | none | Records why the crate exists in the graph: static extension build and init symbol, not a new vector-store abstraction. |
| `removed_dependency_behavior.rusqlite_load_extension_feature` | string | no | `not required by default` | The dynamic-extension feature is removed from the ordinary runtime path unless a future workplan deliberately keeps a dev-only escape hatch. |

### Runtime Configuration Shape

```yaml
embedding:
  endpoint: "http://127.0.0.1:8080/v1/embeddings"
  model: "nomic-embed-text-v1.5"
  dimension: 768
  api_key: ""
  timeout_ms: 30000
  max_retries: 1
  batch_size: 1
removed:
  extension_path: "~/.local/share/hypomnema/sqlite-vec.<ext>"
  env_override: "HYPOMNEMA_VEC_EXT_PATH"
```

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `embedding.endpoint` | URL string | yes | `http://127.0.0.1:8080/v1/embeddings` | OpenAI-compatible embedding endpoint; unchanged by this spec. |
| `embedding.model` | string | yes | `nomic-embed-text-v1.5` | Embedding model name sent to the endpoint; unchanged. |
| `embedding.dimension` | integer | yes | `768` | Must match the schema-baked vector dimension; unchanged. |
| `embedding.api_key` | string | no | `""` | Optional bearer token; unchanged. |
| `embedding.timeout_ms` | integer | no | `30000` | Per-request embedding timeout; unchanged. |
| `embedding.max_retries` | integer | no | `1` | Embedding transport retry count; unchanged. |
| `embedding.batch_size` | integer | no | `1` | Embedding request batch size; unchanged. |
| `removed.extension_path` | path string | no | none | Removed from the normal configuration surface because sqlite-vec is no longer operator-provisioned. |
| `removed.env_override` | env-var name | no | none | Removed from the normal runtime contract unless a future workplan explicitly keeps a compatibility escape hatch. |

### Validation Rules

- The sqlite-vec crate version must be pinned exactly while upstream is pre-v1.
- Store startup must prove sqlite-vec is available through a connection-level probe such as `SELECT vec_version()`.
- Existing schema validation remains authoritative: `embedding.dimension` must still match `chunks_vec`'s `FLOAT[<dim>]`.
- A fresh install must not require `embedding.extension_path`, `HYPOMNEMA_VEC_EXT_PATH`, or a file named `sqlite-vec.<ext>`.

---

## Examples

### Example 1: `cargo install` Fresh Install

**Input**:
```yaml
command: "cargo install hypomnema"
post_install_files:
  - "~/.cargo/bin/hmn"
  - "~/.cargo/bin/hmnd"
absent_files:
  - "~/.local/share/hypomnema/sqlite-vec.so"
  - "~/.local/share/hypomnema/sqlite-vec.dylib"
  - "~/.local/share/hypomnema/sqlite-vec.dll"
```

**Behavior**: The installed `hmnd` binary registers sqlite-vec statically before opening any vault store.

**Result**: The daemon can create/open a vault store and apply migrations that create `chunks_vec USING vec0(...)` without asking the operator to install a sqlite-vec dynamic library.

### Example 2: Existing Config Contains `extension_path`

**Input**:
```yaml
config:
  embedding:
    endpoint: "http://127.0.0.1:8080/v1/embeddings"
    model: "nomic-embed-text-v1.5"
    dimension: 768
    extension_path: "~/.local/share/hypomnema/sqlite-vec.dylib"
```

**Behavior**: Compatibility behavior is a workplan-time choice. The preferred path is a one-release deprecation window where the key is accepted but ignored with a warning, then removed from the accepted config schema.

**Result**: Existing users get a clear message that sqlite-vec is now bundled and the config key is obsolete; new users never see the key.

---

## Edge Cases

### Static Registration Happens Too Late

**Scenario**: Store code opens a rusqlite connection before sqlite-vec's static init has been registered.

**Behavior**: The first migration or probe that touches `vec0` fails. Tests must cover the actual daemon/store path, not only a direct in-memory connection.

**Rationale**: Registration order is the main new correctness boundary. A compile-time dependency is not enough; the extension must be visible to every connection that can run Hypomnema migrations or semantic queries.

### `hmn` Links sqlite-vec Even Though It Does Not Open Stores

**Scenario**: Both binaries share the same crate, and adding sqlite-vec to the shared dependency graph may make `hmn` link native sqlite-vec code even though only `hmnd` needs store access.

**Behavior**: This is acceptable unless measured release binaries grow enough to justify feature-gating. Do not introduce a workspace split or binary-specific dependency architecture as part of this spec.

**Rationale**: ADR-0008 explicitly preserves the single-crate, two-binary shape. A small amount of binary weight is a better trade than packaging friction or premature crate factoring.

### Cross-Compilation and Toolchain Availability

**Scenario**: `cargo install hypomnema` runs on a host without a working C compiler or with an unsupported target/toolchain combination.

**Behavior**: Build failure is surfaced by Cargo/cc at install time. Hypomnema should document the C-toolchain prerequisite for source installs if upstream sqlite-vec requires it.

**Rationale**: Static bundling shifts failure from daemon startup to install/build time. That is a better user boundary for `cargo install`, but it may add source-build prerequisites that binary package managers hide.

### Upstream sqlite-vec API Changes

**Scenario**: The upstream `sqlite-vec` Rust crate changes its exported init symbol, build script behavior, or C source layout before 1.0.

**Behavior**: Hypomnema pins the exact crate version and updates deliberately through normal dependency-maintenance work.

**Rationale**: This preserves reproducibility while still allowing future upgrades when there is a reason to take them.

---

## Error Handling

| Error Condition | Error Code/Type | Message | Recovery |
|-----------------|-----------------|---------|----------|
| Static sqlite-vec registration fails | Startup error via `anyhow` context | `registering statically linked sqlite-vec extension failed` | Treat as a packaging/build bug; report version, platform, and build method. |
| SQLite connection lacks sqlite-vec after registration | Startup error via `anyhow` context | `sqlite-vec probe failed: vec_version() is unavailable` | Fix registration order or dependency linkage; no operator-installed dylib should be requested. |
| Existing config still contains `embedding.extension_path` during deprecation window | Warning log | `embedding.extension_path is ignored; sqlite-vec is bundled into hmnd` | Remove the obsolete config key. |
| Existing config contains `embedding.extension_path` after removal from schema | Config parse error | `unknown field 'extension_path' in [embedding]; sqlite-vec is bundled` | Remove the obsolete config key. |
| `cargo install` host lacks a C compiler required by sqlite-vec's build script | Cargo build error | emitted by Cargo/cc | Install the platform C toolchain or use a prebuilt binary/package-manager install. |

---

## Integration Points

### With Cargo Builds

Cargo builds sqlite-vec as part of Hypomnema's dependency graph. The exact version is pinned. Source installs (`cargo install hypomnema`) may require the platform C toolchain that the sqlite-vec crate's `cc` build uses.

### With Store Initialization

The store registers sqlite-vec once per process before building any r2d2 pool. Connection initialization keeps WAL and synchronous pragmas, but no longer enables dynamic extension loading or loads a filesystem path.

**Data Flow**:
```text
Cargo dependency sqlite-vec
  -> linked sqlite3_vec_init symbol
  -> process-level SQLite registration
  -> r2d2 opens rusqlite connections
  -> migrations/search use vec0
```

### With Configuration

The `[embedding]` section keeps endpoint/model/dimension/service behavior. `extension_path` and `HYPOMNEMA_VEC_EXT_PATH` leave the normal user-facing surface because sqlite-vec is not operator-provisioned.

### With CI

CI no longer downloads sqlite-vec release tarballs into `~/.local/share/hypomnema`. The test matrix validates that the statically linked extension works on Linux and macOS by running the existing tests plus a direct sqlite-vec probe through the production store path.

### With Release Packaging

Package-manager installers and future `gethmn.io` scripts no longer need to install a native sqlite-vec file beside or under the data directory. Prebuilt release artifacts still ship two binaries (`hmn`, `hmnd`); sqlite-vec is included by static linkage.

---

## Implementation Notes

Use SQLite's statically linked extension mechanism rather than extracting a dynamic library at first run. The upstream `sqlite-vec` crate currently exposes the `sqlite3_vec_init` symbol and demonstrates registering it with `rusqlite::ffi::sqlite3_auto_extension`; verify this against the pinned crate version during the workplan.

Keep the existing sqlite-vec table and query patterns. This spec does not introduce a vector-store abstraction, a workspace split, a new semantic-search schema, or a model-switching feature.

Negative fingerprints after implementation:

```sh
rg "load_extension|load_extension_enable|load_extension_disable" src
rg "extension_path|HYPOMNEMA_VEC_EXT_PATH|VEC_EXT_PATH_ENV" src docs/reference docs/specs .github
rg "Install sqlite-vec extension|sqlite-vec-.*loadable|vec0\\.(so|dylib|dll)" .github docs notes
```

Each grep should return zero matches in active code and current canon, except archived notes that intentionally preserve history.

---

## Open Questions

- [ ] Should `embedding.extension_path` receive a deprecation window or be removed immediately? Preferred answer: one-release warning if there are existing external users; immediate removal is acceptable before public release.
- [ ] Should Hypomnema keep a compile-time feature for dynamic sqlite-vec loading for development/debugging? Preferred answer: no, unless a concrete maintainer workflow needs it.
- [ ] Which exact sqlite-vec crate version should ship in the first implementation? The current candidate is `0.1.10-alpha.3`; verify before workplan/task execution.
- [ ] Should `hmn` binary size be measured before and after? Preferred answer: yes, but gate only if growth is materially annoying; do not split crates preemptively.

---

## Revision History

| Version | Date | Changes |
|---------|------|---------|
| 0.1.0 | 2026-05-02 | Initial draft proposing statically linked sqlite-vec as the default packaging/runtime path. |
