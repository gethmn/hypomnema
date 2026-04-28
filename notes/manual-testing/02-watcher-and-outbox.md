# 02 · Watcher and outbox

> Applies to: round 4 / step 12 (per-vault outboxes).
> Prereqs: [`01-running-the-daemon.md`](./01-running-the-daemon.md)
> complete and `hmnd` is running against both fixture vaults.

This doc verifies that file changes inside each vault produce the
expected outbox events on the matching per-vault outbox file, and that
ignored paths produce silence. The event envelope shape lives in
[`docs/specs/change-events.md`](../../docs/specs/change-events.md);
this doc only checks behavior.

Throughout, `<VAULT_A>` is the absolute path to
`notes/manual-testing/fixtures/sample-vault/` and `<VAULT_B>` is the
absolute path to `notes/manual-testing/fixtures/sample-vault-2/`.

## Per-vault outbox layout

In round 4 each vault owns its own outbox file under the daemon's
data dir at `<data_dir>/vaults/<vault_id>/outbox.jsonl`. Capture the
two surrogate IDs once so the runbook commands below can reference
them:

```bash
SAMPLE_ID=$(hmn --json vault list   | jq -r '.vaults[] | select(.name=="sample")   | .id')
SAMPLE2_ID=$(hmn --json vault list  | jq -r '.vaults[] | select(.name=="sample-2") | .id')
echo "sample   = $SAMPLE_ID"
echo "sample-2 = $SAMPLE2_ID"
```

Per-vault outbox paths:

```bash
ls -lh ~/.local/share/hypomnema/vaults/$SAMPLE_ID/outbox.jsonl \
       ~/.local/share/hypomnema/vaults/$SAMPLE2_ID/outbox.jsonl
```

Both files exist (created when the vault was created) and may be
empty until the watcher has observed a change.

## Setup: tail both outboxes

In a third terminal (call it C):

```bash
tail -f ~/.local/share/hypomnema/vaults/$SAMPLE_ID/outbox.jsonl \
        ~/.local/share/hypomnema/vaults/$SAMPLE2_ID/outbox.jsonl
```

`tail -f` prints a header naming each file as new lines arrive, so
you can tell at a glance which vault a change came from. Each event
is one JSON object per line. Fields you'll see: `event_type`
(`created` / `modified` / `deleted`), `path` (vault-relative,
forward-slash), `content_hash` (omitted on `deleted` for files that
were never indexed), `detected_at` (ISO-8601 µs).

**Important**: the *initial* scan for each vault did not write outbox
events — only changes observed by the watcher after a vault's startup
do. If you don't see new lines, `tail` is doing its job; you haven't
triggered anything yet.

## 1. Created event in vault A

```bash
echo '# scratch' > "<VAULT_A>/notes/scratch.md"
```

Within ~1 second (debounce window plus reindex), terminal C should
show one new line on `…/$SAMPLE_ID/outbox.jsonl`:

```json
{"event_type":"created","path":"notes/scratch.md","content_hash":"…","detected_at":"…"}
```

Vault B's outbox stays silent.

## 2. Modified event in vault A

```bash
echo 'edit one' >> "<VAULT_A>/notes/scratch.md"
```

Expect one new line on `…/$SAMPLE_ID/outbox.jsonl`,
`event_type: "modified"`, with a *different* `content_hash` than the
`created` line.

### Hash-gated no-op

```bash
touch "<VAULT_A>/notes/scratch.md"
```

Expect **no new line**. `touch` updates mtime but not bytes; the
content-hash gate suppresses the event.

## 3. Deleted event in vault A

```bash
rm "<VAULT_A>/notes/scratch.md"
```

Expect one new line on `…/$SAMPLE_ID/outbox.jsonl`,
`event_type: "deleted"`. `content_hash` may be the last known hash
or absent depending on the spec — either is acceptable.

`hmn vault status sample` after this should show the original
indexed-files-equivalent for vault A again (no scratch file).

## 4. Vault isolation: a change in vault B touches only vault B's outbox

```bash
echo '# scratch' > "<VAULT_B>/recipes/scratch.md"
```

Expect one new line on `…/$SAMPLE2_ID/outbox.jsonl` only:

```json
{"event_type":"created","path":"recipes/scratch.md","content_hash":"…","detected_at":"…"}
```

Vault A's outbox stays silent. The two watchers run independently;
events for one vault never leak into the other's outbox file.

Clean up:

```bash
rm "<VAULT_B>/recipes/scratch.md"
```

Expect one `deleted` line on `…/$SAMPLE2_ID/outbox.jsonl`.

## 5. Ignored: dotfile component

```bash
mkdir -p "<VAULT_A>/.obsidian"  # already exists in the fixture
echo '# nope' > "<VAULT_A>/.obsidian/note.md"
```

Expect **no new line** on either outbox. The watcher's path filter
rejects any path component starting with `.`, and `.obsidian/**` is
in the default `ignore_patterns`.

Clean up:

```bash
rm "<VAULT_A>/.obsidian/note.md"
```

(Still no event line for the deletion either.)

## 6. Ignored: `.git/`

The default `ignore_patterns` also covers `.git/**`. Verify against
either vault — vault A already has a `.git/` directory in the fixture:

```bash
echo '# nope' > "<VAULT_A>/.git/note.md"
```

Expect no event. Clean up:

```bash
rm "<VAULT_A>/.git/note.md"
```

## 7. Ignored: sync-conflict file (both vaults)

```bash
echo '# conflicted' > "<VAULT_A>/notes/foo.sync-conflict-20260101.md"
echo '# conflicted' > "<VAULT_B>/recipes/bar.sync-conflict-20260201.md"
```

Expect no events on either outbox. The committed
`sample-vault/draft.sync-conflict-20260101.md` and
`sample-vault-2/draft.sync-conflict-20260201.md` exist for the same
reason — they are never indexed and never appear in any search.

Clean up:

```bash
rm "<VAULT_A>/notes/foo.sync-conflict-20260101.md"
rm "<VAULT_B>/recipes/bar.sync-conflict-20260201.md"
```

## 8. Debounce: rapid edits coalesce

```bash
for i in {1..10}; do
  echo "v$i" >> "<VAULT_A>/notes/scratch.md"
done
```

Expect **one or two** event lines on `…/$SAMPLE_ID/outbox.jsonl`, not
ten. The exact count depends on the debounce window (default 500 ms)
and how fast the loop ran. The key check is "much fewer than ten."

Clean up:

```bash
rm "<VAULT_A>/notes/scratch.md"
```

## 9. Outbox on disk

```bash
wc -l ~/.local/share/hypomnema/vaults/$SAMPLE_ID/outbox.jsonl
wc -l ~/.local/share/hypomnema/vaults/$SAMPLE2_ID/outbox.jsonl
```

Each file is append-only, never rotated in v0. Tail consumers (`tail
-f`, the change-events spec's `tail` API) read the per-vault file
under the corresponding vault's subdirectory.

`hmn vault status sample` and `hmn vault status sample-2` each
report the per-vault outbox path and current byte size in their
detail blocks (when the daemon's response includes them — the schema
fields are documented in
[`docs/specs/vault-management.md` § Control-Plane HTTP Wire Shapes](../../docs/specs/vault-management.md#control-plane-http-wire-shapes)).

You're ready for [`03-search.md`](./03-search.md).
