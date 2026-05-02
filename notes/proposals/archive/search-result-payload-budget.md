# Search Result Payload Budget Specification

**Version**: 0.1.0
**Date**: 2026-05-01
**Status**: Draft

---

## Overview

Search result payload budgeting keeps Hypomnema search responses useful for agents without flooding their conversation context. The immediate problem is semantic search: returning many full chunk bodies can consume a large token budget, especially over MCP where structured search results are carried forward in long-running agent conversations.

This proposal combines the Solo scratchpads `Semantic Search Token Budget Handoff` and `Search Result Shapes Documentation v0.5.0`. It treats payload size as part of the search result shape contract, not just a semantic-search implementation tweak. Filesystem search remains metadata-only. Content search remains snippet-bounded. Semantic search keeps returning enough text to let an agent triage results, but the default response should be bounded and explicitly distinguish preview text from full content retrieval.

**Related Documents**:
- [ADR-0003: Indexing in the Daemon](../../docs/decisions/0003-indexing-in-the-daemon.md)
- [ADR-0004: Three Search Modes as Peers](../../docs/decisions/0004-three-search-modes-as-peers.md)
- [ADR-0005: Local Everything](../../docs/decisions/0005-local-everything.md)
- [ADR-0007: sqlite-vec over Alternatives](../../docs/decisions/0007-sqlite-vec-over-alternatives.md)
- [Semantic Search Specification](../../docs/specs/semantic-search.md)
- [Content Search Specification](../../docs/specs/content-search.md)
- [Filesystem Search Specification](../../docs/specs/filesystem-search.md)
- [Content Retrieval Proposal](./content-retrieval.md)
- **User Stories**: `notes/proposals/archive/search-result-payload-budget-stories.md`

---

## Behavior

### Normal Flow

1. Consumer sends one of the existing search requests: filesystem, content, or semantic.
2. Hypomnema applies the existing search-mode semantics: metadata discovery for filesystem, exact/regex matching for content, vector similarity for semantic.
3. The response is shaped according to each mode's payload budget:
   - Filesystem returns metadata only and keeps the existing `limit` default.
   - Content returns per-file match snippets only when requested, bounded by `max_matches_per_file` and per-snippet trim size.
   - Semantic defaults to a small result count and returns a bounded preview for each chunk, not an unbounded full chunk body by default.
4. The response carries enough stable identifiers for follow-up retrieval: `file_path`, `chunk_index`, `heading_path`, `content_hash` when available, `vault`, and `vault_name` where the existing multi-vault shape requires them.
5. Consumers that need complete file text use the separate content-retrieval operation once that proposal ships; consumers that need complete chunk text either request an explicit full-text semantic mode or use a future chunk/section retrieval operation.

### State Machine

**State Machine**: N/A -- payload budgeting is request/response shaping over existing search state. Index freshness, chunk creation, and vector persistence remain owned by the current indexer lifecycle.

---

## Data Schema

### Semantic Request

```yaml
query: "watcher event storms from sync tools"
limit: 10                       # optional; default 10
prefix: "notes/"                # optional
min_similarity: 0.3              # optional; default 0.0
include_text: "preview"          # optional; preview | full | none; default preview
preview_bytes: 600               # optional; default 600, capped by server maximum
vaults:
  - "personal"
```

| Request Field | Type | Required | Default | Description |
|---|---|---:|---|---|
| `query` | string | yes | - | Existing natural-language semantic query. |
| `limit` | integer | no | `10` | Global result cap after cross-vault merge. Validation remains `1..=1000`, but callers must opt in to large responses. |
| `prefix` | string | no | none | Existing vault-relative path prefix. |
| `min_similarity` | float | no | `0.0` | Existing score filter, clamped to `[0.0, 1.0]`. |
| `include_text` | enum string | no | `preview` | Controls semantic result text payload: `preview`, `full`, or `none`. |
| `preview_bytes` | integer | no | `600` | Maximum UTF-8 byte length for semantic preview text. Applied only when `include_text: "preview"`; server clamps to a documented maximum. |
| `vaults` | array of strings | no | none -> all active | Existing cross-vault scope filter. |

### Semantic Response

```yaml
results:
  - score: 0.82
    file_path: "notes/tools/hypomnema.md"
    chunk_index: 4
    heading_path: ["Pitfalls", "Sync conflicts"]
    text: "Syncthing and Dropbox write files in bursts..."
    text_kind: "preview"
    text_truncated: true
    content_hash: "sha256:abc123..."
    vault: "01951f6c-7c3b-7a2e-8c1d-1a2b3c4d5e6f"
    vault_name: "personal"
truncated: false
```

