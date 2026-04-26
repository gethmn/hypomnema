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

#### Step 3 (shipped 2026-04-26)

**Structured Eval**

*Batching outcomes:*
- No batches. All 6 workplan tasks ran as solo task agents (default-not-batch carried forward from steps 1 + 2).
- Solo task 3.1 (Filter helpers + debounce bump): scope = 6 files (`src/config.rs`, `tests/config.rs`, `docs/reference/configuration.md`, `src/lib.rs`, two new under `src/watcher/`), result-comment ~5 paragraphs incl. soft flag. Adjacent (3.2): touched `src/watcher/mod.rs` only (replaced the stub). Assessment: appropriate solo — pure-logic helpers + 4-place config flip; splitting kept filter-vs-pipeline test surfaces bisect-separated.
- Solo task 3.2 (notify deps + Watcher module): scope = 4 files (`Cargo.toml`, `Cargo.lock`, `src/watcher/mod.rs` replaced, `src/watcher/translate.rs` new), 16 new unit tests, comment ~6 paragraphs incl. soft flag. Workplan-flagged medium-high risk. Assessment: appropriate solo — task-flagged-risky rule held; soft flag on `RenameMode::Any|Other` was forwardable.
- Solo task 3.3 (Indexer single-file ops): scope = 1 file (`src/indexer/mod.rs`), 8 new unit tests, comment ~6 paragraphs (no soft flag). Adjacent (3.4): no file overlap. Assessment: appropriate solo — refactor of `run_blocking` into a shared per-file helper landed cleanly without touching the watcher module.
- Solo task 3.4 (Wire watcher into hmnd): scope = 2 files (`src/watcher/mod.rs` extended with `run_consumer` + backpressure logging, `src/bin/hmnd.rs` extended), comment ~5 paragraphs incl. soft flag + manual smoke verification. Workplan-flagged medium-high risk. Assessment: appropriate solo — composes 3.2 + 3.3 + existing shutdown plumbing; soft flag (`Send`-friendly `changed()` over `wait_for`) was load-bearing for 3.5.
- Solo task 3.5 (Integration tests): scope = 1 new file (`tests/watch.rs`, 321 lines, 9 tests), comment ~5 paragraphs (no soft flag). Adjacent (3.6): no file overlap. Assessment: appropriate solo — agent ran 6 consecutive clean local repeats to look for flakes before reporting.
- Solo task 3.6 (Doc updates): scope = 4 doc files (configuration.md was already complete from 3.1; verified), comment ~4 paragraphs (no soft flag). Assessment: appropriate solo.

