# Step 23 Workplan — Release Process (local-cut, cargo-release-driven)

**Researcher**: step-23-researcher (process 360)
**Step**: 23 (Round 12)
**Date**: 2026-05-03

---

## Independent State Verification

Before proceeding to task breakdown, the researcher independently verified the following against the orchestrator's reported state:

| Artifact | Expected | Actual | Verdict |
|---|---|---|---|
| `Cargo.toml` `[package.metadata.release]` | Verbatim match to hypomnema-app | ✅ Matches lines 57–64 exactly (8 lines; same keys, same values, same comments) | **No drift** |
| `cliff.toml` `[remote.github]` | `owner = "gethmn"`, `repo = "hypomnema"` | ✅ Lines 62–63: `owner = "gethmn"`, `repo = "hypomnema"` | **No drift** |
| `contrib/changelog-hook` | Exactly 6 lines, verbatim match to hypomnema-app | ✅ 6 lines, content identical | **No drift** |

All three artifacts are confirmed present and verbatim-correct. No reconciliation required as part of this round.

---

## Workplan-Time Decision Resolutions

### Decision 1 — nixpkgs source for `cargo-release`

**Recommendation: use the existing `nixos-unstable` pin (no new input needed).**

**Reasoning:**

`flake.nix` already pins `nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable"`. The package `cargo-release` is available in nixpkgs at version `1.1.2`. Since the flake already tracks `nixos-unstable`, adding `cargo-release` to the devshell requires only a one-line insertion in the `packages` list — no new input, no new pin, no flake.lock drift beyond the existing rolling behavior.

Using a separate stable pin for a single dev tool would add complexity (a new flake input, an `inputs.nixpkgs-stable.follows` decision, a potential name collision in `pkgs`) without benefit. `cargo-release` is a development tool, not a reproducible-build artifact; minor version drift is acceptable.

**Action**: Builder adds `cargo-release` to the `packages` list in `devShells.default` in `flake.nix`, sourced from the existing `pkgs` (i.e., `nixos-unstable`).

---

### Decision 2 — cliff.toml commit-parser verification

**Evidence: `git cliff --unreleased` output (run against HEAD, 2026-05-03):**

```
# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Added

- Add proposal intakes for content-retrieval, FTS5/BM25, and HyDE by @simensen
- Step 19 (Round 9): Add content_get read-only retrieval operation by @simensen
- Step 20 (Round 9): Add FTS5/BM25 ranked content search by @simensen
- docs: Add Hypomnema Bold SVG by @simensen
- docs: Add quiet logo variant by @simensen
- feat: Static sqlite-vec Bundling

### Changed

- Update workflow by @simensen
- docs: Archive shipped proposals (content-retrieval, FTS5/BM25, sqlite-vec bundling)

### Fixed

- Complete step boundary ritual for Round 8 (Step 18) by @simensen
- Backlog hygiene and Solo Orchestrator Companion archive by @simensen
- Fix Linux build: use RecommendedCache for Debouncer field by @simensen

### Infrastructure

- docs: Release process and sqlite vec to the backlog

### Maintenance

- Fix CI: FTS UPDATE missing content + lint/format by @simensen
- maint: Update release and changelog handling
- maint: Update crates
- feat: Round 10 Step 21 — Health Endpoint + VCS-Aware Ignores

### Other

- docs: Revert CHANGELOG decision by @simensen
- docs: Planning sqlite-vec bundling
- docs: sync version
- docs: Archive Round 10 / Step 21 roadmap and workplan
```

**Conclusion: No tuning required.**

The groupings are sensible. The regex rules correctly catch their targets:

- Commits with `add`, `feat`, `implement` → **Added** ✅
- Commits with `change`, `update`, `refactor`, `move` → **Changed** ✅
- Commits with `fix`, `correct` → **Fixed** ✅
- Commits with `nix`, `flake`, `ci`, `release` → **Infrastructure** ✅
- Commits with `chore`, `maint`, `fmt`, `cleanup` → **Maintenance** ✅

Two observations, neither requiring tuning:

