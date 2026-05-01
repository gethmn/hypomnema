# Flaky Tests

Append-only tracker for tests that have flaked or looked flaky in local runs. Add a new row any time a test fails once and then passes on rerun, or when a known flake shows up again with new context.

| Date | Test | Symptom | Setup / hints | Follow-up |
|---|---|---|---|---|
| 2026-05-01 | `tests/embedding.rs::chunks_vec_row_per_chunks_row` | Failed once in a full `cargo test` run with `expected ≥3 chunks across both files, got 1`. | The daemon was booted against a temp vault plus a stub embedding service; the same test passed immediately when rerun in isolation, and the subsequent full suite was green. | Track as a recurrence candidate; if it shows up again, capture whether the timing window or daemon startup order changed. |
