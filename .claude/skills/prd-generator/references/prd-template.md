# PRD Template

Use this template when generating a PRD. Adapt the level of detail to the scope — a focused feature needs less than a new product. Not every section is required for every PRD. Use judgment, but err on the side of including a section with "[TBD]" rather than silently omitting it.

---

## Document Header

```
# [Product/Feature Name] — PRD
**Author:** [Name]
**Status:** Draft | In Review | Approved
**Last Updated:** [Date]
**Stakeholders:** [List key stakeholders and their roles]
```

---

## 1. Problem Statement

*What user or business problem are we solving, and why now?*

This is the most important section. If this isn't compelling, nothing else matters.

Write 2-4 paragraphs covering:

- **The problem itself** — stated from the user's perspective. Be specific and concrete. "Users can't find previously saved reports because search only matches exact titles, forcing them to scroll through hundreds of items" not "Users are frustrated with search."
- **Who is affected** — specific user segments, with rough scale if known.
- **Current impact** — quantify if possible: support ticket volume, time wasted, revenue lost, churn attributed to this.
- **Why now** — what's changed that makes this worth solving at this moment? New data, competitive pressure, strategic priority shift, regulatory deadline?

**Watch out for:**
- Problem statements that are really solutions in disguise: "Users need a dashboard" → What decision can't they make without one?
- Problems with no evidence. If you don't have data, say so: "Hypothesis based on [source], needs validation via [method]."

---

## 2. Current State & Research Findings

*What exists today? What did codebase/domain research reveal?*

This section captures what was discovered during the research phase. It grounds the PRD in reality and prevents building on false assumptions about the current system.

**Existing Functionality:**
- What related features or capabilities already exist in the product?
- What partial implementations, abandoned attempts, or feature flags were found?
- What existing user journeys does this feature touch or modify?

**Technical Context:**
- What tech stack, frameworks, and patterns does the current system use in this area?
- What data models, APIs, or services are relevant?
- What technical debt or constraints were discovered that affect this feature?

**Gaps & Opportunities:**
- What's missing from the current experience that validates the problem statement?
- What existing infrastructure can be leveraged (e.g., existing models, unused fields, partial implementations)?
- What inconsistencies were found that this feature should address or at least not worsen?

If this is a greenfield product with no existing codebase, use this section for competitive landscape and market research findings instead.

---

## 3. Target Users & Personas

*Who are we building this for?*

For each persona (target 1-3, rarely more):

- **Name & Role** — e.g., "Sarah, Operations Manager at a 50-person logistics company"
- **Key behaviors** — how they interact with the product today
- **Pain points** — specific frustrations relevant to this problem
- **Goal** — what success looks like for this person
- **Constraints** — technical literacy, device usage, time availability, etc.

Identify the **primary persona** — the one you'd optimize for if forced to choose.

---

## 4. Goals & Success Metrics

*How will we know this worked?*

Define 2-3 measurable outcomes. For each:

| Metric | Current Baseline | Target | How Measured |
|--------|-----------------|--------|--------------|
| e.g., Avg. support ticket resolution time | 4 hours | < 1 hour | Zendesk analytics |
| e.g., Feature adoption rate (30-day) | N/A | > 40% of target persona | Product analytics |

**Guidelines:**
- Prefer outcome metrics (user behavior changes) over output metrics (features shipped).
- Include at least one guardrail metric — something that should NOT get worse. E.g., "Conversion rate does not drop by more than 2%."
- If you can't measure it yet, say what instrumentation is needed.

---

## 5. Proposed Solution / Elevator Pitch

*In 2-3 sentences, what are we building?*

Write this in plain language that a non-technical stakeholder could understand. This is the "if your CEO asked in an elevator" version.

For product PRDs, optionally include:
- **Top 3 value propositions** for the user
- **Conceptual model** — a brief description of the key objects/concepts the user will interact with and how they relate

---

## 6. User Journeys & Use Cases

*How will people actually use this?*

For each key use case (usually 2-5):

### Use Case: [Name]

**Persona:** [Which persona]
**Trigger:** What causes the user to start this journey
**Steps:**
1. User does X
2. System responds with Y
3. User does Z
...
**Outcome:** What the user has achieved
**Edge cases / error states:** What happens when things go wrong

Consider the full lifecycle:
- **First-time experience** — onboarding, empty states
- **Core usage** — the happy path
- **Maintenance** — updating, managing, bulk operations
- **Edge cases** — errors, permissions, conflicts, empty/overflowing data
- **Exit** — deletion, export, migration, account closure

---

## 7. Functional Requirements

*What must the product do?*

Organize by use case or user journey, NOT by technical component. For each requirement, assign a priority:

- **P0** — Required for MVP. Users cannot adopt the product without this.
- **P1** — High-value addition for a minimally delightful product. Ship shortly after MVP.
- **P2** — Nice-to-have. Can be deferred to future iterations.

