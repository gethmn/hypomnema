# Hypomnema Backlog

Round-agnostic queue of work pulled into roadmaps as rounds are written. Adding to this file does **not** commit to a timeline. Round workplans pull from here when they're ready; new work drops in here when it's identified but not yet scoped to a round.

This file replaces the earlier "round 4+ / handoff doc § Out of scope" framing. Buckets are by theme, not by round number — the round number gets assigned (or stays unassigned) when a roadmap is written.

> **Conventions**
> - Each item is short. Deeper context lives in the source the item came from (retro, ADR, spec, vision doc). Always link.
> - Items move out of this file when they land in a roadmap (`notes/roadmap/roadmap-N.md` § Round N) or, for non-roadmap work like playbook edits, when they're applied.
> - Items can stay in this file indefinitely — un-scoped is a valid state.

---

## Round-4 candidates

Items raised at the round-3 boundary that have no roadmap slot yet. Round-4 (or later) workplans pull from here; un-scoped is a valid state.

- **Compose-style declarative layer** (Resolution A from step-11 workplan). Surface is pinned in [`docs/specs/vault-management.md` § Compose-Style Declarative Layer (deferred)](../docs/specs/vault-management.md#compose-style-declarative-layer-deferred); a future workplan pins format + merging rules. Originally a step-11 deferred-decision; deferred to round 4 because the round-3 workplan budget was already at scope without it.
- **CHANGELOG.md adoption.** Round-3 ships as `v0.2.0` (round-3 shipping gate); the boundary is a natural moment to settle whether the project starts a CHANGELOG. Carried as a step-11 boundary follow-up.
- **MCP write-tool gating granularity.** Step 10 committed to a single `[mcp] enable_write_tools` flag; with step 11 the gated set grew from 2 tools (create/terminate) to 7 (full lifecycle). Per-tool gating is round-4+ if a use-case surfaces — e.g. an operator who wants `vault_pause` / `vault_resume` enabled for an agent but not `vault_terminate`.
- **Round-4 flake-hardening pass on `tests/outbox.rs::rename_emits_deleted_then_created_lines`.** Pre-existing ~17%-repro flake carried from steps 6/7/10; not encountered in step-11's 3× flake-check or full smoke matrix runs. Needs investigation against macOS / Linux event-coalescing semantics. Step-11 did not touch this surface.
- **Multi-model embedding per vault.** Today the embedding service is daemon-wide and the `chunks_vec` dimension is migration-baked. [`docs/specs/vault-management.md` § Open Questions](../docs/specs/vault-management.md#open-questions) lists this as round-4+ if a use-case surfaces.
- **Cross-vault search pagination + streaming.** Pinned forward-compat in [`docs/specs/vault-management.md` § Open Questions](../docs/specs/vault-management.md#open-questions); request-side cursor field is reserved on the wire shape. Round-4+.

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

- **Outbox flake under `cargo nextest run --fail-fast` cancellation.** Carried from steps 6 and 7; not encountered in steps 8–11 (step 11's 3× flake-check + full smoke matrix were clean). Future flake-investigation candidate. Now duplicated under § Round-4 candidates as the `rename_emits_deleted_then_created_lines`-specific entry; consolidate when round-4 workplan picks it up.
- **`flake.nix` sqlite-vec dylib provisioning.** Carried from steps 6 and 7. The dylib is an operator-side prereq the dev shell does not handle — round 3's first build that exercises sqlite-vec will need it again.
- **Brand-identity override revisit on rmcp major version upgrade.** ADR-0012 § Negative consequences notes this: the `#[tool_handler(name = "hypomnema")]` macro syntax is rmcp-macros-1.5.0-specific.

## Public-presence / brand work — no roadmap slot yet

- **Visual identity.** `notes/vibe/hypomnema-visual-identity.md` (drafted) + the generative-visual-identity workflow under `notes/vibe/`. Currently untracked.
- **GitHub org branding, README hero, logo, favicon.** Not in any roadmap.
- **Project website.** Hinted at in `docs/hypomnema-handoff.md` § Reference material. Not in any roadmap.

Strong candidate for a small dedicated round between 3 and 4, or a continuous low-priority track. The MCP `serverInfo.name = "hypomnema"` brand-identity override (ADR-0012) is the same theme — the project is now *named*, and the visible-look layer is the natural next ring out.

## Product-level non-goals — pointer only

The canonical "real, planned, but not v0" list lives in [`docs/product/vision.md` § Non-Goals](../docs/product/vision.md#non-goals): writes-to-vault, the ownership model, bridge-managed-files format spec, conflict resolution, multi-consumer event delivery beyond outbox tailing, multi-instance coordination (subsumed by round-3 multi-vault-per-single-daemon), Obsidian-specific behavior, bidirectional sync.

These are deferred *as product non-goals*, not "queued for a future round." If one of them ever moves out of non-goal status, it becomes a backlog item here and then a round entry; until then, vision.md is the home.
