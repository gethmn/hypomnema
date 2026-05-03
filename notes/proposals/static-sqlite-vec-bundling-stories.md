# Static sqlite-vec Bundling User Stories

This story set accompanies [static-sqlite-vec-bundling.md](./static-sqlite-vec-bundling.md). The spec defines the behavior boundary; these stories define workplan-ready delivery slices.

## Story 1: Source Install Produces Runnable Binaries

**Story:** As the user, I want `cargo install hypomnema` to install runnable `hmn` and `hmnd` binaries so that I do not have to manually provision sqlite-vec before starting the daemon.

**Acceptance Criteria:**
- [ ] Given a clean machine with Rust/Cargo and no `~/.local/share/hypomnema/sqlite-vec.{so,dylib,dll}`, `cargo install hypomnema` builds and installs `hmn` and `hmnd`.
- [ ] Given the installed `hmnd`, opening a fresh vault store applies migrations that create `chunks_vec USING vec0(...)` without a sqlite-vec dynamic library on disk.
- [ ] Given a production store connection, `SELECT vec_version()` returns a sqlite-vec version string through the same connection path used by daemon startup.
- [ ] The daemon startup error path no longer includes `sqlite-vec extension binary not found`.

## Story 2: Store Initialization Uses Static sqlite-vec Registration

**Story:** As the user, I want Hypomnema to register sqlite-vec inside the binary before it opens database connections so that every vault store has vector search support consistently.

**Acceptance Criteria:**
- [ ] Store pool initialization retains WAL and `synchronous=NORMAL` pragmas while removing dynamic extension loading calls.
- [ ] A test that opens two independent store connections verifies `vec_version()` succeeds on both, proving registration applies to every new connection.
- [ ] Existing semantic-search tests pass without setting `HYPOMNEMA_VEC_EXT_PATH` or writing any sqlite-vec library file to the data directory.
- [ ] Negative fingerprint: `rg "load_extension|load_extension_enable|load_extension_disable" src` returns zero matches.

## Story 3: Obsolete sqlite-vec Configuration Is Removed or Deprecated

**Story:** As the user, I want configuration to stop asking for sqlite-vec's filesystem path so that the config file reflects the self-contained install model.

**Acceptance Criteria:**
- [ ] The generated/default `[embedding]` config no longer contains `extension_path`.
- [ ] `HYPOMNEMA_VEC_EXT_PATH` is no longer required for tests, manual smoke, or daemon startup.
- [ ] If a deprecation window is chosen, a config containing `embedding.extension_path` starts successfully and logs a warning that the key is ignored because sqlite-vec is bundled.
- [ ] If immediate removal is chosen, a config containing `embedding.extension_path` fails with an actionable unknown-field message that says sqlite-vec is bundled.
- [ ] Negative fingerprint: `rg "extension_path|HYPOMNEMA_VEC_EXT_PATH|VEC_EXT_PATH_ENV" src docs/reference docs/specs .github` returns zero matches, except for intentional deprecation-handling code if the deprecation window is selected.

## Story 4: CI Validates Bundled sqlite-vec Without Downloading Artifacts

**Story:** As the user, I want CI to test the same bundled sqlite-vec path that source installs use so that packaging regressions fail before release.

**Acceptance Criteria:**
- [ ] The CI workflow no longer downloads sqlite-vec release tarballs or creates `~/.local/share/hypomnema/sqlite-vec.{so,dylib}`.
- [ ] Linux and macOS test jobs pass with no sqlite-vec dynamic library present in the configured default location.
- [ ] The CI spec is amended to describe static sqlite-vec validation instead of the `Install sqlite-vec extension` step.
- [ ] Negative fingerprint: `rg "Install sqlite-vec extension|sqlite-vec-.*loadable|vec0\\.(so|dylib|dll)" .github docs/specs/ci-pipeline.md` returns zero matches.

## Story 5: LDS Canon Matches the New Packaging Contract

**Story:** As the user, I want the project docs to describe sqlite-vec as bundled into Hypomnema so that install guidance and architecture notes do not preserve the old manual dependency.

**Acceptance Criteria:**
- [ ] ADR-0007 has an amendment clarifying that sqlite-vec remains the vector store but is now statically linked instead of operator-provisioned as a loadable extension.
- [ ] `docs/reference/configuration.md` removes the sqlite-vec extension prerequisite and the `embedding.extension_path` environment override mapping.
- [ ] `docs/architecture/overview.md` updates the component/deployability descriptions from "dynamic library loaded in-process" and "two binaries plus one extension file" to the static-bundling contract.
- [ ] `docs/implementation/tech-stack.md` lists `sqlite-vec` as a pinned crate/static extension build, not as a runtime extension outside Cargo.
- [ ] Archived notes may retain historical references, but active LDS canon must not tell operators to manually download sqlite-vec.
