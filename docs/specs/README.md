# Specifications

Layer 4 of the Layered Documentation System. Specifications detail *how* features work — behaviors, schemas, edge cases, errors. Distinct from *why* (ADRs), *architecture* (component relations), and *reference* (CLI/config lookup tables).

## Documents

| Document | Purpose |
|----------|---------|
| [filesystem-search.md](./filesystem-search.md) | Path/glob queries against the filesystem index |
| [content-search.md](./content-search.md) | Substring/regex queries against the content index |
| [semantic-search.md](./semantic-search.md) | Vector-similarity queries against the chunk index |
| [change-events.md](./change-events.md) | JSONL outbox format and subscription contract |
| [_template.md](./_template.md) | Feature Spec template for new specs |

## Related

- [Decisions](../decisions/) — rationale behind the chosen shapes
- [Architecture Overview](../architecture/overview.md) — how these features compose
- [Reference](../reference/) — lookup-shaped details (CLI flags, config options)
