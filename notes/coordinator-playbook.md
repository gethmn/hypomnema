# Coordinator Playbook

Operational playbook for the orchestrator, step coordinator, and task agent roles in Hypomnema's build workflow. Three roles:

- **Orchestrator** — top-level agent (Solo agent or Claude Code terminal session) that talks directly to the human, spawns the per-step coordinator, and surfaces escalations. Never writes code; never becomes the coordinator.
- **Coordinator** — drives a single roadmap step end-to-end. Freshly spawned by the orchestrator at "start step N" as the workplan-writer; promoted in place to coordinator on "build."
- **Task agent** — ephemeral worker spawned by the coordinator to execute one (or a small batch) of workplan tasks.

This playbook is read by all three roles. It is *not* canonical project documentation — it is a working description of the orchestration pattern, expected to evolve after each step.

---

## Overall shape

```
Human  ⇅  Orchestrator     (Solo agent OR Claude Code terminal session — talks
                            directly to the human; never writes code itself)
              ⇅
         Step coordinator  (Solo agent: step-NN-coordinator, freshly spawned
                            per step; promoted from workplan-writer on "build")
              ↓ spawns
         Task agents       (Solo agents: step-NN-task-MM, ephemeral)
```

> **Invariant**: the orchestrator and the coordinator are never the same Solo process. The coordinator is always a fresh spawn at "start step N", regardless of whether the orchestrator is itself a Solo agent or a terminal session.

- The orchestrator and the coordinator communicate through Solo todos, scratchpads, and (for "wake the coordinator after escalation") timer-driven PTY injections.
- The coordinator and task agents communicate through Solo todos (per task), todo comments (per task report), and the step's rolling-context scratchpad (shared mutable state).
- Bubble-up to the human is **pull-based**: the coordinator creates `needs-human`-tagged todos; the orchestrator surfaces them when the user asks for status.

---

## ORCHESTRATOR section

> **Audience**: the top-level agent that talks directly to the human. Could be a Solo agent or a Claude Code terminal session. If you are a coordinator or task agent, skip this section.

### Identity & setup (run once, on becoming the orchestrator)

A fresh agent that finds itself at the top of the hierarchy — talking directly to the human, no parent agent — is the orchestrator. Recognition triggers: human says "start step N", "build", "status"; or human references roadmap / workplan / coordinator concepts. On recognition:

1. `whoami()` to confirm process identity.
2. Read this ORCHESTRATOR section in full. Do **not** read the COORDINATOR or TASK AGENT sections — those are for agents you'll spawn, not for you.
3. Optionally rename your process to `orchestrator` (or `<project>-orchestrator`) for clarity in `list_processes` output. Not required.
4. Confirm to the human you're set up and (if you don't already know) ask which step they want to start.

### Per-step kickoff (on "start step N")

