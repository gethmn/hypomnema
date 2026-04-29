# Step 13 Workplan — CI pipeline (spec promotion + `.github/workflows/`)

**Step**: 13 of 15 (round 5 of 5 — first step in a 3-step maintenance round). Promotes [`notes/proposals/ci-cd-pipeline.md`](../proposals/ci-cd-pipeline.md) to LDS canon at `docs/specs/ci-pipeline.md` v1.0.0; ships GitHub Actions CI (`format` / `lint` / `test` × `ubuntu-latest` + `macos-latest`); ships Dependabot config; documents recommended branch-protection. See [`roadmap-5.md`](./roadmap-5.md) § Step 13 for the round, and [`archive/step-12-workplan.md`](./archive/step-12-workplan.md) for the immediately prior step.

**Status**: Workplan-phase; pending human review before build. Boundary is the **per-step** ritual only (round-5 shipping gate is step 15, not this step). See § Notes on step-13 boundary at the bottom.

**Round-4 / cross-round lessons carrying forward** (from [`roadmap-5.md`](./roadmap-5.md) § Round-4 lessons feeding into this round + [`notes/project-planning-workflow-notes.md`](../project-planning-workflow-notes.md) § End-of-round retrospective):

- **MSRV cross-check** on any new top-level crate. Step 13 introduces **zero** new top-level Rust crates. The implementation surface is YAML + Markdown + a single `.config/nextest.toml` config file (Resolution B). Re-verified at workplan self-review.
- **Manual smoke verification** is load-bearing for medium-high-risk wiring tasks (now 6-of-6 across rounds 1–4). Step 13 is a wiring task by shape — new GitHub Actions workflow against a remote service we don't fully control (GitHub-hosted runners). Smoke is a default inclusion; **Task 13.5** is the load-bearing manual smoke (push to a feature branch, observe the Actions run is green on Ubuntu + macOS, before any merge to `main`).
- **Spec-fleshout-at-workplan-write** (round-3/4 stable pattern) applies. The proposal at [`notes/proposals/ci-cd-pipeline.md`](../proposals/ci-cd-pipeline.md) is Draft v1.0.0 at workplan-write; **Task 13.1** promotes it to `docs/specs/ci-pipeline.md` v1.0.0 with all open questions resolved (Resolutions A–E below). The promotion includes a CI-only scope cut (Resolution E); release-automation framing stays in the archived proposal.
- **Forward-note prediction-vs-observation** check: step 13's external-prediction surface is the four pinned-action SHAs against GitHub's current release index. Pre-workplan-write verified each SHA against `gh api` queries (Resolution A); the load-bearing residual is whether the GitHub-hosted runners' image versions have a pre-installed Rust that conflicts with `rust-toolchain.toml`'s 1.88.0 pin. Verified at **Task 13.3** task-time — the `dtolnay/rust-toolchain` action installs from `rust-toolchain.toml` and supersedes the runner-baked Rust.
- **Workplan-prose-vs-load-bearing-decision drift** is a stable round-3+ pattern (~0.5 flag-per-task). Step 13's surface (6 tasks across canon + nextest config + workflow YAML + dependabot YAML + manual smoke + boundary) will likely surface 2–4 such flags; treat them as defer-to-boundary by default unless a downstream task is materially affected.
- **Internal-shape claims** check: step 13 makes **no** internal-code claims. The only claim against existing on-disk shape is the `rust-toolchain.toml` pin (`channel = "1.88.0"`, `components = ["rustfmt", "clippy", "rust-src", "rust-analyzer"]`, `profile = "minimal"`) — verified at workplan-write by reading the file. The Justfile's `cargo nextest run` invocation (no profile) is verified the same way; the new `[profile.ci]` does not alter local behavior.
- **Soft-flag self-correction at boundary** (round-3 pattern, round-4 confirmed): when consuming a forward-noted reconciliation, verify the claimed drift is actually present before editing. **Task 13.6**'s boundary verification applies this rule for any forward-noted soft-flag reconciliations from earlier tasks.
- **Silence-as-data for non-recurring flakes** carries forward. The pre-existing outbox flake (`tests/outbox.rs::rename_emits_deleted_then_created_lines`) was silent across steps 9–12. If step 13's first CI run reproduces it (CI is the first uncontrolled multi-platform stress invocation in the project's history), **that signal feeds directly into step 14** (the dedicated flake-hardening step) — surface as a `coordinator-only` soft flag with the run URL captured. If silent again, that's continued silence-as-data.
- **Push-to-origin** at boundary: round-3-step-11 boundary missed the post-tag push and was patched up at round-4 step-12; round-4 added "push HEAD and any new tag(s) to origin" to the boundary ritual. Step 13's per-step boundary (no tag, no version bump) only requires `git push origin main` after the merge — recorded explicitly in § Notes on step-13 boundary.
- **Skills carrying forward**: [`rusqlite-in-async`](../../.claude/skills/rusqlite-in-async/SKILL.md), [`filesystem-watching`](../../.claude/skills/filesystem-watching/SKILL.md), [`markdown-chunking`](../../.claude/skills/markdown-chunking/SKILL.md), [`sqlite-vec-extension`](../../.claude/skills/sqlite-vec-extension/SKILL.md) — none directly relevant this step (no SQLite, no watcher, no chunking, no vec extension). **No new skill anticipated**; if the GitHub Actions + Rust pattern proves codifiable across multiple steps in round 5, write a `github-actions-rust` skill at the round-5 boundary per the playbook precedent.

---

## Goal recap

Every push to `main` and every PR targeting `main` triggers a GitHub Actions CI run with three parallel jobs:

1. `format` — `cargo fmt --all -- --check` on `ubuntu-latest`.
2. `lint` — `cargo clippy --all-targets -- -D warnings` on `ubuntu-latest`.
3. `test` — `cargo nextest run --profile ci` on `ubuntu-latest` + `macos-latest` (matrix; `fail-fast: false`).

