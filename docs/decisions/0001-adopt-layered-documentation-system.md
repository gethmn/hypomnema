# ADR-0001: Adopt the Layered Documentation System

**Status**: accepted
**Date**: 2026-04-23
**Decision-Makers**: Beau Simensen

---

## Context

Hypomnema starts with a single rich orientation document (`hypomnema-handoff.md`) that covers vision, load-bearing decisions, scope, implementation plan, and pitfalls. As the project grows, that single document will either become unmaintainable or will drift from reality — the usual failure mode for free-form design docs.

The project needs a documentation structure that:
- Separates decision rationale from specifications (so decisions don't rot when specs change)
- Keeps reference material (CLI/config) close to the code it describes
- Supports AI-agent-driven maintenance (since agents will be writing much of the code and are well-positioned to maintain the docs alongside)
- Accommodates partial fills — not every layer needs content on day one

## Decision

Adopt the Layered Documentation System (LDS) with six of the seven layers:

- Layer 1: Decisions (ADRs) — MADR Minimal template
- Layer 2: Vision — Lean PRD template
- Layer 3: Architecture — C4-Lite template
- Layer 4: Specifications — Feature Spec template
- Layer 5: Reference — CLI + Configuration templates
- Layer 7: Implementation — Tech Stack template

Skip Layer 6 (Behaviors / Gherkin) for now; revisit if integration-test BDD shape becomes useful.

Seed the layers from the existing `hypomnema-handoff.md`, which stays in place as historical context.

## Consequences

### Positive

- ADRs preserve "why" distinctly from specs' "what" and reference's "how to invoke," reducing drift
- Maintenance workflows (`audit.md`, `sync.md`, `update.md`, `refine.md`) are standardized and agent-executable
- New contributors (human or agent) have a predictable place to look for each kind of question
- Layers can be filled incrementally; missing layers aren't errors

### Negative

- Some upfront overhead writing seed content across six directories instead of one doc
- Risk of over-structuring a project that is still in pre-v0 — we may discover the handoff was enough
- Cross-layer linking discipline has to be maintained manually; easy to let rot creep in

### Neutral

- The handoff remains canonical history; new information goes in the appropriate layer rather than back-edited into the handoff
- `AGENTS.md` continues to be the first-load file for agent contributors; it now points into the layered docs

---

## Notes

- Related to `docs/DOCUMENTATION-GUIDE.md` which documents the installed configuration (thresholds, selected layers, tooling tier)
- Tooling tier: Tier 2 (Recommended) — Vale, Log4brains, Structurizr

## Amendments

<!-- None yet -->
