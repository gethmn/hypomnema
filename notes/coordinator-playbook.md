# Coordinator Playbook Index

This file is the entrypoint and map for the split playbook.

## Why split

The previous single-file playbook required each role to re-read a large
document and ignore most sections. The split reduces token waste, improves
role clarity, and makes updates safer. The playbook is also organized along
a runtime-agnostic / runtime-bound seam so the role model can target any
orchestration layer.

## Role model (default)

Hypomnema uses a four-role default:

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

## Layering

The playbook is split into three layers:

- **Layer A — Role model (agnostic):** `shared-static.md` plus the four
  role files. References operations by capability verb only.
- **Layer B — Capability contract (agnostic seam):** `capabilities.md`.
  Defines abstract operations, tier/role policy, and load-bearing
  escalation criteria.
- **Layer C — Runtime providers (composable; base + overlays):**
  `runtimes/<name>.md` maps capabilities to concrete tools. A session uses
  a profile of one complete base provider plus zero or more partial
  overlays; capabilities resolve through overlays first, then the base.
  The default profile is `base: solo, overlays: []`. See
  `runtimes/README.md` for the composition rules; today's providers are
  `runtimes/solo.md` (complete base) and `runtimes/duo.md` (partial
  overlay covering `spawn-agent-at-tier`).

## Read order

All roles read shared instructions first, then the seam, then the active
runtime binding, then the role-specific file:

1. `notes/playbook/shared-static.md`
2. `notes/playbook/capabilities.md`
3. The active profile's base, then each overlay in order
   (default profile: `runtimes/solo.md` only)
4. Role file:
   - Orchestrator: `notes/playbook/orchestrator.md`
   - Coordinator: `notes/playbook/coordinator.md`
   - Researcher: `notes/playbook/researcher.md`
   - Builder: `notes/playbook/builder.md`

## Compatibility note

- References to sections inside the old `notes/coordinator-playbook.md`
  resolve to the corresponding file under `notes/playbook/`.
- References to `notes/playbook/agent-tool-selection.md` resolve to
  `capabilities.md` (tier policy) and `runtimes/solo.md` § Tier Resolver
  (Solo-specific mechanics). The file remains as a thin pointer.
