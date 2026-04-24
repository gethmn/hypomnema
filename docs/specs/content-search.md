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

- Default: case-insensitive substring match
- Optional: case-sensitive mode
- Optional: regex mode (syntax TBD; likely Rust's `regex` crate flavor)
- A file matches if it contains at least one occurrence of the query

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

---

## Edge Cases

### Binary or very large files

Not a concern in v0: only Markdown files are indexed.

### Query too broad

If `limit` is exceeded, results are truncated and `truncated: true` is set. No pagination in v0.

### Regex with catastrophic backtracking

Rust's `regex` crate does not support backreferences and has linear-time matching, so pathological patterns are not a v0 DoS concern.

---

## Open Questions

- [ ] Should we support phrase search across line boundaries? (Probably yes — Markdown prose wraps.)
- [ ] Should frontmatter-only matches be distinguishable from body matches?

---

## Revision History

| Version | Date | Changes |
|---------|------|---------|
| 0.1.0 | 2026-04-23 | Initial draft, seeded from project handoff v0 scope |
