# Proposals

Home for in-progress proposals: concept notes, draft specs, and working drafts under review. Outside the LDS canonical layers by design — proposals are *transitional* artifacts.

## Lifecycle

1. **Draft** — a `.md` file in this directory. Status header: `Draft` / `In Review` / `Approved`.
2. **Proposal Intake** — when a proposal is ready to shape into roadmap work, write an intake artifact using [`intake-output-template.md`](./intake-output-template.md). This maps planning inputs to proposed steps, deferred decisions, and coverage.
3. **Decomposition** — once approved, the proposal's content is distributed to the appropriate LDS layers (specs, ADRs, architecture, reference, or vision).
4. **Archive** — the original proposal moves to [`archive/`](./archive/) as a frozen historical record. From that point forward, the LDS layers are canonical; the archived proposal is a courtesy reference for "what we were thinking when we approved this."

## Naming

`<slug>.md` for the proposal itself. If user stories or other supporting material are split out, prefix-match the slug: `<slug>-stories.md`, `<slug>-research.md`, `<slug>-intake.md`, etc.

## See also

- [`../project-planning-workflow-notes.md`](../project-planning-workflow-notes.md) — the full planning workflow this directory fits into
- [`../lds-evaluation.md`](../lds-evaluation.md) — why LDS doesn't have a native home for proposals
