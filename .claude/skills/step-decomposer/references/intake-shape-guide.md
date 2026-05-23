# Intake Shape Guide

A walkthrough of `notes/proposals/intake-output-template.md`, section by section, with what good content looks like and what should be deferred to the spec or a later round.

The output artifact must use the same section names, order, and field labels as the template. This guide is about *quality of content*, not structure.

---

## Frontmatter

```
**Status**: draft
**Date**: <YYYY-MM-DD>
**Intake inputs**:

- `<spec path>` — feature spec
- `<stories path>` — user stories (if present, otherwise omit this line)
```

Status stays `draft` when the artifact is first written. The human reviewing it is responsible for moving it forward (to `accepted`, `in progress`, etc.) — the skill does not change status on the user's behalf.

---

## Summary

One paragraph. Implementation-neutral: what the feature is, why this breakdown exists, what the resulting sequence accomplishes when complete.

**Strong**: *"Adds semantic search alongside the existing exact-match content search. This breakdown sequences the work so the embedding pipeline can be verified end-to-end before search results integrate into the CLI."*

**Weak**: *"This roadmap implements semantic search."* (Says nothing the title does not already say.)

Avoid stating who is doing the work, when it will be done, or how the round will be executed. Those details belong outside the artifact.

---

## Source Inputs

A table of the documents this breakdown was built from.

| Source | Type | Role in intake |
|---|---|---|
| `docs/specs/semantic-search.md` | spec | primary |
| `docs/specs/semantic-search-stories.md` | stories | supporting |
| `docs/decisions/0007-embedding-model.md` | ADR | background |

Use the actual paths. `primary` is the spec being decomposed; `supporting` is anything that materially shaped the breakdown (stories, related specs); `background` is what was read for context but did not drive decisions. Do not pad with files that were not actually consulted.

---

## Candidate Outcomes

The user-visible results this round produces, each with a verification signal.

```
- Outcome: User can issue a semantic search and see ranked results
  - Source: spec § Behavior, story SS-3
  - User-visible result: `hmn search --semantic "<query>"` returns N ranked hits
  - Verification signal: integration test issues a known-good query and asserts ordering
```

Outcomes are about what the user can observe, not about internal implementation milestones. "The embedding service is wired up" is an implementation step; "queries return ranked semantic results" is an outcome.

If an outcome cannot be verified without a manual check, name the manual check explicitly. "Looks right" is not a verification signal.

---

## Proposed Roadmap Shape

The heart of the artifact. One subsection per step.

```
### Step N — <short name>

**Goal**: <one sentence>

**Shipping criteria**:

- [ ] <observable condition>
- [ ] <observable condition>

**Deferred decisions resolved in this step**:

- Decision: <what is settled>
  - Source: <spec section / story ID>
  - Why this step: <why now, not earlier or later>

**New deps**:

- <crate / package / external service / "none yet">

**Risk**: low / medium / medium-high / high

**Source coverage**:

- <source>: <ids or short labels>
```

Per-field guidance:

- **Goal** is one sentence. Two sentences means the step is doing two things — split it.
- **Shipping criteria** are checkable. "Works correctly" fails; "schema migration applied and `index.sqlite` opens against the new shape" passes.
- **Deferred decisions resolved in this step** is the field where Open Questions from the spec get closed. If a step does not close any deferred decisions, omit the field rather than writing "none".
- **New deps** captures only deps this step *introduces*. Deps used by earlier steps are not re-listed.
- **Risk** uses one of the four labels. Justify medium-high and high in the same line in parentheses: *"Risk: high (depends on the sqlite-vec extension loading reliably on macOS arm64 in CI)"*.
- **Source coverage** is short labels — story IDs, section anchors, outcome names — not prose.

### Step naming

Name each step by its outcome, not by the module it touches. *"Step 3 — Ranked semantic results"* is better than *"Step 3 — Query module"*. The reader should be able to tell from the name what changes when the step ships.

---

## Coverage Map

A flat table mapping every requirement, story, or outcome from the source inputs to its disposition.

| Source item | Proposed step | Status | Notes |
|---|---|---|---|
| `stories.md#SS-1` | Step 1 | planned | |
| `stories.md#SS-2` | Step 2 | planned | |
| `stories.md#SS-3` | — | deferred | needs vector cache, see Deferred Items |
| `spec.md#open-questions/Q1` | — | open | blocks Step 2 sequencing |

`Status` is one of: `planned`, `deferred`, `out-of-scope`, `open`. Every row links a source item to one of: a step, a Deferred entry, an Out-of-Scope entry, or an Open Question. No row is left dangling.

If the spec is decomposed without a stories file, the source items are spec sections (e.g. `spec.md#behavior/normal-flow`). The coverage map still has to be complete.

---

## Deferred / Out-of-Scope Items

Items the round will not ship. One bullet per item:

```
- Item: Caching of query embeddings
  - Source: spec § Implementation Notes
  - Reason: Not load-bearing for v0 correctness; can be added after the round ships if hit-rate measurements warrant it
  - Revisit trigger: Query latency exceeds 200ms p95 in production use OR storage budget for query log grows past 100MB
```

The Revisit trigger is the discipline that prevents deferrals from becoming forgotten work. "Later" is not a trigger; a measurable condition is.

`Deferred` means it will likely come back; `Out-of-scope` means it explicitly will not (or belongs to a different spec entirely). When in doubt, prefer Deferred — the artifact is easier to change than the round's scope is.

---

## Open Questions

Genuinely unresolved questions. One bullet per question:

```
- Question: Should the embedding model be versioned in the index schema?
  - Why it matters: A model swap invalidates all existing embeddings; if the schema does not track the model, swaps require ad-hoc reindex logic
  - Blocks roadmap? yes
  - Suggested owner: spec author
```

Two failure modes to avoid:

1. **Putting answered questions here.** If you know the answer, write the decision into the step that resolves it; do not park it as a question.
2. **Putting open implementation details here.** "Should we use `bincode` or `serde_json` for serialization?" is a workplan decision, not an intake-blocking question. Drop it.

`Blocks roadmap?` is `yes` if the question would change which steps exist or their ordering. Otherwise `no`.

---

## Recommendation

One of three next actions, with a one- or two-sentence rationale:

```
Proceed to:

- [ ] Draft the workplan for Step 1 at `<path>`
- [ ] Refine the spec first
- [ ] Refine the breakdown first

Rationale: <one or two sentences>
```

The template's checkboxes mention `notes/roadmap/roadmap-N.md` and `notes/roadmap/step-NN-workplan.md` as defaults — those paths are Hypomnema-specific. For other projects, rewrite the checkbox text to name the actual next artifacts. The structure (checkbox list + rationale) stays the same.

Pick exactly one checkbox unless two are genuinely linked (e.g. *"refine the breakdown, then draft step 1"* is a valid pair if the refinement is small).

---

## Human Review Notes

Leave empty on first write. The human reviewing the artifact appends decisions here:

```
## Human Review Notes

(append review decisions here)
```

The skill does not write into this section. If the user gives review feedback during the conversation, capture it in the relevant step / question / deferred entry — not in this trailing section. This section is for *post-handback* review, not for in-conversation edits.
