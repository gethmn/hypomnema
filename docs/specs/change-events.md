# Change Events (Outbox) Specification

**Version**: 0.1.0
**Date**: 2026-04-23
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
5. The outbox writer appends one JSON line and fsyncs (TBD: fsync-every vs fsync-periodic)

This content-hash gate is the primary defense against editor-save noise and sync-tool mtime churn. An agent tailing the outbox sees only real changes, not every save-triggered filesystem event.

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

## Data Schema

| Field | Type | Required | Notes |
|-------|------|----------|-------|
| `event_type` | string | yes | One of `created`, `modified`, `deleted` |
| `path` | string | yes | Vault-relative path |
| `content_hash` | string | yes when known (always for create/modify; for delete, the last known hash from the index) | `sha256:` hex of file content |
| `detected_at` | ISO-8601 string (µs precision, UTC) | yes | When Hypomnema confirmed the change |

When the daemon has no prior record for a deleted path — a rare race where the watcher reports a delete on a path that was never indexed — the outbox emits no event for it; the schema therefore never expresses delete-without-hash in practice.

Future fields (additive, optional) will be added as the daemon learns to notice more. Consumers should ignore unknown fields.

### File Format

- One JSON object per line (JSONL)
- UTF-8
- Location: `~/.local/share/hypomnema/outbox.jsonl` on Linux and macOS; `%APPDATA%\hypomnema\outbox.jsonl` on Windows (see [reference/configuration.md](../reference/configuration.md) for XDG/env overrides)
- Never rotated by Hypomnema in v0 (rotation is an open question)

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
