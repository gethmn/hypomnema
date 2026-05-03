# Health Endpoint Specification

**Version**: v0.5.0
**Date**: 2026-05-02
**Status**: Shipped (Step 21)

---

## Overview

`GET /health` is an HTTP readiness probe for orchestration layers — Kubernetes readiness checks, systemd external checks, and similar consumers that want a single boolean-shaped call to determine whether the daemon is ready to serve traffic.

The endpoint is **HTTP-only** and **read-only**. It is not exposed as an MCP tool (health probes are an HTTP idiom). It is not gated by `[mcp] enable_write_tools`.

**Related Documents**:
- `src/api/health.rs` — handler implementation
- `docs/specs/vault-management.md` — per-vault detail (use `/status` instead)

---

## Behavior

### Normal Flow

1. Caller sends `GET /health` with no body.
2. Daemon collects signals: watcher liveness, DB connectivity, vault counts, embedding reachability (conditional).
3. Daemon applies the signal ladder (see below) to derive `status`.
4. Response is returned as JSON with HTTP status code `200` (healthy) or `503` (degraded/unhealthy).

### Signal Ladder

Signals are evaluated top-to-bottom; the highest-precedence matching signal wins.

| Signal | Condition | Status | HTTP |
|--------|-----------|--------|------|
| Watcher crashed | Any active vault's consumer task has exited unexpectedly | `unhealthy` | 503 |
| DB unreachable | `SELECT 1` fails on any active vault's per-vault store | `unhealthy` | 503 |
| Errored vault | Any vault row has status `errored` | `degraded` | 503 |
| Embedding unreachable | Embedding endpoint configured AND probe fails | `degraded` | 503 |
| (none of the above) | — | `healthy` | 200 |

**`unhealthy`** signals that the daemon cannot serve traffic at all. Orchestrators may use this as a liveness-probe failure.

**`degraded`** signals that some operations are impaired (semantic search may fail, or an errored vault is not being indexed), but filesystem and content search remain available. Orchestrators should use `degraded` as a readiness-probe failure without triggering a restart.

---

## Data Schema

### Response Body

```json
{
  "status": "healthy",
  "vaults_active": 2,
  "vaults_errored": 0,
  "uptime_seconds": 3600,
  "embedding": {
    "status": "healthy",
    "endpoint": "http://127.0.0.1:8080/v1/embeddings"
  }
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `status` | string | yes | `"healthy"`, `"degraded"`, or `"unhealthy"` |
| `vaults_active` | integer | yes | Number of vaults with status `active` |
| `vaults_errored` | integer | yes | Number of vaults with status `errored` |
| `uptime_seconds` | integer | yes | Seconds elapsed since daemon start |
| `embedding` | object\|null | no | Present only when an embedding endpoint is configured; omitted from the wire when absent |
| `embedding.status` | string | (when present) | `"healthy"` (endpoint reachable) or `"degraded"` (endpoint unreachable) |
| `embedding.endpoint` | string | (when present) | Configured embedding endpoint URL; no credentials in the URL |

### Status Codes

| HTTP | Meaning |
|------|---------|
| `200 OK` | `status == "healthy"` |
| `503 Service Unavailable` | `status == "degraded"` or `status == "unhealthy"` |

---

## Embedding Probe Semantics

The embedding probe runs **only when** `embedding_endpoint` is set in the daemon's runtime state (i.e., the daemon was started with an embedding endpoint configured). If embeddings are not configured, the `embedding` field is absent from the response and this signal is not evaluated.

The probe is a single `HEAD` request to the configured endpoint with a **500ms hard timeout**. Any HTTP response (including 4xx or 5xx) counts as "reachable" — the probe distinguishes connectivity failures (connection refused, timeout) from service errors. No retries, no backoff.

The result is **not cached**. Every `/health` call probes fresh. If this proves expensive in production (orchestrators probing every few seconds), a TTL cache is a future-round addition.

---

## Examples

### Example 1: Healthy daemon with embeddings

**Request**: `GET /health`

**Response** (`200 OK`):
```json
{
  "status": "healthy",
  "vaults_active": 1,
  "vaults_errored": 0,
  "uptime_seconds": 120,
  "embedding": {
    "status": "healthy",
    "endpoint": "http://127.0.0.1:8080/v1/embeddings"
  }
}
```

### Example 2: Degraded — errored vault

**Response** (`503 Service Unavailable`):
```json
{
  "status": "degraded",
  "vaults_active": 1,
  "vaults_errored": 1,
  "uptime_seconds": 45
}
```

### Example 3: Degraded — embedding endpoint down

**Response** (`503 Service Unavailable`):
```json
{
  "status": "degraded",
  "vaults_active": 1,
  "vaults_errored": 0,
  "uptime_seconds": 10,
  "embedding": {
    "status": "degraded",
    "endpoint": "http://127.0.0.1:8080/v1/embeddings"
  }
}
```

---

## Manual Testing Fixture

### Uptime increments

1. Start `hmnd`: `cargo run --bin hmnd`
2. `curl -s http://127.0.0.1:7777/health | jq .uptime_seconds`
3. Wait 2 seconds.
4. `curl -s http://127.0.0.1:7777/health | jq .uptime_seconds`
5. Confirm second value is ≥ first value + 2.

### Degraded — errored vault

1. Register a vault pointing to a nonexistent path:
   `curl -s -X POST http://127.0.0.1:7777/vaults -H 'content-type: application/json' -d '{"path":"/does/not/exist","name":"broken"}'`
2. `curl -s http://127.0.0.1:7777/health | jq .`
3. Confirm `status == "degraded"` and `vaults_errored == 1`.

---

## Out of Scope

- **Per-vault detail in `/health`**: use `GET /status` for per-vault breakdown. `/health` is intentionally summary-only so orchestration layers see a single boolean readiness signal.
- **MCP tool surface**: `/health` is HTTP-only. Health probes are an HTTP idiom, not an MCP tool.
- **`/metrics` endpoint**: Prometheus-style metrics are a separate future round (explicitly out of scope for v0.5.0).
- **Caching the embedding probe**: not implemented in v0. Future small round if dogfood shows orchestrators polling too aggressively.

---

## Revision History

| Version | Date | Changes |
|---------|------|---------|
| v0.5.0 | 2026-05-02 | Initial implementation (Step 21, Round 10) |
