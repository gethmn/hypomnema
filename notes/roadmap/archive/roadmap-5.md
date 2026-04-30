# Hypomnema Roadmap — Round 5: Maintenance Pass

**Scope**: A focused maintenance round covering two shipped workstreams and one deferred follow-on: (1) CHANGELOG.md adoption — settle format, back-fill milestones, and establish the going-forward ritual at each shipping gate; (2) CI pipeline — ship a GitHub Actions CI workflow (`ci.yml` + Dependabot config) that runs format, lint, and tests on Ubuntu + macOS for every push/PR, promoting the `notes/proposals/ci-cd-pipeline.md` spec to LDS canon; (3) outbox flake hardening was initially scoped here but is now deferred because the likely next move is removing the outbox entirely.

Release automation (binaries, cross-compilation, OSSF Scorecard, CodeQL) is explicitly **out of scope** for this round and stays in the backlog. The round ships as `v0.4.0`.

**Status**: Shipped 2026-04-29. Round 4 shipped `v0.3.0` on 2026-04-28; step 13 shipped 2026-04-29; step 14 is deferred to round 6+.

**Process**: Same as rounds 1–4. Each step gets a short workplan (`step-NN-workplan.md`) created immediately before that step is built. Deferred decisions are pulled forward to workplan-time. The orchestration shape (orchestrator + per-step coordinator + ephemeral task agents, see [`notes/coordinator-playbook.md`](../coordinator-playbook.md)) carries forward unchanged.

**Round-4 lessons feeding into this round** (see [`notes/project-planning-workflow-notes.md`](../project-planning-workflow-notes.md) § End-of-round retrospective for full text):

- **Single-step round delivered cleanly** — the 1-step-round choice was correct when implementation surface had no meaningful internal boundary. Round 5's three workstreams each have a natural step boundary; the 3-step phasing below reflects that.
- **Spec-fleshout-at-workplan-write scales cleanly** — all proposal open questions resolved at workplan-write; zero scope-question escalations. Round 5 applies the same pattern: `notes/proposals/ci-cd-pipeline.md` stays Draft until step 13's workplan-write phase, at which point it promotes to `docs/specs/ci-pipeline.md`.
- **Manual smoke on every wiring-shape task** — 6-of-6 across rounds 1–4. Step 13's CI wiring (YAML + Actions config) is a "wiring" task by shape; smoke is first-pass verification that the workflow actually runs green on GitHub before declaring the step shipped.
- **Silence-as-data for non-recurring flakes carries forward** — the outbox flake (`tests/outbox.rs::rename_emits_deleted_then_created_lines`) was the candidate for step 14, but the round closed before that work started. The flake notes stay in the backlog for a future outbox-removal or flake-investigation pass.
- **MSRV cross-check at workplan self-review** — any new top-level crate added in a workplan should have its MSRV cross-checked against `rust-toolchain.toml`. Round 5 is unlikely to add new crates (CI is YAML, CHANGELOG is Markdown), but the check applies if anything surfaces.
- **Workplan-prose-vs-load-bearing-decision drift** is the dominant round-3+ source of `coordinator-only` soft flags (~0.5/task). Set the same expectation for round 5; defer-to-boundary routing is correct.
- **`mcp-http-transport` skill candidate dropped** — evaluated and declined at round-4 step-12 boundary. No new skills anticipated for round 5.
- **Skills carrying forward**: `rusqlite-in-async` (no SQLite surface this round), `filesystem-watching` (relevant if a future outbox-removal or flake pass touches the watcher path), `markdown-chunking` (no relevance), `sqlite-vec-extension` (no relevance). `filesystem-watching` is the most likely to be consulted in any future outbox-removal or flake-investigation pass.

**Specs amended or created this round**:

- **`docs/specs/ci-pipeline.md`** — new spec, promoted from `notes/proposals/ci-cd-pipeline.md` v1.0.0 at step-13 workplan-write. Release automation sections stay in the proposal's "future extension points" framing; the promoted spec covers CI-only scope.
- **`CHANGELOG.md`** — new file at repo root. Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) (conventional; tooling-friendly). Back-filled with milestones for `v0.1.0` through `v0.3.0` based on the per-step retros and round shipping criteria. Going-forward: updated at each round's shipping gate as part of the boundary ritual.
- **`notes/project-planning-workflow-notes.md` § Step-boundary ritual** — add a CHANGELOG update step to the boundary checklist.
- **No amendments to architecture or API specs** — round 5 is infrastructure and process; no daemon behavior changes.

**Implementation surface across the round**:

- New files: `.github/workflows/ci.yml`, `.github/dependabot.yml`, `CHANGELOG.md`, `docs/specs/ci-pipeline.md`
- No `src/` changes in steps 13 or 15
- Step 14 was deferred out of round 5; a future outbox-removal round may touch `tests/outbox.rs`, `src/watcher/`, and the outbox plumbing more broadly.