| Field | Type | Required | Description |
|---|---|---:|---|
| `score` | float | yes | Existing cosine similarity score in `[0.0, 1.0]`. |
| `file_path` | vault-relative path | yes | Existing source file path. |
| `chunk_index` | integer | yes | Existing ordinal of the chunk within the file. |
| `heading_path` | array of strings | yes | Existing heading breadcrumb for the chunk. |
| `text` | string | conditional | Present when `include_text` is `preview` or `full`; omitted when `include_text: "none"`. |
| `text_kind` | enum string | conditional | `preview` or `full`, matching what was returned in `text`. |
| `text_truncated` | boolean | conditional | `true` when preview text omits part of the stored chunk. `false` for full text or short chunks that fit within the preview budget. |
| `content_hash` | `sha256:` string | yes | Parent file content hash from the indexed row, for cache validation and follow-up retrieval consistency. |
| `vault` | string | no | Existing surrogate vault ID when multi-vault is active. |
| `vault_name` | string | no | Existing point-in-time display name when multi-vault is active. |
| `truncated` | boolean | yes | Existing result-count truncation flag. |
| `partial_results` | object | no | Existing cross-vault partial-result envelope. |

### Content Search Budget

Content search keeps the current result shape, with these constraints made explicit:

| Field / Parameter | Budget Rule |
|---|---|
| `limit` | Default remains `100`; validation remains `1..=1000`. |
| `include_matches` | Default must match the canonical spec (`false`) unless deliberately amended. Current API schema comments advertising a different default are drift. |
| `max_matches_per_file` | Default remains `5`. |
| `matches[].text` | Each snippet remains trimmed to the implementation's documented byte cap. If the cap changes, the content-search spec and API schema must change together. |

### Filesystem Search Budget

Filesystem search remains metadata-only. No text preview or content field is added in this proposal.

### Validation Rules

- `include_text` must be one of `preview`, `full`, or `none`.
- `preview_bytes` must be positive when supplied. Values above the server maximum are clamped or rejected consistently with the final implementation choice; the behavior must be documented in `docs/specs/semantic-search.md`.
- `limit` remains validated as `1..=1000`, but the default for semantic search must be `10` in docs, API schema, HTTP behavior, MCP tool schema, and tests.
- Semantic responses must expose the canonical `truncated` envelope field. The current canonical spec requires it, and the search-result-shape scratchpad assumes it exists across all three search modes.
- Response-shape docs and generated schemas must not claim semantic search defaults to `100` once this proposal is accepted.

---

## Examples

### Example 1: Default semantic search stays small

**Input**:

```yaml
query: "why do sync tools cause watcher event storms?"
```

**Behavior**: Hypomnema embeds the query, searches each active vault's vector index, merges by score, caps the final response to the default semantic `limit` of 10, and returns preview text for each chunk.

**Result**: The response carries enough context to choose which file or section to inspect next, without returning 100 full chunks into the agent context.

### Example 2: Caller explicitly asks for full semantic chunks

**Input**:

```yaml
query: "sqlite vec migration risks"
limit: 3
include_text: "full"
```

**Behavior**: Hypomnema returns the top three matching chunks with complete stored chunk text.

**Result**: The larger payload is intentional and bounded by the caller's explicit `limit`. Consumers that request `include_text: "full"` are responsible for the downstream context cost.

### Example 3: Discovery-only semantic pass

**Input**:

```yaml
query: "semantic search ranking problems"
limit: 20
include_text: "none"
```

**Behavior**: Hypomnema returns scores, file paths, chunk indexes, heading paths, hashes, and vault metadata without chunk text.

**Result**: The caller can use the response as a cheap candidate list, then fetch selected content through a retrieval operation.

---

## Edge Cases

### Useful result is below the default cutoff

**Scenario**: The most useful result for a query appears at rank 17, while semantic search defaults to `limit: 10`.

**Behavior**: The default response omits rank 17. The caller can rerun with a larger `limit`, lower or remove `min_similarity`, use `include_text: "none"` for a broader cheap candidate pass, or combine with content/ranked lexical search.

**Rationale**: A default is not a recall guarantee. The default should protect the common agent path from context blow-ups; explicit caller intent should unlock broader scans.

### Boilerplate-heavy chunks

**Scenario**: A matched chunk contains large Dataview blocks, fenced code, or generated boilerplate that inflates response size.

**Behavior**: Default semantic responses return a preview. The chunker does not strip fenced code or boilerplate as part of this proposal.

**Rationale**: Code blocks and raw sections can be semantically important in technical vaults. Stripping them at chunking time would mutate the searchable corpus and needs separate research. Response-time previewing is reversible and safer.

### Preview cuts through Markdown structure

**Scenario**: `preview_bytes` lands in the middle of a paragraph, list, or fenced code block.

**Behavior**: The implementation should prefer valid UTF-8 and, where cheap, paragraph or line boundaries. It must never return invalid UTF-8. The exact truncation boundary strategy is a workplan-time detail unless it changes the wire contract.

**Rationale**: The contract is bounded preview text, not perfect rendering. Overfitting preview generation would make search slower without solving retrieval quality.

### Search result shape drift

**Scenario**: `docs/specs/semantic-search.md` says the default limit is 10, while API schema comments, tests, or MCP tool descriptions say 100.

**Behavior**: Treat this as a documentation/contract bug. Canonical specs and generated tool schemas must converge before claiming the feature is done.

