# CI/CD Pipeline Implementation

**Version**: 1.0.0  
**Date**: 2026-04-28  
**Status**: Draft

---

## Overview

This spec defines a GitHub Actions-based CI/CD pipeline for Hypomnema that provides fast feedback on code quality while maintaining the flexibility to evolve release processes independently. The design prioritizes developer velocity with basic quality gates (tests, linting, formatting) while laying groundwork for future release automation.

The pipeline follows Hypomnema's "solo-local-cli" scope profile: Unix-focused, no external dependencies beyond GitHub infrastructure, modular design that separates continuous integration from release processes.

## Behavior

### Primary Workflow: CI Pipeline

**Trigger Conditions:**
- Push to `main` branch
- Pull request targeting `main` branch  
- Manual workflow dispatch (`workflow_dispatch`)

**Execution Flow:**
1. **Parallel quality gate jobs** (fail-fast disabled):
   - Formatting check (`cargo fmt --check`)
   - Linting (`cargo clippy -- -D warnings`)
   - Test suite (`cargo nextest run`)

2. **Platform matrix** for test job:
   - ubuntu-latest (primary)
   - macos-latest (secondary)

3. **Fast feedback optimization**:
   - Jobs run in parallel where possible
   - Formatting/linting jobs complete quickly (~1-2 minutes)
   - Test job takes longer (~30 seconds currently) but runs on both platforms

### Secondary Workflow: Dependabot Integration

**Trigger Conditions:**
- Pull request from dependabot
- Scheduled weekly dependency review

**Execution Flow:**
- Same CI pipeline as primary workflow
- Additional dependency review action (GitHub native)

### Future Workflow Hooks (Spec Extension Points)

**Release Pipeline Integration Points:**
- Distinct workflow file: `.github/workflows/release.yml` 
- Triggered on: GitHub release publication (`release: types: [published]`)
- Artifacts: Platform-specific binaries, checksums, release notes
- No dependency on CI pipeline success (release process can be independently developed)

**Security/Compliance Integration Points:**
- OSSF Scorecard workflow (following Xcind pattern)
- CodeQL analysis (optional, may add later)
- Secret scanning (GitHub native, enabled by default)

## Data Schema

### Workflow Configuration

```yaml
# .github/workflows/ci.yml
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
    runs-on: ubuntu-latest
    steps: [checkout, install-rust, cargo-fmt-check]
    
  lint:
    runs-on: ubuntu-latest  
    steps: [checkout, install-rust, cargo-clippy-deny-warnings]
    
  test:
    runs-on: ${{ matrix.os }}
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, macos-latest]
    steps: [checkout, install-rust, install-nextest, cargo-nextest-run]
```

### Job Input/Output Schema