**Format:**
```
**[P0]** [Requirement ID] — [Persona] can [action/capability]
  Context: [Why this is needed]
```

**Example:**
```
**[P0]** REQ-001 — Authenticated user can search saved reports by keyword, with results matching partial titles
  Context: Currently users must scroll through all reports; average user has 200+ saved reports

**[P1]** REQ-002 — Authenticated user can filter search results by date range and report type
  Context: Power users (15% of base) manage reports across multiple categories

**[P2]** REQ-003 — System suggests recent and frequently-accessed reports before user begins typing
  Context: Analytics show 60% of searches are for reports accessed in the last 7 days
```

**Rules:**
- Focus on WHAT, not HOW. "User can export data" not "API endpoint returns CSV."
- Don't include design details. "User can dismiss notification" not "Blue toast notification with X button at top right."
- Don't include performance metrics unless there's concrete evidence users need them for adoption.
- Include telemetry requirements: "Product team can monitor search usage patterns and result relevance."

---

## 7b. Global Invariants

*Project-wide rules every story must uphold. Implementers and reviewers should see these alongside every story's acceptance criteria.*

Global Invariants are the load-bearing rules that span the whole feature — security boundaries, mandatory payload fields, deprecated paths to delete, data isolation rules, environment-config requirements. They are NOT story-level acceptance criteria; they are the constraints every story must respect simultaneously.

If you do not declare invariants here, the implementer will only see the per-story acceptance criteria, and cross-cutting decisions will get lost between stories. Do not skip this section.

**Format:**

```
## Global Invariants

- [Rule]. **Why:** [the reason — incident, security boundary, decision]. **How to verify:** [what the reviewer should grep for or check].
- ...
```

**Examples** (drawn from a real "threads" feature retrospective):

- Every endpoint accepting a `thread_id` must reject `thread_id`s that do not belong to the authenticated user. **Why:** prior implementation had a cross-tenant write hole. **How to verify:** Form Request must use `Rule::exists(...)->where('user_id', auth()->id())`, AND a feature test must assert another user gets 403/422.
- Every chat-related broadcast event must include `threadId` in its payload. **Why:** the frontend must dispatch live deltas to the correct UI thread. **How to verify:** grep for `broadcastWith()` on chat events; each must include `threadId`.
- No user-wide query exists in `ChatHistory`. Every method takes `threadId` as a required parameter — no nullable defaults. **Why:** thread-unaware queries silently leak context across threads. **How to verify:** grep for `?int $threadId = null` in `ChatHistory`; should be zero matches.
- Authorization for `Thread` lives in `app/Policies/ThreadPolicy.php`, not scattered across FormRequests or controllers. **Why:** decided in §15. **How to verify:** check `app/Policies/ThreadPolicy.php` exists; FormRequests delegate via `$user->can(...)`.

**Rules for writing good invariants:**

- Each invariant should be specific and *enforceable* — a reviewer must be able to grep for a violation or write a test that proves it.
- Tag with `**Why:**` so future stories understand the reason and can judge edge cases.
- Tag with `**How to verify:**` so the reviewer has a concrete check to run.
- An invariant that is satisfied by every story trivially is not a useful invariant. Only include rules that have actually been violated or are easy to violate.
- Keep the list short — 3 to 8 invariants for most features. If you have more than ~10, you are probably writing requirements, not invariants.

**When the PRD's thesis is "eliminate X" (drift, half-wiring, stringly-typed boundaries, consolidation, dead tier):**

- **Map the full boundary graph.** Enumerate every hop the value takes — source → storage → read model → transport → render — and state the type guarantee at each. If any hop is an untyped passthrough, either bring it in scope or state explicitly why that hop is safe to leave untyped. Silent hops are the #1 source of half-wiring churn.
- **Pair every positive grep with a negative fingerprint.** An invariant like "all X come from enum Y" needs both: a positive verification (N expected call sites) AND a negative grep that returns zero matches when the anti-pattern is fully gone. Positive-only greps cannot catch survivors in files the story didn't touch.
- **Forbid silent fallbacks explicitly.** If the motivating bug involved unknown values being silently accepted (default branch, null-coalesce to "safe" value, lenient enum parsing), include an invariant that the unknown-value path must fail loudly at the boundary it crosses. Without this, the cleanup can ship while the fallback pattern survives in unchanged files.
- **Name sibling files explicitly.** If the invariants name `Foo`, they must also name `FooVariant` / `FooSibling`. Silence on the sibling means the sibling drifts.

**Additional example — silent-fallback invariant:**

