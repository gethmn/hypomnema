# Vault Ignores Specification

**Version**: 1.0.0
**Date**: 2026-05-03
**Status**: Approved

---

## Overview

Hypomnema applies a layered path-filtering strategy to decide which files within a vault are indexed and which are ignored. The filter runs in two places: the initial vault walk at startup (and on `rescan`) and the live watcher event pipeline. Both sites use the same `InclusionFilter` predicate — no duplicated rule logic.

The primary motivation is correctness for Git-managed vaults: without VCS-aware filtering, `node_modules/`, `target/`, `.git/` contents, and other `.gitignore`-listed paths would be silently indexed, producing spurious results and bloating the index.

**Related Documents**:
- [vault-management.md](./vault-management.md) — vault lifecycle, configuration, and control-plane API
- [filesystem-search.md](./filesystem-search.md) — how filtered paths surface in search results
- [ADR-0005: Watcher design](../decisions/0005-notify-debouncer-over-custom-watcher.md)
- Source: `src/watcher/inclusion.rs`, `src/watcher/vcs_ignore.rs`, `src/indexer/walk.rs`

---

## Behavior

### Precedence Chain

`InclusionFilter::includes(rel_path, is_dir)` evaluates four rules in order. **The first matching rule wins.**

| Priority | Rule | Source |
|----------|------|--------|
| 1 | **Always exclude `.git/`** | Hard-coded; unconditional; no config can override. |
| 2 | **Config re-include** (`!`-prefixed patterns in `ignore_patterns`) | Operator override; beats `.gitignore` and config excludes below. |
| 3 | **Config exclude** (non-`!` patterns in `ignore_patterns`) | Operator-specified globs; beats VCS ignore. |
| 4 | **VCS ignore** (`.gitignore` chain) | Enabled when `respect_gitignore = true` (default); skipped when `false`. |
| 5 | **Default: include** | Nothing matched → path is included. |

### Step-by-step evaluation

```
rel_path input
    │
    ▼
1. rel_path == ".git" or starts_with ".git/"?
   YES → exclude (return false)
    │
    ▼
2. config.reinclude.is_match(rel_path)?   (! patterns)
   YES → include (return true)
    │
    ▼
3. config.exclude.is_match(rel_path)?     (positive patterns)
   YES → exclude (return false)
    │
    ▼
4. respect_gitignore AND vcs.is_ignored(rel_path, is_dir)?
   YES → exclude (return false)
    │
    ▼
5. default → include (return true)
```

### Re-include semantics for `!` patterns

A pattern in `ignore_patterns` that begins with `!` is a **re-include override**. It is evaluated at priority 2, before config excludes and before VCS ignore, so it can resurrect paths that `.gitignore` or lower-priority config patterns would suppress.

