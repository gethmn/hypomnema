# CI Pipeline Specification

**Version**: 1.0.0
**Date**: 2026-04-28
**Status**: Approved

---

> **File Location**: `docs/specs/ci-pipeline.md`

---

## Overview

This spec defines the GitHub Actions CI pipeline for Hypomnema. Every push to `main` and every pull request targeting `main` triggers three parallel quality-gate jobs: `format` (formatting check), `lint` (clippy), and `test` (nextest on Ubuntu + macOS matrix). CI is the round-5-shipping piece; release automation is a round-6+ workstream tracked in the backlog.

The design follows Hypomnema's "solo-local-cli" scope profile: Unix-focused, no external dependencies beyond GitHub infrastructure, SHA-pinned actions for supply chain security, `rust-toolchain.toml` as the single source of truth for the toolchain version.

**Related Documents**:
- [Architecture Overview](../architecture/overview.md) — system structure and quality attributes
- [Tech Stack](../implementation/tech-stack.md) — existing toolchain (Justfile, cargo-nextest, clippy)
- [ADR-0005: Local Everything](../decisions/0005-local-everything.md) — local-first principles; CI's "no external deps beyond GitHub" framing aligns with this ADR's spirit
- [Xcind CI Reference](https://github.com/scinddev/xcind/tree/main/.github/workflows) — security practices and workflow patterns followed here

---

## Behavior

### Primary Workflow: CI Pipeline

**Trigger Conditions:**
- Push to `main` branch
- Pull request targeting `main` branch
- Manual workflow dispatch (`workflow_dispatch`)

**Execution Flow:**
1. **Parallel quality-gate jobs** (fail-fast disabled):
   - `format` — `cargo fmt --all -- --check` on `ubuntu-latest`
   - `lint` — `cargo clippy --all-targets -- -D warnings` on `ubuntu-latest`
   - `test` — `cargo nextest run --profile ci` on `ubuntu-latest` + `macos-latest` (matrix; `fail-fast: false`)

2. **Platform matrix** for test job:
   - `ubuntu-latest` (primary)
   - `macos-latest` (secondary)

3. **Fast feedback optimization**:
   - Jobs run in parallel
   - Format/lint jobs complete quickly (~1–2 minutes)
   - Test job runs on both platforms with Cargo build caching via `Swatinem/rust-cache`

### Secondary Workflow: Dependabot Integration

Dependabot is configured for both `cargo` and `github-actions` ecosystems (weekly Monday 06:00 PT cadence; minor+patch updates grouped into a single PR per ecosystem; major updates land as separate PRs for individual review). See § Dependabot Configuration.

---

## Data Schema

### Workflow Configuration

The complete workflow file at `.github/workflows/ci.yml`:

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
        uses: actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02 # v4.6.2
        with:
          name: nextest-junit-${{ matrix.os }}
          path: target/nextest/ci/junit.xml
          if-no-files-found: warn
          retention-days: 7
```

### Dependabot Configuration

The complete configuration file at `.github/dependabot.yml`:

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

  # GitHub Actions used in workflows (the SHA-pinned ones from § Action Selection Rationale).
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

### Job Input/Output Schema

**Rust Installation:**
- Source of truth: `rust-toolchain.toml` (channel `1.88.0`, components `rustfmt clippy rust-src rust-analyzer`, profile `minimal`)
- The `dtolnay/rust-toolchain` action reads `rust-toolchain.toml` when present and installs accordingly; no `with: toolchain:` or `with: components:` overrides are passed — the toml file drives the actual version, superseding the action's `@stable` selector on the `uses:` line
- Cache: `Swatinem/rust-cache` per-job with `shared-key` per job name (isolates cache across the format, lint, and test builds)

**Test Execution (CI profile):**
- Runner: `cargo nextest run --profile ci`
- Profile defined in `.config/nextest.toml` (see § Nextest CI Profile)
- Timeout: 30 minutes per job (60× headroom over current ~30s suite runtime)
- Artifact preservation: JUnit XML at `target/nextest/ci/junit.xml`, uploaded via `actions/upload-artifact` with `if: always()` (captures results even on test failure)
- Failure handling: `fail-fast: false` so both matrix platforms complete even if one fails

**Linting Configuration:**
- Clippy: `cargo clippy --all-targets -- -D warnings` (matches `just lint`)
- Format: `cargo fmt --all -- --check` (non-destructive verification; matches `just fmt`)

### Nextest CI Profile

Defined in `.config/nextest.toml`:

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

**Why retries = 0**: the project's anti-flake convention (3× consecutive flake-check clean before shipping) has been maintained without retry magic across rounds 1–4. CI inheriting that stance keeps the same signal-to-noise. If the outbox flake (`tests/outbox.rs::rename_emits_deleted_then_created_lines`) reproduces in CI, that's step-14 input — masking it with retries would defeat the purpose.

---

## Examples

### Successful CI Run Output

```
✅ format (ubuntu-latest) - 1m 23s
✅ lint (ubuntu-latest) - 2m 15s
✅ test (ubuntu-latest) - 4m 42s
✅ test (macos-latest) - 5m 18s

All checks passed - ready to merge
```

### Failed CI Run Output

```
✅ format (ubuntu-latest) - 1m 23s
❌ lint (ubuntu-latest) - 2m 15s
   clippy::too_many_arguments in src/api/search.rs:45
✅ test (ubuntu-latest) - 4m 42s
❌ test (macos-latest) - 5m 18s
   test multi_vault_internal::two_vaults_index_in_isolation ... FAILED

2 of 4 checks failed
```

### Workflow File Structure

```
.github/
├── dependabot.yml              # Dependabot config (this spec)
└── workflows/
    └── ci.yml                  # Core CI pipeline (this spec)
```

Note: `release.yml` is explicitly out of scope for round 5. Release automation is a round-6+ workstream; see `notes/proposals/archive/ci-cd-pipeline.md` for the deferred framing.

---

## Edge Cases

### Test Isolation and Concurrency

**Problem**: Integration tests create temp directories and spawn daemon processes. Parallel test execution could cause port conflicts or filesystem races.

**Mitigation**: The test suite handles isolation via:
- Unique temp directory per test (PID + timestamp + counter pattern)
- Dynamic port allocation in HTTP tests
- No shared global state between test cases

**CI Consideration**: Nextest runs tests in parallel by default. Current evidence (540+ tests, ~30s runtime) suggests isolation is working. Monitor for flaky tests indicating isolation breakage.

### Platform-Specific Test Failures

**Problem**: Tests might pass on developer's macOS but fail on Ubuntu CI, or vice versa.

**Mitigation**:
- Test matrix includes both platforms
- `fail-fast: false` so both platforms complete even if one fails
- Platform differences in temp directory handling already accounted for in test suite

**First-run risk**: round 1–4 shipped without Linux CI. If first-run fails on Ubuntu, investigate root cause — do not paper over with platform-conditional skips.

### Large Test Suite Growth

**Problem**: As Hypomnema grows, test suite runtime may exceed reasonable CI limits.

**Future Mitigation Strategy**:
- Nextest supports test partitioning for parallel runners
- Can split integration tests vs unit tests into separate jobs
- The `timeout-minutes: 30` cap gives 60× headroom over current ~30s runtime; if CI runtime consistently approaches 15 minutes, evaluate splitting

### External Service Dependencies

**Problem**: Tests that require the embedding service (TEI/vLLM) would fail in CI.

**Current Status**: Tests use mock embedding endpoints. No external service dependencies in the test suite.

**Future Consideration**: If real embedding service tests are added, gate them behind feature flags or a separate workflow.

---

## Error Handling

### Test Failure Handling

**Philosophy**: Fail loudly and provide actionable feedback.

**Implementation**:
- Individual job failures do not cancel other jobs (`fail-fast: false`)
- Test artifacts preserved on failure (nextest JUnit XML via `if: always()`)
- Clear failure messages in PR status checks
- Branch protection rules prevent merge on CI failure (see § Branch Protection)

### Workflow Infrastructure Failures

**GitHub Actions Reliability**:
- Network timeouts: Automatic retry for transient failures (GitHub-hosted runner behavior)
- Runner availability: GitHub-hosted runners (high availability)
- Authentication: Default `GITHUB_TOKEN` only (no secret management required)

**Rust Toolchain Reliability**:
- Pin to `1.88.0` via `rust-toolchain.toml` (avoids nightly/toolchain-drift breakage)
- `Swatinem/rust-cache` reduces registry load and speeds builds
- `rust-toolchain.toml` is the single source of truth — any future toolchain bump propagates automatically through CI on the next push

### Security Considerations

**Supply Chain Security**:
- All five actions pinned to specific commit SHAs (see § Action Selection Rationale)
- Read-only permissions by default (`contents: read`)
- No secrets required for basic CI

**Code Security**:
- GitHub native secret scanning (enabled by default on public repos)
- Dependabot for dependency updates (both `cargo` and `github-actions` ecosystems)
- Future: OSSF Scorecard integration — explicitly deferred to round 6+ (see `notes/proposals/archive/ci-cd-pipeline.md` § Security/Compliance Integration Points)

---

## Integration Points

### Pre-existing Hypomnema Infrastructure

**Justfile Integration** (CI commands mirror Justfile commands exactly):
- `just lint` → `cargo clippy --all-targets -- -D warnings`
- `just test` → `cargo nextest run` (default profile; CI uses `--profile ci`)
- `just fmt` → `cargo fmt --all --check` (non-destructive in CI)

The CI workflow runs the same commands the Justfile runs locally. The `--profile ci` addition to the test job is the only delta; it changes output formatting and enables JUnit XML, not test selection or invocation.

**Cargo.toml Compatibility**:
- Respects `edition = "2024"` → uses stable rust toolchain via `rust-toolchain.toml`
- Uses existing dev-dependencies (`tempfile`, `tower` for tests)
- No additional dependencies required for CI

### External GitHub Features

**PR/Issue Integration**:
- Status checks visible in PR interface: `format`, `lint`, `test (ubuntu-latest)`, `test (macos-latest)`
- Test failure details linked from PR status
- Workflow re-run capability for transient failures

---

## Branch Protection

Recommended GitHub UI configuration for `main` branch protection. This is operator guidance — enforcing branch protection is a GitHub UI action, not a code action.

**Recommended settings**:
- **Required status checks before merging**: `format`, `lint`, `test (ubuntu-latest)`, `test (macos-latest)` (all four)
- **Require branches to be up to date before merging**: enabled
- **Require conversation resolution before merging**: enabled
- **Require linear history**: recommended (matches the project's commit-per-task-per-playbook discipline)
- **Allow administrator override**: enabled (for CI infrastructure issues, e.g., a GitHub Actions outage blocking a critical fix)

**Enforcement timing**: configure after the first green CI run on `main` (Task 13.5 manual smoke). Before CI is confirmed green, branch protection may block you from iterating on the workflow YAML itself.

---

## Implementation Notes

### Cargo Nextest Integration

**Why Nextest**: Already in use in the Justfile; provides better parallel execution, cleaner output formatting, and JUnit XML reporting compared to standard `cargo test`.

**CI Profile Details** (from `.config/nextest.toml` § Nextest CI Profile):
- `failure-output = "immediate-final"`: prints failures inline as they occur AND summarizes at the end — CI logs are read top-to-bottom
- `final-status-level = "all"`: all test outcomes listed at summary
- `retries = 0`: anti-flake stance — a flake in CI is signal, not noise to suppress
- `slow-timeout = { period = "60s", terminate-after = 2 }`: 120s effective ceiling (4× headroom over current ~30s full-suite runtime)
- JUnit XML at `target/nextest/ci/junit.xml` (consumed by GitHub Actions test reporting via `actions/upload-artifact`)

**Local vs CI behavior**: local `just test` runs the default profile (unchanged); CI runs `--profile ci`. The distinction is output shape and JUnit XML generation, not test selection.

### Action Selection Rationale

**Pinned actions** (following Xcind security practices; SHA values verified at workplan-write 2026-04-28):

| Action | SHA | Comment | Notes |
|--------|-----|---------|-------|
| `actions/checkout` | `34e114876b0b11c390a56381ad16ebd13914f8d5` | `# v4.3.1` | Lightweight tag; v4.3.1 is the latest v4.x release |
| `dtolnay/rust-toolchain` | `29eef336d9b2848a0b548edc03f92a220660cdb8` | `# stable (branch HEAD as of 2026-04-28)` | Branch alias; reads `rust-toolchain.toml` when present |
| `Swatinem/rust-cache` | `e18b497796c12c097a38f9edb9d0641fb99eee32` | `# v2.9.1` | Annotated tag deref to commit (v2.9.1, latest v2.x) |
| `taiki-e/install-action` | `a987447a36adfd8769c91cf36dd91c79b8452fe0` | `# nextest (named alias)` | Maintainer-curated lightweight tag for nextest |
| `actions/upload-artifact` | `ea165f8d65b6e75b540449e92b4886f43607fa02` | `# v4.6.2` | Used for JUnit XML upload; pinned at task-time |

**SHA verification recipe** (re-run at task time before committing):

```sh
gh api repos/actions/checkout/git/refs/tags/v4               --jq '.object.sha'
gh api repos/dtolnay/rust-toolchain/branches/stable          --jq '.commit.sha'
# Swatinem/rust-cache uses annotated tag — two-step deref:
gh api repos/Swatinem/rust-cache/git/refs/tags/v2            --jq '.object.sha'
# (use that SHA in the next call to get the commit SHA)
gh api repos/taiki-e/install-action/git/refs/tags/nextest    --jq '.object.sha'
gh api repos/actions/upload-artifact/git/refs/tags/v4        --jq '.object.sha'
```

If any SHA has advanced between workplan-write and task-execution, take the new SHA and record the drift in the task results comment. The principle: pin to the most recent verified SHA at the moment the workflow lands.

**Security posture**:
- All actions pinned to specific commit SHAs (not semver tags, which are mutable)
- Minimal permission grants (`contents: read`)
- No third-party secret access required

**Note on Xcind reference drift**: the Xcind project has moved to `actions/checkout@v6.0.2` (`de0fac2e4500dabe0009e67214ff5f5447ce83dd`). Hypomnema stays at v4.3.1 per the proposal's explicit `@v4` callout; the first Dependabot PR will propose a major bump for individual review.

### Performance Optimization

**Cache Strategy** (via `Swatinem/rust-cache`):
- Cargo registry cache (dependencies)
- Target directory cache (build artifacts)
- `shared-key` per job isolates caches across the format, lint, and test builds — prevents the non-compiling `cargo fmt --check` job from poisoning the build cache used by `cargo clippy` and `cargo nextest`

**Expected Runtime** (with warm cache):
- Format job: ~1–2 minutes
- Lint job: ~2–3 minutes
- Test job: ~4–6 minutes (including nextest parallelism)
- Total pipeline: ~6–8 minutes (parallel execution)

---

## Open Questions

### Test Suite Reliability in CI

**Current Unknown**: How often does the existing test suite produce false negatives in the CI environment vs local development?

**Resolution Approach**: Monitor for 2–3 weeks after implementation. If the flaky-test rate exceeds 5%, investigate specific failure patterns. The pre-existing outbox flake (`tests/outbox.rs::rename_emits_deleted_then_created_lines`) has been silent across rounds 1–4; if it reproduces in CI, that feeds directly into step 14 (the dedicated outbox-flake investigation step).

### Performance vs Coverage Trade-off

**Current Decision**: Run the full test suite on every CI run for maximum coverage.

**Future Decision Point**: If CI runtime consistently exceeds 10 minutes, evaluate splitting into:
- Fast feedback job (unit tests only, ~100 fastest tests)
- Complete validation job (full integration suite)
- Matrix strategy for different test categories

---

## Revision History

| Version | Date | Changes |
|---------|------|---------|
| 1.0.0 | 2026-04-28 | Promoted from `notes/proposals/ci-cd-pipeline.md` (was Draft 1.0.0 dated 2026-04-28). Round-5 step 13 workplan resolutions applied: Resolution A (SHA-pinned actions with verified commit hashes including `actions/upload-artifact`), Resolution B (`.config/nextest.toml` `[profile.ci]` with JUnit XML, immediate-final output, 0 retries), Resolution C (`rust-toolchain.toml` drives toolchain via `dtolnay/rust-toolchain` action — no `with:` overrides), Resolution D (Dependabot weekly + grouped minor/patch + `cargo` + `github-actions` ecosystems), Resolution E (CI-only scope; release automation / OSSF Scorecard / Windows CI framing stays in archived proposal). § Branch Protection promoted from sub-section to top-level section. Forward-references and cross-links updated to LDS-relative paths. |

---

## Related Documents

- [Architecture Overview](../architecture/overview.md) — CI gate noted in § Quality Attributes
- [Tech Stack](../implementation/tech-stack.md) — existing toolchain surface CI mirrors
- [ADR-0005: Local Everything](../decisions/0005-local-everything.md) — local-first principles; CI's no-external-deps-beyond-GitHub framing aligns
- [Xcind CI Reference](https://github.com/scinddev/xcind/tree/main/.github/workflows) — security practices followed here
- Archived proposal: `notes/proposals/archive/ci-cd-pipeline.md` — historical context including deferred release-automation and OSSF Scorecard framing
- Step-13 workplan (post-archive): `notes/roadmap/archive/step-13-workplan.md` — workplan-time resolutions A–E documented in full
