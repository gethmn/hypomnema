# Content Search Specification

**Version**: 0.1.0
**Date**: 2026-04-23
**Status**: Draft

---

## Overview

Content search answers exact-string questions: *which files contain this phrase?* It is grep-shaped — queries are strings or regexes; results are files (optionally with matching lines). This is the search mode an agent uses to verify a reference or find a specific quote.

**Related Documents**:
- [ADR-0004: Three Search Modes as Peers](../decisions/0004-three-search-modes-as-peers.md)
- [Architecture: Search API](../architecture/overview.md#search-api)

---

## Behavior

### Normal Flow

1. Consumer sends a query (substring or regex) with optional path filter
2. Hypomnema queries the content index in the store
3. Response includes files that match, optionally with line-level matches

File text is stored inside the SQLite store as part of the indexer's work — content search does not re-read files on every query.

### Semantics

- Default: case-insensitive substring match (ASCII-folded; Unicode case folding is not applied in v0).
- Optional: case-sensitive mode
- Optional: regex mode using the Rust `regex` crate's default Unicode flavor. The request's `case_sensitive` flag is ignored when `regex: true`; case-sensitivity is a property of the pattern (`(?i)foo`).
- A file matches if it contains at least one occurrence of the query
- Phrase searches span line boundaries — the matcher operates over the file's full byte content, not per-line. See [step-5 workplan § Deferred decision 3](../roadmap/step-05-workplan.md#3-phrase-search-across-line-boundaries).

---

## Data Schema

### Request

```yaml
query: "pgvector"             # required
regex: false                   # optional; if true, query is a regex
case_sensitive: false          # optional
prefix: "notes/databases/"     # optional; restrict to a subdirectory
include_matches: true          # optional; return matching lines
max_matches_per_file: 5        # optional; default 5
limit: 100                     # optional; default 100
```

### Response

```yaml
results:
  - path: "notes/databases/pgvector.md"
    match_count: 7
    matches:
      - line: 12
        text: "pgvector supports HNSW and IVF indexes."
      - line: 45
        text: "Compared to pgvector, sqlite-vec trades features for portability."
truncated: false
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `path` | string | yes | Vault-relative path |
| `match_count` | integer | yes | Total matches in the file (may exceed `matches.len()` when `max_matches_per_file` truncates) |
| `matches` | array | no | Per-line match details when `include_matches: true`; omitted otherwise |
| `vault` | string | no | Reserved; always absent in v0. Will carry the source vault identifier when multi-vault ships. |
| `truncated` | boolean | yes | True if results exceeded `limit` |

---

## Edge Cases

### Binary or very large files

Not a concern in v0: only Markdown files are indexed.

### Query too broad

If `limit` is exceeded, results are truncated and `truncated: true` is set. No pagination in v0.

### Regex with catastrophic backtracking

Rust's `regex` crate does not support backreferences and has linear-time matching, so pathological patterns are not a v0 DoS concern.

### Lossy UTF-8

Invalid UTF-8 byte sequences in file bodies are decoded with `String::from_utf8_lossy` before storage (replacement char `U+FFFD` substituted in). Matches against the lossy form are still surfaced — vault hygiene problems become searchable-but-noisy rather than indexer crashes. The `content_hash` continues to be computed over the raw bytes; lossy decode is a storage-side concern only.

---

## Open Questions

- [x] Should we support phrase search across line boundaries? (Probably yes — Markdown prose wraps.) — Resolved in step 5 as line-agnostic matching. See [step-5 workplan § Deferred decision 3](../roadmap/step-05-workplan.md#3-phrase-search-across-line-boundaries).
- [ ] Should frontmatter-only matches be distinguishable from body matches?

---

## Revision History

| Version | Date | Changes |
|---------|------|---------|
| 0.1.0 | 2026-04-23 | Initial draft, seeded from project handoff v0 scope |
