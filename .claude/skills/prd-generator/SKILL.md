---
name: prd-generator
description: Generate Product Requirements Documents (PRDs) with user stories through a structured discovery interview. Use this skill whenever the user mentions PRD, product requirements document, feature spec, product spec, feature requirements, user stories for a feature, writing requirements, or wants to define what to build for a product or feature. Also trigger when someone says things like "I need to spec out a feature", "help me define requirements", "write up what we're building", "I have a feature idea I need to document", "create user stories for X", or "help me write a product brief". Even if they just describe a product idea and want help turning it into something actionable, this skill applies. Use it for both greenfield products and individual features on existing products.
---

# PRD Generator

Generate structured, high-quality Product Requirements Documents with user stories through a conversational discovery process.

## Core Philosophy

A PRD answers **what** and **why** — never **how**. Its purpose is alignment: getting product, engineering, design, and stakeholders on the same page about the problem being solved, who it's for, and what success looks like. A PRD is not a design spec, not a technical architecture doc, and not a project plan. It captures validated decisions, it does not replace validation.

Key principles:

- **Modern PRDs are lean but insightful.** The era of 50-page waterfall PRDs is over. A great PRD for a focused feature might be 2-4 pages. A PRD for a new product might be 6-10. Brevity forces clarity.
- **Content quality beats structural completeness.** A PRD with every section filled but vacuous content (e.g., "Ensuring alignment with legal standards") is worse than a shorter doc with real substance. Don't fill sections with tautologies just to appear thorough.
- **The PRD should excite the team to build**, not just inform them. Use real user pain points, data, competitor gaps — make the reader feel why this matters.
- **User stories are not requirements in disguise.** They are invitations to conversation. The story captures essence; the acceptance criteria define "done."
- **Don't prescribe implementation.** "The button must be blue and positioned 20px from the top right" is a design spec. "User can dismiss the notification" is a requirement.

## Independent Thought

Avoid simply agreeing with the user's points or taking their conclusions at face value. The goal is real intellectual challenge, not just affirmation. When they propose an idea:

- **Question their assumptions.** What are they treating as true that might be questionable?
- **Offer a skeptic's viewpoint.** What objections would a critical, well-informed voice raise?
- **Check their reasoning.** Are there flaws or leaps in logic being overlooked?
- **Suggest alternative angles.** How else might the idea be viewed, interpreted, or challenged?
- **Focus on accuracy over agreement.** If their argument is weak or wrong, correct them plainly and show how.

Stay constructive but rigorous. You're not arguing for argument's sake — you're sharpening the thinking behind the PRD and keeping them honest. If you catch bias or unfounded assumptions, say so plainly.

---

## Process Overview

The process is **research-first**. You are an intelligent collaborator with access to the codebase — use that advantage. Don't ask questions you can answer yourself.

**The flow:**

1. **Immediate Research** — The moment the user provides context, explore the codebase and relevant docs
2. **Grounded Conversation** — Come back with knowledge and have an informed discussion about the problem and scope
3. **Deep Research** — If needed, do targeted exploration based on what you learn
4. **Document Generation** — Produce the PRD and user stories
5. **Refinement** — Iterate based on feedback

The critical insight: **research before you ask, not after.** When someone drops a concept doc or describes a feature, your first move is to grep, read code, find related features. Then you can have a real conversation — "I see X exists, your concept extends it, let me push on the problem framing" — instead of asking questions you could have answered yourself.

---

## Phase 1: Immediate Research

**This happens first, before asking ANY questions.**

When the user provides context — a concept doc, a feature idea, even a vague "I want to improve X" — immediately explore the codebase to understand what exists. Extract keywords from their input and search.

### What to Look For

**From the user's input, extract:**
- Feature names, product areas, or domain terms they mention
- Existing functionality they reference (even implicitly)
- User types, personas, or roles they describe
- Technical terms, model names, or API references

**Then search:**

```
# Example: user mentions "thread context pinning" and "slash commands"
rg -l "thread" --type ts --type tsx
rg -l "context.*pin"
rg -l "slash.*command"
rg -l "prompt" --type ts   # if they mention prompts
find . -type d -name "*thread*" -o -name "*prompt*" -o -name "*skill*"
```

**Read what you find:**
- Source files for related features
- README files in relevant directories
- Existing PRDs, RFCs, or design docs
- Test files (they describe intended behavior clearly)
- Database migrations or schema files
- Configuration or feature flag files

**Look for patterns that inform the conversation:**
- Does the thing they're extending already exist? (Don't ask "does X exist?" if you can grep for it)
- Are there partial implementations or abandoned attempts?
- What's the current user flow for adjacent features?
- What technical patterns does the codebase use in this area?
- What prior art exists internally or in similar tools they reference (e.g., "like Claude Code's slash commands")?

### Time-box This

Spend 2-5 minutes on initial research. You're not trying to understand everything — you're trying to know enough to have an intelligent conversation. You can always do more targeted research later.

