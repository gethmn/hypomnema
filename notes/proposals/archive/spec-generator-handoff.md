# spec-generator — Handoff

> **Archived 2026-04-27 — outcome: SHIPPED.** The fork was approved; `.claude/skills/spec-generator/` is in tree and was used to draft `docs/specs/vault-management.md` (outline form). Pre-queued for the round-3 spec amendments via Solo todos 64 (four spec amendments) and 65 (full vault-management spec from outline). Frozen here for the original reasoning + open questions discussed at proposal time.

**Status**: Open for review
**Created**: 2026-04-25
**Author**: Claude (with Beau)
**Decision needed by**: before the next time we'd reach for `prd-generator` on a new feature

---

## Context

We currently have the upstream `prd-generator` skill at [`.claude/skills/prd-generator/`](../../.claude/skills/prd-generator/). It produces a 17-section PRD plus INVEST-shaped user stories through a research-first interview process.

A review against this project's LDS docs and planning workflow ([full analysis in plan file at `~/.claude/plans/review-the-documents-you-refactored-music.md`](../../)) surfaced three concerns specific to Hypomnema:

1. **Section overload.** Half the sections (Analytics, Launch & Rollout, Business Viability risks, multi-persona work) don't fit a solo local-CLI project. The skill notes "for a quick enhancement consider a one-pager" but doesn't act on that aggressively.
2. **Output shape mismatch.** The recently-recorded policy ([`../project-planning-workflow-notes.md` § "PRD / spec-generator scope policy"](../project-planning-workflow-notes.md)) says PRDs in this project are always feature-scoped and become specs after decomposition. So the natural output is *a spec*, not a PRD that has to be decomposed into one.
3. **Save location.** The skill saves to `prds/{name}.md` at project root, which doesn't fit the LDS convention.

Beau's preferred direction: an aggressive fork into a `spec-generator` rather than a thin LDS-aware wrapper. Sections from the PRD template that we'd otherwise drop should become **optional**, gated by an upfront scope-profile question and added back just-in-time when conversational signals warrant.

This handoff captures the proposed direction in detail and lists the open questions we should answer before committing to a fork.

---

## Proposed direction

### Aggressive fork, not a wrapper

Build `spec-generator` from scratch, copying selectively from `prd-generator`. Reasons:

- The output shape is different (spec, not PRD). Trying to retrofit the prd-generator's section structure into a spec template will be lossy in both directions.
- The "scope profile + JIT prompts" mechanism is a structural change to the skill's flow, not an addition to it.
- Forking gives us license to drop the framing language ("PRDs answer what and why; not project plans") that doesn't apply when the output is a spec.
- We keep `prd-generator` available unchanged for cases where a *true* PRD is wanted (rare in this project but not zero).

### Output shape: spec, not PRD

Output matches [`docs/specs/_template.md`](../../docs/specs/_template.md) (Feature Spec template — Overview, Behavior, Data Schema, Edge Cases, Open Questions). User stories with acceptance criteria are produced as a peer artifact, not a section of the spec — they inform the workplan, they don't live in the canonical spec.

### Scope profiles seed defaults

At the start of a session, the skill asks a single question: *"What's the rough shape of the project this spec is for?"*

Initial profile set:

- `solo-local-cli` — single-user local tools (Hypomnema, personal scripts)
- `internal-tool` — small team / internal-only product
- `public-saas` — externally-facing, multi-tenant, paid or free
- `library-or-sdk` — code intended for other developers to consume

Each profile presets which sections are on, off, or optional:

| Section / concern | `solo-local-cli` | `internal-tool` | `public-saas` | `library-or-sdk` |
|---|---|---|---|---|
| Personas | one (the user) | small list | required | API consumers |
| Success metrics | qualitative ok | mixed | quantitative required | adoption / DX |
| Functional requirements | required | required | required | required |
| Global Invariants | only if security-relevant | usually | always | always |
| Acceptance criteria rigor | observable, less paranoid | observable + discriminating | full prd-generator rigor | full + API contract tests |
| Adversarial / cross-tenant ACs | off (no tenancy) | on if multi-user | required | n/a |
| Analytics & Instrumentation | off (offer JIT) | off (offer JIT) | on | off |
| Launch & Rollout | off | optional | on | versioning + deprecation only |
| Risks (Business Viability) | off | optional | on | off |
| Risks (Value, Usability, Feasibility) | feasibility on, others optional | usability + feasibility | all four | feasibility + DX |
| Decomposition manifest (LDS-aware) | on | on | on | on |

