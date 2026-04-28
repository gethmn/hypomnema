# 03 · Search

> Applies to: round 4 / step 12 (cross-vault search by default; `--vaults`
> filter; partial-results diagnostics). Prereqs:
> [`01-running-the-daemon.md`](./01-running-the-daemon.md) complete;
> `hmnd` is running against both fixture vaults. Semantic search
> additionally requires TEI per [`00-setup.md`](./00-setup.md) §4.

This doc runs every supported search mode against the unmodified
fixture vaults and checks results against
[`fixtures/README.md`](./fixtures/README.md). If the watcher tests in
[`02-watcher-and-outbox.md`](./02-watcher-and-outbox.md) left scratch
files behind, clean those up before starting — the contract assumes
exactly the **7 + 10 = 17** committed `.md` files across the two
vaults.

`hmn` runs from terminal B; `hmnd` keeps running in terminal A.
JSON output via `hmn --json …` or `curl … | jq` makes counts easier
to verify.

## Cross-vault by default

Every search mode fans out across all currently-`active` vaults by
default. Each result row carries a `vault` field (the surrogate UUID)
and a `vault_name` field (the registry's mutable name). Use
`--vaults` to narrow scope:

- `--vaults sample` — restrict to vault A only.
- `--vaults sample-2` — restrict to vault B only.
- `--vaults sample,sample-2` — explicit both (equivalent to omitting
  `--vaults` when both are active).
- `--vaults` may be repeated: `--vaults sample --vaults sample-2`
  works too.
- `--vaults ""` (empty list) is rejected as `invalid_request`.

Names match first; surrogate IDs (full UUID strings) match second.
Unknown entries do not fail the request — they appear in
`partial_results.failed[]` with `code: "vault_not_found"` and the
search proceeds against the recognized subset. See § Partial-results
diagnostics below.

---

## Filesystem search

Surface: `/search/filesystem` (POST) and `hmn search filesystem`. Glob
over vault paths; results sorted by ascending path within each vault,
with cross-vault results interleaved by path.

### A. Match every Markdown file across both vaults

```bash
hmn --json search filesystem '**/*.md' | jq '.results | length'
```

Expect **17** (7 from `sample` + 10 from `sample-2`).

```bash
hmn --json search filesystem '**/*.md' \
  | jq '[.results[] | .vault_name] | group_by(.) | map({vault: .[0], count: length})'
```

Expect:

```json
[
  {"vault": "sample",   "count": 7},
  {"vault": "sample-2", "count": 10}
]
```

### B. Restrict to vault A

```bash
hmn --json search filesystem '**/*.md' --vaults sample | jq '.results | length'
```

Expect **7**. Paths match the inventory in
[`fixtures/README.md`](./fixtures/README.md) § Vault A.

### C. Restrict to vault B

```bash
hmn --json search filesystem '**/*.md' --vaults sample-2 | jq '.results | length'
```

Expect **10**. Paths match
[`fixtures/README.md`](./fixtures/README.md) § Vault B.

### D. Glob a subdirectory (vault A)

```bash
hmn --json search filesystem 'notes/databases/*.md' \
  | jq '.results[].path'
```

Expect exactly:

```
"notes/databases/pgvector.md"
"notes/databases/sqlite.md"
```

Both rows carry `vault_name: "sample"`. Vault B has no
`notes/databases/` subdirectory, so it contributes zero results
without any error or partial-results entry.

### E. Glob a subdirectory (vault B)

```bash
hmn --json search filesystem 'recipes/*.md' \
  | jq '.results[].path'
```

Expect:

```
"recipes/bread.md"
"recipes/pasta-dough.md"
"recipes/sourdough.md"
```

All three rows carry `vault_name: "sample-2"`.

### F. Prefix filter

```bash
hmn --json search filesystem '*.md' --prefix notes/journal --vaults sample \
  | jq '.results[].path'
```

Expect:

```
"notes/journal/2026-01-15.md"
"notes/journal/2026-02-03.md"
```

### G. Limit + truncation flag

```bash
hmn --json search filesystem '**/*.md' --limit 5
```

Expect `.results` of length 5 and `.truncated: true`. Results respect
the cross-vault interleaving order; the truncation is computed
post-merge.

### H. No-match glob

```bash
hmn --json search filesystem 'recipes/cocktail/**' \
  | jq '.results | length'
```

Expect **0**.

---

## Content search

Surface: `/search/content` (POST) and `hmn search content`. Default is
case-insensitive substring match. The CLI doesn't yet expose `regex`
or `case_sensitive` toggles — for those, hit the HTTP endpoint
directly.

### A. Substring (vault A only by content)

```bash
hmn --json search content 'pgvector' | jq '.results[] | {vault_name, path, match_count}'
```

Expect a single result:

```json
{"vault_name": "sample", "path": "notes/databases/pgvector.md", "match_count": 2}
```

`match_count` is at least 2 (heading line + body mention).

### B. Substring matches in vault B

```bash
hmn --json search content 'sourdough' \
  | jq '.results[] | {vault_name, path}'
```

Expect three results, all from vault `sample-2`:

```json
{"vault_name": "sample-2", "path": "ingredients/sourdough-starter.md"}
{"vault_name": "sample-2", "path": "recipes/bread.md"}
{"vault_name": "sample-2", "path": "recipes/sourdough.md"}
```

(Order may vary; the contract is the file set.)

### C. Case-insensitive default (vault A)

```bash
hmn --json search content 'NOTIFY' | jq '.results[].path'
```

Expect exactly:

```
"notes/design/watchers.md"
```

The file mentions `notify-debouncer-full`, the `notify` crate, and a
`Notify backend` heading; the case-insensitive default catches all of
them.

### D. No matches

```bash
hmn --json search content 'definitely-not-in-vault' | jq '.results | length'
```

Expect **0**.

### E. Regex (HTTP)

```bash
curl -s -X POST http://127.0.0.1:7777/search/content \
  -H 'Content-Type: application/json' \
  -d '{"query":"^# .*","regex":true,"case_sensitive":true}' \
  | jq '.results | length'
```

Expect **17** — every indexed `.md` file across both vaults has at
least one H1 line.

### F. Case-sensitive (HTTP)

```bash
curl -s -X POST http://127.0.0.1:7777/search/content \
  -H 'Content-Type: application/json' \
  -d '{"query":"NOTIFY","regex":false,"case_sensitive":true}' \
  | jq '.results | length'
```

Expect **0** — no file contains the literal uppercase string `NOTIFY`.

---

## Semantic search

Surface: `/search/semantic` (POST) and `hmn search semantic`. Embeds
the query through TEI, runs nearest-neighbor over `chunks_vec`, joins
to `chunks` for path / heading / text. Cosine similarity, score in
`[0, 1]`.

**Approximate by nature.** The contract is "top-1 must match"; top-3
should overlap with the listed files. Exact ordering of further
results varies with model and tie-breaking.

If the daemon was started with TEI **down**, the initial scan of
either vault skipped embedding for every file. Restart the daemon now
(Ctrl+C terminal A, `hmnd` again) so the scans re-embed; the next
startup should log non-zero `inserted` or `updated` counts on the
chunk path for each vault.

### A. Vault-A query → watchers.md

```bash
hmn --json search semantic 'how do we prevent spurious reindexes' \
  | jq '.results[0]'
```

Expect `.file_path == "notes/design/watchers.md"`, `.vault_name ==
"sample"`. The chunk's `heading_path` should include `"Content hash
gating"`. Score above 0.5 indicates a healthy match.

### B. Vault-B query → sourdough-starter.md

```bash
hmn --json search semantic 'wild yeast culture maintenance' \
  | jq '.results[0]'
```

Expect `.file_path == "ingredients/sourdough-starter.md"`,
`.vault_name == "sample-2"`.

### C. Cross-vault topical query

```bash
hmn --json search semantic 'heading-aware document chunking' \
  | jq '.results[0]'
```

Expect `.file_path == "notes/design/chunking.md"`, `.vault_name ==
"sample"`. The `chunking.md` file is unique to vault A; vault B has
no comparable chunk, so this query lands cleanly on vault A.

### D. chunking.md produces multiple chunks (vault A)

```bash
hmn --json search semantic 'chunking' --limit 20 \
  | jq '[.results[] | select(.file_path == "notes/design/chunking.md")] | length'
```

Expect **at least 3** — `chunking.md` has three H2 sections, each
producing a chunk; the H1 intro may produce a fourth.

### E. fermentation.md frontmatter strip (vault B)

```bash
hmn --json search semantic 'salt-tolerant lactobacillus brine' \
  | jq '.results[0] | {file_path, vault_name, text}'
```

Expect `.file_path == "techniques/fermentation.md"`, `.vault_name ==
"sample-2"`. The result `text` should describe the lacto-fermentation
chemistry — **not** the YAML frontmatter (`title:` / `tags:` / etc.),
which the chunker strips before chunking. If you see frontmatter
content in the result, the strip step regressed.

### F. Restrict semantic search by vault

```bash
hmn --json search semantic 'pasta' --vaults sample-2 \
  | jq '.results[0].vault_name'
```

Expect `"sample-2"`. With `--vaults sample` instead, the same query
returns top-N results from vault A only (lower scores; no exact match).

### G. Edge: TEI down

Stop the TEI container (Ctrl+C its terminal, or
`docker stop <container>`). Then:

```bash
hmn search semantic 'anything' ; echo "exit=$?"
```

Or by HTTP for the exact code:

```bash
curl -s -o /dev/null -w '%{http_code}\n' \
  -X POST http://127.0.0.1:7777/search/semantic \
  -H 'Content-Type: application/json' \
  -d '{"query":"anything"}'
```

Expect HTTP **503** with an error body containing
`code: "embedding_unavailable"`. The daemon stays up — `/health`
still returns 200, filesystem and content search still work.

Restart TEI; semantic search should recover without restarting `hmnd`:

```bash
hmn search semantic 'how do we prevent spurious reindexes'
```

Should succeed again with the same top-1 result as in §A.

---

## Partial-results diagnostics

When a vault is `paused` or `errored`, search responses include a
`partial_results.skipped[]` array listing the excluded vault's
`vault` (UUID), `vault_name`, `status`, and `reason`; the search
proceeds across the remaining active vaults. When `--vaults` includes
a name that doesn't resolve, the unknown entry lands in
`partial_results.failed[]` with `code: "vault_not_found"` and the
search continues against the recognized subset.

### A. Pause a vault and observe `skipped[]`

```bash
hmn vault pause sample-2
hmn --json search filesystem '**/*.md' | jq '{count: (.results|length), skipped: .partial_results.skipped}'
```

Expect:

```json
{
  "count": 7,
  "skipped": [
    {
      "vault": "<sample-2 surrogate UUID>",
      "vault_name": "sample-2",
      "status": "paused",
      "reason": "vault paused"
    }
  ]
}
```

`partial_results.failed` is omitted when there were no unknown vault
selectors. Resume:

```bash
hmn vault resume sample-2
```

Subsequent searches against both vaults emit no `partial_results`
field at all (it's serialized only when non-empty).

### B. Unknown vault selector → `failed[]`

```bash
hmn --json search filesystem '**/*.md' --vaults sample,nonesuch \
  | jq '{count: (.results|length), failed: .partial_results.failed}'
```

Expect:

```json
{
  "count": 7,
  "failed": [
    {
      "vault": "nonesuch",
      "vault_name": "nonesuch",
      "code": "vault_not_found",
      "message": "<some hint or empty>"
    }
  ]
}
```

The recognized half (`sample`) returns its 7 results; the unknown
half is reported in `failed[]` rather than failing the whole request.

---

## Wrapping up

```bash
ls -lh ~/.local/share/hypomnema/
ls -lh ~/.local/share/hypomnema/vaults/
```

Expect:

- `vaults.sqlite` (+ `-wal`, `-shm`) — the registry.
- One subdirectory per registered vault, each containing
  `index.sqlite` (+ `-wal`, `-shm`), `outbox.jsonl`, and `meta.toml`.
- `sqlite-vec.<ext>` — the extension you installed in `00`.

In terminal A, Ctrl+C `hmnd`. Clean exit, code 0.

If everything in this doc lined up with
[`fixtures/README.md`](./fixtures/README.md), the multi-vault search
surface through round 4 is healthy. Drift on any specific check
points at either fixture content drift or a real regression —
investigate.

You're ready for [`04-mcp.md`](./04-mcp.md) (MCP-over-stdio) and
[`06-mcp-http.md`](./06-mcp-http.md) (the round-4 HTTP-MCP transport).
