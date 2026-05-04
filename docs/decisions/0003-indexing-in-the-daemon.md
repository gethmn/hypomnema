# ADR-0003: Indexing in the Daemon, Not in the Consumer

**Status**: accepted
**Date**: 2026-04-23
**Decision-Makers**: Beau Simensen

---

## Context

An earlier scaled-back version of this project (the "bridge-as-service" refinement of the original Iris vault bridge) kept Hypomnema as a thin filesystem wrapper and relied on each consumer — principally Iris — to do its own chunking and embedding via its own retrieval pipeline (Iris's `ContextRetriever`). The bridge would have provided filesystem and content search only; semantic search would have been the consumer's problem.

The alternative — moving chunking, embedding, and vector storage *into* Hypomnema — changes the shape of the project materially:
- Hypomnema becomes opinionated about embedding models, chunk boundaries, and vector stores
- Hypomnema becomes useful to consumers beyond Iris (Claude Code, skills.sh packages, ad-hoc CLI use, possibly other Iris instances)
- Hypomnema becomes a long-running service with persistent state, not a thin library

## Decision

Do the indexing (scanning, chunking, embedding, storing) inside Hypomnema itself. Expose all three search modes — filesystem, content, and semantic — as first-class peers of the daemon's API.

This means Hypomnema:
- Picks a concrete chunking strategy (pulldown-cmark heading-aware chunks)
- Picks a concrete embedding shape (nomic-embed-text-v1.5 via local TEI or vLLM, 768 dims)
- Picks a concrete vector store (sqlite-vec)
- Maintains the chunk → vector → source-file mapping persistently

Consumers consume the search API; they don't rebuild any of this themselves.

## Consequences

### Positive

- Consumers other than Iris get semantic search for free; no need to each reinvent the chunk/embed/store pipeline
- Justifies Hypomnema's shape as a daemon (long-running, persistent state) rather than a library
- Retrieval quality is consistent across consumers — one implementation to tune, one place to fix bugs
- Makes the three search modes (filesystem / content / semantic) peers of each other, which is the shape an agent actually wants

### Negative

- Hypomnema becomes opinionated about embedding models and vector stores; no longer a neutral format toolkit
- Model-switching is not a runtime switch — it requires a full re-index (dimension is baked into the schema)
- Consumers that want to use a different embedding strategy have to either tolerate Hypomnema's choices or not use its semantic layer (filesystem and content search still work)

### Neutral

- Iris's `ContextRetriever` becomes thinner, not fatter — it can lean on Hypomnema for vault content and focus on Iris's own state

---

## Notes

- Related to [ADR-0004](./0004-three-search-modes-as-peers.md) (Three search modes as peers), [ADR-0007](./0007-sqlite-vec-over-alternatives.md) (sqlite-vec over alternatives)
- Alternative embedding providers can be added later behind an abstraction; not a v0 concern (see the "no abstraction layers until the second concrete implementation demands it" line in the project handoff)

## Amendments

### 2026-05-03 — Chunking strategy is v0's choice, not a forever architectural commitment

When this ADR was written, "pulldown-cmark heading-aware chunks" was named as the concrete chunking strategy without explicitly distinguishing *v0's chosen strategy* from *the only strategy this ADR permits*. To keep future scope conversations honest:

- The decision to do indexing **inside the daemon** (rather than push it to consumers) is format-agnostic and remains in force as written.
- The decision to ship **pulldown-cmark heading-aware chunking** is v0's chunking strategy. It is load-bearing for v0 (semantic-search.md, the watcher's `.md` filter, the `markdown-chunking` skill all depend on it) but it does not preclude additional chunking strategies for non-Markdown text in a later phase.
- Adding a second chunking strategy later is an *extension* of this ADR, not a supersession — analogous to "Alternative embedding providers can be added later behind an abstraction" already noted above. A future ADR (or an extension) would specify the strategy and the trigger condition (e.g., a configurable per-vault `index_extensions` plus a chunker selected by file type).

This amendment changes no v0 behavior; it clarifies the boundary so the project's positioning ("a directory of notes" rather than "Markdown only", per `docs/product/vision.md` § Non-Goals → "Text-format coverage beyond Markdown") doesn't silently contradict the ADR.
