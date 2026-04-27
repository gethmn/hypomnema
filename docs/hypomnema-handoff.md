# Hypomnema: Project Handoff

> *"The hypomnemata constituted a material memory of things read, heard, or thought, thus offering these as an accumulated treasure for rereading and later meditation."*
> — Michel Foucault, on the ancient Greek notebook tradition

> **Purpose:** Orientation for picking up Hypomnema in a fresh chat or a new project context. Captures what was decided, why, and what’s deliberately deferred. Not a tutorial; not a spec. The “what we know going in” document.
>
> **Origin:** Spun out from the Iris Vault Bridge design exploration. The full path of that exploration lives in the iris-vault-bridge-* document series; this doc compresses the parts that survived into Hypomnema’s scope.

## What Hypomnema is

A local daemon that watches a directory of Markdown files and indexes them so that any consumer — most commonly an AI agent connected via MCP — can search the contents three ways:

- **Filesystem search** — what files exist, what’s in this directory, glob patterns
- **Content search** — grep-shaped: which files contain this exact string
- **Semantic search** — vector similarity over chunked content

It also emits change events to a durable local log, so consumers can subscribe to “this file changed” notifications without polling.

The watched directory is treated as the user’s. Hypomnema reads from it. Hypomnema does not write to it. State that Hypomnema maintains (the index, the event log, configuration, logs) lives in the daemon’s own data directory, never under the watched path.

## On the name

*Hypomnema* (Greek ὑπόμνημα, plural *hypomnemata*) is the ancient term for a personal notebook of gathered material — quotations, observations, reflections assembled from the outside world for later rereading. Marcus Aurelius’s *Meditations* is the canonical example. Foucault revived the term in his late work to describe the practice of constituting oneself through accumulated external material — tools one returns to, not a diary of internal feelings.

The fit to this project is near-literal: a hypomnema is a reachable substrate of things read, heard, or thought. That is exactly what this daemon builds from the user’s vault.

During early design the project was carried under the working name `mdkb` (Markdown Knowledge Base). Earlier notes and chat histories may still reference that name.

The CLI binary is `hmn`. The crate is `hypomnema`. Pronunciation: roughly *hi-POM-nih-muh* in English, *hoo-POM-nay-mah* in Greek.

## What Hypomnema is not

It is not Iris’s vault bridge. The original framing — “Iris projects its truths to a Markdown vault and reconciles user edits back into the database” — turned out to be a different and much harder problem (bidirectional sync, conflict resolution, last-known-synced tracking, ownership models). That problem belongs to a CRDT-based system (Hexist, in this author’s case), not to a daemon over a filesystem.

Hypomnema is the smaller, generic thing that fell out of asking “what would still be useful even without the bidirectional half?” The answer turned out to be: a lot. Probably enough to live as its own project with its own users.

It is not Obsidian-specific. Obsidian is the vault format that motivated it (and where the prototype consumer lives), but the design assumes nothing about Obsidian. Any directory of Markdown files works. Frontmatter is read but not interpreted. Wikilinks aren’t parsed in v0. Tags aren’t indexed specially.

## How we got here

Five conceptual moves, each one narrowing the scope:

**The full vault bridge** was sketched as a bidirectional sync between Iris’s database and an Obsidian vault — Iris’s truths projected as Markdown files, user edits to those files reconciled back into the database. The sibling docs (`iris-vault-bridge-*.md`) explored the hard parts: ownership boundaries, conflict resolution, atomic writes, change detection, decoupling from Iris.

**The off-the-shelf experiment** showed that wiring up existing tools (obsidian-local-rest-api + an MCP wrapper + an agent framework) failed in recognizable, predictable ways — exactly the failure modes the sibling docs predicted. The diagnostic value was high; the result was unusable.

**The PKB landscape survey** revealed that the Markdown-files-on-disk storage model is largely an Obsidian-and-friends thing. The dominant trend in new PKB tools is CRDT-based (AFFiNE, Anytype, Logseq’s transition). Markdown remains the lingua franca for *export*, even when it’s not the storage format. This made the bidirectional bridge feel even more like Obsidian-specific work that wouldn’t generalize.

**The scope-down to read-only** asked: what does the bridge become if Iris doesn’t try to write back? The answer turned out to be cleaner than the full version — no conflict resolution, no manifest mapping, no atomic-write dance, no three-way merge. Just indexing and search.

**The bridge-as-service refinement** moved the indexing into the bridge itself rather than relying on Iris to chunk and embed via its own ContextRetriever. This made the bridge useful to consumers other than Iris (Claude Code, skills.sh packages, ad-hoc CLI use, possibly other Iris instances). It also made the bridge a long-running service rather than a library, which had real architectural implications.

Hypomnema is the artifact at the end of those five moves. The sibling docs that preceded it remain useful as historical context — they’re the design space that Hypomnema chose a small corner of.

## Scope (v0)