**No top-level crate additions anticipated.** CI is pure YAML; CHANGELOG is Markdown; outbox flake investigation is test-surface work against existing crates.

---

## Phasing

Three steps, one per workstream, ordered to put the most uncertain work (flake investigation) in the middle where it can slip to post-round without blocking the shipping gate:

| Step | Contents | Risk |
|------|----------|------|
| 13 | CI pipeline: spec promotion + `ci.yml` + `dependabot.yml` + branch-protection docs note | Low |
| 14 | Deferred out of round 5; likely superseded by outbox removal | N/A |
| 15 | CHANGELOG.md: format decision + back-fill `v0.1.0`–`v0.3.0` + `v0.4.0` entry + boundary ritual update | Low |

Step 14 is now deferred out of round 5. The round-5 shipping gate no longer depends on it; the flake notes stay in the backlog as a candidate for a future outbox-removal round.

---

## Pre-round prep (before step 13 starts)

One small item:

1. **Verify GitHub Actions availability and Xcind reference patterns.** The CI proposal references `https://github.com/scinddev/xcind/tree/main/.github/workflows` as the security-practice reference for SHA-pinned actions and minimal permissions. The step-13 workplan author should review that reference at workplan-write time to confirm the current SHA hashes for `actions/checkout`, `dtolnay/rust-toolchain`, `Swatinem/rust-cache`, and `taiki-e/install-action` — these change as new releases land. Not a step in itself; a workplan-write task for step 13.

---

## Step 13 — CI pipeline (spec promotion + `.github/workflows/`)

**Status**: Shipped 2026-04-29.

**Goal**: Every push to `main` and every PR targeting `main` triggers a GitHub Actions CI run with three parallel jobs: (1) `cargo fmt --check` on ubuntu-latest, (2) `cargo clippy --all-targets -- -D warnings` on ubuntu-latest, (3) `cargo nextest run` on ubuntu-latest + macos-latest. Jobs use SHA-pinned actions, `rust-cache` for build caching, and minimal permissions (`contents: read`). Dependabot is configured for weekly Cargo dependency review. The `notes/proposals/ci-cd-pipeline.md` spec is promoted to `docs/specs/ci-pipeline.md` v1.0.0.

**Shipping criteria**:

- `.github/workflows/ci.yml` exists and passes validation (`act --list` or a GitHub dry-run confirms job names match `format`, `lint`, `test`).
- All three jobs run green against the current `main` branch (verified by pushing and observing the Actions run, or via `act` locally if available).
- `dependabot.yml` configured for Cargo (weekly schedule, `main` target branch).
- `docs/specs/ci-pipeline.md` v1.0.0 exists, promoted from the proposal, with release-automation "future extension points" clearly scoped as deferred. Proposal archived at `notes/proposals/archive/ci-cd-pipeline.md`.
- `docs/architecture/overview.md` or `docs/reference/configuration.md` (whichever is appropriate at workplan-time) references the CI pipeline as a project quality gate — a one-sentence cross-reference is sufficient.
- Negative-fingerprint: `rg 'release.yml|cross-compile|cargo-dist|goreleaser' .github/` returns zero matches.
- Branch protection recommended configuration documented (one-paragraph note in `docs/specs/ci-pipeline.md` § Branch Protection) — not enforced by code, just documented as operator guidance.
- The full test suite (`cargo nextest run`) is green locally before any `.github/` commit lands — CI must start green.

**Deferred decisions to resolve at workplan-time**:

- **Exact SHA hashes** for all four actions (`actions/checkout@v4`, `dtolnay/rust-toolchain@stable`, `Swatinem/rust-cache@v2`, `taiki-e/install-action@nextest`) — resolve by reading the Xcind reference + current GitHub releases at workplan-write.
- **`cargo nextest` profile for CI** — the proposal suggests JUnit XML output (`--profile ci`); decide at workplan-write whether to add a `[profile.ci]` entry to `.config/nextest.toml` (or equivalent) or use inline `--reporter junit` flags. The existing Justfile uses `cargo nextest run` without a profile; CI may want structured output for GitHub's test-reporting surface.
- **`rust-toolchain.toml` vs. `dtolnay/rust-toolchain@stable`** — the project already has a `rust-toolchain.toml` pinning the toolchain version. The `dtolnay/rust-toolchain` action respects `rust-toolchain.toml` when present; verify at workplan-write that this is the expected behavior and no `toolchain:` override is needed.
- **Dependabot PR frequency and grouping** — weekly is the proposal default; decide at workplan-write whether to add a `groups:` entry to batch Rust dependency PRs (reduces PR noise).
- **Spec promotion scope** — the proposal covers both CI and a "future" release pipeline. The promoted spec should scope to CI-only; verify at workplan-write which sections to carry forward vs. move to a `notes/proposals/archive/ci-cd-pipeline.md` note about deferred release work.

**New deps**: none. Pure YAML + Markdown.

**Risk**: low. No `src/` changes; the blast radius is limited to CI infrastructure. The main failure mode is a YAML syntax error or an action version mismatch — both caught immediately by the first CI run.