All actions are SHA-pinned (Resolution A); permissions default to `contents: read`; `Swatinem/rust-cache@v2` provides Cargo build caching; `dtolnay/rust-toolchain@stable` reads `rust-toolchain.toml` (the project's existing 1.88.0 pin) as its source of truth (Resolution C); `taiki-e/install-action@nextest` installs `cargo-nextest` ahead of the test job. Dependabot is configured for `cargo` + `github-actions` ecosystems (weekly, grouped minor+patch, separate major PRs — Resolution D).

The `notes/proposals/ci-cd-pipeline.md` proposal is promoted to `docs/specs/ci-pipeline.md` v1.0.0, scoped to CI-only (Resolution E); the release-automation / OSSF-Scorecard / CodeQL framing stays in the archived proposal as deferred future work. `notes/proposals/ci-cd-pipeline-stories.md` archives alongside the proposal; its acceptance criteria absorb into the spec's § Examples / § Edge Cases sections and into this workplan's § Shipping criteria.

The CI pipeline is **strictly additive** to the daemon: no `src/` changes; no test-suite changes (other than CI invocation shape); no behavioral changes. The trust boundary, ADR commitments, and operator-facing CLI / config surface are unchanged. Branch protection (recommended GitHub UI configuration) is documented in the spec's § Branch Protection section but not enforced by code.

---

## Deferred-decision resolutions

The five workplan-time deferred decisions from [`roadmap-5.md`](./roadmap-5.md) § Step 13 § Deferred decisions to resolve at workplan-time are pulled forward to workplan-time per the round-3/4 spec-fleshout-at-workplan-write discipline. All five resolved here.

### A. Exact SHA hashes for all four actions

**Resolution**: pin to commit SHAs verified against the `gh api` at workplan-write (2026-04-28). Each `uses:` line carries an inline comment naming the human-readable version.

| Action | Pin (commit SHA) | Comment | Resolution mode |
|--------|------------------|---------|-----------------|
| `actions/checkout` | `34e114876b0b11c390a56381ad16ebd13914f8d5` | `# v4.3.1` | `git/refs/tags/v4` resolves to commit (lightweight tag); v4.3.1 is the latest v4.x release per `gh api repos/actions/checkout/releases`. |
| `dtolnay/rust-toolchain` | `29eef336d9b2848a0b548edc03f92a220660cdb8` | `# stable (branch HEAD as of 2026-04-28; rust-toolchain.toml drives actual toolchain — see Resolution C)` | `git/refs/heads/stable` (branch alias); the action reads `rust-toolchain.toml` when present. |
| `Swatinem/rust-cache` | `e18b497796c12c097a38f9edb9d0641fb99eee32` | `# v2.9.1` | `git/refs/tags/v2` is annotated; derefs to commit `e18b...` (v2.9.1, latest v2.x). |
| `taiki-e/install-action` | `a987447a36adfd8769c91cf36dd91c79b8452fe0` | `# nextest (named alias for the nextest install path)` | `git/refs/tags/nextest` resolves to commit (lightweight tag); maintainer-curated alias matching the `@nextest` callout in the proposal. |

**Verification recipe** (Task 13.3 re-runs at task-time, before committing):

```sh
gh api repos/actions/checkout/git/refs/tags/v4               --jq '.object.sha'
# expect: 34e114876b0b11c390a56381ad16ebd13914f8d5
gh api repos/dtolnay/rust-toolchain/branches/stable          --jq '.commit.sha'
# expect: 29eef336d9b2848a0b548edc03f92a220660cdb8
gh api repos/Swatinem/rust-cache/git/tags/$(gh api repos/Swatinem/rust-cache/git/refs/tags/v2 --jq '.object.sha') --jq '.object.sha'
# expect: e18b497796c12c097a38f9edb9d0641fb99eee32
gh api repos/taiki-e/install-action/git/refs/tags/nextest    --jq '.object.sha'
# expect: a987447a36adfd8769c91cf36dd91c79b8452fe0
```

If any of these have advanced between workplan-write and task-execution, **Task 13.3 takes the new SHA** and records the drift in its results comment. The principle is "pin to the most recent verified SHA at the moment the workflow lands"; the workplan-time values are the floor, not the lock.

**Note on Xcind reference drift**: the proposal cites Xcind as the security-practice reference. Xcind has since moved to `actions/checkout@v6.0.2` (`de0fac2e4500dabe0009e67214ff5f5447ce83dd`). Step 13 stays at v4.3.1 per the proposal's explicit `@v4` callout — bumping major versions is a separate decision that can ride the first Dependabot PR (which is exactly the system we're shipping). Recording the v6 reference here so the first Dependabot PR knows it's an expected bump, not a surprise.

### B. `cargo nextest` profile for CI

**Resolution**: add a new `.config/nextest.toml` file with a `[profile.ci]` block. CI invokes `cargo nextest run --profile ci`. Local `just test` (default profile) is unchanged.

The proposal flags JUnit XML output and conservative retries as candidates. The cleanest shape is a profile in the standard nextest config location rather than inline `--reporter junit` flags — the inline flags are repeated each run and easy to drift; a profile centralizes the CI runner config.

`.config/nextest.toml` (full file content; this file does not exist yet):

```toml
# Default profile applies to local `cargo nextest run` and `just test`.
# It currently inherits nextest's built-in defaults; named here for forward-compat.
[profile.default]

# CI profile — invoked from `.github/workflows/ci.yml` as `cargo nextest run --profile ci`.
# Differences from default:
#   - immediate-final failure output: prints failures inline AND at the end (CI logs are read top-to-bottom).
#   - JUnit XML report at target/nextest/ci/junit.xml (consumed by GitHub Actions test reporting).
#   - 0 retries: a flake in CI is a signal we want to see, not silence (the v0 anti-flake stance).
[profile.ci]
failure-output = "immediate-final"
final-status-level = "all"
retries = 0
slow-timeout = { period = "60s", terminate-after = 2 }

[profile.ci.junit]
path = "junit.xml"
```

**Why retries = 0**: the round-2/3/4 anti-flake convention (3× consecutive flake-check clean before shipping) has been maintained without retry magic. CI inheriting that stance keeps the same signal-to-noise. If the outbox flake (`tests/outbox.rs::rename_emits_deleted_then_created_lines`) reproduces in CI, that's a step-14 input — masking it with retries would defeat the purpose.

**Slow-timeout 60s × 2 = 120s effective**: the current full suite runs in ~30s locally; 120s gives 4× headroom on a cold runner without over-tolerating a runaway test.

**Test-threads**: not set (use nextest's default). The current 540 tests run in parallel with no shared global state per the proposal; if CI reveals an isolation gap, surface as a `coordinator-only` soft flag and decide whether to add `test-threads = "num-cpus"` or pin to a lower count.

**Justfile interaction**: no change. Local `just test` continues to invoke `cargo nextest run` (default profile). A future addition could add `just test-ci` as a local CI-shape rehearsal; not required for this step. Surface as `coordinator-only` if Task 13.2 finds a reason to add it.

**File location**: `.config/nextest.toml` is the standard nextest discovery path (per the nextest docs); no `--config-file` flag needed in the CI invocation.

### C. `rust-toolchain.toml` vs. `dtolnay/rust-toolchain@stable`

**Resolution**: rely on `rust-toolchain.toml` as the single source of truth for the toolchain version + components. The `dtolnay/rust-toolchain@stable` action invocation passes **no `with: toolchain:` override** and **no `with: components:` override** — both come from the toml file.

The project's `rust-toolchain.toml` (verified at workplan-write):

```toml
[toolchain]
channel    = "1.88.0"
components = ["rustfmt", "clippy", "rust-src", "rust-analyzer"]
profile    = "minimal"
```

`dtolnay/rust-toolchain` documented behavior: when `rust-toolchain.toml` is present in the checkout, the action reads `channel`, `components`, and `targets` from the file and installs accordingly. The `@stable` selector on the `uses:` line becomes inert (the file overrides it). The action's repo recommends pinning to a commit SHA (Resolution A) plus optionally a `with:` block; we pass an empty `with:` block.

**CI workflow shape** (Task 13.3 builds from this):

```yaml
- uses: dtolnay/rust-toolchain@29eef336d9b2848a0b548edc03f92a220660cdb8 # stable (rust-toolchain.toml drives the actual version)
  # No `with:` overrides — rust-toolchain.toml is the source of truth.
```

**Component coverage**: `rustfmt` (needed by `format` job) and `clippy` (needed by `lint` job) are both listed in `rust-toolchain.toml`. `rust-src` and `rust-analyzer` are local-developer conveniences with negligible install cost on CI; we don't strip them. `profile = "minimal"` keeps the install footprint small.

**`rust-toolchain.toml` change cadence**: any future edit to the toolchain pin propagates automatically through CI on the next push. No CI-side change required.

**Verification at Task 13.3**: the test job logs should show `rustc 1.88.0` (or whatever the toml says at run-time); confirm in the manual smoke (Task 13.5) by reading the first few lines of the test job log output.

### D. Dependabot PR frequency and grouping

**Resolution**: weekly cadence; group minor+patch updates into a single PR per ecosystem; majors land as separate PRs (so they get individual review). Cover both `cargo` and `github-actions` ecosystems.

Single grouped PR per week per ecosystem keeps the PR queue manageable while still surfacing each new dependency change. Major updates often require code changes — separating them avoids a grouped PR being blocked by a single major bump.

`.github/dependabot.yml` (full file content):

```yaml
version: 2
updates:
  # Cargo dependencies — direct deps in Cargo.toml.
  - package-ecosystem: cargo
    directory: /
    schedule:
      interval: weekly
      day: monday
      time: "06:00"
      timezone: America/Los_Angeles
    target-branch: main
    open-pull-requests-limit: 5
    labels:
      - dependencies
      - cargo
    groups:
      cargo-minor-and-patch:
        patterns:
          - "*"
        update-types:
          - "minor"
          - "patch"
        # Major updates land as separate PRs (require human review for breaking changes).

  # GitHub Actions used in workflows (the SHA-pinned ones from Resolution A).
  - package-ecosystem: github-actions
    directory: /
    schedule:
      interval: weekly
      day: monday
      time: "06:00"
      timezone: America/Los_Angeles
    target-branch: main
    open-pull-requests-limit: 5
    labels:
      - dependencies
      - github-actions
    groups:
      github-actions-minor-and-patch:
        patterns:
          - "*"
        update-types:
          - "minor"
          - "patch"
```

**Why both ecosystems**: the proposal calls out cargo only, but the workflow YAML is itself a maintenance surface (the four pinned action SHAs need updating as upstream releases land). Without `github-actions` ecosystem, those SHAs go stale and start drifting from the Xcind reference's security stance.

**Why `open-pull-requests-limit: 5`**: round-5 is a maintenance round; we want Dependabot to surface backlog without overwhelming. 5 open PRs per ecosystem (10 total cap) is a reasonable ceiling. Adjustable later if it proves too high or too low.

**Labels**: `dependencies` + per-ecosystem label allows GitHub UI filtering. No PR template required (Dependabot auto-generates an informative description).

**Timezone**: `America/Los_Angeles` matches the project's authoring timezone (per the human's working hours pattern in `notes/coordinator-playbook.md` and round retros). PRs land Monday 06:00 PT — early enough to be triaged in the week.

