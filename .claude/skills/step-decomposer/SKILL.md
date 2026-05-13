---
name: step-decomposer
description: Decompose a finished feature spec into a step-by-step round breakdown shaped like `notes/proposals/intake-output-template.md`. Use this skill when the user wants to turn a spec into a sequence of shippable steps, draft a roadmap for a feature, or get a step breakdown for X. Triggers on phrases like "break this spec into steps", "decompose the spec", "step breakdown for", "draft a roadmap for", "what steps does <feature> need". Reads from `docs/specs/<feature>.md` by default; an explicit input path overrides. Writes to `notes/roadmap/<feature>.md` by default; an explicit output path overrides. Produces artifact content only — no orchestration, role, or agent guidance.
---

# Step Decomposer

Turn a feature spec into a step-by-step breakdown. The output is an intake-shaped artifact: a summary, a proposed sequence of steps, a coverage map, deferred items, and open questions. It is a planning artifact, not an execution playbook.

## Core Philosophy

The spec defines **what** the feature is. The step breakdown defines **the order in which it can be built and verified**. Each step is one shippable outcome — something an implementer can finish, test, and check off before moving on. The breakdown is implementer-agnostic: it describes work, not who does it.

Key principles:

- **One shippable outcome per step.** If a step has no observable result on its own, it is mis-sized. Either merge it into the next step or split out what is actually shippable.
- **Every step has a verification signal.** "The next step works" is not a verification signal — it pushes verification down the chain until nothing is checkable. Each step must answer: how do I know this step is done?
- **Steps respect the spec, do not re-litigate it.** If you find yourself wanting to expand or amend the spec during decomposition, stop and surface it as an open question. Specs are settled before decomposition starts.
- **Phasing is a decomposition concern, scope is not.** The spec covers the full surface of the feature. The breakdown decides which slice of that surface ships first. Do not narrow the spec to match a shorter sequence.
- **No orchestration vocabulary.** The output describes steps a single implementer (human or agent) could execute in order. It does not reference roles, hand-offs, agents, or coordination tools — those belong to the execution layer, which this artifact deliberately leaves open.

## Process Overview

The flow has four phases:

1. **Locate inputs** — find the spec and any peer stories file
2. **Read and confirm scope** — read the spec; agree with the user on the in-scope surface
3. **Decompose** — propose steps, assign coverage, surface deferrals and open questions
4. **Write the artifact** — emit the intake-shaped file at the chosen path

---

## Phase 1: Locate Inputs

### Default input path

Read the spec at `docs/specs/<feature>.md`. If the user has not named a feature, ask:

> *Which feature's spec are we decomposing?*

Resolve `<feature>` against the names in `docs/specs/` (excluding `_template.md`).

### Override input path

If the user supplies an explicit path (e.g. `planning/X-spec.md`, `notes/proposals/X.md`, or anything else), use it. Always announce the resolved input path before reading:

> *"Reading the spec from `<path>`. Override?"*

This matters in two cases especially:
- **Non-LDS projects** that do not have a `docs/specs/` directory at all
- **Still-in-draft proposals** at `notes/proposals/<slug>.md` that have not promoted to `docs/specs/` yet

### Optional companion stories file

After resolving the spec path, check for a peer stories file at the same directory: `<spec-dir>/<feature>-stories.md`. If it exists, read it too. Stories populate the coverage map: every story should map to at least one step.

If the stories file is missing, do not fabricate one — proceed without it and note the absence in Open Questions.

---

## Phase 2: Read and Confirm Scope

Read the full spec before asking questions. The decomposition will be shaped by:

- **Behavior** — the states and flows the feature exposes
- **Data Schema** — the persisted shapes the feature reads and writes
- **Integration Points** — the modules and external systems the feature touches
- **Open Questions** — items the spec deliberately left unresolved
- **Implementation Notes** — any pre-stated phasing or sequencing hints from the spec author

After reading, surface your understanding of the in-scope surface:

> *"The spec covers A, B, C, D. I read C and D as the load-bearing surfaces; A and B feel like prerequisites. Does this match how you want the round to land?"*

Resolve any of the following before drafting steps:

1. **Surface narrowing the user wants for this round** (the round may not deliver every section of the spec at once — that is fine, the rest goes to Deferred / Out-of-Scope Items with a revisit trigger).
2. **Hard dependencies** the spec implies but does not spell out (e.g. "X cannot ship before Y indexes are populated").
3. **Open Questions from the spec** that would change the step sequence if answered one way or the other.

Do not start writing steps until the round's scope is settled.

---

## Phase 3: Decompose

Read `references/step-sizing-heuristics.md` before drafting steps. Read `references/intake-shape-guide.md` for what each section of the output should contain.

