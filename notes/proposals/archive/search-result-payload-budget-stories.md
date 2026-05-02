# Search Result Payload Budget User Stories

This story artifact pairs with `notes/proposals/archive/search-result-payload-budget.md`. It defines delivery scope for keeping search responses useful without consuming excessive agent context.

---

## Story 1: Correct Semantic Default Limit Drift

**Story:** As the user, I want semantic search to default to a small result count so that a normal agent query does not inject an unexpectedly large response into the conversation.

**Acceptance Criteria:**

- [ ] When `POST /search/semantic` is called without `limit` against a fixture with more than 10 matching chunks, the response contains exactly 10 results and omits lower-ranked chunks.
- [ ] When the same request supplies `limit: 17`, the response can include the seventeenth ranked chunk if the fixture has at least 17 matches.
- [ ] Filesystem and content search still default to 100 results when `limit` is omitted.
- [ ] Semantic responses include the canonical `truncated` field and set it to `true` when result count is capped by the final merged limit.
- [ ] MCP `search_semantic` tool schema describes semantic `limit` as defaulting to 10, not 100.
- [ ] Negative fingerprint: `rg 'Maximum number of result chunks. Defaults to 100|DEFAULT_LIMIT == 100' src tests docs/specs` returns no semantic-search default-limit survivors.

---

## Story 2: Return Bounded Semantic Preview Text by Default

**Story:** As the user, I want semantic results to include bounded preview text so that I can triage relevant chunks without receiving every full chunk body by default.

**Acceptance Criteria:**

- [ ] Given a chunk whose stored content is longer than the default preview budget, a semantic search without `include_text` returns preview text no longer than the documented budget and marks the result as truncated.
- [ ] Given a chunk whose stored content fits within the default preview budget, a semantic search without `include_text` returns the whole chunk text and marks the result as not truncated.
- [ ] Preview truncation never emits invalid UTF-8, including when the truncation boundary falls near a multibyte character.
- [ ] The response includes stable follow-up identifiers for each result: `file_path`, `chunk_index`, `heading_path`, and vault metadata where multi-vault is active.
- [ ] The HTTP response and MCP structured content are byte-for-byte equivalent for the same semantic query, preserving the existing transport parity tests.

---

## Story 3: Allow Explicit Full-Text Semantic Results

**Story:** As the user, I want to explicitly request full semantic chunk text so that I can trade a larger response for immediate context when I know the result count is small.

**Acceptance Criteria:**

- [ ] When `include_text: "full"` and `limit: 3` are supplied, semantic search returns complete stored text for the top three chunks.
- [ ] Full-text responses identify the text as full, not preview, through the chosen response metadata.
- [ ] `include_text: "full"` does not change ranking, vault scoping, `min_similarity`, or `truncated` behavior compared with the same query using default preview text.
- [ ] Invalid `include_text` values return `invalid_request` over HTTP and an equivalent MCP structured error.

---

## Story 4: Allow Metadata-Only Semantic Candidate Lists

**Story:** As the user, I want semantic search to optionally omit result text so that I can run broader candidate searches and fetch selected content later.

**Acceptance Criteria:**

- [ ] When `include_text: "none"` is supplied, each semantic result omits text payload while preserving score, file path, chunk index, heading path, and vault metadata.
- [ ] With the same fixture and query, `include_text: "none"` returns the same result ordering as default preview mode.
- [ ] A request with `include_text: "none"` and `limit: 20` produces a materially smaller JSON body than the same request with `include_text: "full"` for long chunks.
- [ ] The API schema documents `include_text` enum values and the default.

---

## Story 5: Keep Search and Retrieval Boundaries Separate

**Story:** As the user, I want search results to point to retrievable content without becoming full content retrieval so that discovery stays cheap and follow-up reads remain explicit.

**Acceptance Criteria:**

- [ ] The semantic response includes enough identifiers for a follow-up retrieval request: source path, chunk index or heading path, and vault identity when multi-vault is active.
- [ ] The search-result payload-budget spec references `notes/proposals/content-retrieval.md` rather than adding full-file content to search results.
- [ ] Filesystem search remains metadata-only; no `text`, `preview`, or `content` field is added to filesystem results.
- [ ] Content search remains snippet-bounded; no full-file `content` field is added to content search results.

---

## Story 6: Verify Content Search Budget Drift

**Story:** As the user, I want content-search snippet defaults to match the documented contract so that exact-match search does not surprise agents with avoidable payload size.

**Acceptance Criteria:**

- [ ] The canonical content-search spec, API schema description, and behavior agree on the default for `include_matches`.
- [ ] The canonical content-search spec, API schema description, and behavior agree on the default for `max_matches_per_file`.
- [ ] A content search without `include_matches` returns no `matches` array if the accepted canonical default remains `false`.
- [ ] A content search with `include_matches: true` returns at most `max_matches_per_file` snippets per file.
- [ ] Long `matches[].text` values are trimmed to the documented byte cap without invalid UTF-8.
