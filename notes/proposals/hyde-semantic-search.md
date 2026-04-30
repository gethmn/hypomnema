# HyDE Semantic Search Proposal

**Status**: Draft
**Date**: 2026-04-30

---

## Summary

HyDE, or Hypothetical Document Embeddings, is a semantic-search query expansion technique. Instead of embedding the user's short query directly, a language model first writes a short hypothetical answer or note section that would satisfy the query. Hypomnema then embeds that generated text and runs the existing `search_semantic` vector lookup against indexed chunks.

This is different from FTS5/BM25. FTS5/BM25 improves ranked lexical retrieval over terms already present in the vault. HyDE improves semantic retrieval when the user has an abstract, vague, or question-shaped query whose wording may not overlap with the best chunks.

## Current Baseline

Today `search_semantic` does:

```text
query text -> embedding service -> query vector -> sqlite-vec kNN -> chunks
```

HyDE would do:

```text
query text -> generation service -> hypothetical document -> embedding service -> query vector -> sqlite-vec kNN -> chunks
```

The stored semantic index does not need to change. Chunks stay chunked and embedded exactly as they are now.

## What It Would Bring

HyDE is most useful for queries like:

- `how do we prevent spurious reindexes?`
- `why did we choose sqlite-vec?`
- `what are the risks around watcher event storms?`
- `how does qmd differ from Hypomnema?`

For these, a generated hypothetical answer can introduce project vocabulary that the original query omitted: `content_hash`, `notify-debouncer-full`, `chunks_vec`, `sqlite-vec`, `outbox`, `data_dir`, and so on. Embedding models often compare document-like text to document-like text better than they compare a short question to document chunks.

HyDE is not a replacement for:

- substring/regex content search for exact quote verification
- FTS5/BM25 ranked lexical search for known terms
- direct semantic search when the user's query already describes the relevant concept well

## Design Options

### Option A: Agent-side HyDE recipe first

No daemon changes. The agent generates a hypothetical answer itself, then calls existing `search_semantic` with that generated paragraph as the query.

```text
agent query -> agent drafts hypothetical answer -> search_semantic(hypothetical answer)
```

This is the lowest-risk path. It validates whether HyDE helps in real Hypomnema usage before the daemon owns generation prompts, model config, timeout behavior, or deterministic tests.

### Option B: Daemon-side query mode

Add an optional field to semantic search:

```yaml
query: "how do we prevent spurious reindexes?"
query_mode: "hyde"   # default: "direct"
limit: 10
```

`query_mode: "direct"` preserves current behavior. `query_mode: "hyde"` makes the daemon call a configured generation endpoint, embed the generated hypothetical document, and search with that vector.

This would likely amend `docs/specs/semantic-search.md`, `docs/reference/configuration.md`, and `docs/reference/cli.md`.

## Design Impact

HyDE is larger than FTS5/BM25 from a daemon-design standpoint because it introduces text generation into the retrieval path. That raises questions Hypomnema does not currently need to answer:

- What generation endpoint is configured?
- Is it local-only by default per ADR-0005?
- Is it the same service as embeddings or a separate `[generation]` config section?
- What prompt is used?
- What timeout and token limit apply?
- How are generation failures surfaced?
- How do tests avoid depending on nondeterministic LLM output?

The semantic index itself is unchanged. The main impact is request-time latency, configuration surface, and one new failure class before embedding.

## Error Shape If Daemon-Side

Likely new or extended errors:

| Condition | Suggested Code | Behavior |
|---|---|---|
| Generation endpoint unavailable | `generation_unavailable` | HTTP 503 / MCP structured error; do not fall back silently |
| Generation returns empty text | `generation_unavailable` or `invalid_generation` | Fail the HyDE request; direct mode remains available |
| Unknown `query_mode` | `invalid_request` | Existing validation-style error |
| Embedding generated text fails | `embedding_unavailable` | Existing semantic-search error behavior |

Silent fallback from HyDE to direct semantic search is not recommended. It would hide the fact that a different retrieval path ran.

## Proposed Direction

Treat HyDE as a future semantic-search enhancement, not part of the FTS5/BM25 content-search proposal.

Recommended priority path:

1. Document an agent-side HyDE recipe in notes or manual testing.
2. Try it against real Hypomnema vaults and compare results to direct semantic search.
3. If it repeatedly improves recall, draft a full semantic-search amendment for `query_mode: "hyde"`.

## Open Questions

- [ ] Is agent-side HyDE good enough, or does daemon-side support materially improve ergonomics?
- [ ] Should Hypomnema ever own a text-generation dependency, or should it stay retrieval-only?
- [ ] If daemon-side, should generation reuse the embedding endpoint shape when possible or get a separate `[generation]` config block?
- [ ] What prompt template should be stable enough to test and document?
- [ ] Should HyDE results expose the generated hypothetical document for debugging, or keep it internal?

## Related Documents

- [`docs/specs/semantic-search.md`](../../docs/specs/semantic-search.md)
- [`notes/qmd-comparison.md`](../qmd-comparison.md)
- [`notes/proposals/fts5-bm25-content-search.md`](./fts5-bm25-content-search.md)
