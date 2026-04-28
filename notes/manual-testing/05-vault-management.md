# 05 · Vault management

> Applies to: round 3 / steps 9–11 (multi-vault registry, lifecycle
> ops). Prereqs: [`01-running-the-daemon.md`](./01-running-the-daemon.md)
> complete and `hmnd` is running against both fixture vaults.

This doc walks through every `hmn vault …` subcommand against the two
runbook fixture vaults (`sample` and `sample-2`). The full nine-op
lifecycle ships:

| Op | Effect |
|---|---|
| `create` | Register a new vault; allocate UUIDv7; create per-vault subdir; start watcher + indexer. |
| `list` | Print every registered vault. |
| `status` | Single-vault detail. |
| `pause` | Drain watcher + indexer; preserve index; vault skipped from default search scope. |
| `resume` | Restart watcher + indexer; clear `last_error` on success. |
| `reset [--rebuild]` | Restart watcher + indexer, clearing `last_error`; with `--rebuild`, also drop and rebuild `chunks` + `chunks_vec`. |
| `rename --new-name` | Registry UPDATE only; surrogate ID unchanged. |
| `rescan` | Force a full directory walk; re-stat and re-hash every file. |
| `terminate` | Stop watcher + indexer; remove registry row; remove per-vault subdir (never the vault path's own files). |

Specifications:
[`docs/specs/vault-management.md`](../../docs/specs/vault-management.md);
HTTP wire shapes:
[`docs/specs/vault-management.md` § Control-Plane HTTP Wire Shapes](../../docs/specs/vault-management.md#control-plane-http-wire-shapes).
The MCP tool surface (12 tools — 3 search + 9 vault) is documented
in [`04-mcp.md`](./04-mcp.md) and
[`06-mcp-http.md`](./06-mcp-http.md).

Run all commands from terminal B; `hmnd` keeps running in terminal A.

---

## 1. `vault list` — read the registry

```bash
hmn vault list
```

Expect a two-row table. Names: `sample`, `sample-2`. Status: `active`
for both. Created: ISO-8601 timestamps from
[`00-setup.md`](./00-setup.md) §7.

JSON form for scripting:

```bash
hmn --json vault list | jq '.vaults | length'
```

Expect **2**.

```bash
hmn --json vault list | jq '.vaults[] | {id, name, status, path}'
```

Both rows include the surrogate UUID (UUIDv7), the registry name,
the canonical absolute path, and the current status. `last_error`
is omitted when null (`#[serde(skip_serializing_if = "Option::is_none")]`).

Capture the surrogate IDs for use further down — several commands
later in this doc accept either name or ID:

```bash
SAMPLE_ID=$(hmn --json vault list  | jq -r '.vaults[] | select(.name=="sample")   | .id')
SAMPLE2_ID=$(hmn --json vault list | jq -r '.vaults[] | select(.name=="sample-2") | .id')
```

---

## 2. `vault status` — single-vault detail

By name:

```bash
hmn vault status sample
hmn vault status sample-2
```

Each prints a labeled key/value block:

```
id:         019dd258-3992-7c3b-7a2e-8c1d1a2b3c4d
name:       sample
path:       <ABS>/notes/manual-testing/fixtures/sample-vault
status:     active
created_at: 2026-04-28T15:30:00.123456Z
```

By surrogate ID:

```bash
hmn vault status "$SAMPLE_ID"
```

Same output as `hmn vault status sample`.

Without a selector — resolves to `default_vault_name` (`sample` per
[`00-setup.md`](./00-setup.md)):

```bash
hmn vault status
```

JSON form:

```bash
hmn --json vault status sample-2 | jq
```

Returns the `VaultRow` shape verbatim.

### Closest-name hint on a typo

```bash
hmn vault status sampel
```

Expect exit code 5 (`vault not found / not in expected state`) with
a stderr message naming the closest match within Levenshtein
distance 3 (`did you mean 'sample'?`). The same hint flows over the
HTTP and MCP transports as a top-level `hint` field on the
`vault_not_found` error envelope.

---

## 3. `vault create` — register a third vault

The runbook already has `sample` and `sample-2` registered (per
[`00-setup.md`](./00-setup.md) §7). Create a small ad-hoc third vault
to exercise the full create→terminate cycle without disturbing the
fixtures:

```bash
mkdir -p /tmp/hmn-scratch-vault/notes
echo '# scratch vault' > /tmp/hmn-scratch-vault/README.md
echo '# note 1'        > /tmp/hmn-scratch-vault/notes/one.md

hmn vault create --name scratch /tmp/hmn-scratch-vault
```

Expect a single-row response — the new `VaultRow` with status
`active`. Confirm via `hmn vault list`:

```bash
hmn --json vault list | jq '.vaults | length'
```

Expect **3**.

The daemon creates `<data_dir>/vaults/<scratch_id>/` with its own
`index.sqlite`, `outbox.jsonl`, and `meta.toml`; the watcher and
indexer are started; the initial scan summary lands in `hmnd`'s
log (`inserted=2 updated=0 hash_unchanged=0 deleted=0`).

### Failure modes

```bash
hmn vault create --name sample /tmp/hmn-scratch-vault
```

Expect HTTP 409 `vault_name_conflict` — the name `sample` is already
in use.

```bash
hmn vault create --name another /tmp/hmn-scratch-vault
```

Expect HTTP 409 `vault_path_conflict` — the canonicalized path is
already registered (under the name `scratch`).

```bash
hmn vault create --name nope /this/path/does/not/exist
```

Expect HTTP 422 `vault_path_invalid`.

---

## 4. `vault pause` and `vault resume`

Pause vault A:

```bash
hmn vault pause sample
hmn vault status sample | grep '^status:'
```

Expect `status: paused`. The per-vault `index.sqlite` and
`outbox.jsonl` stay in place; the watcher and indexer drain
(cooperative, 30s drain cap). Searches without `--vaults` now skip
this vault and report it under `partial_results.skipped[]` (see
[`03-search.md`](./03-search.md) § Partial-results diagnostics §A).

Pause-on-paused is idempotent (returns the existing row):

```bash
hmn vault pause sample   # second call, same vault
```

Expect a successful response; `status` remains `paused`.

Resume:

```bash
hmn vault resume sample
hmn vault status sample | grep '^status:'
```

Expect `status: active`. The watcher and indexer respawn; `last_error`
is cleared if it was set.

Resume-on-active is idempotent.

---

## 5. `vault reset` — restart watcher + indexer

Plain `reset` is non-destructive; it clears `last_error` and
respawns the watcher and indexer:

```bash
hmn vault reset scratch
```

(No prompt; non-destructive.) Useful when the underlying vault path
becomes reachable again after an `errored → active` transition the
operator wants to force.

`reset --rebuild` additionally drops `chunks` and `chunks_vec` and
clears every `files.content_hash` for the vault, so the next
indexing pass re-embeds every file from scratch:

```bash
hmn vault reset scratch --rebuild --yes
```

Without `--yes`, the CLI prompts on stderr:

```
Reset vault 'scratch' and rebuild chunks? (y/N)
```

Any answer not beginning with `y`/`Y` aborts. With `--json` and an
aborted prompt, the CLI emits `{"reset": false, "aborted": true}` to
stdout.

After `--rebuild`, `hmn vault status scratch` reports `status:
active`; the watcher's next pass re-embeds every file. With TEI down,
expect the same skip-and-log behavior covered in
[`03-search.md`](./03-search.md) §G.

---

## 6. `vault rename` — change the user-facing label

```bash
hmn vault rename scratch --new-name notes
hmn vault list | grep -E 'sample|notes'
```

Expect three rows: `sample`, `sample-2`, `notes`. The surrogate ID is
unchanged; the per-vault subdirectory is unchanged; the per-vault
`meta.toml` is rewritten with the new name.

Rename validation:

```bash
hmn vault rename notes --new-name 'has spaces'
```

Expect HTTP 422 `vault_path_invalid` — the new name must match
`[A-Za-z0-9_-]+` (CLI-friendly, no whitespace, no path separators).

```bash
hmn vault rename notes --new-name sample
```

Expect HTTP 409 `vault_name_conflict`.

Rename it back to keep the rest of the doc readable:

```bash
hmn vault rename notes --new-name scratch
```

---

## 7. `vault rescan` — force a full directory walk

```bash
hmn vault rescan scratch --yes
```

Without `--yes`, the CLI prompts on stderr:

```
Rescan vault 'scratch'? This will re-emit outbox events. (y/N)
```

The HTTP response carries a `rescan_initiated_at` timestamp:

```bash
hmn --json vault rescan scratch --yes | jq
```

The rescan itself runs asynchronously. For each file whose
`content_hash` differs from the stored value, an outbox `modified`
event is emitted; for files with stable hashes, no event is written.
For cold-start emission against every file regardless of hash, use
`reset --rebuild` instead (which clears every `files.content_hash`
first).

Rescan against a paused or errored vault is a silent no-op (returns
the row unchanged; no signal sent):

```bash
hmn vault pause scratch
hmn vault rescan scratch --yes   # no-op while paused
hmn vault resume scratch
```

---

## 8. `vault terminate` — remove a vault

Interactive prompt by default:

```bash
hmn vault terminate scratch
```

Stderr prompt:

```
Terminate vault 'scratch'? (y/N)
```

Answer `y` to proceed. With `--json` and an aborted prompt, the CLI
emits `{"terminated": false, "aborted": true}` to stdout.

Successful termination emits:

```json
{"terminated": true, "id": "<scratch's UUIDv7>"}
```

The daemon stops the watcher and indexer, removes the registry row,
and removes `<data_dir>/vaults/<scratch_id>/` (the vault path's own
files at `/tmp/hmn-scratch-vault/` are **never** touched).

Confirm:

```bash
hmn --json vault list | jq '.vaults | length'
```

Expect **2** again (`sample` and `sample-2`).

For non-interactive use (scripts), pass `--yes`:

```bash
mkdir -p /tmp/hmn-scratch-vault-2
hmn vault create --name scratch /tmp/hmn-scratch-vault-2
hmn vault terminate scratch --yes
```

### Terminate-then-create-with-same-name

```bash
hmn vault create --name scratch /tmp/hmn-scratch-vault
hmn vault terminate scratch --yes
hmn vault create --name scratch /tmp/hmn-scratch-vault
```

The third command succeeds — the previous `scratch`'s registry row
and per-vault subdirectory were both removed by the terminate, so
the second `create` allocates a fresh UUIDv7 and a fresh subdirectory.

Tear down the scratch artifacts before continuing:

```bash
hmn vault terminate scratch --yes
rm -rf /tmp/hmn-scratch-vault /tmp/hmn-scratch-vault-2
```

`hmn vault list` should now show exactly the two runbook fixtures.

---

## 9. Concurrency

Operations on the **same** vault serialize at the daemon (per-vault
`op_lock`); operations on **different** vaults run in parallel.
Spot-check with two pauses dispatched concurrently:

```bash
hmn vault pause sample &  hmn vault pause sample-2
wait
```

Both succeed; both vaults end up `paused`. Resume:

```bash
hmn vault resume sample &  hmn vault resume sample-2
wait
```

Both end up `active`. Same-vault concurrent ops serialize; the second
caller blocks briefly on the per-vault op_lock before its op begins.

---

## Pass criteria summary

If everything above lined up, the round-3 vault-management surface is
healthy:

- `vault list` and `vault status` agree on two registered vaults
  (`sample` and `sample-2`), both `active`, with stable surrogate IDs.
- `vault create` round-trips through registry insert + per-vault
  subdirectory creation + watcher start; rejected paths (nonexistent,
  registered-elsewhere, name-conflict) produce the documented error
  envelopes.
- `pause` / `resume` / `reset` / `rename` / `rescan` / `terminate`
  each transition the registry as documented and either survive the
  daemon restart (state changes are durable in `vaults.sqlite`) or
  no-op idempotently.
- Destructive ops (`terminate`, `reset --rebuild`, `rescan`) prompt
  on stderr unless `--yes` is passed.
- Closest-name hints fire on `vault_not_found` typos.
- Concurrent ops on different vaults run in parallel; concurrent ops
  on the same vault serialize.

The same surface is reachable over MCP — see [`04-mcp.md`](./04-mcp.md)
and [`06-mcp-http.md`](./06-mcp-http.md). Where the CLI exits non-zero
on error, the MCP transports return a structured error envelope at
`result.structuredContent.error.code`; the codes are identical
(`vault_not_found`, `vault_path_conflict`, `vault_name_conflict`,
`vault_path_invalid`, `vault_errored`, `internal`).

You're ready for [`06-mcp-http.md`](./06-mcp-http.md).
