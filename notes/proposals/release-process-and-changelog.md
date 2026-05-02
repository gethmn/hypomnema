# Release Process and Changelog Proposal

**Version**: 0.1.0
**Date**: 2026-05-02
**Status**: Draft

---

## Overview

Hypomnema should not maintain a standalone manual changelog process. A changelog becomes useful when it is generated or finalized as part of a real release process: version bump, release commit, tag, release notes, and artifact publication or draft publication.

Until that release process exists, the project should rely on roadmaps, workplans, retrospectives, tags, and Git history as the project record. Do not require agents to update `CHANGELOG.md` at round boundaries, and do not add CI checks that enforce changelog updates.

The preferred future shape is Xcind-like: use `git-cliff` during a release command to generate the release changelog section from commits since the previous tag, prepend or update `CHANGELOG.md`, commit the version bump plus changelog, tag the release, and hand the generated notes to GitHub Releases.

## Rationale

The manual Keep a Changelog ritual adopted in round 5 sat between two complete states:

- no changelog process, where project history is carried by roadmaps, retros, tags, and commits
- release-owned changelog generation, where release automation makes the changelog current because it is part of shipping

The middle state was too easy to stale. It required manual compare-link maintenance and round-boundary bookkeeping without providing a concrete release artifact. That creates process drag before the project has binary distribution or a publish workflow that can consume the result.

Xcind's `git-cliff` setup works because it is attached to `contrib/release`: the release command bumps versions, updates version-bearing files, generates the changelog for unreleased commits, commits, tags, and tells the maintainer how to publish. The changelog is not a separate ritual; it is a release output.

Hypomnema should copy that coupling, not just the presence of `CHANGELOG.md`.

## Current Policy

- No repo-root `CHANGELOG.md` is maintained.
- No round-boundary changelog step exists in the shipping ritual.
- No changelog CI check is required on pull requests.
- No pre-commit hook, bot comment, or GitHub Action should open or update changelog entries.
- Historical references to the round-5 changelog adoption remain in archived roadmap and retrospective notes as history, not current policy.

## Future Release Process

A future release workstream may introduce:

- `cliff.toml`
- `CHANGELOG.md`, generated or updated by release tooling
- a `just release-*` recipe or `contrib/release` script
- a GitHub release workflow, probably triggered by tags or GitHub release publication
- binary artifact builds, checksums, and any signing or provenance policy the project needs

The release command should own the changelog. A reasonable first version:

```bash
#!/usr/bin/env bash
set -Eeuo pipefail

version="${1:?usage: contrib/release X.Y.Z}"
tag="v${version}"

if ! git diff --quiet; then
  echo "working tree must be clean"
  exit 1
fi

# Update version-bearing files here, for example Cargo.toml and Cargo.lock.
# cargo set-version "$version"

git-cliff --unreleased --tag "$tag" --prepend CHANGELOG.md

git add Cargo.toml Cargo.lock CHANGELOG.md
git commit -m "release: ${tag}"
git tag -a "$tag" -m "$tag"

echo "Run:"
echo "  git push --follow-tags"
echo "  gh release create ${tag} --notes-file <(git-cliff --latest)"
```

The exact implementation can differ. The invariant is that a release command creates the changelog update at the same time as the release commit and tag.

## Gating Criteria

Do not reintroduce `CHANGELOG.md` until a release-process proposal or workplan resolves these points:

- **Release trigger**: tag push, GitHub release publication, manual `just`/script command, or a combination.
- **Version source of truth**: how `Cargo.toml`, `Cargo.lock`, tags, and any binary metadata stay aligned.
- **Artifact story**: whether the release publishes binaries, drafts a GitHub Release, or only tags source.
- **Changelog generation**: whether `git-cliff` is configured to generate only unreleased commits, how it groups commits, and whether commit messages are good enough without Conventional Commits.
- **Release notes handoff**: whether GitHub Releases uses the generated changelog section, generated GitHub notes, or both.
- **Failure behavior**: what happens when generation produces noisy or empty sections.
- **Local and CI split**: what runs locally before tagging, and what runs in GitHub Actions after the tag or release exists.

If these criteria are not resolved, the project should continue without changelog machinery.

## Non-Goals

- Do not add a manual changelog checklist independent of release automation.
- Do not add pull-request changelog enforcement.
- Do not require Conventional Commits solely for changelog generation.
- Do not introduce binary distribution, signing, or provenance policy as part of this proposal; those belong in the eventual release workplan.

## Open Questions

- Should the first release process be source-tag-only, or wait until binary artifacts are ready?
- Should release notes be generated from merge commits, squash commits, or individual commits?
- Should `git-cliff` group maintenance and documentation commits, or omit them unless they are user-visible?
- Should release automation land before `v1`, or only when Hypomnema has external binary users?
