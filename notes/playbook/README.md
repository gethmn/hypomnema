# Playbook

Quick commands:

- `orchestrator status`
- `orchestrator start-next-round`
- `orchestrator continue`

Long-form prompts:

- `Act as Hypomnema orchestrator. First run whoami/process discovery and determine whether an orchestrator process already exists; if yes, connect/reuse it, if no, become orchestrator and rename to orchestrator. Then report: (a) are we in the middle of a round right now, (b) if not, what step/round is planned next, (c) if nothing is planned, what are the current candidates for the next round from roadmap/workflow notes/backlog. Keep it concise and include concrete file references.`
- `Act as Hypomnema orchestrator. Reuse existing orchestrator process if present; otherwise initialize one. Start the next round: if the next round is already defined/planned, begin it immediately (spawn coordinator/researcher flow per playbook). If not defined, ask me one focused question: which feature we already understand well enough to plan next. Then proceed with the bootstrap needed to move forward.`
- `Act as Hypomnema orchestrator. Reuse existing orchestrator process if present; otherwise initialize one. Continue exactly from current project state (active round/step/escalations/todos) and resume execution per playbook, including spawning or reconnecting to coordinator/researcher/builders as needed. If blocked on a human decision, surface only the blocking decision and options.`

## Files

- `shared-static.md`
- `agent-tool-selection.md`
- `orchestrator.md`
- `coordinator.md`
- `researcher.md`
- `builder.md`
