# ADR-0004: Three Search Modes as Peers

**Status**: accepted
**Date**: 2026-04-23
**Decision-Makers**: Beau Simensen

---

## Context

When an agent works with a notes vault, it naturally uses search in a sequence that spans three distinct shapes:

1. **Filesystem-shaped**: "Do I have notes on X?" → list/glob/path questions
2. **Content-shaped**: "Which files mention the exact phrase Y?" → grep
3. **Semantic-shaped**: "What in this vault is conceptually similar to Z?" → vector similarity

Each shape answers a different kind of question. An agent trying to work an actual task typically moves between them: start with a concept ("notes on databases"), narrow by path ("in `notes/databases/`"), confirm by content ("which one mentions pgvector?"). Missing any one of the three produces an immediately obvious gap — the agent either has to guess (if filesystem is missing), can't find conceptually-similar content it doesn't know the exact phrase for (if semantic is missing), or can't verify an exact reference (if content is missing).

Off-the-shelf MCP servers for Markdown typically offer only content search (grep) or only semantic search (rag). The experience of watching an agent work a task in these incomplete environments was the proximate cause for building Hypomnema rather than continuing to try to compose existing tools.

## Decision

Hypomnema exposes all three search modes as peers of its API surface. Neither HTTP nor MCP prioritizes one mode over the others. All three are discoverable by agents as equivalent-shaped tools.

Each mode has its own operation:
- `search_filesystem` — list/glob/stat
- `search_content` — grep-shaped substring/regex
- `search_semantic` — embed query, vector search, return chunks with metadata

## Consequences

### Positive

- Agents can compose searches naturally — start broad, narrow, verify — without having to work around the absence of a shape
- Each operation stays small and well-defined; adding a new one later (e.g., symbol/graph search) is additive, not a rewrite
- The project scope stays sharply defined; "a daemon that does these three things" is a succinct pitch

### Negative

- More surface area to design, document, and maintain than a single-mode tool would have
- Three modes mean three quality dimensions to tune (regex behavior, ranking, embedding model choice)

### Neutral

- The three modes are independent; if one turns out hard to ship (semantic is the likely candidate), the other two can ship first and remain useful — an early shipping-gate decision captured in the v0 step plan

---

## Notes

- Related to [ADR-0003](./0003-indexing-in-the-daemon.md) (Indexing in the daemon); the choice to do indexing locally is what makes semantic-as-peer affordable
- The v0 step plan (see `implementation/tech-stack.md`) deliberately orders filesystem + content (step 5) before semantic (step 7), so a partial ship remains useful

## Amendments

<!-- None yet -->
