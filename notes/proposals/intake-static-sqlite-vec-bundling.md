# Proposal Intake: Static sqlite-vec Bundling

**Status**: Intake complete
**Date**: 2026-05-03
**Intake inputs**:

- `notes/proposals/static-sqlite-vec-bundling.md` — Primary proposal (Status: Draft, 2026-05-02, v0.1.0)
- `notes/proposals/static-sqlite-vec-bundling-stories.md` — Five acceptance-criteria stories
- `notes/backlog.md` § Round-6 carry-over — backlog entry flagging this as "do sooner rather than later" (recurring install/dev-shell/CI footgun)

---

## Summary

Move sqlite-vec from an operator-provisioned dynamic loadable extension to a statically linked extension built into the Hypomnema crate and registered with SQLite before any store connection opens. The vector-store choice does not change: same `chunks_vec` schema, same dimension validation, same delete-then-reinsert indexing path. Only the *delivery* changes — `cargo install hypomnema` produces runnable `hmn`/`hmnd` binaries with no separate `sqlite-vec.{so,dylib,dll}` download, no `~/.local/share/hypomnema/sqlite-vec.<ext>` lookup, and no `HYPOMNEMA_VEC_EXT_PATH` contract. The change touches the Cargo dependency graph (adds the upstream `sqlite-vec` crate at a pinned version, drops `rusqlite/load_extension` from the default path), one process-level registration call before pool construction, the `[embedding]` config schema (drops `extension_path`), CI (no tarball download), and LDS canon (ADR-0007 amendment, configuration reference, architecture overview, tech-stack). Failure shifts from daemon startup ("extension binary not found") to install/build time (C-toolchain prerequisite for source installs).

## Source Inputs

| Source | Type | Role in intake |
|---|---|---|
| `notes/proposals/static-sqlite-vec-bundling.md` | proposal | primary — defines behavior, dependency shape, config delta, edge cases, error handling, integration points, open questions |
| `notes/proposals/static-sqlite-vec-bundling-stories.md` | stories | primary — five stories spanning install, store init, config, CI, and LDS canon |
| `notes/backlog.md` § Round-6 carry-over | backlog | supporting — flags urgency ("recurring install/dev-shell/CI footgun") and aligns with the self-contained Rust deployment rationale |
| ADR-0007 (sqlite-vec over alternatives) | decision | background — vector-store choice unchanged; this round amends rather than replaces |
| ADR-0008 (two binaries, one crate) | decision | background — single-crate shape preserved; `hmn` linking sqlite-vec is acceptable |
| `docs/specs/ci-pipeline.md` | spec | background — CI workflow currently downloads sqlite-vec tarballs; this round amends the spec |
| `docs/reference/configuration.md` § embedding | reference | background — `extension_path` / `HYPOMNEMA_VEC_EXT_PATH` documented today; this round removes them |
| `docs/architecture/overview.md` § system components | architecture | background — describes sqlite-vec as a dynamic library loaded in-process; this round updates that wording |

## Candidate Outcomes

- **Outcome: Self-contained source install**
  - Source: Story 1
  - User-visible result: `cargo install hypomnema` on a clean machine yields runnable `hmn` and `hmnd` with no manual sqlite-vec provisioning step; opening a fresh vault store applies migrations that create `chunks_vec USING vec0(...)` without a sqlite-vec dynamic library on disk.
  - Verification signal: `SELECT vec_version()` succeeds through a production store connection; daemon startup error path no longer includes `sqlite-vec extension binary not found`.

- **Outcome: Static registration applies to every connection**
  - Source: Story 2
  - User-visible result: Each new rusqlite connection opened by the r2d2 pool sees sqlite-vec without `load_extension` plumbing.
  - Verification signal: A test that opens two independent store connections sees `vec_version()` succeed on both; `rg "load_extension|load_extension_enable|load_extension_disable" src` returns zero matches; WAL + `synchronous=NORMAL` pragmas remain intact.

- **Outcome: Configuration surface no longer references the extension path**
  - Source: Story 3
  - User-visible result: Default `[embedding]` config has no `extension_path`; `HYPOMNEMA_VEC_EXT_PATH` is not required for tests, manual smoke, or daemon startup.
  - Verification signal: Negative fingerprint `rg "extension_path|HYPOMNEMA_VEC_EXT_PATH|VEC_EXT_PATH_ENV" src docs/reference docs/specs .github` returns zero matches (modulo intentional deprecation-handling code if a deprecation window is selected).

