# User Story & Acceptance Criteria Guide

Comprehensive reference for writing user stories, acceptance criteria, and organizing them into epics. Read this before generating any user stories.

---

## User Story Fundamentals

### Format

```
As a [specific persona/role], I want to [goal/action] so that [value/benefit].
```

All three parts are mandatory. The "so that" clause is the most important — it explains WHY, which lets the team find better solutions and make trade-off decisions.

### The Three C's

User stories are defined by Card, Conversation, and Confirmation:

1. **Card** — The story itself. Brief enough to fit on an index card. It's a promise for a conversation, not a complete specification.
2. **Conversation** — The discussion between the product owner, developers, and designers that fleshes out the details. The story is an invitation to this conversation.
3. **Confirmation** — The acceptance criteria that define when the story is done.

### What Makes a Good Story

**DO:**
- Use specific personas, not generic "user." "As a warehouse manager" not "As a user."
- Focus on one goal per story. If you see "AND" in the goal, split it.
- State the goal without prescribing the solution. "I want to find relevant reports quickly" not "I want a search bar with autocomplete in the top nav."
- Make the benefit concrete and distinct from the goal. "so that I can make data-driven decisions in my weekly team meeting" not "so that I can find reports" (which just restates the goal).
- Keep language clear, jargon-free, and accessible to all team members.

**DON'T:**
- Use vague adjectives. "Modern experience" — what does that mean? "Fast" — how fast?
- Write stories from the system's perspective. "As the system, I want to send an email" — systems don't want things.
- Include implementation details. "As a user, I want a React modal with a form that POSTs to /api/settings" — that's a task, not a story.
- Write stories you can't validate with users. If the persona wouldn't recognize the goal as something they care about, rewrite.

---

## INVEST Criteria

Every story should be evaluated against INVEST before being considered ready for development:

### I — Independent
The story can be developed without depending on other stories being completed first. If two stories are tightly coupled, consider combining them or restructuring.

**Test:** Can this story be re-ordered in the backlog without breaking anything?

### N — Negotiable
The story is a starting point for conversation, not a rigid contract. The goal and value are fixed, but the details of implementation should be open for discussion with the team.

**Test:** Is the story describing WHAT and WHY, leaving HOW to the team?

### V — Valuable
The story delivers tangible value to a user or the business. Technical tasks ("refactor the database schema") are not user stories — they may be necessary work items, but frame them as enablers, not stories.

**Test:** Would a user or stakeholder recognize this as something worth building?

### E — Estimable
The team has enough information to estimate the effort. If they can't, the story needs refinement or a spike.

**Test:** Can the team give a rough size estimate (even relative) without asking a dozen clarifying questions?

### S — Small
The story can be completed within a single sprint. Stories that span multiple sprints are too large — use SPIDR to split them.

**Rule of thumb:** A story should take no more than 50% of a sprint's capacity. If it's bigger, split it.

### T — Testable
The story has clear acceptance criteria that allow verification of completion. If you can't write a test for it, you can't confirm it's done.

**Test:** Can someone write a pass/fail test for every acceptance criterion?

---

## Acceptance Criteria

Acceptance criteria define "what done looks like" for a specific story. They are NOT the Definition of Done (which applies to all stories and covers process quality like code review, testing, etc.).

### Format Options

Choose the format that best fits the story's complexity:

#### 1. Checklist Format
Best for: simple stories, straightforward requirements, teams new to AC.

```
**Story:** As a customer, I want to filter products by price range so that I can find items within my budget.

**Acceptance Criteria:**
- [ ] Price filter appears on the product listing page
- [ ] User can set minimum and maximum price values
- [ ] Product list updates to show only items within the selected range
- [ ] Filter persists across pagination
- [ ] "Clear filter" option resets to showing all products
- [ ] If no products match the filter, a helpful empty state is shown
```

#### 2. Given/When/Then (Scenario-Based)
Best for: complex behavior, multiple scenarios, BDD teams, stories with important edge cases.

```
**Story:** As a registered user, I want to reset my password so that I can regain access if I forget it.

**Acceptance Criteria:**

Scenario: Requesting a password reset
  Given I am on the login page
  When I click "Forgot Password" and enter my registered email
  Then I receive a reset link within 5 minutes

Scenario: Using a valid reset link
  Given I received a password reset email
  When I click the reset link within 24 hours and enter a new valid password
  Then my password is updated and I can log in with the new password

Scenario: Using an expired reset link
  Given I received a password reset email more than 24 hours ago
  When I click the reset link
  Then I see a message that the link has expired with an option to request a new one

Scenario: Entering an unregistered email
  Given I am on the Forgot Password page
  When I enter an email not associated with any account
  Then I see the same confirmation message (to prevent email enumeration)
```

