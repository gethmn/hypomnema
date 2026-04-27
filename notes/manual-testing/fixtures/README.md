# Sample fixture vault — expected results

The vault at [`sample-vault/`](./sample-vault/) is engineered for
deterministic search outcomes. This file documents what each search in
[`../03-search.md`](../03-search.md) should return when run against an
unmodified copy of the vault.

If observed behavior diverges from these expectations, it's either a
fixture-content drift (the vault was edited) or a real regression in the
daemon. Treat the table here as the contract.

## Inventory

Files **indexed** by `hmnd` (7 total):

| Path | Role |
|---|---|
| `README.md` | vault root description |
| `notes/databases/pgvector.md` | DB content; mentions pgvector, HNSW, sqlite-vec |
| `notes/databases/sqlite.md` | DB content; mentions SQLite, WAL, vector |
| `notes/design/watchers.md` | watcher behavior; debounce + content-hash gating |
| `notes/design/chunking.md` | multi-H2 file used to force ≥ 3 chunks |
| `notes/journal/2026-01-15.md` | filler |
| `notes/journal/2026-02-03.md` | filler |

Files **excluded** from the index:

| Path | Reason |
|---|---|
| `.obsidian/workspace.json` | not `.md`, plus dotfile-component filter and default `ignore_patterns` |
| `draft.sync-conflict-20260101.md` | matches `*.sync-conflict-*` ignore pattern |

## Filesystem search

| Query | `--prefix` | Expected result count | Expected paths |
|---|---|---|---|
| `**/*.md` | — | **7** | all indexed files above |
| `notes/databases/*.md` | — | **2** | `notes/databases/pgvector.md`, `notes/databases/sqlite.md` |
| `*.md` | `notes/journal` | **2** | `notes/journal/2026-01-15.md`, `notes/journal/2026-02-03.md` |
| `**/*.md` with `--limit 3` | — | **3 results, `truncated: true`** | first 3 by ascending path |
| `notes/nope/**` | — | **0** | — |

Result ordering is path-ascending and stable.

## Content search

Default mode: case-insensitive substring match.

| Query | `regex` | `case_sensitive` | Expected files (count) | Notes |
|---|---|---|---|---|
| `pgvector` | false | false | `notes/databases/pgvector.md` (1 file) | `match_count >= 2` (heading + body) |
| `NOTIFY` | false | false | `notes/design/watchers.md` (1 file) | matches lowercase `notify` and the `Notify` heading |
| `definitely-not-in-vault` | false | false | (0) | — |
| `^# .*` | true | true | every indexed `.md` | one match per file's H1 line |

The CLI doesn't expose `regex` or `case_sensitive` flags yet — for those
modes, hit `/search/content` directly with `curl` (see
[`../03-search.md`](../03-search.md)).

## Semantic search

Semantic ranking is approximate. The contract is **top-1 must match**;
top-3 should overlap with the listed files. Exact ordering of further
results depends on model and tie-breaking.

| Query | Expected top-1 file | Notes |
|---|---|---|
| `how do we prevent spurious reindexes` | `notes/design/watchers.md` | the vault's only chunk that talks about hash-gated reindex suppression |
| `vector similarity in sqlite` | `notes/databases/pgvector.md` *or* `notes/databases/sqlite.md` | both files discuss vector search in SQLite-shaped storage; either is acceptable as top-1 |
| `heading-aware document chunking` | `notes/design/chunking.md` | exact-topic chunk |

Each result carries `score`, `file_path`, `chunk_index`, `heading_path`
(array of headings), and `text`. `notes/design/chunking.md` should
produce **at least 3 chunks** (one per H2 section), each with a
`heading_path` array that includes the relevant H2 title.

## Outbox events

Watcher actions documented in
[`../02-watcher-and-outbox.md`](../02-watcher-and-outbox.md) operate on
files added to or removed from this vault during the test run, not on
the seven committed files.
