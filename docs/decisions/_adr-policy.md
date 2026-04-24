# ADR Change Policy

This document defines when and how to modify Architecture Decision Records (ADRs).

---

## Status Values

| Status | Meaning |
|--------|---------|
| `proposed` | Under consideration, not yet accepted |
| `accepted` | Approved and in effect |
| `rejected` | Considered but not chosen |
| `deprecated` | No longer recommended (may still be in use) |
| `superseded by [ADR-XXXX](link)` | Replaced by another ADR |

---

## When to Amend an ADR

**Amend** the existing ADR when the core decision remains valid but needs updates.

Use amendments for:
- **Clarifications**: Adding context that helps future readers understand the decision
- **Implementation learnings**: Recording what worked or didn't work in practice
- **External changes**: Vendor pricing updates, API changes, license modifications
- **New information**: Insights from new team members or evolving requirements

### Amendment Format

```markdown
## Amendments

### 2025-03-15 - Clarification on edge cases

Based on 6 months of production use, we're adding guidance for handling
concurrent requests. The original decision to use optimistic locking still
holds, but we've found that retry logic with exponential backoff is essential
for high-contention scenarios.
```

### Key Principle

If you're adding information that **supports or refines** the original decision, amend it.

---

## When to Supersede an ADR

**Supersede** when the original decision is being replaced entirely.

Create a new ADR that supersedes when:
- **Reversing the decision**: The team has decided to go a different direction
- **Decision proved wrong**: Real-world results showed the choice was incorrect
- **Context fundamentally changed**: The original rationale no longer applies

### How to Supersede

1. Create a new ADR with its own number
2. In the new ADR's context, explain why the previous decision is being replaced
3. Update the old ADR's status to: `superseded by [ADR-XXXX](link)`
4. In the new ADR, reference: `Supersedes [ADR-YYYY](link)`

### Example

**Old ADR (ADR-0005):**
```markdown
**Status**: superseded by [ADR-0012](./0012-switch-to-postgres.md)
```

**New ADR (ADR-0012):**
```markdown
**Status**: accepted

## Context

This decision supersedes [ADR-0005](./0005-use-mysql.md).

After 18 months with MySQL, we've encountered significant limitations with
JSON querying and the need for better support for complex transactions...
```

---

## When to Extend an ADR

**Extend** when expanding the scope of an existing decision without invalidating it.

Create a new ADR that extends when:
- **Adding new aspects**: The original decision applies to a new domain
- **Expanding scope**: Applying the same pattern to additional use cases
- **Building upon**: The new decision depends on and reinforces the original

### How to Extend

1. Create a new ADR
2. Reference the original: `Extends [ADR-XXXX](link)`
3. The original ADR remains `accepted` (no status change needed)
4. Optionally add to the original's Notes: `Extended by [ADR-YYYY](link)`

### Example

**Original ADR-0003** decided to use React for the web frontend.

**New ADR-0015** extends this to mobile:
```markdown
## Context

This decision extends [ADR-0003](./0003-use-react-for-frontend.md) to mobile platforms.

Given our success with React on web and the team's expertise, we're adopting
React Native for our mobile applications...
```

---

## Decision Tree

```
Is the core decision still valid?
├── YES → Is new information available?
│         ├── YES → AMEND the existing ADR
│         └── NO → No action needed
│
└── NO → Are you replacing or expanding?
          ├── REPLACING → SUPERSEDE with new ADR
          └── EXPANDING → EXTEND with new ADR
```

---

## Git History as Backup

Remember that ADRs stored in git have full version history. Even with amendments, you can always see what the ADR said at any point in time via `git log -p`.

This means:
- Amendments don't lose history
- You can be pragmatic about in-place updates
- The audit trail is preserved in version control
