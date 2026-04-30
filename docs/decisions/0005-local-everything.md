# ADR-0005: Local Everything — No Required Cloud Dependencies

**Status**: accepted
**Date**: 2026-04-23
**Decision-Makers**: Beau Simensen

---

## Context

Hypomnema indexes a personal or team vault of Markdown files. Those files frequently contain notes on unreleased work, customer details, or other content that may not be allowed to leave the local machine or network — this is hard-required in some office deployments and strongly preferred in most others.

The components that most commonly introduce network dependencies are:
- The embedding model (typically called against a hosted API: OpenAI, Voyage, Cohere)
- The vector store (typically a managed service: Pinecone, Weaviate Cloud, qdrant Cloud)
- Logging/telemetry/crash reporting

Each of these has a natural cloud-first default; each has local alternatives of varying maturity.

## Decision

Every component Hypomnema requires to run is local. Specifically:

- **Embedding model**: Local, via a sidecar process — `nomic-embed-text-v1.5` served by TEI (Text Embeddings Inference) is the reference configuration. Any OpenAI-API-shaped endpoint is configurable, so pointing at a hosted model is possible but not required.
- **Vector store**: `sqlite-vec`, one file on disk, loaded as a SQLite extension into the daemon's existing connection pool. No separate process, no network port.
- **Change events**: Live event delivery happens inside the local daemon and over local CLI/HTTP/MCP transports. Durable replay, if added later, must use local daemon-owned storage.
- **Logs and metrics**: Local only; no crash reporting or usage telemetry is sent anywhere.

Running Hypomnema produces no outbound network traffic except to the configured embedding endpoint (which may itself be on localhost).

## Consequences

### Positive

- Passes the "this data doesn't leave the box" requirement by default, without configuration tuning
- Operationally simpler — no API keys, no rate limits, no service-status dependencies
- Reproducible; same vault + same model version = same embeddings, deterministic across machines
- Cost: predictable zero marginal cost per search

### Negative

- Embedding quality is bounded by what a locally-servable model provides (nomic-embed-text-v1.5 is strong, but not at the frontier)
- Users who want a hosted embedding model have to configure it explicitly; no "just works with OpenAI" default
- Local inference has a memory/CPU cost the host has to absorb — TEI with a ~100MB model is modest but not free

### Neutral

- Hosted embeddings remain an escape hatch — the embedding client speaks OpenAI's API shape, so pointing at any compatible endpoint works. This is configuration, not a new code path.

---

## Notes

- Related to [ADR-0003](./0003-indexing-in-the-daemon.md) (indexing in the daemon) and [ADR-0007](./0007-sqlite-vec-over-alternatives.md) (sqlite-vec specifically)
- The embedding dimension (768 for nomic-embed-text-v1.5) is baked into the schema at creation time; model-switching is a re-index, not a runtime switch (see [ADR-0007](./0007-sqlite-vec-over-alternatives.md))

## Amendments

<!-- None yet -->
