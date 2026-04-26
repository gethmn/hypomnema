# Layered Documentation System (LDS) — Explore

**For AI Agents**: This document contains instructions for negotiating a proposed change against the existing LDS canon. Use this when a proposal (a feature idea, a refactor, a removal of a Non-Goal, an ADR amendment, anything) conflicts with one or more load-bearing canon items and the path forward is not obvious.

**Terminology**: See the [Glossary](../DOCUMENTATION-GUIDE.md#glossary) for definitions of key terms.

---

## When to Use This Guide

Use this guide when:
- A proposal contradicts an item in vision (Non-Goal, Guiding Principle, success criterion)
- A proposal contradicts a load-bearing ADR
- A proposal would amend behavior in an existing spec in a way that touches multiple specs or the spec's underlying ADR
- A proposal violates a stated invariant in `architecture/overview.md`
- `spec-generator` (or any other tool) bottoms out into a "stop and reconsider" recommendation because the LDS authority order would be inverted
- You want to think through "what would actually have to change if we did X?" before committing to the change

**Do NOT use this guide for**:
- A proposal that fits inside existing canon — invoke `spec-generator` directly to produce a spec
- Updating docs after code changes — use `update.md`
- Periodic documentation audits — use `sync.md`
- Quality improvements without proposed canon changes — use `refine.md`
- Inventorying documentation — use `audit.md`

---

## Core Philosophy

The LDS authority order — Vision > Decisions > Specs > Architecture > Implementation > Reference — is load-bearing. A spec that silently contradicts an ADR, or an ADR that silently contradicts vision, breaks the canon. The cost of breakage is not just immediate confusion; it's that future readers can no longer trust any one layer to mean what it says, because they don't know which silent contradiction is in force.

The job of this prompt is to make the contradiction *visible*, then to find the smallest, most honest change to canon that accommodates the proposal — or to find a tweaked version of the proposal that doesn't require changing canon at all. The work is collaborative: surface the trade-offs, propose alternatives, recommend, but do not commit to canon edits without the user's explicit approval.

This is a generic prompt: it handles vision conflicts, ADR conflicts, architecture conflicts, spec amendment conflicts, or any combination. A single proposal may conflict with multiple layers simultaneously — the prompt handles that case by name.

---

## Process Overview

```
1. Capture proposal & scan canon → What is the proposal? What does it conflict with?
2. Surface the why            → For each conflict, what is the load-bearing rationale?
3. Map impact & side effects  → If we accommodated, what would have to change downstream?
4. Propose alternatives       → scope-down / scope-around / defer-with-forward-compat / amend-canon / drop
5. Recommend & let user choose
6. (If amend-canon) Draft edits & ADRs
7. (If amend-canon) Review & execute approved edits
8. Hand off to next workflow  → spec-generator / workplan note / nothing
```

---

## Explore Process

### Step 1: Capture the Proposal & Scan Canon

#### 1a: Capture the proposal

Record the user's proposal verbatim. If it's vague ("I want to think about multi-vault support"), ask one clarifying question to make it concrete enough to evaluate against canon. The proposal needs to be specific enough that you can identify what canon it would conflict with.

Store as `PROPOSAL`.

#### 1b: Scan canon in LDS authority order

Read the LDS layers in priority order (highest authority first). For each layer, look for items that the proposal would contradict, amend, or violate.

1. **`docs/product/vision.md`** — Non-Goals, Guiding Principles, success criteria, Open Questions
2. **`docs/decisions/*.md`** — Status, Decision sections, Consequences, Amendments
3. **`docs/specs/*.md`** — behavior contracts, schema shapes, edge case decisions
4. **`docs/architecture/overview.md`** — module boundaries, contracts, invariants
5. **`docs/implementation/tech-stack.md`** — chosen tech, deferred crates
6. **`docs/reference/*.md`** — CLI surface, config surface (usually downstream of higher layers; conflicts here usually trace back to a higher-layer conflict)

#### 1c: Enumerate ALL conflicts

A single proposal may conflict with multiple layers at once. Enumerate every conflict, not just the first one found. A conflict in a higher layer often implies cascading conflicts in lower layers — note both the root and the ripples.

> **Conflicts Found**
>
> | Layer | Document | Reference | What conflicts |
> |-------|----------|-----------|----------------|
> | Vision | `docs/product/vision.md` | Non-Goals (line NN) | Proposal would amend "<exact line>" |
> | Decisions | `docs/decisions/NNNN-<slug>.md` | Decision section | Proposal would supersede "<short summary>" |
> | Specs | `docs/specs/<feature>.md` | Behavior section | Proposal would amend "<short summary>" |
> | Architecture | `docs/architecture/overview.md#<anchor>` | Invariant on `<X>` | Proposal would violate "<short summary>" |
> | Reference | `docs/reference/<topic>.md` | Section | Downstream of vision conflict above |

If no conflicts are found, **stop and route correctly**: this proposal does not need exploration; invoke `spec-generator` directly.

#### 1d: Detect "the canon already considered this"

Some canon items already enumerate alternatives the team considered (e.g., `vision.md` may list options A/B/C with their costs, or an ADR may have a Notes section discussing rejected alternatives). When you find this, **surface it** — the user may want to pick from the existing list rather than propose a new alternative. The exploration may converge faster.

> **Existing alternatives in canon**
>
> | Source | Alternatives enumerated | Currently chosen / leaning |
> |--------|-------------------------|----------------------------|
> | `docs/product/vision.md:NN-MM` | A: <summary>, B: <summary>, C: <summary> | <which one, if any> |

---

### Step 2: Surface the Why

For each conflict from Step 1, find the load-bearing rationale. Rationale lives in different places:

- **Explicit in the doc**: vision Non-Goals often have parenthetical "(because <reason>)" or are discussed in nearby paragraphs; ADR Context and Consequences sections name the trade-offs; specs cite their related ADRs
- **In a related ADR**: a Non-Goal in vision often traces to an ADR that decided the related question
- **In commit history**: when rationale is implicit, `git log` for the line/file may surface the original reasoning
- **In notes**: `notes/project-planning-workflow-notes.md` and other notes files often capture rationale that didn't make it into canon yet
- **Inferable**: sometimes the rationale is implicit from related Non-Goals / Guiding Principles / Goals — name your inference and confirm with the user

#### 2a: Document the rationale per conflict

> **Rationale per conflict**
>
> | Conflict | Source of rationale | Why this canon exists |
> |----------|---------------------|-----------------------|
> | Vision Non-Goal "<X>" | Paragraph at `vision.md:NN` + ADR-NNNN | <one-paragraph summary of why this is the current canon> |
> | ADR-NNNN | Context section | <one-paragraph summary> |

#### 2b: Confirm with the user before mapping

Before moving to Step 3, present the rationale summary back to the user:

> *"Before I map what would have to change to accommodate the proposal, I want to confirm the load-bearing rationale for the current canon. [Summary above.] Does this match your understanding? Is there context I'm missing?"*

If the user supplies missing context, update the rationale summary and proceed. Do not skip this step — moving to alternatives without an agreed understanding of the why produces alternatives that miss the point.

---

### Step 3: Map Impact & Side Effects

For each conflict, name the change that would be required if the proposal were accommodated as-stated. Then name the side effects that change introduces.

Side effects come in several shapes:
- **Cascading canon edits** — changing canon item X requires changing items Y and Z because they were predicated on X
- **New invariants** — accommodating the change introduces new things the system has to enforce, validate, or guarantee
- **New failure modes** — the change opens up scenarios that didn't exist before (concurrency, ordering, partial failure)
- **Schema / contract migrations** — wire shapes, persisted data, public APIs that consumers depend on
- **New review surface** — code paths, security boundaries, error catalogs that grow

#### 3a: Per-conflict impact map

> **Impact map**
>
> **Conflict 1: <name>**
>
> *Required change*: <what would have to change in canon>
>
> *Cascading canon edits*:
> - `<file>` — <what edit>
> - `<file>` — <what edit>
>
> *New invariants*: <list, or "none">
>
> *New failure modes*: <list, or "none">
>
> *Schema / contract migrations*: <list, or "none">
>
> *Estimated downstream work*: <amendments to N specs, M new reference doc edits, K new ADRs needed>
>
> ---
>
> *(Repeat per conflict.)*

#### 3b: Cross-conflict consolidation

If multiple conflicts share a root cause (e.g., a vision Non-Goal change cascades into multiple ADRs), name the *root* and treat the cascade as one decision, not several. Do not present 5 separate "amend this layer" choices when the user is really making one call.

---

### Step 4: Propose Alternatives

For every conflict (or the consolidated root from 3b), evaluate the standard alternatives:

| Alternative | Description |
|-------------|-------------|
| **Scope-down** | Do less than the proposal asks. Some intent is lost; the conflict shrinks or disappears. |
| **Scope-around** | Achieve the same outcome via a different path that doesn't conflict. The user gets what they want, the canon stays intact. |
| **Defer with forward-compat** | Do nothing now. Add hooks (nullable fields, additive schema, marked Open Questions) so a later canon change is additive rather than breaking. |
| **Amend canon** | Pay the cost. Edit canon, draft new ADR(s), cascade the changes. The proposal is honored as-stated. |
| **Drop** | Decide the proposal isn't worth the cost. Document the reasoning so the same proposal doesn't surface again without new information. |

Not every alternative makes sense for every conflict. Some proposals have no scope-around path; some don't admit forward-compat. Name the alternatives that genuinely apply; explicitly mark the ones that don't with a one-line "why not."

#### 4a: Alternatives table

> **Alternatives**
>
> | Alternative | What you get | What it costs | Notes |
> |-------------|--------------|---------------|-------|
> | Scope-down to <X> | <subset of original intent> | <reduced canon impact> | Loses <intent Y> |
> | Scope-around via <Z> | <full intent> | <new artifact / different path> | Doesn't amend canon; introduces <new component> |
> | Defer with forward-compat (<hook>) | <nothing now; future-additive> | <small now; preserves option> | Doesn't decide; revisit at <trigger> |
> | Amend canon (per Step 3 impact map) | <full intent> | <impact from Step 3> | Drafted ADR(s) and edits in Step 6 |
> | Drop | <nothing> | <documented decision; doesn't recur> | Reason: <why> |
> | ~~Scope-around via <W>~~ | n/a | n/a | Not applicable: <why> |

#### 4b: Recommend

After presenting the alternatives, recommend one. State the recommendation in one sentence, then a one-paragraph rationale citing the trade-offs you weighted.

> **Recommendation**: <Alternative X>.
>
> *Why*: <one paragraph — what made this the right call given the proposal's intent, the canon's rationale, and the cost of each alternative>.

---

### Step 5: User Picks a Path

Present the alternatives table + recommendation and ask the user to pick. Single choice. Do not move on without an explicit decision.

> *"Which path do you want to take? (1: scope-down / 2: scope-around / 3: defer / 4: amend canon / 5: drop / other: describe)"*

If the user proposes a different alternative not in the table, evaluate it — add a row to Step 4's table, update the impact map in Step 3 if needed, and re-present.

#### 5a: Route by chosen path

| Chosen path | Next step in this prompt | Hand-off after this prompt |
|-------------|---------------------------|----------------------------|
| Scope-down | Skip to Step 8 | Invoke `spec-generator` with the scoped-down proposal |
| Scope-around | Skip to Step 8 | Invoke `spec-generator` with the alternative approach |
| Defer with forward-compat | Step 6b (write the forward-compat note) | Likely none — note lives in vision Open Questions or workplan |
| Amend canon | Step 6, 7 (draft + execute) | After Step 7, invoke `spec-generator` for downstream spec amendments |
| Drop | Step 6c (record the decision) | None |

---

### Step 6: Produce the Artifact (Path-Specific)

#### 6a: Amend canon — draft edits and ADRs

For each canon edit named in Step 3's impact map, produce a concrete edit plan. Two artifacts emerge:

**(i) Edit plan table** — what files change, what the change is

> **Proposed canon edits**
>
> | File | Section | Change |
> |------|---------|--------|
> | `docs/product/vision.md` | Non-Goals (line NN) | **Remove** "<exact line>" |
> | `docs/product/vision.md` | Non-Goals | **Add** new line: "<text>" |
> | `docs/product/vision.md` | Open Questions | **Update** entry "<X>" to reference ADR-NNNN |
> | `docs/architecture/overview.md` | `#<anchor>` | **Amend** invariant from "<old>" to "<new>" |
> | `docs/decisions/NNNN-<existing>.md` | Amendments section | **Append** amendment dated YYYY-MM-DD with summary of the change |

**(ii) New ADR draft(s)** — full content

For each significant decision the canon edit hangs on, draft a new ADR following `docs/decisions/0000-template.md`. Use the next available ADR number (read `docs/decisions/` and pick the next integer after the highest existing). Status defaults to `proposed`. The Decision-Makers field is the user (or whoever they name).

The ADR draft should include:
- **Context**: the proposal, the conflicting canon, the rationale for amending (from Step 2 + Step 3)
- **Decision**: what is now true after this ADR
- **Consequences** (Positive / Negative / Neutral): drawn from Step 3's impact map
- **Notes**: relationships to other ADRs (Supersedes / Extends / Related to per `docs/decisions/_adr-policy.md`)

If the change *amends* an existing ADR rather than supersedes it, **do not draft a new ADR** — instead, draft an amendment entry to append to the existing ADR's Amendments section. Per `docs/decisions/_adr-policy.md`: amend for clarifications / learnings / external changes; supersede for reversing the decision.

Present both artifacts (edit plan + draft ADRs) for user review:

> *"Here is the proposed edit plan and N draft ADR(s). Review for accuracy and completeness. Reply with approvals (per item or 'all'), or with edits / objections."*

#### 6b: Defer with forward-compat — write the note

Forward-compat is the lightest-weight path. Produce a short note that:
- States what is being deferred
- States the trigger that should cause re-evaluation ("when external consumers wire up to wire shapes," "when N users request multi-vault," "after step 5")
- States the forward-compat hook(s) that should be in place now to make the future change additive

Suggest where the note should live:
- Vision **Open Questions** section (if the deferred decision affects guiding principles or Non-Goals)
- Workplan note (if the hook applies to an in-flight step)
- Spec **Open Questions** (if the hook is feature-scoped)

> **Forward-compat note**
>
> *Defers*: <decision>
> *Trigger to revisit*: <condition>
> *Hooks to put in place now*: <list>
> *Suggested location*: <file + section>

The user reviews and you apply (Step 7 still runs for this path — but the artifact is just the note, not edits to canon).

#### 6c: Drop — record the decision

Even "drop" deserves a written trace so the same proposal doesn't keep recurring without new information. Suggest an entry to vision's Open Questions (if the proposal touches product canon) or to a notes file (if it's strictly engineering-side):

> **Dropped proposal**
>
> *Proposal*: <PROPOSAL from Step 1>
> *Reason*: <one paragraph from Step 4 evaluation>
> *Reconsider when*: <new information that would justify revisiting>
> *Suggested location*: <file + section>

---

### Step 7: Review & Execute

For paths that produce edits (6a, 6b, 6c), present every proposed edit/file/note for explicit user approval before writing.

#### 7a: Approval gate

Do not write any file without explicit per-artifact or "all" approval. Approval shapes:

- *"Approved — apply all"*
- *"Approved 1, 2, 3; reject 4"* (then re-present 4 for revision)
- *"Edits to ADR-NNNN: <changes>; otherwise approved"*
- *"Reject all — go back to Step 4 with <new constraint>"*

#### 7b: Execute approved edits

Apply edits exactly as approved. For each:
- **Vision / spec / architecture edits**: edit in place; bump Version in frontmatter where applicable; add a row to Revision History
- **New ADR**: write to `docs/decisions/NNNN-<short-title>.md` using the template
- **ADR amendment**: append the dated amendment block to the existing ADR's Amendments section
- **Forward-compat note**: append to the named location
- **Dropped-proposal record**: append to the named location

#### 7c: Verify

After execution:
- Cross-links between edited files are valid
- ADR numbering is contiguous
- Revision History entries are present where required
- Frontmatter Version increments are consistent

---

### Step 8: Hand Off

Based on the chosen path, recommend the next workflow.

| Chosen path | Recommended next |
|-------------|------------------|
| Scope-down | Invoke `spec-generator` for the scoped-down feature |
| Scope-around | Invoke `spec-generator` for the alternative approach |
| Defer with forward-compat | Likely no spec needed; the note is the artifact. If hooks need implementation, invoke `spec-generator` for those |
| Amend canon — downstream specs need amendment | Invoke `spec-generator` for each amended-spec target listed in Step 3 |
| Amend canon — no downstream spec impact | None; the canon edits stand alone |
| Drop | None |

State the hand-off explicitly:

> *"Canon updates are in. Next: invoke `spec-generator` to produce amendments to `docs/specs/<a>.md`, `docs/specs/<b>.md`, and a new spec for `<c>`. Each will pick up the new canon (ADR-NNNN, vision changes) automatically during its Phase 1 LDS research."*

---

## Final Report

Produce a structured report at the end of the session, regardless of which path was chosen.

```markdown
# LDS Canon Exploration Report

**Date**: {date}
**Proposal**: {PROPOSAL verbatim}
**Chosen path**: {scope-down / scope-around / defer / amend / drop}

## Conflicts Identified
{table from Step 1c}

## Rationale (Why current canon exists)
{summary from Step 2a}

## Impact Map
{summary from Step 3, root-causes consolidated per 3b}

## Alternatives Evaluated
{table from Step 4a}

## Recommendation
{from Step 4b}

## Decision
{the user's pick from Step 5}

## Artifacts Produced
{depending on path:
  - Amend: list of edited files + new ADRs + amendment entries
  - Forward-compat: note location + content
  - Drop: dropped-proposal record location + content
  - Scope-down/around: no LDS artifacts; hand-off destination only
}

## Hand-Off
{from Step 8}

## Next Spec-Generator Targets (if any)
{list of specs to draft / amend with spec-generator}
```

---

## Quick Reference

### When in doubt: which alternative to weight

- **If the proposal's intent could be substantially achieved without changing canon** → weight scope-around heavily.
- **If the canon item is older than the project's current understanding and was set conservatively** → weight amend-canon higher; the canon may be wrong, not the proposal.
- **If the proposal is speculative and the trigger to act on it is months out** → weight defer with forward-compat.
- **If the proposal exists because of a real, current need but the cost is high** → present scope-down explicitly; sometimes a 60% solution unblocks the user.
- **If you cannot construct a plausible scope-around or scope-down** → that's a strong signal the user genuinely needs amend-canon; don't make them argue against straw alternatives.

### ADR drafting cheats

- **Number**: read `docs/decisions/`; the next ADR is `max(NNNN) + 1`, zero-padded to 4 digits
- **Status**: `proposed` for drafts; the user changes to `accepted` when they accept
- **Decision-Makers**: the user, unless they name others
- **Template**: `docs/decisions/0000-template.md`
- **Policy** (amend vs supersede vs extend): `docs/decisions/_adr-policy.md`
- **Cross-references**: per the policy, reference Supersedes / Extends / Related-to in the Notes section

### What this prompt does NOT do

- It does not write specs. Specs are downstream of canon; once canon is settled, hand off to `spec-generator`.
- It does not skip the rationale step. Moving to alternatives without surfacing the why produces solutions that miss the point.
- It does not commit edits without per-artifact approval. The LDS canon is too important for "looks good, ship it" auto-application.
- It does not assume vision is wrong. Vision is the highest-authority layer; the bar for amending it is high. Lean on scope-around and forward-compat first; reach for amend-canon only when the alternatives genuinely don't fit.

### Multi-layer conflicts

A proposal that conflicts with multiple layers (e.g., vision + ADR + 3 specs simultaneously) is common — particularly when the proposal touches a foundational decision. Handle by:

1. Enumerate all conflicts (Step 1c)
2. Identify the *root* — the highest-authority layer's conflict that the others derive from (Step 3b)
3. Resolve at the root; the lower-layer conflicts cascade automatically once the root is decided
4. Draft canon edits for every affected layer in one approval cycle (Step 6a)
5. Hand off to `spec-generator` for the downstream spec work (Step 8)

Do not propose alternatives separately for each conflict; that fragments the decision the user needs to make.
