# Hypomnema

> *"a material memory of things read, heard, or thought"*
> — Foucault, on the hypomnemata of the ancient Greeks

Hypomnema makes a directory of notes searchable and reachable
programmatically — from the command line and from AI agents over MCP —
running locally on your machine. Search by filename, by content, or by
meaning; subscribe to change events while you're connected.

The original v0 gate is complete. Hypomnema now ships the core daemon,
CLI, HTTP API, MCP surfaces, multi-vault lifecycle, live CLI/HTTP change
events, content retrieval, and filesystem/content/semantic search over
Markdown notes.

- **Orientation:** [`AGENTS.md`](./AGENTS.md)
- **Scope and history:** [`docs/hypomnema-handoff.md`](./docs/hypomnema-handoff.md)
- **Skills (pattern guides):** [`.claude/skills/`](./.claude/skills/)

Current boundaries are tracked in
[`docs/product/vision.md`](./docs/product/vision.md). Commonly discussed
features that are **not implemented yet** include structured
frontmatter/tag/backlink metadata, durable/replayable event history,
semantic indexing for non-Markdown formats, MCP `vault_watch`, and the
Unix-socket MCP transport. These are no longer blocked by "v0 scope";
they need normal product/design work before implementation.