- **Outcome: CI validates the bundled path, not a tarball download**
  - Source: Story 4
  - User-visible result: CI exercises the same code path source installs use; packaging regressions fail in CI before release.
  - Verification signal: CI workflow no longer downloads sqlite-vec tarballs or writes `~/.local/share/hypomnema/sqlite-vec.{so,dylib}`; Linux + macOS jobs pass with no dylib present; `docs/specs/ci-pipeline.md` describes static validation.

- **Outcome: LDS canon describes sqlite-vec as bundled**
  - Source: Story 5
  - User-visible result: ADR-0007 amendment, configuration reference, architecture overview, and tech-stack all describe sqlite-vec as a pinned crate / static extension build.
  - Verification signal: `rg "Install sqlite-vec extension|sqlite-vec-.*loadable|vec0\\.(so|dylib|dll)" .github docs notes` returns zero matches in active canon (archived notes may retain history).

## Proposed Roadmap Shape

Single round, single step. The five stories form a tightly coupled deliverable: the Cargo/registration work (Stories 1–2) cannot ship without the config work (Story 3) without leaving the daemon in an inconsistent state (binary ignores `extension_path` while config docs still demand it), and CI + LDS canon (Stories 4–5) must move with the code to avoid a window where docs lie. Splitting into multiple steps would invite drift; bundling keeps the negative-fingerprint sweeps coherent.

### Step N — Static sqlite-vec Bundling

**Goal**: Replace the operator-provisioned sqlite-vec loadable extension with a statically linked extension built and registered by Hypomnema, and align config schema, CI, and LDS canon with the new packaging contract.

**Shipping criteria**:

- [ ] `Cargo.toml` adds `sqlite-vec` at a pinned exact version (workplan-time choice; current candidate `=0.1.10-alpha.3`); `rusqlite/load_extension` is removed from the default feature set.
- [ ] Process-level `sqlite3_auto_extension` (or equivalent verified against the pinned crate version) registers sqlite-vec's init symbol exactly once before the first r2d2 pool is constructed.
- [ ] Store pool initialization retains WAL + `synchronous=NORMAL` pragmas; all dynamic-extension-loading code paths are removed (`rg "load_extension"` zero matches in `src/`).
- [ ] A test opens at least two independent store connections through the production code path and verifies `SELECT vec_version()` succeeds on both.
- [ ] `embedding.extension_path` is removed from the accepted config schema (or accepted-with-warning if a deprecation window is chosen — see deferred decision 1); `HYPOMNEMA_VEC_EXT_PATH` is removed from the runtime contract.
- [ ] `cargo install hypomnema` on a clean host (no `~/.local/share/hypomnema/sqlite-vec.<ext>`) produces runnable `hmn` + `hmnd`; daemon opens a fresh vault store and applies `chunks_vec` migrations without filesystem-side sqlite-vec.
- [ ] CI workflow stops downloading sqlite-vec tarballs and stops creating `~/.local/share/hypomnema/sqlite-vec.{so,dylib}`; Linux and macOS jobs are green; `docs/specs/ci-pipeline.md` is amended.
- [ ] ADR-0007 amendment documents the static-linkage choice; `docs/reference/configuration.md`, `docs/architecture/overview.md`, and `docs/implementation/tech-stack.md` updated.
- [ ] Negative fingerprints (from spec § Implementation Notes) all return zero matches in active code/canon: `load_extension*`, `extension_path|HYPOMNEMA_VEC_EXT_PATH|VEC_EXT_PATH_ENV`, `Install sqlite-vec extension|sqlite-vec-.*loadable|vec0\.(so|dylib|dll)`.
- [ ] `cargo test` green; `cargo clippy -- -D warnings` clean.
- [ ] Manual-testing fixture (`notes/manual-testing/`) updated: install steps no longer mention provisioning sqlite-vec; smoke path validates `vec_version()` through `hmnd`.
- [ ] `flake.nix` sqlite-vec dylib provisioning is removed (the long-standing operational follow-up in `notes/backlog.md` § Operational follow-ups closes here).

**Deferred decisions resolved in this step**:

- Decision: Pinned `sqlite-vec` crate version
  - Source: Spec § Open Questions Q3 (`0.1.10-alpha.3` candidate)
  - Why this step: The exact version determines the init-symbol contract (`sqlite3_vec_init`) and the build-script behavior; researcher must verify against the actual crate at workplan-write.
- Decision: Deprecation window vs. immediate removal of `embedding.extension_path` / `HYPOMNEMA_VEC_EXT_PATH`
  - Source: Spec § Open Questions Q1; Stories 3 acceptance criteria branch on this
  - Why this step: The schema parser path differs (warn-and-ignore vs. unknown-field error). Hypomnema is pre-public-release; preferred answer in spec is immediate removal, but researcher confirms.
