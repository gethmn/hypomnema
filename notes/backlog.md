# Hypomnema Backlog

Round-agnostic queue of work pulled into roadmaps as rounds are written. Adding to this file does **not** commit to a timeline. Round workplans pull from here when they're ready; new work drops in here when it's identified but not yet scoped to a round.

This file replaces the earlier "round 4+ / handoff doc § Out of scope" framing. Buckets are by theme, not by round number — the round number gets assigned (or stays unassigned) when a roadmap is written.

> **Conventions**
> - Each item is short. Deeper context lives in the source the item came from (retro, ADR, spec, vision doc). Always link.
> - Items move out of this file when they land in a roadmap (`docs/roadmap/roadmap-N.md` § Round N) or, for non-roadmap work like playbook edits, when they're applied.
> - Items can stay in this file indefinitely — un-scoped is a valid state.

---

## Multi-vault — committed to Round 3

Scope settled by [ADR-0009](../docs/decisions/0009-multi-vault-per-daemon.md), [ADR-0010](../docs/decisions/0010-vault-definitions-as-runtime-state.md), [ADR-0011](../docs/decisions/0011-vault-management-on-hmn.md); roadmap entry in [`docs/roadmap/roadmap-2.md`](../docs/roadmap/roadmap-2.md) § Round 3 (post-v0). Spec work pre-queued in Solo todos 64 (four spec amendments) and 65 (vault-management.md full spec from outline). Five open deferred decisions to be resolved at the round-3 workplan.

## Agent-host integration — round-3-or-later, no roadmap slot yet

- **MCP tool discoverability.** `search_filesystem` and `search_content` are reliably triggered by natural-language phrasing ("files named X" / "files containing Y"). `search_semantic` is not — agent hosts (verified against Claude Code) fan out to multiple content searches instead of selecting the semantic tool for "files about X" phrasing. Workaround today: explicit invocation. Fix path: agent-side skill magic — better tool descriptions, examples, possibly a dedicated Hypomnema skill installed into agent contexts. Not a daemon bug; lives at the host. Captured in step-8 retro § Human perspective.
- **MCP Streamable HTTP transport.** Third standard MCP transport (single HTTP endpoint with SSE for server-streamed messages). Not in [ADR-0012](../docs/decisions/0012-mcp-transport-stdio-v0.md) — that ADR enumerates only stdio (shipped) and Unix socket (deferred). Earns its keep when an agent host can't spawn subprocesses (browser-hosted hosts) or when the daemon and host aren't co-located (remote MCP). Pulls in the same `rmcp` crate Hypomnema already links; the implementation would live on `hmnd` (long-lived listener, like the deferred socket transport). Tension with [ADR-0005 Local Everything](../docs/decisions/0005-local-everything.md)'s loopback-only posture — would need either a same-host-only-by-default binding (loopback HTTP, like the existing `/search/*` surface) or a deliberate remote-allowed mode with auth (which the local-everything trust model deliberately punted on). Round-3-or-later, no slot yet.

This bucket is its own track, not part of round 3 (multi-vault). Could become a focused workplan slotted between rounds 3 and 4, or a continuous low-priority track, or a dedicated round 4.

## Process / playbook (rolling)

Captured from round-2 retros; apply when the next coordinator/orchestrator/task-agent natural touch points:

- **MSRV cross-check at workplan self-review.** Any new top-level crate added in a workplan should have its MSRV cross-checked against `rust-toolchain.toml` at workplan-write time. Catches the escalation 81 (rmcp 1.5.0 / Rust 1.88) shape. (Step-8 retro item 1 + § Step-boundary follow-ups.)
- **"Act-now vs defer-to-boundary" rule for soft flags that demonstrate real bugs.** Codify in COORDINATOR § Wake-up routing. Two data points (step-6 Task 6.4r1, step-8 toolchain auto-mode approval) — pattern is stable. (Step-8 retro § What would we change item 2.)
- **Forward-note prediction-vs-observation check.** When a forward note makes a testable prediction about external library behavior, the receiving task agent should explicitly verify and report agreement or correction. (Step-8 retro § What would we change item 3.)
- **Mid-stream course-correction shape.** Either a lightweight patch-task-agent pattern (orchestrator- or coordinator-spawned, single-todo, no scratchpad, terse retro line) or a formalization of "human commits directly during Phase 2." Today's round-2 ad-hoc approach (e.g. `7379dd0`, `fcc4aa3`) worked but felt unstructured. (Step-8 retro § Human perspective item 3.)
- **Task-agent self-write to rolling-context scratchpad.** Process question — playbook says coordinator writes to scratchpad; task agent reports via todo comment. Step-8 Task 8.3 self-appended its outcome paragraph (content-faithful). Possible playbook edit if pattern recurs. (Step-8 retro § Notes; § Step-boundary follow-ups.)

## Operational follow-ups

- **Outbox flake under `cargo nextest run --fail-fast` cancellation.** Carried from steps 6 and 7; not encountered in step 8. Future flake-investigation candidate; not blocking round 3.
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
