# Content Retrieval — User Stories

**Spec**: [`notes/proposals/content-retrieval.md`](./content-retrieval.md)

These stories define delivery scope for the content retrieval feature. They do not duplicate the spec's behavior contract — the spec is the reference; these stories define what "done" looks like for each deliverable slice.

---

## Epic: Core retrieval operation

### Story 1: Retrieve a single file by vault-relative path

**As the user, I want to retrieve the full indexed text of a file I already know the path of, so that I can read its content without having to search for it first.**

**Acceptance Criteria**:

- [ ] A `POST /content/get` request with `paths: ["notes/databases/pgvector.md"]` and `vaults: ["personal"]`, where the file is indexed in `personal`, returns HTTP 200 with a `results` array containing exactly one item: `path`, `content`, `content_hash`, `size`, `mtime`, `vault`, `vault_name` all populated.
- [ ] The returned `content` matches the raw text stored in `files.content` for that row — not a live filesystem read. Verified by: modifying the file on disk without triggering a rescan, then calling `content_get` and confirming the response still matches the pre-modification indexed text.
- [ ] The returned `content_hash` matches the `content_hash` field for the same file in a `search_filesystem` result (same index row, same value).
- [ ] A request with `paths: ["nonexistent/file.md"]` returns HTTP 200 with one result item containing `error.code: "path_not_found"` — not a 404 top-level error.
- [ ] `rg 'fs::read\|tokio::fs::read\|File::open' src/` (or the equivalent handler path) returns zero matches attributable to the content-get handler — the handler never opens vault files directly.

---

### Story 2: Retrieve multiple files in one request

**As the user, I want to retrieve several files in a single request, so that I avoid round-tripping to the daemon once per file when I need multiple files after a search.**

**Acceptance Criteria**:

- [ ] A request with `paths: ["a.md", "b.md", "c.md"]` (all present in the vault) returns a `results` array with exactly three success items, one per path.
- [ ] A request with `paths: ["a.md", "missing.md"]` returns two result items: one success for `a.md` and one `path_not_found` error for `missing.md`. The response is HTTP 200 — partial success is not a request-level failure.
- [ ] A request with `paths: []` (empty array) returns HTTP 422 with `code: "invalid_request"`.
- [ ] Result items in the response are ordered by `path` ascending, then `vault_id` ascending — not by the order the paths were listed in the request.

---

### Story 3: Per-item errors do not abort the batch

**As the user, I want a missing or unreachable file in my batch to produce a per-item error rather than failing the whole request, so that I still get the files that were found.**

**Acceptance Criteria**:

- [ ] A batch of five paths where three exist and two do not returns HTTP 200 with five result items: three success items and two `path_not_found` error items.
- [ ] Each error item contains `path`, `vault`, `vault_name`, and `error.code` + `error.message`.
- [ ] The success items contain the full `content`, `content_hash`, `size`, and `mtime` fields.
- [ ] A batch where all five paths are missing returns HTTP 200 with five `path_not_found` items (not a 404 or 503 top-level error — all paths were legitimately queried and definitively not found).

---

## Epic: Multi-vault fan-out

### Story 4: Fan out to all active vaults by default

**As the user, I want a content retrieval request with no vault selector to search all active vaults, so that I don't have to know which vault a file lives in when I already know its path.**

**Acceptance Criteria**:

- [ ] A request with `paths: ["notes/index.md"]` and no `vaults` field, where the daemon has two active vaults both containing `notes/index.md`, returns two result items — one per vault — in the response.
- [ ] Both items carry distinct `vault` IDs and `vault_name` values, with content from each respective vault's index.
- [ ] A paused vault is not included in the default-scope fan-out; one entry for it appears in `partial_results.skipped`.

---

### Story 5: Scope retrieval to a named vault

**As the user, I want to restrict a content retrieval request to a specific vault by name, so that I can avoid path-collision ambiguity when I know which vault I want.**

**Acceptance Criteria**:

