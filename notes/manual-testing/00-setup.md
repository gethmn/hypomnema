# 00 · Setup

> Applies to: steps 1–7. Prereqs: a clone of the repo, Docker (or
> equivalent) for TEI, internet access for the sqlite-vec download.

This doc gets you to a state where `hmnd` and `hmn` exist as binaries,
the sqlite-vec extension is on disk where the daemon expects it, an
embedding service is reachable on `localhost:8080`, and a config file
points at the fixture vault. Subsequent docs assume this is done.

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

## 3. sqlite-vec extension

The daemon loads `sqlite-vec` as a dynamic library at runtime. It is
**not bundled**. Download a prebuilt artifact for the platform from
<https://github.com/asg017/sqlite-vec/releases>.

Place the file at the default path:

```bash
mkdir -p ~/.local/share/hypomnema
# macOS:
mv ~/Downloads/vec0.dylib ~/.local/share/hypomnema/sqlite-vec.dylib
# Linux:
mv ~/Downloads/vec0.so    ~/.local/share/hypomnema/sqlite-vec.so
# Windows:
mv ~/Downloads/vec0.dll   ~/.local/share/hypomnema/sqlite-vec.dll
```

Or override with an env var (takes precedence over the config path):

```bash
export HYPOMNEMA_VEC_EXT_PATH=/some/other/path/sqlite-vec.dylib
```

If the file is missing at startup, the daemon exits with a structured
error naming both the configured path and the env-var override
(`src/store/mod.rs:65-72`, exit code 1).

## 4. Embedding service (TEI)

Required for the indexing path that produces `chunks` rows (step 6) and
for `/search/semantic` (step 7). The daemon does **not** require it to
start — see step 5 below for the boot path with TEI down — but search
results that depend on embeddings will be empty or 503 until it's up.

### Bring TEI up

CPU image (works on any host; slower):

```bash
docker run --rm -p 8080:80 \
  ghcr.io/huggingface/text-embeddings-inference:cpu-latest \
  --model-id nomic-ai/nomic-embed-text-v1.5
```

GPU images and other model IDs are available in the TEI README. Whatever
you pick, the daemon's `embedding.dimension` config must match the
model's output dimension (768 for `nomic-embed-text-v1.5`, the
Hypomnema default).

### Smoke-check TEI

```bash
curl -s -X POST http://127.0.0.1:8080/v1/embeddings \
  -H 'Content-Type: application/json' \
  -d '{"model":"nomic-embed-text-v1.5","input":["hello"]}' \
  | jq '.data[0].embedding | length'
```

Expect `768`. If you see `404`, the model isn't loaded; if connection
refused, the container isn't up.

## 5. Minimal config

Default location: `~/.config/hypomnema/config.toml` (override with
`-c` / `--config` or `HYPOMNEMA_CONFIG`).

Replace `<ABSOLUTE_PATH_TO_REPO>` with the absolute path to your local
clone of this repo:

```toml
vault = "<ABSOLUTE_PATH_TO_REPO>/notes/manual-testing/fixtures/sample-vault"

[http]
bind = "127.0.0.1:7777"

[embedding]
endpoint = "http://127.0.0.1:8080/v1/embeddings"
model = "nomic-embed-text-v1.5"
dimension = 768

[logging]
level = "info"
```

All other keys take defaults — see
[`docs/reference/configuration.md`](../../docs/reference/configuration.md)
for the full schema. (That reference is at v0.2.0 and shows
`default_vault_name` instead of `vault`; the shipped code in this
working directory still uses the top-level `vault` key. Use the form
above.)

## 6. Validate the config

```bash
hmnd config-validate
```

Expect exit 0 and no errors. Common failures:

- **vault path doesn't exist or isn't a directory** — fix the absolute
  path in the config.
- **`storage.data_dir` is under `vault`** — ADR-0006 forbids it; pick a
  different `data_dir` (default `~/.local/share/hypomnema` is fine).

## 7. Stale state from prior runs

If you've run `hmnd` against a different vault or with a different
embedding dimension, the existing index will conflict. Reset:

```bash
rm -rf ~/.local/share/hypomnema/index.sqlite \
       ~/.local/share/hypomnema/index.sqlite-wal \
       ~/.local/share/hypomnema/index.sqlite-shm \
       ~/.local/share/hypomnema/outbox.jsonl
```

Do **not** delete `sqlite-vec.dylib` (or `.so` / `.dll`) — that's the
extension you just installed.

You're ready for [`01-running-the-daemon.md`](./01-running-the-daemon.md).
