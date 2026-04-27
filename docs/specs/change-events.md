# Change Events (Outbox) Specification

**Version**: 0.1.1
**Date**: 2026-04-26
**Status**: Draft

---

## Overview

Hypomnema emits a durable stream of change events so consumers can react to vault changes without polling the search API. Events are written as JSON lines to an append-only outbox file in the daemon's data directory. Consumers subscribe by tailing the file.

**Related Documents**:
- [ADR-0006: Outbox Lives Outside the Watched Directory](../decisions/0006-outbox-outside-watched-directory.md)
- [Architecture: Outbox Writer](../architecture/overview.md#outbox-writer)

---

## Behavior

### Normal Flow

1. Watcher observes a filesystem event (create / modify / delete) under the watched vault
2. Debouncer coalesces the event storm around a single logical save
3. Indexer computes the new content hash and compares against the stored hash
4. *Only if the hash changed* does the indexer emit an event to the outbox
5. The outbox writer appends one JSON line and fsyncs per event (resolved in step 4 as per-event `sync_data`; see line 97).

This content-hash gate is the primary defense against editor-save noise and sync-tool mtime churn. An agent tailing the outbox sees only real changes, not every save-triggered filesystem event.

### Cold Start

The outbox records *changes detected after the watcher is running*. The initial indexing walk performed when the daemon starts (or when a vault is first created) is silent — it populates the index but does **not** emit outbox events for files it discovers.

A consumer that subscribes to a freshly-created vault and tails from byte 0 will see no events until a real change occurs (file edit, create, delete observed by the watcher).

To obtain initial state, consumers should query the search / index API once at subscription time, then begin tailing the outbox for subsequent deltas. The two surfaces are complementary: the index answers "what exists now?"; the outbox answers "what changed since I last looked?".

### Event Envelope (minimum)

```json
{
  "event_type": "modified",
  "path": "notes/databases/pgvector.md",
  "content_hash": "sha256:abc123…",
  "detected_at": "2026-04-23T14:22:08.123456Z"
}
```

Event types: `created`, `modified`, `deleted`. v0 behavior, confirmed in step 3: renames are observed as a `deleted` + `created` pair. Fused rename detection remains open (line 98).

### Consumer Subscription

Consumers tail the outbox file. On startup a consumer may:
- Start from end-of-file (only see new changes)
- Replay from a byte offset it persists across restarts
- Replay from a specific `detected_at` timestamp by filtering during read

Hypomnema offers no push, no webhook, no in-process callback in v0. See the handoff's "Out of scope" for deferred fan-out work.

---

## Consumer Model

The outbox is the change-notification surface for **external consumers** — applications and AI agents that subscribe to vault changes by tailing `outbox.jsonl`. Examples: Iris, Claude Code, custom scripts.

The Hypomnema binaries themselves do not consume the event stream:
- `hmnd` writes events but never tails them.
- `hmn` displays outbox metadata (path, size) via the `/status` API but does not parse events.

This separation is intentional: per [vision.md §Consumer](../product/vision.md#consumer), "Hypomnema has no awareness of its consumers." The daemon's job ends at producing a durable, ordered event log; reacting to changes is the consumer's job.

**Tail contract**: consumers open the file, read line-by-line, and persist the byte offset they have processed. On restart they reopen and seek to the persisted offset. Lines are JSON-per-line and complete-line-terminated; consumers should treat a partial trailing line (no terminating `\n`) as not-yet-committed and re-read it on next poll.

---

## Data Schema

| Field | Type | Required | Notes |
|-------|------|----------|-------|
| `event_type` | string | yes | One of `created`, `modified`, `deleted` |
| `path` | string | yes | Vault-relative path |
| `content_hash` | string | yes when known (always for create/modify; for delete, the last known hash from the index) | `sha256:` hex of file content |
| `detected_at` | ISO-8601 string (µs precision, UTC) | yes | When Hypomnema confirmed the change |
| `vault` | string | no | Reserved; always absent in v0. Will carry the source vault identifier when multi-vault ships. Consumers must accept its absence and tolerate any string value. |

When the daemon has no prior record for a deleted path — a rare race where the watcher reports a delete on a path that was never indexed — the outbox emits no event for it; the schema therefore never expresses delete-without-hash in practice.

Future fields (additive, optional) will be added as the daemon learns to notice more. Consumers should ignore unknown fields.

### File Format

- One JSON object per line (JSONL)
- UTF-8
- Location: `~/.local/share/hypomnema/outbox.jsonl` on Linux and macOS; `%APPDATA%\hypomnema\outbox.jsonl` on Windows (see [reference/configuration.md](../reference/configuration.md) for XDG/env overrides)
- Never rotated by Hypomnema in v0 (rotation is an open question)

### Size & Growth

Each event serializes to ~130–150 bytes typical (path-length dependent; SHA256 hash adds ~50 bytes when present). Rough envelope:

| Events | File size |
|--------|-----------|
| 1,000 | ~150 KB |
| 100,000 | ~15 MB |
| 1,000,000 | ~150 MB |

v0 never rotates (see Open Questions). Operators on long-running vaults with high churn should plan for unbounded growth or perform manual archival (see [Edge Cases: Intentional reset](#intentional-reset-start-over-with-a-fresh-outbox)).

---

## Edge Cases

### Large write followed by immediate delete

Debouncer coalesces; final state wins. If a file is written then deleted within the debounce window, only a `deleted` event is emitted — there was no stable state to index.

### Content hash collision

sha256 collisions are considered impossible for this purpose. No mitigation.

### Outbox file removed or truncated externally

Daemon recreates/opens the file on next event. Consumers that were tailing an old inode will stop seeing events — the correct recovery is consumer-side (reopen on `ENOENT` or detect inode change).

### Crash during write

The write is small (one line). In the worst case a consumer sees a truncated JSON line and must skip it. The daemon on restart picks up from the end-of-file; no duplicate is emitted for events that made it through before the crash.

### Outbox file corrupted (partial trailing line)

Per the [Consumer Model tail contract](#consumer-model), consumers should treat a partial trailing line as not-yet-committed and re-read on next poll. A line is committed when terminated with `\n`. The daemon writes one event per `writeln!` followed by `fdatasync`, so a partial line on disk indicates an in-flight write or a crash mid-write — not corruption to escalate.

### Intentional reset (start over with a fresh outbox)

Operators may need to discard outbox history (e.g., file grew unmanageably; consumer state is unrecoverable). Procedure:

1. Stop `hmnd`.
2. Delete or move `outbox.jsonl`.
3. Restart `hmnd`. The daemon recreates the file empty on next event (or on first append).
4. Reset any consumer-side persisted offsets to 0.

This is safe because the outbox is **not authoritative for vault state** — the index (`index.sqlite`) is. Resetting the outbox loses notification history but does not affect indexed state. A consumer that needs to re-bootstrap after a reset should re-query the index (per [Cold Start](#cold-start)).

---

## Open Questions

- [x] Exact fsync policy: per-event vs periodic? Resolved in step 4 as per-event `sync_data`. See [step-4 workplan § Deferred decision 1](../roadmap/step-04-workplan.md#1-fsync-policy-per-event-vs-periodic).
- [ ] Rename detection: should `renamed` be a distinct event type?
- [ ] Outbox rotation / retention — should Hypomnema rotate after N MB or N days, or leave this to the user?
- [ ] Should a consumer be able to ask Hypomnema for the current outbox byte offset, so it can checkpoint without inspecting the file directly?

---

## Revision History

| Version | Date | Changes |
|---------|------|---------|
| 0.1.0 | 2026-04-23 | Initial draft, seeded from project handoff v0 scope |
| 0.1.1 | 2026-04-26 | Clarify consumer model (external-only), cold-start semantics, size envelope, and recovery procedures. No behavior change. |
