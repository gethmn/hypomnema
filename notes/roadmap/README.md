# Roadmap

Home for active and archived round-level roadmaps and per-step workplans. Outside the LDS canonical layers by design — roadmaps and workplans are *transitional* artifacts that drive build cycles, then become historical record.

## Lifecycle

1. **Active round** — `roadmap-N.md` lives in this directory while round N is in flight, with `step-NN-workplan.md` for each step as it's built. Status headers: `Not started` / `In progress` / `Shipped <date>`.
2. **Step ships** — the step's workplan moves to [`archive/`](./archive/) with `**Status**: Shipped <date>` overwritten on the file. Per-step retros land in [`../project-planning-workflow-notes.md`](../project-planning-workflow-notes.md).
3. **Round ships** — the round's `roadmap-N.md` moves to [`archive/`](./archive/) alongside its step workplans. The active dir resets, ready for round N+1.

## Naming

`roadmap-N.md` for round-level docs (numbered from round 1). `step-NN-workplan.md` for per-step workplans (zero-padded step number, scoped to the round that contains the step).

## See also

- [`../project-planning-workflow-notes.md`](../project-planning-workflow-notes.md) — the full planning workflow this directory fits into, including the step-boundary ritual and per-step retro template
- [`../coordinator-playbook.md`](../coordinator-playbook.md) — the Solo orchestrator/coordinator/task-agent contract that drives builds against these workplans
- [`../lds-evaluation.md`](../lds-evaluation.md) — why LDS doesn't have a native home for roadmaps
