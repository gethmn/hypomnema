# Documentation Audit

**For AI Agents**: This document contains instructions for auditing the documentation in this directory. Use this for ongoing documentation health checks.

**Terminology**: See the [Glossary](../DOCUMENTATION-GUIDE.md#glossary) for definitions of `DOCS_DIR` and other terms.

---

## Audit Configuration

- **Documentation Root** (`DOCS_DIR`): `docs/`
- **Install Date**: 2026-04-23

---

# Documentation Audit — Ongoing Health Check

**For AI Agents**: Use this audit process for regular documentation health checks. This works for both fresh installs and post-migration documentation.

---

## Prerequisites

- Documentation system has been installed using `install.md`
- Read the common audit instructions in `common-audit.md` (or the Common Instructions section below)

---

## Ongoing Audit Process

This audit focuses on documentation health regardless of how the documentation was created (fresh or migrated). It does **not** compare against source documentation. (This project has no pre-LDS documentation; skip migration-style audits.)

### Step 1: Verify Structure (Conditional)

Check which directories exist and report their status. Not all layers are required—the system supports partial installations.

```
Directory Inventory:
- [ ] decisions/ — {exists/missing}
- [ ] product/ — {exists/missing}
- [ ] architecture/ — {exists/missing}
- [ ] specs/ — {exists/missing}
- [ ] reference/ — {exists/missing}
- [ ] implementation/ — {exists/missing}
- [ ] DOCUMENTATION-GUIDE.md — {exists/missing}
- [ ] maintenance/ — {exists/missing}
```

For each existing directory, verify:
- [ ] README.md or index file exists
- [ ] Templates are present (if applicable to this layer)

**Note**: Missing layers are not errors—they indicate layers that were not selected during installation.

### Step 2: Verify Templates (For Existing Layers)

For each layer that exists, check for template files:

| Layer | Template | Status |
|-------|----------|--------|
| decisions (if exists) | `0000-template.md` | {found/missing/N/A} |
| specs (if exists) | `_template.md` | {found/missing/N/A} |

### Step 3: Content Completeness Check

For each existing layer, analyze content completeness:

#### 3a: Decisions Layer (if exists)
- Count total ADRs
- Check for ADRs with status "Proposed" (may need resolution)
- Verify ADR numbering sequence is contiguous

#### 3b: Specifications Layer (if exists)
- Count specification files
- Check for stub files (marked as TODO/incomplete)
- Verify appendix links are valid

#### 3c: Reference Layer (if exists)
- Verify all commands/options have examples
- Check for TODO markers
- Verify appendix content is linked from main documents

### Step 4: Cross-Link Validation

Check that cross-references between layers are valid:
- Links to other documents resolve correctly
- No broken internal links
- Relative paths are correct

### Step 5: Run Common Audit

Execute the common audit process to inventory all content and generate the distribution summary.

---

## Report Format

```markdown
# Documentation Health Audit Report

**Date**: {timestamp}
**Documentation Root** (`DOCS_DIR`): {path}

---

## Structure Status

| Directory | Status | Content |
|-----------|--------|---------|
| decisions/ | {exists/missing} | {N} ADRs |
| product/ | {exists/missing} | — |
| architecture/ | {exists/missing} | — |
| specs/ | {exists/missing} | {N} spec files |
| reference/ | {exists/missing} | {N} reference docs |
| implementation/ | {exists/missing} | — |

## Template Status

| Layer | Template | Status |
|-------|----------|--------|
| decisions | 0000-template.md | {found/missing/N/A} |
| specs | _template.md | {found/missing/N/A} |

## Content Summary

{Include common audit summary here — layer distribution, appendix usage, totals}

## Health Status

- Structure: {Complete/Partial — N of M selected layers present}
- Templates: {Complete/Partial/N/A}
- Content Distribution: {Balanced/Skewed toward X}
- Orphaned Content: {None/N items need linking}
- Broken Links: {None/N items need fixing}

## Recommendations

1. {Priority recommendations based on findings}
2. ...
```

---

# Documentation Audit — Common Instructions

**For AI Agents**: This section contains the core audit process shared by all audit types.

---

## Purpose

The audit workflow:
1. Compares documentation content against expectations
2. Calculates content distribution across layers
3. Identifies content that may have been missed or misclassified
4. Reports on documentation completeness

---

## Core Audit Process

### Step 1: Inventory Documentation

Scan the documentation root (`DOCS_DIR`) and categorize all content:

#### 1a: Count All Content

For the entire documentation directory:
- List all Markdown files recursively
- Count total lines per file
- Calculate total lines across all files

```
DOCS_INVENTORY = {
  total_files: N,
  total_lines: N,
  files: [
    { path: "decisions/0001-example.md", lines: N },
    { path: "specs/feature.md", lines: N },
    { path: "specs/appendices/feature/details.md", lines: N },
    ...
  ]
}
```

#### 1b: Categorize by Layer

Group files by their layer:

```
LAYER_INVENTORY = {
  "decisions": {
    documents: N,
    total_lines: N,
    files: [...]
  },
  "product": {
    documents: N,
    total_lines: N,
    files: [...]
  },
  "architecture": { ... },
  "specs": { ... },
  "reference": { ... },
  "implementation": { ... }
}
```

#### 1c: Identify Appendix Content

Separately track appendix content:

```
APPENDIX_INVENTORY = {
  total_files: N,
  total_lines: N,
  by_parent: {
    "specs/appendices/feature-name": { files: N, lines: N },
    "reference/appendices/cli": { files: N, lines: N },
    ...
  }
}
```

---

### Step 2: Analyze Content Distribution

#### 2a: Calculate Layer Distribution

For each layer, calculate:
- Percentage of total documentation
- Main content vs appendix content ratio
- Number of cross-references to other layers

#### 2b: Check for Orphaned Content

Identify files that:
- Are not linked from any index or README
- Don't follow naming conventions
- Appear to be duplicates

---

### Step 3: Generate Summary Report

Create a summary of the documentation state:

```markdown
## Documentation Summary

**Total Content**: {N} files, {M} lines

### Layer Distribution

| Layer | Files | Lines | % of Total |
|-------|-------|-------|------------|
| Decisions | {N} | {M} | {X}% |
| Vision | {N} | {M} | {X}% |
| Architecture | {N} | {M} | {X}% |
| Specifications | {N} | {M} | {X}% |
| Reference | {N} | {M} | {X}% |
| Implementation | {N} | {M} | {X}% |

### Appendix Usage

| Parent Document | Appendix Files | Appendix Lines |
|-----------------|----------------|----------------|
| {path} | {N} | {M} |
| ... | ... | ... |

**Total Appendix Content**: {N} files, {M} lines ({X}% of total)
```

---

## Verification Checklist

After completing the audit:

- [ ] All Markdown files have been read in full
- [ ] Line counts are accurate (not estimated)
- [ ] Layer categorization is complete
- [ ] Appendix content is accounted for
- [ ] Summary report is generated
