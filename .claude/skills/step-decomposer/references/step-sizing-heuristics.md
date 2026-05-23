# Step Sizing Heuristics

How to decide where a step ends and the next one begins. Use these as a checklist when drafting steps and as a critique pass when reviewing a draft breakdown.

---

## The shippability test

A step is correctly sized when:

1. It has **one observable outcome** that did not exist before the step started.
2. The outcome can be **verified on its own** — without running the next step.
3. The work fits into **a single coherent change** — a single PR's worth, not a multi-week branch.

If any of those three fail, the step is mis-sized.

### Diagnosing the failure

| Symptom | Cause | Fix |
|---|---|---|
| No outcome you can name | Step is purely internal plumbing | Merge it into the step that consumes the plumbing |
| Two outcomes in one step | Step is too big | Split into two steps |
| Outcome is verifiable only by running the next step | Step does not stand alone | Either fold the verification into this step, or merge with the step that does verify it |
| Outcome is "lay the groundwork for X" | Vacuous goal | Restate as a concrete piece of X that ships now |
| Step spans multiple unrelated areas | Step grouped by calendar, not by build order | Re-split by what depends on what |

---

## Verification signal patterns

Every step needs a way to know it is done. Acceptable shapes:

- **A test passes that did not pass (or did not exist) before.** Unit, integration, or end-to-end — any of them count.
- **A specific command produces a specific output.** "`hmn search foo` returns three ranked results in under 200ms" is a signal. "Search is faster" is not.
- **A new file or schema exists with a checkable shape.** "The `embeddings` table exists with columns (id, vector, model_version)" is a signal. "The schema is updated" is not.
- **A documented behavior is observable.** "A spec section's example transcript runs and produces the documented output" is a signal.

Unacceptable shapes:

- "It works."
- "Looks right."
- "The implementation is clean."
- "Ready for the next step."
- "All planned changes are merged." (This is about process, not about what changed.)

If a step's only verification is "the next step depends on it," the step is hiding its verification debt. Either give it a real signal or merge it into the next step.

---

## Splitting big steps

Common shapes for steps that should be split:

### Shape A — "Schema + behavior + UI" in one step

> *"Step 3 — Add semantic search: migration adds `embeddings` table, indexer populates it, CLI exposes `--semantic` flag, ranking algorithm respects model versions."*

Four outcomes, four potential verification signals, one step. Split:

1. Schema and population — done when the table exists and a known input file produces the expected rows
2. Query path — done when an internal query helper returns ranked rows for a sample query
3. CLI surface — done when `hmn search --semantic` works end-to-end
4. (Optional) Model-version handling — done when a version mismatch reindexes correctly

### Shape B — "Refactor + feature" in one step

> *"Step 2 — Move the indexer into its own module and add background reindexing."*

Two unrelated outcomes. Split:

1. Refactor — done when the existing tests pass against the new module shape
2. Feature — done when background reindex triggers correctly on a sample event

The refactor either ships first (so the feature lands on a stable base) or after (so the feature drives the refactor's shape). Either is fine; both in one step is not.

### Shape C — "Implementation + observation tooling" in one step

If the only way to verify the step is to *also* build the observation tooling, split the tooling into a prior step. Then the implementation step has something to point at for its signal.

---

## Merging small steps

Common shapes for steps that should be merged:

### Shape A — "Add a field" + "use the field"

If a schema field has no consumer yet, do not split adding the field from the first thing that reads it. Otherwise step N's verification signal is "the column exists," which is a hollow signal — schemas exist in the service of behavior.

### Shape B — "Plumb a value through" with no behavior change

Pure plumbing without an observable consequence at the end is not a step. Wrap it into the step that gives the value a consumer.

### Shape C — "Test infrastructure"

Adding a test harness, fixture set, or CI plumbing is a step only if the harness itself is the deliverable (e.g. "the project gains an integration test runner"). If the harness is built in service of a feature step, fold it into that step.

---

## Ordering

Once steps are sized, order them by **what has to be true before the next thing can ship**, not by:

- Which module the code touches
- Who is most likely to do the work
- What feels most exciting

Two ordering checks:

1. **No forward references.** Step N's shipping criteria do not depend on anything Step N+k produces (k > 0). If they do, either reorder or merge.
2. **Independent risk frontloading.** When two steps are equally available to run first, prefer the higher-risk one. Discovering a blocker in Step 1 is cheaper than discovering it in Step 4.

---

## Risk labels

- **Low** — well-trodden path; the project has done this kind of work before; failure modes are known.
- **Medium** — new territory in some dimension (new dep, new module shape, new external interface) but the unknowns are scoped.
- **Medium-high** — load-bearing unknown: a key assumption has not been validated yet, and if it falls the step has to be redesigned.
- **High** — load-bearing unknown AND a tight constraint (platform, performance, compatibility). Likely needs a spike before it can be confidently scheduled.

When labeling medium-high or high, name the unknown in parentheses on the same line:

> *"Risk: high (depends on sqlite-vec loading reliably under CI's sandboxed FS layout)"*

A high-risk step at the front of the sequence is healthier than the same step buried in the middle. If the breakdown has a high-risk step late in the order, surface that as an Open Question: *"Step 5 is high risk; should we frontload its load-bearing unknown as a spike before this round starts?"*

---

## Step count sanity

There is no fixed rule for how many steps a round should have. But:

- **One step** usually means the breakdown is not adding value over the spec. Reconsider whether decomposition is needed.
- **Two to five steps** is the common shape for a focused round.
- **More than seven** is a signal the round is too big, or the steps are too small. Re-examine sizing.

The right number is whatever satisfies the shippability test for each step without inflating step count for its own sake.