- Decision: Whether to keep a compile-time feature flag for dynamic loading as a maintainer escape hatch
  - Source: Spec § Open Questions Q2
  - Why this step: Affects feature gating in `Cargo.toml` and CI matrix shape; spec preference is "no" unless a concrete workflow needs it.
- Decision: Binary-size measurement and reporting policy
  - Source: Spec § Open Questions Q4 + § Edge Cases ("`hmn` Links sqlite-vec Even Though It Does Not Open Stores")
  - Why this step: Workplan should record before/after sizes for both binaries; ADR-0008 forbids splitting crates as a *response* to size, but the data is worth capturing.
- Decision: C-toolchain prerequisite documentation
  - Source: Spec § Edge Cases ("Cross-Compilation and Toolchain Availability")
  - Why this step: `cargo install` failure mode shifts from runtime to build time; install docs need a one-liner about platform C-toolchain requirements.

**New deps**:

- `sqlite-vec` (Rust crate, exact-pin) — replaces the operator-provisioned loadable extension. Per AGENTS.md "ask vs proceed", this is being flagged at intake; the *category* (one upstream sqlite-vec build crate) is pre-approved by the round; the specific version pin is a workplan-time decision the human will see at workplan-ready handoff.

**Risk**: medium. Static registration order is a real correctness boundary (spec § Edge Cases — "Static Registration Happens Too Late"); the negative-fingerprint sweeps are non-trivial because the existing `extension_path` / env-var contract has touched config, CI, docs, and the dev shell. Mitigations: tests must exercise the production code path (not just direct in-memory connections), and the round bundles config + CI + canon updates so partial states are not shippable. Build-time C-toolchain failure on source installs is a new user-visible failure mode — documented and acceptable, but worth calling out in install docs.

**Source coverage**:

- `static-sqlite-vec-bundling.md`: Behavior § Normal Flow + Data Schema § Cargo / Runtime Configuration + Edge Cases (all four) + Error Handling (all five rows) + Integration Points (Cargo / Store Init / Config / CI / Release Packaging) + Implementation Notes
- `static-sqlite-vec-bundling-stories.md`: Stories 1–5 (all)
- `notes/backlog.md`: "Static sqlite-vec bundling" entry (Round-6 carry-over) + `flake.nix` sqlite-vec dylib provisioning (Operational follow-ups)

## Coverage Map

| Source item | Proposed step | Status | Notes |
|---|---|---|---|
| Story 1 (source install produces runnable binaries) | Step N | planned | Verified via `cargo install` on a clean host + `vec_version()` probe. |
| Story 2 (store init uses static registration) | Step N | planned | Two-connection probe + `load_extension` negative fingerprint. |
| Story 3 (obsolete config removed/deprecated) | Step N | planned | Deferred decision 2 picks deprecation window vs. immediate removal. |
| Story 4 (CI validates bundled path) | Step N | planned | `docs/specs/ci-pipeline.md` amendment + `.github/workflows/*.yml` edits. |
| Story 5 (LDS canon matches new contract) | Step N | planned | ADR-0007 amendment + `docs/reference/configuration.md` + `docs/architecture/overview.md` + `docs/implementation/tech-stack.md`. |
| Spec § Open Question Q1 (deprecation window) | Step N | planned | Resolved at workplan-write (deferred decision 2). |
| Spec § Open Question Q2 (compile-time dynamic-loading feature flag) | Step N | planned | Resolved at workplan-write (deferred decision 3). |
| Spec § Open Question Q3 (exact crate version) | Step N | planned | Resolved at workplan-write (deferred decision 1). |
| Spec § Open Question Q4 (binary-size measurement) | Step N | planned | Measure-and-record only; no crate split. Deferred decision 4. |
| Spec § Edge Cases (cross-compilation toolchain) | Step N | planned | Documented as install-time prerequisite (deferred decision 5). |
| `notes/backlog.md` "flake.nix sqlite-vec dylib provisioning" | Step N | planned | Closes when the dev-shell stops needing the dylib; remove provisioning code from `flake.nix`. |
| `notes/backlog.md` "Static sqlite-vec bundling" entry | Step N | planned | Strikethrough-in-place at round close per backlog conventions. |

## Deferred / Out-of-Scope Items

- Item: Vector-store abstraction / swappable backends
  - Source: Spec § Implementation Notes ("This spec does not introduce a vector-store abstraction…")
  - Reason: AGENTS.md "What not to build" forbids abstract traits for swappable backends in v0.
  - Revisit trigger: A second concrete vector store enters the picture.