### Draft the steps

For each step, fill in:

- **Goal** — one sentence, observable
- **Shipping criteria** — checklist of conditions that have to be true for the step to be done
- **Deferred decisions resolved in this step** — items the spec listed as Open Questions that this step settles (with source)
- **New deps** — any dependencies the step introduces; default to "none yet" if nothing changes
- **Risk** — low / medium / medium-high / high
- **Source coverage** — which sources (spec sections, story IDs, outcomes) this step satisfies

### Coverage map

Build the coverage table: every requirement, story, or outcome from the source inputs maps to a step, a Deferred row, or an Out-of-Scope row. Anything uncovered becomes an Open Question with `Blocks roadmap?` set to `yes`.

### Deferred and Out-of-Scope

For each item the round will not ship, capture:

- the source it came from
- the reason it is deferred (not "we ran out of time" — the actual reason: dependency, risk, scope, awaiting decision)
- a revisit trigger (the condition under which this should be picked up again)

### Open Questions

For each unresolved question:

- why it matters (what it changes about the breakdown)
- whether it blocks the roadmap (does the round need this answered before starting?)
- a suggested owner (who is best positioned to resolve it)

Do not invent answers to questions the spec left open. If the question would change the step shape, raise it now.

### Recommendation

End with one of:

- **Proceed to draft a workplan** for step 1 (if the breakdown is settled and step 1 is ready to start)
- **Refine the spec first** (if decomposition surfaced gaps the spec needs to fill)
- **Refine the breakdown first** (if open questions block confident sequencing)

State the rationale in one or two sentences.

---

## Phase 4: Write the Artifact

### Default output path

Write to `notes/roadmap/<feature>.md`. Create the directory if it does not exist.

### Override output path

If the user supplies an explicit path, use it. Always announce the resolved output path before writing:

> *"Writing the step breakdown to `<path>`. Override?"*

If the target file already exists, confirm before overwriting.

### Shape

The artifact conforms exactly to `notes/proposals/intake-output-template.md` — same section names, same order, same field labels. Sections without content take a brief `TBD: <reason>` marker rather than disappearing.

The template's Recommendation section lists `notes/roadmap/roadmap-N.md` and `notes/roadmap/step-NN-workplan.md` as next-action targets. Those paths are Hypomnema-specific and may not apply to other projects — rewrite the Recommendation lines so they name the actual next artifacts for this project (e.g. "draft step 1 workplan at `planning/X-step-01.md`" for a non-LDS project). Keep the section's structure: a checkbox list of next actions plus a rationale.

### Frontmatter

```
**Status**: draft
**Date**: <today, ISO format>
**Intake inputs**:

- `<spec path>` — feature spec
- `<stories path>` — user stories (if present)
```

---

## Vocabulary Discipline

The artifact must read cleanly to someone with no execution-layer context. Avoid all of these terms in the output:

- *orchestrator*, *coordinator*, *researcher*, *builder*
- *agent*, *role*, *hand-off*, *spawn*, *delegate*
- *Solo*, *Duo*, *MCP*, *scratchpad*, *playbook*
- *round*, *cadence*, *retro*, *step boundary ritual*

When describing what happens in a step, write in plain implementer-neutral language: *"Step N produces X"*, *"Step N is done when Y is true"*. Do not write *"the coordinator hands step N to a builder"* or anything in that register.

Use *round* in the user-facing conversation if the user uses it — but in the written artifact, prefer phrases like *"this sequence of steps"* or *"this breakdown"*.

---

## Anti-Patterns to Watch For

Flag these when you see them:

1. **Unshippable step.** A step with no observable outcome on its own. Merge or split.
2. **Verification by next step.** Step N "verified" only because step N+1 needs it. Each step needs its own signal.
3. **Spec amendment in disguise.** The breakdown changes the feature's behavior or surface. Stop — that is a spec change, not a decomposition. Route back to the spec.
4. **Story orphaned.** A user story with no step covering it. Either add a step, or mark the story Deferred / Out-of-Scope with a reason.
5. **Phantom dependency.** A step labeled "depends on X" where X is not in this round and not in the spec. Either pull X into the round or remove the dependency claim.
6. **Step ordering by file layout, not by build order.** Steps grouped by which module they touch instead of by what has to be true before the next thing can ship.
7. **Orchestration vocabulary leaking in.** See the list above. The artifact stays implementer-neutral.

---

## Reference Files

- `references/intake-shape-guide.md` — section-by-section walkthrough of the intake template, with what good content looks like and what to defer
- `references/step-sizing-heuristics.md` — how to decide step boundaries and flag mis-sized steps
