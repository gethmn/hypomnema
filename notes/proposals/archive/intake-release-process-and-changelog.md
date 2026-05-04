# Proposal Intake: Release Process and Changelog

**Status**: Intake complete
**Date**: 2026-05-03
**Intake inputs**:

- `notes/proposals/release-process-and-changelog.md` — Primary proposal (Status: Draft, 2026-05-02, v0.1.0)
- `notes/backlog.md` § Round-6 carry-over — "Release automation" entry; gates changelog generation
- `notes/backlog.md` § Round-5 candidates — strikethrough "CHANGELOG.md adoption" (Retired 2026-05-02), explaining the policy reversal that produced this proposal

> **No peer stories file.** `notes/proposals/release-process-and-changelog-stories.md` does not exist. The proposal is a *policy + gating-criteria* document, not a feature spec; it deliberately defers concrete delivery to a future release workplan.

---

## Summary

The proposal does two distinct things in one document:

1. **Codifies the current policy** that Hypomnema maintains no standalone changelog ritual until a real release process exists. Roadmaps, workplans, retrospectives, tags, and Git history carry the project record. No round-boundary changelog step, no CI changelog enforcement, no PR-time changelog requirement, no bots.
2. **Lists the gating criteria** that a future release workstream must resolve before reintroducing changelog machinery: release trigger, version source of truth, artifact story, changelog generation rules, release-notes handoff, failure behavior, and the local-vs-CI split. The preferred shape is Xcind-like: `git-cliff` invoked from a `contrib/release` script (or `just` recipe) that bumps versions, regenerates the changelog for unreleased commits, commits, tags, and hands notes to GitHub Releases — release command *owns* the changelog as an output, not a separate ritual.

Critically, the proposal explicitly excludes binary distribution, signing, provenance, and Conventional Commits enforcement from its own scope; those belong to the eventual release workplan, not this document.

There is a live discrepancy in the repo today worth surfacing during workplan-write: `CHANGELOG.md`, `cliff.toml`, and `contrib/changelog-hook` exist (shipped in round 5, step 13/15), but the proposal-stated current policy is "no repo-root CHANGELOG.md is maintained." The retired backlog entry confirms changelog adoption was rolled back 2026-05-02. The cleanup/codification scope of any near-term step needs to reconcile artifact presence with stated policy.

## Source Inputs

| Source | Type | Role in intake |
|---|---|---|
| `notes/proposals/release-process-and-changelog.md` | proposal | primary — policy statement, gating criteria, sketch of future release script, non-goals, open questions |
| `notes/backlog.md` § Round-6 carry-over → "Release automation" | backlog | primary — the live un-shipped item; explicitly notes "Changelog generation is gated on this work" |
| `notes/backlog.md` § Round-5 candidates → "CHANGELOG.md adoption" (strikethrough, Retired 2026-05-02) | backlog | supporting — historical context for why this proposal exists; the manual-changelog ritual it replaces |
| `notes/roadmap/archive/roadmap-5.md` (steps 13, 15) | shipped round | background — round-5 shipped the CI pipeline and the manual-changelog adoption that this proposal retires |
| `notes/project-planning-workflow-notes.md` § Current changelog policy (line 118) | workflow notes | background — policy reference already updated to point at this proposal |
| Repo state: `CHANGELOG.md`, `cliff.toml`, `contrib/changelog-hook` | source artifacts | background — present today; reconciliation with stated policy is part of any near-term codification step |

## Candidate Outcomes

- **Outcome: Codified "no changelog ritual" policy**
  - Source: Proposal § Current Policy + § Non-Goals
  - User-visible result: Agents and contributors have a single source of truth that they do not need to update `CHANGELOG.md` at round boundaries, and PR review does not block on changelog edits.
  - Verification signal: `notes/project-planning-workflow-notes.md` references the policy (already done at line 118); orchestrator/coordinator playbooks have no changelog steps; CI has no changelog-enforcement check.

- **Outcome: Repo state matches stated policy**
  - Source: Proposal § Current Policy ("No repo-root `CHANGELOG.md` is maintained") + repo artifacts (`CHANGELOG.md`, `cliff.toml`, `contrib/changelog-hook`)
  - User-visible result: Either (a) the artifacts are removed/archived to match policy, or (b) policy text is amended to acknowledge the artifacts exist as historical record but are not maintained at round boundaries. Contributors have no ambiguity.
  - Verification signal: A new contributor reading the proposal and the repo simultaneously cannot find a contradiction.

