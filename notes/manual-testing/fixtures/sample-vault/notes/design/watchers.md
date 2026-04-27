# Filesystem watchers

How Hypomnema observes vault changes without losing events or storming the
database.

## Debounce

Editor saves and sync-tool writes generate event storms. The daemon uses
`notify-debouncer-full` with a configurable window (default 500 ms) to
coalesce these into a single logical event per file.

## Content hash gating

Even after debouncing, plenty of events are spurious — for example mtime
updates with no byte change. Before any database write, the watcher reads
the file, hashes it, and compares against the stored hash. Only real
content changes pass the gate. This is what prevents spurious reindexes
when a sync tool rewrites a file with identical bytes.

## Notify backend

The `notify` crate sits on top of inotify (Linux), FSEvents (macOS), or
ReadDirectoryChangesW (Windows). The Notify backend is selected
automatically; we don't tune it.