**Rationale**: Agents often discover behavior through MCP schemas and tool descriptions, not by reading docs. Schema drift creates the exact surprise this proposal exists to remove.

---

## Error Handling

| Error Condition | Error Code/Type | Message | Recovery |
|---|---|---|---|
| Unknown `include_text` value | `invalid_request` | `include_text must be one of preview, full, none` | Fix the request. |
| `preview_bytes` is zero | `invalid_request` | `preview_bytes must be greater than 0` | Send a positive value or omit the field. |
| `preview_bytes` exceeds server maximum | `invalid_request` or clamped behavior | If rejected: `preview_bytes must be <= <max>` | Use the documented maximum. If clamped, inspect `text_truncated`. |
| Semantic embedding service unavailable | existing `embedding_unavailable` | Existing semantic-search message | Retry later or use filesystem/content search. |
| Invalid prefix / vault selector | existing validation errors | Existing search error messages | Fix path or vault selector. |
| Per-vault search failure | existing `partial_results.failed` entry | Existing per-vault diagnostic | Inspect daemon logs, reset/rescan affected vault. |

---

## Integration Points

### `docs/specs/semantic-search.md`

This proposal most directly amends semantic search. The canonical spec already states `limit` defaults to `10`; the amendment should make text budgeting explicit and decide whether the wire field remains `text` with metadata (`text_kind`, `text_truncated`) or changes to a new `preview` field. Backward compatibility favors keeping `text` for `preview`/`full` responses and adding metadata.

### `src/api/types.rs` and MCP Tool Schemas

`SemanticQueryJson` currently advertises `limit` as defaulting to `100` in its schema description. That must be corrected to `10`. New request fields such as `include_text` and `preview_bytes` must appear in the generated JSON schema so MCP clients see the same contract as HTTP clients.

`SemanticSearchResponse` should also be checked against the canonical envelope. At the time of drafting, the semantic spec requires `truncated`, but the Rust response type does not expose it. That is result-shape drift and should be fixed with the limit/default cleanup.

### `src/api/search.rs`

The shared `DEFAULT_LIMIT` currently applies across all three search modes. Semantic search needs its own default constant. Filesystem and content can continue using `100`; semantic should use `10`.

### `src/search/semantic.rs`

The SQL query may still read stored chunk content, but the result mapping should apply response shaping after retrieval and before serialization. If preview generation is pure string work, it does not need `spawn_blocking`; SQL still stays inside `spawn_blocking` per `.claude/skills/rusqlite-in-async/`.

### `notes/proposals/content-retrieval.md`

This proposal should not grow into full file retrieval. Search remains discovery and triage. Full content belongs in the content-retrieval operation. If chunk- or section-level retrieval is needed, it should amend content retrieval or get its own small proposal rather than bloating search.

### Manual Testing and Dogfood Notes

The manual search testing docs should include at least one semantic query that demonstrates default payload size, explicit larger `limit`, and `include_text: "none"` or equivalent once implemented.

---

## Implementation Notes

This is intentionally a proposal rather than an immediate canonical spec patch because there are two separable decisions:

- The low-risk correction is to align semantic default limit across docs, API schema, tests, and implementation at `10`.
- The larger behavior change is to stop treating full chunk text as the default semantic result body and introduce an explicit text budget contract.

Do not solve boilerplate-heavy chunks by stripping fenced code blocks at indexing time in this work. The scratchpad observation about Dataview noise is real, but code and raw data are legitimate note content. Chunk-time filtering would change embeddings, ranking, and stored content; it needs separate evidence and likely a separate spec.

Negative-fingerprint greps after implementation:

```sh
rg 'Defaults to 100' src/api docs/specs tests -g '*.rs' -g '*.md'
rg 'DEFAULT_LIMIT' src/api/search.rs
rg 'semantic_handler_default_limit_caps_at_default|DEFAULT_LIMIT == 100' src tests
rg 'struct SemanticSearchResponse' -n src/api/types.rs
```

Expected result: no semantic-search schema, test, or prose path claims semantic's default limit is 100. A shared `DEFAULT_LIMIT` may remain only if it is not used for semantic search.

---

## Open Questions

- [ ] Should semantic preview use `text` plus `text_kind` / `text_truncated`, or introduce a separate `preview` field and reserve `text` for full chunks? Backward compatibility favors the former; clarity favors the latter.
- [ ] What server maximum should apply to `preview_bytes`? A reasonable starting point is 600-800 bytes, but the exact value should be chosen during implementation against real dogfood payloads.
- [ ] Should `content_hash` be added to semantic results now? The semantic spec names it in chunk metadata but does not currently expose it in the response table. Adding it improves retrieval consistency and cache validation.
- [ ] Should there be a chunk/section retrieval operation, or is full-file content retrieval enough for follow-up workflows?
- [ ] Should content search's default `include_matches` be corrected in code/schema if it has drifted from the canonical spec?

---

## Revision History

| Version | Date | Changes |
|---|---|---|
| 0.1.0 | 2026-05-01 | Initial proposal combining semantic token-budget handoff and search-result shape scratchpad into a payload-budget contract. |