**Worked example**: vault `.gitignore` contains `.env*`. The operator wants `.env.example` indexed (it's a checked-in template, not a secrets file). They add `!.env.example` to `watcher.ignore_patterns`. Result:

- `.env` → excluded (priority 4: VCS ignore matches; no re-include override)
- `.env.local` → excluded (same)
- `.env.example` → included (priority 2: re-include override matches first)

Re-include also beats config-exclude patterns in the same `ignore_patterns` list, because priority 2 is checked before priority 3.

---

## Configuration

Both fields live under the `[watcher]` section of `config.toml`.

### `respect_gitignore`

```toml
[watcher]
respect_gitignore = true  # default
```

| Value | Behavior |
|-------|----------|
| `true` (default) | `.gitignore` files in the vault root and nested directories are parsed at startup. Paths matching any `.gitignore` rule are excluded (priority 4 in the precedence chain). |
| `false` | VCS ignore step is skipped entirely. Only `.git/` (priority 1) and `ignore_patterns` (priorities 2–3) apply. |

**When to set `false`**: vault is checked into Git for backup purposes and `.gitignore` excludes files the operator wants indexed (e.g., generated docs, compiled outputs that are also reference material). Rather than duplicating the `.gitignore` exclusions as inverse `ignore_patterns`, disable VCS filtering and use `ignore_patterns` alone for explicit exclusions.

### `ignore_patterns`

```toml
[watcher]
ignore_patterns = [
  "**/*.tmp",          # exclude all .tmp.md files
  "scratch/**",        # exclude everything in scratch/
  "!scratch/keep.md",  # re-include one file from scratch/
]
```

Each entry is a glob pattern evaluated against the vault-relative path.

| Prefix | Evaluated at | Effect |
|--------|-------------|--------|
| (none) | Priority 3 | Exclude: paths matching this glob are excluded. |
| `!` | Priority 2 | Re-include: paths matching this glob are force-included, overriding both `.gitignore` and other config-exclude patterns. |

Pattern syntax follows the [`globset`](https://docs.rs/globset) crate:
- `**` matches zero or more path segments
- `*` matches within a single segment
- `?` matches any single character
- `{a,b}` matches either `a` or `b`

**Default value**: an empty list `[]` (no operator-configured excludes).

---

## Defaults

| Setting | Default | Notes |
|---------|---------|-------|
| `respect_gitignore` | `true` | VCS-aware filtering active for all new vaults. |
| `ignore_patterns` | `[]` | No operator-configured excludes. |
| `.git/` exclusion | always | Not configurable; unconditional at priority 1. |

---

## Limitations

### Editing `.gitignore` at runtime

The `.gitignore` chain is parsed once when the vault starts (or when `VcsIgnore::build` is called during `rescan`). Changes to `.gitignore` while the daemon is running **do not take effect** until the operator runs `hmn vault rescan <vault>` or restarts the daemon. Live watcher events for `.gitignore` itself are filtered by `is_relevant_path` (not a `.md` file) and never reach the reindex pipeline.

### Symlink behavior unchanged

The watcher (`notify`) does not follow symlinks. A `.gitignore` line that excludes a directory reachable only through a symlink will exclude both the real path and the symlink target. Conversely, a file that exists only through a symlink inside the vault will be indexed on the initial scan (the walker follows links) but live edits will not produce watcher events. This is a v0 known limitation documented in `src/watcher/mod.rs`.

### `.dockerignore` and other ignore-file formats not supported

Only `.gitignore` files are parsed. `.dockerignore`, `.npmignore`, `.hgignore`, and similar formats are not consulted. Operators who want their exclusions from these files mirrored in the Hypomnema index must duplicate relevant patterns in `watcher.ignore_patterns`.

### Only `.md` files are ever indexed

`is_relevant_path` rejects any path whose extension is not `.md` and any path with a dotfile component (e.g., `.obsidian/workspace.md`, `.git/COMMIT_EDITMSG`). The `InclusionFilter` precedence chain is only reached for `.md` paths that pass this pre-filter. Re-include patterns for non-`.md` files have no effect.

---

## Examples

### Example 1: Default Git-managed vault

**Config** (all defaults):
```toml
[watcher]
# respect_gitignore = true (default)
# ignore_patterns = [] (default)
```

**Vault `.gitignore`**:
```
node_modules/
target/
.env
```

**Result**: `node_modules/`, `target/`, `.env*` paths are excluded. All `.md` files outside those directories (and outside `.git/`) are indexed.

### Example 2: Re-include a specific file from a `.gitignore`-excluded directory

**Config**:
```toml
[watcher]
ignore_patterns = ["!drafts/important.md"]
```

**Vault `.gitignore`**:
```
drafts/
```

**Result**: `drafts/important.md` is indexed (priority 2 re-include wins). All other files in `drafts/` remain excluded (VCS ignore applies; no override).

### Example 3: Disable VCS filtering entirely

**Config**:
```toml
[watcher]
respect_gitignore = false
ignore_patterns = ["**/node_modules/**", "**/target/**"]
```

**Result**: `.gitignore` is not consulted. Only `ignore_patterns` and the unconditional `.git/` exclusion apply.

---

## Edge Cases

### `.git/` is excluded unconditionally

Even if `.gitignore` contains a negation pattern for `.git/`, or if `ignore_patterns` contains `!.git/something`, the hard-coded priority-1 rule fires first. There is no way to index `.git/` contents.

**Rationale**: `.git/` contents mutate on every git operation. Indexing them would produce constant spurious change events and meaningless search results.

### Re-include beats config-exclude in the same list

If `ignore_patterns = ["logs/**", "!logs/important.md"]`, then `logs/important.md` is included (priority 2 re-include) even though `logs/**` (priority 3 exclude) would match it. Evaluation order within the list does not matter; what matters is whether the pattern is a re-include (`!` prefix) or an exclude.

### VCS negation patterns inside `.gitignore` are honoured

The `VcsIgnore` backend (using `ignore::WalkBuilder`) respects negation patterns within `.gitignore` itself (e.g., `*.log\n!important.log`). The net result from `.gitignore` is what reaches priority 4 in the `InclusionFilter`. Config re-include (priority 2) is an additional override layer on top of whatever `.gitignore` decides.

---

## Integration Points

### With Watcher (`src/watcher/`)

`InclusionFilter` is constructed in `spawn_runner_parts` (control plane) with the vault's `VcsIgnore` and the compiled `ignore_patterns`. It is passed to `spawn_watcher` as an `Arc<InclusionFilter>` and used inside the `translate` pipeline on every debounced event batch.

### With Initial Walk (`src/indexer/walk.rs`)

`walk_vault` takes an `&InclusionFilter` and applies `filter.includes(rel, is_dir)` to every entry emitted by `WalkDir`. The same filter instance is used for both the walk and the watcher, ensuring startup and live-event behavior are identical.

### With Configuration (`src/config.rs`)

`WatcherConfig::compiled_ignores_split()` compiles `ignore_patterns` into two `GlobSet`s: `exclude` (positive patterns) and `reinclude` (`!`-prefixed patterns stripped of their leading `!`). Both are stored in `CompiledIgnores` and passed into `InclusionFilter`.

---

## Implementation Notes

- `InclusionFilter` is the single authoritative predicate for path inclusion. Neither the walker nor the watcher evaluates inclusion independently — all calls go through `filter.includes`.
- `.git/` exclusion is in `InclusionFilter::includes`, not delegated to `.gitignore`. This guarantees the exclusion even when `respect_gitignore = false` or when the vault has no `.gitignore`.
- `VcsIgnore::build(vault_path)` walks `.gitignore` files at construction time. No I/O happens on the hot path (`is_ignored` is an in-memory match).
- Changing `respect_gitignore` or `ignore_patterns` at runtime requires a daemon restart or `hmn vault rescan` to take effect, because `InclusionFilter` is constructed once per vault start.

---

## Revision History

| Version | Date | Changes |
|---------|------|---------|
| 1.0.0 | 2026-05-03 | Initial spec — VCS-aware ignores, precedence chain, `!`-negation semantics (Step 21) |