---

## Step 14 — Outbox flake hardening

**Status**: Deferred to round 6+. The likely follow-on is outbox removal, not a flake-only fix.

**Note**: the step-14 flake candidates remain recorded in [`notes/backlog.md`](../backlog.md) as an open follow-on. If the next round removes outbox entirely, that work supersedes the old flake hardening task and can retire the stale tests as part of the broader cleanup.

---

## Step 15 — CHANGELOG.md adoption (round-5 shipping gate)

**Status**: Shipped 2026-04-29. See [`notes/project-planning-workflow-notes.md`](../project-planning-workflow-notes.md) § Step 15 for the retrospective.

**Goal**: `CHANGELOG.md` exists at the repo root in [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) format, back-filled with entries for `v0.1.0` through `v0.3.0` derived from the per-step retros and round shipping criteria, with a `v0.4.0` entry capturing this round's changes (CI pipeline, CHANGELOG adoption itself, and the decision to defer outbox flake hardening). The going-forward ritual is codified: the step-boundary checklist in `notes/project-planning-workflow-notes.md` includes a CHANGELOG update step at each round's shipping gate.

**Shipping criteria**:

- `CHANGELOG.md` exists at repo root; `## [Unreleased]` section is present and empty (or absent, workplan-time decision); `## [0.4.0]`, `## [0.3.0]`, `## [0.2.0]`, `## [0.1.0]` sections are present with dates and substantive entries.
- Each back-filled version's entries accurately reflect what shipped: `v0.1.0` = daemon skeleton + scan + watcher + outbox + HTTP search, `v0.2.0` = chunking + embedding + semantic search + MCP stdio + multi-vault + vault lifecycle, `v0.3.0` = MCP Streamable HTTP transport.
- `v0.4.0` entry covers: CI pipeline (GitHub Actions), CHANGELOG adoption, and the explicit deferral of outbox flake hardening to a later round.
- `notes/project-planning-workflow-notes.md` § Step boundary ritual includes a CHANGELOG update step.
- `cargo clippy` and `cargo test` remain green (no code changes; this is a doc-only step).
- The round-5 shipping tag is `v0.4.0`.

**Deferred decisions to resolve at workplan-time**:

- **Back-fill granularity** — per-round summaries (4 entries) vs. per-step entries (12 entries). Recommendation at roadmap-write time: per-round summaries (4 entries back-filled + current round). Per-step would be 12 entries before `v0.4.0`; the CHANGELOG would be large before the project has external users, and the retros in `notes/project-planning-workflow-notes.md` are the authoritative per-step record. The workplan author may override this.
- **`## [Unreleased]` section policy** — whether to maintain an `[Unreleased]` section between rounds. Decide at workplan-write.
- **Version bump location** — `Cargo.toml` currently tracks the version; verify at workplan-write that `v0.4.0` bumps `Cargo.toml` version as part of the shipping gate (or confirm the existing boundary ritual already handles this).

**New deps**: none. Doc-only.

**Risk**: low. No code changes. The main risk is getting the back-filled dates/details wrong; the retros in `notes/project-planning-workflow-notes.md` are the authoritative source and should be used directly.

---

## Out of scope for round 5

These stay in [`notes/backlog.md`](../backlog.md) and become candidates for round 6+:

- **Release automation** (`release.yml`, binary cross-compilation, checksums, cargo-dist) — the CI proposal explicitly scopes this as a separate workstream. Backlog.
- **OSSF Scorecard / CodeQL** — security tooling; round-6+ when the project has public visibility.
- **Windows CI matrix** — the proposal notes Windows as a non-goal; `unix-platforms-only` is the current scope.
- **Compose-style declarative layer** — pinned in `docs/specs/vault-management.md` § Compose-Style Declarative Layer (deferred). Backlog.
- **MCP write-tool gating granularity** — per-tool vs. single flag. Backlog.
- **Multi-model embedding per vault**. Backlog.
- **Cross-vault search pagination + streaming**. Backlog.
- **Agent-host integration / MCP-tool discoverability**. Backlog.
- **Public-presence / brand work**. Backlog.

---

## Notes on the round-5 shipping gate

The round-5 shipping gate is:

1. `.github/workflows/ci.yml` is live and green on `main` for format + lint + test (ubuntu + macos).
2. `CHANGELOG.md` exists with back-filled entries for `v0.1.0`–`v0.3.0` and a `v0.4.0` entry covering this round.
3. Step 14 has been explicitly deferred to the backlog, with outbox removal called out as the likely follow-on.
4. `docs/specs/ci-pipeline.md` v1.0.0 is the canonical spec; proposal archived.
5. `notes/project-planning-workflow-notes.md` § Step boundary ritual includes CHANGELOG update.
6. Full test suite green; no regressions.
7. Round tag: `v0.4.0`.

After the gate hits, round 5 archives alongside its step workplans, and round 6's roadmap is written when the human picks the next focus from the backlog.
