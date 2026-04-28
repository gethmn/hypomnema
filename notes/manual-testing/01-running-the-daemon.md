# 01 · Running the daemon

> Applies to: round 4 / step 12 (multi-vault registry).
> Prereqs: [`00-setup.md`](./00-setup.md) complete; config validated;
> sqlite-vec on disk; both fixture vaults registered via
> `hmn vault create`.

This doc verifies that `hmnd` comes up cleanly against a multi-vault
registry, that the HTTP surface answers, and that the per-vault
breakdown reported by `hmn vault list` agrees with what the daemon
logged at startup. It does **not** exercise file changes (that's
[`02-watcher-and-outbox.md`](./02-watcher-and-outbox.md)) or search
(that's [`03-search.md`](./03-search.md)).

Run each command in a separate terminal where useful: terminal A for
`hmnd`, terminal B for `hmn` and `curl`.

## 1. Start the daemon

In terminal A:

```bash
hmnd
```

(Or `cargo run --release --bin hmnd` if you didn't put `target/release/`
on `PATH`.)

Expected log lines, in order (timestamps elided):

- `hmnd: starting daemon` — config summary (data dir, HTTP bind, debounce, pid).
- `vault_registry: opened …` — `vaults.sqlite` opens (created on first
  run if absent).
- per-vault startup sequence for each registered vault: store opened,
  sqlite-vec extension loaded, migrations applied (first run only),
  initial scan summary (`inserted=N updated=0 hash_unchanged=0
  deleted=0 duration=…ms`).
- embedding health probe — `INFO` if TEI is up, `WARN` if down (the
  daemon proceeds either way; see [`00-setup.md`](./00-setup.md) §4).
- `hmnd: mcp http transport mounted path=/mcp enabled=true` — the
  round-4 HTTP-MCP route is mounted (default-on per
  [`06-mcp-http.md`](./06-mcp-http.md)).
- `hmnd: http server listening bind=127.0.0.1:7777 vault_count=2` —
  Axum is live; **`vault_count` is the count of `active` vaults from
  the registry** at startup.

Per-vault initial-scan totals on the runbook's fixtures: **7** files
for `sample`, **10** for `sample-2`.

If TEI is down at startup, the `WARN` line names the configured
endpoint and model. The daemon stays up; per-file embed failures
during the initial scans are logged at `ERROR` and the chunk rows for
those files are skipped.

## 2. /health

In terminal B:

```bash
curl -s http://127.0.0.1:7777/health
```

Expect:

```json
{"status":"ok"}
```

`/health` is a liveness probe — it returns 200 OK as long as the HTTP
server is responding. It does not enumerate vaults; for the per-vault
view see `hmn vault list` (§5 below).

## 3. hmn status

```bash
hmn status
```

`hmn status` reports a daemon-wide summary built from one
representative vault and a cross-vault total. With both fixture vaults
active, expect:

- `vault:` — the **first registered vault's** canonicalized path
  (typically `…/sample-vault`). This is a representative line, not a
  per-vault breakdown.
- `indexed files:` — the **sum** across all active vaults: **17** for
  the unmodified runbook fixtures (7 + 10).
- `last indexed:` — the latest `indexed_at` timestamp seen across all
  active vaults.
- `outbox:` — the **first registered vault's** outbox path and current
  size.

For the JSON shape:

```bash
hmn --json status | jq
```

Same fields, machine-readable. The single-representative shape is
backwards-compatible with the v0.1.0 single-vault `/status` body —
clients that already consume it keep working without changes. The
per-vault view is documented in
[`05-vault-management.md`](./05-vault-management.md) and surfaced via
`hmn vault list`.

## 4. /status

The same data over HTTP:

```bash
curl -s http://127.0.0.1:7777/status | jq
```

Should match `hmn --json status`.

## 5. hmn vault list — per-vault view

For the multi-vault breakdown that `hmn status` does not provide,
read the registry directly:

```bash
hmn vault list
```

Expect a table with one row per registered vault:

```
ID                                    NAME      STATUS    CREATED                       PATH
019dd258-…  sample    active    2026-04-28T…              <ABS>/…/fixtures/sample-vault
019dd258-…  sample-2  active    2026-04-28T…              <ABS>/…/fixtures/sample-vault-2
```

Or as JSON:

```bash
hmn --json vault list | jq '.vaults | length'
```

Expect **2**.

`hmn vault status sample` and `hmn vault status sample-2` print the
single-vault detail block (id, name, path, status, created_at,
optional last_error) for one vault at a time. With no selector, the
command resolves to `default_vault_name` (`sample` per
[`00-setup.md`](./00-setup.md) §5).

## 6. Clean shutdown

In terminal A, Ctrl+C. Expect:

- a `hmnd: drain complete, exiting cleanly` log line.
- per-vault watcher and indexer drain messages on the way out.
- exit code 0 (run with `echo $?` after to confirm).

The daemon installs a SIGINT/SIGTERM handler; the same shutdown path
applies whether you Ctrl+C interactively or send `kill -TERM <pid>`.

## 7. Idempotent second run

Start `hmnd` again. The per-vault initial-scan summaries should now
report something like `inserted=0 updated=0 hash_unchanged=N
deleted=0` for each vault — every file's content hash matches the
stored row, no work to do.

## 8. Common gotchas

- **`hmn` exits with code 4** — daemon unreachable. Check that `hmnd`
  is still running and that `[http].bind` matches the URL `hmn` is
  using. Override with `--daemon-url`:
  ```bash
  hmn --daemon-url http://127.0.0.1:7777 status
  ```
  Or set `HYPOMNEMA_DAEMON_URL`.
- **`hmn` exits with code 3** — config error reading client-side
  settings. Confirm the same `--config` path the daemon used.
- **`hmn vault status` exits with code 5** — vault not found. Run
  `hmn vault list` to confirm the registry contents; the daemon's
  404 response carries a closest-name hint when the registry has a
  near match (Levenshtein distance ≤ 3).
- **Verbose output**: `hmn -v status`, `hmn -vv status`, etc.
- **JSON mode**: `hmn --json <subcommand>` for machine-readable output.

You're ready for [`02-watcher-and-outbox.md`](./02-watcher-and-outbox.md).
