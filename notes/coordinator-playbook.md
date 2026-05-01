# Coordinator Playbook Index

This file is now the entrypoint and map for the split playbook.

## Why split

The previous single-file playbook required each role to re-read a large document and ignore most sections. The split reduces token waste, improves role clarity, and makes updates safer.

## Role model (default)

Hypomnema now uses a four-role default:

- `Orchestrator` (human-facing control plane)
- `Coordinator` (step execution control plane)
- `Researcher` (workplan author + persistent research sidecar)
- `Builder` (ephemeral task executor)

Default behavior is researcher-first:

- Coordinator spawns researcher.
- Researcher writes the step workplan.
- Researcher remains alive through build for consult requests.
- Builders route research needs through coordinator.

No non-researcher fallback path is defined here.

## Read order

Runtime/tool selection and tier-based spawning are defined in:

- `notes/playbook/agent-tool-selection.md`

All roles read shared instructions first:

1. `notes/playbook/shared-static.md`

Then role-specific file:

1. Orchestrator: `notes/playbook/orchestrator.md`
2. Coordinator: `notes/playbook/coordinator.md`
3. Researcher: `notes/playbook/researcher.md`
4. Builder: `notes/playbook/builder.md`

## Compatibility note

If an older prompt or note references sections inside `notes/coordinator-playbook.md`, treat those references as pointing to the corresponding file under `notes/playbook/`.