*Escalations:*
- Count: 0.
- By type: ambiguity=0, test-failure=0, scope-question=0, surprise-decision=0, other=0.
- *Soft flag count: 3* (tasks 3.1 / 3.2 / 3.4), vs. step 2's 4 and step 1's 2. Two of the three (3.2 → 3.4 and 3.4 → 3.5) carried genuine downstream impact and were forwarded as guidance via the rolling-context scratchpad. Both forwards were consumed correctly by the receiving agent (3.4 implemented backpressure logging in the closure exactly as 3.2's note recommended; 3.5 used the same `tokio::sync::watch::channel` shape 3.4 chose). Promoting "soft flag forwarding via scratchpad § Per-task outcomes" from observed pattern to documented playbook contract is now overdue (step 1 + step 2 retros both surfaced this; step 3 confirms it as load-bearing on a higher-complexity, multi-handoff step). Recommend a playbook edit before step 4.

*Retries:*
- Tasks with retries: none.
- Per task: all 6 succeeded on first attempt.
- 2-retry ceiling hit without success: none.

*Time and overhead:*
- Total wall-clock: ~37m (todo 36 created 2026-04-26 00:05:46 UTC → todo 41 completed 00:42:48 UTC).
- Per-task wall-clock from todo `created_at` to `completed_at`: task 3.1 = 4m 41s, task 3.2 = 12m 31s (from 3.1's completion: 8m), task 3.3 = 18m 29s (from 3.2's completion: 6m 12s), task 3.4 = 26m 00s (from 3.3's completion: 7m 52s), task 3.5 = 33m 06s (from 3.4's completion: 7m 28s incl. 6 local repeat-runs for flake check), task 3.6 = 35m 29s (from 3.5's completion: 2m 38s).
- Coordinator wake-up count: 6 (one per task; all genuine completions; zero false-positive idle wake-ups). Two timers used the extended 25-min `max_wait_ms` (3.2 + 3.4, both medium-high risk); the rest used the default 15min. Task 3.6 used 10min (doc-only). All fired well inside their windows.
- Context drift symptoms: none observed across 6 wake-ups. Rolling-context scratchpad's § Per-task outcomes was actively used to forward soft-flag downstream impact between tasks (notably 3.2→3.4 and 3.4→3.5). Coordinator re-read scratchpad once per wake-up via `todo_get` rather than full reload.

**Notes**

- *Net wall-clock about 7m above step 2 despite the workplan flagging step 3 as "medium-high risk — the project's biggest landmines."* Step 1: 34m (low-risk skeleton). Step 2: 30m (medium scan + first spawn_blocking workload). Step 3: 37m (medium-high watcher with backpressure, async composition, and 9 integration tests). The marginal overhead is clearly absorbed by 3.4 (composing three modules under one async runtime, manual smoke verification) and 3.5 (running the test suite 6× to look for flakes) — both deliberate quality investments by the task agents. The roadmap's risk-grading remains a useful predictor of where wall-clock will go.
- *Soft-flag forwarding is now the project's load-bearing context-passing pattern.* Step 1 named it ("context-passing baton"); step 2 ran on it (4× soft flags, all forwarded cleanly); step 3 shipped on it through a chain (3.2 → 3.4 → 3.5) where each task agent's downstream-impact paragraph in the scratchpad shaped the next agent's implementation choices. Concretely: 3.2's recommendation that backpressure logging belongs in the closure (not the consumer) was implemented verbatim by 3.4's agent; 3.4's `Send`-friendly `changed()` shape was mirrored by 3.5's tests without prompting. Three steps of evidence is enough — promote "soft flag forwarding via scratchpad § Per-task outcomes" from anecdote to documented playbook contract before step 4. The COORDINATOR § Per-task execution loop step 6 is where it goes.
- *Anti-flake rule held under stress.* Task 3.5's `sustained_save_loop_*` test naturally needs `SETTLE * 4` because sustained writes keep extending the debouncer's quiet period — the agent caught the *mechanical* basis for the longer settle window and explained it inline rather than treating it as a CI-bump exception. The workplan's "do not introduce a polling-loop helper that hides timing" rule was honored without needing a coordinator intervention.
- *Workplan TBDs handled cleanly.* Both step-3 deferred decisions (debounce window tuning, rename-as-distinct vs delete+create) resolved at workplan time and held through the build with zero in-build revision. Neither warranted ADR promotion. The "fits in 1–3 paragraphs of workplan prose with a 'Why' → not an ADR" rubric (named in step 2's retro) held for the third consecutive step.
- *Idle-detection reliability remains 100%.* 6/6 fires were genuine completions in step 3 (and 14/14 across steps 1–3). Three clean steps; the playbook's open question on "Idle-detection false positives" is now answered — `timer_fire_when_idle_any` is reliable as a "task agent done" signal in this workflow shape. Recommend retiring the open question in the next playbook edit.
- *Coordinator process-context did not drift across 6 wake-ups* — the playbook open question on coordinator context drift remains unresolved at small scale, but step 3 added one more clean data point. Step 5 (HTTP shipping gate, more tasks, more forward notes) is where this will actually be stressed.
- *Task 3.6 surfaced a coordinator-level concern (workplan-untracked-at-task-time) without escalating* — agent flagged it in the results comment as out-of-scope-for-the-task and intentionally did not commit it. This is a clean instance of "soft flag for the coordinator, not for downstream agents." Worth a separate note in the playbook: soft flags can be addressed *to the coordinator* about coordinator-level concerns (e.g. "this looks like a boundary-ritual responsibility"), not just *to the next task agent* about implementation concerns.
- *Step-boundary follow-up (not-an-ADR, intentional)*: none. Step 3's resolutions held; no scaffolding artifact to clean up at the start of step 4. Step 4 (outbox) can begin its workplan on the human's signal.
- *Pilot risk assessment update.* The roadmap labels step 3 as "the biggest landmines in this entire project." The build did expose those landmines — debouncer event coalescing, sync-conflict patterns broader than the globset, rename decomposition, backpressure, async composition with `Send` constraints — and every one of them resolved inside the task-agent / coordinator loop without escalation. The pattern that made this work was load-bearing skill cross-references (`.claude/skills/filesystem-watching` cited at every notify site, `.claude/skills/rusqlite-in-async` at every SQL site) plus the soft-flag forwarding chain. Steps 4 (outbox; low risk) and 5 (HTTP shipping gate; medium risk, first external surface) will not stress the same axes; step 5 will instead test workplan task density (likely 8+ tasks) and forward-note volume.

#### Step 4 (shipped 2026-04-26)

**Structured Eval**

*Batching outcomes:*
- No batches. All 6 workplan tasks ran as solo task agents (default-not-batch carried forward from steps 1–3).
- Solo task 4.1 (ChangeEvent type + serde envelope): scope = 3 files (`src/lib.rs`, `src/outbox/mod.rs` new, `src/outbox/event.rs` new), result-comment ~6 paragraphs (no soft flag). Adjacent (4.2): both touched `src/outbox/`. Assessment: appropriate solo — workplan justified the split ("isolated test surface; serializer's tests are kept apart from the writer's"); 4.2's soft-flag-style cosmetic notes (private test helpers) would have muddied 4.1's tighter test boundary.
- Solo task 4.2 (Outbox writer with per-event `sync_data`): scope = 2 files (`src/outbox/writer.rs` new, `src/outbox/mod.rs` replace stub), comment ~7 paragraphs (no soft flag; cosmetic test-internal helpers noted but not flagged). Adjacent (4.3): different module (indexer vs outbox). Assessment: appropriate solo.
- Solo task 4.3 (Indexer outcomes carry content_hash): scope = 1 file (`src/indexer/mod.rs`), 8 single-file outcome tests touched (5 updated + 3 new), comment ~6 paragraphs (no soft flag). Adjacent (4.4): different module. Assessment: appropriate solo — outcome enums lost `Copy` (now `Clone + PartialEq + Eq` since they own a `String`); workplan-flagged contract change to public surface deserved its own commit and test cycle.
- Solo task 4.4 (Wire Outbox into run_consumer): scope = 3 files (`src/watcher/mod.rs`, `src/bin/hmnd.rs`, plus 3-line compile-fix to `tests/watch.rs` per soft flag below), comment ~10 paragraphs incl. soft flag + smoke test detail. Workplan-flagged medium risk (composition under one async runtime). Assessment: appropriate solo — task-flagged-risky rule held; soft flag (absorbing 3 lines of 4.5's nominal scope to keep the tree compiling between commits) was load-bearing for 4.5 and was honored cleanly.
- Solo task 4.5 (Integration tests against tempdir vault + outbox): scope = 1 new file (`tests/outbox.rs`, 379 lines, 9 cases). 3× consecutive flake-check clean per workplan (matching step 3 task 3.5 precedent). Comment ~6 paragraphs (no soft flag). Adjacent (4.6): no overlap. Assessment: appropriate solo; agent honored 4.4's forward note correctly (did NOT re-add `tests/watch.rs` constructor that 4.4 already absorbed).
- Solo task 4.6 (Reference docs reflect step-4 resolutions): scope = 5 doc files. Comment ~8 paragraphs incl. 2 soft flags (both coordinator-level). Assessment: appropriate solo — doc-only by design; both soft flags landed at the boundary, neither was a downstream-agent concern.

*Escalations:*
- Count: 0.
- By type: ambiguity=0, test-failure=0, scope-question=0, surprise-decision=0, other=0.
- *Soft flag count: 3* (tasks 4.4 / 4.6×2), vs. step 3's 3, step 2's 4, step 1's 2. The pattern's rate is stable across four shipped steps. Of the 3 step-4 soft flags, one (4.4's `tests/watch.rs` absorption) was forwarded as guidance via the rolling-context scratchpad and consumed correctly by 4.5; the other two (4.6's stale parenthetical + workplan-untracked) were addressed *to the coordinator* about boundary-ritual responsibilities — the same coordinator-targeted soft-flag pattern step 3 first surfaced (task 3.6's workplan-untracked flag). Step 3's retro recommended distinguishing coordinator-targeted vs. agent-targeted soft flags in the playbook; the playbook edit (commit 7813dbe between tasks 4.1 and 4.2) made that distinction explicit, and 4.6 used it as designed.

*Retries:*
- Tasks with retries: none.
- Per task: all 6 succeeded on first attempt.
- 2-retry ceiling hit without success: none.

*Time and overhead:*
- Total wall-clock: ~30m 25s (todo 42 created 2026-04-26 01:11:40 UTC → todo 47 completed 01:42:05 UTC).
- Per-task wall-clock from previous task's completion to this task's completion: 4.1 = 3m 23s (no predecessor; `step-04-coordinator` startup + first dispatch = the implicit pre-roll), 4.2 = 2m 35s, 4.3 = 4m 03s, 4.4 = 8m 10s, 4.5 = 7m 51s, 4.6 = 4m 23s. Net agent time totals 30m 25s — comparable to step 2's 30m, faster than step 3's 37m. The roadmap's "low risk — thin layer on top of step 3" risk-grade held.
- Coordinator wake-up count: 6 (one per task; all genuine completions; zero false-positive idle wake-ups). Two timers used the extended 25-min `max_wait_ms` (4.4 + 4.5, both medium-complexity in different ways — 4.4 for composition, 4.5 for the 3× flake check); 4.6 used 10min (doc-only). All fired well inside their windows.
- Context drift symptoms: none observed across 6 wake-ups. Rolling-context scratchpad's § Per-task outcomes was actively used to forward soft-flag downstream impact between tasks (notably 4.2→4.4 about `&Outbox` shape, 4.3→4.4 about non-`Copy` outcomes, and 4.4→4.5 about the absorbed compile-fix). Coordinator re-read scratchpad once per wake-up via `todo_get` rather than full reload.

**Notes**

- *Step 4 was the cleanest build to date.* Zero soft flags forwarded as agent-targeted guidance for 4.5 or 4.6 to actively act on (4.4's flag was load-bearing for 4.5, but the action was "don't do this thing 4.4 already did" — the simplest possible forward shape). Zero retries. Zero escalations. The roadmap's "low risk — thin layer on top of step 3" call was accurate; the workplan's mechanical clarity on the emit table (the load-bearing invariant) and the deferred-decision resolutions (per-event `sync_data`, content_hash on deletes) gave each task agent enough to ship without revisiting the architecture.
- *Soft-flag-to-coordinator pattern matured.* Task 4.6 surfaced two soft flags both addressed to the coordinator (stale parenthetical + workplan-untracked-at-task-time). The playbook edit between 4.1 and 4.2 (`7813dbe`) had just promoted this distinction from anecdote to documented contract; 4.6 used it as designed. Both flags landed cleanly at boundary — the coordinator fixed the parenthetical in the boundary commit and bundled the workplan per steps 1–3 precedent. This is the third step where coordinator-targeted soft flags caught boundary-level cleanup the task agent correctly identified as out-of-scope-for-the-task; the pattern is now load-bearing for boundary hygiene.
- *Forward-note pattern continues to scale.* Step 3 escalated the forward-note channel to a documented playbook contract (commit 7813dbe). Step 4 ran on it across three handoffs (4.2→4.4, 4.3→4.4, 4.4→4.5). Each note was actionable and consumed correctly. The compounding effect: 4.5's agent honored 4.4's "don't re-add the watch.rs constructor" forward note **without prompting**, which would have produced a redundant-edit failure mode in earlier steps.
- *Workplan TBDs handled cleanly.* All three step-4 deferred decisions resolved at workplan time and held through the build with zero in-build revision: per-event `sync_data` for fsync; two-line decomposition for renames (already resolved at watcher boundary in step 3, confirmed for outbox); prior-hash on deletes (a third resolution, surfaced during workplan-writing as a roadmap-vs-spec conflict). None warranted ADR promotion; the rubric "if the resolution fits in 1–3 paragraphs of workplan prose with a 'Why', it does not need to be an ADR" held for the fourth consecutive step.
- *Idle-detection retirement validated post-edit.* The playbook edit before 4.2 retired the open question on `timer_fire_when_idle_any` reliability. Step 4 added 6/6 clean fires, bringing the cumulative tally to 20/20 across steps 1–4. The retirement decision is correct; the open question stays closed.
- *Coordinator process-context did not drift across 6 wake-ups* — the playbook open question on coordinator context drift remains unresolved at small scale. Step 4's modest task density (6 tasks, comparable to steps 2 and 3) does not stress this; step 5 (HTTP shipping gate, expected 8+ tasks per step 3 retro) is where it'll actually be tested.
- *Net wall-clock was the fastest of the four shipped steps* despite step 4 introducing a third deferred decision (prior-hash on deletes) at workplan time. Two visible contributors from the eval data: (a) every task agent went idle exactly once (no re-prompts; no status checks needed); (b) the medium-risk task (4.4) absorbed a compile-fix that protected the next task agent from a non-compiling-tree start — a soft-flag pattern that strictly reduced 4.5's debug surface.
- *Step-boundary follow-up (not-an-ADR, intentional)*: none. Step 4's resolutions held; no scaffolding artifact to clean up at the start of step 5. Step 5 (HTTP shipping gate; medium risk; first external surface) can begin its workplan on the human's signal — and will be the first build to genuinely stress workplan task density and forward-note volume per step 3's retro prediction.

### Step 5

_Not yet started._

### Step 5

_Not yet started._

### End-of-round retrospective (after step 5 ships)

_Not yet started. Should answer: did the roadmap→workplan→build cadence work? What would we change for the next round (steps 6–8)?_