### If There's No Codebase

For greenfield projects or when the user is a non-technical PM:
- Skip codebase research
- Do market/domain research instead — use web search for competitors, similar products, industry patterns
- Look for prior art they reference ("like Slack's slash commands")
- Come back with informed questions about differentiation and approach

---

## Phase 2: Grounded Conversation

Now you come back to the user **with context**. This is not a generic interview — it's a collaborative discussion where you bring knowledge to the table.

### Start with What You Found

Open by briefly sharing your research. This builds trust and catches misunderstandings early:

> "I looked through the codebase and found that thread context pinning already exists in `src/features/threads/pinning.ts` — you have a `pinnedResources` array on the Thread model. I also see you have prompts in `.agents/prompts/` and skills in `.agents/skills/`. Your concept extends this with a keyboard-first UX for power users, similar to how Claude Code uses slash commands. Let me push on the problem framing..."

### Then Have a Real Conversation

Don't follow a rigid interview script. Have an intelligent back-and-forth based on what you know. But make sure these questions get answered (in whatever order makes sense):

**The Problem:**
- What's the actual pain? Not "users don't have X" but "users struggle to do Y because Z"
- Who experiences this? Specific roles, not abstractions
- How severe is it? What's the workaround today? (This reveals real requirements)
- Why now? What's changed?

**Watch for solution-first thinking.** If the user describes a feature without articulating the problem, push: "I see the solution you want to build. Help me understand the pain it solves — what happens today that's frustrating?"

**The Scope:**
- What does success look like? Push for measurable outcomes
- What's explicitly out of scope?
- MVP vs. full vision?
- Known constraints (timeline, team size, etc.)?

**The Users:**
- Which personas matter? (1-3, not everyone)
- What do they care about that's different from other users?

**Risks & Open Questions:**
- What could go wrong?
- What's unknown and needs a spike?
- Dependencies on other teams?

### Adapt to the User

- **If they gave a detailed spec:** Validate their assumptions against the code. Come back with things they may have missed — existing partial implementations, technical constraints, patterns to follow.
- **If they gave a vague idea:** Do a quick research scan, state your understanding explicitly, ask them to confirm or correct.
- **If they want to move fast:** Don't force unnecessary rounds. Get enough to write a good PRD, not a perfect one.

### Do More Research If Needed

If the conversation reveals areas you haven't explored, pause and go look:

> "You mentioned this needs to integrate with the notification system. Let me quickly check how that works..."

Then come back with specific findings.

---

## Phase 3: Deep Research (If Needed)

For complex features, you may need a more thorough exploration after the initial conversation. This is especially true when:

- The feature touches multiple systems
- There are significant technical constraints to understand
- You found partial implementations that need investigation
- The user revealed context that changes what you need to look at

### What to Research

Adapt your strategy based on what you've learned:

**Existing Feature Landscape:**
- Code related to the feature area
- Current data models, API endpoints, UI components, business logic
- Related features the new one integrates with, extends, or replaces
- Configuration files, feature flags, environment-based toggles

**Documentation & Specs:**
- README files, docs/ directories, wiki links
- Existing PRDs, RFCs, ADRs
- API documentation (OpenAPI/Swagger, GraphQL schemas)
- CHANGELOG or release notes

**Data & Schema:**
- Database migrations, schema files, ORM models
- Seed data or fixtures
- Analytics/tracking code

**Test Files:**
- Unit and integration tests (often describe behavior better than code)
- E2E tests that walk through user journeys
- Test fixtures revealing edge cases

**Architecture & Dependencies:**
- Tech stack, frameworks, key libraries
- Service boundaries
- Third-party integrations
- Rate limits, quotas, infrastructure constraints

### Harmony Assessment (The Beck Principle)

Kent Beck: *"For each desired change, make the change easy (warning: this may be hard), then make the easy change."*

Assess: **Can the current codebase absorb this feature naturally, or does the structure need to change first?**

Look for:
- **Patterns to follow** — existing conventions the new code should mirror
- **Friction points** — places where the architecture fights what the feature needs
- **Load-bearing assumptions** — code that assumes the feature doesn't exist
- **Partial implementations** — existing code that attempted something similar

Classify what you find:

1. **Minor alignment** — Small patterns to follow, no scope change needed
2. **Preparatory refactoring (in-scope)** — "Making the change easy" work that should be explicitly scoped
3. **Foundation work (separate PRD)** — Restructuring significant enough to justify its own PRD

Surface this assessment explicitly — the user needs to make an informed decision about scope.

### Summarize What You Found

Before writing the PRD, organize your findings:

1. **Current state:** What exists today? What's the user journey now?
2. **Technical context:** Tech stack, patterns, constraints
3. **Opportunities:** Partial implementations, related features to extend, tech debt
4. **Questions raised:** Ambiguities, conflicting patterns, gaps between code and user description
5. **Entity surface area:** For features that mutate data, list everything affected:
   - Persistence layer: columns, child relations, pivots, inverse references
   - Type/domain layer: models, DTOs, enums (and all enum values!), validators
   - Non-DB state: cache keys, search indexes, queued jobs, external system records