- Unknown severity values must raise an error at the boundary they cross, not fall through to a default tier. **Why:** the original bug was possible because `?? 'low'`-style fallbacks silently mapped unknown strings to the Low tier, hiding the read/write asymmetry for months. **How to verify:** grep for null-coalescing fallbacks against the severity map returns zero matches; a unit test asserts the parser throws on an unknown value.

---

## 8. User Stories & Acceptance Criteria

*The detailed backlog.*

See `references/user-story-guide.md` for comprehensive guidance.

### Acceptance criteria must be observable

Every acceptance criterion must be checkable by something an external observer can run — an HTTP request/response, a UI assertion, a browser test, an integration test that hits a real endpoint. If the only proof of a criterion is "a row exists in the database" while the user-visible behavior is broken, the criterion is satisfied at the wrong layer.

**Bad:** `- [ ] When the user sends a message, a conversation row is created with thread_id set.`
**Good:** `- [ ] When the user sends a message in Thread A, then opens Thread B, then returns to Thread A, the message is visible in Thread A and absent from Thread B.`

The first criterion can pass while the user sees nothing. The second cannot.

### Every AC must be discriminating

Universal smoke check for any requirement or AC: *would this still pass if the function under test returned a constant, a trivial value, or an overly-inclusive result?* If yes, the criterion is tautological and will not catch the bug it claims to prevent.

**Failure shape 1 — response-shape ACs that check structure, not values.**

**Bad:** `- [ ] The response includes a thread object with an id.` (`assertJsonStructure(['thread' => ['id']])` passes even if `forked_from_thread_id` — the field that proves the fork worked — is missing.)
**Good:** `- [ ] The response is 201 with body `{thread: {id: <new id>, forked_from_thread_id: <source id>, is_renamed: true}}`.`

**Failure shape 2 — selection/cutoff predicates keyed on non-authoritative fields.**

**Bad:** `- [ ] Messages where `created_at <= fork_point.created_at` are cloned.` (Two messages sharing a one-second timestamp both match; the boundary cannot distinguish "before" from "same instant as.")
**Good:** `- [ ] Messages where `(created_at, id) <= (fork_point.created_at, fork_point.id)` are cloned.` (Authoritative, collision-free.) When specifying cutoffs across related requirements (messages, summaries, attachments), use the same key style — mixing timestamp-based and id-based boundaries for the same logical cutoff is silent drift.

**Failure shape 3 — guardrails where the fixture sets both sides.**

**Bad (tautological):** `- [ ] The test creates an entity with field='X', attaches children, and asserts compute() returns X.`
**Good (constructed oracle):** `- [ ] The factory derives the persisted field by calling compute() over the attached children; the test fails if compute() returns a different value than the construction step used.`

### Don't prescribe preserving a known anti-pattern

Criteria that explicitly preserve the defect class the PRD exists to eliminate ("continue to use untyped string — do not introduce narrowing as part of this cleanup") bake the bug back in. If a boundary needs tightening, tighten it. If the tightening is truly out of scope, move it to §9 and note it in the thesis check — don't anchor it in an AC.

### Docs references: symbol, not line

When a story's output is a doc, specify references as `path` plus class / function / constant name — stable across refactors. Line numbers in prose rot on the next unrelated edit. Reserve line numbers for research citations (write-once) and machine-checked greps.

### Stories touching ownership-scoped resources MUST include adversarial criteria

If a story touches an endpoint, query, or resource keyed by `user_id` / `tenant_id` / owner, include at least one explicit adversarial criterion:

- `- [ ] Given a thread belonging to another user, when the authenticated user submits a request with that thread_id, then the request is rejected with 403/422 and no rows are written.`

This is non-negotiable. Cross-tenant defense missing from a story will be caught by the reviewer only if the invariant is declared in §7b AND the story's ACs exercise it.

### Structure stories by epic

### Epic: [Name — maps to a user journey or major capability]

**Story [ID]:** As a [persona], I want to [goal] so that [benefit].

**Acceptance Criteria:**
- [ ] [Criterion 1 — specific, testable condition]
- [ ] [Criterion 2]
- [ ] [Criterion 3]

*Or in Given/When/Then format for complex behavior:*

**Acceptance Criteria:**
- Given [context], when [action], then [expected result]
- Given [context], when [action], then [expected result]

**Priority:** P0 | P1 | P2
**Notes:** [Edge cases, open questions, links to designs]

---

## 9. Out of Scope

*What are we explicitly NOT doing?*

This section prevents scope creep. For each item:

