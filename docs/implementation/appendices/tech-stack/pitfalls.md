# Pitfalls Catalog

> **Parent**: [Tech Stack](../../tech-stack.md)

---

These hazards were predicted during design before any code was written. Each has a corresponding skill in `.claude/skills/` or a section in `AGENTS.md`. An agent writing Hypomnema code should know all of them before touching the relevant subsystem.

## 1. Blocking the async runtime with rusqlite

**What goes wrong**: A blocking SQL call inside an async context holds a tokio worker thread. Enough of these and the runtime deadlocks — search requests hang with no error, the watcher stops draining events, the process looks alive but does nothing.

**Mitigation**: Every SQL call goes inside `tokio::task::spawn_blocking`. No exceptions.

**Captured in**: `.claude/skills/rusqlite-in-async/SKILL.md`

---

## 2. Watcher event storms during editor saves and sync operations

**What goes wrong**: Editors (Obsidian, VS Code, vim) and sync tools (Syncthing, Dropbox) write files in bursts — temp file, rename, chmod, a dozen notify events for a single logical save. Naively indexing on every event means a single save triggers dozens of reindexes and outbox entries.

**Mitigation**: Use `notify-debouncer-full` to coalesce bursts. Never roll your own debouncer — the edge cases are subtle and well-handled by the crate.

**Captured in**: `.claude/skills/filesystem-watching/SKILL.md`

---

## 3. Spurious re-indexing from mtime-only change detection

**What goes wrong**: mtime changes whenever anything touches the file, including sync tools re-writing identical content. Using mtime alone as the change signal means constant false-positive reindexes.

**Mitigation**: Hash the content. Compare against the last known hash. Emit a change event and reindex *only* on actual hash change.

**Captured in**: `.claude/skills/filesystem-watching/SKILL.md`

---

## 4. Sync-conflict files from Syncthing / Obsidian Sync / Dropbox

**What goes wrong**: Sync tools create `.sync-conflict-*`, `conflicted copy`, and similar files when they can't auto-resolve. These are not user content — they are sync failure detritus. Indexing them pollutes search results and can trigger feedback loops.

**Mitigation**: Filter sync-conflict filenames at the watcher. Never index them. Surface counts in a health view so users notice accumulating conflicts.

**Captured in**: `.claude/skills/filesystem-watching/SKILL.md`

---

## 5. Putting state in the watched directory

**What goes wrong**: Constantly-growing files (the outbox) and frequently-mutated binaries (the SQLite index) inside a synced directory produce pathological sync behavior — conflicts, wasted bandwidth, spurious change notifications fanning out across devices, and in bad cases corruption.

**Mitigation**: All daemon state (index, outbox, logs, config) lives in the daemon's data directory. Nothing mutable is written under the watched path.

**Captured in**: `AGENTS.md`, and codified in [ADR-0006](../../../decisions/0006-outbox-outside-watched-directory.md)

---

## 6. Model-dimension mismatches

**What goes wrong**: Config says one dimension, schema says another. Queries silently return nonsense vectors, or (if the store is strict) fail cryptically deep in sqlite-vec.

**Mitigation**: Bake the dimension in at schema creation time. Fail loudly at startup if config disagrees — don't let the daemon come up at all.

**Captured in**: `.claude/skills/sqlite-vec-extension/SKILL.md`

---

## 7. In-place vector updates

**What goes wrong**: sqlite-vec's vec0 virtual table does not update rows gracefully. Attempting an in-place update produces inconsistent state.

**Mitigation**: On any chunk change, delete all rows for the affected file in the vec table and reinsert the new set. Treat updates as delete-then-insert, always.

**Captured in**: `.claude/skills/sqlite-vec-extension/SKILL.md`

---

## 8. Regex-based or blank-line-based chunking

**What goes wrong**: Regex boundary heuristics miss headings inside code blocks, within list items, or after frontmatter. Blank-line boundaries split mid-paragraph on Markdown hard-wrapped prose. Semantic search quality collapses when chunks don't respect document structure.

**Mitigation**: Use pulldown-cmark's event stream. Heading events mark chunk boundaries; code blocks stay whole; frontmatter is a separate parse phase.

**Captured in**: `.claude/skills/markdown-chunking/SKILL.md`

---

## Meta: why these are named before any code is written

Each of the above is a hazard that was identified during design — before implementation. Naming them this way serves two purposes:

1. **They inform the stack.** The choice of `notify-debouncer-full` (not raw `notify`), sqlite-vec (not an external vector DB), `spawn_blocking` (not direct async SQL), and pulldown-cmark (not regex) are each responses to one of the above.
2. **They live in skills.** The [Claude Code skills](../../../../AGENTS.md) under `.claude/skills/` exist specifically so that an agent editing a file that touches one of these subsystems gets the relevant guidance loaded without having to re-derive it.

When a new hazard appears in practice, the workflow is: fix the specific bug → promote the lesson to a skill → add an entry here.