6. **Harmony assessment:** Minor alignment, preparatory refactoring, or foundation work?

### Resolve ALL Open Questions

**Do NOT proceed to document generation until every question is answered.** No "[TBD]" placeholders that depend on user input. If you have gaps after a round of questions, ask another round.

The only acceptable "[TBD]" items are those requiring input from someone other than the user (e.g., "[TBD: @Legal to confirm retention policy]").

---

## Phase 4: Document Generation

Once you have enough context, generate the PRD. Read `references/prd-template.md` for the full template structure, and `references/user-story-guide.md` for user story guidance.

### Choosing the Right Scope

- **Product PRD** (new product or major initiative): Higher-level. Focus on opportunity, target users/use cases, high-level requirements.
- **Feature PRD** (feature on existing product): More detailed. Specific user journeys, detailed requirements, acceptance criteria.

### Global Invariants

Before writing stories, identify **Global Invariants** — load-bearing, cross-cutting rules every story must uphold. Security boundaries, mandatory payload fields, deprecated paths, data isolation rules. Declare them in a dedicated `## Global Invariants` section.

### Writing the PRD

Follow the template in `references/prd-template.md`. Key rules:

1. **Start with the problem.** The first thing anyone reads should be why this matters.
2. **Ground claims in evidence.** Cite sources — interview quotes, analytics, support tickets. Note assumptions that need validation.
3. **Be specific in requirements.** "Fast page load" → "Page loads in under 2 seconds on 3G."
4. **Prioritize ruthlessly.** P0 = required for MVP. P1 = high-value for min-delightful. P2 = nice-to-have.
5. **Bucket by use case**, not technical component.
6. **Explicit "Out of Scope" section.** This prevents scope creep more than anything else.
7. **Don't delegate thinking.** Capture UX considerations even if rough.
8. **Mark unknowns.** "[TBD: @Legal to confirm]" is better than pretending you know.
9. **Acceptance criteria must be observable.** Checkable from outside the database — HTTP requests, UI assertions, browser tests. Not "a row exists in table X."
10. **Stories touching ownership-scoped resources MUST include adversarial criteria.** Cross-tenant denial tests are non-negotiable.

### Writing User Stories

Read `references/user-story-guide.md`. Key principles:

- **Format:** "As a [specific persona], I want to [goal] so that [value/benefit]."
- **The "so that" clause is not optional.** It explains WHY.
- **One story = one goal.** If you see "AND", split it.
- **Don't prescribe solutions.** "Search by keyword" not "search bar in top navigation."
- **Apply INVEST:** Independent, Negotiable, Valuable, Estimable, Small, Testable.
- **Organize stories into epics** that map to user journeys.

### Save the Output

Save as `{project root}/prds/{prd-name}.md`. Create `prds/` if needed. Confirm before overwriting existing files.

---

## Phase 4.5: Post-Draft Consistency Pass

**MANDATORY before presenting to the user.** Run these checks:

1. **Thesis check.** Does any Out-of-Scope item leave an instance of the problem class intact? If yes: pull it in or narrow the thesis.
2. **Boundary-graph check.** If the problem is drift/type/consistency, enumerate every hop the value crosses with type guarantees.
3. **Discriminating-AC check.** Would each criterion still pass if the function returned a constant? Watch for structure-only checks that miss values.
4. **Negative-fingerprint check.** For anti-patterns to eliminate, is there a grep that returns zero matches when cleanup is complete?
5. **Entity surface check.** Does every item from the entity surface area have an explicit decision?

Edit the PRD until all pass.

---

## Phase 5: Refinement

After presenting the draft:

1. Ask for feedback on each major section — don't just ask "does this look good?"
2. Challenge constructively: Are success metrics measurable? Is scope realistic? Missing edge cases?
3. Iterate until the user is satisfied.
4. Apply INVEST criteria to new user stories discovered during refinement.

---

## Anti-Patterns to Watch For

Flag these when you see them:

1. **Solution-first thinking.** Feature without articulated problem. Push for "why" before "what."
2. **Vacuous content.** Tautologies like "ensure alignment with standards." Be specific or mark TBD.
3. **Missing the other side.** Good PRDs address trade-offs.
4. **No evidence.** Note what validation is missing.
5. **Excessive delegation.** "Design: TBD" for every UX question means the PM hasn't thought it through.
6. **Scope creep via "phases."** Don't use more than 2-3 phases.
7. **Requirements that are design specs.** "Blue button, 20px from top right" → "User can perform [action]."
8. **Confusing PRD with project plan.** Sprint schedules don't belong here.

---

## Reference Files

Read these before generating any PRD:

- `references/prd-template.md` — The PRD template with all sections
- `references/user-story-guide.md` — Guide for user stories and acceptance criteria