- **Outcome: Documented gating criteria for the future release workstream**
  - Source: Proposal § Gating Criteria + § Open Questions
  - User-visible result: The seven gating criteria become a documented checklist that any future release-workstream proposal must answer before workplan-write — release trigger, version source of truth, artifact story, changelog generation rules, release-notes handoff, failure behavior, local-vs-CI split.
  - Verification signal: When the next proposal in this lineage lands (the actual release workstream), it can reference these criteria directly rather than rediscovering them.

- **Outcome (deferred): Working release command**
  - Source: Proposal § Future Release Process (sketched) + backlog "Release automation"
  - User-visible result: A maintainer runs `contrib/release X.Y.Z` (or `just release X.Y.Z`); version-bearing files update; changelog regenerates from unreleased commits; commit + tag land; GitHub Release publishes with generated notes; binaries optionally attached.
  - Verification signal: A real release goes out through the documented command.
  - **Status**: Out of scope for any near-term step; a separate proposal cycle is needed.

## Proposed Roadmap Shape

The proposal supports **two distinct workstreams** at very different readiness levels. They should not be bundled into one round.

### Step N (near-term, plannable now) — Release/Changelog Policy Codification

**Goal**: Reconcile the repo with the stated "no changelog ritual until release exists" policy, and lift the gating criteria into a durable contributor-visible reference.

**Shipping criteria**:

- [ ] Repo state reconciled with policy. Workplan-time choice between two paths:
  - Path A (remove): delete `CHANGELOG.md`, `cliff.toml`, and `contrib/changelog-hook`; document removal in a project-planning-workflow-notes entry.
  - Path B (preserve as historical, mark unmaintained): keep `CHANGELOG.md` with a header noting it is frozen and references this proposal; remove or archive `cliff.toml` + `contrib/changelog-hook` if they imply active automation.