### E. Spec promotion scope

**Resolution**: promote `notes/proposals/ci-cd-pipeline.md` → `docs/specs/ci-pipeline.md` v1.0.0, scoping the spec to CI-only. Release-automation / OSSF-Scorecard / CodeQL / Windows-CI framing **does not** carry forward to the spec — those stay in the archived proposal as historical context for future workplans. Stories file archives alongside the proposal.

**Sections to carry forward to `docs/specs/ci-pipeline.md`**:

- **Overview** — rewritten to scope to CI-only; remove the "lays groundwork for future release automation" framing toward "CI is the round-5-shipping piece; release automation is round-6+ workstream tracked in the backlog."
- **Behavior § Primary Workflow: CI Pipeline** — full carry-forward with concrete SHAs from Resolution A inlined into § Action Selection.
- **Behavior § Secondary Workflow: Dependabot Integration** — full carry-forward, updated with Resolution D's grouping.
- **Data Schema § Workflow Configuration** — full carry-forward; replace the abstract step lists with the concrete action invocations from Task 13.3.
- **Data Schema § Job Input/Output Schema** — full carry-forward; add the `--profile ci` invocation from Resolution B and the `rust-toolchain.toml` source-of-truth note from Resolution C.
- **Examples § Successful CI Run / Failed CI Run / Workflow File Structure** — full carry-forward; update the file-structure tree to omit `release.yml` (out of scope) and add `dependabot.yml` location (`.github/dependabot.yml`, not `.github/workflows/dependabot.yml`).
- **Edge Cases § Test Isolation, Platform-Specific, Test Suite Growth, External Service Dependencies** — full carry-forward.
- **Error Handling § Test Failure, Workflow Infrastructure** — full carry-forward.
- **Error Handling § Security Considerations** — partial carry-forward; keep "supply chain security" + "code security" sub-sections; **drop** the OSSF Scorecard reference (defer to round-6+ per [`backlog.md`](../backlog.md) § Round-6 candidates).
- **Integration Points § Pre-existing Hypomnema Infrastructure** — full carry-forward; verify the Justfile-mirroring claim against the current Justfile at task time.
- **Integration Points § External GitHub Features § Branch Protection Integration** — promote to a top-level **§ Branch Protection** section (per the roadmap's shipping-criteria callout). Document recommended GitHub UI configuration; explicitly note this is operator guidance, not enforced by code.
- **Integration Points § External GitHub Features § PR/Issue Integration** — full carry-forward.
- **Implementation Notes § Cargo Nextest Integration** — rewrite around Resolution B's `[profile.ci]` shape.
- **Implementation Notes § Action Selection Rationale** — full carry-forward; **inline the four concrete SHAs** from Resolution A; reduce "future security posture" wording so OSSF-Scorecard reads as a clear backlog item, not an imminent extension.
- **Implementation Notes § Performance Optimization** — full carry-forward.

**Sections to drop from the promoted spec** (stay in the archived proposal):

- **Behavior § Future Workflow Hooks § Release Pipeline Integration Points** — release automation is round-6+.
- **Behavior § Future Workflow Hooks § Security/Compliance Integration Points** — OSSF Scorecard, CodeQL: round-6+.
- **Implementation Notes § Future Extension Readiness § Release Pipeline Preparation** — release automation framing.
- **Open Questions § Release Automation Integration** — round-6+ open question.
- **Open Questions § Windows Platform Support** — round-6+ open question; the spec's § Branch Protection notes the Unix-only matrix as scope decision (not as an open question).

**Open Questions in the promoted spec** — keep these (with workplan-time-deferral framing):

- **Test Suite Reliability in CI** — keep as-is (monitor for 2–3 weeks after implementation; round-5-step-14 is the dedicated outbox-flake investigation step that consumes any signal CI surfaces).
- **Performance vs Coverage Trade-off** — keep as-is.

**Stories file disposition** (matches step-12 Resolution H): `notes/proposals/ci-cd-pipeline-stories.md` → `notes/proposals/archive/ci-cd-pipeline-stories.md`. No content change — its acceptance criteria absorb into the spec's § Edge Cases / § Examples sections (already present in narrative form) plus this workplan's § Shipping criteria.

**ADR strategy**: **no new ADR for step 13**. The CI pipeline is process / infrastructure, not a daemon-architecture decision. Existing ADRs (0001–0013) all govern daemon shape; adding ADR-0014 for "we have CI" would dilute the ADR layer's load-bearing role. The promoted spec at `docs/specs/ci-pipeline.md` is the canonical artifact. If a load-bearing principle surfaces during build (e.g., "all releases ride CI green" as a project-wide rule), surface as a `coordinator-only` soft flag at boundary; ADR-0014 can land as a step-13 boundary follow-up if warranted, not as a precondition.

**Cross-references** to update at promotion (Task 13.1 verifies each at task-time):

- `docs/architecture/overview.md` — add a one-sentence note in § Quality Attributes (or wherever the project-level quality framing lives) referencing the new spec as the project's CI gate. Cross-reference: `docs/specs/ci-pipeline.md`.
- `docs/reference/configuration.md` — no change anticipated (CI doesn't expose runtime configuration); verify-then-skip per the soft-flag self-correction pattern.
- `docs/implementation/tech-stack.md` — verify-then-amend; the proposal's § Integration Points § Pre-existing Hypomnema Infrastructure references `tech-stack.md` as covering existing toolchain. If adding a one-line "CI runs the Justfile commands" pointer reads cleanly, do it; otherwise skip.
- `notes/backlog.md` — strike the round-5 candidate "CI pipeline (GitHub Actions)" entry (already marked as pulled-into-round-5 per `backlog.md` line 21); add round-6 candidates that surface from Resolution E's deferrals (release automation, OSSF Scorecard, Windows CI matrix — all already present in `backlog.md` § Round-6 candidates per the current state, but verify).
- The proposal at `notes/proposals/ci-cd-pipeline.md` is moved to `notes/proposals/archive/ci-cd-pipeline.md` per the proposals lifecycle in [`notes/proposals/README.md`](../proposals/README.md).

---

## Self-review for prose accuracy

This workplan is projected at ~750–850 lines (similar shape to step 12's 737-line CI-shape sibling — though step 12's surface was substantially different — and well below step 1's ~1000-line heuristic threshold). The round-1 large-workplan trigger does not fire. Spot-check on testable claims:

### Internal-shape claims

1. **`rust-toolchain.toml`** is at `1.88.0` with `components = ["rustfmt", "clippy", "rust-src", "rust-analyzer"]` and `profile = "minimal"` (verified at workplan-write by reading the file). Resolution C depends on this exact shape.
2. **Justfile** has `lint = cargo clippy --all-targets -- -D warnings`, `test = cargo nextest run`, `fmt = cargo fmt --all` (verified at workplan-write). The CI workflow's commands match the Justfile commands by intent (the spec's § Integration Points calls this out as a deliberate parity goal).
3. **`Cargo.toml`** is at `version = "0.3.0"` (verified at workplan-write). Step 13 does not bump the version (round-5 shipping gate is step 15, not this step).
4. **No `.github/` directory exists** at workplan-write (verified by `ls /Users/beausimensen/Code/hypomnema/.github/` returning "No such file or directory"). Task 13.3 + 13.4 create it.
5. **`.config/` directory does not exist** at workplan-write (verified by `ls`). Task 13.2 creates it for the new nextest config file.
6. **Negative-fingerprint** for the CI scope: `rg 'release\.yml|cross-compile|cargo-dist|goreleaser' .github/` returns zero matches after Task 13.3 + 13.4 ship (per the roadmap's shipping criteria; the only files in `.github/` after the step ships are `workflows/ci.yml` and `dependabot.yml`).

### External-library / external-service claims

1. **`actions/checkout@v4.3.1`** SHA `34e114876b0b11c390a56381ad16ebd13914f8d5` — verified at workplan-write via `gh api repos/actions/checkout/git/refs/tags/v4 --jq '.object.sha'`. Lightweight tag; the SHA is the commit directly.
2. **`dtolnay/rust-toolchain@stable`** branch HEAD `29eef336d9b2848a0b548edc03f92a220660cdb8` — verified at workplan-write via `gh api repos/dtolnay/rust-toolchain/branches/stable --jq '.commit.sha'`. Branch (not tag); HEAD moves over time, hence the explicit `# stable (branch HEAD as of 2026-04-28)` comment.
3. **`Swatinem/rust-cache@v2`** annotated-tag → commit `e18b497796c12c097a38f9edb9d0641fb99eee32` (v2.9.1) — verified at workplan-write via two-step deref (`git/refs/tags/v2` → annotated-tag SHA `42dc...` → `git/tags/<tag-sha>` → commit SHA).
4. **`taiki-e/install-action@nextest`** SHA `a987447a36adfd8769c91cf36dd91c79b8452fe0` — verified at workplan-write via `gh api repos/taiki-e/install-action/git/refs/tags/nextest --jq '.object.sha'`. Lightweight tag; the maintainer-curated alias for nextest installation.
5. **`dtolnay/rust-toolchain` reads `rust-toolchain.toml`** when present in the checkout. Verified by reading the action's README at `https://github.com/dtolnay/rust-toolchain` — the action documents the `rust-toolchain.toml` interaction explicitly. Task 13.3 verifies the runtime behavior in the test job's log.
6. **GitHub-hosted runner Rust pre-install**: `ubuntu-latest` (currently `ubuntu-24.04`) and `macos-latest` (currently `macos-14`) ship with a stable Rust pre-installed. The `dtolnay/rust-toolchain` action overrides this with the `rust-toolchain.toml`-pinned 1.88.0. Verified by inspection at task time (the test job log shows `rustc 1.88.0` after the action runs).

### Cross-platform claims

1. **No filesystem operations** in the CI workflow YAML. The actions handle checkout, Rust install, cache, and nextest install; the test invocation is a single `cargo nextest run --profile ci` shell command.
2. **macOS-vs-Ubuntu test parity**: round-1 through round-4 have shipped without Linux CI. The full suite passes locally on macOS (the project's primary dev platform); first-run on `ubuntu-latest` may surface platform-specific issues. **If first-run fails on Ubuntu**, the failure is the signal — do not paper over with platform-conditional skips. Investigate, fix, and re-run. Surface as `next-task-agent` soft flag for Task 13.5 if it materializes.

---

## Tasks

The 6-task decomposition is small for a step (rounds 1–4 averaged 7–8 tasks per step) but matches the doc-only / YAML-only surface. Per the round-1/2/3/4 default-not-batch rule (now 12-of-12 consecutive clean steps), tasks ship as solo agents. Each task ships its own commit per the playbook's TASK AGENT § Reporting; risk grades and dependencies noted at each task header.

### Task 13.1 — Spec promotion + stories archive + arch overview cross-reference + backlog updates

**Risk**: low. Doc-only by design; mechanical-mostly. Lands first because tasks 13.2–13.4 reference the promoted spec by path, and Task 13.5 (manual smoke) verifies against the spec's shipping criteria.

**Scope**:

- **Promote the spec** (Resolution E):
  - Move `notes/proposals/ci-cd-pipeline.md` content → new file `docs/specs/ci-pipeline.md`. Bump `Version: 1.0.0` (kept; the proposal was already at v1.0.0 — bumping to v1.0.1 or higher is unnecessary since the spec hasn't shipped previously). `Status: Draft` → `Status: Approved`. `Date: 2026-04-28` → set to the actual ship date (likely 2026-04-28 or later).
  - Apply the section carry-forward / drop list from Resolution E.
  - Inline the four pinned-action SHAs (Resolution A) into § Action Selection Rationale.
  - Inline the `[profile.ci]` shape (Resolution B) into § Cargo Nextest Integration.
  - Inline the `rust-toolchain.toml`-as-source-of-truth note (Resolution C) into § Job Input/Output Schema § Rust Installation.
  - Promote the existing § Integration Points § External GitHub Features § Branch Protection Integration content to a top-level **§ Branch Protection** section. Document recommended GitHub UI configuration: status checks `format`, `lint`, `test (ubuntu-latest)`, `test (macos-latest)`; require conversation resolution; require linear history (or whatever the project's git policy is — verify at task time by reading the existing `notes/coordinator-playbook.md` § COORDINATOR or human's git policy notes; default to "require all status checks to pass before merge, allow administrator override for CI-infrastructure issues" per the proposal's shipping language).
  - Add a § Revision History entry: `1.0.0 | <date> | Promoted from notes/proposals/ci-cd-pipeline.md (was Draft 1.0.0 dated 2026-04-28). Round-5 step 13 workplan resolutions: Resolution A (SHAs pinned with verified commit hashes), Resolution B (.config/nextest.toml ci profile + JUnit XML), Resolution C (rust-toolchain.toml drives toolchain via dtolnay/rust-toolchain action), Resolution D (Dependabot weekly + grouped minor/patch + cargo + github-actions ecosystems), Resolution E (CI-only scope; release automation framing stays in archived proposal).`
  - Rewrite cross-references to LDS-relative paths per `_template.md` conventions: `../docs/...` → `../{layer}/...` (verify each at task time).
  - Add a § Related Documents section with cross-links to `docs/architecture/overview.md`, `docs/implementation/tech-stack.md`, `docs/decisions/0005-local-everything.md` (the trust-boundary ADR that the project-level scope inherits — even though CI is process, the "no external services beyond GitHub" framing aligns with ADR-0005's spirit). Add the Xcind reference URL.

- **Move the original proposal**: `notes/proposals/ci-cd-pipeline.md` → `notes/proposals/archive/ci-cd-pipeline.md` (no content change; the file's role is done). The dropped sections (release automation, OSSF Scorecard framing, etc.) live here as the historical record per Resolution E.

- **Archive the stories file**: `notes/proposals/ci-cd-pipeline-stories.md` → `notes/proposals/archive/ci-cd-pipeline-stories.md` (no content change).

- **Sync `docs/architecture/overview.md`**:
  - Add a one-sentence reference to the new spec in the Quality Attributes / project-level framing section. The location depends on the file's current shape — verify at task time. If the existing arch overview has a "Quality Gates" or "Build & Test" subsection, the sentence lands there; otherwise add a small new subsection.
  - Cross-reference: `docs/specs/ci-pipeline.md`.

- **Verify-then-amend `docs/implementation/tech-stack.md`**: if the existing tech-stack doc references the Justfile commands (`just lint`, `just test`, `just fmt`) without acknowledging that CI now runs the same commands, add a one-sentence cross-reference. If the doc already covers CI generically (unlikely; CI didn't exist), add a brief subsection. If the addition reads forced, skip per the soft-flag self-correction pattern.

- **Update `notes/backlog.md`**:
  - Verify the "CI pipeline (GitHub Actions)" entry under § Round-5 candidates is already struck through (per current `backlog.md` line 21). If not, strike it.
  - Verify the round-6-candidates entries for "Release automation", "OSSF Scorecard / CodeQL", "Windows CI matrix" are already present (they are, per current `backlog.md`). If any are missing, add them with a back-reference to the archived proposal.
  - Add any boundary follow-ups surfaced by the promotion work.

- **Forward-references**: the promoted spec adds a forward-reference at the bottom of § Implementation Notes pointing to `notes/roadmap/archive/step-13-workplan.md` (post-archive) for the workplan-time resolutions of A–E.

**Tests**: doc-only; no code tests in this task. Verify post-edit:
- `cargo doc --no-deps` runs cleanly (catches any rustdoc cross-link breakage if markdown paths get referenced from doc-comments).
- No broken cross-references in the promoted spec: spot-check 5 random cross-refs.
- `find docs/ -name "*.md" -exec grep -l 'docs/specs/ci-pipeline' {} \;` returns at least one match (the arch overview cross-ref).

**Files touched**:
- New: `docs/specs/ci-pipeline.md`, `notes/proposals/archive/ci-cd-pipeline.md`, `notes/proposals/archive/ci-cd-pipeline-stories.md`.
- Removed: `notes/proposals/ci-cd-pipeline.md`, `notes/proposals/ci-cd-pipeline-stories.md` (now in archive).
- Edited: `docs/architecture/overview.md`, `notes/backlog.md`, possibly `docs/implementation/tech-stack.md` (verify-then-amend).

**Dependencies**: none. Lands first; the canonical foundation for tasks 13.2–13.5.

**Soft-flag-ready territory**:
- The `notes/proposals/archive/` directory exists (verified at workplan-write — `ls notes/proposals/` shows `archive` already present from round-4's mcp-streamable-http archival).
- `tech-stack.md` may already mention CI generically; verify-then-amend per the soft-flag self-correction pattern. Surface as a `coordinator-only` flag if the verify pass shows no drift.
- The Branch Protection section's "require linear history" sub-decision: not a load-bearing CI claim, but worth getting right. Default to recommending it (matches typical Rust project posture); surface as `coordinator-only` if the human has prior branch-policy preferences (none documented at workplan-write — check `notes/coordinator-playbook.md` and any CONTRIBUTING.md if present).
- Revision History entry's date: workplan-write date is 2026-04-28; if Task 13.1 ships on a later date, use the actual ship date.

### Task 13.2 — Nextest CI profile (`.config/nextest.toml`)

**Risk**: low. Single new config file; no behavioral change for local `cargo nextest run` (default profile inherits nextest's built-ins — explicit `[profile.default]` is forward-compat naming, not a value change).

**Scope**:

- **Create `.config/nextest.toml`** with the exact content from Resolution B above. The directory `.config/` does not exist yet; `mkdir -p .config/` is the first step.

- **Verify nextest discovery**: run `cargo nextest run --help | grep -i 'config'` (or `cargo nextest --version` and consult the docs) to confirm `.config/nextest.toml` is the discovery path nextest uses. The standard path per nextest docs.

- **Verify default profile parity**: run `cargo nextest run` locally; the suite should pass with the same shape it did before (no change to default behavior).

- **Smoke the `ci` profile locally**: run `cargo nextest run --profile ci`. Expected outputs:
  - The same test results (540+ tests, all green).
  - Failure-output is `immediate-final` (run with one deliberately-skipped test if you want to verify the failure shape; otherwise just confirm the profile is recognized).
  - JUnit XML at `target/nextest/ci/junit.xml`.
- Verify: `ls target/nextest/ci/junit.xml` after the run; the file should be valid XML.

- **Justfile interaction**: no change. Local `just test` continues with default profile. **Do not** add a `just test-ci` recipe at this time (Resolution B's "future addition could add" — not required for this step). If the manual smoke (Task 13.5) reveals a need for local CI rehearsal, surface as a step-13 boundary follow-up.

- **README / CLAUDE.md / AGENTS.md**: no documentation update required. The `.config/nextest.toml` file is self-documenting via its inline comments. The spec promoted in Task 13.1 covers the rationale.

**Tests**: as above (default profile parity + ci profile smoke).

**Files touched**:
- New: `.config/nextest.toml`.

**Dependencies**: none directly, but ordering keeps the workplan coherent (13.1 establishes canon; 13.2 adds the config the workflow YAML in 13.3 depends on).

**Soft-flag-ready territory**:
- If `cargo nextest run --profile ci` fails for a reason other than test failure (e.g., nextest version doesn't recognize `failure-output = "immediate-final"`), the task investigates: nextest 0.9.x vs newer version, the local install vs the version `taiki-e/install-action@nextest` provides. Surface as `next-task-agent` soft flag for Task 13.3.
- The `slow-timeout` value (60s × 2) is a heuristic; if local smoke shows a test exceeding 60s on the slowest machine, raise it. Surface as `coordinator-only`.

### Task 13.3 — `.github/workflows/ci.yml` (the CI workflow file)

**Risk**: medium. The wiring task. **Load-bearing for Task 13.5's manual smoke.** Wires four SHA-pinned actions, three jobs, and a 2-OS matrix into a single YAML file that GitHub will execute on every push and PR.

**Scope**:

- **Create `.github/` and `.github/workflows/`**: both directories do not exist (verified at workplan-write).

- **Re-verify SHAs at task-time** by running the verification recipe from Resolution A. If any SHA has advanced since 2026-04-28, take the new SHA and update the workflow + record the drift in the task's results comment.

- **Author `.github/workflows/ci.yml`**:

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]
  workflow_dispatch:

permissions:
  contents: read

jobs:
  format:
    name: format
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@34e114876b0b11c390a56381ad16ebd13914f8d5 # v4.3.1
      - uses: dtolnay/rust-toolchain@29eef336d9b2848a0b548edc03f92a220660cdb8 # stable (rust-toolchain.toml drives the actual version)
      - uses: Swatinem/rust-cache@e18b497796c12c097a38f9edb9d0641fb99eee32 # v2.9.1
        with:
          shared-key: format
      - name: cargo fmt --check
        run: cargo fmt --all -- --check

  lint:
    name: lint
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@34e114876b0b11c390a56381ad16ebd13914f8d5 # v4.3.1
      - uses: dtolnay/rust-toolchain@29eef336d9b2848a0b548edc03f92a220660cdb8 # stable (rust-toolchain.toml drives the actual version)
      - uses: Swatinem/rust-cache@e18b497796c12c097a38f9edb9d0641fb99eee32 # v2.9.1
        with:
          shared-key: lint
      - name: cargo clippy
        run: cargo clippy --all-targets -- -D warnings

  test:
    name: test
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, macos-latest]
    runs-on: ${{ matrix.os }}
    timeout-minutes: 30
    steps:
      - uses: actions/checkout@34e114876b0b11c390a56381ad16ebd13914f8d5 # v4.3.1
      - uses: dtolnay/rust-toolchain@29eef336d9b2848a0b548edc03f92a220660cdb8 # stable (rust-toolchain.toml drives the actual version)
      - uses: Swatinem/rust-cache@e18b497796c12c097a38f9edb9d0641fb99eee32 # v2.9.1
        with:
          shared-key: test-${{ matrix.os }}
      - uses: taiki-e/install-action@a987447a36adfd8769c91cf36dd91c79b8452fe0 # nextest (named alias)
      - name: cargo nextest run
        run: cargo nextest run --profile ci
      - name: Upload JUnit XML
        if: always()
        uses: actions/upload-artifact@<resolve-at-task-time>
        with:
          name: nextest-junit-${{ matrix.os }}
          path: target/nextest/ci/junit.xml
          if-no-files-found: warn
          retention-days: 7
```

- **Resolve `actions/upload-artifact` SHA at task time** (the workplan does not pre-pin this — it's a fifth action surface that the proposal didn't enumerate). Use `gh api repos/actions/upload-artifact/git/refs/tags/v4 --jq '.object.sha'` (or whichever major version is current; v4.x is the long-stable line as of late 2025). Inline the SHA + `# v4.x.y` comment into the YAML. Add this fifth action to `dependabot.yml`'s github-actions ecosystem coverage automatically (it already covers `*` patterns, so no edit needed in 13.4).

- **Why `shared-key` per job in rust-cache**: Swatinem/rust-cache shares cache across jobs that share a key. Splitting per-job (`format`, `lint`, `test-ubuntu-latest`, `test-macos-latest`) prevents cache thrashing — `cargo fmt --check` doesn't compile, but `cargo clippy` does, and the test build differs from the clippy build by features/profiles. Per-job cache isolates these.

- **Why `timeout-minutes: 30` on test only**: the proposal's edge-case § Large Test Suite Growth flags the timeout; current ~30s runtime gives 60× headroom. format and lint don't need explicit timeouts (they run in <5min by default).

- **Why JUnit upload `if: always()`**: capture results even on test failure for post-mortem. `if-no-files-found: warn` avoids erroring the workflow on edge cases (e.g., the test runner crashes before writing JUnit).

- **Verify the YAML lints cleanly**: run `actionlint .github/workflows/ci.yml` if `actionlint` is available locally (Nix flake may already include it; check via `which actionlint`). If not available, ship without — GitHub validates the YAML on push (any syntax error will fail-fast on first run, and Task 13.5 will catch it).

**Tests**: workflow YAML; no Rust tests. Verify post-edit:
- `actionlint .github/workflows/ci.yml` (if available; surface a `coordinator-only` flag if not).
- Visual diff against the spec's § Workflow Configuration section to catch any drift.

**Files touched**:
- New: `.github/workflows/ci.yml`.

**Dependencies**: 13.1 (the spec is the canonical reference for what the workflow should match); 13.2 (the `--profile ci` invocation requires `.config/nextest.toml`).

**Soft-flag-ready territory**:
- The `actions/upload-artifact@v4.x.y` SHA pin: the workplan defers to task-time. Surface as `coordinator-only` if the resolved SHA differs materially from the v4.x line (e.g., if v5 has shipped and v4 has been deprecated by Dependabot's standards, choose v5).
- `actionlint` availability: workplan assumes it may or may not be in the dev shell. If absent, ship without local lint — first push catches any syntax error. Surface as `coordinator-only` to record the choice.
- If the first push surfaces a runner-image vs `rust-toolchain.toml` interaction we didn't predict (e.g., `dtolnay/rust-toolchain` doesn't actually read `rust-toolchain.toml` on a particular runner image), surface as `next-task-agent` for Task 13.5 (smoke).
- Cache hit-rate: the workplan picks `shared-key` per job; if the first manual-smoke run shows cache misses on subsequent runs (no warm-up benefit), revisit. Surface as `coordinator-only` for boundary.

### Task 13.4 — `.github/dependabot.yml` (Dependabot config)

**Risk**: low. Single new YAML file; no immediate behavioral change (Dependabot first runs on the next scheduled trigger after merge). Pure additive surface.

**Scope**:

- **Create `.github/dependabot.yml`** with the exact content from Resolution D above.

- **Verify YAML syntax**: same `actionlint` check as Task 13.3 if available (actionlint covers GitHub workflow + dependabot files). Otherwise, GitHub validates on push.

- **Verify Dependabot recognizes the file**: post-merge, GitHub UI's "Dependabot" tab should show two ecosystems active. Not part of this task's verification (lives in Task 13.5's manual smoke).

- **README / CLAUDE.md / AGENTS.md**: no update required. The Dependabot file is self-documenting; the spec promoted in Task 13.1 covers the rationale.

**Tests**: as above; YAML syntax only.

**Files touched**:
- New: `.github/dependabot.yml`.

**Dependencies**: 13.1 (the spec covers dependabot framing); none on 13.2 / 13.3 (independent file).

**Soft-flag-ready territory**:
- The `open-pull-requests-limit: 5` value is a heuristic; if Dependabot generates a flood on first run (unlikely — there are <30 direct cargo deps + 4 actions), the limit caps it. If the limit is too low and some updates are perpetually queued, surface as `coordinator-only` for boundary follow-up.
- Timezone choice (`America/Los_Angeles`): Mac-default project authoring timezone. If the human prefers UTC for predictability, surface as `coordinator-only` flag at task time (verify against any existing timezone preferences in `notes/coordinator-playbook.md`).
- The `groups:` syntax: Dependabot's grouping syntax has evolved across 2024–2025. The shape used in Resolution D is the current (2026-04) supported format. If Dependabot's UI rejects the file post-merge, the syntax may have drifted — surface as a build-failure escalation; Resolution D's group config is the load-bearing piece.

### Task 13.5 — Manual smoke: feature branch + GitHub Actions run + observe-then-merge

**Risk**: medium-high. **Load-bearing for the step's shipping criteria.** First time the workflow actually runs against a remote service (GitHub Actions). Verifies the SHAs, the YAML, the runner image compatibility, the rust-toolchain.toml interaction, the nextest profile, and the JUnit upload all compose correctly.

**Scope**:

- **Push the workflow + dependabot files to a feature branch**, not directly to `main`. Branch name: `step-13-ci-pipeline-smoke` (or whatever the human prefers — clarify before push if ambiguous).

- **Open a PR against `main`** with the four files (`docs/specs/ci-pipeline.md`, `.config/nextest.toml`, `.github/workflows/ci.yml`, `.github/dependabot.yml`) plus any cross-reference edits from Task 13.1. The PR triggers the CI workflow; observe.

- **Verify each job runs green**:
  - `format (ubuntu-latest)` — `cargo fmt --check` passes.
  - `lint (ubuntu-latest)` — `cargo clippy --all-targets -- -D warnings` passes.
  - `test (ubuntu-latest)` — `cargo nextest run --profile ci` passes; JUnit XML uploaded.
  - `test (macos-latest)` — same; JUnit XML uploaded.
- **Capture transcripts**: copy the relevant log fragments inline into the task's results comment per the round-2/3/4 manual-smoke precedent. At minimum: the `rustc --version` output (verifying 1.88.0 from `rust-toolchain.toml`); the test job's "Tests passed" summary; the JUnit upload artifact link.

- **Iterate on failure**: if any job fails, do not paper over. Investigate root cause:
  - Format failure on Ubuntu: rustfmt edition / config differences? — likely a real fix, not a CI quirk.
  - Lint failure on Ubuntu: clippy warnings the local pre-commit didn't catch? — same; real fix.
  - Test failure on Linux that doesn't reproduce on macOS: this is **the high-value signal**. Capture it; root-cause; fix or characterize. If it's `tests/outbox.rs::rename_emits_deleted_then_created_lines` (the round-4 carry-forward flake), it feeds directly into step 14 — surface as `coordinator-only` soft flag with the run URL captured (do not block step 13 on a step-14-scoped flake; first verify it's not a step-13-introduced regression).
  - Test failure on macOS that didn't reproduce locally: investigate platform-vs-CI environment differences (PATH, env vars, runner image version).

- **Verify the JUnit XML artifact**: download from the Actions UI; spot-check that it's valid XML and lists the test count expected (~540+).

- **Verify Dependabot recognizes `.github/dependabot.yml`**: post-merge to main, the GitHub UI's "Dependabot" or "Insights → Dependency graph → Dependabot" tab should show two ecosystems active. Capture screenshot or text confirmation.

- **Verify branch-protection recommendations land cleanly**: do **not** enforce branch protection in this task — the spec documents the recommendation; enforcement is the human's call (operator action, not code action). Surface a `coordinator-only` soft flag if the human wants to gate merge on CI checks now (one-paragraph guidance to add status-check requirements in GitHub UI after this PR merges).

- **Re-run the workflow** at least once via `workflow_dispatch` (manual trigger) on the feature branch to verify the trigger works and that subsequent runs benefit from cache (cache hit on rust-cache; faster runtime).

- **Merge**: only after all four jobs are green and the smoke transcripts are captured. The PR landing on `main` is the step's shipping moment.

**Tests**: GitHub Actions runs are the test. No additional Rust tests in this task.

**Files touched**: none new (this is a verification task; the files are from 13.1–13.4).

**Dependencies**: 13.1, 13.2, 13.3, 13.4. Lands second-to-last; 13.6 boundary closes after this.

**Soft-flag-ready territory**:
- Outbox flake reproduction in CI: surface as `coordinator-only` for step 14 input. Do NOT fix in step 13 (out of scope; step 14 owns it).
- Runner image quirks (e.g., a `cargo nextest` install path mismatch on macOS-14): document and decide between fix-now or boundary-follow-up. Surface as `coordinator-only` if not blocking.
- Cache hit/miss patterns on the second `workflow_dispatch` run: capture the timing delta in the task's results comment for future reference.
- If first push produces a workflow YAML syntax error (caught by GitHub's validator), iterate on the YAML in 13.3-style fix-up commits on the same feature branch. Each fix is a commit per playbook convention.

### Task 13.6 — Boundary verification + roadmap-5 status update

**Risk**: low. Doc-only by design; lands at boundary so any forward-noted soft-flag corrections from earlier tasks can be incorporated. Per-step boundary (not round-shipping boundary; that's step 15).

**Scope**:

- **Verify `docs/specs/ci-pipeline.md`** v1.0.0 from Task 13.1 is consistent with shipped reality. Apply the round-3-step-10 "soft-flag self-correction at boundary" pattern: read the current file, compare against the actual workflow YAML + nextest config + dependabot config that shipped, only edit if drift is real.

- **Verify `docs/architecture/overview.md`** cross-reference from Task 13.1 reads cleanly post-shipped. Same verify-before-editing pattern.

- **Verify `notes/backlog.md`** updates from Task 13.1 are still accurate after any iteration in 13.5.

- **Update `notes/roadmap/roadmap-5.md` § Step 13 status**:
  - Add `**Status**: Shipped <date>` at top of Step 13 section.
  - Cross-reference the workplan archive path: `notes/roadmap/archive/step-13-workplan.md` (post-archival).
  - Note any deferred-decision drift: e.g., if Resolution A's SHAs were updated at task-time, record the final SHAs.

- **Apply the per-step boundary ritual** per [`notes/project-planning-workflow-notes.md`](../project-planning-workflow-notes.md) § Step boundary ritual:
  1. Mark step 13 done in roadmap (above).
  2. Capture any ADRs that hardened during the build (anticipated: none — see ADR strategy in Resolution E; surface as `coordinator-only` if a load-bearing principle emerged).
  3. Update the roadmap if reality drifted from the original plan.
  4. Append a per-step retro to `notes/project-planning-workflow-notes.md` § Retrospectives following the template (Structured Eval + Notes).
  5. Push HEAD to `origin/main` per the round-3-postmortem-driven addition (push the merge commit; no tag at per-step boundaries).
  6. Expand step 14 (outbox flake hardening) into a workplan — **but** that's the next coordinator's responsibility, not Task 13.6's. Task 13.6 closes step 13; the orchestrator's next round starts step 14's workplan-write phase.

- **Step-14 input handoff**: if Task 13.5's manual smoke surfaced the outbox flake in CI, Task 13.6 captures the run URL + symptom in the per-step retro as load-bearing input for step 14's workplan author. This is the step-13-to-step-14 forward-note channel.

- **Update `notes/backlog.md`** with any step-13 boundary follow-ups surfaced by Tasks 13.1–13.5's soft flags.

**Tests**: doc-only; no code tests in this task. `cargo doc --no-deps` runs cleanly post-edit (defensive check).

**Files touched**: `notes/roadmap/roadmap-5.md`, `notes/project-planning-workflow-notes.md` (per-step retro append), possibly `docs/specs/ci-pipeline.md` (verify-then-amend), `docs/architecture/overview.md` (verify-then-amend), `notes/backlog.md`. The workplan archive itself (`notes/roadmap/step-13-workplan.md` → `notes/roadmap/archive/step-13-workplan.md`) is part of the post-task boundary ritual run by the coordinator after this task ships.

**Dependencies**: 13.1–13.5. Lands last.

**Soft-flag-ready territory**:
- Forward-noted soft-flag reconciliations from earlier tasks (likely 2–4 of them per the round-3/4 stable pattern). Apply the round-3-step-10 "soft-flag self-correction at boundary" rule: verify the prose is current before editing; the prior task's observation may have been the drift.
- ADR-0014 candidacy: surface as `coordinator-only` if a load-bearing CI-related principle emerged that warrants ADR-layer documentation. Default: skip (CI is process; the spec covers it).
- Step-14 input handoff: surface the outbox flake reproduction (or non-reproduction) as a `coordinator-only` flag for step-14's workplan author.

---

## Shipping criteria

The step ships when **all** of these hold:

- [ ] `.github/workflows/ci.yml` exists, passes GitHub's YAML validation, and runs three named jobs: `format`, `lint`, `test` (matrix `ubuntu-latest` + `macos-latest`).
- [ ] All three jobs run green against the feature branch (Task 13.5) and stay green when the PR merges to `main`.
- [ ] `.github/dependabot.yml` exists, passes GitHub's validation, and shows two active ecosystems (`cargo` + `github-actions`) in the GitHub UI Dependabot tab post-merge.
- [ ] `.config/nextest.toml` exists with `[profile.ci]` block; local `cargo nextest run` (default) and `cargo nextest run --profile ci` both pass against the current suite.
- [ ] `docs/specs/ci-pipeline.md` v1.0.0 exists, status `Approved`, with all five workplan-time resolutions baked in (SHAs, nextest profile, rust-toolchain.toml interaction, dependabot grouping, CI-only scope cut). § Branch Protection section documents recommended GitHub UI configuration.
- [ ] `notes/proposals/ci-cd-pipeline.md` and `notes/proposals/ci-cd-pipeline-stories.md` archived to `notes/proposals/archive/`.
- [ ] `docs/architecture/overview.md` references the new CI spec as a project quality gate (one-sentence cross-reference).
- [ ] Negative-fingerprint: `rg 'release\.yml|cross-compile|cargo-dist|goreleaser' .github/` returns zero matches.
- [ ] Positive-fingerprint: `rg 'cargo nextest run --profile ci' .github/` returns at least one match (the test job invocation); `rg '34e114876b0b11c390a56381ad16ebd13914f8d5' .github/` returns at least three matches (one per job using `actions/checkout`); `rg 'transport-streamable-http' .github/` returns zero matches (no daemon-internal references in CI files).
- [ ] All four pinned-action SHAs match Resolution A's verified values (or any task-time-resolved drift documented in Task 13.3's results comment).
- [ ] `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo nextest run --profile ci` are all green locally before any `.github/` commit lands — CI must start green.
- [ ] The full test suite (`cargo nextest run`) is green locally on macOS (the dev platform) before the feature branch is pushed; CI verifies Linux green on first push.
- [ ] JUnit XML artifact `nextest-junit-ubuntu-latest` and `nextest-junit-macos-latest` are uploaded by Task 13.5's run; spot-check confirms valid XML with the expected test count.
- [ ] `notes/roadmap/roadmap-5.md` § Step 13 marked `**Status**: Shipped <date>` (Task 13.6).
- [ ] Per-step retro appended to `notes/project-planning-workflow-notes.md` § Retrospectives (Task 13.6).
- [ ] `notes/backlog.md` has any step-13 boundary follow-ups surfaced by soft flags.
- [ ] Push to `origin/main` after merge (per round-3-postmortem boundary-ritual addition; no tag at per-step boundary).
- [ ] One commit per task per the playbook (Task 13.5's manual smoke can use the round-3/4 inline-transcripts pattern; the manual-smoke task does not produce a code commit, but its inline-transcripts results comment is the artifact).

---

## Step boundary follow-ups (anticipated)

- **Release automation** (Resolution E — deferred): when binary distribution becomes a project goal. `release.yml`, cross-compilation, checksums, cargo-dist or goreleaser. Already in `notes/backlog.md` § Round-6 candidates.
- **OSSF Scorecard / CodeQL** (Resolution E — deferred): when the project has public visibility. Already in `notes/backlog.md` § Round-6 candidates.
- **Windows CI matrix** (Resolution E — deferred): when Windows support becomes a project goal. Already in `notes/backlog.md` § Round-6 candidates.
- **`actions/checkout` major bump** (Resolution A — workplan-time choice): v4.3.1 ships in step 13; v6.0.2 is what Xcind uses. The first Dependabot PR will likely propose the major bump. Decide at PR-review time whether to take the bump now or stay on v4.x.
- **Per-tool nextest configuration tuning** (Resolution B — heuristic defaults): if CI runtime grows beyond ~10 minutes, evaluate `test-threads`, partition strategy, or split into fast / full jobs. Already in promoted spec § Open Questions.
- **Local CI-shape rehearsal recipe** (Resolution B — `just test-ci`): if developers want to mirror CI exactly before pushing, add a Justfile recipe in a future round. Surface at boundary if a contributor (or the human) requests it.
- **Branch-protection enforcement**: spec documents the recommendation; the human-as-operator decides whether to enforce in GitHub UI. Step-13 boundary follow-up if not done at PR-merge time.
- **ADR-0014 candidacy** (no-op default): if a load-bearing CI-related principle emerges during build (e.g., "all releases ride CI green"), add an ADR. Anticipated outcome: skip; the spec is sufficient.
- **CHANGELOG.md adoption** (round-5 step 15): step 13's CI infrastructure work is one of the items the round-5 CHANGELOG entry will cover. Step 13 does not write the CHANGELOG; step 15 does.
- **Outbox flake (`tests/outbox.rs::rename_emits_deleted_then_created_lines`)**: if reproduces in step-13's first CI run, that's high-value step-14 input. If silent, that's continued silence-as-data. Step 14 owns the investigation.

---

## Notes on workplan-write deferred-decision handling

The five workplan-time deferred decisions per [`roadmap-5.md`](./roadmap-5.md) § Step 13 § Deferred decisions to resolve at workplan-time are resolved in § Deferred-decision resolutions above:

- **Resolution A** — Exact SHA hashes for all four actions: pinned to commit SHAs verified at workplan-write via `gh api`. Inline `# version` comments name the human-readable version. Task-time re-verification is the load-bearing check.
- **Resolution B** — `cargo nextest` profile for CI: new `.config/nextest.toml` with `[profile.ci]` block including JUnit XML output, immediate-final failure shape, and 0 retries (preserves anti-flake signal). CI invokes `cargo nextest run --profile ci`. Local `just test` unchanged.
- **Resolution C** — `rust-toolchain.toml` vs. `dtolnay/rust-toolchain@stable`: `rust-toolchain.toml` is the source of truth; the action reads it when present and overrides its `@stable` selector. No `with: toolchain:` or `with: components:` overrides on the action invocation.
- **Resolution D** — Dependabot PR frequency and grouping: weekly Monday 06:00 PT; group minor+patch into one PR per ecosystem; majors as separate PRs. Two ecosystems: `cargo` + `github-actions`.
- **Resolution E** — Spec promotion scope: CI-only. Release-automation framing stays in archived proposal; stories file archives alongside. No new ADR.

The promoted spec at `docs/specs/ci-pipeline.md` v1.0.0 ships with these resolutions baked in. § Open Questions retains only the deferred ones (test-suite-reliability monitoring, performance-vs-coverage tradeoff). Future rounds can pull deferred-OQ resolutions without a canon rewrite — the round-3/4 LDS pattern.

---

## Notes on step-13 boundary

This step is a **per-step** boundary, not a round-shipping boundary (round-5 ships at step 15, after step 14's outbox-flake investigation). The boundary ritual is the per-step variant per [`notes/project-planning-workflow-notes.md`](../project-planning-workflow-notes.md) § Step boundary ritual.

Boundary ritual sequence (run by coordinator after Task 13.6 ships):

1. **Mark step 13 shipped** in `notes/roadmap/roadmap-5.md` § Step 13 with shipping date.
2. **No git tag** — per-step boundary; round-5 milestone tag is `v0.4.0` at step 15.
3. **No `Cargo.toml` version bump** — same reasoning.
4. **Capture any ADRs that hardened during the build** — anticipated: none (see Resolution E ADR strategy). Surface as `coordinator-only` soft flag if a load-bearing principle emerged.
5. **Per-step retro for step 13** in `notes/project-planning-workflow-notes.md` § Step 13 (Task 13.6 drafts; coordinator finalizes after merge).
6. **No end-of-round retro** — step 15 owns the round-5 close-out.
7. **Archive step-13 workplan**: `notes/roadmap/step-13-workplan.md` → `notes/roadmap/archive/step-13-workplan.md` per the step-archival policy.
8. **Update `notes/backlog.md`** with step-13 boundary follow-ups (already partially seeded by Task 13.6).
9. **Push HEAD to `origin/main`** after merge per round-3-postmortem boundary-ritual addition.
10. **Step-14 workplan-write** is the next coordinator's responsibility; the orchestrator triggers it after step 13 closes.

Step 13's structural question for the per-step retro is: "did the spec-fleshout-at-workplan-write discipline still apply cleanly for an infrastructure-shape step (no `src/` changes, pure YAML + Markdown)? Did the manual-smoke-as-load-bearing-quality-gate pattern hold for a CI-shape smoke (push + observe Actions, vs. round-3/4's curl + JSON-RPC against a local daemon)?" The 12-of-12-clean-steps cadence is on track; step 13 is the 13th data point.
