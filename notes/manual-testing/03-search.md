# 03 · Search

> Applies to: steps 5, 6, 7. Prereqs:
> [`01-running-the-daemon.md`](./01-running-the-daemon.md) complete;
> `hmnd` is running against the fixture vault. Semantic search
> additionally requires TEI per [`00-setup.md`](./00-setup.md) §4.

This doc runs every supported search mode against the unmodified
fixture vault and checks results against
[`fixtures/README.md`](./fixtures/README.md). If the watcher tests in
[`02-watcher-and-outbox.md`](./02-watcher-and-outbox.md) left scratch
files behind, clean those up before starting — the contract assumes
exactly the seven committed `.md` files.

`hmn` runs from terminal B; `hmnd` keeps running in terminal A.
JSON output via `hmn --json …` or `curl … | jq` makes counts easier
to verify.

---

## Filesystem search

Surface: `/search/filesystem` (POST) and `hmn search filesystem`. Glob
over vault paths; results sorted by ascending path.

### A. Match every Markdown file

```bash
hmn --json search filesystem '**/*.md' | jq '.results | length'
```

Expect **7**. Paths (in `.results[].path`):

- `README.md`
- `notes/databases/pgvector.md`
- `notes/databases/sqlite.md`
- `notes/design/chunking.md`
- `notes/design/watchers.md`
- `notes/journal/2026-01-15.md`
- `notes/journal/2026-02-03.md`

### B. Glob a subdirectory

```bash
hmn --json search filesystem 'notes/databases/*.md' | jq '.results[].path'
```

Expect exactly:

```
"notes/databases/pgvector.md"
"notes/databases/sqlite.md"
```

### C. Prefix filter

```bash
hmn --json search filesystem '*.md' --prefix notes/journal | jq '.results[].path'
```

Expect:

```
"notes/journal/2026-01-15.md"
"notes/journal/2026-02-03.md"
```

### D. Limit + truncation flag

```bash
hmn --json search filesystem '**/*.md' --limit 3
```

Expect `.results` of length 3 and `.truncated: true`.

### E. No-match glob

```bash
hmn --json search filesystem 'notes/nope/**' | jq '.results | length'
```

Expect **0**.

---

## Content search

Surface: `/search/content` (POST) and `hmn search content`. Default is
case-insensitive substring match. The CLI doesn't yet expose `regex` or
`case_sensitive` toggles — for those, hit the HTTP endpoint directly.

### A. Substring

```bash
hmn --json search content 'pgvector' | jq '.results[].path'
```

Expect exactly one path:

```
"notes/databases/pgvector.md"
```

`.results[0].match_count` should be at least 2 (heading line + body
mention).

### B. Case-insensitive default

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

### C. No matches

```bash
hmn --json search content 'definitely-not-in-vault' | jq '.results | length'
```

Expect **0**.

### D. Regex (HTTP)

```bash
curl -s -X POST http://127.0.0.1:7777/search/content \
  -H 'Content-Type: application/json' \
  -d '{"query":"^# .*","regex":true,"case_sensitive":true}' \
  | jq '.results[].path'
```

Expect every indexed `.md` file (one H1 line each). Result count: **7**.

### E. Case-sensitive (HTTP)

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

If the daemon was started with TEI **down**, the initial scan skipped
embedding for every file. Restart the daemon now (Ctrl+C terminal A,
`hmnd` again) so the scan re-embeds. The next startup should log a
non-zero `inserted` or `updated` count for the chunk path.

### A. Behavioral query → watchers.md

```bash
hmn --json search semantic 'how do we prevent spurious reindexes' \
  | jq '.results[0]'
```

Expect `.file_path == "notes/design/watchers.md"`. The chunk's
`heading_path` should include `"Content hash gating"`. Score above 0.5
indicates a healthy match.

### B. Conceptual query → DB files

```bash
hmn --json search semantic 'vector similarity in sqlite' \
  | jq '.results[0:3] | .[].file_path'
```

Expect the top result to be one of:

- `notes/databases/pgvector.md`
- `notes/databases/sqlite.md`

The other should appear within the top 3.

### C. Topic query → chunking.md

```bash
hmn --json search semantic 'heading-aware document chunking' \
  | jq '.results[0]'
```

Expect `.file_path == "notes/design/chunking.md"`.

### D. chunking.md produces multiple chunks

```bash
hmn --json search semantic 'chunking' --limit 20 \
  | jq '[.results[] | select(.file_path == "notes/design/chunking.md")] | length'
```

Expect **at least 3** — `chunking.md` has three H2 sections, each
producing a chunk; the H1 intro may produce a fourth.

### E. Edge: TEI down

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
`code: "embedding_unavailable"`. The daemon stays up — `/health` still
returns 200, filesystem and content search still work.

Restart TEI; semantic search should recover without restarting `hmnd`:

```bash
hmn search semantic 'how do we prevent spurious reindexes'
```

Should succeed again with the same top-1 result as in §A.

---

## Wrapping up

```bash
ls -lh ~/.local/share/hypomnema/
```

Expect:

- `index.sqlite` (+ `-wal`, `-shm`) — the metadata + chunks + vec index
- `outbox.jsonl` — bigger than zero if you ran `02`
- `sqlite-vec.<ext>` — the extension you installed in `00`

In terminal A, Ctrl+C `hmnd`. Clean exit, code 0.

If everything in this doc lined up with
[`fixtures/README.md`](./fixtures/README.md), the v0 surface through
step 7 is healthy. Drift on any specific check points at either fixture
content drift or a real regression — investigate.
