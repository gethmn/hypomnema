# Hypomnema Roadmap — Round 11: Static sqlite-vec Bundling

**Scope**: Replace operator-provisioned sqlite-vec loadable extension with a statically linked extension built into the Hypomnema crate and registered with SQLite before any store connection opens. Single round, single step. No vector-store abstraction, no schema change, no workspace split.

**Status**: Shipped 2026-05-03. Round 11 closed; see step-22 retro in `notes/project-planning-workflow-notes.md`.

**Process**: Same as rounds 1–10. One step. Coordinator + researcher + ephemeral builders. See [`notes/playbook/`](../playbook/) for the orchestration contract.

**Source of truth for scope**:
- Intake: [`notes/proposals/intake-static-sqlite-vec-bundling.md`](../proposals/intake-static-sqlite-vec-bundling.md)
- Spec: [`notes/proposals/static-sqlite-vec-bundling.md`](../proposals/static-sqlite-vec-bundling.md)
- Stories: [`notes/proposals/static-sqlite-vec-bundling-stories.md`](../proposals/static-sqlite-vec-bundling-stories.md)

**Why this round**:

- Round 10 closed v0.5.0-ready operational polish. Static sqlite-vec bundling is the next-most-load-bearing improvement: it removes a recurring install/dev-shell/CI footgun by making `cargo install hypomnema` produce runnable binaries with no separate sqlite-vec download.
- Backlog flagged as "do sooner rather than later" since round 6 (`notes/backlog.md` § Round-6 carry-over). Aligns with the self-contained-deployment rationale of ADR-0002, ADR-0005, and ADR-0008.
- **Risk is bounded**. Behavior of `chunks_vec`, dimension validation, and indexing pipeline does not change — only how sqlite-vec reaches each connection. The new correctness boundary is registration order: sqlite-vec's static init must register before the first r2d2 pool connection opens. Mitigated by tests that exercise the production code path (not direct in-memory connections).
- **No blocking questions** for the round. Open questions resolve at workplan-write (researcher).

**Skills carrying forward**:

- `sqlite-vec-extension` — schema patterns and the loading contract change here from dynamic to static; the table/query patterns are unchanged.
- `rusqlite-in-async` — DB touches stay inside `spawn_blocking`; no change to the async boundary.

**New deps**: One new Cargo dependency — the upstream `sqlite-vec` Rust crate at an exact version pin (current candidate `=0.1.10-alpha.3`; researcher verifies at workplan-write). Per AGENTS.md "ask vs proceed", the *category* (one upstream sqlite-vec build crate) is pre-approved by this round; the specific version pin is a workplan-time decision the human will see at workplan-ready handoff. `rusqlite/load_extension` is removed from the default feature set.

**Out of scope for round 11** (explicitly):

- Vector-store abstraction or swappable backends (forbidden by AGENTS.md "What not to build").
- Workspace split into multiple crates (load-bearing single-crate shape per ADR-0008; minor `hmn` linkage of sqlite-vec is acceptable).
- Prebuilt binary distribution / cargo-dist / cross-compilation / checksums / `gethmn.io` install scripts — separate "Release automation" backlog item gated on a future release-process round.
- Compile-time feature flag for dynamic sqlite-vec loading (researcher may revisit if a maintainer workflow needs it; default is no flag).
- Windows support (separate "Windows CI matrix" backlog item).

---

## Phasing

One step. The five stories form a tightly coupled deliverable: the Cargo/registration work cannot ship without the config work without leaving the daemon in an inconsistent state, and CI + LDS canon must move with the code to avoid a window where docs lie.

| Step | Contents | Risk |
| ---- | -------- | ---- |
| 22 | Static sqlite-vec bundling — Cargo dep + static registration + config schema cleanup + CI amendment + LDS canon updates | Medium |

---

## Step 22 — Static sqlite-vec Bundling

**Goal**: Replace the operator-provisioned sqlite-vec loadable extension with a statically linked extension built and registered by Hypomnema, and align config schema, CI, and LDS canon with the new packaging contract.

**Shipping criteria** (full breakdown in `step-22-workplan.md`):

