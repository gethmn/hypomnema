# Hypomnema vs. qmd

Comparison of [Hypomnema](https://github.com/gethmn/hypomnema) and [qmd](https://github.com/tobi/qmd) as of 2026-04-28. Both tools occupy the same neighborhood — a local index over a Markdown directory exposed to AI agents via MCP — but they make almost opposite design choices around **freshness model**, **retrieval sophistication**, and **deployment shape**.

> **Source**: Generated via Amp librarian probe of `tobi/qmd` `main` branch on 2026-04-28; cross-referenced against the Hypomnema tree at `v0.3.0`. Captured for orientation when answering "how does this differ from qmd?" or evaluating cross-pollination opportunities.

---

## Where they overlap

- **Vault-agnostic Markdown indexing.** Neither hardcodes Obsidian. Both treat the watched/indexed directory as just "a folder of `.md` files." qmd extends this to code files; Hypomnema is `.md`-only by spec.
- **MCP as a first-class consumer surface.** Both ship a real MCP server (qmd via `@modelcontextprotocol/sdk`, Hypomnema via [rmcp](../Cargo.toml)) with tool routes for search. Both expose Streamable HTTP MCP transports for browser-hosted/remote agents.
- **SQLite + sqlite-vec for vector storage.** Identical foundation — Hypomnema is bound by [ADR-0007](../docs/decisions/0007-sqlite-vec-immutable-dimension.md) to immutable dimensions; qmd does the same via its bundled `vectors_vec` virtual table.
- **Lexical search via SQLite-native primitives.** qmd uses FTS5 with Porter stemming; Hypomnema does substring/regex matching against `files.content` ([`src/search/content.rs`](../src/search/content.rs)). Same database, different lexical strategy.
- **Pluggable embedding models.** Both isolate the embedding backend behind a clean boundary — Hypomnema's [`Embedder` trait](../src/embedding.rs), qmd's GGUF/node-llama-cpp wrapper.

## Where they diverge

### Freshness: watch-and-push vs. pull-and-batch

The single largest design split. Hypomnema is built around `notify-debouncer-full` — a long-running daemon whose job is to *react* to filesystem changes and emit a [change-event outbox](../src/outbox/writer.rs). qmd has **no watcher at all**: indexing is explicit (`qmd update` + `qmd embed`), with cron or `update:` shell hooks doing the freshness work. qmd's HTTP daemon mode keeps *models* loaded in VRAM, not the *index* in sync. Hypomnema's [outbox](../docs/specs/change-events.md) is a capability qmd entirely lacks.

This is a genuine philosophical difference, not just a missing feature. qmd's model is "search this set as it was when you last ran update"; Hypomnema's model is "this set, live."

### Retrieval sophistication

qmd is much richer here:

| Capability | qmd | Hypomnema |
|---|---|---|
| BM25 / FTS | Yes (FTS5 + Porter) | No (substring/regex only) |
| Vector kNN | Yes | Yes |
| HyDE | Yes | No |
| LLM query expansion | Yes (custom fine-tuned 1.7B model) | No |
| Cross-encoder rerank | Yes (Qwen3-Reranker 0.6B) | No |
| RRF fusion + position-aware blend | Yes | No |

qmd has clearly invested in retrieval quality — including a **fine-tuned query-expansion model trained in-repo** under `finetune/`. Hypomnema's three search modes (filesystem / content / semantic) are mode-distinct and unfused; the agent picks one. That's deliberate (per the round-3 work on cross-vault search) but it means a Hypomnema query is dumber than a qmd query at equivalent index size.

### Deployment surface and weight

qmd ships **three** modes from one package — CLI, MCP server (stdio or HTTP daemon), and a TypeScript SDK (`createStore({ ... })`). Hypomnema is two binaries ([per ADR-0008](../docs/decisions/0008-two-binary-daemon-plus-cli.md)): the [`hmnd` daemon](../src/bin/hmnd.rs) and [`hmn` thin client](../src/bin/hmn.rs). There's no library-mode story — Hypomnema assumes a daemon is running.

Weight asymmetry is significant: qmd ships ~2 GB of GGUF models (embedding + reranker + query expansion) that auto-download on first use. Hypomnema delegates embedding to an external OpenAI-compatible HTTP service (configured in [`embedding.endpoint`](../src/config.rs)) — no local models in the daemon's address space. If your embedding service is `llama.cpp` on `localhost:8080`, the deployments end up similar in resource footprint; if it's a hosted endpoint, Hypomnema is much lighter.

### Multi-vault model

Hypomnema's [round-3 multi-vault work](backlog.md) is genuinely novel against qmd: one daemon manages N vaults, each with isolated runners ([`VaultRunner`](../src/control_plane/runner.rs)), separate stores ([`<data_dir>/vaults/<id>/index.sqlite`](../src/store/mod.rs)), separate outboxes, with cross-vault search-result fusion at the [API layer](../src/api/search.rs). qmd has "collections" — but they share one `index.sqlite` and one process; collections are a filter, not an isolation boundary. Hypomnema's lifecycle ops (`pause`, `resume`, `terminate`, `reset`, `rescan`, `rename`, in [`src/control_plane/manager.rs`](../src/control_plane/manager.rs)) have no qmd equivalent.

### Language and runtime philosophy

TypeScript (Node ≥22 or Bun) vs. Rust. qmd inherits npm's distribution story: `npm i -g @tobilu/qmd` and you're done. Hypomnema is build-from-source today (the release-automation [backlog item](backlog.md) is round-6+); the operator-facing complexity is higher.

## The shape of the split

```diagram
                    ╭─────────────────╮
                    │ Markdown vault  │
                    ╰────────┬────────╯
                             │
              ┌──────────────┴──────────────┐
              │                             │
              ▼                             ▼
      ╭───────────────╮            ╭───────────────╮
      │ qmd           │            │ Hypomnema     │
      │ ──────        │            │ ──────        │
      │ pull-batch    │            │ watch-push    │
      │ heavy local   │            │ light daemon  │
      │ retrieval     │            │ external      │
      │ sophistication│            │ embeddings    │
      │ no events     │            │ outbox events │
      │ collections   │            │ multi-vault   │
      │ as filter     │            │ as isolation  │
      │ CLI+MCP+SDK   │            │ daemon+client │
      ╰───────┬───────╯            ╰───────┬───────╯
              │                            │
              └──────────────┬─────────────┘
                             ▼
                  ╭────────────────────╮
                  │ MCP-speaking agent │
                  ╰────────────────────╯
```

If you're building one, you're not really competing with the other — qmd is a **search-quality** project (the fine-tuned query expansion model is the giveaway) that happens to expose MCP; Hypomnema is a **substrate-as-events** project (the outbox is the load-bearing piece) that happens to include search. They're solving adjacent but distinct problems against the same input.

## Cross-pollination opportunities

The clearest thing Hypomnema could borrow from qmd: **BM25/FTS5 for `search_content`** would be a real upgrade over substring/regex, with no architectural cost (FTS5 ships with bundled SQLite — the dependency is already in tree, see `rusqlite = { features = ["bundled", "load_extension"] }` in [`Cargo.toml`](../Cargo.toml)). Worth a backlog candidate when retrieval-quality work surfaces as load-bearing.

The clearest thing qmd could borrow from Hypomnema: a **watcher-driven freshness mode** would close its biggest user-experience gap. (Out of scope for us to act on.)

Other items worth noting but not acting on:

- **HyDE prompt mode** for semantic search — would be a `search_semantic` request-shape addition (the agent crafts a hypothetical answer paragraph; we embed *that* instead of the question). Pure agent-side workaround possible today; daemon-side support would just be a new `query_mode` field.
- **Cross-encoder reranking** — would require a new "rerank" service alongside the embedding service, and a re-ranking loop in the search pipeline. Heavy; firmly out of scope without a use-case.
- **Library / SDK mode** for Hypomnema — qmd's `createStore({ ... })` API is a real second consumer surface. Doesn't fit Hypomnema's daemon-as-substrate framing today, but worth noting if a Rust consumer ever wants in-process embedding.

---

## Reference table

| Dimension | qmd | Hypomnema |
|---|---|---|
| Language | TypeScript (Node/Bun) | Rust |
| Search modes | BM25, vector, HyDE, hybrid | Filesystem, content, semantic |
| Index freshness | Explicit pull (`qmd update` + `qmd embed`) | Filesystem watcher (daemon) |
| Change events / outbox | None | Yes — change-event outbox over HTTP/MCP |
| Daemon | Optional HTTP MCP server (search-only) | Always-on watcher daemon |
| MCP | Yes — full MCP server with tools + resources | Yes |
| HTTP API | Yes — `POST /mcp`, `POST /query`, `GET /health` | Yes |
| Library / SDK | Yes — published npm package | Daemon-oriented, not a library |
| Vault format | Agnostic (any directory + glob) | Agnostic (any Markdown dir) |
| LLM on-device | Yes — embedding + reranking + query expansion (GGUF) | Not intrinsic (search only) |
| Query expansion | Yes — custom fine-tuned model | No |
| Re-ranking | Yes — LLM cross-encoder | No |
| Vector store | sqlite-vec (bundled in SQLite) | sqlite-vec (loaded extension) |
| File types | Markdown primary; code files (TS/JS/Py/Go/Rust) with AST chunking | Markdown only |
| Obsidian coupling | None | None |
| Multi-instance / multi-vault | Collections (filter, single process) | Vaults (isolation, multi-runner per daemon) |
