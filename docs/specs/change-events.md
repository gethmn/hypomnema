# Change Events (Outbox) Specification

**Version**: 0.2.0
**Date**: 2026-04-27
**Status**: Draft

---

## Overview

Hypomnema emits a durable stream of change events so consumers can react to vault changes without polling the search API. Events are written as JSON lines to an append-only outbox file under each vault's per-vault subdirectory in the daemon's data directory. Consumers subscribe by tailing the file.

**Related Documents**:
- [ADR-0006: Outbox Lives Outside the Watched Directory](../decisions/0006-outbox-outside-watched-directory.md)
- [ADR-0009: Multi-Vault per Daemon](../decisions/0009-multi-vault-per-daemon.md)
- [Vault Management § Per-Vault Layout](./vault-management.md#per-vault-layout)
- [Architecture: Outbox Writer](../architecture/overview.md#outbox-writer)

---

## Behavior

### Normal Flow

1. Watcher observes a filesystem event (create / modify / delete) under a watched vault
2. Debouncer coalesces the event storm around a single logical save
3. Indexer computes the new content hash and compares against the stored hash
4. *Only if the hash changed* does the indexer emit an event to the vault's outbox
5. The outbox writer appends one JSON line and fsyncs per event (resolved in step 4 as per-event `sync_data`).

This content-hash gate is the primary defense against editor-save noise and sync-tool mtime churn. An agent tailing an outbox sees only real changes, not every save-triggered filesystem event.

Each vault has its own watcher, indexer, and outbox writer; events from one vault never appear in another vault's outbox.

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
  "detected_at": "2026-04-23T14:22:08.123456Z",
  "vault": "01951f6c-7c3b-7a2e-8c1d-1a2b3c4d5e6f"
}
```

Event types: `created`, `modified`, `deleted`. v0 behavior, confirmed in step 3: renames are observed as a `deleted` + `created` pair. Fused rename detection remains open.

### Consumer Subscription

Consumers tail one or more per-vault outbox files at `<data_dir>/vaults/<id>/outbox.jsonl`. On startup a consumer may:
- Start from end-of-file (only see new changes)
- Replay from a byte offset it persists across restarts (per outbox file — offsets are per-vault, not global)
- Replay from a specific `detected_at` timestamp by filtering during read

Hypomnema offers no push, no webhook, no in-process callback in v0. See the handoff's "Out of scope" for deferred fan-out work.

A consumer that wants change events from every vault must enumerate vaults via the control plane (`GET /vaults`) and tail each vault's outbox file. The set of active vaults can change at runtime (`vault create` / `vault terminate`); consumers that need to react to that should re-enumerate periodically or on `daemon-unreachable` recovery.

---

## Consumer Model

The outbox is the change-notification surface for **external consumers** — applications and AI agents that subscribe to vault changes by tailing per-vault `outbox.jsonl` files. Examples: Iris, Claude Code, custom scripts.

The Hypomnema binaries themselves do not consume the event stream:
- `hmnd` writes events but never tails them.
- `hmn` displays outbox metadata (path, size) via the `/status` API but does not parse events.

This separation is intentional: per [vision.md §Consumer](../product/vision.md#consumer), "Hypomnema has no awareness of its consumers." The daemon's job ends at producing a durable, ordered event log per vault; reacting to changes is the consumer's job.

**Tail contract**: consumers open the file, read line-by-line, and persist the byte offset they have processed (per-vault — one offset per outbox file). On restart they reopen and seek to the persisted offset. Lines are JSON-per-line and complete-line-terminated; consumers should treat a partial trailing line (no terminating `\n`) as not-yet-committed and re-read it on next poll.

**Why the surrogate vault ID and not the name on the wire**: outbox files are durable; vault names are mutable (the `rename` operation updates the registry without touching the outbox). Consumers that persist offsets keyed by vault name would silently break on rename. Keying by the immutable surrogate ID — and resolving the name on demand via `GET /vaults/{id}` for display — is the durable shape. This is why the outbox event envelope includes `vault` (the ID) but **never** `vault_name`. Search responses, by contrast, do include `vault_name`: search is point-in-time and the name is fresh.

---

## Data Schema

| Field | Type | Required | Notes |
|-------|------|----------|-------|
| `event_type` | string | yes | One of `created`, `modified`, `deleted` |
| `path` | string | yes | Vault-relative path |
| `content_hash` | string | yes when known (always for create/modify; for delete, the last known hash from the index) | `sha256:` hex of file content |
| `detected_at` | ISO-8601 string (µs precision, UTC) | yes | When Hypomnema confirmed the change |
| `vault` | string | yes (multi-vault) / no (single-vault) | Surrogate vault ID (UUIDv7). Populated when multi-vault is active (round 3+); omitted from v0/step-9 single-vault outbox lines. Consumers must accept its absence in v0 lines and treat it as "the only vault." |

The outbox **never** carries a `vault_name` field. Names are mutable; the outbox is durable. Consumers that want a name for display should resolve `vault` against the registry at display time.

When the daemon has no prior record for a deleted path — a rare race where the watcher reports a delete on a path that was never indexed — the outbox emits no event for it; the schema therefore never expresses delete-without-hash in practice.

Future fields (additive, optional) will be added as the daemon learns to notice more. Consumers should ignore unknown fields.

### File Format

- One JSON object per line (JSONL)
- UTF-8
- Location (multi-vault, round 3+): `<data_dir>/vaults/<vault_id>/outbox.jsonl`. Each vault has its own outbox file under its per-vault subdirectory; see [vault-management.md § Per-Vault Layout](./vault-management.md#per-vault-layout).
- Location (v0/step-9 single-vault, pre-multi-vault startup-migration): `<data_dir>/outbox.jsonl`. Step-9's reconcile pass migrates the legacy single-file layout into the per-vault layout when multi-vault first activates.
- Default `<data_dir>` is `~/.local/share/hypomnema/` on Linux and macOS; `%APPDATA%\hypomnema\` on Windows (see [reference/configuration.md](../reference/configuration.md) for XDG/env overrides)
- Never rotated by Hypomnema in v0 (rotation is an open question)

### Size & Growth

Each event serializes to ~150–200 bytes typical (path-length dependent; SHA256 hash adds ~50 bytes when present; the new `vault` UUIDv7 field adds ~40 bytes). Rough envelope (per vault):

| Events | File size |
|--------|-----------|
| 1,000 | ~180 KB |
| 100,000 | ~18 MB |
| 1,000,000 | ~180 MB |

v0 / round 3 never rotates (see Open Questions). Operators on long-running vaults with high churn should plan for unbounded growth or perform manual archival per the per-vault Intentional Reset procedure below.

---

## Edge Cases

### Large write followed by immediate delete

Debouncer coalesces; final state wins. If a file is written then deleted within the debounce window, only a `deleted` event is emitted — there was no stable state to index.

### Content hash collision

sha256 collisions are considered impossible for this purpose. No mitigation.

### Outbox file removed or truncated externally

Daemon recreates/opens the file on next event. Consumers that were tailing an old inode will stop seeing events — the correct recovery is consumer-side (reopen on `ENOENT` or detect inode change). This applies per-vault: removing one vault's outbox file does not affect another's.

### Crash during write

The write is small (one line). In the worst case a consumer sees a truncated JSON line and must skip it. The daemon on restart picks up from the end-of-file; no duplicate is emitted for events that made it through before the crash.

### Outbox file corrupted (partial trailing line)

Per the [Consumer Model tail contract](#consumer-model), consumers should treat a partial trailing line as not-yet-committed and re-read on next poll. A line is committed when terminated with `\n`. The daemon writes one event per `writeln!` followed by `fdatasync`, so a partial line on disk indicates an in-flight write or a crash mid-write — not corruption to escalate.

### Vault terminated mid-tail

A consumer tailing `<data_dir>/vaults/<id>/outbox.jsonl` when the operator terminates that vault sees the file disappear (the per-vault subdirectory is removed by the terminate flow per [vault-management.md § Operations § terminate](./vault-management.md#operations)). Consumer-side recovery: detect `ENOENT` on next read, drop the offset, optionally call `GET /vaults` to confirm the vault is gone, stop tailing.

### Intentional reset (start over with a fresh outbox) — per-vault

Operators may need to discard outbox history for a single vault (file grew unmanageably; consumer state for that vault is unrecoverable). The reset is **per-vault**, not daemon-wide:

1. Pause or terminate the target vault's writer (round-3 ships `vault terminate`; round-4 will ship `vault pause`). Both stop the outbox writer for that vault.
2. Delete or move `<data_dir>/vaults/<id>/outbox.jsonl`.
3. If the vault was paused, resume it (round 4); if terminated, recreate it. The daemon recreates the file empty on next append.
4. Reset any consumer-side persisted offsets for that vault to 0.

Resetting one vault's outbox does **not** affect any other vault's outbox. Operators wanting to reset every vault's outbox must repeat the procedure per vault, or stop the daemon and clear `<data_dir>/vaults/*/outbox.jsonl` directly before restart.

This is safe because the outbox is **not authoritative for vault state** — the per-vault `index.sqlite` is. Resetting an outbox loses notification history but does not affect indexed state. A consumer that needs to re-bootstrap after a reset should re-query the index (per [Cold Start](#cold-start)).

---

## Open Questions

- [x] Exact fsync policy: per-event vs periodic? Resolved in step 4 as per-event `sync_data`. See [step-4 workplan § Deferred decision 1](../roadmap/step-04-workplan.md#1-fsync-policy-per-event-vs-periodic).
- [ ] Rename detection: should `renamed` be a distinct event type?
- [ ] Outbox rotation / retention — should Hypomnema rotate after N MB or N days, or leave this to the user? In multi-vault, this question is per-vault: operators with many vaults may want global retention, not per-file rotation.
- [ ] Should a consumer be able to ask Hypomnema for the current outbox byte offset (per vault), so it can checkpoint without inspecting the file directly?
- [ ] Cross-vault aggregated outbox: should the daemon expose a merged stream for consumers that want every vault's events, or is per-vault tailing the right shape? Round-3 ships per-vault only; aggregation is consumer-side (or future round).

---

## Revision History

| Version | Date | Changes |
|---------|------|---------|
| 0.1.0 | 2026-04-23 | Initial draft, seeded from project handoff v0 scope |
| 0.1.1 | 2026-04-26 | Clarify consumer model (external-only), cold-start semantics, size envelope, and recovery procedures. No behavior change. |
| 0.2.0 | 2026-04-27 | Multi-vault adoption (round 3 / step 10): `vault` field semantics flipped from "always absent" to "populated when multi-vault active"; explicit "no `vault_name`" rule (durability over display ergonomics); per-vault outbox path `<data_dir>/vaults/<id>/outbox.jsonl`; per-vault Intentional Reset procedure; vault-terminated-mid-tail edge case. |