### Just-in-time prompts for off-by-default sections

Off doesn't mean "never." When the conversation surfaces a signal, the skill should offer to add the section back. Concrete signal → section pairs (starting list — see open question 9):

| Signal | Offer to add |
|---|---|
| User mentions a metric, KPI, conversion, or "how would we know" | Analytics & Instrumentation, Success Metrics |
| User mentions multiple roles, "admin vs user," tenancy | Personas, Adversarial ACs, Global Invariants on auth |
| User mentions "ship," "release," "rollout," feature flag | Launch & Rollout |
| User mentions cost, pricing, business case | Business Viability risks |
| User mentions an existing spec by name | Amendment-vs-new-spec branch (open Q3) |
| User mentions "what could break" | Risks |

The offer should be lightweight: "This sounds like it might warrant an Analytics section — want me to add one?" Single yes/no, no interrogation.

### LDS-aware research

Phase 1 (research) reads in priority order:

1. `docs/product/vision.md` — establishes scope boundary (does this proposal stay inside vision, or amend it?)
2. `docs/decisions/*.md` — load-bearing decisions that constrain the spec
3. `docs/specs/*.md` — existing feature behavior the spec might touch or amend
4. `docs/architecture/overview.md` — system shape constraints
5. `docs/implementation/tech-stack.md` — what's available to build with
6. `.claude/skills/*/SKILL.md` — subsystem patterns that constrain implementation
7. Codebase grep — only after the docs research is done

If the draft spec contradicts a higher-authority doc, the skill flags it and either narrows the spec or surfaces a "this needs an ADR amendment" recommendation — it doesn't silently override.

### Decomposition manifest (always on)

Even with output already shaped like a spec, the skill produces a short manifest at the end of the session:

- New ADRs to draft (one bullet per significant decision)
- Vision amendments needed (if scope shifted)
- Architecture diagram updates needed
- New CLI / config to add to `docs/reference/`
- Open Questions routed to the spec they touch
- Workplan-ready user stories (the peer artifact)

This is the "what to do next" handoff to the human or to the roadmap/workplan workflow.

---

## Open follow-up questions

These are the decisions we need before forking. Answer these and the build is straightforward.

### 1. Canonical scope profiles

My starting set: `solo-local-cli`, `internal-tool`, `public-saas`, `library-or-sdk`.