#### 3. Rule-Based
Best for: business logic, compliance requirements, data validation rules.

```
**Story:** As a finance manager, I want failed payments to trigger alerts so that I can respond quickly.

**Rules:**
- Alert is sent within 60 seconds of a payment failure
- Alert includes: transaction ID, amount, failure reason, customer name
- Alerts go to the assigned account manager AND the finance team channel
- If 3+ failures occur for the same customer within 24 hours, escalate to senior finance
- Failed payment alerts are logged for audit purposes
```

### Writing Good Acceptance Criteria

**DO:**
- Make each criterion independently testable with a clear pass/fail.
- Be specific: "Page loads in under 2 seconds" not "Page loads quickly."
- Cover the happy path AND edge cases (errors, empty states, boundary conditions).
- Include negative criteria where relevant: "User cannot submit the form without a valid email."
- Keep the total number manageable. If you have 15+ criteria, your story is probably too big — split it.

**DON'T:**
- Restate the story as a criterion. The story says "I want to search products"; don't write "User can search products" as an AC.
- Include implementation details. "jQuery validation fires on blur" — that's an engineering decision.
- Write criteria so broad they're untestable. "The feature is user-friendly."
- Forget about accessibility. If relevant, include criteria like "All interactive elements are keyboard-navigable."

### Observability — criteria must be checkable from outside the database

A criterion is only useful if it can be proven by something a user (or an end-to-end test) can directly observe: an HTTP request/response, a UI assertion, a browser test, an integration test that hits a real endpoint and inspects a real response.

If the only proof of a criterion is "a row exists in table X with column Y set," the criterion is satisfied at the wrong layer. The implementer can write code that creates the row while the user-visible behavior is silently broken — and the criterion will still pass.

**Bad (DB-layer):**
- `- [ ] When a user sends a message in a thread, a conversation row is created with thread_id set.`

**Good (observable):**
- `- [ ] When the user sends a message in Thread A, switches to Thread B, then returns to Thread A, the message appears in Thread A and is absent from Thread B.`

The first can pass while the user sees nothing on screen. The second cannot.

When you write a criterion, ask: "what does an external observer SEE when this is true?" That sentence is the criterion.

### Guardrail / no-regression criteria: construct the oracle, don't assert a fixture

When a criterion is of the form *"computation must not change for existing data,"* specify **how the expected value is derived** from the inputs — never just "assert derived equals persisted." A test where the fixture sets both sides of the comparison passes even if the function under test is broken.

**Bad (tautological — fixture sets both sides):**
- `- [ ] Test creates an audit with score='high', attaches items, asserts compute() returns High.`

**Good (constructed oracle — derivation is load-bearing):**
- `- [ ] The factory derives the persisted score by calling compute() over the attached items; the test fails if compute() returns a different value than the construction step used.`

Smoke check before accepting any guardrail AC: *if the function under test returned a constant, would the criterion still pass?* If yes, the criterion is tautological and will not catch the regression it claims to catch.

### Don't prescribe preserving a known anti-pattern

Criteria that explicitly preserve the defect class the PRD exists to eliminate — e.g. "the severity prop continues to use untyped String — do not introduce narrowing as part of this cleanup" — bake the bug back in. If a boundary needs tightening, tighten it. If the tightening is genuinely out of scope, list it in the PRD's Out-of-Scope section with a reason; do not anchor it in an AC.

### Docs references: by symbol, not line number

When a story's output is a doc, write references as `path` plus class / function / constant name — stable across refactors. Line numbers in prose rot on the next unrelated edit. Reserve line numbers for research citations and for machine-checked grep fingerprints.

### Adversarial criteria — required for ownership-scoped resources

Any story that touches an endpoint, query, or resource keyed by `user_id` / `tenant_id` / owner MUST include at least one explicit adversarial criterion proving cross-tenant access is denied.

**Required pattern:**

```
- [ ] Given a [resource] belonging to another user, when the authenticated user submits a request that references it, then the request is rejected with 403/422 and no rows are written.
```

This is non-negotiable. Implementations whose ownership-scoped stories lack a cross-tenant denial criterion should be rejected at review.

