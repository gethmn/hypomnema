# Hypomnema Backlog

Round-agnostic queue of work pulled into roadmaps as rounds are written. Adding to this file does **not** commit to a timeline. Round workplans pull from here when they're ready; new work drops in here when it's identified but not yet scoped to a round.

This file replaces the earlier "round 4+ / handoff doc § Out of scope" framing. Buckets are by theme, not by round number — the round number gets assigned (or stays unassigned) when a roadmap is written.

> **Conventions**
> - Each item is short. Deeper context lives in the source the item came from (retro, ADR, spec, vision doc). Always link.
> - When an item lands in a roadmap or is applied (for non-roadmap work like playbook edits), mark it shipped in place by wrapping the item label in `~~strikethrough~~` and prefixing or appending a lifecycle annotation such as **Pulled into round N** (`notes/roadmap/roadmap-N.md` § Round N) or **Shipped <date>**. Strikethrough-in-place is the default because it preserves historical context for future readers; outright removal is acceptable only when the item is genuinely obsolete and not worth a historical breadcrumb.
> - Live (un-shipped) items have no strikethrough and no lifecycle annotation. Anything with strikethrough or a "Pulled into round N" / "Shipped" annotation is **done** — do not surface it as a candidate for a future round.
> - Items can stay in this file indefinitely — un-scoped is a valid state.

---

## Round-5 candidates (pulled into round 5)

Items raised at the round-3/4 boundary and pulled into round 5. See [`notes/roadmap/archive/roadmap-5.md`](roadmap/archive/roadmap-5.md) for the full workstream scoping.

- ~~**CHANGELOG.md adoption.**~~ **Pulled into round 5** (step 15 — shipping gate).
- ~~**Outbox flake hardening (`rename_emits_deleted_then_created_lines`).**~~ **Superseded by planned outbox removal**; no longer a round-5 item.
- ~~**CI pipeline (GitHub Actions).**~~ **Pulled into round 5** (step 13 — `ci.yml` + `dependabot.yml` + spec promotion). Source proposal: `notes/proposals/ci-cd-pipeline.md`.

## Round-6 candidates (pulled into roadmap-6.md)

Items pulled into the round-6 draft roadmap. See [`notes/roadmap/roadmap-6.md`](roadmap-6.md) for the current scope.

