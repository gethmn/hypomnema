# Changelog

All notable changes to this project will be documented in this file.

## [0.7.0] - 2026-05-02

### Added

- Add search result payload budget proposal by @simensen
- Playbook: add tiered spawn policy and agent-tool selection spec by @simensen
- Docs: add playbook quick commands to AGENTS and README by @simensen
- Add portable workflow scaffold by @simensen
- Add CLAUDE.md by @simensen
- Step 17 · Task 2: Add truncated field to SemanticSearchResponse by @simensen
- Step 17 · Task 3: Add content_hash to semantic results by @simensen
- Task 17.6: Audit and correct content-search `include_matches` default (false per spec) by @simensen

### Maintenance

- chore: update Cargo.lock for v0.5.0 by @simensen
- chore: apply safe dependabot updates (sha2 0.11, checkout v6, upload-artifact v7) (#8) by @Copilot in [#8](https://github.com/gethmn/hypomnema/pull/8)
- maint: Moved roadmap for round 6 by @simensen
- Task 17.8: Verification gate — all shipping criteria confirmed, negative-fingerprint clean, manual smoke pass by @simensen

### Other

- Complete round 7 dependency upgrades by @simensen
## [0.5.0] - 2026-04-30

### Added

- Capture amp code review findings: 3 backlog items + 7 carry-forward notes by @simensen
- Step 13 · Task 13.1: promote CI pipeline proposal to canonical spec by @simensen
- Step 13 · Task 13.2: add .config/nextest.toml with CI profile by @simensen
- Step 13 · Task 13.5: add sqlite-vec download step to CI test job by @simensen
- Step 13 · Task 13.6: boundary verification + roadmap-5 status update by @simensen
- docs: add content-retrieval spec proposal and user stories by @simensen

### Changed

- Document live change event stream by @simensen

### Fixed

- Step 12 boundary: fix ADR-0013 workplan link to archive path by @simensen
- Step 13 · Task 13.5: fix sqlite-vec download (arm64→aarch64, -f flag, tmp file) by @simensen
- fix: correct relative links in archived step-16-workplan after move to archive/ by @simensen

### Infrastructure

- Step 13 · Task 13.3: .github/workflows/ci.yml (SHA-pinned CI workflow) by @simensen
- Step 13: CI pipeline + Dependabot configuration by @simensen in [#1](https://github.com/gethmn/hypomnema/pull/1)

### Maintenance

- maint: Clean up changes that should have been committed earlier by @simensen
- Step 16 · Task 16.1: Canonical docs and contract cleanup by @simensen
- v0.5.0: release with live-only event streaming by @simensen

### Other

- Step 13 · Task 13.4: .github/dependabot.yml (Dependabot config) by @simensen
- Step 13 · Task 13.6: archive step-13 workplan to notes/roadmap/archive/ by @simensen
- Lock Hypomnema visual identity mark by @simensen
- Step 16 · Task 16.2: Event types and live bus by @simensen
- docs: propose FTS5 BM25 content search by @simensen
- docs: propose HYDE semantic search strategy by @simensen
- Step 16 · Task 16.3: Watcher and manager runtime rewire by @simensen
- Step 16 · Task 16.4: HTTP watch endpoint by @simensen
- Step 16 · Task 16.5: CLI hmn vault watch streaming command by @simensen
- Step 16 · Task 16.6: defer MCP streaming, document rmcp 1.5.0 verdict by @simensen
- Step 16 · Boundary: mark shipped 2026-04-30 by @simensen
- docs: append Step 16 + end-of-round-6 retrospectives by @simensen
- docs: archive step-16-workplan with Shipped status by @simensen

### Removed

- Step 16 · Task 16.7: Remove status/config/migration/fixture outbox leftovers by @simensen
## [0.3.0] - 2026-04-28

### Changed

- Step 12 · Task 12.8: reference docs verify + roadmap-4 status update by @simensen

### Other

- Step 12 · Task 12.2: HypomnemaBackend trait + Arc<dyn> server by @simensen
- Step 12 · Task 12.3: InProcessBackend full implementation by @simensen
- Step 12 · Task 12.4: [mcp.http] config + HTTP-MCP route + Origin middleware by @simensen
- Step 12 · Task 12.5: integration tests + 5-story coverage by @simensen
- Step 12 · Task 12.6: manual-testing runbook refresh by @simensen
- Step 12 boundary + round-4 ship: archive workplan + roadmap, retro, v0.3.0 by @simensen

### Testing

- Step 12 · Task 12.1: spec promotion + ADR-0013 + canon sync by @simensen
## [0.2.0] - 2026-04-28

### Added

- notes/backlog.md: add MCP Streamable HTTP transport to agent-host integration by @simensen
- notes/roadmap: relocate from docs/, archive shipped workplans by @simensen
- Step 9 · Task 9.1: vault_registry module + vaults.sqlite schema + CRUD by @simensen
- Step 9 · Task 9.6: search response + outbox serde-shape tests by @simensen
- Step 9 · Task 9.7: integration tests + behavior-preservation gate by @simensen
- Step 9 boundary: mark shipped, archive workplan, retro by @simensen
- Step 10 boundary: mark shipped, archive workplan, retro by @simensen
- Step 11 workplan: remaining lifecycle ops + `hmnd scan` removal by @simensen

### Changed

- Step 9 · Task 9.2: store per-vault refactor by @simensen
- Step 9 · Task 9.4: watcher + outbox per-vault refactor by @simensen
- Step 11 · Task 11.1: VaultRunner interior-mutability + pause/resume/rename by @simensen
- Step 11 · Task 11.4: hmn vault {pause,resume,reset,rename,rescan} CLI + DaemonClient by @simensen

### Fixed

- Step 11 boundary + round-3 ship: archive workplan + roadmap, retro, v0.2.0 by @simensen

### Maintenance

- notes/backlog.md: round-agnostic queue + archive cleanup by @simensen
- Step 9 · Task 9.3: indexer per-vault refactor by @simensen
- Step 10 · Task 10.5: cross-vault search refinements + vaults filter + partial_results by @simensen

### Other

- Step 9 · Task 9.5: daemon startup-sequence rewrite + reconcile + legacy-state migration by @simensen
- Step 9 · Task 9.8: reference docs reflect per-vault layout by @simensen
- Step 10 · Task 10.2: control_plane module + VaultManager by @simensen
- Step 10 · Task 10.3: HTTP control-plane routes by @simensen
- Step 10 · Task 10.4: hmn vault CLI subcommands + DaemonClient by @simensen
- Step 10 · Task 10.6: MCP vault tools + write-tool gating by @simensen
- Step 10 · Task 10.7: vault control-plane integration tests by @simensen
- Step 11 · Task 11.2: reset (with --rebuild) + rescan by @simensen
- Step 11 · Task 11.3: HTTP routes for 5 lifecycle ops by @simensen
- Step 11 · Task 11.5: MCP tools for 5 lifecycle ops (gated by enable_write_tools) by @simensen
- Step 11 · Task 11.7: integration tests for 5 lifecycle ops over HTTP by @simensen
- Step 11 · Task 11.8: reference docs + roadmap-3 status + boundary prep by @simensen

### Removed

- Step 11 · Task 11.6: remove hmnd scan subcommand by @simensen

### Testing

- Step 10 workplan: vault control plane (read + create/terminate) + cross-vault search by @simensen
- Step 10 · Task 10.1: spec amendments + vault-management fleshout by @simensen
- Step 10 · Task 10.8: reference docs + roadmap-3 status by @simensen
## [0.1.0] - 2026-04-27

### Added

- Playbook + workflow-notes: round-2 boundary cleanups by @simensen
- Canon amendment: multi-vault adoption (ADR-0009/0010/0011) by @simensen
- Install explore workflow: negotiate canon conflicts by @simensen
- Note: Mutagen-like multi-vault design exploration by @simensen
- Step 6 boundary: status, workplan amendments, retro by @simensen
- Step 7 · Task 7.1: migration 0004 (chunks_vec cosine) by @simensen
- Add manual testing runbook for steps 1–7 by @simensen
- Clarify change-events spec: consumer model, cold start, size, recovery by @simensen
- Justfile: add idiomatic build/install recipes by @simensen
- Step 8 · Task 8.5: brand-identity override (ADR-0012) by @simensen
- Step 8 boundary (Phase 1): workplan, docs, ADR-0012, retro skeleton by @simensen
- Add support for TLS by @simensen
- Step 8 boundary (Phase 2): retro additions from manual testing + human perspective by @simensen

### Changed

- Step 8 · Task 8.6: manual-testing runbook for MCP (04-mcp.md + README update) by @simensen

### Infrastructure

- Step 7 boundary: status, workplan amendments, retro by @simensen

### Maintenance

- Bump rust-toolchain to 1.88.0 (rmcp 1.5.0 MSRV) by @simensen
- Step 8 · Task 8.1: rmcp dep + JsonSchema derives on src/api/types.rs by @simensen

### Other

- Step 6 — workplan: chunking and embedding by @simensen
- Step 6 · Task 6.1: chunks schema + sqlite-vec extension load by @simensen
- Step 6 · Task 6.2: chunking module (pulldown-cmark) by @simensen
- Step 6 · Task 6.3: embedding client (HTTP, retry, typed errors) by @simensen
- Step 6 · Task 6.4: indexer chunk → embed → write transaction by @simensen
- Step 6 · Task 6.5: embedding health probe + hmnd wiring by @simensen
- Step 6 · Task 6.4r1: reclassify DimensionMismatch as skip-and-log by @simensen
- Step 6 · Task 6.6: integration tests against live daemon + stub embedding service by @simensen
- Step 6 · Task 6.7: reference docs reflect step-6 resolutions by @simensen
- Step 7 · Task 7.2: semantic search query module by @simensen
- Step 7 · Task 7.3: HTTP /search/semantic + ApiState wiring by @simensen
- Step 7 · Task 7.4: hmn search semantic + DaemonClient::search_semantic by @simensen
- Step 7 · Task 7.5: integration tests for /search/semantic + hmn binary by @simensen
- Step 8 · Task 8.2: src/mcp/ HypomnemaMcpServer + tool router by @simensen
- Step 8 · Task 8.3: hmn mcp + non-stdio warn + stderr logging by @simensen
- Step 8 · Task 8.4: integration tests for hmn mcp subprocess by @simensen
- Indexer: log scan progress and per-chunk embed timing by @simensen
- Step 8 boundary (Phase 3): mark step 8 shipped in roadmap-2.md by @simensen

### Testing

- Install spec-generator skill: LDS-aware feature specs by @simensen
- Step 7 · Task 7.6: reference docs + spec amendments by @simensen
## [0-shipping-gate] - 2026-04-26

### Added

- Add nix-direnv flake for reproducible Rust dev environment by @simensen
- Split into two binaries: hmnd (daemon) + hmn (CLI client) by @simensen
- Step 2 · Task 6: integration tests against tempdir vaults by @simensen
- Playbook: resolve idle-detection + orchestrator-separation questions by @simensen
- Vision + roadmap: log multi-vault forward-compat question for step 5 by @simensen

### Changed

- Playbook: name the coordinator the coordinator from spawn time by @simensen
- Step 5 — boundary: workplan, retros, round-2 roadmap by @simensen

### Fixed

- Third round of docs-review cleanups by @simensen
- Fourth round of docs-review cleanups by @simensen

### Infrastructure

- Step 2 — boundary: workplan + roadmap status + retro by @simensen
- Step 3 — boundary: workplan + retro by @simensen
- Playbook: split soft-flag audiences and name forward-note artifact by @simensen

### Maintenance

- Land LDS docs tree with docs-review batch cleanups by @simensen
- Step 1 — Skeleton + planning/orchestration infrastructure by @simensen
- Step 4 — boundary: workplan + retro by @simensen

### Other

- Bootstrap Hypomnema: AGENTS.md, handoff, skills, step-1 deps by @simensen
- Second round of docs-review cleanups by @simensen
- Close guardrail gaps: shutdown and embedding-service pitfalls by @simensen
- Step 2 · Task 1: Lift binary-target tracing into compose_filter by @simensen
- Step 2 · Task 2: globset ignore matcher + .git/** default by @simensen
- Step 2 · Task 3: store module (pool, migrations, smoke open) by @simensen
- Step 2 · Task 4: indexer module (scan + hash + reconcile) by @simensen
- Step 2 · Task 5: wire scan into hmnd (default + scan subcommand) by @simensen
- Step 2 · Task 7: reference docs reflect step-2 resolutions by @simensen
- Coordinator playbook: split orchestrator role from coordinator by @simensen
- Step 3 · Task 1: watcher filter helpers + bump default debounce by @simensen
- Step 3 · Task 2: notify deps + Watcher module by @simensen
- Step 3 · Task 3: indexer single-file reindex_path + remove_path by @simensen
- Step 3 · Task 4: wire watcher into hmnd run_daemon by @simensen
- Step 3 · Task 5: integration tests against tempdir vault by @simensen
- Step 3 · Task 6: reference docs reflect step-3 resolutions by @simensen
- Step 4 · Task 1: ChangeEvent type + serde envelope by @simensen
- Step 4 · Task 2: Outbox writer with per-event sync_data by @simensen
- Step 4 · Task 3: indexer outcomes carry content_hash by @simensen
- Step 4 · Task 4: wire outbox into run_consumer by @simensen
- Step 4 · Task 5: integration tests against tempdir vault + outbox by @simensen
- Step 4 · Task 6: reference docs reflect step-4 resolutions by @simensen
- Step 5 · Task 5.1: schema migration 0002 — content column by @simensen
- Step 5 · Task 5.2: indexer stores body content by @simensen
- Step 5 · Task 5.3: search query module by @simensen
- Step 5 · Task 5.4: HTTP API types, router, handlers by @simensen
- Step 5 · Task 5.5: wire HTTP server into hmnd by @simensen
- Step 5 · Task 5.6: hmn HTTP client + commands by @simensen
- Step 5 · Task 5.7: integration tests against live daemon by @simensen
- Step 5 · Task 5.8: reference docs reflect step-5 resolutions by @simensen

