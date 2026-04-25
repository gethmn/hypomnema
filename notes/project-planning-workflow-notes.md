# Project-Planning Workflow Notes

**Started**: 2026-04-24, while planning the initial implementation kick-off (steps 1–5) for Hypomnema.

**Purpose**: A working description of the planning process we are inventing as we go. Append-mostly. Revise as reality teaches us what fits.

**Status**: Draft, untested in practice. The first real exercise is the steps-1–5 build that this document is being created alongside.

---

## The three phases

### Phase A — Roadmap

A short, scannable document covering the full scope of a planning round (here: steps 1–5). Lives at `docs/roadmap/roadmap.md`.

**Per step it answers**:
- **Goal** — what is demonstrably working at the end
- **Shipping criteria** — the test that says "done"
- **Deferred decisions** — TBDs from specs/vision that this step will resolve, with file:line references
- **New deps** — crates added in this step
- **Risk** — low / medium / medium-high / high, with one-line rationale

**Does not contain**:
- File-level task lists
- Test code or test plans
- API shapes (those are spec concerns)
- Implementation details

**Read time**: 5 minutes. If it takes longer, it's too detailed.

### Phase B — Workplan (per step, just-in-time)

A concrete task list for **the step being started next**, written immediately before implementation begins. Lives at `docs/roadmap/step-NN-workplan.md` while active.

**Per step it answers**:
- Ordered task list (each task should be mergeable on its own)
- Files touched / modules created
- Test strategy for the step
- Cross-references to relevant ADRs, specs, and skills
- Definition-of-done checkboxes

**Lifecycle**: Created when the step starts. Updated as tasks complete. Archived or deleted when the step ships.

**We do not write all five workplans up front.** They will rot. Each is written when its step's turn arrives.

### Phase C — Build

Implementation against the workplan. The user reviews before code lands and at step boundaries.

---

## TBD handling

Per Beau's call: **just-in-time through step 5**.

- Each roadmap step lists the TBDs it will resolve
- The workplan expansion turns each TBD into either a code-level decision (captured in the commit) or a new ADR (when the decision is significant enough to outlive the immediate code)
- TBDs that don't belong to any step in this round stay open in their original spec/vision file

---

## PRD / spec-generator scope policy

**Decision (2026-04-25)**: PRDs and PRD-shaped artifacts in this project are always **feature-scoped** and become specs (`docs/specs/<feature>.md`) after decomposition. They do not sit alongside `docs/product/vision.md` as a parallel canon.

**Why**: the LDS already provides a PRD-Lite at the vision level. A second PRD-shaped artifact at the same level would duplicate problem statements, success criteria, and non-goals across two documents and confuse the LDS authority order. Keeping PRDs feature-scoped means decomposition is mostly 1:1 with a spec, which collapses the prd-generator's downstream work.

**How to apply**: if a proposal's scope is wide enough that it would amend `vision.md` rather than slot into a spec, treat that as a signal to *not* use the prd-generator (or its successor) — that's product-level work and warrants editing `vision.md` directly with an ADR for the load-bearing decisions. The spec-generator successor (see [`proposals/spec-generator-handoff.md`](./proposals/spec-generator-handoff.md)) is built on this assumption.

---

## Step boundary ritual

When a step ships:

1. Mark the step done in the roadmap (e.g. add a `**Status**: shipped <date>` line at the top of its section)
2. Capture any ADRs that hardened during the build
3. Update the roadmap if reality drifted from the original plan (note what changed and why)
4. Append a short retrospective to this file: what worked, what didn't, what we'd do differently
5. Expand the **next** step into a workplan
6. User reviews the new workplan before I (Claude) start coding

---

## Where artifacts live

| Artifact | Location | Lifecycle |
|----------|----------|-----------|
| Roadmap | `docs/roadmap/roadmap.md` | Long-lived; revised at step boundaries |
| Workplan (active step) | `docs/roadmap/step-NN-workplan.md` | Short-lived; one file per step, while active |
| Workplan (archived) | TBD — see open questions | After the step ships |
| In-flight proposals | `notes/proposals/<slug>.md` | Short/medium-lived; moves to `archive/` after approval and decomposition |
| Archived proposals | `notes/proposals/archive/<slug>.md` | Long-lived; frozen historical record |
| LDS gaps | `notes/lds-evaluation.md` | Long-lived; appended as new gaps surface |
| Process notes (this file) | `notes/project-planning-workflow-notes.md` | Long-lived; revised continuously |
| Plan-mode plans | `.claude/plans/` | Harness-scoped; not a project artifact |

