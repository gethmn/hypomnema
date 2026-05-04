This is the release runbook for Hypomnema. The project ships two binaries — `hmn` (the CLI client) and `hmnd` (the daemon) — from a single crate. `cargo-release` drives the cut, with `git-cliff` generating the changelog as a pre-release hook.

## Prerequisites

- `cargo-release` — available via `nix develop`; non-nix install: `cargo install cargo-release`
- `git-cliff` — available via `nix develop`
- Active checkout on `main` with a clean working tree

## Pre-cut checklist

- CI is green on the commit being released
- Working tree is clean (`git status` shows nothing)
- On `main` branch
- Recent commits reviewed; release level (patch / minor / major) determined

## Cut command

```
cargo release <level>
```

Where `<level>` is `patch`, `minor`, or `major`. `cargo-release` runs `contrib/changelog-hook` (which invokes `git cliff --unreleased --tag ...` and prepends to `CHANGELOG.md`), bumps the version in `Cargo.toml` and `Cargo.lock`, commits, and creates a tag. Note that `push = false` means nothing is pushed automatically.

## Push policy

After reviewing the release commit and tag, the maintainer pushes manually:

```
git push --follow-tags
```

`push = false` and `publish = false` in `[package.metadata.release]` are intentional; publishing to crates.io is also manual.

## Versioning notes

Hypomnema follows Semver. Versions are decided at cut time from merged history. Do not pre-assign versions to rounds or steps in the roadmap. `cargo-release` handles `Cargo.toml` + `Cargo.lock` alignment; no other version-bearing files exist.

## Two-binary note

This crate ships two binaries (`hmnd` daemon and `hmn` CLI) from a single crate. A single `cargo release` command covers both. No per-binary release steps.