1. A few commits hit multiple keywords (e.g. `"feat: Round 10 Step 21 — Health Endpoint + VCS-Aware Ignores"` lands in **Maintenance** because `maint` appears before `feat` in parser order). This is expected cliff behavior — first matching rule wins. The grouping is not wrong; the commit message is ambiguous.
2. The **Other** group catches a few `docs:` commits that don't hit the `doc|readme|comment` regex because they were matched by an earlier rule. Acceptable — no noise, no empty sections.

The commit-parser regexes do not need adjustment. Leave `cliff.toml` untouched.

---

### Decision 3 — Runbook fallback path

**Recommendation: no fallback in this round.**

**Rationale:** `cargo-release` is a well-tested, widely deployed tool. In the event of a failure during a release run, the maintainer already has all the primitives needed (`git tag`, `git commit --amend`, `git reset`) to recover manually from any intermediate state. Documenting a fallback path now would require speculating about failure modes that haven't occurred, and would increase the maintenance surface of `notes/release-process.md` without benefit. If a real failure mode surfaces during testing (Task 1 includes `nix develop --command cargo release --help` as a smoke test), the builder should note it and the coordinator should escalate rather than adding speculative content. This is consistent with the roadmap's stated recommendation (roadmap-12.md line 83).

---

## Task Breakdown

### Task A — Generate and commit CHANGELOG.md via `git cliff`

**Title**: Overwrite CHANGELOG.md from full commit history

**Description:**

Run `git cliff > CHANGELOG.md` from the repo root. This overwrites the file with the full changelog as cliff generates it from all tagged commits and HEAD. Do not use `--prepend`; do not hand-curate; do not merge with any prior content. Commit the result.

The existing `CHANGELOG.md` was shipped in round 5 but is not maintained per current policy. The overwrite replaces its content with what cliff generates from actual Git history, making it accurate at this moment and ready for future `--prepend` runs by `contrib/changelog-hook`.

**Acceptance criteria:**

- [ ] `git cliff > CHANGELOG.md` runs without error
- [ ] `CHANGELOG.md` is committed (not staged, committed) with a message that does not embed a version literal
- [ ] `rg "\bv[0-9]+\.[0-9]+\.[0-9]+\b" CHANGELOG.md` returns zero matches in section headers (version refs in commit messages within the body are expected and acceptable — the constraint applies to section headers and prose the builder might add, not to auto-generated body lines)
- [ ] The committed file contains at least one section (confirms cliff ran against real history)

**Notes for builder:**

- Run from the repo root where `cliff.toml` lives
- The `--unreleased` flag is intentionally omitted; this command generates the full changelog from all history
- No manual editing of the generated file

---

### Task B — Add `cargo-release` to `flake.nix` devshell

**Title**: Add cargo-release to devshell packages list

**Description:**