`docs/roadmap/` is intentionally **outside** LDS's seven canonical layers. See [`lds-evaluation.md`](./lds-evaluation.md) for why.

---

## Open questions about the workflow itself

These are gaps in the process we'll have to answer through use:

1. **Workplan task granularity**: per-PR? per-coding-session? per-logical-unit (one task = one function/module)? My instinct is per-logical-unit, with rough PR boundaries marked, but we'll see what feels right after step 1.
2. **Archiving policy**: When step N ships, do we delete its workplan, move it to `docs/roadmap/archive/`, or keep it in place with a `**Status**: shipped` header? Pro of keeping: we can look back at what we planned vs. what we built. Pro of deleting: less clutter; the git history has the artifact anyway.
3. **Mid-step roadmap revision**: If I learn during step 3 that step 4's plan is wrong, do I revise the roadmap immediately, or wait for the step boundary? Probably revise immediately, but flag the change in the next step-boundary retro.
4. **Per-step retrospectives**: live in this file (one section per step), or as a separate `notes/retrospectives.md`? Starting in this file; split out if it gets unwieldy.
5. **What if a step proves wrong?** If step 3 reveals that step 4's goal is mis-specified, does that mean re-roadmapping? I think yes — pause, revise the roadmap, get user signoff, resume.
6. **Visibility of "currently in progress"**: A reader of the repo today cannot tell which step is active. A `**Currently working on**: step N` header at the top of `roadmap.md` would fix this. Adding it the first time we have an in-progress step.

---

## Retrospectives (one section per step, appended as we ship)

### Retro template

Every per-step retro follows this shape so the structured data accumulates comparably across steps. The coordinator fills the **Structured Eval** section per `notes/coordinator-playbook.md` § Post-build evaluation; the human (or coordinator + human together) fills the **Notes** section.

```
#### Step N (date shipped)

**Structured Eval**

*Batching outcomes:*
- Batch [tasks A, B]: <outcome — clean / split / escalated>. Assessment: <good batch / should have split / etc.>.
- Solo task M: scope signal <files touched, result-comment sentence count>. Adjacent-task overlap: <none / shared files with task M+1>. Assessment: <appropriate solo / could have batched with task M+1>.
- (one bullet per batch and per solo task)

*Escalations:*
- Count: N.
- By type: ambiguity=X, test-failure=Y, scope-question=Z, surprise-decision=W, other=V.
- Per-escalation: <todo id> — <type> — preventable with better workplan? <yes/no/notes>.

*Retries:*
- Tasks with retries: <task numbers>.
- Per task: <task M> — <N retries> — <failure type>.
- 2-retry ceiling hit without success: <task numbers, or "none">.

*Time and overhead:*
- Total wall-clock: <hh:mm>.
- Per-task wall-clock: <task M = mm:ss, ...>.
- Coordinator wake-up count: <N>.
- Context drift symptoms: <free-form notes, or "none observed">.

**Notes**

(Free-form prose: what worked, what didn't, what would we change for the next step. Subjective. The structured data above is the input; this is the synthesis.)
```

#### Step 1 (shipped 2026-04-25)

**Structured Eval**