- [ ] Coordinator/orchestrator/researcher playbooks (`notes/playbook/*.md`) checked for any residual changelog instructions; removed or amended.
- [ ] `notes/project-planning-workflow-notes.md` § shipping-gate checklist (round-boundary ritual) verified to omit changelog steps. The note at line 118 already references the proposal — confirm consistency across the document.
- [ ] Any CI workflow steps or pre-commit hooks that touch the changelog are removed or made no-ops with a comment pointing at the proposal.
- [ ] Gating criteria from § Gating Criteria promoted to a stable location (e.g. a section in `docs/specs/ci-pipeline.md`, or a new short `docs/decisions/00NN-no-changelog-until-release.md` ADR — researcher's call) so the next proposal in this lineage has a single referent.
- [ ] Backlog hygiene: confirm strikethrough on "CHANGELOG.md adoption" is still accurate; ensure live "Release automation" entry references this intake/proposal.
- [ ] Negative fingerprint: `rg -i "update CHANGELOG|update the changelog|changelog entry" notes/playbook docs/specs .github` returns zero matches in active canon.
- [ ] `cargo test` green; `cargo clippy -- -D warnings` clean (no code path changes expected, but verify if any contributor doc generation is affected).

**Deferred decisions resolved in this step**:

- Decision: Remove vs. preserve `CHANGELOG.md` / `cliff.toml` / `contrib/changelog-hook`
  - Source: Repo-state-vs-policy reconciliation (above) — not explicit in the proposal but unavoidable
  - Why this step: Proposal says "no repo-root `CHANGELOG.md` is maintained"; artifacts exist; one of the two paths must be picked.
- Decision: Where the gating criteria live (new ADR vs. section in existing spec vs. dedicated `docs/release-policy.md`)
  - Source: Workplan-time call
  - Why this step: The criteria need a stable home for the future release proposal to anchor against.
- Decision: Whether `git-cliff` config (`cliff.toml`) is preserved as a research artifact for the future release workstream or removed and rebuilt later
  - Source: Repo-state-vs-policy reconciliation
  - Why this step: Keeping it imposes a maintenance question (does it drift?); removing it means the future workstream starts from scratch. Recommend remove unless explicitly load-bearing.

**New deps**: none.

**Risk**: low. This is a docs/policy step with limited code surface. The only real risk is missing a residual changelog instruction in playbooks or CI that creates a future surprise; mitigated by the negative-fingerprint sweep.

**Source coverage**:

- `release-process-and-changelog.md`: § Overview, § Rationale, § Current Policy, § Gating Criteria (lifted), § Non-Goals
- `notes/backlog.md`: "CHANGELOG.md adoption" strikethrough confirmed; "Release automation" entry stays live and references this intake

### Step M (deferred — needs its own proposal cycle) — Release Workstream

**Goal**: Implement the release command, version-bumping flow, changelog generation as a release output, GitHub Release integration, and (optionally) binary artifact distribution.

**Status**: **Not plannable as-is.** All four § Open Questions are unresolved; § Gating Criteria has seven points that need explicit decisions; binary distribution and signing/provenance are explicit non-goals of *this* proposal but would need to be in scope for the actual release workstream. Recommend a follow-up proposal (`notes/proposals/release-workstream.md` or similar) that answers the gating criteria, then runs Proposal Intake.

**What this step would cover (sketch only)**:

- `cliff.toml` (or equivalent) tuned to the project's commit conventions
- `CHANGELOG.md` regeneration as part of release (not as a manual ritual)
- `contrib/release` script (or `just release-*` recipe) — bumps version, regenerates changelog, commits, tags
- GitHub Release workflow — triggered by tag push or manual dispatch
- Binary artifact decisions (cargo-dist, prebuilt binaries, checksums, `gethmn.io` install scripts) — these are listed in `notes/backlog.md` § Round-6 carry-over under "Release automation" and may or may not ship in the same round
- `docs/specs/release-process.md` (or similar) canonically describing the release contract

**Why deferred**: Every gating criterion is currently unresolved (see § Open Questions). Workplan-write would be guesswork.

## Coverage Map

| Source item | Proposed step | Status | Notes |
|---|---|---|---|
| Proposal § Overview | Step N | planned | Policy codification. |
| Proposal § Rationale (manual changelog ritual was wrong middle state) | Step N | planned | Captured in policy reconciliation. |
| Proposal § Current Policy (5 bullets) | Step N | planned | Source-of-truth promotion + repo-artifact reconciliation. |
| Proposal § Future Release Process (sketch) | Step M | deferred | Out of scope for Step N; informs Step M when planned. |
| Proposal § Gating Criteria (7 points) | Step N (lift) + Step M (resolve) | planned (lift) / deferred (resolve) | Lift verbatim into a stable docs home in Step N; resolve in Step M's proposal. |
| Proposal § Non-Goals (4 items) | Step N | planned | Documented as part of policy. |
| Proposal § Open Questions Q1 (source-tag-only vs. binary artifacts) | Step M | deferred | Blocks Step M planning; needs human decision. |
| Proposal § Open Questions Q2 (commit-source for release notes) | Step M | deferred | Blocks Step M planning; depends on Hypomnema's commit-style policy. |
| Proposal § Open Questions Q3 (`git-cliff` grouping policy) | Step M | deferred | Workplan-time decision in Step M, not earlier. |
| Proposal § Open Questions Q4 (timing — pre-`v1` vs. external users) | Step M | deferred | Strategic decision; informs *whether* and *when* Step M lands. |
| `notes/backlog.md` "Release automation" | Step M | deferred | Lives in backlog until Step M proposal cycle starts. |
| `notes/backlog.md` "CHANGELOG.md adoption" (retired) | Step N | planned | Confirm strikethrough remains accurate at step close. |
| Repo: `CHANGELOG.md`, `cliff.toml`, `contrib/changelog-hook` | Step N | planned | Reconcile per Path A or Path B (deferred decision 1). |
| `notes/project-planning-workflow-notes.md` § current changelog policy (line 118) | Step N | planned | Already partial; verify and tighten. |

## Deferred / Out-of-Scope Items

- Item: Binary distribution, cross-compilation, checksums, signing, provenance
  - Source: Proposal § Non-Goals
  - Reason: Explicitly out of scope for *this* proposal; lives in backlog "Release automation" and would belong to Step M's proposal.
  - Revisit trigger: Step M proposal cycle.
- Item: Conventional Commits enforcement
  - Source: Proposal § Non-Goals ("Do not require Conventional Commits solely for changelog generation")
  - Reason: Should be a Step M decision *only if* the changelog generation grouping policy needs it.
  - Revisit trigger: Step M, Q3 resolution.
- Item: `gethmn.io` install scripts / package-manager packaging
  - Source: Aligned with `notes/backlog.md` § Round-6 carry-over "Release automation"
  - Reason: Not in this proposal's scope.
  - Revisit trigger: Step M proposal cycle (potentially as a separate sub-step).
- Item: PR-time changelog enforcement / bot comments
  - Source: Proposal § Current Policy + § Non-Goals
  - Reason: Permanently out — these are the very rituals the proposal exists to retire.
  - Revisit trigger: None expected.

## Open Questions

These are the proposal's own § Open Questions, all blocking confident planning of Step M (the actual release workstream). They are *not* blocking for Step N (policy codification).

- Question: Should the first release process be source-tag-only, or wait until binary artifacts are ready?
  - Why it matters: Determines the entire shape of Step M — a tag-only release is a small docs+script step; a binary-distribution release is a multi-step round with cargo-dist, cross-compilation, checksums, and possibly signing.
  - Blocks roadmap? yes (for Step M); no (for Step N).
  - Suggested owner: Human (project owner) — strategic call.
- Question: Should release notes be generated from merge commits, squash commits, or individual commits?
  - Why it matters: Affects `cliff.toml` configuration and indirectly whether commit-style discipline is enforced.
  - Blocks roadmap? yes (for Step M); no (for Step N).
  - Suggested owner: Researcher at Step M workplan-write, given the project's actual commit history pattern.
- Question: Should `git-cliff` group maintenance and documentation commits, or omit them unless they are user-visible?
  - Why it matters: Tunes `cliff.toml` grouping; affects readability of generated release notes.
  - Blocks roadmap? yes (for Step M); no (for Step N).
  - Suggested owner: Researcher at Step M workplan-write.
- Question: Should release automation land before `v1`, or only when Hypomnema has external binary users?
  - Why it matters: Determines *whether* Step M is the next round, a later round, or doesn't ship in v0 at all.
  - Blocks roadmap? yes (for Step M); no (for Step N).
  - Suggested owner: Human (project owner) — strategic call. v0.5.0 just shipped per round-10 retro; the natural decision point is now or at v1.
- Question (raised by intake): Path A vs. Path B for repo-state reconciliation — remove or preserve `CHANGELOG.md` / `cliff.toml` / `contrib/changelog-hook`?
  - Why it matters: The proposal says "no repo-root `CHANGELOG.md` is maintained" but the file exists. Step N must pick.
  - Blocks roadmap? no — blocks Step N workplan.
  - Suggested owner: Researcher at Step N workplan-write; recommend Path A (remove) unless preserving the artifacts has demonstrable value.
- Question (raised by intake): Where do the gating criteria live durably?
  - Why it matters: Step M's eventual proposal needs a stable referent. Options: new ADR, section in `docs/specs/ci-pipeline.md`, dedicated `docs/release-policy.md`, or in-place in this proposal moved to `docs/`.
  - Blocks roadmap? no — blocks Step N workplan.
  - Suggested owner: Researcher at Step N workplan-write.

## Recommendation

**Split-track recommendation.**

For Step N (policy codification):

- [x] Draft `notes/roadmap/roadmap-N.md` (small dedicated round, or fold into a polish-style round if one is already planned)
- [x] Draft `notes/roadmap/step-NN-workplan.md` after researcher resolves Path A vs. Path B and the gating-criteria home
- [ ] Refine planning inputs first

For Step M (release workstream):

- [ ] Draft `notes/roadmap/roadmap-N.md`
- [ ] Draft `notes/roadmap/step-NN-workplan.md`
- [x] Refine planning inputs first

Rationale: The proposal as written is two readiness levels in one document. The policy half is shippable now — the inputs are unambiguous, the surface is small, and there is a real repo-vs-policy discrepancy that costs contributor clarity until reconciled. The release-workstream half is unplannable as-is: every § Open Question is a real blocker, and § Gating Criteria has seven points that need explicit human and researcher input. Recommend running Step N opportunistically (it's small enough to bundle with another low-risk round if desired), and parking Step M behind a follow-up proposal that answers the gating criteria. Given Hypomnema just shipped v0.5.0 (round 10, 2026-05-03), Q4's "before v1 vs. when external users exist" decision is the natural strategic gate; the human's answer to that question determines whether Step M's proposal cycle starts soon or waits.

Comparison to peer proposals currently in `notes/proposals/`:

- **Static sqlite-vec bundling** (`intake-static-sqlite-vec-bundling.md`, 2026-05-03) — fully planned, ready to start; medium risk; closes a recurring footgun.
- **HyDE semantic search** (`intake-hyde-semantic-search.md`, 2026-05-02) — intake complete, ready to start.
- **Release process & changelog** (this intake) — split: Step N small/ready, Step M needs a follow-up proposal.

If the orchestrator is selecting the next round, this proposal's Step N is the smallest of the three but also the lowest user-visible value; it can ride alongside another round as a side-step or wait for the strategic decision on Q4. Static sqlite-vec bundling and HyDE are both stronger first-pick candidates on their own merits.

## Human Review Notes

(append review decisions here)
