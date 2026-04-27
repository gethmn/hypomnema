# 01 · Running the daemon

> Applies to: steps 1, 2, 5. Prereqs: [`00-setup.md`](./00-setup.md)
> complete; config validated; sqlite-vec on disk.

This doc gets `hmnd` running against the fixture vault, verifies the
HTTP surface comes up, and confirms `hmn status` agrees with what you
see in the daemon logs. It does **not** exercise file changes (that's
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

- config summary — vault path, HTTP bind, data dir
- vault canonicalized to its absolute path
- store opened; sqlite-vec extension loaded
- migrations applied (first run only)
- embedding health probe — `INFO` if TEI is up, `WARN` if down (the
  daemon proceeds either way; see
  [`00-setup.md`](./00-setup.md) §4)
- initial scan summary — something like
  `inserted=7 updated=0 hash_unchanged=0 deleted=0 duration=…ms`
- HTTP server bound to `127.0.0.1:7777`

If TEI is down at startup, the `WARN` line names the configured
endpoint and model. The daemon stays up; per-file embed failures during
the initial scan are logged at `ERROR` and the chunk rows for those
files are skipped.

## 2. /health

In terminal B:

```bash
curl -s http://127.0.0.1:7777/health
```

Expect:

```json
{"status":"ok"}
```

## 3. hmn status

```bash
hmn status
```

Expect a human-readable summary that includes:

- vault path (canonical, matches what the daemon logged)
- indexed file count: **7** for the unmodified fixture vault
- last indexed time: ISO-8601 timestamp from the just-run scan
- outbox file size: bytes (typically 0 on first boot — see
  [`02-watcher-and-outbox.md`](./02-watcher-and-outbox.md))

For the JSON shape:

```bash
hmn --json status | jq
```

## 4. /status

The same data over HTTP:

```bash
curl -s http://127.0.0.1:7777/status | jq
```

Should match `hmn --json status`.

## 5. Clean shutdown

In terminal A, Ctrl+C. Expect:

- a "shutdown" or "stopping" log line
- a final clean-exit log
- exit code 0 (run with `echo $?` after to confirm)

## 6. Idempotent second run

Start `hmnd` again. The initial scan should now report something like
`inserted=0 updated=0 hash_unchanged=7 deleted=0` — every file's
content hash matches the stored row, no work to do.

## 7. Common gotchas

- **`hmn` exits with code 4** — daemon unreachable. Check that `hmnd`
  is still running and that `[http].bind` matches the URL `hmn` is
  using. Override with `--daemon-url`:
  ```bash
  hmn --daemon-url http://127.0.0.1:7777 status
  ```
  Or set `HYPOMNEMA_DAEMON_URL`.
- **`hmn` exits with code 3** — config error reading client-side
  settings. Confirm the same `--config` path the daemon used.
- **Verbose output**: `hmn -v status`, `hmn -vv status`, etc.
- **JSON mode**: `hmn --json <subcommand>` for machine-readable output.

You're ready for [`02-watcher-and-outbox.md`](./02-watcher-and-outbox.md).