The minimum useful daemon:

- Watch one configured directory recursively for Markdown file changes
- Maintain three indexes: filesystem (paths, sizes, mtimes), content (the file text), semantic (chunked, embedded, in sqlite-vec)
- Expose search over HTTP and MCP
- Emit change events to an append-only JSONL outbox in the daemon’s data directory
- Run as a local daemon with a CLI for management

That’s the whole v0. Single directory, single user, single machine, read-only.

## Out of scope (deferred)

Canonical "real, planned, but not v0" list lives in [`product/vision.md` → Non-Goals](product/vision.md#non-goals). The round-agnostic backlog of work that *could* land in some future round (multi-vault, agent-host integration, public-presence work, process/playbook edits, operational follow-ups) lives in [`notes/backlog.md`](../notes/backlog.md) — pulled into roadmaps as rounds are written. Implementation-shaped deferrals specific to this handoff (no workspace split, no `thiserror`, no abstraction layers) are documented below under [Crate stack](#crate-stack).

## Load-bearing decisions

These are the choices that shaped everything else. Each was a real call between alternatives.

### Rust over Python

The honest near-tie: Python gets to v0 faster, with more mature watcher and embedding library ecosystems for prototyping. Rust matches the project shape better — long-running daemon, file I/O under sync pressure, deployable as a single binary, type system catches the bugs that haunt long-running services.

The tipping factors were the office deployment scenario (single binary beats Python install management) and the project’s expected lifespan (Rust’s upfront cost amortizes over years; the porting cost from Python wouldn’t realistically be paid). The MCP Rust SDK (`rmcp`, official) is mature enough to remove the main practical objection.

### Indexing in the bridge, not in the consumer

The earlier scaled-back version had Iris doing its own chunking and embedding via ContextRetriever, with the bridge as a thin filesystem wrapper. Moving the indexing into the bridge:

- Makes the bridge useful to consumers beyond Iris without each one rebuilding the same retrieval pipeline
- Justifies the daemon shape (long-running service with persistent state) instead of a library
- Lets the bridge offer all three search modes (filesystem, content, semantic) as peers, where the third one is the hard one

Cost: the bridge becomes opinionated about embedding models, chunking, and vector stores. It’s no longer a neutral format toolkit. That’s an honest trade — what it loses in adapter-shaped purity it gains in being immediately useful.

### Three search modes as peers

Filesystem, content, and semantic search answer different question shapes. Agents naturally use them in sequence (“do I have notes on X?” → “show me what’s in `notes/databases/`” → “which one mentions pgvector?”). An MCP server offering only one of these is incomplete in a way that’s immediately obvious watching an agent try to work.

### Local everything

Local embedding model (nomic-embed-text-v1.5 via TEI or vLLM sidecar, configurable to anything OpenAI-API-shaped). Local vector store (sqlite-vec, one file on disk). Local outbox (JSONL in the daemon’s data directory). No cloud dependencies, no required external services. This is non-negotiable for the office case where data leaving the box may be a hard restriction, and it’s also just operationally simpler.

### Outbox lives outside the watched directory

The daemon’s append-only event log lives in `~/.local/share/hypomnema/` (or platform equivalent), never inside the watched vault. A constantly-growing file inside a Syncthing or Dropbox directory creates pathological sync behavior. The principle generalizes: any state that mutates frequently or is device-specific stays out of the synced path.

### sqlite-vec over Lance, qdrant, etc.

One file on disk. No separate process. No network port. Loaded as a SQLite extension into the daemon’s existing connection pool. The daemon ships as the binary plus the extension `.so`/`.dylib`/`.dll` — two files, not a service architecture.

The dimension of the vector column is baked into the schema (768 for nomic-embed-text-v1.5). Model-switching isn’t a v0 concern; if it ever becomes real, it’s a re-index, not a runtime switch.

## The v0 step plan

Eight steps, dependency-ordered, each independently useful as a stopping point:

1. **Skeleton.** Daemon starts, reads config, logs what it’s watching, exits cleanly on SIGINT.
2. **Scan + hash.** Walk the directory, compute content hashes, store in SQLite. Re-runs are deterministic.
3. **Watcher.** `notify` + `notify-debouncer-full`, filtered to `.md` files, with the content-hash check that distinguishes “OS noticed a write” from “content changed.”
4. **Outbox.** Persist real change events to JSONL in the daemon’s data directory.
5. **Filesystem and content search over HTTP.** List/glob and grep, exposed via Axum. CLI built against these. Useful enough to dogfood for ordinary find/grep work.
6. **Chunking and embedding.** pulldown-cmark heading-aware chunking, embed via TEI, store in sqlite-vec. The step most likely to surprise you.
7. **Semantic search.** Query → embed → vector search → return chunks with metadata.
8. **MCP wrapper.** Same operations, MCP transport via `rmcp`. Test against an actual agent (Claude Code, Iris).

If a step is hard, ship the previous one and keep using it. Step 5 is genuinely valuable on its own.

## Crate stack

- `tokio` — async runtime
- `axum` — HTTP server
- `rmcp` — MCP protocol (official Rust SDK)
- `rusqlite` (`bundled` + `load_extension`) — SQLite access
- `r2d2` + `r2d2_sqlite` — blocking connection pool
- `notify` + `notify-debouncer-full` — filesystem watching
- `pulldown-cmark` — Markdown parsing
- `reqwest` — HTTP client for the embedding service
- `serde`, `serde_json`, `toml` — serialization
- `tracing`, `tracing-subscriber` — logging
- `anyhow` — error handling
- `clap` (`derive`) — CLI

No `thiserror` until there’s a public library API. No async-trait abstractions until there’s a second concrete implementation demanding one. No workspace split until a second consumer demands it.

## Repository orientation

- `AGENTS.md` — always-loaded orientation for any agent opening the repo. Read first.
- `.claude/skills/rusqlite-in-async/` — the spawn_blocking discipline. The single highest-value skill.
- `.claude/skills/sqlite-vec-extension/` — extension loading and vector table patterns.
- `.claude/skills/filesystem-watching/` — notify + debouncer + sync-tool gotchas.
- `.claude/skills/markdown-chunking/` — heading-aware chunk boundaries.

The skills are written to be loaded as Claude Code skills automatically when relevant. Their content is also useful as plain reading for human collaborators.

## Pitfalls already named

These are gotchas the conversation predicted before any code was written. Each has a corresponding skill or AGENTS.md section:

- **Blocking the async runtime with rusqlite.** Every SQL call inside `spawn_blocking`, no exceptions. (`rusqlite-in-async` skill)
- **Watcher event storms during editor saves and sync operations.** Use the debouncer, never roll your own. (`filesystem-watching` skill)
- **Spurious re-indexing from mtime-only change detection.** Hash the content; compare against the last known hash; emit only on actual change. (`filesystem-watching` skill)
- **Sync-conflict files from Syncthing/Obsidian Sync/Dropbox.** Filter at the watcher; never index. Surface counts in a health view. (`filesystem-watching` skill)
- **Putting state in the watched directory.** Outbox, index, logs all live in the daemon’s data directory. Synced directories mangle anything that mutates frequently. (AGENTS.md)
- **Model-dimension mismatches.** Bake the dimension in at schema-creation time; fail loudly at startup if config disagrees. (`sqlite-vec-extension` skill)
- **In-place vector updates.** Always delete-and-reinsert. Vec0 doesn’t update gracefully. (`sqlite-vec-extension` skill)
- **Regex-based or blank-line-based chunking.** Use pulldown-cmark events. (`markdown-chunking` skill)

## Relationship to Iris

Hypomnema has no awareness of Iris. Iris is one consumer of Hypomnema among potentially several.

The Iris-side integration is thin: an adapter that calls Hypomnema’s search endpoints (via MCP or HTTP) as agent tools, and tails Hypomnema’s outbox to invalidate any vault-derived state Iris caches. No forward path from Iris’s model changes to vault writes — there’s no write path on the Hypomnema side to feed.

When Hexist arrives and provides proper bidirectional sync via CRDTs, Hypomnema’s search capability is still useful. What changes is how Iris’s own state gets managed, which is Hexist’s domain, not Hypomnema’s.

The deferred work — ownership model, conflict resolution, format spec, atomic writes — is preserved in the original `iris-vault-bridge-*.md` documents in the Iris project. Those docs are not lost; they describe a problem Hypomnema chose not to solve, and they remain accurate as design groundwork for whatever does eventually solve it.

## Open questions for early implementation

Canonical list lives in [`product/vision.md` → Open Questions](product/vision.md#open-questions).

## Done when

Canonical list lives in [`product/vision.md` → Success Criteria](product/vision.md#success-criteria).

## Reference material

In the Iris project (historical context, design groundwork):

- `iris-vault-bridge-handoff.md` — original full-bridge scope
- `iris-vault-bridge-ownership-model.md` — `vault_root` / `vault_path` (deferred)
- `iris-vault-bridge-conflict-resolution.md` — three-way merge (deferred)
- `iris-vault-bridge-atomic-writes.md` — write safety (deferred)
- `iris-vault-bridge-change-detection.md` — watcher patterns (informs v0)
- `iris-vault-bridge-manifest-placement.md` — state-outside-the-vault rule (informs v0)
- `iris-vault-bridge-decoupling.md` — toolkit/spec/adapter framing (informs how Hypomnema stays Iris-agnostic)
- `iris-vault-bridge-off-the-shelf-experiment.md` — concrete failure modes Hypomnema avoids

In the Hypomnema project (operational):

- `AGENTS.md` — always-loaded orientation
- `.claude/skills/*/SKILL.md` — pattern-specific guidance loaded on demand
