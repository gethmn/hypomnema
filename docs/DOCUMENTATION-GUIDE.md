# Hypomnema Documentation Guide

This guide documents the Hypomnema project's installed Layered Documentation System (LDS) configuration. It is the entry point for anyone — human or AI agent — adding to or maintaining Hypomnema's documentation.

**Installed**: 2026-04-23
**LDS Tooling Tier**: Tier 2 (Recommended)

---

## Quick Start

### For AI Agents

1. Read [`AGENTS.md`](../AGENTS.md) first — it has the project-level orientation
2. Read [`hypomnema-handoff.md`](./hypomnema-handoff.md) — origin story and design-space context (kept in place as historical context)
3. Use the decision tree in this guide to place new content in the right layer
4. Consult the appropriate skill under `.claude/skills/` when editing a file that touches a named subsystem

### For Humans

- **Adding a decision?** → [`decisions/`](./decisions/) — start with [`_adr-policy.md`](./decisions/_adr-policy.md)
- **Documenting a feature?** → [`specs/`](./specs/) — copy [`_template.md`](./specs/_template.md)
- **Documenting a command/config option?** → [`reference/`](./reference/)
- **Changing the architecture?** → [`architecture/overview.md`](./architecture/overview.md)
- **Changing the tech stack?** → [`implementation/tech-stack.md`](./implementation/tech-stack.md)
- **Running a documentation audit?** → [`maintenance/audit.md`](./maintenance/audit.md)
- **Negotiating a proposed change against existing canon?** → [`maintenance/explore.md`](./maintenance/explore.md)

---

## Glossary

### Directory Terminology

| Term | Definition |
|------|------------|
| `DOCS_DIR` | The project documentation root — `docs/` in this repo |
| `LDS_DIST_DIR` | The LDS distribution used to install this setup (lives outside the repo) |

### Core System Terms

| Term | Definition |
|------|------------|
| **Layered Documentation System (LDS)** | The documentation framework organizing docs into seven distinct layers with defined purposes and lifecycles |
| **Layer** | A category of documentation with a specific purpose, stability, audience, and lifecycle |
| **Canonical Source** | The single authoritative location for a piece of information |
| **Single Source of Truth (SSOT)** | The principle that each piece of information lives in exactly one place |

### Document Structure Terms

| Term | Definition |
|------|------------|
| **Appendix** | Supplementary file for large content (code, error catalogs, scripts) that exceeds thresholds; stored in `appendices/{topic}/` alongside the main document |
| **Main Document** | The primary document for a topic, with links to any appendices |
| **Cross-Layer Link** | A relative Markdown link connecting documents in different layers |

### Canonical Layer Names

| # | Canonical Name | Directory | Status in this repo |
|---|---------------|-----------|---------------------|
| 1 | **Decisions** | `decisions/` | Installed (MADR Minimal template, 8 ADRs) |
| 2 | **Vision** | `product/` | Installed (Lean PRD template) |
| 3 | **Architecture** | `architecture/` | Installed (C4-Lite template) |
| 4 | **Specifications** | `specs/` | Installed (Feature Spec template, 4 specs) |
| 5 | **Reference** | `reference/` | Installed (CLI + Config templates) |
| 6 | **Behaviors** | `features/` | **Not installed** — revisit when Gherkin is useful |
| 7 | **Implementation** | `implementation/` | Installed (Tech Stack template) |

Use canonical names in prose; use directory names in file paths.

### Layer-Specific Terms

| Term | Definition |
|------|------------|
| **ADR (Architectural Decision Record)** | An immutable Layer 1 document capturing a significant decision with context, decision, and consequences |
| **MADR (Markdown Any Decision Records)** | The template format used for this project's ADRs — minimal variant |
| **PRD-Lite** | The Layer 2 document style: problem, vision, core concepts, current product boundaries, completion record |
| **Gherkin** | A DSL for Layer 6 executable specifications (Given/When/Then). Not used here today. |

### Authority Order (for conflict resolution)

When documents conflict, higher-authority documents win:

1. **ADRs** (highest)
2. **Vision**
3. **Architecture**
4. **Specifications**
5. **Reference**
6. **Behaviors** (not installed)
7. **Implementation** (lowest)

### Hypomnema-Specific Terms