- **[Item]** — [Brief reason why it's out of scope]

Example:
- **Admin bulk operations** — Only 3% of admins manage >50 items; defer until post-launch usage data confirms need.
- **Mobile app support** — Desktop is 92% of target persona usage; mobile deferred to Phase 2.
- **Internationalization** — English-only for MVP; i18n infrastructure will be built but translations deferred.

**Scope-out thesis check.** Before finalizing this section, re-read §1's defect class. Any scope-out that leaves an instance of that defect class intact undermines the PRD — a "consolidate untyped boundaries" feature cannot ship with an untyped boundary on the hot read path. Resolve by (a) pulling the item in scope, or (b) narrowing §1's framing so the PRD isn't overclaiming. Don't scope out instances of your own thesis.

---

## 10. Design & UX Considerations

*What do we know about the experience?*

Don't delegate all UX thinking. Include what you know:

- Key UX principles or constraints for this feature
- Empty states — what does the user see before they've created any data?
- Error states — how should failures be communicated?
- Accessibility requirements (WCAG level, screen reader support, etc.)
- Links to wireframes, prototypes, or mockups if they exist
- Open UX questions that need design exploration

Per Marty Cagan: the majority of a product spec's value often comes from the prototype. Link to one if it exists.

---

## 11. Technical Considerations & Dependencies

*What does engineering need to know?*

This section describes constraints, invariants, FK graphs, atomicity and ordering rules, and dependencies. It does NOT prescribe method shape, file structure, validation-rule composition, or controller-vs-service layering — those are implementer decisions. If the PRD dictates them, the implementer ends up citing "per PRD §11" in code comments and translating prose into brittle literal method signatures. State what must be true; leave how to the implementer.

- Known technical constraints or platform limitations
- Dependencies on other teams, services, or third-party APIs
- Data migration or backfill needs
- Feature flag / rollout strategy
- Performance or scalability concerns (only if validated, not speculative)
- Security or compliance requirements

---

## 12. Risks & Mitigations

*What could go wrong?*

Address four categories of risk (per Cagan):

| Risk Type | Risk | Likelihood | Impact | Mitigation |
|-----------|------|-----------|--------|------------|
| **Value** | Users don't adopt because... | | | |
| **Usability** | Users can't figure out how to... | | | |
| **Feasibility** | Engineering can't build X because... | | | |
| **Business Viability** | This conflicts with... | | | |

---

## 13. Analytics & Instrumentation

*What events do we need to track?*

- Key events and their properties
- Funnels to monitor (e.g., onboarding completion funnel)
- Dashboards needed at launch
- Data retention and privacy considerations

---

## 14. Launch & Rollout Plan

*How do we get this to users?*

- Phasing / feature flag strategy
- Beta / dogfood plan
- Go-to-market considerations (if applicable)
- Support readiness (documentation, training)
- Rollback criteria — under what conditions would we pull this feature?

---

## 15. Open Questions & Decisions Log

*What's unresolved?*

| # | Question | Owner | Status | Decision | Date |
|---|----------|-------|--------|----------|------|
| 1 | [Question] | [Name] | Open/Resolved | [Decision if resolved] | |

---

## 16. Appendix

Links to supporting materials:
- User research / interview notes
- Competitive analysis
- Technical spikes or feasibility assessments
- Historical context or previous attempts
- Related PRDs or RFCs

---

## 17. Author's Consistency Pass (Before Marking Approved)

Run these four checks on the finished PRD. Each catches a specific class of post-implementation churn that satisfies the story ACs but violates the thesis.

1. **Thesis check.** Does any §9 Out-of-Scope item — or any "do not do X as part of this cleanup" AC — leave an instance of the defect class §1 names intact? If yes: pull it in, or narrow §1.
2. **Boundary-graph check.** If §1's problem is drift / type / consistency, does §7b enumerate every hop the value takes and state the type guarantee at each? Untyped passthroughs in unnamed hops are where half-wirings survive.
3. **Discriminating-AC check.** Walk every requirement and AC. *Would this still pass if the function under test returned a constant, a trivial value, or an overly-inclusive result?* Watch for response-shape ACs that check structure but not values, selection predicates keyed on non-authoritative fields (timestamp where id is authoritative), and guardrails where the fixture sets both sides. For guardrails specifically, the expected value must be *constructed* from inputs by the function under test, not asserted against a fixture literal.
4. **Negative-fingerprint check.** For every anti-pattern §1 wants eliminated, is there a grep in §7b's "How to verify" that returns zero matches when the cleanup is complete? Positive-only greps miss survivors in files the stories didn't touch.

Revise until all four pass. In aggregate these four catch most review-time findings that would otherwise force a follow-up PRD.

---

## Template Usage Notes

- **For a focused feature:** Sections 1-9 are usually sufficient. Sections 10-16 can be abbreviated or omitted.
- **For a new product:** All sections are relevant. Sections 5 and 6 become especially important.
- **For a quick enhancement:** Consider a lean one-pager instead: Problem → Goal metric → Scope (in/out) → 3-5 acceptance criteria → Key risks.
- **Mark unknowns clearly.** "[TBD: needs input from @team]" is always better than omitting or guessing.
- **This is a living document.** Date and describe material changes. Don't let it gather dust.
