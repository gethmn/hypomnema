# Change Events Specification

**Version**: 0.4.0
**Date**: 2026-04-30
**Status**: Draft

---

## Overview

Hypomnema emits a live stream of change notifications so consumers can react to vault changes without polling search endpoints in a tight loop. In v0 this stream is **not durable**: events are delivered to subscribers that are connected at the time the daemon observes a real change, and missed events are recovered by re-querying the current index state.

The public v0 surfaces are:

- `hmn vault watch NAME|ID` for humans and scripts
- HTTP streaming from the daemon for CLI and non-MCP clients
- MCP control-plane access for agent hosts

The old JSONL outbox file is no longer the public subscription contract. A durable replayable event log remains a possible future feature, but it must be designed as an event store with stream generations, sequence numbers, retention, and explicit invalidation semantics rather than as consumers tailing daemon-owned files.

**Related Documents**:
- [ADR-0006: Daemon State Lives Outside the Watched Directory](../decisions/0006-outbox-outside-watched-directory.md)
- [ADR-0009: Multi-Vault per Daemon](../decisions/0009-multi-vault-per-daemon.md)
- [Vault Management § MCP Tool Surface](./vault-management.md#mcp-tool-surface)
- [Architecture: Change Event Bus](../architecture/overview.md#change-event-bus)

---

## Behavior

### Normal Flow

1. Watcher observes a filesystem event under a watched vault.
2. Debouncer coalesces editor-save and sync-tool event storms.
3. Indexer computes the new content hash and compares it against the stored hash.
4. Only if indexed state changes does the daemon publish a change notification.
5. Active subscribers receive the event over their streaming transport.

The content-hash gate remains the primary defense against editor-save noise and sync-tool mtime churn. A consumer watching events sees real indexed changes, not every filesystem notification.

Each event carries a vault surrogate ID. The event stream never carries `vault_name`; names are mutable and consumers can resolve a display name through the vault control plane when they need one.

### Live-Only Contract

The v0 event stream starts at subscription time. It does not replay changes that occurred before the subscription was established, and it does not guarantee delivery across daemon restarts, client disconnects, slow consumers, channel overflow, or vault lifecycle resets.

Consumers that need a complete view use the index as source of truth:

1. Query current state through search or vault-status APIs.
2. Subscribe to the live event stream.
3. Treat events as invalidation hints for affected vault paths.
4. Re-query Hypomnema when an event arrives or when the stream reports lag/loss.

This avoids pretending that the daemon has durable history it does not actually maintain. The index answers "what exists now"; the live stream answers "what changed while I was listening?"

### Race Window

A consumer that queries current state and then starts watching can miss a change in the gap between those two operations. v0 accepts this as part of the live-only contract. Consumers that need a no-gap bootstrap require the future durable/replayable stream design described below.

### Event Envelope

```json
{
  "type": "file_changed",
  "event_type": "modified",
  "vault": "01951f6c-7c3b-7a2e-8c1d-1a2b3c4d5e6f",
  "path": "notes/databases/pgvector.md",
  "content_hash": "sha256:abc123...",
  "detected_at": "2026-04-30T14:22:08.123456Z"
}
```

Initial v0 file event types are `created`, `modified`, and `deleted`. Renames are observed as a `deleted` + `created` pair. Fused rename detection remains a future option.

The top-level `type` field leaves room for stream-control events without overloading file-change semantics.

### Stream Control Events

The stream may emit control events:

```json
{
  "type": "stream_lagged",
  "vault": "01951f6c-7c3b-7a2e-8c1d-1a2b3c4d5e6f",
  "missed": 42,
  "action": "resync_required",
  "detected_at": "2026-04-30T14:24:01.000000Z"
}
```

`stream_lagged` means the subscriber missed one or more events from the live in-memory channel. The consumer must discard any assumption that it has a continuous event sequence and re-query the index for current state.

### CLI Subscription

`hmn vault watch NAME|ID` subscribes to one vault. `hmn vault watch --all` subscribes to all currently active vaults.

Default output is newline-delimited JSON event envelopes so scripts can pipe it safely. A future text mode may be added, but JSON is the canonical v0 shape.

The command exits when the daemon closes the stream, the selected vault is terminated, the user interrupts the process, or the client loses connection to the daemon. On reconnect, the command starts a new live-only stream; it does not ask for replay.

### HTTP Subscription

The HTTP surface is streaming and maps directly to the CLI. Route shape:

- `GET /vaults/{name_or_id}/watch` for a single vault.
- `GET /events/watch` for all-active-vault subscriptions.

The response body is **newline-delimited JSON** (one JSON event envelope per line). Server-Sent Events are not the v0 framing; NDJSON is the canonical choice because it is simpler for CLI piping and does not add browser-specific semantics.

### MCP Subscription

MCP streaming for `vault_watch` is **deferred** pending explicit architectural design for long-lived resource subscriptions in a future `rmcp` version or v1+ framing.

**Current status**: `vault_watch` tool is **not** shipped today. Verified in Task 16.6: `rmcp` 1.5.0 exposes `subscribe`/`unsubscribe` resource handler stubs that default to `method_not_found`, but provides no server-side API for proactively pushing notifications from a tool call — `CallToolResult` is a single-response type, not a stream. The MCP tool surface exposes only request/response operations (`vault_status`, vault CRUD). MCP clients can consume live change events by accessing the daemon's HTTP watch endpoints (`GET /vaults/{id}/watch`, `GET /events/watch`) through their host/runtime integration until a separate MCP-streaming workplan pins the transport shape.

**Future Direction** (v1+): if and when `rmcp` or MCP spec adds native long-lived streaming support (e.g., resource subscriptions, server notifications), the design for `vault_watch` would mirror the HTTP/CLI surface:
- `target?: string` selects one vault by name or ID; omission follows the same default-vault resolution as `vault_status`.
- `all?: bool` subscribes to all active vaults.
- Live-only semantics; no `since` argument, no replay.

---

## Data Schema

### File Change Event

| Field | Type | Required | Notes |
|-------|------|----------|-------|
| `type` | string | yes | `"file_changed"` |
| `event_type` | string | yes | One of `created`, `modified`, `deleted` |
| `vault` | string | yes | Surrogate vault ID (UUIDv7) |
| `path` | string | yes | Vault-relative path |
| `content_hash` | string | yes when known | `sha256:` hex of file content; for deletes, the last known hash from the index |
| `detected_at` | ISO-8601 string (microsecond precision, UTC) | yes | When Hypomnema confirmed the change |

### Stream Control Event

| Field | Type | Required | Notes |
|-------|------|----------|-------|
| `type` | string | yes | `"stream_lagged"` initially; future control events are additive |
| `vault` | string | no | Present when lag is known to affect one vault; omitted for daemon-wide stream loss |
| `missed` | integer | no | Number of missed events when the channel can report it |
| `action` | string | yes | `"resync_required"` |
| `detected_at` | ISO-8601 string (microsecond precision, UTC) | yes | When Hypomnema detected the stream condition |

Consumers must ignore unknown fields and unknown `type` values they do not understand, except that unknown control events should be treated conservatively as `resync_required` if they indicate stream continuity may be broken.

---

## Edge Cases

### Large Write Followed By Immediate Delete

Debouncer coalesces; final state wins. If a file is written then deleted within the debounce window, only a delete may be emitted, and only if the daemon had a prior indexed record for the path.

### Content Hash Collision

SHA-256 collisions are considered impossible for this purpose. No mitigation.

### Slow Subscriber

The daemon may use a bounded in-memory channel. If a subscriber falls behind and the channel drops events before that subscriber receives them, the subscriber receives `stream_lagged` when the runtime can detect the loss. The consumer must re-query the index.

### Daemon Restart

All live subscriptions end. Consumers reconnect and perform their normal bootstrap: query current state, then watch live events.

### Vault Paused

Paused vaults do not emit live file-change events because their watcher/indexer lifecycle is stopped. Resuming a vault starts a fresh live stream from that point forward.

### Vault Reset Or Rebuild

Reset/rebuild may invalidate consumer assumptions about derived state. v0 can surface this as a control event if the event bus is active when the operation happens, but consumers must still treat reset/rebuild as a reason to re-query current state. Durable replay semantics for reset require stream generations and are deferred.

### Vault Terminated

A subscriber watching a terminated vault receives stream closure or a terminal control event, depending on transport. The consumer should stop watching that vault and drop any cached state keyed to its surrogate ID unless it intentionally preserves historical display data.

---

## Future Durable Event Stream

A replayable event stream is deferred. If a real consumer needs "subscribe since X", Hypomnema should implement it as a database-backed event store, not as a public JSONL file contract.

Minimum durable design requirements:

- Per-vault `stream_id` or `generation`, changed whenever history is invalidated.
- Monotonic per-vault `seq` values inside a stream generation.
- A durable event table, likely in SQLite, with `(vault_id, stream_id, seq)` as the replay key.
- Retention policy: size-based, time-based, manual, or "forever"; undefined retention makes `since` unsafe.
- Explicit `410 Gone` / `stream_reset` behavior when a consumer asks for events before the retention floor or from an old generation.
- A way to get a bootstrap watermark so a consumer can query current state and then subscribe without a race.
- Tests for ordering, reconnect, retention boundary, reset/rebuild, rename, pause/resume, terminate, and multi-vault aggregation.

Illustrative schema:

```sql
CREATE TABLE change_events (
    vault_id TEXT NOT NULL,
    stream_id TEXT NOT NULL,
    seq INTEGER NOT NULL,
    event_type TEXT NOT NULL,
    path TEXT,
    content_hash TEXT,
    detected_at TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    PRIMARY KEY (vault_id, stream_id, seq)
);
```

Future event types may need to go beyond path-level file changes. Examples include vault lifecycle events, index rebuild boundaries, embedding-skip or embedding-recovery signals, chunk-level invalidation, and richer rename events. Those should be added only when a consumer has a concrete invalidation need; otherwise the event stream should stay an invalidation hint and the index should remain the source of truth.

---

## Open Questions

- [ ] Whether `hmn vault watch --all` should include vaults created after the command starts or only the active set at subscription time. v0 pins to active-at-subscription-time per the workplan default; a spec amendment can relax this.
- [ ] Whether reset/rebuild should emit a v0 control event, or whether lifecycle operations remain visible only through the control-plane response.
- [ ] Whether a future durable stream belongs in each per-vault `index.sqlite`, in `vaults.sqlite`, or in a separate daemon-level event store.
- [ ] MCP streaming design for `vault_watch` tool pending explicit rmcp support for long-lived subscriptions (v1+ framing).

---

## Revision History

| Version | Date | Changes |
|---------|------|---------|
| 0.1.0 | 2026-04-23 | Initial durable JSONL outbox draft, seeded from project handoff v0 scope. |
| 0.1.1 | 2026-04-26 | Clarified consumer model, cold-start semantics, size envelope, and recovery procedures. |
| 0.2.0 | 2026-04-27 | Multi-vault adoption: per-vault outbox path and `vault` field semantics. |
| 0.3.0 | 2026-04-30 | Replaced JSONL outbox as public v0 contract with a live internal event bus exposed through CLI/HTTP/MCP; durable replay moved to future event-store design. |
| 0.4.0 | 2026-04-30 | Pinned NDJSON as the HTTP/CLI v0 framing; narrowed MCP `vault_watch` to deferred pending Task 16.6 rmcp verification; closed HTTP-framing open question. |
| 0.4.1 | 2026-04-30 | Task 16.6: updated § MCP Subscription to record that rmcp 1.5.0 was inspected and confirmed to lack server-side push streaming from tool calls; decision to defer is final for v0. |