See the [Vision glossary](./product/vision.md#glossary) for Hypomnema's domain terminology (vault, chunk, consumer, event bus, etc.).

---

## Content Thresholds (Configurable)

These thresholds control when content should move from a main document to an appendix. Edit to customize.

| Threshold | Value | Purpose |
|-----------|-------|---------|
| `CODE_BLOCK_LINES` | 50 | Code blocks ≥ this go to appendix |
| `STEP_LIST_ITEMS` | 10 | Step lists ≥ this go to appendix |
| `TABLE_ROWS` | 20 | Tables ≥ this go to appendix |
| `EXAMPLE_FILE_ALWAYS_APPENDIX` | true | Complete file examples → appendix |
| `ERROR_CATALOG_ALWAYS_APPENDIX` | true | Error catalogs → appendix |
| `SHELL_SCRIPT_ALWAYS_APPENDIX` | true | Shell scripts → appendix |

**Exception**: ADRs (Layer 1) include code inline even when above threshold. ADRs are single-file decision records; splitting them across appendices defeats their archival purpose.

---

## Layer Overview

### Layer 1: Decisions (`decisions/`)

**Purpose**: Capture *why* a significant technical or product choice was made.

**Characteristics**: Immutable once accepted. New information arrives as Amendments or in a superseding ADR. Single file per decision.

**Template**: MADR Minimal — Status, Date, Context, Decision, Consequences.

### Layer 2: Vision (`product/`)

**Purpose**: Define *what* the product is and *why* it exists. Stable; rarely changes.

**Template**: Lean PRD — Problem, Vision, Core Concepts, Current Product Boundaries, Completion Record.

### Layer 3: Architecture (`architecture/`)

**Purpose**: Show *how* components relate. Evolves with design.

**Template**: C4-Lite — System Context, Containers, Communication Patterns, Quality Attributes, Risks.

### Layer 4: Specifications (`specs/`)

**Purpose**: Detail *how* features work. Living; evolves with implementation.

**Template**: Feature Spec — Overview, Behavior, Data Schema, Edge Cases, Open Questions.

### Layer 5: Reference (`reference/`)

**Purpose**: Lookup-shaped information (CLI commands, config options, error codes).

**Templates**: CLI Reference + Configuration Reference.

### Layer 7: Implementation (`implementation/`)

**Purpose**: *How to build* the system. Short-lived; absorbed into code and tests as work progresses.

**Template**: Tech Stack — Dependencies, Architecture Patterns, Project Structure, Implementation Priority.

---

## Classification Decision Tree

When placing new content, work top-down. Stop at the first match.

```
Is this explaining WHY a choice was made?
├─ YES → Layer 1: Decisions (ADR)
└─ NO ↓

Is this about product vision, goals, or core concepts?
├─ YES → Layer 2: Vision
└─ NO ↓

Is this about how components fit together?
├─ YES → Layer 3: Architecture
└─ NO ↓

Is this detailing HOW a feature behaves?
├─ YES → Layer 4: Specifications
└─ NO ↓

Is this a lookup table (commands, options, errors)?
├─ YES → Layer 5: Reference
└─ NO ↓

Is this implementation scaffolding or stack choice?
├─ YES → Layer 7: Implementation
└─ NO → May not need documentation (or belongs in a code comment / AGENTS.md)
```

---

## Classification Heuristics

For each layer, phrases that signal content belongs there:

### Decisions (ADR)
- "We chose X because..." / "We decided to..."
- "Unlike typical approaches, we..."
- Trade-off discussions, technology choices, architectural decisions

### Vision
- "The product provides..." / "What this does NOT do..."
- "Core concepts:" / "Non-goals:" / "Success criteria:"

### Architecture
- "X talks to Y via..." / "The system has the following containers..."
- Quality attributes, cross-cutting concerns, known risks

### Specifications
- "When X happens, the system does Y..."
- Input/output schemas, edge cases, error conditions

### Reference
- Command syntax, flag tables, option descriptions
- Exit codes, error codes, environment variables

### Implementation
- "Install package X version Y..." / "The project structure is..."
- Dependency rationale, scaffolding scripts, build order

---

## Cross-Layer Linking

| From | To | Purpose |
|------|----|---------|
| Specification | ADR | Explain "why" for design choices |
| Specification | Reference | Point to detailed syntax / options |
| Architecture | ADR | Justify architectural patterns |
| Architecture | Specification | Deep-dive into component behavior |
| Reference | Specification | Provide conceptual context |
| Implementation | ADR | Explain technology choices |
| Implementation | Specification | Reference what's being implemented |

Add links in a `## Related Documents` or `## See Also` section at the bottom of the document.

---

## Directory Structure

```
docs/
├── DOCUMENTATION-GUIDE.md          # This file
├── hypomnema-handoff.md            # Origin context (kept in place)
├── .lds-manifest.yaml              # LDS install tracking (for upgrades)
│
├── maintenance/                    # Documentation maintenance workflows
│   ├── audit.md                    # Documentation health audit
│   ├── explore.md                  # Negotiate a proposed change against canon
│   ├── refine.md                   # Quality improvements
│   ├── sync.md                     # Audit docs against implementation
│   └── update.md                   # Update docs after code changes
│
├── decisions/                      # Layer 1: ADRs
│   ├── README.md                   # ADR index
│   ├── 0000-template.md            # Template
│   ├── _adr-policy.md              # Amend / supersede / extend policy
│   └── NNNN-*.md                   # Individual ADRs
│
├── product/                        # Layer 2: Vision
│   ├── README.md
│   └── vision.md
│
├── architecture/                   # Layer 3: Architecture
│   ├── README.md
│   └── overview.md
│
├── specs/                          # Layer 4: Specifications
│   ├── README.md
│   ├── _template.md                # Feature spec template
│   └── *.md                        # Individual specs
│
├── reference/                      # Layer 5: Reference
│   ├── README.md
│   ├── cli.md
│   └── configuration.md
│
└── implementation/                 # Layer 7: Implementation
    ├── README.md
    ├── tech-stack.md
    └── appendices/
        └── tech-stack/
            └── pitfalls.md
```

---

## Appendix Guidelines

Appendices live in `{layer}/appendices/{topic}/`, where `{topic}` matches the main document's basename.

- Create an appendix when content exceeds a threshold above
- Reference the appendix from the main document
- Include a back-link in the appendix: `> **Parent**: [Main Document](../../{topic}.md)`
- Keep the main document scannable; push detail to the appendix

---

## Tooling (Tier 2: Recommended)

This install targets Tier 2 tooling. Add tools as the project needs them.

| Tool | Purpose | Status |
|------|---------|--------|
| **markdownlint** | Format consistency (Tier 1 baseline) | Recommended; not yet configured |
| **Git + PRs** | Version control, review (Tier 1 baseline) | In use |
| **Vale** | Terminology consistency and style-guide enforcement | Recommended; not yet configured — consider adding a `.vale.ini` pointing at this glossary |
| **Log4brains** | ADR static-site generation | Recommended; defer until there are 15+ ADRs |
| **Structurizr** | C4 diagram generation from DSL | Recommended; start with ASCII diagrams (see `architecture/overview.md`); upgrade to Structurizr when diagrams become hard to keep in sync |

Tier 3 tools (Cucumber, link checker, custom generators) are deferred; revisit if the docs outgrow manual review.

---

## Maintenance Workflows

Workflows in `maintenance/` are installed to be executed by AI agents. Each is a step-by-step procedure.

| Workflow | When to Run |
|----------|-------------|
| [`audit.md`](./maintenance/audit.md) | Periodic health checks — structure, templates, cross-links |
| [`sync.md`](./maintenance/sync.md) | Before releases, after significant refactoring — audit docs against current code |
| [`update.md`](./maintenance/update.md) | After a code change that affects documented behavior |
| [`refine.md`](./maintenance/refine.md) | Quality improvements, consolidation, clarity passes |
| [`explore.md`](./maintenance/explore.md) | A proposed change conflicts with vision / ADR / spec / architecture canon — negotiate trade-offs and produce canon edits |

---

## Relationship to `AGENTS.md` and `.claude/skills/`

- **`AGENTS.md`** is the always-loaded project-level orientation. It says "read this first" and points into `docs/`. This guide is the next layer of detail.
- **`.claude/skills/`** contains pattern-specific guidance (rusqlite-in-async, sqlite-vec-extension, filesystem-watching, markdown-chunking) that is loaded automatically when relevant. Those skills are the technical specifics that back the claims made in the ADRs and the implementation layer.

This guide documents the *documentation system*. `AGENTS.md` documents the *project*. `.claude/skills/` documents the *subsystems*. All three are complementary.

---

## Notes & Proposals (outside LDS)

Long-lived process artifacts and in-flight planning material live in [`../notes/`](../notes/), not under `docs/`. LDS is built for steady-state documentation of what the system *is*; the front of the planning funnel is intentionally outside it. The current `notes/` contents:

- [`../notes/project-planning-workflow-notes.md`](../notes/project-planning-workflow-notes.md) — the planning-workflow description (roadmap → workplan → build cadence)
- [`../notes/lds-evaluation.md`](../notes/lds-evaluation.md) — gaps observed in LDS for forward-looking planning
- [`../notes/proposals/`](../notes/proposals/) — in-progress proposals (concept notes, draft specs, working drafts under review)
- [`../notes/proposals/archive/`](../notes/proposals/archive/) — frozen records of approved-and-decomposed proposals; LDS layers are canonical after decomposition