1. Read the step N todo (from the original 17–21 set) and the relevant section of `docs/roadmap/roadmap.md`.
2. **Spawn a fresh coordinator-to-be**: `spawn_process(kind="agent", agent_tool_id=<id>, name="step-NN-coordinator")`. Capture the returned `process_id`. **Never reuse an existing process for this role** — the orchestrator never becomes the coordinator. Even if you yourself are a Solo agent, you spawn a *new* Solo agent for this.
3. Send the workplan-writing prompt to the new coordinator-to-be (template in [§ Workplan-writing prompt](#workplan-writing-prompt)).
4. Set an idle timer to wake when the coordinator-to-be finishes the workplan: `timer_fire_when_idle_any(processes=[<coordinator-pid>], max_wait_ms=1800000, body="<wake-up: step-NN coordinator-to-be went idle, check workplan>")`.
5. On wake-up: read the workplan output, surface its path to the human for review. Do not modify it.
6. On the human's "build" / "go" / "approved": forward "build" to the same coordinator-to-be process (don't spawn a new one — this is the moment of the in-place coordinator promotion). The coordinator takes it from there per the COORDINATOR section. Then **arm the build-phase poll** per [§ Polling the coordinator](#polling-the-coordinator-cheap-check-and-re-arm) — the workplan-completion timer set in step 4 only covered the writing phase.

### Per-step ongoing

While the coordinator runs:

- The orchestrator handles human-facing surfacing only (status checks, escalation routing per [§ Orchestrator surfacing rules](#orchestrator-surfacing-rules)).
- The orchestrator **never** spawns task agents directly. If you find yourself wanting to spawn a `step-NN-task-MM`, stop — you are not the coordinator. Re-read this section.

#### Polling the coordinator (cheap-check-and-re-arm)

The coordinator goes idle many times during a build — once between every per-task wake-up. The orchestrator's idle-watch on the coordinator therefore fires more often than the orchestrator has work to do. Keep the noise cheap rather than over-thinking each fire:

1. Arm an idle-watch on the coordinator: `timer_fire_when_idle_any(processes=[<coordinator-pid>], max_wait_ms=600000, body="<wake-up: coordinator idle, run bounded check per playbook>")`. 10-min cap is the default — short enough that a real `needs-human` event doesn't sit too long, long enough to absorb routine coordinator-idle gaps. Tighten for higher-vigilance phases (e.g. 5 min if you've just answered an escalation and expect rapid follow-through).
2. **On every wake-up, run exactly this bounded checklist:**
   1. `todo_list(tags=["needs-human"], completed=false)` — if non-empty, surface to the human per [§ Orchestrator surfacing rules](#orchestrator-surfacing-rules) and stop (do not re-arm; the human's answer is the next event).
   2. `get_process_status(<coordinator-pid>)` — if non-Running, check the latest comment on the step's outer todo (17–21 set); if it confirms completion, run § Step boundary; otherwise treat as a crash.
   3. Otherwise re-arm the timer with the same parameters and stop.
3. **Do not reason further per fire.** If steps 1–2 show nothing for the orchestrator to act on, the answer is "still waiting on the subagents" and the next step is re-arm. Future-orchestrator-self may feel an urge to glance at the scratchpad or count tasks — resist it. The coordinator surfaces what the orchestrator needs via `needs-human` todos; everything else is the coordinator's business.

Why this shape: the coordinator's own `timer_fire_when_idle_any` on its task agents is reliable as a "task agent done" signal (14/14 in pilot), but the orchestrator's same signal on the coordinator picks up every gap between the coordinator's per-task timers, not just step boundaries. Bounding per-fire work keeps that noise cheap; tightening `max_wait_ms` keeps real escalations fresh. Pilot lesson: a 30-min cap let two task completions go unnoticed before the orchestrator next checked — too long. 10 min is the calibrated default.

### Step boundary

When the coordinator reports the step shipped:

- Acknowledge to the human.
- Close the coordinator process (`close_process(process_id=<coordinator-pid>)`).
- Wait for the next "start step N+1".

### Response style for ambiguous start questions

When the human asks "what do I do to start step N?" / "how do I kick off step N?" / similar (a *meta* question, not a command):

- Do **not** answer with "just type 'start step N'." That answer hides the spawn the orchestrator is about to perform and invites the orchestrator to silently skip it.
- Instead, describe the spawn you would perform: "I'll spawn `step-NN-coordinator` as a fresh Solo agent and send it the workplan-writing prompt for step N. The coordinator-to-be writes the workplan, you review, then on 'build' it gets promoted in place. Want me to go ahead now?"
- This both confirms the architecture is intact and gives the human a chance to redirect before the spawn happens.

---

## COORDINATOR section

### Identity & promotion

> **Audience**: the Solo agent that wrote the workplan and is being promoted to coordinator. The orchestrator should not act on this section — it spawned you, but it is not you.

You become the step N coordinator the moment the human says "build" on the workplan you wrote. From that point on:

1. Rename your Solo process if you can, conceptually if not — you are now `step-NN-coordinator`. You no longer write code yourself; you orchestrate task agents.
2. Confirm your identity with `whoami()`.

### Setup (run once, at build start)

1. **Create the step's rolling-context scratchpad.** Name: `step-NN-context`. Initial content uses the template in [§ Scratchpad templates](#scratchpad-templates) below. Tag with `step-NN`, `coordinator-context`.
2. **Decide task batching.** Read your own workplan. For each adjacent pair of tasks, decide whether to batch them. Apply the rules in [§ Batching rules](#batching-rules). Record your batching plan as a section in the step-context scratchpad (so it's visible to the human and to future task agents).
3. **Create one todo per task** (not per batch). Title: `Step N · Task M — <one-line>`. Tag with `step-NN`, `task`. Body: copied/condensed from the workplan task description, plus the batch ID if batched. If batched together, set blockers so they show their grouping (`todo_set_blockers` — task M+1 blocked-by task M for ordering, even when batched, so the visible state matches the build order).
4. **Note the build start** in the scratchpad with a timestamp.

### Per-task (or per-batch) execution loop

For each task (or batch) in workplan order:

1. **Pre-flight checks.** Re-read the step-context scratchpad (your own context may have drifted; the scratchpad is the source of truth). Verify the previous task's outcome was recorded. If a previous task left an unresolved decision, do not advance — escalate.
2. **Spawn task agent.** `spawn_process(kind="agent", agent_tool_id=3, name="step-NN-task-MM")`. For a batch, use the lowest task number in the batch (e.g., `step-01-task-03` for batch `[task-03, task-04]`). Capture the returned `process_id` and `agent_instructions`.
3. **Send the task prompt.** Use the template in [§ Task agent bootstrap prompt](#task-agent-bootstrap-prompt) below, filled in with: the task agent's process_id (from the spawn), the todo IDs being executed, the step-context scratchpad ID, the workplan task numbers. Send via `send_input(process_id=<task agent>, input=<filled template>, wait_ms=2000)`.
4. **Schedule wake-up on idle.** `timer_fire_when_idle_any(processes=[<task agent process_id>], max_wait_ms=900000, body="<wake-up message>")`. The wake-up message must be self-contained and instruct future-you to: check the task agent's todo(s) status, read any new todo comments, decide outcome → advance / retry / escalate. Use the template in [§ Wake-up message](#wake-up-message). 15min max-wait is the default; raise for tasks the workplan flagged as long.
5. **When the timer fires** (your PTY receives the wake-up message as a fresh user turn): execute the routing logic in [§ Wake-up routing](#wake-up-routing).
6. **After a task completes successfully**, scan the results comment for a `**Soft flag:**` line. If present, record it in the step-context scratchpad's § Decisions made during build, and forward the substance to subsequent task agents (via the rolling context) if its downstream impact applies. Then append a one-paragraph summary to the step-context scratchpad (use `scratchpad_append`) noting: which task(s) finished, what files changed, anything downstream tasks need to know.
7. **Close the task agent.** `close_process(process_id=<task agent>)`. Task agents are ephemeral.
8. Move to the next task.

### Wake-up routing

When you wake up because the task agent went idle, do *not* assume "idle = done." Check in this order:

1. `todo_get(<each task todo>, include_comments=true)`.
2. **If the todo is `completed`** AND has a results comment → record outcome in scratchpad, close the task agent, advance.
3. **If the todo has a `needs-human` tag** → the task agent escalated. Read the comment for context. Mirror the escalation up: create a coordinator-level escalation todo (see [§ Escalation](#escalation)) referencing the task's escalation todo. Stop the loop and wait for resolution (timer in [§ Escalation polling](#escalation-polling)).
4. **If the todo is open with no recent comment AND task agent is idle** → status check. Send to the task agent: `"Status check: are you done with todo <id>? If yes, mark it complete and post a results comment. If no, what's the blocker?"` Re-arm a 5-min idle timer.
5. **If the task agent is dead/unresponsive** (`get_process_status` shows non-Running, or hangs) → see [§ Failure handling](#failure-handling).

### Batching rules

Batch two (or more) adjacent workplan tasks into one task agent invocation when **all** of these hold:

- They are adjacent in the workplan (no skipped task).
- Each is small — workplan task description fits in 1–2 paragraphs and would plausibly take a fresh agent <30 min.
- They touch the same files or directly related concerns (e.g., add a struct + a test for that struct).
- Neither has an unresolved deferred decision.
- The combined work fits in one agent's reasonable context budget.
- Neither task is flagged in the workplan as risky or sensitive.

If you batch, the *task agent* must report sub-task outcomes individually (see [§ TASK AGENT section](#task-agent-section)). When a batched task agent escalates, scope the escalation to the specific sub-task that blocked.

Default to **not batching** if you're unsure. Single-task agents are simpler to reason about.

### Failure handling

When the task agent reports a failure (test failure, compile error, lint failure):

- **Fixable, has clear error context** → re-prompt the same task agent: `"The previous attempt failed: <paste failure>. Try again — fix the cause, re-run, report outcome."` Allow up to **2 retries** (3 attempts total). Set timer for the next idle.
- **Same failure twice in a row** → stop. Escalate. Don't retry a third time on the same failure mode.
- **Different failure each retry** → that's progress; allow the full 2 retries.
- **Task agent process died** → spawn a fresh task agent (same name + `-r1` suffix), re-prompt with full context including what was already attempted. Allow **1** such respawn before escalating.
- **Ambiguity, missing requirement, scope question, surprise architectural decision** → no retries. Escalate immediately.

### Escalation

When you must escalate to the human:

1. Create a coordinator-level escalation todo:
   - Title: `[ESCALATION step-NN/task-MM] <one-line summary>`
   - Tags: `step-NN`, `needs-human`, `escalation`
   - Body: what's blocked, what context the human needs, what options you've considered (1–3), your recommendation if you have one, and the task todo ID(s) the resolution will unblock.
2. Append a `## Escalation` line to the step-context scratchpad with the escalation todo ID and a one-line summary.
3. **Stop spawning new task agents.** Wait for resolution.
4. Schedule an escalation poll: see [§ Escalation polling](#escalation-polling).

### Escalation polling

The human's answer comes back as a comment on your escalation todo. Poll for it:

- `timer_set(delay_ms=300000, body="<wake-up: check escalation todo X for human resolution>")` — 5 min cadence.
- When the timer fires: `todo_get(<escalation todo>, include_comments=true)`. If the most recent comment is from the human (or if the todo's `needs-human` tag has been removed) → resolved. Otherwise re-arm.
- On resolution: record the answer in the step-context scratchpad, propagate the answer to the blocked task (post as a comment on the task's todo), then re-spawn a task agent for that task with the resolution included in its prompt.

### Post-build evaluation

Before filing the per-step retrospective, run a structured evaluation of how the build went. The output is **data, not opinion** — opinion belongs in the retro prose that follows. **Capture-only**: this data is for the human to review later and (eventually, across many steps) refine the playbook. Do NOT auto-feed it into future coordinator decisions.

Compute and capture the following from the step-context scratchpad, the task todos, and the escalation todos:

**Batching outcomes**
- For each batched group: composition (which task numbers), final outcome (clean / split mid-batch / escalated), retry count for the batch, brief assessment ("good batch" / "should have split — sub-task X blocked while Y completed cleanly").
- For each solo task: actual scope signal (file count touched, lines of comment in the task agent's results report). If the scope looks tiny (≤2 files touched AND result comment is 1–2 sentences) AND the adjacent task touched related files, flag as "could have been batched with task M".

**Escalation patterns**
- Total escalation count.
- Classification of each escalation: `ambiguity` / `test-failure` / `scope-question` / `surprise-decision` / `other`.
- Resolution reference (the comment that resolved the needs-human todo).
- For each: was it preventable with a better workplan upfront? Brief note.

**Retry counts and failure modes**
- Per task: number of retries used (0, 1, or 2).
- For tasks that hit retries: failure type (`test` / `compile` / `lint` / `other`).
- Did any task hit the 2-retry ceiling and still fail? If yes, that's a signal the default may be wrong for that failure mode.

**Time and overhead signals**
- Per task: wall-clock from todo created_at to completed_at.
- Total coordinator wake-up count (count of timer fires that landed in the coordinator's PTY).
- Any context-drift symptom you noticed (you forgot something the scratchpad recorded, you re-read the scratchpad more than 3 times for the same task, etc.).

The eval output goes directly into the per-step retro entry in `notes/project-planning-workflow-notes.md` (see § Step boundary ritual step 4 below). Use the retro template at the top of that file's Retrospectives section. Also post the eval as a comment on the step's outer todo (id 17–21) so the human can review without opening the workflow notes file.

### Step boundary ritual

After the last task completes (and shipping criteria pass — verify against the roadmap):

1. Mark the step done in `docs/roadmap/roadmap.md` (add `**Status**: shipped <date>` line at the top of the step's section).
2. Capture any ADRs that hardened during the build into `docs/decisions/`.
3. Run § Post-build evaluation. The eval populates the structured-data portion of the retro entry.
4. Append the per-step retrospective to `notes/project-planning-workflow-notes.md` using the template at the top of that file's Retrospectives section. The eval data goes in the Structured Eval subsection; subjective notes go in the Notes subsection.
5. Mark all step-NN todos completed (`todo_complete(id, true)` for each).
6. Archive the step-context scratchpad: `scratchpad_archive(scratchpad_id=<X>)`.
7. Post a final comment on the step's "outer" todo (the one in the original 17–21 set): `"Step shipped <date>. Workplan, retro, and decisions in repo. Coordinator standing down."`
8. Stop. The next step's coordinator is a fresh agent for a fresh workplan.

---

## TASK AGENT section

You are an ephemeral worker. You exist to execute one task (or a small batch) and report back. You do not advance the build; the coordinator does that.

### On wake-up (your first turn)

1. **Read the playbook's TASK AGENT section** (`notes/coordinator-playbook.md` — this section). Required.
2. **Read your todo(s)** with `todo_get(id, include_comments=true)`. The todo body has the task description; comments may have prior attempts or coordinator notes.
3. **Read the step-context scratchpad** (id given in your bootstrap prompt). Read in `mode=full` if it's small, `mode=headings` first if it's large then `mode=section` for the parts you need. This is your authoritative context for what's been done so far.
4. **Read the workplan task** in `docs/roadmap/step-NN-workplan.md` for the full task description.
5. Then execute.

### Reporting (mandatory)

When you complete a task (success or unsuccessful but bounded):

1. **Verify quality gates pass.** `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test` (or whatever the workplan task specifies) all green.
2. **Commit the task's changes.** Stage the specific files you changed (not `git add -A` or `git add .` — be explicit). Commit message format:

   ```
   git commit -m "Step N · Task M: <one-line summary, ~50 chars>"
   ```

   Use the task's todo title as the basis for the one-line summary. If a pre-commit hook fails the commit did **not** happen — fix the underlying issue, re-stage, and create a **new** commit. Do NOT `--amend` (you'd modify the previous task's commit). If you can't get the commit through after one retry, treat it as a `test-failure`-class outcome per coordinator § Failure handling.
3. `todo_comment_create(todo_id, body=<results comment>)` — body should include: what you did (1–2 sentences), files touched (paths), test results summary, **the commit SHA from step 2**, any decisions you made that weren't already specified, anything downstream tasks should know.
4. `todo_complete(todo_id, completed=true)`.
5. If batched (multiple todos) → **one commit per logical sub-task**, not one commit per batch. Batching is a coordinator scoping decision; commits stay per-task for clean blame/revert/bisect. Repeat steps 2–4 per todo so each sub-task ships its own commit and its own results comment.
6. **Stop.** Do not pick up the next task. Do not spawn anything. Wait — the coordinator will close you.

### Soft flag (optional — judgment call surfaced without escalating)

A third state between full success and escalation: you made a non-trivial judgment call **within the bounds of the task** — choosing between two reasonable implementations, settling on a small naming convention, working around an unexpected technical constraint — and you shipped the task successfully. Surface the call to the coordinator without blocking the build.

How to flag:

1. Complete the task as normal (results comment + `todo_complete`).
2. In the results comment, include a line of the form `**Soft flag:** <one-line summary>` followed by:
   - **What you decided** (the choice made)
   - **The trade-off** (briefly — why this over the alternative)
   - **Downstream impact** (whether subsequent task agents need to know)

The coordinator reads soft flags during wake-up routing. They may be forwarded as guidance to subsequent task agents via the rolling-context scratchpad, surfaced in the post-build evaluation, or quietly accepted.

When **NOT** to use a soft flag:
- The decision would warrant an ADR → escalate.
- The decision contradicts the workplan, an ADR, or a spec → escalate.
- You're uncertain whether the choice was right → escalate.

Soft flags are for *"I made a defensible choice in the task's natural latitude — here's what and why."* Not *"I might have made the wrong choice, please confirm."*

### Escalation (when to stop early)

Escalate immediately, without retrying, if:

- The task description is ambiguous or contradicts something you read.
- A required file/symbol/decision is missing from the workplan and the scratchpad.
- The task implies a decision that should be a deferred-decision-resolution (an ADR) and isn't.
- You discover the task as written would break a constraint from an ADR or spec.
- You discover the task is significantly larger than the workplan suggested.

To escalate:

1. `todo_add_tag(todo_id, "needs-human")`.
2. `todo_comment_create(todo_id, body=<escalation comment>)` — body should include: what blocked you, what you tried (if anything), what you need to know to proceed, options you considered (if any).
3. **Do not** mark the todo completed.
4. **Stop.**

For batched todos: tag and comment only the specific sub-task's todo that blocked. Mark the others completed if they actually completed.

### Retry input from the coordinator

If the coordinator re-prompts you with failure context, treat it as the next iteration of the same task. Read the prior comment(s) on your todo for what was attempted. Do not retry the same approach blindly — change something.

### Code quality

Apply the project's standing rules from `AGENTS.md` and `CLAUDE.md` (if present). Default to "no comments unless WHY is non-obvious" per the project's standing convention. Run `cargo fmt` and `cargo clippy` (or whatever the workplan task specifies) before reporting success.

---

## Conventions

### Naming

| Thing | Pattern | Example |
|---|---|---|
| Coordinator process | `step-NN-coordinator` | `step-01-coordinator` |
| Task agent process | `step-NN-task-MM` (or `-MM-r1` for respawn) | `step-01-task-03` |
| Step rolling-context scratchpad | `step-NN-context` | `step-01-context` |
| Per-task todo | `Step N · Task M — <one-line>` | `Step 1 · Task 3 — Logging init` |
| Escalation todo | `[ESCALATION step-NN/task-MM] <summary>` | `[ESCALATION step-01/task-03] EnvFilter strategy unclear` |

### Tags

- `roadmap` — the original 17–21 step todos
- `step-NN` — anything related to step N
- `task` — per-task todos
- `escalation` — escalation todos
- `needs-human` — currently waiting on human input
- `coordinator-context` — the rolling-context scratchpad

### Orchestrator surfacing rules

When the human asks for status, anything related, or just a vague "what's going on":

1. `todo_list(tags=["needs-human"], completed=false)` — first thing checked.
2. Surface those plus a short "step N coordinator is on task M" summary.
3. The human's answer to an escalation goes back as `todo_comment_create` on the escalation todo, and the `needs-human` tag is removed from the task todo (and from the escalation todo). The coordinator's poll picks this up.

---

## Scratchpad templates

### `step-NN-context` initial content

```markdown
# Step N — Rolling Context

**Coordinator**: <process name and id>
**Workplan**: docs/roadmap/step-NN-workplan.md
**Build started**: <ISO timestamp>

## Batching plan

| Batch | Tasks | Rationale |
|---|---|---|
| (filled by coordinator at setup) | | |

> **Live task status**: query `todo_list(tags=["step-NN"])` rather than maintaining a table here. Per-task results land as comments on each task's todo and as paragraphs in § Per-task outcomes below. This scratchpad is **append-only** during the build — don't put a status table here that goes stale on the first task completion.

## Decisions made during build

(append as the build runs)

## Escalations

(append as escalations occur, with resolution when done)

## Per-task outcomes

(append a short paragraph per task as it completes)
```

---

## Workplan-writing prompt

Used by the orchestrator at "start step N" when spawning the coordinator-to-be. Fill in the angle-bracket slots. Send as a single line via `send_input(submit=true)`.

> [SOLO ORCHESTRATION CONTEXT] You are running inside Solo as the STEP \<NN\> COORDINATOR-TO-BE. Solo process ID: \<coordinator-pid\>, name: step-\<NN\>-coordinator, project: Hypomnema, project ID: 4. Your orchestrator is Solo process \<orchestrator-pid\>. [END SOLO ORCHESTRATION CONTEXT] Your job right now is to write the workplan for step \<NN\> at docs/roadmap/step-\<NN\>-workplan.md. Read in this order: (1) docs/roadmap/roadmap.md § Step \<NN\>; (2) the relevant ADRs in docs/decisions/ for this step's deferred decisions; (3) the relevant specs in docs/specs/; (4) any skills surfaced by the step's scope; (5) notes/project-planning-workflow-notes.md for the workplan format expectations. Then write the workplan and post a short summary back to the human; stop and wait for review. When the human says "build" / "go" / "approved", you will be promoted in place to the step \<NN\> coordinator — at that point read notes/coordinator-playbook.md § COORDINATOR section in full (the TASK AGENT section can wait until you're spawning task agents). Do NOT read the ORCHESTRATOR section — that's the role of the agent that spawned you, not yours.

---

## Task agent bootstrap prompt

Use this template when sending the first prompt to a freshly-spawned task agent. Fill in the angle-bracket slots. Send as a single line via `send_input(submit=true)`.

> [SOLO ORCHESTRATION CONTEXT] You are running inside Solo as a TASK AGENT. Solo process ID: \<task-agent-pid\>, name: \<task-agent-name\>, project: Hypomnema, project ID: 4. Your coordinator is Solo process step-\<NN\>-coordinator. [END SOLO ORCHESTRATION CONTEXT]  You are executing Solo todo(s) \<comma-separated todo IDs\> (workplan task(s) \<comma-separated task numbers\> from docs/roadmap/step-\<NN\>-workplan.md). Before you start, read in this order: (1) notes/coordinator-playbook.md — TASK AGENT section is your reporting and escalation contract; (2) Solo todo(s) \<ids\> with todo_get(include_comments=true); (3) the step's rolling-context scratchpad id \<context-scratchpad-id\>; (4) your workplan task section. Then execute. Follow the playbook for reporting and escalation. When done (success or escalation), stop — the coordinator will close you. Do not advance to the next task.

---

## Wake-up message

Use this template for `timer_fire_when_idle_any(body=...)` so future-you knows what to do when woken. Fill in the angle-bracket slots.

> Wake-up: task agent \<name\> (process \<pid\>) for todo(s) \<ids\> went idle. Route per playbook § Wake-up routing. Current task: \<M\>. Next: \<M+1, or "boundary ritual" if last\>.

The full routing logic lives in COORDINATOR § Wake-up routing — don't restate it in every wake-up body. Future-coordinator-self reads the playbook on wake.

---

## Orchestrator (human-facing) cheat sheet

When the human says…

| User says… | Orchestrator does |
|---|---|
| "What do I do to start step N?" / "How do I kick off step N?" (a *meta* question) | Per ORCHESTRATOR § Response style for ambiguous start questions: describe the spawn you'd perform ("I'll spawn `step-NN-coordinator` and send it the workplan-writing prompt — want me to go ahead?"). Do **not** answer "just type 'start step N'" — that hides the spawn and invites silently skipping it. |
| "Start step N" | Per ORCHESTRATOR § Per-step kickoff: read the step N todo and `docs/roadmap/roadmap.md` § Step N; **spawn a fresh** `step-NN-coordinator` Solo agent (never reuse an existing process — the orchestrator never becomes the coordinator); send the workplan-writing prompt; set an idle timer to wake on workplan completion. |
| "Build" / "Approved, build it" / "Go" (after workplan review) | Forward "build" to the same coordinator-to-be process you spawned at "start step N" (don't spawn a new one — this is the in-place promotion). The coordinator takes it from there. |
| "Status" / "Any updates?" / "What's going on?" | `todo_list(tags=["needs-human"], completed=false)` first. Also `get_process_status` on the active coordinator. Surface both. |
| "Approve option A" (in response to an escalation) | `todo_comment_create` on the escalation todo with the resolution. `todo_remove_tag(needs-human)` on both the escalation todo and the task todo it references. The coordinator's escalation poll picks it up. |
| "Pause step N" | Send to the coordinator: `"Pause: do not start a new task after the current one finishes. Set a timer to wake on resume."` |
| "Resume step N" | Send to the coordinator: `"Resume: continue with the next task per workplan."` |
| "Cancel step N" | Confirm with human first. Then close the coordinator and any active task agents, archive the scratchpad with a `cancelled` tag, and update the step todo with the cancellation reason. |

---

## Open process questions (revisit after the pilot)

These are unresolved and worth noting during the pilot run:

- **Coordinator context drift.** The coordinator's session context grows across many task wake-ups. After ~10 wake-ups, does it still behave correctly, or does it need to compact / re-read scratchpad more aggressively?
- **Status-check interruption.** When the coordinator interrupts an agent via `send_input` for a status check, does that derail an in-flight task? May need a less intrusive signal.
- **Batching pattern emergence.** Across multiple steps, do the structured batching evals (per § Post-build evaluation) reveal a stable pattern about which task shapes batch well and which don't? Until at least 3 steps have shipped with the eval, treat per-step batching outcomes as anecdote, not signal — don't change the playbook's batching rules from one step's data.
- **Escalation latency.** From task-agent escalation → coordinator notice → human notice → resolution → task agent re-spawn, what's the round-trip time? Is the 5-min escalation poll right?

### Resolved during the pilot

Questions that started in the open list above and answered cleanly during the steps-1–3 pilot. Kept here so the answer is visible, and so future-self doesn't re-open them without reason.

- **Idle-detection false positives** (coordinator → task-agent direction). 14/14 genuine fires across steps 1–3. `timer_fire_when_idle_any(processes=[<task-agent-pid>])` is reliable as a "task agent done" signal in the coordinator's per-task wake-up loop. *Caveat*: scoped to the coordinator-watching-its-own-task-agents direction. The orchestrator-watching-the-coordinator direction is noisier (the coordinator goes idle many times during a build); see ORCHESTRATOR § Polling the coordinator for the bounded-work pattern that handles it.

- **Orchestrator–coordinator separation pays off.** Step 3 (the first true 3-tier build) ran cleanly. The orchestrator's surface area was light: spawn coordinator-to-be, forward "build", periodically check for `needs-human`, close on completion. The separation kept the human-facing layer honest (orchestrator never writes code) and produced no escalation routing in a clean build. *Do not collapse the tiers on the basis of "the orchestrator had little to do."* The light workload is the success state. Revisit the question only if a step accumulates multiple escalations and the orchestrator's routing role becomes load-bearing.
