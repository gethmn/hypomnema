# Release Process

This file describes how releases are cut for this project. Fill it in once and edit as the process evolves. The orchestration playbook delegates all release/tag/version concerns here so the rest of the workflow can stay tool-agnostic.

**Tool**: cargo-release
**Version scheme**: semver
**Trigger**: per round

## Pre-cut checks

- [ ] CI is green on the commit being released
- [ ] CHANGELOG (or generated equivalent) reviewed

## Cut command

```
cargo release patch
cargo release patch --execute
```

## Push policy

- open PR first then tag from main

## Versioning notes

Versions are decided at cut time from the merged history (commit messages, manual judgment, or whatever the chosen tool consumes). **Do not pre-assign versions to rounds or steps in the roadmap** — a single late-arriving change can flip the resulting version, and tools like `cargo-release` / `git-cliff` infer the version themselves.