**Rust Installation:**
- Toolchain: `stable` (follows Hypomnema's current Cargo.toml edition = "2024")
- Components: `rustfmt`, `clippy` (explicit installation)
- Cache: Cargo registry, git dependencies, build artifacts

**Test Execution:**
- Runner: `cargo nextest run` (matching existing Justfile)
- Timeout: 30 minutes (generous buffer beyond current ~30s runtime)
- Artifact preservation: Test reports (junit XML format via nextest)
- Failure handling: Continue on test failures to collect full matrix results

**Linting Configuration:**
- Clippy: `cargo clippy --all-targets -- -D warnings` (matching Justfile)
- Format: `cargo fmt --all --check` (non-destructive verification)

## Examples

### Successful CI Run Output

```yaml
✅ format (ubuntu-latest) - 1m 23s
✅ lint (ubuntu-latest) - 2m 15s  
✅ test (ubuntu-latest) - 4m 42s
✅ test (macos-latest) - 5m 18s

All checks passed - ready to merge
```

### Failed CI Run Output

```yaml
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
└── workflows/
    ├── ci.yml              # Core CI pipeline (this spec)
    ├── dependabot.yml      # Dependency review workflow  
    └── release.yml         # Future: release automation (separate spec)
```

## Edge Cases

### Test Isolation and Concurrency

**Problem**: Integration tests create temp directories and spawn daemon processes. Parallel test execution could cause port conflicts or filesystem races.

**Mitigation**: Current test suite already handles isolation via:
- Unique temp directory per test (PID + timestamp + counter pattern)  
- Dynamic port allocation in HTTP tests
- No shared global state between test cases

**CI Consideration**: Nextest runs tests in parallel by default. Current evidence (540 tests, ~30s runtime) suggests isolation is working. Monitor for flaky tests indicating isolation breakage.

### Platform-Specific Test Failures

**Problem**: Tests might pass on developer's macOS but fail on Ubuntu CI, or vice versa.

**Mitigation**: 
- Test matrix includes both platforms
- Fail-fast disabled so both platforms complete even if one fails
- Platform differences in temp directory handling already accounted for in test suite

### Large Test Suite Growth

**Problem**: As Hypomnema grows, test suite runtime may exceed reasonable CI limits.

**Future Mitigation Strategy**:
- Nextest supports test partitioning for parallel runners
- Can split integration tests vs unit tests into separate jobs
- Can add test result caching for unchanged code paths

### External Service Dependencies

**Problem**: Tests that require embedding service (TEI/vLLM) would fail in CI.

**Current Status**: Tests use mock embedding endpoints. No external service dependencies in test suite.

**Future Consideration**: If real embedding service tests are added, they should be gated behind feature flags or separate workflow.

## Error Handling

### Test Failure Handling

**Philosophy**: Fail loudly and provide actionable feedback.

**Implementation**:
- Individual job failures don't cancel other jobs (fail-fast: false)
- Test artifacts preserved on failure (nextest JUnit XML output)
- Clear failure messages in PR status checks
- Branch protection rules prevent merge on CI failure

### Workflow Infrastructure Failures

**GitHub Actions Reliability**:
- Network timeouts: Automatic retry for transient failures
- Runner availability: Use GitHub-hosted runners (high availability)
- Authentication: Use default `GITHUB_TOKEN` (no secret management)

**Rust Toolchain Reliability**:
- Pin to stable channel (avoid nightly breakage)
- Use rust-cache action (faster builds, reduced registry load)
- Explicit component installation (avoid implicit dependencies)

### Security Considerations

**Supply Chain Security**:
- Pin action versions with SHA hashes (following Xcind pattern)
- Read-only permissions by default (`contents: read`)
- No secrets required for basic CI

**Code Security**:
- GitHub native secret scanning (already enabled)
- Future: OSSF Scorecard integration (following Xcind model)
- Dependabot for dependency updates

## Integration Points

### Pre-existing Hypomnema Infrastructure

**Justfile Integration**:
- CI commands mirror Justfile commands exactly
- `just lint` → `cargo clippy --all-targets -- -D warnings`
- `just test` → `cargo nextest run`
- `just fmt` → `cargo fmt --all --check` (non-destructive in CI)

**Cargo.toml Compatibility**:
- Respects edition = "2024" → use stable rust toolchain
- Uses existing dev-dependencies (tempfile, tower for tests)
- No additional dependencies required for CI

### Future Integration Points

**Release Pipeline Boundaries**:
- CI workflow success is NOT a prerequisite for release workflow
- Release workflow can be developed/deployed independently
- Common pattern: CI on every PR/push, Release on GitHub release publication
- Shared artifact patterns: binary checksums, platform detection, cross-compilation

**Development Tool Integration**:
- Pre-commit hook compatibility (if adopted)
- IDE integration unchanged (CI matches local `just` commands)
- Developer local runs remain authoritative

### External GitHub Features

**Branch Protection Integration**:
- Status checks: `format`, `lint`, `test (ubuntu-latest)`, `test (macos-latest)`
- Require all checks to pass before merge
- Allow administrator override (for CI infrastructure issues)

**PR/Issue Integration**:
- Status checks visible in PR interface
- Test failure details linked from PR status
- Workflow re-run capability for transient failures

## Implementation Notes

### Cargo Nextest Integration

**Why Nextest**: Already in use in Justfile, provides better parallel execution and output formatting than standard `cargo test`.

**CI-Specific Configuration**:
- JUnit XML output for GitHub Actions test reporting
- Retry configuration for flaky tests (conservative default: 0 retries initially)
- Partition support ready for future scale requirements

### Action Selection Rationale

**Core Actions** (following Xcind security practices):
- `actions/checkout@v4` - Standard, pinned to SHA
- `dtolnay/rust-toolchain@stable` - Rust community standard
- `Swatinem/rust-cache@v2` - Proven performance optimization  
- `taiki-e/install-action@nextest` - Targeted nextest installation

**Security Posture**:
- All actions pinned to specific SHA hashes
- Minimal permission grants (`contents: read`)
- No third-party secret access required

### Performance Optimization

**Cache Strategy**:
- Cargo registry cache (dependencies)
- Target directory cache (build artifacts)  
- Git dependencies cache (common in Rust projects)

**Expected Runtime**:
- Format job: ~1-2 minutes
- Lint job: ~2-3 minutes  
- Test job: ~5-6 minutes (including cache warm-up)
- Total pipeline: ~6-8 minutes (parallel execution)

### Future Extension Readiness

**Modular Workflow Design**:
- Each quality gate as separate job (can be split into separate workflows if needed)
- Matrix strategy ready for additional platforms (Windows, additional Unix variants)
- Conditional job execution ready for feature flags

**Release Pipeline Preparation**:
- Workflow naming convention (`ci.yml` vs `release.yml`)
- Artifact naming patterns ready for binary distribution
- Platform detection patterns ready for cross-compilation

## Open Questions

### Test Suite Reliability in CI

**Current Unknown**: How often does the existing test suite produce false negatives in CI environment vs local development?

**Resolution Approach**: Monitor for 2-3 weeks after implementation. If flaky test rate > 5%, investigate specific failure patterns and add retry logic or test isolation improvements.

### Performance vs Coverage Trade-off

**Current Decision**: Run full test suite (540 tests) on every CI run for maximum coverage.

**Future Decision Point**: If CI runtime exceeds 10 minutes consistently, evaluate splitting into:
- Fast feedback job (unit tests only, ~100 fastest tests)  
- Complete validation job (full integration suite)
- Matrix strategy for different test categories

### Windows Platform Support

**Current Scope**: Unix platforms only (ubuntu-latest, macos-latest).

**Future Decision Trigger**: If Windows support becomes a project goal, add `windows-latest` to test matrix. Current architecture (temp directories, filesystem operations, port binding) should be cross-platform compatible.

### Release Automation Integration

**Current Boundary**: CI pipeline is independent of release process.

**Future Integration Questions**:
- Should successful CI be a prerequisite for release tagging?
- Should release artifacts be built in CI pipeline vs separate release pipeline?  
- What's the handoff mechanism between CI validation and release artifact generation?

**Resolution Approach**: Address when release automation spec is written (separate workstream).

## Revision History

| Version | Date | Changes |
|---------|------|---------|
| 1.0.0 | 2026-04-28 | Initial specification covering basic CI pipeline with GitHub Actions, test matrix for Unix platforms, modular design for future release integration |

---

## Related Documents

- [Vision](../docs/product/vision.md) — solo-local-cli scope and local-everything principles
- [Architecture Overview](../docs/architecture/overview.md) — system structure and quality attributes  
- [Tech Stack](../docs/implementation/tech-stack.md) — existing toolchain (Justfile, cargo-nextest, clippy)
- [Xcind CI Reference](https://github.com/scinddev/xcind/tree/main/.github/workflows) — security practices and workflow patterns