A common gap: a Form Request validates `exists:table,id` *without* constraining by `user_id`, and no test exercises the cross-tenant case. The validation passes, the controller trusts the validated id, and any user can write into another user's data. Adversarial criteria force you to think about this attacker model up front.

---

## Organizing Stories into Epics

An epic is a large body of work that can be broken down into stories. Epics should map to user journeys or major capability areas, not technical components.

### Structure

```
Theme: [High-level strategic area]
  └── Epic: [User journey or capability]
        ├── Story 1
        ├── Story 2
        └── Story 3
```

**Example:**
```
Theme: Self-Service Account Management
  └── Epic: Password & Security Management
        ├── Story: Password reset via email
        ├── Story: Enable two-factor authentication
        ├── Story: View login history
        └── Story: Revoke active sessions
  └── Epic: Profile Management
        ├── Story: Update display name and avatar
        ├── Story: Change email address with verification
        └── Story: Delete account with data export
```

---

## SPIDR: Splitting Large Stories

When a story is too large for a sprint, apply one of these five techniques (by Mike Cohn):

### S — Spikes
If the story is large because of unknowns, separate the research from the implementation. Do a time-boxed spike to learn what you need, then write implementation stories based on what you discover.

**Example:** "Investigate Stripe API capabilities for recurring billing" (spike) → then "User can set up monthly recurring payment" (implementation).

### P — Paths
If a user can accomplish the goal through multiple paths, split by path.

**Example:** "User can pay for order" → Split into:
- "User can pay via credit card"
- "User can pay via PayPal"
- "User can pay via Apple Pay"

### I — Interfaces
Split by device type, platform, or interface layer.

**Example:** "User can view dashboard" → Split into:
- "User can view dashboard on desktop browsers"
- "User can view dashboard on mobile (responsive)"
- "User can view dashboard data via API"

### D — Data
Split by restricting the data scope initially, then expanding.

**Example:** "User can generate sales report for any time period" → Split into:
- "User can generate sales report for the current month"
- "User can generate sales report for a custom date range"
- "User can generate sales report with year-over-year comparison"

### R — Rules
Split by simplifying or deferring business rules.

**Example:** "User can purchase tickets with all business rules enforced" → Split into:
- "User can purchase up to 10 tickets" (defer the per-email limit rule)
- "System enforces maximum 5 tickets per email address"
- "System applies group discount for 5+ tickets"

### When to Split

Apply SPIDR when:
- The team estimates the story at more than half a sprint's capacity
- The team can't agree on the estimate (high variance in planning poker)
- The story has more than 8-10 acceptance criteria
- You can see multiple distinct scenarios or paths within one story

---

## Story Quality Checklist

Before considering a story "ready" (meeting Definition of Ready), verify:

- [ ] Follows "As a / I want / So that" format with all three parts
- [ ] Persona is specific (not generic "user")
- [ ] Goal is a single action (no "AND")
- [ ] Benefit is concrete and distinct from the goal
- [ ] No implementation details in the story or acceptance criteria
- [ ] Passes INVEST evaluation
- [ ] Has 3-8 acceptance criteria (if more, consider splitting)
- [ ] Acceptance criteria are testable with clear pass/fail
- [ ] Edge cases and error states are considered
- [ ] Priority is assigned (P0/P1/P2)
- [ ] Dependencies (if any) are identified

---

## Common User Story Anti-Patterns

Watch for and correct these:

1. **The Epic Disguised as a Story:** "As a user, I want a complete onboarding experience so that I can get started." This is an epic — break it down.

2. **The Technical Task as a Story:** "As a developer, I want to refactor the auth module." Not a user story. Frame it as an enabler or technical debt item.

3. **The Solution-Prescribing Story:** "As a user, I want a dropdown menu with alphabetical sorting." What's the actual goal? Probably "I want to find items in a list efficiently."

4. **The Tautological Benefit:** "As a user, I want to search so that I can search for things." The benefit must explain WHY the goal matters, not restate it.

5. **The Kitchen Sink Story:** "As an admin, I want to manage users, set permissions, view audit logs, and configure integrations." Split this into 4+ stories.

6. **The Untestable Story:** "As a user, I want the app to feel intuitive." How do you test "feel"? Rewrite with measurable criteria.

7. **The Missing Persona:** "As a user..." WHICH user? Different users have different needs, constraints, and goals.
