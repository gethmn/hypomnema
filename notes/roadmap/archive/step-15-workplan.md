# Step 15 Workplan -- CHANGELOG.md adoption (round-5 shipping gate)

**Status**: Shipped 2026-04-29. This workplan is for [`notes/roadmap/roadmap-5.md`](./roadmap-5.md) Step 15 and todo #122.

**Goal**: create repo-root `CHANGELOG.md` in Keep a Changelog format, back-fill the released `v0.1.0` through `v0.3.0` milestones from the shipped round retros / round gates, add a `v0.4.0` entry for this round, and codify the going-forward boundary ritual in `notes/project-planning-workflow-notes.md`.

## Changelog automation decision

Hypomnema adopts a **manual Keep a Changelog ritual** for now.

- Do **not** adopt Xcind's `git-cliff` / `contrib/release` release pipeline in this step.
- Do **not** add pre-commit hooks, CI drift checks, PR comments/artifacts, or a GitHub Action that opens or updates a changelog PR.
- Do **not** introduce a release helper just to generate `[Unreleased]` material.
- If the project later wants a convenience wrapper, prefer a tiny `just` recipe over release automation, but keep that out of Step 15.

Rationale: Step 15 is a docs-only shipping gate. The changelog is the human-edited boundary record, and release automation is still explicitly out of scope for round 5.

## Task plan

1. **Write `CHANGELOG.md` at the repo root.**
   - Use Keep a Changelog structure with an empty `## [Unreleased]` section at the top.
   - Add version sections for `v0.4.0`, `v0.3.0`, `v0.2.0`, and `v0.1.0` in descending order.
   - Keep the entries round-level, not step-level:
     - `v0.1.0`: chunking + embedding, semantic search, stdio MCP wrapper, and the round's shipping gate work.
     - `v0.2.0`: multi-vault refactor, vault control plane, lifecycle ops, cross-vault search, and the `hmnd scan` removal boundary.
     - `v0.3.0`: Streamable HTTP MCP transport, `/mcp`, Origin validation, and transport parity with stdio MCP.
     - `v0.4.0`: CI pipeline, outbox flake hardening / characterization, and CHANGELOG adoption itself.
   - Include date stamps that match the shipped round dates.
   - Add reference links at the bottom for the released tags and compare links.

2. **Update `notes/project-planning-workflow-notes.md`.**
   - Add a CHANGELOG step to the step-boundary ritual.
   - Make the ritual explicit enough that future steps update `CHANGELOG.md` before the next workplan is written.
   - Keep the note short and procedural; do not turn it into a release-automation policy doc.

3. **Self-review the changelog against the roadmap and retros.**
   - Cross-check the round summaries against `notes/roadmap/roadmap-2.md`, `notes/roadmap/roadmap-3.md`, and `notes/roadmap/roadmap-4.md`.
   - Make sure the `v0.1.0` / `v0.2.0` / `v0.3.0` entries match the shipped round-level scope, not the individual step lists.
   - Verify the `v0.4.0` entry mentions only this round's work: CI, outbox flake follow-up, and CHANGELOG adoption.
   - Keep the prose concise and release-note-like; the retros remain the authoritative detailed record.

## Verification plan

- Run `git diff --check` after editing.
- Spot-check the rendered markdown headings and link targets with a quick file read.
- No Rust test suite run is expected for this step; the work is doc-only.

## Completion checklist

- [ ] `CHANGELOG.md` exists and reads cleanly in Keep a Changelog form.
- [ ] `notes/project-planning-workflow-notes.md` includes the CHANGELOG boundary step.
- [ ] Step 15 work is consistent with the round shipping records and the roadmap.
- [ ] Final completion comment posted on todo #122.
