# Step 1 Workplan — Skeleton

**Roadmap step**: [Step 1 — Skeleton](./roadmap.md#step-1--skeleton)
**Status**: drafted, awaiting review
**Created**: 2026-04-25

---

## Goal recap

`hmnd` starts from a TOML config, initializes tracing, logs what it's watching,
handles SIGINT/SIGTERM via a cancellation token, and exits cleanly. `hmn` parses
CLI args via clap so `--help` / `--version` render and the subcommand surface is
shaped for steps 2–5 (even though most subcommands stub out for now). One smoke
test per binary.

No watcher, no SQLite, no HTTP server, no outbox in this step. The scaffolding
those will land into is what step 1 builds.

## Deferred-decision resolutions

The roadmap flagged three TBDs to be resolved here. Each is settled below with a
proposal and rationale; the build follows the resolutions.

### 1. CLI subcommand naming

The `hmn start / hmn scan / hmn search / hmn status` shape from
[`vision.md` line 115](../product/vision.md) predates [ADR-0008](../decisions/0008-two-binary-daemon-plus-cli.md).
With the daemon split into its own binary, `start` and `scan` belong on `hmnd`,
not `hmn`. The draft [`reference/cli.md`](../reference/cli.md) already encodes
the post-ADR-0008 shape — adopt it as final for v0:

**`hmnd`** (daemon binary)
- *no subcommand* → start the daemon in the foreground (the common case)
- `hmnd scan` → one-shot reindex without starting servers (lands in step 2/3)
- `hmnd config-validate` → parse + validate config, exit 0/3 (lands in step 1)

**`hmn`** (CLI client)
- `hmn search <mode> <query>` where `<mode>` ∈ `filesystem | content | semantic`
  (clap nested subcommand, so `hmn search filesystem ...`, `hmn search content
  ...`, `hmn search semantic ...` all render in `--help`)
- `hmn status`

**Global flags on both**: `--config / -c`, `--verbose / -v` (repeatable), `--help`,
`--version`. `hmn` additionally has `--daemon-url` and `--json`.

**Why this shape**:
- Mirrors `sshd / ssh`, `dockerd / docker` (the convention ADR-0008 leans on).
- `hmn search <mode>` as a nested subcommand keeps each search mode's flags
  scoped to its own help block — easier to document, easier for clap-derive to
  validate, lines up with the three peer modes from
  [ADR-0004](../decisions/0004-three-search-modes-as-peers.md).
- `hmnd config-validate` lets us land config validation logic in step 1 with a
  user-visible surface (rather than a hidden `--check-config` flag).
- Stays compatible with the `hmnd --mcp-stdio` flag ADR-0008 contemplates;
  that's a flag on the default daemon action, not a subcommand.

**Step-1 scope**: wire all of the above through `clap` derive so `--help` /
`--version` render the full shape. The daemon's default action runs the (still
trivial) startup loop. `hmnd config-validate` is fully implemented. `hmnd scan`
returns a "not implemented in step 1" error and exits non-zero. `hmn search` /
`hmn status` are stubbed the same way — they parse args and print "not
implemented yet (step 5)" to stderr, exiting 1. Stubs make the help text honest
without dragging step 5 forward.

### 2. TOML config schema

The schema documented in [`reference/configuration.md`](../reference/configuration.md)
was drafted with all eight v0 steps in mind. Adopt it verbatim as the canonical
schema; step 1 *parses* the whole thing but only *uses* the subset relevant to
the skeleton (vault path, data_dir, logging levels, http.bind for the
about-to-listen-here log line).

**Top-level keys** (final for v0):

```toml
vault = "/path/to/vault"             # required, no default

[http]
bind = "127.0.0.1:7777"              # default

[mcp]
transport = "stdio"                  # default; "stdio" | "socket"
socket = "~/.local/share/hypomnema/mcp.sock"  # default; only used if transport = "socket"

[embedding]
endpoint = "http://127.0.0.1:8080/v1/embeddings"  # required when step 6 lands; in step 1 we accept the default
model = "nomic-embed-text-v1.5"      # default
dimension = 768                      # default
api_key = ""                         # default

[watcher]
debounce_ms = 400                    # default
ignore_patterns = [                  # default
  ".obsidian/**",
  ".trash/**",
  "*.sync-conflict-*",
  "**/*.tmp",
]

[storage]
data_dir = "~/.local/share/hypomnema"  # default (see path-resolution below)
index_file = "index.sqlite"            # default; relative to data_dir
outbox_file = "outbox.jsonl"           # default; relative to data_dir

[logging]
level = "info"                       # default
notify_level = "warn"                # default
tokio_level = "error"                # default
```