- `Cargo.toml` adds `sqlite-vec` at a pinned exact version; `rusqlite/load_extension` is removed from the default feature set.
- Process-level `sqlite3_auto_extension` (or equivalent verified against the pinned crate version) registers sqlite-vec's init symbol exactly once before the first r2d2 pool is constructed.
- Store pool initialization retains WAL + `synchronous=NORMAL` pragmas; all dynamic-extension-loading code paths are removed.
- A test opens at least two independent store connections through the production code path and verifies `SELECT vec_version()` succeeds on both.
- `embedding.extension_path` is removed from the accepted config schema (deprecation window vs. immediate removal — workplan-time decision); `HYPOMNEMA_VEC_EXT_PATH` is removed from the runtime contract.
- `cargo install hypomnema` on a clean host (no `~/.local/share/hypomnema/sqlite-vec.<ext>`) produces runnable `hmn` + `hmnd`; daemon opens a fresh vault store and applies `chunks_vec` migrations without filesystem-side sqlite-vec.
- CI workflow stops downloading sqlite-vec tarballs; Linux and macOS jobs are green; `docs/specs/ci-pipeline.md` amended.
- ADR-0007 amendment documents the static-linkage choice; `docs/reference/configuration.md`, `docs/architecture/overview.md`, and `docs/implementation/tech-stack.md` updated.
- Negative fingerprints from spec § Implementation Notes return zero matches in active code/canon: `load_extension*`, `extension_path|HYPOMNEMA_VEC_EXT_PATH|VEC_EXT_PATH_ENV`, `Install sqlite-vec extension|sqlite-vec-.*loadable|vec0\.(so|dylib|dll)`.
- `cargo test` green; `cargo clippy -- -D warnings` clean.
- Manual-testing fixture (`notes/manual-testing/`) updated.
- `flake.nix` sqlite-vec dylib provisioning removed (closes the long-standing "Operational follow-ups" backlog entry).

**Deferred decisions to resolve at workplan-time**:

1. **Pinned `sqlite-vec` crate version** — current candidate `=0.1.10-alpha.3`. Researcher verifies upstream still exposes `sqlite3_vec_init` and demonstrates `rusqlite::ffi::sqlite3_auto_extension` registration at the chosen pin.
2. **Deprecation window vs. immediate removal of `embedding.extension_path` / `HYPOMNEMA_VEC_EXT_PATH`** — spec preference is immediate removal (Hypomnema is pre-public-release); researcher confirms.
3. **Compile-time feature flag for dynamic loading as a maintainer escape hatch** — spec preference is no flag.
4. **Binary-size measurement and reporting policy** — measure-and-record only; no crate split per ADR-0008.
5. **C-toolchain prerequisite documentation** — install docs need a one-liner for source-install build-time C-toolchain requirement.

**Risk**: Medium. Static registration order is the new correctness boundary (spec § Edge Cases — "Static Registration Happens Too Late"). Tests must exercise the production code path. CI + LDS canon sweeps are non-trivial but mechanical; bundled gates prevent partial-state shipping. Build-time C-toolchain failure on source installs is a new user-visible failure mode — documented and acceptable.

**Coverage**: Maps to all five stories in `static-sqlite-vec-bundling-stories.md`, all four spec § Open Questions, and four spec § Edge Cases. Closes two backlog items at round close: "Static sqlite-vec bundling" (Round-6 carry-over) and "flake.nix sqlite-vec dylib provisioning" (Operational follow-ups).

---

## Step Sequencing

Single step. Workplan-write resolves intra-step task ordering and dependency graph. Coordinator orchestrates per-task builders; gate review verifies all shipping criteria + negative fingerprints + deferred decisions resolved.

1. Coordinator (`step-22-coordinator`) spawns researcher and requests `step-22-workplan.md`.
2. Researcher resolves deferred decisions and produces full task breakdown, dep version pin, and testing strategy.
3. Coordinator surfaces workplan to human for review.
4. On `build/go/approved`, coordinator orchestrates builders per task.
5. Gate verifies all shipping criteria + negative fingerprints + deferred decisions resolved.
6. Round 11 ships; archive workplan + roadmap; backlog hygiene closes the two related entries.

---

## Notes on round-11 philosophy

This is a packaging/runtime-init change, not a feature round. The goal is a single self-contained `cargo install` for source installs and a smaller dev/CI/install footprint everywhere. Both anchor to existing patterns (Cargo dependency graph, store pool init) and avoid introducing new abstractions. If workplan-time analysis surfaces a reason to split this into multiple steps or expand scope, that's a coordinator-to-orchestrator escalation rather than a quiet expansion.
