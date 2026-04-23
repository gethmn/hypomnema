---
name: filesystem-watching
description: Use when building, modifying, or debugging the Hypomnema filesystem watcher, handling file change events from editors or sync tools, or investigating spurious reindex behavior. Covers the notify + notify-debouncer-full pattern, editor-save patterns, sync-tool event storms, and conflict-file filtering. Apply whenever code touches file change detection, event handling, or the reindex loop.
---

# Watching filesystems in Hypomnema

The watcher has one job: detect when Markdown files under the watched directory change, and emit a clean event stream. "Clean" is doing real work — editors and sync tools produce event storms that need coalescing.

## Always use the debouncer

`notify` alone gives you raw OS events, and raw events are chaotic:

- Obsidian saves via write-to-temp + rename: Create + Modify + Remove on three paths within ~10ms.
- Syncthing writes via a `.syncthing.*.tmp` file + rename: same pattern.
- Some editors fire two Modify events back-to-back (buffer flush + metadata update).
- Sync-tool rescans fire dozens of events at once.

`notify-debouncer-full` coalesces these into one event per logical change, per path, per debounce window.

```rust
use notify::RecursiveMode;
use notify_debouncer_full::{new_debouncer, DebouncedEvent};
use std::time::Duration;
use tokio::sync::mpsc;

pub fn spawn_watcher(
    path: PathBuf,
    debounce: Duration,
) -> anyhow::Result<(mpsc::Receiver<Vec<DebouncedEvent>>, impl Drop)> {
    let (tx, rx) = mpsc::channel(128);
    let mut debouncer = new_debouncer(debounce, None, move |res| {
        match res {
            Ok(events) => {
                let _ = tx.blocking_send(events);
            }
            Err(errs) => {
                for e in errs {
                    tracing::warn!(error = ?e, "watcher error");
                }
            }
        }
    })?;
    debouncer.watch(&path, RecursiveMode::Recursive)?;
    Ok((rx, debouncer))
}
```

The returned debouncer handle must be kept alive — dropping it stops the watcher. Store it at a scope that matches the daemon's lifetime.

## Debounce interval

500ms is a reasonable default. Short enough that interactive use doesn't feel laggy, long enough to coalesce most editor save patterns. Sync tools sometimes write in bursts that exceed 500ms; if logs show duplicate processing for the same file, bump to 1-2s.

Make it configurable, default 500ms, don't tune it speculatively.

## Filter at the watcher

Filter to `.md` files and skip dotfile-prefixed components (`.obsidian/`, `.git/`, etc.) *before* handing events downstream. Filtering at the watcher keeps the event log compact and the index clean.

```rust
fn is_relevant(path: &Path) -> bool {
    if path.components().any(|c| matches!(
        c,
        std::path::Component::Normal(os) if os.to_str().map_or(false, |s| s.starts_with('.'))
    )) {
        return false;
    }
    path.extension().and_then(|s| s.to_str()) == Some("md")
}
```

## Content-hash check before emitting

The debouncer tells you a file changed. That doesn't mean its *content* changed — editors sometimes bump mtime on save even when the content is identical. Before emitting an outbox event, read the file, hash it, compare against the last-known hash in the SQLite index. Same hash → drop.

This is the difference between "the watcher noticed a filesystem operation" and "something actually changed." The outbox records only the latter.

## Sync-conflict files

Sync tools produce filenames like these when they give up on a merge:

- `file.sync-conflict-20260422-a1b2c3d4.md` (Syncthing)
- `file (conflicted copy 2026-04-22).md` (Obsidian Sync)
- `file (Device's conflicted copy).md` (Dropbox)

Filter these out at the watcher — they should never enter the index.

```rust
fn is_sync_conflict(path: &Path) -> bool {
    let name = match path.file_name().and_then(|n| n.to_str()) {
        Some(n) => n,
        None => return false,
    };
    name.contains(".sync-conflict-")
        || name.contains("(conflicted copy")
        || name.contains("conflicted copy)")
}
```

Count them in a metric; surface the count in a health endpoint later. Don't log each one — the whole point is they're symptoms of upstream pain we don't want to re-spam.

## Self-writes (later phase)

v0 is read-only, so self-writes aren't a concern. When writes get added in a later phase, the watcher needs a way to ignore events it produced itself. The pattern is a short-lived "I'm writing X" set scoped to the write job; the debounce/hash layer filters events for paths in that set.

Don't prematurely add this to v0.

## Processing events

The watcher callback runs in a thread owned by `notify`. Don't do reindex work inside the callback — push events through the channel and let a separate task consume them.

```rust
while let Some(events) = rx.recv().await {
    for event in events {
        // validate, hash-check, process
    }
}
```

If the channel fills up because processing is slow, that's backpressure — log it and consider parallelism or batch processing. Don't increase the channel buffer size to "fix" it without understanding why.

## Smells

- Any code that reimplements debouncing with a HashMap and timestamps.
- Processing raw `notify::Event` values rather than `DebouncedEvent` — means you're not using the debouncer.
- Using `recommended_watcher` without the debouncer wrapper.
- Emitting outbox events without the content-hash check — leads to spurious re-indexing.
- Running reindex work in the watcher callback thread.
- `std::fs` I/O inside an async task that's already in `spawn_blocking` — at that point you're fine, but if it's directly in an async fn, that's blocking the runtime.