- Are there other profiles worth a first-class slot? Candidates: `research-spike` (strips even more — only Problem, Approach, Success criteria for the spike), `internal-platform-service` (multi-team but not external-facing), `cli-binary-with-server` (Hypomnema's actual shape — overlaps `solo-local-cli` but has a server surface).
- Should profiles be hierarchical (e.g., `solo-local-cli` inherits from a base) or flat?
- Should the profile be sticky per-project (write it to the skill config) or asked every session?

### 2. Where does the spec-generator save its output?

Three candidates:

- **A.** Directly to `docs/specs/<feature>.md`. Matches the policy that PRDs become specs. But: a draft spec landing in the canonical layer before review is awkward.
- **B.** To `notes/proposals/<slug>.md` first, then promoted to `docs/specs/` on approval. Matches the proposals workflow we just set up. But: two-step process, easier to forget the promotion.
- **C.** Both: write to `notes/proposals/<slug>.md` as the working copy; on approval, promote to `docs/specs/<feature>.md` and *freeze* the proposals copy as the archive. Most honest about the lifecycle. But: most ceremony.

My lean: **C** for non-trivial features, **A** for tiny ones the skill identifies as "this is a one-paragraph spec amendment, not worth the proposal cycle." The skill should be able to tell the difference.

### 3. Amendment vs. new spec

When the feature touches an existing spec (e.g., adding regex to filesystem search):

- New separate spec? (clutter, duplication)
- Amendment patch against the existing spec? (clean, but harder to review without context)
- Interactive guidance to edit in place? (best DX, but the skill becomes editor-shaped, not generator-shaped)

My lean: amendment patch with the full spec re-printed for review context. After approval, the patch is applied to the canonical spec.

### 4. Relationship to ADRs

The spec-generator will surface load-bearing decisions during conversation. What does it do with them?

- **A.** Draft ADRs as part of its output, save to `docs/decisions/NNNN-*.md`. Risk: ADRs are immutable; auto-generating them while the spec is still in draft is dangerous.
- **B.** Queue them as a list at the end of the spec ("ADRs to draft after approval"). Lower risk, requires human follow-through.
- **C.** Just call them out in conversation and leave authoring to the human.

My lean: **B**. Let the human draft the ADR after the spec is approved — that's when the decision is actually committed.

### 5. User stories — separate file or section of the spec?

The current LDS spec template (`docs/specs/_template.md`) doesn't have a user-stories section.

- Add a "User Stories" section to the template? (changes the canon for one consumer)
- Keep stories as `notes/proposals/<slug>-stories.md`? (separate artifact, lifecycle has to be defined)
- Have the workplan reference stories directly without a persistent file? (loses them across step boundaries)

My lean: separate `notes/proposals/<slug>-stories.md` artifact, lifecycle parallel to the proposal — when the proposal is archived, so are the stories. The workplan references the archived stories file by path.

### 6. Coexistence with prd-generator

If we keep `prd-generator` around, when do you reach for it over `spec-generator`?

- Hypothesis: only for product-level work that would amend `vision.md` — but the policy says that should usually skip the skill entirely and edit vision.md directly.
- Realistic answer: maybe never in this project. In which case, do we remove the prd-generator from this project's `.claude/skills/` entirely? Or leave it as a fallback?

My lean: leave it in place but unused; it's already there, removing it doesn't help anything. If you ever spin up a public-SaaS-shaped subproject, you'll want it.

### 7. Skill location

- **A.** Project-local at `.claude/skills/spec-generator/`. Doesn't help other projects but keeps changes scoped.
- **B.** User-level at `~/.claude/skills/spec-generator/`. Available across all your projects, but other projects might not be LDS-shaped.
- **C.** Upstream into a generally-useful skill that detects LDS / Diátaxis / similar layouts. Most ambitious; longest path to value.

My lean: **A** to start. Once it's worked twice in Hypomnema and you have a second LDS-shaped project, promote it to **B**. **C** is a separate project.

### 8. Upstream "ideation" skill

The original analysis flagged that `prd-generator` handles convergent ideation (tightening a fuzzy idea) but not divergent (generating ideas from "blank page"). Possible additions:

- A `concept-explorer` or `ideation` skill that runs upstream of `spec-generator` — JTBD interviews, "5 whys," competitive teardowns, opportunity-solution trees.
- Output: a concept note in `notes/proposals/<slug>.md` that becomes the input to `spec-generator`.

This is out of scope for the spec-generator fork itself, but worth deciding whether we want to commit to building it next. My lean: defer until a real "blank page" moment hits — premature for now.

### 9. JIT-prompt trigger list

The signal → section mapping above is a starting list. Need:

- A canonical, exhaustive-enough list of signals
- Each signal needs a clear test ("user mentions a metric" — is "fast" a metric? what about "users like it"?)
- Some way to suppress repeat offers if the user said no once

My lean: start with the table above, refine the trigger phrasing after the first real session. Don't pre-engineer this.

### 10. Acceptance-criteria rigor for `solo-local-cli`

`prd-generator` spends significant template real estate on observable + discriminating ACs (cross-tenant denial, constructed-oracle guardrails, negative-fingerprint greps, boundary-graph checks). For a solo CLI:

- Cross-tenant denial doesn't apply (no tenancy).
- Constructed oracles still apply (and matter).
- Negative-fingerprint greps still apply (great for "make sure no untyped passthroughs survive a refactor").
- Boundary-graph checks still apply (and matter for the daemon's hot paths).

My lean: relax cross-tenant ACs to off for `solo-local-cli`, keep the rest on. The discriminating-AC checks are language-agnostic correctness work, not enterprise ceremony.

---

## Recommended next session

Pick a feature you'd otherwise have hand-written a spec for — *not* one of the steps 1–5 features (those are already specced) — and do a paper dry-run of a hypothetical `spec-generator` session against it.

Candidates that come to mind from the open scope of Hypomnema:
- "Add an `hmn vault watch` command that streams outbox events as they arrive" (a thin CLI surface over the existing outbox; small enough to expose where the skill bottoms out)
- "Add an embedding-cache layer so re-indexing the same chunk doesn't recompute the embedding" (touches multiple existing specs; good test of amendment vs. new spec)
- "Support multiple watched directories per daemon" (currently a vision-level Non-Goal — good test of the 'this would amend vision.md' branch)

The dry-run will reveal which open questions above are *blocking* (must answer before forking) and which are *nice-to-have* (can defer to second session). Walk through the proposed direction conversationally as if the skill existed, and note where you stall.
