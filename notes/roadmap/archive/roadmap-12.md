# Round 12 — Release Process (local-cut, cargo-release-driven)

**Status**: Planning phase  
**Date**: 2026-05-03  
**Steps**: 23  
**Scope**: Single-step round dedicated to wiring up a local-cut release process using `cargo-release` with `git-cliff`-driven changelog generation.

---

## Overview

Round 12 completes the release-process wiring described in `notes/proposals/release-process-and-changelog.md`. The core release artifacts (`Cargo.toml` `[package.metadata.release]` block, `cliff.toml`, `contrib/changelog-hook`, `CHANGELOG.md`) reached file-presence in round 5 but were never wired into a working release flow. Round 12 completes that wiring without re-authoring the artifacts.

Human-resolved decisions:
- Pure local-cut; no CI automation
- `cargo-release` as the cut tool
- Changelog generation as a pre-release hook, not a separate ritual
- All push/publish decisions left to the maintainer

---

## Step 23 — Release Process Implementation

### Objective

Establish a maintainer-facing release workflow that:
1. Bumps versions deterministically via `cargo-release`
2. Generates changelog entries from recent commits via `git-cliff` pre-release hook
3. Creates a dated release commit and tag (not signed — `sign-commit=false`, `sign-tag=false`)
4. Provides clear next-steps guidance to the maintainer (push, announce, etc.)
5. Leaves all push/publish decisions to the maintainer (no auto-push)

### Shipping Criteria

All items below must be complete and passing before the step is marked shipped:

- [ ] **CHANGELOG.md overwritten via `git cliff`**
  - Run `git cliff > CHANGELOG.md` (overwrite, not `--prepend`)
  - Whatever the command emits is committed — no hand-curation, no merging of round-5 content
  - Committed as part of the step

- [ ] **cliff.toml commit-parser verified**
  - Run `git cliff --unreleased` and inspect output for sensible grouping
  - If keyword groupings produce noise, tune `commit_parsers` regexes; otherwise leave untouched
  - No Conventional Commits requirement; keyword-driven grouping is sufficient

- [ ] **cargo-release added to `flake.nix` devshell**
  - Researcher decides: nixpkgs-stable pin vs. unstable
  - `nix develop` provides `cargo-release` after this change

- [ ] **`notes/release-process.md` authored**
  - Hypomnema-specific runbook (two binaries: `hmn` CLI + `hmnd` daemon)
  - Sections: pre-cut checklist, cut command (`cargo release <level>`), push policy (manual — maintainer pushes after cargo-release commits and tags), versioning notes
  - Versioning notes must include: "Versions are decided at cut time from merged history. Do not pre-assign versions to rounds or steps in the roadmap."
  - Includes one-liner install instruction for non-nix users: `cargo install cargo-release`
  - No version numbers appear anywhere in the runbook

- [ ] **No version-specific assumptions in any doc produced this round**
  - Negative fingerprint: `rg "\bv[0-9]+\.[0-9]+\.[0-9]+\b" notes/roadmap/roadmap-12.md notes/release-process.md` returns zero matches (placeholders like `X.Y.Z` are fine)

- [ ] **Build verification**
  - `cargo test` passes
  - `cargo clippy -- -D warnings` clean

---

## Out of Scope

- GitHub Release workflows, tag-triggered automation, release.yml, release-bin.yml
- Binary cross-compilation, cargo-dist, checksums, signing, provenance
- `gethmn.io` install scripts, package-manager packaging
- Conventional Commits enforcement on PRs or commits
- Flipping `publish` or `push` to `true` in `[package.metadata.release]`
- Edits to `notes/backlog.md` entries about CHANGELOG or Release automation
- Edits to `notes/project-planning-workflow-notes.md` § current changelog policy

---

## Workplan-Time Decisions (for Researcher)

1. **nixpkgs source for `cargo-release`** — stable pin or unstable?
2. **cliff.toml commit-parser tuning** — verify via `git cliff --unreleased`; adjust only if output is noisy.
3. **Runbook fallback path** — should `notes/release-process.md` document a manual fallback if `cargo-release` itself fails? Recommend: no fallback in this round; add only if a real failure mode surfaces during testing.

---

## Related References

- **Proposal (policy)**: `notes/proposals/release-process-and-changelog.md`
- **Proposal intake**: `notes/proposals/intake-release-process-and-changelog.md`
- **Reference project (sibling)**: `~/Code/hypomnema-app/` — Cargo.toml lines 18–26, cliff.toml, contrib/changelog-hook, notes/release-process.md

---

## Build Strategy (post-approval)

**Phase 1 — Workplan production** (researcher-driven)
- Spawn `step-23-researcher` (medium tier)
- Researcher produces `notes/roadmap/step-23-workplan.md` with task breakdown and deferred-decision resolutions
- Surface workplan to human for go/no-go

**Phase 2 — Build orchestration** (coordinator-driven, if approved)
- No production-code surface in this round — only docs, config files (already present), and a flake change
- Builder task count: 2–3 small tasks, not many
- Create step-23-context scratchpad, spawn builders, verify shipping criteria at step boundary

---

## Notes

- The round-5 artifacts (`CHANGELOG.md`, `cliff.toml`, `contrib/changelog-hook`, `[package.metadata.release]`) are confirmed-present and verbatim-correct. Round 12 does not re-author them; it operationalizes the flow.
- Versioning deferred to `cargo-release` runtime; no pre-assigned versions appear in any docs produced this round.
- A maintainer can run `cargo release` manually once this round ships. GitHub Release workflow and binary distribution belong to a future round.
