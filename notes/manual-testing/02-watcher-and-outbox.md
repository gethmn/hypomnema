# 02 · Watcher and outbox

> Applies to: steps 3, 4. Prereqs:
> [`01-running-the-daemon.md`](./01-running-the-daemon.md) complete and
> `hmnd` is running against the fixture vault.

This doc verifies that file changes inside the vault produce the
expected outbox events, and that ignored paths produce silence. The
event envelope shape lives in
[`docs/specs/change-events.md`](../../docs/specs/change-events.md);
this doc only checks behavior.

Throughout, `<VAULT>` is the absolute path to
`notes/manual-testing/fixtures/sample-vault/` from your config.

## Setup: tail the outbox

In a third terminal (call it C):

```bash
tail -f ~/.local/share/hypomnema/outbox.jsonl
```

Each event is one JSON object per line. Fields you'll see:
`event_type` (`created` / `modified` / `deleted`), `path` (vault-
relative, forward-slash), `content_hash` (omitted on `deleted` for
files that were never indexed), `detected_at` (ISO-8601 µs).

**Important**: the *initial* scan does not write outbox events — only
changes observed by the watcher after startup do. If you don't see new
lines, `tail` is doing its job; you haven't triggered anything yet.

## 1. Created event

```bash
echo '# scratch' > "<VAULT>/notes/scratch.md"
```

Within ~1 second (debounce window plus reindex), terminal C should
show one new line:

```json
{"event_type":"created","path":"notes/scratch.md","content_hash":"…","detected_at":"…"}
```

## 2. Modified event

```bash
echo 'edit one' >> "<VAULT>/notes/scratch.md"
```

Expect one new line, `event_type: "modified"`, with a *different*
`content_hash` than the `created` line.

### Hash-gated no-op

```bash
touch "<VAULT>/notes/scratch.md"
```

Expect **no new line**. `touch` updates mtime but not bytes; the
content-hash gate suppresses the event.

## 3. Deleted event

```bash
rm "<VAULT>/notes/scratch.md"
```

Expect one new line, `event_type: "deleted"`. `content_hash` may be the
last known hash or absent depending on the spec — either is acceptable.

`hmn status` after this should show `indexed_file_count: 7` again.

## 4. Ignored: dotfile component

```bash
mkdir -p "<VAULT>/.obsidian"  # already exists in the fixture
echo '# nope' > "<VAULT>/.obsidian/note.md"
```

Expect **no new line**. The watcher's path filter rejects any path
component starting with `.`, and `.obsidian/**` is in the default
`ignore_patterns`.

Clean up:

```bash
rm "<VAULT>/.obsidian/note.md"
```

(Still no event line for the deletion either.)

## 5. Ignored: `.git/`

The default `ignore_patterns` also covers `.git/**`. Verify:

```bash
mkdir -p "<VAULT>/.git"
echo '# nope' > "<VAULT>/.git/note.md"
```

Expect no event. Clean up:

```bash
rm -rf "<VAULT>/.git"
```

## 6. Ignored: sync-conflict file

```bash
echo '# conflicted' > "<VAULT>/notes/foo.sync-conflict-20260101.md"
```

Expect no event. The committed
`fixtures/sample-vault/draft.sync-conflict-20260101.md` exists for the
same reason — it's never indexed, never appears in any search.

Clean up:

```bash
rm "<VAULT>/notes/foo.sync-conflict-20260101.md"
```

## 7. Debounce: rapid edits coalesce

```bash
for i in {1..10}; do
  echo "v$i" >> "<VAULT>/notes/scratch.md"
done
```

Expect **one or two** event lines, not ten. The exact count depends on
the debounce window (default 500 ms) and how fast the loop ran. The
key check is "much fewer than ten."

Clean up:

```bash
rm "<VAULT>/notes/scratch.md"
```

## 8. Outbox on disk

```bash
ls -lh ~/.local/share/hypomnema/outbox.jsonl
wc -l  ~/.local/share/hypomnema/outbox.jsonl
```

The file is append-only, never rotated in v0. Tail consumers (`tail
-f`, the change-events spec's `tail` API) read the same file.

Confirm `hmn status` reports a non-zero outbox file size now — earlier
in [`01-running-the-daemon.md`](./01-running-the-daemon.md) it was 0.

You're ready for [`03-search.md`](./03-search.md).