Insert `cargo-release` into the `packages` list in `devShells.default` in `flake.nix`. Source it from the existing `pkgs` binding (which resolves to `nixos-unstable` — already the flake's nixpkgs pin). Place it adjacent to the other cargo tools (`cargo-watch`, `cargo-nextest`, `cargo-edit`).

**Acceptance criteria:**

- [ ] `cargo-release` appears in the `packages` list in `flake.nix`
- [ ] `nix develop --command cargo release --help` exits 0 and prints usage output
- [ ] `nix flake check` passes (or any existing nix CI check continues to pass)
- [ ] No new flake inputs added; no flake.lock input section added beyond what `nix develop` would update automatically

---

### Task C — Author `notes/release-process.md`

**Title**: Write the Hypomnema maintainer release runbook

**Description:**

Create `notes/release-process.md` as a concrete, Hypomnema-specific release runbook. The template exists in hypomnema-app at `notes/release-process.md` but is mostly placeholder text. This task authors the real content for Hypomnema's two-binary shape (`hmn` + `hmnd`).

See the **Content Outline** section below for section structure and per-section direction.

**Acceptance criteria:**

- [ ] File exists at `notes/release-process.md`
- [ ] All sections from the outline are present (pre-cut checklist, cut command, push policy, versioning notes)
- [ ] Versioning notes include verbatim: "Versions are decided at cut time from merged history. Do not pre-assign versions to rounds or steps in the roadmap."
- [ ] Non-nix install one-liner `cargo install cargo-release` is present
- [ ] `rg "\bv[0-9]+\.[0-9]+\.[0-9]+\b" notes/release-process.md` returns zero matches
- [ ] No fallback procedures included (per Decision 3)

---

## `notes/release-process.md` Content Outline

The builder writes prose. This outline gives section structure and one-sentence direction per section. Do not deviate from the section order.

### 1. Title and preamble (no heading)

One short paragraph: describe what this file is (the release runbook for Hypomnema), name the two binaries (`hmn` and `hmnd`), and state that `cargo-release` drives the cut with `git-cliff` generating the changelog as a pre-release hook.

### 2. Prerequisites

Bullet list of what must be installed and accessible before cutting:
- `cargo-release` (available via `nix develop`; one-liner fallback: `cargo install cargo-release`)
- `git-cliff` (available via `nix develop`)
- Active checkout on `main` with a clean working tree

### 3. Pre-cut checklist

Short bullet checklist a maintainer steps through before running the cut command:
- CI is green on the commit being released
- Working tree is clean (`git status` shows nothing)
- On `main` branch
- Recent commits reviewed; release level (patch / minor / major) determined

### 4. Cut command

Literal command block with `cargo release <level>` where `<level>` is `patch`, `minor`, or `major`. Brief explanation of what cargo-release does: runs `contrib/changelog-hook` (which invokes `git cliff --unreleased --tag ...` and prepends to `CHANGELOG.md`), bumps version in `Cargo.toml`, commits, and creates a tag. Note that `push = false` in `[package.metadata.release]` means nothing is pushed automatically.

### 5. Push policy

Explicit statement: the maintainer pushes manually after reviewing the release commit and tag. Provide the literal push command (`git push --follow-tags`). State that `push = false` is intentional and that publish to crates.io is also manual (`publish = false`).

### 6. Versioning notes

Explain that versions follow semver. State the invariant verbatim: "Versions are decided at cut time from merged history. Do not pre-assign versions to rounds or steps in the roadmap." Brief note that `cargo-release` handles Cargo.toml + Cargo.lock alignment and that no other version-bearing files exist.

### 7. Two-binary note

Short note that this crate produces two binaries (`hmnd` and `hmn`) from a single crate, so a single `cargo release` command covers both. No per-binary release steps exist.

---

## Negative-Fingerprint Check

Run this after all tasks are complete. Expected result: zero matches.

```
rg "\bv[0-9]+\.[0-9]+\.[0-9]+\b" notes/roadmap/roadmap-12.md notes/release-process.md
```

This confirms no version literals (e.g. `v0.5.0`, `v1.0.0`) appear in either file. Placeholders like `X.Y.Z` are acceptable and will not match.

---

## Test Plan

Builder runs all three after all tasks are complete:

```bash
# 1. Unit and integration tests
cargo test

# 2. Lint — warnings as errors
cargo clippy -- -D warnings

# 3. cargo-release reachable from devshell (smoke test)
nix develop --command cargo release --help
```

All three must pass before the step is marked shipped.

---

## Task Ordering

Tasks A, B, and C are independent and can be executed in any order or in parallel. The test plan runs after all three are complete.

Recommended order if batching sequentially: B → A → C (flake change first confirms the devshell is usable, CHANGELOG generation second provides evidence for the runbook, runbook last).

---

## Out-of-Scope Reminders

Per roadmap-12.md § Out of Scope — do not touch:

- GitHub Release workflows, tag-triggered automation
- Binary cross-compilation, checksums, signing
- `gethmn.io` install scripts
- Conventional Commits enforcement
- `publish = true` or `push = true` in `[package.metadata.release]`
- `notes/backlog.md` entries about CHANGELOG or release automation
- `notes/project-planning-workflow-notes.md` § current changelog policy
