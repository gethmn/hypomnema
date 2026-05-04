# FTS5 / BM25 Content Search Stories

These stories correspond to [`fts5-bm25-content-search.md`](./fts5-bm25-content-search.md). They describe delivery scope; the spec owns the behavior contract.

## Story 1: Ranked lexical content search

**Story:** As the user, I want content search to rank files by lexical relevance so that I can decide which note to open first when I know the topic but not the exact phrase.

**Acceptance Criteria:**
- [ ] Given a vault with `a.md` containing repeated discussion of `sqlite`, `vector`, and `index`, `b.md` containing only `sqlite`, and `c.md` containing none of those terms, `POST /search/content` with `{"query":"sqlite vector index","mode":"ranked","limit":10}` returns `a.md` before `b.md` and omits `c.md`.
- [ ] The same ranked request returns `score` and `rank` for each result, with `rank: 1` on `a.md`; `a.md.score` is less than or equal to `b.md.score` because lower BM25 values rank better.
- [ ] Given two files whose FTS score is equal for query `sqlite` because each contains exactly one `sqlite` token, they are ordered by `path` ascending; equal paths across vaults break ties by `vault`.
- [ ] The ranked path runs through HTTP, stdio MCP, and HTTP MCP with the same request fields and result ordering.

## Story 2: Exact content search remains available

**Story:** As the user, I want exact substring and regex search to keep their current behavior so that I can verify quotes and literal references without tokenization changing the answer.

**Acceptance Criteria:**
- [ ] Given a file containing the literal phrase `sqlite-vec stores vectors`, `POST /search/content` with `{"query":"sqlite-vec stores vectors","mode":"substring","include_matches":true}` returns that file with a matching snippet.
- [ ] Given files that contain `sqlite`, `vec`, `stores`, and `vectors` separately but not the full literal phrase, the same substring request does not return those files.
- [ ] Existing legacy requests with `{"query":"sqlite-vec stores vectors","regex":false}` behave as substring mode.
- [ ] Existing legacy requests with `{"query":"sqlite.*vectors","regex":true}` behave as regex mode and still return `invalid_regex` for invalid Rust regex patterns.

## Story 3: FTS index freshness follows file lifecycle

**Story:** As the user, I want ranked search to reflect creates, edits, deletes, rescans, and resets so that it is as fresh as the existing content index.

**Acceptance Criteria:**
- [ ] After a watched file is created and indexed, a ranked search for a term unique to that file returns the new path without requiring daemon restart.
- [ ] After that file is edited so the unique term is removed and the watcher/indexer completes, the same ranked search no longer returns the path.
- [ ] After that file is deleted and the watcher/indexer completes, ranked search does not return a stale path.
- [ ] After `hmn vault reset --rebuild` followed by rescan, ranked search returns files from the rebuilt index and does not return rows for deleted files.

## Story 4: Request validation is explicit

**Story:** As the user, I want invalid ranked-search requests to fail clearly so that I can fix my query or flags without guessing which matcher ran.

**Acceptance Criteria:**
- [ ] `{"query":"sqlite","mode":"ranked","case_sensitive":true}` returns an `invalid_request` envelope explaining that `case_sensitive` only applies to substring mode.
- [ ] `{"query":"sqlite","mode":"ranked","regex":true}` returns an `invalid_request` envelope explaining the `regex`/`mode` conflict.
- [ ] An FTS-syntax-invalid ranked query returns `invalid_query` and does not appear as `vault_search_failed` in `partial_results`.
- [ ] `hmn search content "sqlite" --mode ranked --case-sensitive` exits non-zero and prints the daemon's structured error in text mode.

## Story 5: Multi-vault ranked merge is deterministic

**Story:** As the user, I want ranked content search across vaults to merge results predictably so that repeated searches are stable and partial failures are visible.

**Acceptance Criteria:**
- [ ] Given vault `alpha` with `shared.md` containing `sqlite sqlite vector` and vault `bravo` with `shared.md` containing `sqlite`, a ranked request without `vaults` returns the `alpha` row before the `bravo` row.
- [ ] Given vault `alpha` and vault `bravo` both containing `shared.md` with identical content `sqlite`, a ranked request without `vaults` returns both rows ordered by `vault` ID as the final tie-break.
- [ ] Given `vaults: ["personal"]`, ranked search returns only matches from the `personal` vault.
- [ ] Given one active vault and one paused vault in default scope, ranked search returns active-vault results and includes the paused vault in `partial_results.skipped`.
- [ ] Given an unknown vault name in `vaults`, ranked search reports `vault_not_found` in `partial_results.failed` and still searches recognized vaults.

## Cross-Story Guardrails

- [ ] All FTS5 schema creation, FTS maintenance, and ranked query SQL happens inside `spawn_blocking`; `rg "files_fts|bm25" src | rg -v "spawn_blocking|schema|test"` should be manually reviewed for any SQL path outside the blocking pattern.
- [ ] Negative fingerprint: after implementation, `rg "SELECT path, content FROM files" src/search/content.rs` must not be the only content-search query path.
- [ ] Negative fingerprint: after implementation, `rg "ORDER BY path ASC" src/search/content.rs` must not be the ordering used for `mode: "ranked"`.
