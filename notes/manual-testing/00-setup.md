# 00 · Setup

> Applies to: round 4 / step 12 (multi-vault registry + HTTP-MCP).
> Prereqs: a clone of the repo, Docker (or equivalent) for TEI.

This doc gets you to a state where `hmnd` and `hmn` exist as binaries,
an embedding service is reachable on `localhost:8080`, and a config file
points at an empty `<data_dir>` ready for the runbook's two fixture
vaults to be registered. Subsequent docs assume this is done.

sqlite-vec is statically linked into `hmnd` via the `sqlite-vec` Rust
crate — there is no separate extension file to download or place on
disk.

All commands assume the working directory is the repository root unless
noted.

## 1. Toolchain

Rust toolchain is pinned by `rust-toolchain.toml`. Sanity check:

```bash
cargo --version
rustc --version
```

Both should resolve without prompting for installation. If they don't,
let `rustup` install whatever the toolchain file requests, then retry.

## 2. Build both binaries

```bash
cargo build --release --bin hmnd --bin hmn
```

Resulting binaries:

- `target/release/hmnd` — the daemon
- `target/release/hmn` — the CLI

Add `target/release/` to `PATH` for this shell, or invoke the binaries
by full path / via `cargo run --release --bin <name>`. The rest of this
runbook uses bare `hmnd` / `hmn`.

Supplemental sanity checks (optional):

```bash
just check         # cargo check --all-targets
just lint          # cargo clippy --all-targets -- -D warnings
cargo nextest run  # full test suite
```

## 3. Embedding service (TEI)

Required for the indexing path that produces `chunks` rows and for
`/search/semantic`. The daemon does **not** require it to start — see
step 5 below for the boot path with TEI down — but search results that
depend on embeddings will be empty or 503 until it's up.

### Bring TEI up

CPU image (works on any host; slower):

```bash
docker run --rm -p 8080:80 \
  ghcr.io/huggingface/text-embeddings-inference:cpu-latest \
  --model-id nomic-ai/nomic-embed-text-v1.5
```

GPU images and other model IDs are available in the TEI README.
Whatever you pick, the daemon's `embedding.dimension` config must
match the model's output dimension (768 for `nomic-embed-text-v1.5`,
the Hypomnema default).

### Smoke-check TEI

```bash
curl -s -X POST http://127.0.0.1:8080/v1/embeddings \
  -H 'Content-Type: application/json' \
  -d '{"model":"nomic-embed-text-v1.5","input":["hello"]}' \
  | jq '.data[0].embedding | length'
```

Expect `768`. If you see `404`, the model isn't loaded; if connection
refused, the container isn't up.

## 4. Minimal config

Default location: `~/.config/hypomnema/config.toml` (override with
`-c` / `--config` or `HYPOMNEMA_CONFIG`).

The runbook drives a two-vault setup; both vaults are registered at
runtime via `hmn vault create` (see step 7 below). The config file
itself does **not** name any vault — vaults are runtime state per
[ADR-0010](../../docs/decisions/0010-vault-definitions-as-runtime-state.md).
The single ergonomic knob is `default_vault_name`, used by `hmn vault
create` (when `--name` is omitted) and by `hmn vault status` (when
the selector is omitted).

```toml
default_vault_name = "sample"

[http]
bind = "127.0.0.1:7777"

[mcp.http]
enabled = true
path = "/mcp"

[embedding]
endpoint = "http://127.0.0.1:8080/v1/embeddings"
model = "nomic-embed-text-v1.5"
dimension = 768

[logging]
level = "info"
```

All other keys take defaults — see
[`docs/reference/configuration.md`](../../docs/reference/configuration.md)
for the full schema. `mcp.http.enabled = true` is the shipped default;
the explicit line above documents the round-4 transport for clarity
and also serves as the toggle point for the disabling exercise in
[`06-mcp-http.md`](./06-mcp-http.md).

> **Legacy `[vault]` block**: pre-round-3 configs that still carry a
> top-level `vault = "..."` block continue to parse. On a fresh start
> with an empty `vaults.sqlite` and a populated `[vault]` block, the
> daemon auto-migrates the legacy state (renames the four legacy files
> under `<data_dir>/vaults/<id>/`, inserts a registry row using
> `default_vault_name`) and logs a deprecation `WARN`. Once
> `vaults.sqlite` is populated the warning stops. Remove the `[vault]`
> block from the config when convenient — see
> [`configuration.md` § Legacy `[vault]` block migration](../../docs/reference/configuration.md#legacy-vault-block-migration).
> The runbook below assumes you start from an empty `vaults.sqlite` and
> register the two fixture vaults explicitly.

## 5. Validate the config

```bash
hmnd config-validate
```

Expect exit 0 and no errors. Common failures:

- **`default_vault_name` is empty after trimming whitespace** —
  configuration error; either set a name or accept that every vault
  command will require an explicit selector.
- **`mcp.http.path` is anything other than `/mcp`** — rejected at
  startup with the message `mcp.http.path must be "/mcp" in this
  version of Hypomnema`.

## 6. Stale state from prior runs and registering the fixture vaults

If you've run an older `hmnd` against a different vault or with a
different embedding dimension, the existing data dir will conflict.
Reset to an empty `<data_dir>` so the runbook can drive a clean
two-vault setup:

```bash
rm -rf ~/.local/share/hypomnema/vaults.sqlite \
       ~/.local/share/hypomnema/vaults.sqlite-wal \
       ~/.local/share/hypomnema/vaults.sqlite-shm \
       ~/.local/share/hypomnema/vaults \
       ~/.local/share/hypomnema/index.sqlite \
       ~/.local/share/hypomnema/index.sqlite-wal \
       ~/.local/share/hypomnema/index.sqlite-shm \
       ~/.local/share/hypomnema/outbox.jsonl
```

The first three lines drop the registry; the fourth wipes any
per-vault subdirectories; the last three lines clean up legacy v0
state if it was ever present (so the auto-migration doesn't re-engage
on the next start).

Start the daemon (it will idle with zero registered vaults):

```bash
hmnd
```

In a second terminal, register the runbook's two fixture vaults.
Replace `<ABS>` with the absolute path to your local clone of this
repo:

```bash
hmn vault create --name sample   <ABS>/notes/manual-testing/fixtures/sample-vault
hmn vault create --name sample-2 <ABS>/notes/manual-testing/fixtures/sample-vault-2
```

Each command returns the new vault row — surrogate ID, name,
canonicalized path, status (`active`), creation timestamp. The daemon
creates `<data_dir>/vaults/<vault_id>/` for each vault, runs the
initial scan, and logs an indexing summary per vault. After both
commands return, expect 7 + 10 = 17 indexed files across the two
vaults.

`hmn vault list` should now show both rows. You're ready for
[`01-running-the-daemon.md`](./01-running-the-daemon.md).
