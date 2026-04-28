# Sample fixture vaults — expected results

The runbook ships two committed vaults:

- [`sample-vault/`](./sample-vault/) — the original databases-and-design
  vault from steps 1–8. Single-vault flows in the runbook target this
  one.
- [`sample-vault-2/`](./sample-vault-2/) — a second vault on a
  deliberately disjoint topic (cooking and kitchen technique) added in
  round 4 to exercise multi-vault behavior end-to-end (cross-vault
  search, `--vaults` filtering, partial-results diagnostics, vault-
  management operations).

Both are engineered for deterministic search outcomes. This file is the
expected-results contract for every example query; the runbook's
[`../03-search.md`](../03-search.md), [`../04-mcp.md`](../04-mcp.md),
[`../05-vault-management.md`](../05-vault-management.md), and
[`../06-mcp-http.md`](../06-mcp-http.md) point back here for ground
truth.

If observed behavior diverges from these expectations, it is either
fixture-content drift (a vault was edited) or a real regression in the
daemon. Treat the tables here as the contract.

---

## Vault A — `sample-vault/`

### Inventory

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

### Filesystem search (vault A only)

Run as: `hmn search filesystem '<query>' --vaults sample`.

| Query | `--prefix` | Expected result count | Expected paths |
|---|---|---|---|
| `**/*.md` | — | **7** | all indexed files above |
| `notes/databases/*.md` | — | **2** | `notes/databases/pgvector.md`, `notes/databases/sqlite.md` |
| `*.md` | `notes/journal` | **2** | `notes/journal/2026-01-15.md`, `notes/journal/2026-02-03.md` |
| `**/*.md` with `--limit 3` | — | **3 results, `truncated: true`** | first 3 by ascending path |
| `notes/nope/**` | — | **0** | — |

Result ordering is path-ascending and stable.

### Content search (vault A only)

Default mode: case-insensitive substring match.

| Query | `regex` | `case_sensitive` | Expected files (count) | Notes |
|---|---|---|---|---|
| `pgvector` | false | false | `notes/databases/pgvector.md` (1 file) | `match_count >= 2` (heading + body) |
| `NOTIFY` | false | false | `notes/design/watchers.md` (1 file) | matches lowercase `notify` and the `Notify` heading |
| `definitely-not-in-vault` | false | false | (0) | — |
| `^# .*` | true | true | every indexed `.md` | one match per file's H1 line |

The CLI doesn't expose `regex` or `case_sensitive` flags yet — for
those modes, hit `/search/content` directly with `curl` (see
[`../03-search.md`](../03-search.md)).

### Semantic search (vault A only)

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

---

## Vault B — `sample-vault-2/`

### Inventory

Files **indexed** by `hmnd` (10 total):

| Path | Role |
|---|---|
| `README.md` | vault root description |
| `recipes/bread.md` | yeast-leavened white loaf; mentions yeast, gluten, autolyse |
| `recipes/sourdough.md` | starter-leavened country loaf; mentions sourdough, fermentation |
| `recipes/pasta-dough.md` | egg pasta dough; mentions gluten, kneading |
| `techniques/knife-skills.md` | dicing and slicing technique |
| `techniques/braising.md` | moist-heat technique for tough cuts |
| `techniques/fermentation.md` | YAML-frontmatter file on lacto-fermentation |
| `ingredients/sourdough-starter.md` | wild-yeast starter maintenance |
| `ingredients/olive-oil.md` | olive oil grades and storage |
| `journal/2026-03-22.md` | filler |

Files **excluded** from the index:

| Path | Reason |
|---|---|
| `draft.sync-conflict-20260201.md` | matches `*.sync-conflict-*` ignore pattern |

`techniques/fermentation.md` carries a five-key YAML frontmatter
block (`title`, `tags`, `created`, `updated`, `difficulty`) used to
exercise the chunker's frontmatter strip — the body is what gets
indexed; the frontmatter does not.

### Filesystem search (vault B only)

Run as: `hmn search filesystem '<query>' --vaults sample-2`.

| Query | `--prefix` | Expected result count | Expected paths |
|---|---|---|---|
| `**/*.md` | — | **10** | all indexed files above |
| `recipes/*.md` | — | **3** | `recipes/bread.md`, `recipes/pasta-dough.md`, `recipes/sourdough.md` |
| `*.md` | `techniques` | **3** | `techniques/braising.md`, `techniques/fermentation.md`, `techniques/knife-skills.md` |
| `**/*.md` with `--limit 4` | — | **4 results, `truncated: true`** | first 4 by ascending path |
| `recipes/cocktail/**` | — | **0** | — |

### Content search (vault B only)

| Query | `regex` | `case_sensitive` | Expected files (count) | Notes |
|---|---|---|---|---|
| `sourdough` | false | false | `recipes/bread.md`, `recipes/sourdough.md`, `ingredients/sourdough-starter.md` (3 files) | `recipes/sourdough.md` has the highest `match_count` |
| `Lactobacillus` | false | false | `ingredients/sourdough-starter.md`, `techniques/fermentation.md` (2 files) | case-insensitive default catches both |
| `LACTOBACILLUS` | false | true | (0) | no file contains the literal uppercase string |
| `definitely-not-in-vault` | false | false | (0) | — |

### Semantic search (vault B only)

| Query | Expected top-1 file | Notes |
|---|---|---|
| `wild yeast culture maintenance` | `ingredients/sourdough-starter.md` | exact-topic chunk |
| `developing gluten in dough` | `recipes/bread.md` *or* `recipes/pasta-dough.md` | both discuss gluten development; either is acceptable as top-1 |
| `slicing food evenly with a kitchen knife` | `techniques/knife-skills.md` | exact-topic chunk |

`techniques/fermentation.md` should produce **at least 3 chunks** (one
per H2 section: lacto-fermentation, temperature, signs of a healthy
ferment); the H1 intro may produce a fourth.

---

## Cross-vault search (both vaults)

Run without `--vaults` (or pass both names: `--vaults sample,sample-2`).

| Search mode | Query | Expected behavior |
|---|---|---|
| filesystem | `**/*.md` | **17** total results (7 from `sample`, 10 from `sample-2`) — each result carries a `vault` field (the surrogate UUID) and a `vault_name` field (`"sample"` or `"sample-2"`) |
| filesystem | `notes/databases/*.md` | **2** results, all from vault `sample` (vault `sample-2` has no `notes/databases/` directory) |
| content | `pgvector` | **1** result, from vault `sample` |
| content | `sourdough` | **3** results, all from vault `sample-2` |
| semantic | `vector similarity in sqlite` | top-1 from vault `sample` |
| semantic | `wild yeast culture maintenance` | top-1 from vault `sample-2` |

### Partial-results diagnostics

When a vault is `paused` or `errored`, search responses include a
`partial_results.skipped` array listing the excluded vault's `vault`
(UUID), `vault_name`, `status`, and `reason`; the search proceeds
across the remaining active vaults. When `--vaults` includes a name
that doesn't resolve, the unknown entry lands in
`partial_results.failed` with `code: "vault_not_found"` and the
search continues against the recognized subset. See
[`../03-search.md`](../03-search.md) §Cross-vault search and
[`docs/specs/vault-management.md` § Cross-Vault Search Semantics](../../../docs/specs/vault-management.md#cross-vault-search-semantics).

## Outbox events

Watcher actions documented in
[`../02-watcher-and-outbox.md`](../02-watcher-and-outbox.md) operate
on files added to or removed from these vaults during the test run,
not on the seven + ten committed files. Each vault has its own outbox
file at `<data_dir>/vaults/<vault_id>/outbox.jsonl`.