*Batching outcomes:*
- No batches. All 7 workplan tasks ran as solo task agents (default-not-batch on the pilot run).
- Solo task 1 (Config module): scope = 3 files (`src/config.rs`, `src/lib.rs`, `tests/config.rs`), result-comment ~25 sentences. Adjacent-task overlap: task 2 consumed `LoggingConfig` from task 1 (lib-API dep, not file overlap). Assessment: appropriate solo — substantial new module + 14 tests; not a candidate to batch with task 2.
- Solo task 2 (Logging module): scope = 3 files (`src/logging.rs`, `src/lib.rs`, `Cargo.toml`), comment ~30 sentences. Overlap with task 3: both touched `src/lib.rs` only (re-export line). Assessment: appropriate solo — distinct module concerns and a Cargo.toml-feature decision flagged for coordinator review.
- Solo task 3 (Shutdown helper): scope = 2 files (`src/shutdown.rs`, `src/lib.rs`), comment ~18 sentences. Overlap with task 4: none (task 4 wrote a different binary file). Assessment: appropriate solo.
- Solo task 4 (`hmnd` binary): scope = 1 file (`src/bin/hmnd.rs`), comment ~30 sentences — surfaced an architectural finding (binary-crate `tracing` events bypass `compose_filter`'s module list). Adjacent (task 5): same trap pattern, distinct binary file. Assessment: appropriate solo — sequential discovery → coordinator-mediated forwarding to task 5 was load-bearing; batching could have hidden the trap behind silent in-batch workarounds.
- Solo task 5 (`hmn` binary): scope = 3 files (`src/bin/hmn.rs`, `src/cli.rs`, `src/lib.rs`), comment ~30 sentences. Adjacent (task 6): no file overlap. Assessment: appropriate solo. Confirmed task 4's trap as cross-binary, validating the structural-fix flag for step boundary.
- Solo task 6 (Smoke tests): scope = 1 file (`tests/skeleton.rs`), comment ~22 sentences. Adjacent (task 7): no overlap (tests vs docs). Assessment: appropriate solo.
- Solo task 7 (Doc updates): scope = 2 files (`docs/reference/cli.md`, `docs/reference/configuration.md`), comment ~12 sentences. No further adjacent task. Assessment: appropriate solo.

*Escalations:*
- Count: 0.
- By type: ambiguity=0, test-failure=0, scope-question=0, surprise-decision=0, other=0.
- Note: two "soft flags" (decisions called out for coordinator review without `needs-human` tag) emerged in tasks 2 (Cargo.toml feature toggle) and 5 (workplan literal struct shape that wouldn't compile). Coordinator accepted both; no human round-trip needed. The playbook's escalation model is binary (escalate / don't) but practice produced a useful third state — flagged-but-shipping. Worth a playbook note before step 2.

*Retries:*
- Tasks with retries: none.
- Per task: all 7 succeeded on first attempt.
- 2-retry ceiling hit without success: none.

*Time and overhead:*
- Total wall-clock: 34m 34s (18:57:01 → 19:31:35, 2026-04-25).
- Per-task wall-clock from todo `created_at` to `completed_at`: task 1 = 6m 32s, task 2 = 11m 46s, task 3 = 14m 54s, task 4 = 21m 37s, task 5 = 26m 38s, task 6 = 30m 03s, task 7 = 33m 49s. (The bulk of each later task's wall-clock is queue time behind its blocker; net agent-time once unblocked was 3m 13s – 6m 52s per task.)
- Coordinator wake-up count: 7 (one timer fire per task; all genuine completions, zero false-positive idle wake-ups).
- Context drift symptoms: none observed. The 7-task scale is too short to stress-test drift; revisit at step 5 (more tasks, longer-lived state).

**Notes**

- *The roadmap → workplan → build cadence works.* All four shipping criteria from `roadmap.md` § Step 1 passed end-to-end: `hmnd --config` idles cleanly with a config summary banner; SIGINT exits 0 with a drain-complete log line; `hmn --help` renders the resolved subcommand surface; 42/42 tests green (26 lib + 9 config integration + 7 skeleton). No deferred decisions slipped past the workplan.
- *Rolling-context scratchpad as a "context-passing baton" was validated.* Task 4 surfaced the binary-target tracing trap; the coordinator's outcome paragraph forwarded it as guidance into task 5's bootstrap; task 5 applied the workaround correctly without escalating, and confirmed the trap as cross-binary (which strengthened the case for the structural fix). This is the load-bearing pattern the playbook implies but doesn't make explicit. Worth promoting to a named pattern in the playbook before step 2.
- *Scratchpad task-status table at the top went stale.* The playbook prescribes append-only updates ("append a one-paragraph summary" via `scratchpad_append`), but the template puts a status table near the top that no append can update. The "Per-task outcomes" section at the bottom is the live source of truth, and a reader scanning the top sees stale "pending" rows for completed tasks. Two viable fixes for step 2: (a) coordinator does `scratchpad_write` on completion to refresh the table at the top — costs an extra revision per task; or (b) drop the table from the template and rely on `todo_list(tags=["step-NN"])` for current status. Lean toward (b) — Solo todos are already the source of truth for status.
- *Wake-up message bodies got long and repetitive.* Each ~600 chars of boilerplate restating playbook routing rules. Future-me reads the playbook anyway. A condensed form ("Wake-up for todo \<id\>; route per § Wake-up routing; current task \<M\>; next \<M+1\>") would be enough. Worth condensing the template in the playbook.
- *Idle-detection reliability: 7/7 fires were genuine completions.* `timer_fire_when_idle_any` performed cleanly as a "task agent done" signal for this run. Open-questions list said this needed validating across multiple steps — one data point, but a clean one.
- *Step-boundary follow-up (not-an-ADR, intentional):* lift the binary-target tracing workaround into `compose_filter()`. Per-call-site `target: "hypomnema::hmnd"` / `target: "hypomnema::hmn"` is in both binaries; extending each `BinaryKind`'s default filter to include the binary crate name would replace the per-call tagging with one filter-level fix. Step 2's indexer will add many `tracing::*!` call-sites where forgetting the workaround would silently lose log lines — the cost of the workaround grows nonlinearly with binary log volume. Recommend doing this lift at the start of step 2 before the indexer scaffolding lands. Not an ADR (implementation cleanup, not load-bearing).
- *"Soft flag" pattern.* Tasks 2 and 5 both flagged judgment calls for coordinator review without escalating. Tasks shipped cleanly, coordinator accepted, zero human round-trips. This is a useful third state the playbook should name explicitly: "Decision flagged for coordinator review — proceeded with X, see results comment for trade-off." Could go in the TASK AGENT § Reporting section as an option alongside escalation. Worth adding before step 2.
- *Pilot risk assessment.* Step 1 was a low-risk skeleton. The coordinator pattern handled it cleanly with no pressure on the failure-handling, retry, or escalation paths. Step 2 (Scan + hash, medium risk, schema-design lock-in) and step 3 (Watcher, medium-high risk, the project's biggest landmines per the roadmap) will test the parts of the playbook that didn't fire here.

#### Step 2 (shipped 2026-04-25)

**Structured Eval**

*Batching outcomes:*
- No batches. All 7 workplan tasks ran as solo task agents (default-not-batch carried forward from step 1's clean pilot).
- Solo task 1 (Compose-filter binary-target lift): scope = 3 files (`src/logging.rs`, `src/bin/hmnd.rs`, `src/bin/hmn.rs`), result comment ~3 paragraphs. Adjacent (task 2): no file overlap. Assessment: appropriate solo — surgical cleanup with its own test surface.
- Solo task 2 (Globset matcher + `.git/**` default): scope = 3 files (`Cargo.toml`, `src/config.rs`, `tests/config.rs`), comment ~6 sentences. Adjacent (task 3): both touched `Cargo.toml` only. Assessment: appropriate solo — globset is a config-side concern, store deps are a separate concern.
- Solo task 3 (Store module): scope = 6 files (`Cargo.toml`, `Cargo.lock`, `src/lib.rs`, three new under `src/store/`), comment ~12 paragraphs incl. soft flag. Workplan-flagged medium risk (schema lock-in). Assessment: appropriate solo — soft flag pulled forward `tempfile = "3.10"` to dev-deps to allow a real-FS WAL test.
- Solo task 4 (Indexer module): scope = 6 files (`Cargo.toml`, `Cargo.lock`, `src/lib.rs`, three new under `src/indexer/`), comment ~14 paragraphs incl. soft flag. Workplan-flagged medium-high risk. Assessment: appropriate solo — task-flagged-risky rule held; the soft flag (5 extra scanner-level unit tests beyond the workplan ask) tightened the next task's debug loop.
- Solo task 5 (Wire `hmnd` binary): scope = 1 file (`src/bin/hmnd.rs`), comment ~6 paragraphs incl. manual smoke verification. Adjacent (task 6): no file overlap. Assessment: appropriate solo — even a 1-file task carries integration concerns (log-line shape verification end-to-end).
- Solo task 6 (Integration tests): scope = 3 files (1 new `tests/scan.rs`, 2 extended), comment ~10 paragraphs incl. soft flag. Adjacent (task 7): no overlap. Assessment: appropriate solo.
- Solo task 7 (Doc updates): scope = 3 doc files, comment ~5 paragraphs incl. soft flag. Assessment: appropriate solo.

*Escalations:*
- Count: 0.
- By type: ambiguity=0, test-failure=0, scope-question=0, surprise-decision=0, other=0.
- *Soft flag count: 4* (tasks 2.3 / 2.4 / 2.6 / 2.7), vs. step 1's 2. The pattern step 1 named ("decision flagged for coordinator review without escalating") proved load-bearing across a higher-complexity step. Coordinator accepted all 4 with zero human round-trips. Worth promoting to a first-class entry in the playbook's TASK AGENT § Reporting before step 3 — it currently lives only in the step-1 retro.

*Retries:*
- Tasks with retries: none.
- Per task: all 7 succeeded on first attempt.
- 2-retry ceiling hit without success: none.

*Time and overhead:*
- Total wall-clock: ~30m (todo creation 21:58:46 → last task completed 22:27:44, 2026-04-25).
- Per-task wall-clock from todo `created_at` to `completed_at`: task 1 = 3m 22s, task 2 = 5m 58s, task 3 = 10m 40s, task 4 = 17m 16s, task 5 = 20m 21s, task 6 = 25m 26s, task 7 = 27m 57s. (As in step 1, these include queue-behind-blocker time. Net agent-time ranged ~2m–7m per task; total summed agent-time ≈ 26m.)
- Coordinator wake-up count: 7 (one per task; all genuine completions; zero false-positive idle wake-ups).
- Context drift symptoms: none observed. Rolling-context scratchpad's § Per-task outcomes was actively used to forward soft-flag downstream impact between tasks (notably 2.3→2.4 and 2.4→2.6).

**Notes**

- *Net wall-clock dropped vs. step 1 despite higher complexity.* Step 1: 34m for low-risk skeleton. Step 2: 30m for medium / medium-high tasks (schema design + first spawn_blocking workload). Two contributing factors visible from the data: (a) every task agent went idle exactly once (vs. step 1's occasional re-prompts), suggesting tighter workplan task descriptions reduced agent decision fanout; (b) more forward-notes flowed task→task via the scratchpad, pre-empting questions that would have surfaced as soft flags or re-reads. The roadmap→workplan→build cadence is paying off as the team (one agent + one coordinator) gains shared vocabulary.
- *Soft flags are now load-bearing.* The pattern step 1 surfaced and named got exercised 4× in step 2. Two were forwarded as guidance to the next task agent via the rolling-context scratchpad (2.3's tempfile-pulled-forward → 2.6 skipped Cargo.toml edit; 2.4's 5-extra-unit-tests → 2.6 chose scope-not-mirror). One was post-hoc accepted (2.7's prose-and-example consistency). One was a defensible scope-creep flag (2.6's scope-not-mirror, which the agent had also been pre-flagged about by 2.4's forward note). All zero round-trips to the human. Recommend promoting "Soft flag" from the step-1 retro mention to a first-class subsection under the playbook's TASK AGENT § Reporting before step 3.
- *Forward-note pattern in the scratchpad's § Per-task outcomes is the load-bearing channel.* Step 1's retro called this out as a "context-passing baton" worth naming. Step 2 ran on it — every per-task outcome ended with a `Forward note for Task X.Y` paragraph addressing the next agent (sometimes two). The next task's bootstrap prompt referenced "forward notes are at the end of Task X entry" by line, and agents read them. This is now a deliberate pattern the playbook should describe in COORDINATOR § Per-task execution loop step 6 (currently says only "append a one-paragraph summary"; bump to "and write a `Forward note for Task M+1` paragraph if anything material applies").
- *Idle-detection reliability remains 100%.* 7/7 fires were genuine completions in step 2 (and 7/7 in step 1). Two clean steps is signal enough to retire the playbook open question "Idle-detection false positives" — `timer_fire_when_idle_any` is reliable as a "task agent done" signal in this workflow shape. Will keep an eye on it as task complexity grows (step 3's watcher work will spawn longer-running tests).
- *Workplan TBDs handled cleanly.* All four step-2 deferred decisions (auto-rescan default, ignore-pattern set incl. VCS, symlink handling, schema-migration strategy) resolved at workplan time and held through the build with zero in-build revision. None warranted ADR promotion. Confirms the step-1 instinct that "if the resolution fits in 1–3 paragraphs of workplan prose with a 'Why', it does not need to be an ADR." Holding that rubric.
- *Cross-task `Cargo.toml` ordering didn't bite.* Five of seven tasks edited `Cargo.toml`. They ran sequentially (blockers preserved), so no merge friction. If a future step batches Cargo.toml-touching tasks (e.g., a step that adds 2-3 unrelated small deps in one PR), this is the file most likely to surface a coordination question. Worth noting; not a current problem.
- *Step-boundary roadmap revision worked as a side-effect of the workplan.* The workplan's "pulled `globset` forward from step 5" call became a one-line edit in roadmap step 5's deps list at boundary. Open question 3 from this file ("Mid-step roadmap revision: revise immediately or wait?") is partially answered: small revisions that fall out of a step's scope are cheap to apply at boundary; we don't need a separate "revise the roadmap mid-step" ritual for them.
- *No structural fixes needed before step 3.* Step 1 retro left one structural fix (the binary-target tracing lift) which became Task 2.1. Step 2 leaves no analogous step-3-blocker. Step 3 (watcher) can start its workplan immediately on the human's signal.
- *Coordinator process-context has not drifted at 7 wake-ups.* The playbook open question on context drift remains unresolved at small scale. Step 5 will be the stress test (more tasks, more forward notes).

### Step 3

### Step 3

_Not yet started._

### Step 4

_Not yet started._

### Step 5

_Not yet started._

### End-of-round retrospective (after step 5 ships)

_Not yet started. Should answer: did the roadmap→workplan→build cadence work? What would we change for the next round (steps 6–8)?_
