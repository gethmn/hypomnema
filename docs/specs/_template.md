# {feature_name} Specification

**Version**: {version}
**Date**: {date}
**Status**: {status}

---

> **File Location**: `docs/specs/{feature_slug}.md`
>
> Create appendices directory at `docs/specs/appendices/{feature_slug}/` if large content is needed.

---

## Overview

{overview}

**Related Documents**:
- [ADR-{related_adr_number}: {related_adr_title}](../decisions/{related_adr_number}-{related_adr_slug}.md)
- [Architecture: {related_architecture_section}](../architecture/overview.md#{related_architecture_anchor})

**Appendices** (if applicable):
- [Detailed Examples](./appendices/{feature_slug}/examples.md)
- [Error Catalog](./appendices/{feature_slug}/errors.md)

---

## Behavior

### Normal Flow

{normal_flow_description}

1. {step_1}
2. {step_2}
3. {step_3}

### State Machine

{state_machine_description}

```
┌─────────┐     {trigger_1}      ┌─────────┐
│ {state_a} │─────────────────►│ {state_b} │
└─────────┘                  └────┬────┘
                                  │
                             {trigger_2}
                                  │
                                  ▼
                            ┌─────────┐
                            │ {state_c} │
                            └─────────┘
```

| State | Description | Transitions |
|-------|-------------|-------------|
| {state_a} | {state_a_description} | → {state_b} (on {trigger_1}) |
| {state_b} | {state_b_description} | → {state_c} (on {trigger_2}) |
| {state_c} | {state_c_description} | Terminal |

---

## Data Schema

### {schema_name}

```yaml
{schema_example}
```

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| {field_1} | {field_1_type} | {field_1_required} | {field_1_default} | {field_1_description} |
| {field_2} | {field_2_type} | {field_2_required} | {field_2_default} | {field_2_description} |

### Validation Rules

{validation_rules}

---

## Examples

{examples_overview}

### Example 1: {example_1_name}

**Input**:
```yaml
{example_1_input}
```

**Behavior**: {example_1_behavior}

**Result**: {example_1_result}

### Example 2: {example_2_name}

**Input**:
```yaml
{example_2_input}
```

**Behavior**: {example_2_behavior}

**Result**: {example_2_result}

> **Note**: If code blocks exceed 50 lines or you need complete file examples, move to `appendices/{feature_slug}/examples.md`.

---

## Edge Cases

### {edge_case_1_name}

**Scenario**: {edge_case_1_scenario}

**Behavior**: {edge_case_1_behavior}

**Rationale**: {edge_case_1_rationale}

### {edge_case_2_name}

**Scenario**: {edge_case_2_scenario}

**Behavior**: {edge_case_2_behavior}

---

## Error Handling

{error_handling_overview}

| Error Condition | Error Code/Type | Message | Recovery |
|-----------------|-----------------|---------|----------|
| {error_1_condition} | {error_1_code} | {error_1_message} | {error_1_recovery} |
| {error_2_condition} | {error_2_code} | {error_2_message} | {error_2_recovery} |

> **Note**: If this table exceeds 20 rows, move the full catalog to `appendices/{feature_slug}/errors.md`.

---

## Integration Points

### With {integration_component}

{integration_description}

**Data Flow**:
```
{data_flow_diagram}
```

---

## Implementation Notes

{implementation_notes}

---

## Open Questions

- [ ] {open_question_1}
- [ ] {open_question_2}

---

## Revision History

| Version | Date | Changes |
|---------|------|---------|
| {initial_version} | {initial_date} | Initial draft |