**Validation rules implemented in step 1**:
- `vault` is required, must exist, must be a directory, must be readable.
- `storage.data_dir` must not be under `vault` — the
  [ADR-0006](../decisions/0006-outbox-outside-watched-directory.md) invariant,
  enforced as an absolute-path comparison after canonicalization.
- `logging.level` / `notify_level` / `tokio_level` must parse as
  `tracing::Level`.
- Unknown keys reject (`#[serde(deny_unknown_fields)]` on every struct) — typos
  fail fast rather than silently using defaults.

Validation rules *deferred to later steps* (parser tolerates them, no semantic
checks yet): `embedding.dimension` matches schema (step 6/7), `mcp.socket`
parent writable when `transport = "socket"` (step 8), `watcher.debounce_ms`
sanity bounds (step 3).

**Path resolution**:
- Leading `~` expands to `$HOME`. No `${VAR}` expansion in v0 — keep it simple.
- Relative paths in `vault` and `storage.data_dir` resolve against the current
  working directory at config-load time.
- After expansion, `vault` is canonicalized (via `std::fs::canonicalize`) so the
  data-dir-not-under-vault check uses a normalized form. `storage.data_dir`
  isn't canonicalized at parse time (it may not exist yet); we compare
  ancestry against the canonical vault using a string-prefix check after
  resolving symlinks on the parts that exist.

**Default config path**:
- `$HYPOMNEMA_CONFIG` if set, else `$XDG_CONFIG_HOME/hypomnema/config.toml`,
  else `$HOME/.config/hypomnema/config.toml`.
- On macOS we deliberately use the Linux XDG layout (`~/.config/...`,
  `~/.local/share/...`), matching what
  [`reference/configuration.md`](../reference/configuration.md) already
  documents. Reasoning: keeps platform branches out of the path code, matches
  what most `tracing`/`serde`-shaped Rust CLIs do on macOS, and
  `~/Library/Application Support` adds nothing for a single-user developer
  daemon.
- No new dep for path resolution — roll our own via `std::env::var` to honor the
  roadmap's "no new deps" constraint. If this becomes painful in step 2 we'll
  add `dirs` then; called out so we don't fight it.

**Default config bootstrapping**:
- If the config path doesn't exist, *don't* auto-create one — fail with a
  message pointing at `reference/configuration.md` and showing the path that
  was tried. Auto-creating a default file would write to `~/.config` without
  the user asking, and worse, would let a missing `vault =` key turn into a
  silent default at write time.

### 3. Default logging verbosity per module

Adopt the [`vision.md` line 113](../product/vision.md) lean as final, with the
per-binary tweak that's natural for a daemon-vs-CLI split:

**`hmnd` (daemon)** — defaults from `[logging]`:
- `hypomnema = info` (everything we write)
- `notify = warn` (chatty crate, see Pitfall #2)
- `tokio = error` (suppress runtime traffic)

**`hmn` (CLI client)** — much quieter; the user wants results, not chatter:
- `hypomnema = warn` (only print warnings/errors from our code)
- everything else default-`error`

**`-v` / `--verbose` (both binaries)**:
- `-v` bumps the `hypomnema` filter one level (info → debug, or warn → info on
  the CLI).
- `-vv` bumps it two (→ trace, or → debug on the CLI).
- `-vvv` is trace on both.
- Other crates' filters are not affected by `-v` — those stay at their config
  values. `RUST_LOG` overrides the whole filter when set, since that's the
  conventional escape hatch.

**Output**:
- stdout via `tracing-subscriber`'s `fmt` layer.
- Compact format on a TTY; JSON-per-line when `HYPOMNEMA_LOG_FORMAT=json` is
  set (cheap to add now, useful when a supervisor captures logs). No file
  output yet — the "log to a file in `data_dir`" line in
  [`architecture/overview.md`](../architecture/overview.md) is deferred until
  there's something other than the startup banner to capture.

The `[logging]` config table's three keys (`level`, `notify_level`,
`tokio_level`) compose into a single `tracing_subscriber::EnvFilter` directive
of the form `hypomnema={level},notify={notify_level},tokio={tokio_level}` at
process start.