- **Compose-style declarative layer** (Resolution A from step-11 workplan). Surface is pinned in [`docs/specs/vault-management.md` § Compose-Style Declarative Layer (deferred)](../docs/specs/vault-management.md#compose-style-declarative-layer-deferred); a future workplan pins format + merging rules. Originally a step-11 deferred-decision; deferred past rounds 4 and 5.
- **MCP write-tool gating granularity.** Step 10 committed to a single `[mcp] enable_write_tools` flag; with step 11 the gated set grew from 2 tools (create/terminate) to 7 (full lifecycle). Per-tool gating is round-6+ if a use-case surfaces — e.g. an operator who wants `vault_pause` / `vault_resume` enabled for an agent but not `vault_terminate`.
- **Multi-model embedding per vault.** Today the embedding service is daemon-wide and the `chunks_vec` dimension is migration-baked. [`docs/specs/vault-management.md` § Open Questions](../docs/specs/vault-management.md#open-questions) lists this as a future candidate if a use-case surfaces.
- **Cross-vault search pagination + streaming.** Pinned forward-compat in [`docs/specs/vault-management.md` § Open Questions](../docs/specs/vault-management.md#open-questions); request-side cursor field is reserved on the wire shape. Round-6+.
- **Release automation** (`release.yml`, binary cross-compilation, checksums, cargo-dist). Explicitly out of scope for round 5; round-6+ when the project needs binary distribution.
- ~~**Outbox removal / outbox simplification.**~~ **Pulled into round 6** (step 16--17). The likely next move is removing the outbox entirely, with the exact replacement event model pinned at workplan time.
- **OSSF Scorecard / CodeQL.** Security tooling for when the project has public visibility. Round-6+.
- **Windows CI matrix.** Current CI scope is unix-only (ubuntu + macos). Add when Windows support becomes a project goal.
- **Search-error classification: replace string-prefix routing with typed errors.** Today [`From<anyhow::Error> for ApiError`](../src/api/error.rs) and [`anyhow_is_request_validation`](../src/api/search.rs) classify errors by formatting the chain with `{err:#}` and matching `starts_with("invalid_glob")` / `"invalid_regex"` / `"invalid_prefix"`. The producers ([`search/content.rs`](../src/search/content.rs), [`search/filesystem.rs`](../src/search/filesystem.rs), [`search/mod.rs::normalize_prefix`](../src/search/mod.rs)) emit `anyhow!("invalid_regex: {e}")` etc. Any future `.context(...)` wrap upstream of these sentinels silently degrades the response from 400 to 500 with no test coverage of that case. [`SemanticSearchError::InvalidPrefix`](../src/search/semantic.rs) already half-models this and ends up re-parsing its own Display string. **Round-N research**: confirm the sentinel-prefix pattern is the only contract today, then introduce a small typed error (per-search-mode or a shared `SearchValidationError`) that the API layer pattern-matches structurally. Source: amp code review 2026-04-28 ([notes/amp-code-review.md](amp-code-review.md) preface).
- **`path_under` / `paths_equal` swallow canonicalize failures.** [`src/control_plane/manager.rs`](../src/control_plane/manager.rs) helpers fall back to `to_path_buf()` on canonicalize error, then run `starts_with` / `==`. For `path_under`, the data-dir-under-vault check during `create` can pass spuriously when either path is inaccessible. For `paths_equal`, two un-canonicalizable paths spelled differently compare unequal, so the `VaultPathConflict` precheck could let two registry rows resolve to the same logical directory. The `canonicalize_for_create` call earlier in `create` mitigates the second case for the *new* path but not for an existing row whose stored path is now inaccessible. **Round-N research**: enumerate the call sites, decide whether "can't canonicalize" should fail closed (return `VaultPathInvalid`) or open with a logged warning. Source: amp code review 2026-04-28.
- **Embedding-skipped files produce no consumer-level signal.** [`src/indexer/mod.rs::process_entry`](../src/indexer/mod.rs) returns `ProcessEffect::EmbeddingSkipped` on transient embedding failure; the public `ReindexOutcome` collapses that to `HashUnchanged`; [`watcher::apply_event`](../src/watcher/mod.rs) treats `HashUnchanged` as a silent no-op. Net: a modified-on-disk file that hits an embedding outage produces no outbox event and no warn log at the consumer level (the indexer-level error log is there but isn't tied to the watch-event surface). For an operator tailing the outbox, a real change just doesn't show up. **Round-N research**: confirm this is the live behavior under a stub embedder that returns `Transport`; decide between (a) propagating `EmbeddingSkipped` through `ReindexOutcome` so the watcher can log specifically, or (b) emitting an `info` line at the watcher when an `Upsert` collapses to `HashUnchanged`. Source: amp code review 2026-04-28.

---

## Multi-vault — shipped in Round 3

Scope settled by [ADR-0009](../docs/decisions/0009-multi-vault-per-daemon.md), [ADR-0010](../docs/decisions/0010-vault-definitions-as-runtime-state.md), [ADR-0011](../docs/decisions/0011-vault-management-on-hmn.md); shipped across roadmap-3 steps 9–11 (per-vault refactor → control plane create/list/status/terminate + cross-vault search → remaining lifecycle ops + `hmnd scan` removal). The four search/event spec amendments (Solo todo 64) and the vault-management.md fleshout (Solo todo 65) landed in step 10. The Compose-style declarative layer was deferred to round 4 (see § Round-4 candidates above).

## Agent-host integration — round-3-or-later, no roadmap slot yet

- **MCP tool discoverability.** `search_filesystem` and `search_content` are reliably triggered by natural-language phrasing ("files named X" / "files containing Y"). `search_semantic` is not — agent hosts (verified against Claude Code) fan out to multiple content searches instead of selecting the semantic tool for "files about X" phrasing. Workaround today: explicit invocation. Fix path: agent-side skill magic — better tool descriptions, examples, possibly a dedicated Hypomnema skill installed into agent contexts. Not a daemon bug; lives at the host. Captured in step-8 retro § Human perspective.
- ~~**MCP Streamable HTTP transport.**~~ **Shipped in round 4** ([ADR-0013](../docs/decisions/0013-mcp-transport-streamable-http.md), [`docs/specs/mcp-streamable-http.md`](../docs/specs/mcp-streamable-http.md)). The third standard MCP transport now mounts on `hmnd`'s Axum router at `/mcp` alongside `/search/*` and `/vaults/*`; trust-posture-inheritance from the existing HTTP listener (loopback by default, no auth, no TLS) plus Origin-header validation as DNS-rebinding defense. Browser-hosted hosts and remote-MCP scenarios reachable.

This bucket is its own track, not part of round 3 (multi-vault). Could become a focused workplan slotted between rounds 3 and 4, or a continuous low-priority track, or a dedicated round 4.

## Process / playbook (rolling)

Captured from round-2 retros; apply when the next coordinator/orchestrator/task-agent natural touch points:

- **MSRV cross-check at workplan self-review.** Any new top-level crate added in a workplan should have its MSRV cross-checked against `rust-toolchain.toml` at workplan-write time. Catches the escalation 81 (rmcp 1.5.0 / Rust 1.88) shape. (Step-8 retro item 1 + § Step-boundary follow-ups.)
- **"Act-now vs defer-to-boundary" rule for soft flags that demonstrate real bugs.** Codify in COORDINATOR § Wake-up routing. Two data points (step-6 Task 6.4r1, step-8 toolchain auto-mode approval) — pattern is stable. (Step-8 retro § What would we change item 2.)
- **Forward-note prediction-vs-observation check.** When a forward note makes a testable prediction about external library behavior, the receiving task agent should explicitly verify and report agreement or correction. (Step-8 retro § What would we change item 3.)
- **Mid-stream course-correction shape.** Either a lightweight patch-task-agent pattern (orchestrator- or coordinator-spawned, single-todo, no scratchpad, terse retro line) or a formalization of "human commits directly during Phase 2." Today's round-2 ad-hoc approach (e.g. `7379dd0`, `fcc4aa3`) worked but felt unstructured. (Step-8 retro § Human perspective item 3.)
- **Task-agent self-write to rolling-context scratchpad.** Process question — playbook says coordinator writes to scratchpad; task agent reports via todo comment. Step-8 Task 8.3 self-appended its outcome paragraph (content-faithful). Possible playbook edit if pattern recurs. (Step-8 retro § Notes; § Step-boundary follow-ups.)
- **Manual-testing drift evaluation at every round boundary.** [`notes/manual-testing/`](manual-testing/) is at step 8; round 3 multi-vault changes haven't been reflected (single-vault fixture, v0 `vault = ...` config key in `00-setup.md`, `hmn vault …` marked unshipped). Round 4 will decide at step-12 workplan-write whether to fold a refresh into the round (see `roadmap-4.md` § Manual-testing drift). Going-forward rounds: every end-of-round retro evaluates manual-testing drift as a structural item — what shipped this round, what's now stale, what's the plan. The retro template doesn't currently prompt for this; consider a small addition to `notes/project-planning-workflow-notes.md` § Retro template if the pattern proves persistent.

## Operational follow-ups

- ~~**Outbox flake under `cargo nextest run --fail-fast` cancellation.**~~ **Pulled into round 6** via outbox removal. Investigation history: silent across steps 9–12 (round-3 step-11 3× flake-check, round-4 step-12 full-suite sweep). **Step-13 CI update**: a *second* outbox test, `deleting_file_emits_one_deleted_line_with_prior_hash` (`tests/outbox.rs:201`), reproduced on *both* macOS CI runs (run 25086730532 + workflow_dispatch run 25086929198) — consistently, not as a rare local flake. The timing-sensitive assertion fails with "expected one deleted event, got []". Same `tests/outbox.rs` file, same event-timing family. `tests/outbox.rs` was unchanged in step 13 (confirmed: `git log 8cd5add..f4130fd -- tests/outbox.rs` returned empty).
- **`flake.nix` sqlite-vec dylib provisioning.** Carried from steps 6 and 7. The dylib is an operator-side prereq the dev shell does not handle — any future round that exercises sqlite-vec from a fresh dev shell will need it again.
- **Brand-identity override revisit on rmcp major version upgrade.** ADR-0012 § Negative consequences notes this: the `#[tool_handler(name = "hypomnema")]` macro syntax is rmcp-macros-1.5.0-specific.

## Public-presence / brand work — no roadmap slot yet

- **Visual identity.** `notes/vibe/hypomnema-visual-identity.md` (drafted) + the generative-visual-identity workflow under `notes/vibe/`. Currently untracked.
- **GitHub org branding, README hero, logo, favicon.** Not in any roadmap.
- **Project website.** Hinted at in `docs/hypomnema-handoff.md` § Reference material. Not in any roadmap.

Strong candidate for a small dedicated round between 3 and 4, or a continuous low-priority track. The MCP `serverInfo.name = "hypomnema"` brand-identity override (ADR-0012) is the same theme — the project is now *named*, and the visible-look layer is the natural next ring out.

## Product-level non-goals — pointer only

The canonical "real, planned, but not v0" list lives in [`docs/product/vision.md` § Non-Goals](../docs/product/vision.md#non-goals): writes-to-vault, the ownership model, bridge-managed-files format spec, conflict resolution, multi-consumer event delivery beyond outbox tailing, multi-instance coordination (subsumed by round-3 multi-vault-per-single-daemon), Obsidian-specific behavior, bidirectional sync.

These are deferred *as product non-goals*, not "queued for a future round." If one of them ever moves out of non-goal status, it becomes a backlog item here and then a round entry; until then, vision.md is the home.
