# Changelog

All notable changes to this project are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

## Step 19 — Content Retrieval (Round 9)

### Added

- Add `content_get` read-only operation for fetching indexed file content by vault-relative path
- HTTP: `POST /content/get`
- MCP: `content_get` tool (stdio + Streamable-HTTP transports)
- CLI: `hmn content get PATH... [--vault ...] [--json]`

## Step 20 — FTS5 / BM25 Ranked Content Search (Round 9)

### Added

- Add third matching strategy `mode: "ranked"` to `search_content` using FTS5/BM25
- HTTP: `mode: "substring" | "regex" | "ranked"` parameter on `POST /search/content`
- Response includes `score` and `rank` fields for ranked-mode results
- MCP: `search_content` tool supports same `mode` parameter on both transports
- CLI: `hmn search content "<query>" --mode ranked`
- Migration 0005: creates external-content FTS5 virtual table `files_fts` per vault with backfill

### Changed

- `search_content` now validates mode/regex conflicts and rejects `case_sensitive: true` with ranked mode
- FTS5 maintenance integrated into indexer upsert/delete/reset paths with transactional atomicity
- Error classification: FTS5 syntax errors return HTTP 400 (invalid_query), not 500
- Ranked-mode results merged deterministically across vaults by `(score asc, path asc, vault_id asc)`

## [0.5.0] - 2026-04-30

### Changed

- Retired the durable JSONL outbox in favor of a live-only in-memory event stream.
- HTTP and CLI `watch` endpoints now stream NDJSON change events with lag detection.
- Removed all outbox configuration, status reporting, and test surfaces.

### Removed

- `src/outbox/` module and associated integration tests (`tests/outbox.rs`).
- Outbox-related config keys and status fields.
- MCP `vault_watch` tool remains deferred pending `rmcp` long-lived streaming support.

## [0.4.0] - 2026-04-29

### Added

- GitHub Actions CI for format, lint, and tests on Ubuntu and macOS.
- Dependabot coverage for Cargo dependencies.
- A repo-root CHANGELOG and the round-boundary ritual that keeps it current.

### Changed

- Deferred the outbox flake hardening work to a later outbox-removal round.

## [0.3.0] - 2026-04-28

### Added

- Streamable HTTP MCP transport on the existing `/mcp` route.
- Origin validation for browser-hosted clients while keeping loopback and no-Origin requests working.
- MCP transport parity with stdio, including the same search and vault-management tool surface.

### Changed

- Updated the architecture and configuration docs to describe the third MCP transport.

## [0.2.0] - 2026-04-28

### Added

- Multi-vault storage, registry, watcher, indexer, store, and outbox plumbing.
- Vault control-plane operations for create, list, status, terminate, pause, resume, reset, rename, and rescan.
- Cross-vault search and `vaults` filtering over HTTP, CLI, and MCP.

### Changed

- Removed the old `hmnd scan` surface in favor of `hmn vault rescan`.

## [0.1.0] - 2026-04-27

### Added

- Markdown chunking and embedding against the local OpenAI-compatible embedding service.
- Semantic search backed by sqlite-vec.
- The stdio MCP wrapper exposing the three search tools.

### Changed

- Completed the round-2 semantic and MCP layer on top of the existing read/search surfaces.

[Unreleased]: https://github.com/gethmn/hypomnema/compare/v0.4.0...HEAD
[0.4.0]: https://github.com/gethmn/hypomnema/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/gethmn/hypomnema/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/gethmn/hypomnema/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/gethmn/hypomnema/releases/tag/v0.1.0
