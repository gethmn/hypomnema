---
name: Backlog Hygiene — Proposal Archival at Round Close
description: Close the recurring gap where shipped proposals + intakes are left in notes/proposals/ instead of moved to notes/proposals/archive/.
type: proposal
date: 2026-05-03
---

# Backlog Hygiene — Proposal Archival at Round Close

## Problem

Proposals and their intake artifacts are not being archived when the round that consumed them ships. Two known occurrences:

- **Round 9** (Steps 19–20, shipped 2026-05-02): `content-retrieval*.md`, `intake-content-retrieval.md`, `fts5-bm25-content-search*.md`, `intake-fts5-bm25-content-search.md` were left in `notes/proposals/` until manually moved on 2026-05-03. `roadmap-9.md` itself was archived in the same cleanup pass.
- **Round 10** (Static sqlite-vec bundling): `static-sqlite-vec-bundling*.md` and `intake-static-sqlite-vec-bundling.md` also left behind, moved in the same 2026-05-03 sweep.

This is now at least the second occurrence with a "missed at round close, applied during later cleanup" annotation in the roadmap file itself. Pattern, not accident.

## Root cause

The lifecycle is documented; the ownership is not.

- `notes/project-planning-workflow-notes.md` § "Where artifacts live" says proposals move to `archive/` "after approval and decomposition" — passive voice, no role.
- `notes/project-planning-workflow-notes.md` § "Step boundary ritual" enumerates seven actions (mark step done, capture ADRs, update roadmap, retro, expand next workplan, user review, push). Proposal archival is not on the list.
- `notes/playbook/coordinator.md` § "Step Boundary" lists: verify shipping criteria, retro, **archive workplan and scratchpad**, post step-shipped comment, close processes. Notably scoped to workplan/scratchpad — proposal/intake archival is absent.
- `notes/playbook/orchestrator.md` § "Round Candidate Sourcing" tells the orchestrator to *detect* discrepancies between `backlog.md` and shipped scope and "flag … as a backlog-hygiene action." Detection-only — no closing action defined.
- The artifacts being archived live one directory up from the workplan (`notes/proposals/` vs `notes/roadmap/`), so the coordinator's existing archive step doesn't naturally sweep them.

Result: every role assumes someone else owns it. Round 9 demonstrates this — the coordinator archived workplans (`step-19-workplan.md`, `step-20-workplan.md` are in `notes/roadmap/archive/`) but the round-level roadmap file *and* the proposals it consumed both stayed put. The drift is consistent: round-scoped artifacts (roadmap-N, proposals fed into round N) are nobody's job; step-scoped artifacts (workplans, scratchpads) get archived reliably.

## Proposed solution

A two-part fix: explicit ownership in playbook + a lightweight automated check.

### 1. Playbook & workflow-notes amendments (the cheap, durable part)

**`notes/playbook/coordinator.md` § "Step Boundary"** — add an item between current 3 and 4:

> 3a. If this step closes a round (last step in `roadmap-N.md`): also archive `roadmap-N.md` → `roadmap/archive/`, and archive every proposal + intake file referenced by that roadmap's `Intakes:` block → `proposals/archive/`.

**`notes/project-planning-workflow-notes.md` § "Step boundary ritual"** — add a step 8:

> 8. If the step closes a round, archive `roadmap-N.md` and all proposals/intakes named in its `Intakes:` block.

**`notes/proposals/archive/README.md`** — add a one-line note: "Files land here at round close, archived by the coordinator that ships the round's last step."

This makes the *coordinator that ships the last step* the unambiguous owner. Existing roadmap convention (each `roadmap-N.md` already lists its intakes in a structured `Intakes:` block) gives the coordinator the exact list of files to move.

### 2. Hygiene check script (the robust part)

A small script — `scripts/check-proposal-hygiene.sh` (or a `cargo xtask`) — that:

- For every `notes/roadmap/archive/roadmap-N.md`, parses the `Intakes:` block (markdown links).
- For each linked proposal/intake path, checks whether the file is under `notes/proposals/` (bad) or `notes/proposals/archive/` (good).
- Exits non-zero with a list of orphans.

Wire it into:

- A pre-push or pre-commit hook (optional; cheap to skip for non-roadmap commits).
- The orchestrator's `Round Candidate Sourcing` flow — already does discrepancy detection in prose; this turns the detection into a single shell call with a deterministic answer.

The proposals already use a stable naming convention (`<slug>.md`, `<slug>-stories.md`, `intake-<slug>.md`), so parsing is a five-line awk job.

## Tradeoffs

| | Manual checklist only | Automated check only | Both (recommended) |
|---|---|---|---|
| Cost | ~10 min doc edit | ~30 min script + wire-up | ~40 min total |
| Catches future drift | Only if coordinator reads playbook | Always, deterministically | Always |
| Self-healing if playbook drifts | No | Yes (script is single source of truth) | Yes |
| Cognitive load on coordinator | Adds one bullet | Zero (script flags it) | Adds one bullet, script is safety net |

The pure-checklist approach is exactly the failure mode we already have — `project-planning-workflow-notes.md` *does* list a step-boundary ritual, and proposal archival just isn't on it. Adding another bullet to a list that's already being skipped is optimistic. The script costs little and turns "did the coordinator remember" into "did CI pass."

**Recommendation: do both.** Playbook edit codifies ownership (so the work *should* happen); script enforces it (so we *find out* when it didn't). The orchestrator's existing "flag backlog-hygiene discrepancies" responsibility becomes a one-line script call, which is strictly better than re-deriving the check by hand each time.

## Scope: roadmap step, backlog item, or one-shot doc PR?

**One-shot doc PR + script**, not a roadmap step.

- The playbook edits are ~10 lines across three files. No code, no design tradeoffs.
- The script is ~30 lines of shell/awk against an existing file convention. No new dependency, no architectural surface.
- A roadmap step implies coordinator+researcher+builder orchestration, which is heavier than the change warrants and would itself be subject to the bug it's fixing.

A backlog item is appropriate only if we want to defer this. We shouldn't — the next round close will produce the same orphans.

Suggested PR title: `docs/tooling: own proposal archival at round close`. Lands the three doc edits and `scripts/check-proposal-hygiene.sh` together. After merge, run the script once to confirm the 2026-05-03 manual cleanup left no residue.