- [ ] A request with `paths: ["notes/index.md"]` and `vaults: ["personal"]` returns at most one result item — the one from `personal` — even when another active vault also contains `notes/index.md`.
- [ ] A request with `vaults: ["nonexistent-vault"]` returns HTTP 404 with `code: "vault_not_found"`.
- [ ] A request with `vaults: []` (empty array) returns HTTP 422 with `code: "invalid_request"`.

---

### Story 6: Explicit retrieval from a paused vault

**As the user, I want to be able to retrieve indexed content from a paused vault I explicitly name, so that I can access archived notes even when their watcher is stopped.**

**Acceptance Criteria**:

- [ ] A request with `vaults: ["archive"]` where `archive` is paused and its `index.sqlite` is readable returns HTTP 200 with the result items populated from the index.
- [ ] The response also includes a `partial_results.skipped` entry for `archive` with `status: "paused"`.
- [ ] A request with no `vaults` selector does not include the paused vault's results — paused vaults are excluded from the default scope.

Scenario (Given/When/Then):

```
Given vault "archive" is paused and its index contains "notes/ref.md"
When POST /content/get with paths: ["notes/ref.md"] and vaults: ["archive"]
Then HTTP 200
  AND results contains one success item for "notes/ref.md" with content from archive's index
  AND partial_results.skipped contains one entry: vault="archive", status="paused"
```

---

## Epic: Transport surface

### Story 7: Retrieve content via CLI

**As the user, I want to retrieve a file's content with `hmn content get`, so that I can read vault files from the terminal without knowing the daemon's HTTP API.**

**Acceptance Criteria**:

- [ ] `hmn content get "notes/databases/pgvector.md" --vault personal` prints the file's indexed content to stdout, with a metadata header line showing path, vault name, size, and content_hash.
- [ ] `hmn content get "a.md" "b.md" --vault personal --json` prints a JSON envelope matching the HTTP response schema.
- [ ] When one of two requested paths is not found, the found file's content is printed to stdout (exit 0) and the not-found path is reported to stderr.
- [ ] When all requested paths are not found, the command exits non-zero and prints error details to stderr.
- [ ] `hmn content get` with no paths argument prints usage help and exits non-zero.

---

### Story 8: Retrieve content via MCP tool `content_get`

**As an AI agent, I want to call the `content_get` MCP tool after discovering a file path through search, so that I can read the file's full content without issuing a separate HTTP call or reading the vault filesystem directly.**

**Acceptance Criteria**:

- [ ] The `content_get` tool is present in the tool list returned by `hmn mcp` (stdio transport) with input schema matching the `paths` + `vaults` request shape.
- [ ] The `content_get` tool is present in the tool list returned by the Streamable-HTTP MCP endpoint (`/mcp` on `hmnd`).
- [ ] Calling `content_get` with `paths: ["notes/pgvector.md"], vaults: ["personal"]` via the stdio shim returns a tool result with `results` containing the file's content.
- [ ] The `content_get` tool is available regardless of the `[mcp] enable_write_tools` config setting — it is a read-only tool.
- [ ] An MCP round-trip that calls `search_filesystem` to discover a path, then calls `content_get` with that path, produces a `content_hash` in the `content_get` response that matches the `content_hash` in the `search_filesystem` result (same indexed row, consistent state).

---

## Epic: Request validation

### Story 9: Reject invalid paths

**As the user, I want the daemon to reject paths containing `..` or a leading `/`, so that I get a clear error rather than an unexpected empty result or security surprise.**

**Acceptance Criteria**:

- [ ] A request with `paths: ["/absolute/path.md"]` returns HTTP 422 with `code: "invalid_path"` and a message indicating the path must be vault-relative.
- [ ] A request with `paths: ["../escape.md"]` returns HTTP 422 with `code: "invalid_path"`.
- [ ] A request with `paths: ["notes/../escape.md"]` returns HTTP 422 with `code: "invalid_path"` (internal `..` segments are also rejected).
- [ ] A valid vault-relative path like `"notes/databases/pgvector.md"` is accepted without error.