- Item: Workspace split into multiple crates
  - Source: Spec § Edge Cases ("`hmn` Links sqlite-vec…") + ADR-0008
  - Reason: Single-crate shape is load-bearing; minor `hmn` bloat is preferred over crate factoring.
  - Revisit trigger: Measured release-binary growth becomes operator-visible (e.g. >2× current size).
- Item: Prebuilt binary distribution / cargo-dist / release automation
  - Source: `notes/backlog.md` § Round-6 carry-over ("Release automation")
  - Reason: Source-install path (`cargo install`) is in scope; binary cross-compilation, checksums, and `gethmn.io` install scripts are a separate backlog item gated on a future release process.
  - Revisit trigger: Release-process round (separate proposal: `notes/proposals/release-process-and-changelog.md`).
- Item: Compile-time feature flag for dynamic sqlite-vec loading
  - Source: Spec § Open Questions Q2
  - Reason: Spec preference is "no" unless a concrete maintainer workflow needs it. Default to no flag.
  - Revisit trigger: A maintainer encounters a debugging scenario that genuinely needs a dynamic-loading override.
- Item: Windows static bundling
  - Source: `notes/backlog.md` "Windows CI matrix" (Round-6 carry-over)
  - Reason: Current CI scope is unix-only; Windows is a separate backlog item.
  - Revisit trigger: Windows enters the supported-platform matrix.

## Open Questions

- Question: Does the upstream `sqlite-vec` crate at the candidate pin (`=0.1.10-alpha.3`) still expose `sqlite3_vec_init` and demonstrate `rusqlite::ffi::sqlite3_auto_extension` registration?
  - Why it matters: The spec's Implementation Notes explicitly says "verify this against the pinned crate version during the workplan." If the symbol or registration shape has shifted, the registration call needs adjustment.
  - Blocks roadmap? no — blocks workplan. The round can be approved on the assumption that *some* version of the crate exposes a viable registration path.
  - Suggested owner: Researcher at workplan-write.
- Question: Are there existing external users on the current `extension_path` config contract whose rollout matters?
  - Why it matters: Determines deprecation-window vs. immediate-removal choice (deferred decision 2). Hypomnema appears pre-public-release based on the backlog's "OSSF Scorecard / public visibility" framing, suggesting immediate removal is fine.
  - Blocks roadmap? no — blocks one workplan-time decision.
  - Suggested owner: Human (project owner) confirms public-visibility status; researcher proposes the rollout.
- Question: Does the macOS CI runner currently in `.github/workflows/*.yml` ship a working C toolchain for the sqlite-vec build script, or do we need an explicit setup step?
  - Why it matters: Story 4 expects Linux + macOS green without tarball downloads; if the runner image needs `xcode-select --install` or similar, that's a CI workflow edit.
  - Blocks roadmap? no — blocks workplan.
  - Suggested owner: Researcher at workplan-write.
- Question: Does the current `flake.nix` provisioning of sqlite-vec serve any other purpose beyond making the loadable extension available (e.g. development tooling that links against it directly)?
  - Why it matters: The "flake.nix sqlite-vec dylib provisioning" backlog item closes here only if the dylib has no other consumer in the dev shell.
  - Blocks roadmap? no — informs the dev-shell cleanup scope.
  - Suggested owner: Researcher at workplan-write.

## Recommendation

Proceed to:

- [x] Draft `notes/roadmap/roadmap-N.md` (next round number — verify against `notes/roadmap/archive/`; round 10 shipped, so round 11 is the natural next number unless the orchestrator decides to bundle this with another item)
- [x] Draft `notes/roadmap/step-NN-workplan.md` after researcher resolves the deferred decisions
- [ ] Refine planning inputs first

Rationale: The proposal and stories together cover the surface end-to-end. All four spec § Open Questions have stated preferred answers and clear workplan-time owners. The deferred decisions are bounded and well-scoped — none of them threaten to expand the round. Risk is medium (registration-order correctness + CI/canon sweeps) but isolated to one well-defined boundary. The backlog entry has flagged urgency since round 6 ("do sooner rather than later") and aligns with Hypomnema's self-contained-deployment rationale (ADR-0002, ADR-0005, ADR-0008). The peer proposals — HyDE semantic search (intake complete) and release process & changelog (drafted, no intake) — do not block this work; this round can run in parallel with HyDE planning or precede it. Recommend starting the next round on this proposal: spawn coordinator, request `step-NN-workplan.md`.

## Human Review Notes

(append review decisions here)