---

## Tasks (ordered, each independently mergeable)

Each task lands as its own commit, and the set is intended to be a single PR
unless any one balloons. Granularity follows the per-logical-unit instinct from
[`project-planning-workflow-notes.md` open question 1](../../notes/project-planning-workflow-notes.md#open-questions-about-the-workflow-itself).

### Task 1.1 — Config module: parse + defaults + validation

**Files**:
- `src/config.rs` (new)
- `src/lib.rs` (export `pub mod config;`)
- `tests/config.rs` (new — integration-shaped unit tests for the parser)

**What lands**:
- `Config` struct with the full schema above, `#[derive(Deserialize, Debug)]`,
  `#[serde(deny_unknown_fields)]` on every struct.
- `Config::load(path: Option<&Path>) -> anyhow::Result<Config>`:
  - resolves the default path (env > XDG > HOME) when `path` is `None`
  - reads the file, deserializes via `toml`
  - runs `validate()` (vault exists, vault is dir, data_dir not under vault,
    logging levels parse)
- Tilde expansion helper, `expand_tilde(p: &Path) -> PathBuf`, used by the
  `Deserialize` impl on a thin newtype `ConfigPath(PathBuf)` so every path
  field gets the same treatment.
- A `Config::default_for_smoke_test()` constructor (test-only, behind
  `#[cfg(test)]`) that returns a struct populated entirely with defaults plus a
  caller-provided `vault`. Useful for the binary smoke tests.

**Why a separate module first**: every other module reads from it. Landing it
first lets binary stubs `use hypomnema::config::Config` and the test crate
exercise validation without spinning up the rest of the daemon.

### Task 1.2 — Logging module: tracing init from config + verbosity

**Files**:
- `src/logging.rs` (new)
- `src/lib.rs` (export `pub mod logging;`)

**What lands**:
- `init(config: &LoggingConfig, verbose: u8, binary: BinaryKind) -> anyhow::Result<()>`
  — composes the per-module filter, honors `RUST_LOG` if set, picks the JSON
  formatter when `HYPOMNEMA_LOG_FORMAT=json`, installs the subscriber.
- `BinaryKind::Hmnd` vs `::Hmn` chooses the daemon-vs-client default base
  filter (resolved decision #3 above).
- Idempotent for tests — uses
  `tracing::subscriber::set_global_default` and tolerates
  AlreadySetError (returns Ok, since tests in the same process may collide).

**Why separate from `hmnd.rs`**: both binaries use it. Step 5 will call it from
`hmn.rs`. Putting it in the lib avoids duplicating the EnvFilter composition.

### Task 1.3 — Shutdown signal helper

**Files**:
- `src/shutdown.rs` (new)
- `src/lib.rs` (export)

**What lands**:
- `pub fn install() -> tokio::sync::watch::Receiver<bool>` — spawns a task
  that listens for SIGINT and (on Unix) SIGTERM via `tokio::signal`, flips the
  watch channel to `true` on first signal, logs at `info`.
- Second signal aborts the process with `std::process::exit(130)` so a stuck
  shutdown can be force-killed via Ctrl+C twice — a convention worth getting
  right early because Pitfall #9 is about *clean* shutdown, but a hung clean
  shutdown still needs an out.

**Why a module**: every long-running task in steps 2–5 will accept this
receiver. Pitfall #9 (ungraceful shutdown) is on the named-hazards list; the
shape we choose here propagates everywhere.

### Task 1.4 — `hmnd` binary wired up

**Files**:
- `src/bin/hmnd.rs` (replace placeholder)

**What lands**:
- `clap` derive `Cli` with `--config` / `-c`, `--verbose` / `-v` (count),
  `--version`, optional subcommand enum:
  - default (no subcommand) → `run_daemon`
  - `Scan` → `bail!("hmnd scan: not implemented yet (lands in step 2)")`
  - `ConfigValidate` → load config, exit 0; on error, anyhow propagates and
    `main` maps it to exit code 3.
- `#[tokio::main]` async `main()`:
  - parses CLI
  - loads config
  - inits logging
  - dispatches subcommand
- `run_daemon`:
  - logs a one-line startup banner: vault path, data_dir, http.bind
    (the address it *will* listen on once step 5 lands), pid
  - logs the full config at `debug`
  - awaits the shutdown receiver
  - on shutdown: logs `"hmnd: shutdown signal received, exiting cleanly"` at
    `info`, returns `Ok(())`
- `main` maps anyhow errors to exit codes per
  [`reference/cli.md`](../reference/cli.md):
  - configuration error → 3
  - other → 1
  - clap parse error → 2 (clap handles this for us)

### Task 1.5 — `hmn` binary wired up

**Files**:
- `src/bin/hmn.rs` (replace placeholder)
- `src/cli.rs` (new — shared `hmn` CLI definition; one place clap-derive lives
  for the client surface)
- `src/lib.rs` (export `pub mod cli;` for testability)

**What lands**:
- Top-level `Cli` with global flags (`--config`, `--daemon-url`, `--verbose`,
  `--json`) and a subcommand enum: `Search { mode: SearchMode, query: String,
  prefix: Option<String>, limit: Option<usize> }`, `Status`.
- `SearchMode` is a clap-derived enum with three variants
  (`Filesystem`, `Content`, `Semantic`) so `hmn search --help` lists them.
- Both subcommands print `"hmn: not implemented yet (lands in step 5)"` to
  stderr and exit 1 — but they parse cleanly, so `--help`, `--version`, and
  arg validation all render correctly.
- `main` is `#[tokio::main]`-free for now (no async work in step 1); step 5
  will switch when `reqwest` arrives.

**Why a `cli.rs` module**: the clap definitions are testable in isolation
(parse-arg unit tests). Splitting them out also gives step 5 something to
import from when the actual handlers land.

### Task 1.6 — Smoke tests + Justfile sanity

**Files**:
- `tests/skeleton.rs` (new)
- `tests/config.rs` (extends from task 1.1)
- `Justfile` (no changes expected, but verify `just check` / `just test` / `just
  lint` all pass)

**What lands**:
- `tests/skeleton.rs`:
  - `hmn --help` succeeds (uses `assert_cmd` or `std::process::Command` against
    the built binary — pick `std::process::Command` to avoid a new dep, since
    the roadmap forbids them).
  - `hmn --version` succeeds.
  - `hmnd --help` succeeds.
  - `hmnd config-validate --config <tmp>` succeeds against a hand-rolled
    minimal valid config in a tempdir.
  - `hmnd config-validate` fails with exit 3 against a config whose `vault`
    doesn't exist.
  - `hmnd config-validate` fails with exit 3 when `storage.data_dir` is under
    `vault`.
- `tests/config.rs`:
  - default round-trip (parse a minimal `vault = "..."` config, defaults fill
    in)
  - unknown-key rejection
  - tilde expansion
  - `data_dir`-under-`vault` rejection
  - logging level parse error rejection

`tempfile` is not in `Cargo.toml` and the roadmap says "no new deps". Worked
around by hand-rolling tmpdirs via `std::env::temp_dir()` + a uniquified
subdir. If this becomes annoying we add `tempfile` in step 2 (it's a strict
test-only dep and is already implied by step 2's "use `tempfile::TempDir`" line
in `AGENTS.md`). Flagging here so we don't get surprised.

The "binary smoke test runs the built binary" pattern needs the test to know
the binary path. `cargo test` sets `CARGO_BIN_EXE_<name>` env var for tests in
the same crate — we can rely on that without `assert_cmd`.

### Task 1.7 — Update reference docs to drop "TBD" markers

**Files**:
- `docs/reference/cli.md` — flip "draft" header, remove "subcommand names not
  finalized" warning, mark resolved.
- `docs/reference/configuration.md` — same treatment for "format not finalized."
- `docs/roadmap/roadmap.md` — add `**Status**: shipped <date>` to the Step 1
  section per the step-boundary ritual in
  [`project-planning-workflow-notes.md`](../../notes/project-planning-workflow-notes.md#step-boundary-ritual)
  *(this happens at the end of the step, not the start)*.

**What does *not* land here**: ADRs. The three deferred decisions resolved
above are encoded in this workplan and in the reference docs they update.
None of them are load-bearing enough to outlive the step (per the
"workplan turns each TBD into either a code-level decision or a new ADR"
rubric in
[`project-planning-workflow-notes.md`](../../notes/project-planning-workflow-notes.md#tbd-handling)).
If review surfaces a different read on any of the three, that's the moment to
promote it to an ADR.

---

## Test strategy

**Unit tests** (`#[cfg(test)]` next to the code):
- `config.rs`: parser shapes, validation predicates, tilde expansion.
- `cli.rs`: clap parse for representative argv vectors; `--help` doesn't panic.
- `logging.rs`: filter-string composition (a pure function — no subscriber
  install in unit tests).
- `shutdown.rs`: not unit-tested in step 1 (signal handling is hard to test
  without process forks; covered indirectly by the integration smoke).

**Integration tests** (`tests/`):
- `tests/config.rs`: real TOML files written into a tempdir, parsed end-to-end
  through `Config::load`.
- `tests/skeleton.rs`: shell out to the built `hmnd` and `hmn` binaries,
  assert exit codes and that `--help` produces non-empty output.
- A "daemon idles and exits cleanly on signal" test is *not* in step 1 — it
  needs cross-process signal delivery and adds CI flakiness. Defer to step 3
  when the watcher is the thing actually being kept alive.

**Lint and format**: `cargo clippy --all-targets -- -D warnings` and
`cargo fmt --all -- --check` both pass before opening review.

---

## Definition of done

- [ ] `cargo run --bin hmnd -- --config <path>` against a valid config logs the
      vault path, data_dir, and bind address, then idles.
- [ ] Sending SIGINT or SIGTERM to the running daemon causes it to log a clean
      shutdown line and exit 0.
- [ ] `hmn --help` renders without error and lists `search` and `status`.
- [ ] `hmn search --help` lists `filesystem`, `content`, `semantic`.
- [ ] `hmnd --help` renders without error and lists `scan` and `config-validate`.
- [ ] `hmnd config-validate --config <tmp-with-bad-vault>` exits 3 with a
      message naming the vault path.
- [ ] `cargo test` passes (smoke tests + config tests).
- [ ] `cargo clippy --all-targets -- -D warnings` passes.
- [ ] No new dependencies added to `Cargo.toml`.
- [ ] `docs/reference/cli.md` and `docs/reference/configuration.md` drop their
      "draft" warnings and reflect the resolved decisions.
- [ ] Step 1 marked shipped in `docs/roadmap/roadmap.md` with the ship date.
- [ ] Step 1 retrospective appended to
      `notes/project-planning-workflow-notes.md` (one paragraph minimum).

---

## Cross-references

**Specs / decisions**:
- [ADR-0002: Rust over Python](../decisions/0002-rust-over-python.md) — language
- [ADR-0006: Outbox outside watched directory](../decisions/0006-outbox-outside-watched-directory.md)
  — `data_dir`-under-`vault` rejection
- [ADR-0008: Two binaries (hmnd + hmn)](../decisions/0008-two-binary-daemon-plus-cli.md)
  — binary shape

**Reference docs (updated by this step)**:
- [CLI reference](../reference/cli.md)
- [Configuration reference](../reference/configuration.md)

**Pitfalls touched**:
- #9 *Ungraceful shutdown and torn writes* — the shutdown helper from task 1.3
  is the foundation every long-running task in steps 2–5 will use.

**Skills**: none of the load-bearing skills apply yet —
- `rusqlite-in-async` lands with step 2
- `filesystem-watching` lands with step 3
- `sqlite-vec-extension` lands with step 6
- `markdown-chunking` lands with step 6

---

## Out of scope (will not appear in this PR)

- SQLite store, schema, migrations
- The watcher and `notify` integration
- Outbox writer
- HTTP server (Axum), MCP server (rmcp)
- `reqwest` client
- Anything that talks to the embedding service
- Auto-creating a default config file on first run
- File-based log output (stdout only in step 1)
- A `daemon idles cleanly under signal` cross-process integration test (deferred
  to step 3)

If review surfaces a strong reason to pull any of the above forward, that's a
roadmap revision — see the
[mid-step roadmap revision](../../notes/project-planning-workflow-notes.md#open-questions-about-the-workflow-itself)
open question.
