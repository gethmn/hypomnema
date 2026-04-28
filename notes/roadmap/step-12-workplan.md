# Step 12 Workplan — MCP Streamable HTTP transport (round-4 shipping gate)

**Step**: 12 of 12 (round 4 of 4 — **the round-4 shipping gate**, single-step round). Adds the third standard MCP transport — *Streamable HTTP* — alongside the shipped stdio transport (round 2, [ADR-0012](../../docs/decisions/0012-mcp-transport-stdio-v0.md)) and the deferred Unix-socket transport (also ADR-0012). See [`roadmap-4.md`](./roadmap-4.md) § Step 12 for the round, and [`archive/step-11-workplan.md`](./archive/step-11-workplan.md) for the immediately prior step (whose nine vault-management MCP tools this transport newly serves over HTTP).

**Status**: Workplan-phase; pending human review before build. Boundary is the **full round-4 shipping ritual** — milestone tag `v0.3.0`, per-step retro for step 12, end-of-round retro for round 4, round-4 roadmap archived alongside the workplan. See § Notes on round-4 shipping gate at the bottom.

**Round-3 / cross-round lessons carrying forward** (from [`notes/project-planning-workflow-notes.md`](../project-planning-workflow-notes.md) § End-of-round retrospective + steps 9–11 retros, and [`roadmap-4.md`](./roadmap-4.md) § Round-3 lessons feeding into this round):

- **MSRV cross-check** on any new top-level crate. Step 12 introduces **zero** new top-level crates. The single Cargo.toml line that changes is the `rmcp` features list — adding `transport-streamable-http-server` to the existing `["server", "transport-io", "macros", "schemars"]` set (resolved at pre-round prep; rmcp 1.5.0 ships the feature). `tower-http` direct promotion is **deferred** (Resolution G below); Origin validation is a hand-rolled axum middleware. Re-verified at workplan self-review.
- **Manual smoke verification** is load-bearing for medium-high-risk wiring tasks (now 6-of-6 across rounds 1–3). Step 12 is a wiring task by shape — new transport mounted on existing router. Smoke is a default inclusion; **Task 12.7** is the round-4 shipping gate's load-bearing manual smoke (curl + JSON-RPC against a real multi-vault daemon, plus an MCP-HTTP-capable client if available).
- **Spec-fleshout-at-workplan-write** (round-3 stable pattern) applies. The proposal at [`notes/proposals/mcp-streamable-http.md`](../proposals/mcp-streamable-http.md) is Draft v0.1.1 at workplan-write; **Task 12.1** promotes it to `docs/specs/mcp-streamable-http.md` v1.0.0 with all 5 open questions resolved (Resolutions B–F below). The promotion includes ADR-0013 + amendments to ADR-0012 / ADR-0008, plus arch + config doc syncs.
- **Forward-note prediction-vs-observation** check: step 12's external-library prediction surface is `rmcp = "1.5.0"`'s `transport-streamable-http-server` feature against `axum = "0.7"`. Pre-round prep verified the feature exists and `StreamableHttpService` is axum-version-agnostic (tower service over standard `http`/`http-body`); the load-bearing residual is whether `Router::nest_service("/mcp", service)` works cleanly on axum 0.7 without an upstream bump. Verified at **Task 12.4** task-time before the build commits to a specific mount shape — if `nest_service` doesn't compose with `StreamableHttpService` on axum 0.7, the workplan-time fallback is a thin axum handler that wraps the rmcp service's `tower::Service::call` directly (smaller blast radius than upgrading axum mid-round). No upgrade to axum 0.8 anticipated.
- **Workplan-prose-vs-load-bearing-decision drift** is a stable round-3 pattern (~0.5 flag-per-task across 24 round-3 tasks, all `coordinator-only` audience, zero human round-trips). **Carry-forward expectation**: step 12's surface (8 tasks across canon + trait extraction + HTTP-MCP route + tests + manual-testing refresh + smoke + boundary docs) will likely surface 3–5 such flags; treat them as defer-to-boundary by default unless a downstream task is materially affected. The playbook codifies this at § COORDINATOR Per-task execution loop step 6.1.
- **Internal-shape claims** (round-3 step-9 self-review addition): for any task that reshapes an existing module, re-read the task body against the current module signature at workplan self-review and flag aspirational language. Step 12's load-bearing reshape is **`HypomnemaMcpServer.client: DaemonClient`** → **`backend: Arc<dyn HypomnemaBackend>`** in Task 12.2. Self-review pass at the bottom of this workplan covers it.
- **Soft-flag self-correction at boundary** (round-3 step-10 new pattern): when consuming a forward-noted reconciliation, verify the claimed drift is actually present before editing. **Task 12.8**'s reference-docs agent applies this rule when consuming any forward-noted reconciliation requests from earlier tasks.
- **API-error stall + tail-peek diagnostic** (round-3 step-9 new pattern) and **silence-as-data for non-recurring flakes** (round-3 step-11 new pattern) carry forward to the coordinator's wake-up routing and post-build evaluation respectively. The pre-existing outbox flake (`tests/outbox.rs::rename_emits_deleted_then_created_lines`) was silent across step 9/10/11 sweeps; if it stays silent across step 12's quality-gate sweep, that's data for the round-4 retro per the silence-as-data rule (not a resolution).
- **Skills carrying forward**: [`rusqlite-in-async`](../../.claude/skills/rusqlite-in-async/SKILL.md) (any vault-management tool path through HTTP-MCP still routes through the daemon's connection pool via the in-process `VaultManager`); [`filesystem-watching`](../../.claude/skills/filesystem-watching/SKILL.md), [`markdown-chunking`](../../.claude/skills/markdown-chunking/SKILL.md), [`sqlite-vec-extension`](../../.claude/skills/sqlite-vec-extension/SKILL.md) — no direct relevance this round but remain authoritative for the substrates HTTP-MCP touches transitively. **No new skill anticipated**; if the rmcp + axum integration pattern proves codifiable across `Task 12.4`'s build, write an `mcp-http-transport` skill at boundary per the round-3 step-7 retro recommendation.

---

## Goal recap

`hmnd` exposes the **full MCP tool surface** (3 search tools + 9 vault-management tools = 12 tools) over a single HTTP endpoint at `/mcp` on its existing Axum router. An MCP-HTTP-capable agent host (Iris, a browser-hosted MCP client, or any MCP-compliant host that prefers HTTP transport) issues `initialize`, `tools/list`, and `tools/call` against the daemon and gets back the same response shapes that stdio-MCP returns today, with `serverInfo.name == "hypomnema"`. Origin-header validation defends against DNS rebinding from a malicious browser page. Stdio-MCP (`hmn mcp`, shipped in round 2) and HTTP-MCP coexist without interference — both transports serve the same tools against the same daemon state.

The transport is a **control plane that offers strictly less than the existing `/search/*` HTTP API in scope** (per [`notes/proposals/mcp-streamable-http.md`](../proposals/mcp-streamable-http.md) § Overview): same handlers, behind an MCP framing, with no new behavior, no new tool surface, no new trust posture. Authentication, TLS, bind-address policy, and rate limiting are concerns of `hmnd`'s HTTP listener as a whole (today: loopback-only, no auth, no TLS — extending [ADR-0005](../../docs/decisions/0005-local-everything.md)'s local-everything trust boundary). The MCP HTTP transport inherits whatever posture the listener is configured with; it does not introduce its own.

The round-4 shipping gate composes:

1. Behavior preservation: every step-9 / step-10 / step-11 integration test passes unchanged. The stdio-MCP transport (`hmn mcp`) keeps working; the new HTTP-MCP transport is additive.
2. **Real-shape multi-vault end-to-end** over HTTP-MCP (the round-3 multi-vault smoke shape extended to the new transport): a multi-vault daemon serves `tools/call` for every search tool and the read-only vault tools (and the seven write tools when `[mcp] enable_write_tools = true`) over HTTP-MCP, with results equivalent to the existing `/search/*` and `/vaults/*` HTTP endpoints.
3. **Manual-testing runbook refreshed for shipped reality** (Option A from [`roadmap-4.md`](./roadmap-4.md) § Manual-testing drift): the runbook in [`notes/manual-testing/`](../manual-testing/) is currently at "step 8" and predates round 3's multi-vault control plane; step 12 refreshes it to cover the multi-vault control plane (steps 9–11) plus the new HTTP-MCP transport.

The **boundary ritual** for this step is the full round-4 shipping variant per [`notes/project-planning-workflow-notes.md`](../project-planning-workflow-notes.md) § Step boundary ritual + § End-of-round retrospective: milestone tag `v0.3.0` (single-step round bumps minor by precedent — round 3 went `v0.1.0` → `v0.2.0`, round 4 goes `v0.2.0` → `v0.3.0`), per-step retro for step 12, end-of-round retro for round 4, round-4 roadmap archived. See § Notes on round-4 shipping gate.

---

## Deferred-decision resolutions

The five Open Questions from [`notes/proposals/mcp-streamable-http.md`](../proposals/mcp-streamable-http.md) are pulled forward to workplan-time per the round-3 spec-fleshout-at-workplan-write discipline (Resolutions A–E). Two additional fall-out resolutions surfaced during task decomposition (F and G) — they are not in the proposal's open-questions list but are workplan-time decisions worth naming.

### A. rmcp 1.5 Streamable HTTP server transport availability + axum-0.7 mount shape

**Resolution**: **rmcp ships it; mount via `Router::nest_service("/mcp", StreamableHttpService::new(...))` on axum 0.7, with a hand-rolled-handler fallback if `nest_service` fails to compose**.

Pre-round prep ([`roadmap-4.md`](./roadmap-4.md) § Pre-round prep item 1, marked Done 2026-04-28) verified that `rmcp = "1.5.0"` ships the `transport-streamable-http-server` feature, exposing `StreamableHttpService` (a tower service) and a `LocalSessionManager` default (in-memory session store). The canonical [`counter_streamhttp.rs`](https://github.com/modelcontextprotocol/rust-sdk/blob/main/examples/servers/src/counter_streamhttp.rs) example uses axum 0.8; Hypomnema is on axum 0.7. `StreamableHttpService` is generic over standard `http`/`http-body`/`tower` types (not axum-version-specific), and `Router::nest_service` exists in both 0.7 and 0.8 — so **0.7 should work without an upgrade**, but verify with `cargo build` at Task 12.4 task-time before committing.

**Fallback if `nest_service` doesn't compose** (e.g., axum 0.7's nest_service has a tighter `Service::Error` bound than rmcp's): wrap the rmcp service in a thin axum handler:

```rust
async fn mcp_handler(
    State(state): State<McpHttpState>,
    req: Request<Body>,
) -> Response {
    state.service.clone().call(req).await
        .unwrap_or_else(|infallible| match infallible {})
}
// router.route("/mcp", get(mcp_handler).post(mcp_handler))
```

This is structurally smaller than upgrading axum mid-round (axum 0.7 → 0.8 is a meaningful blast radius across `src/api/`, `src/bin/hmnd.rs`, and tests). Default to `nest_service` if it works; the hand-rolled handler is the safety net. **Task 12.4 picks the shape and records the choice in its results comment.**

**No top-level crate additions**. The single Cargo.toml change is adding `transport-streamable-http-server` to the existing `rmcp` features list:

```toml
rmcp = { version = "1.5", features = [
    "server",
    "transport-io",
    "transport-streamable-http-server",  # NEW
    "macros",
    "schemars",
] }
```

### B. Session management (`Mcp-Session-Id`)

**Resolution**: **v1 ships stateless; use rmcp's `LocalSessionManager` default but treat each tool/call as independent**.

MCP Streamable HTTP supports `Mcp-Session-Id` for stateful sessions; v1 deliberately ships stateless because Hypomnema's tool surface is single-shot request/response with no session-scoped state to track between calls. `LocalSessionManager` is rmcp's in-memory default — it satisfies the type bound `StreamableHttpService` requires without introducing persistence. Each `tools/call` is independent; Hypomnema does not require clients to send a session id and does not depend on session state across calls.

Future-amendment trigger: if a future round ships a tool that needs session-scoped state (e.g., multi-step transactions, paginated searches with cursor-in-session), the spec amends to add session semantics. The v1 stateless shape doesn't preclude future stateful tools — it just doesn't pre-commit to them.

**How to apply**: Task 12.4 instantiates `LocalSessionManager::default()` and wires it into `StreamableHttpService::new(factory, manager.into(), config)`. No additional config surface; no `[mcp.http]` session knob.

### C. Resumable SSE streams (`Last-Event-ID`)

**Resolution**: **defer**.

v1 has no notifications or server-pushed messages to send for the read-only search tools or the idempotent vault-management tools. Each tool/call is a single request → single response over POST. The SSE GET channel opens, stays open, and closes cleanly with no events emitted (per [`notes/proposals/mcp-streamable-http.md`](../proposals/mcp-streamable-http.md) § Examples § Example 2).

**Future-amendment trigger**: when long-running tools (e.g., a streaming semantic search that emits per-result progress) or server-initiated tool calls land. The spec amends then; v1 doesn't pre-commit.

### D. CORS

**Resolution**: **v1 sets no CORS headers**.

The first real browser-hosted host consumer's requirements decide what to allow. Hand-rolled `tower-http::cors` is the natural future shape; deferred until a concrete consumer surfaces.

The Origin-validation middleware (Task 12.4 — DNS-rebinding defense; *not* CORS) is a separate concern: Origin validation rejects unauthorized cross-origin POSTs at the request-entry boundary; CORS preflight is the browser's mechanism for the client to know which cross-origin requests to issue. Hypomnema doesn't expose itself for cross-origin use in v1, so no preflight machinery is needed.

### E. `mcp.http.path` configurability

**Resolution**: **v1 rejects any value other than `"/mcp"` with a startup error**.

Keeps the routing story trivial; future versions may allow customization (e.g., for operators reverse-proxying multiple Hypomnema daemons under different prefixes). **Out of scope for v1**.

**How to apply** (Task 12.4): in `Config::validate()` (or wherever validation lives — verify at task time), if `config.mcp.http.path != "/mcp"`, return a structured error with the exact message `mcp.http.path must be "/mcp" in this version of Hypomnema`. The daemon exits non-zero at startup with the message on stderr.

### F. ADR strategy: amend ADR-0012 vs. write ADR-0013

**Resolution**: **write a new ADR-0013; amend ADR-0012 § Decision (one entry) + § Amendments (back-pointer) + ADR-0008 § Amendments (one-line note)**.

The cleaner option: 0013 documents the round-4 transport addition as its own canon-touching decision, leaving 0012 as the historical record of the v0 stdio-shipped / socket-deferred decision. ADR-0012 § Decision lists three transports going forward (stdio shipped, socket deferred, HTTP-MCP shipped); the § Amendments section gets a 2026-XX-XX entry pointing at 0013. ADR-0008 § Amendments gets a one-line note that HTTP-MCP joins the deferred socket transport as the second `hmnd`-resident MCP transport — reusing the existing "each transport's binary matches its lifetime" principle (no decision change).

**ADR-0013 outline** (Task 12.1 fleshes from this):

- **Title**: `0013-mcp-transport-streamable-http.md`.
- **Status**: accepted.
- **Context**: round-4 adds the third standard MCP transport. Browser-hosted hosts can't spawn `hmn mcp` subprocesses; remote MCP scenarios want a network endpoint. Streamable HTTP is the MCP-spec-defined post-2024-11-05 transport.
- **Decision**: HTTP-MCP lives on `hmnd`'s existing Axum router at `/mcp` (POST + SSE GET). Mounted via `rmcp::transport::streamable_http_server::StreamableHttpService` (rmcp 1.5+). The transport inherits the daemon's HTTP listener trust posture (loopback-only, no auth, no TLS — extending ADR-0005's local-everything boundary). Origin-header validation defends against DNS rebinding (the standard MCP-spec-recommended browser-side defense). v1 ships stateless (Resolution B), no resumable SSE (Resolution C), no CORS (Resolution D), `path` reserved at `/mcp` (Resolution E).
- **Consequences**:
  - *Positive*: browser-hosted hosts and remote-MCP scenarios become reachable; trust model unchanged; no parallel listener; the brand-identity macro pin from ADR-0012 § Resolution 4 applies unchanged.
  - *Negative*: third transport means three sites to revisit on rmcp major upgrades; Origin-validation is now a load-bearing security boundary that round-4 tests explicitly verify.
  - *Neutral*: the MCP HTTP transport is structurally identical to the existing `/search/*` HTTP API in trust posture and binding — same listener, same loopback default, same no-auth posture; trade-offs that earned `/search/*` apply unchanged.
- **Amends ADR-0012** (one row in 0012 § Amendments pointing at 0013 + one new bullet in 0012 § Decision listing HTTP as third transport). **Amends ADR-0008** (one row noting HTTP-MCP joins socket as `hmnd`-resident MCP transports).

### G. `HypomnemaBackend` trait shape + `tower-http` direct-dep promotion

**Resolution**: **define a `HypomnemaBackend` trait with 12 methods covering all current MCP tool ops; two implementations — existing `DaemonClient` (HTTP shim, used by `hmn mcp`) and new `InProcessBackend` (direct VaultManager calls, used by `hmnd /mcp`). `HypomnemaMcpServer.client: DaemonClient` → `backend: Arc<dyn HypomnemaBackend + Send + Sync>`. No `tower-http` direct-dep promotion in v1 — Origin validation is a hand-rolled axum middleware**.

The proposal's [§ Implementation Notes](../proposals/mcp-streamable-http.md) flags the trait shape as a workplan-time decision, with two sub-questions:

1. *Does HTTP-MCP need a trait at all, or can it just instantiate `HypomnemaMcpServer` with a `DaemonClient` pointing at `hmnd`'s own loopback bind?* **Trait wins**: the loopback-DaemonClient option introduces a wasted HTTP round-trip on every tool call inside the daemon, doubles the failure surface (the loopback DaemonClient could in principle fail to reach itself if the listener isn't yet bound at startup), and conflicts with the proposal's data-flow framing ("in-process; no DaemonClient HTTP shim"). The trait extraction is meaningful but bounded — 12 methods, two impls, one server-side struct change.

2. *Does the trait cover only search ops or also vault-management ops?* **All 12 ops**: round-3 step-10 already shipped 4 vault-management MCP tools and step-11 added 5 more, so the stdio MCP server already calls `DaemonClient` for all 9 vault-management ops alongside the 3 search ops. Extracting only search would leave 9 vault-management methods on `DaemonClient` that need a parallel call path for the in-process case anyway — same blast radius, with worse cohesion. The trait covers all 12 ops for consistency with the round-3 vault-management surface.

**Trait sketch** (Task 12.2 pins the exact shape):

```rust
// src/mcp/backend.rs (new module)
#[async_trait::async_trait]
pub trait HypomnemaBackend: Send + Sync + 'static {
    // search (3)
    async fn search_filesystem(&self, q: &FilesystemQueryJson)
        -> anyhow::Result<FilesystemSearchResponse>;
    async fn search_content(&self, q: &ContentQueryJson)
        -> anyhow::Result<ContentSearchResponse>;
    async fn search_semantic(&self, q: &SemanticQueryJson)
        -> anyhow::Result<SemanticSearchResponse>;
    // vault read (2)
    async fn list_vaults(&self) -> anyhow::Result<VaultListResponse>;
    async fn get_vault(&self, name_or_id: &str) -> anyhow::Result<VaultRowJson>;
    // vault write (7)
    async fn create_vault(&self, req: &CreateVaultRequest) -> anyhow::Result<VaultRowJson>;
    async fn pause_vault(&self, name_or_id: &str) -> anyhow::Result<VaultRowJson>;
    async fn resume_vault(&self, name_or_id: &str) -> anyhow::Result<VaultRowJson>;
    async fn reset_vault(&self, name_or_id: &str, rebuild: bool) -> anyhow::Result<VaultRowJson>;
    async fn rename_vault(&self, name_or_id: &str, new_name: &str) -> anyhow::Result<VaultRowJson>;
    async fn rescan_vault(&self, name_or_id: &str) -> anyhow::Result<RescanResponseJson>;
    async fn terminate_vault(&self, name_or_id: &str) -> anyhow::Result<TerminateVaultResponse>;
    // diagnostic surface (replaces the free-function is_connect_error)
    fn is_connect_error(&self, err: &anyhow::Error) -> bool;
}
```

The 12-method surface is **isomorphic to the existing `DaemonClient` public methods** ([`src/client.rs`](../../src/client.rs) lines 50–131) — Task 12.2 defines the trait, implements it on `DaemonClient` (zero behavior change), and refactors `HypomnemaMcpServer` to hold `Arc<dyn HypomnemaBackend>` instead of `DaemonClient`. The `is_connect_error` method becomes part of the trait so the MCP server can synthesize `daemon_unreachable` envelopes for the stdio path while the in-process path returns `false` (the daemon is always reachable from itself).

`async_trait` is already a transitive dep via tokio's `full` feature in many trees — verify at Task 12.2 task-time (the alternative is rmcp's existing `Future`-returning method shape, but `async_trait` reads more directly). If `async_trait` is not yet in the dep graph, the workplan-time fallback is to return `Pin<Box<dyn Future ...>>` manually (one extra line per method, no new crate).

**`tower-http` direct-dep promotion**: **deferred**. v1 has no CORS (Resolution D), no rate limiting, no auth, no request-id propagation requirements that would benefit from the framework's middleware shape. Origin validation is a 30-line hand-rolled axum middleware (Task 12.4) — promoting `tower-http` for one middleware costs more than it saves. When CORS lands (round-5+), `tower-http`'s `cors` module is the natural answer; promotion happens then.

### H. Promotion target + stories file disposition

**Resolution**: **spec only — `notes/proposals/mcp-streamable-http.md` → `docs/specs/mcp-streamable-http.md` v1.0.0. Stories file does not promote; it archives alongside the proposal. Acceptance criteria absorb into this workplan's § Shipping criteria as the concrete acceptance bar; the spec's existing § Examples + § Edge Cases sections are the canonical wire-shape contract**.

Round-3 didn't promote story files to LDS canon — stories stayed in `notes/proposals/` and got archived alongside their proposals. Round 4 follows the same precedent: the spec is the canonical artifact; the stories file's role is workplan-input ("here are the testable stories the workplan must cover"). After workplan-write, the stories file's job is done — its acceptance criteria live in the spec's examples/edge-cases sections and in this workplan's § Shipping criteria.

**How to apply** (Task 12.1):

1. Move `notes/proposals/mcp-streamable-http.md` → `docs/specs/mcp-streamable-http.md`. Bump `Status: Draft` → `Status: Approved`. Bump version `0.1.1` → `1.0.0`. Add a § Revision History entry: `1.0.0 | <date> | Promoted from notes/proposals/mcp-streamable-http.md (was 0.1.1). Round-4 step 12 workplan resolutions: Open Question 1 → resolved at pre-round prep (rmcp ships transport feature; mount via nest_service on axum 0.7 with hand-rolled-handler fallback); Open Question 2 → v1 stateless; Open Question 3 → defer; Open Question 4 → v1 sets no CORS headers; Open Question 5 → v1 rejects mcp.http.path != "/mcp" with startup error.` Remove the resolved entries from § Open Questions (preserve the 4 deferred ones with their workplan-time-deferral rationales).
2. Rewrite cross-references from `../../docs/...` to LDS-relative per `_template.md` conventions: `../../docs/decisions/0012-...` → `../decisions/0012-...`; `../../docs/specs/filesystem-search.md` → `./filesystem-search.md`; etc. The proposal's preamble note about this rewrite gets removed in the promoted version.
3. Move `notes/proposals/mcp-streamable-http-stories.md` → `notes/proposals/archive/mcp-streamable-http-stories.md` (no content change; the file's role is done).
4. Move the (now-promoted-and-renamed) original `notes/proposals/mcp-streamable-http.md` to `notes/proposals/archive/mcp-streamable-http.md` as a frozen historical record (per [`notes/proposals/README.md`](../proposals/README.md) § Lifecycle). The promoted file at `docs/specs/mcp-streamable-http.md` is the canonical going-forward source.

### I. `docs/specs/vault-management.md` wording amendment vs. no-op

**Resolution**: **verify-then-amend at workplan-write — likely a small sentence-level edit**.

Vault-management.md § Overview already commits to "same handlers, three transports": *"Vault lifecycle operations (`create`, `list`, `status`, `pause`, `resume`, `reset`, `rename`, `rescan`, `terminate`) are exposed by `hmnd` over an HTTP control plane, the `hmn` CLI, and MCP tools — same handlers, three transports."* — but reads ambiguously: it could be parsed as (HTTP, CLI, MCP) = three transports OR (HTTP, stdio-MCP, HTTP-MCP) = three MCP transports. The first parse was the round-3 reading; the second is what HTTP-MCP introduces. § MCP Tool Surface (line 317–340) currently doesn't mention HTTP-MCP at all.

**Verify-at-workplan-write**: the wording IS sufficient at the § Overview level (operators reading the spec see "MCP tools" without caring whether the transport is stdio or HTTP — both serve the same tools), but § MCP Tool Surface benefits from a one-sentence forward-reference to `mcp-streamable-http.md` so the round-4 reader sees the bridge. **Task 12.1 amends § MCP Tool Surface with a single sentence** at the bottom of the section: `"Both stdio MCP (the hmn mcp subcommand, ADR-0012) and HTTP MCP (the /mcp endpoint on hmnd, ADR-0013) serve this same tool surface; the [mcp] enable_write_tools flag governs both transports identically."` Vault-management.md ticks to v1.1.0 with the edit; § Revision History entry is `1.1.0 | <date> | Step 12 workplan: small wording amendment to § MCP Tool Surface naming both stdio MCP and HTTP MCP transports as peers serving the same tool list. No behavioral change.`

If verification at task time shows the existing wording is already unambiguous (operators reading don't trip over the transport ambiguity), Task 12.1 records a verify-before-editing pass per the round-3-step-10 soft-flag self-correction pattern and skips the edit. **Default to amend; allow skip on verification failure-to-find-drift.**

---

## Self-review for prose accuracy

This workplan is projected at ~750–850 lines (slightly larger than step 10's 703 and step 11's 692, both round-3 references; smaller than step 8's 958, the round-2 MCP wrapper that introduced `rmcp` and was the closest comparable scope). The round-1 ~1000-line heuristic does not fire automatically; the step-9 internal-shape-claims heuristic does. Spot-check on testable claims:

### Internal-shape claims (round-3 step-9 self-review addition)

1. **`HypomnemaMcpServer.client: DaemonClient`** is the current shape ([`src/mcp/server.rs:14–19`](../../src/mcp/server.rs)). Workplan Task 12.2 reshapes this to `backend: Arc<dyn HypomnemaBackend + Send + Sync>` with the trait defined in a new `src/mcp/backend.rs` module. Verified by reading `src/mcp/server.rs` at workplan-write — current shape matches the prescription (a single struct field plus the `is_connect_error` free-function call inside `envelope_from_anyhow`). Internal callers of `self.client.<method>` inside the 12 `#[tool]` method bodies (lines 37, 54, 71, 84, 99, etc. — verify ranges at Task 12.2 task-time) all change from `self.client.foo(...)` to `self.backend.foo(...)`; ergonomic ripple is small (each call site changes the receiver, not the argument shape).

2. **`DaemonClient` public methods** are 12 `pub async fn`s ([`src/client.rs:50–131`](../../src/client.rs) — verified at workplan-write: `search_filesystem`, `search_content`, `search_semantic`, `create_vault`, `list_vaults`, `get_vault`, `terminate_vault`, `pause_vault`, `resume_vault`, `reset_vault`, `rename_vault`, `rescan_vault`) plus `health`, `status`, `from_config`, `base_url`, and the free function `is_connect_error`. The 12-tool-method set is exactly the trait surface; `health` and `status` are not MCP tools (they're CLI / scripting paths), so they stay non-trait. `is_connect_error` becomes a trait method (Resolution G). `from_config` and `base_url` stay as inherent methods on `DaemonClient`.

3. **`HypomnemaMcpServer` rmcp tool registration**: 12 `#[tool]` methods in `src/mcp/server.rs` (verified at workplan-write — round-3 step-11 task 11.5 added the last 5; tool count was confirmed in vault-management.md § MCP Tool Surface). The `#[tool_handler(name = "hypomnema")]` attribute on `impl ServerHandler for HypomnemaMcpServer` (line 25) is the brand-identity override per [ADR-0012 § Resolution 4](../../docs/decisions/0012-mcp-transport-stdio-v0.md#decision); HTTP-MCP inherits it unchanged. **No new rmcp macro syntax learned**; the trait-extraction in Task 12.2 doesn't touch the macro layer.

4. **Existing axum router** at [`src/api/mod.rs:47–65`](../../src/api/mod.rs) (verified at workplan-write — 12 routes: `/health`, `/status`, `/search/{filesystem,content,semantic}`, `/vaults` POST + GET, `/vaults/:name_or_id` GET + DELETE, `/vaults/:name_or_id/{pause,resume,reset,rename,rescan}`). Task 12.4 adds **one new route**: `/mcp` (mounted via `nest_service` if it works, fallback to a route handler). Wiring on `hmnd`'s `run_daemon` in [`src/bin/hmnd.rs:128–152`](../../src/bin/hmnd.rs) is unchanged — `axum::serve(listener, app).with_graceful_shutdown(...)` already covers the new route by virtue of being on the same `Router`.

5. **`Config` struct** in `src/config.rs` currently has `[mcp]` fields (`transport`, `socket`, `enable_write_tools`) per the existing round-3 surface. Task 12.4 adds a nested `[mcp.http]` block — likely as a struct field `http: McpHttpConfig` on the existing `Mcp` config struct, with `McpHttpConfig { enabled: bool, path: String }`. Verified at task time by reading `src/config.rs`. Validation in `Config::validate()` (or wherever) rejects `path != "/mcp"` per Resolution E.

6. **`hmnd` HTTP server graceful shutdown** at [`src/bin/hmnd.rs:144–151`](../../src/bin/hmnd.rs): `axum::serve(listener, app).with_graceful_shutdown(async move { let _ = http_shutdown.wait_for(|v| *v).await; })`. `StreamableHttpService` mounted as a child on `app` inherits this shutdown wiring — when the watch-channel fires, axum's `serve` future resolves, all in-flight requests (including SSE GETs to `/mcp`) drain. **Task 12.4 does not add new shutdown machinery**; Story 4 (graceful shutdown) is structurally satisfied by reusing the existing path. Verified by the existence of the `with_graceful_shutdown` integration at line 145.

7. **rmcp `transport-streamable-http-server` feature**: `Cargo.toml:31` currently has `rmcp = { version = "1.5", features = ["server", "transport-io", "macros", "schemars"] }`. Task 12.4 adds `"transport-streamable-http-server"` to that list. Pre-round-prep verified the feature exists at rmcp 1.5.0; no version bump.

8. **Origin validation positive grep**: after Task 12.4 ships, `rg 'Origin' src/api/` returns at least one match in `src/api/mcp_http.rs` (the validator). Verified per the proposal's [§ Implementation Notes](../proposals/mcp-streamable-http.md): "*Origin-validation positive grep: After this spec ships, `rg 'Origin' src/api/` returns at least one match in the MCP handler module — the validator is required, not optional.*"

9. **Negative-fingerprint greps** (per `mcp-streamable-http-stories.md` Stories 2 + 3): post-build, `rg 'allow_any_origin|cors_allow_all|Access-Control-Allow-Origin: \*' src/` returns zero matches in the MCP handler module; `rg 'reqwest::Client|hyper::Client|TcpListener::bind' src/api/mcp_http*.rs` returns zero matches; `rg 'rustls|server_config|ServerCertVerifier|verify_token|api_key' src/api/mcp_http*.rs` returns zero matches. These are load-bearing for the v1 trust-posture-inheritance shape — Task 12.5's integration tests grep for them, and a hit is a build failure.

### External-library claims

1. **`rmcp::transport::streamable_http_server::StreamableHttpService`** is a tower service; mounts on axum via `Router::nest_service("/mcp", service)` per the canonical example. Verified at pre-round prep against rmcp 1.5.0's release; verify with `cargo build` at Task 12.4 task-time.

2. **`rmcp::transport::streamable_http_server::session::local::LocalSessionManager`** is the in-memory session store default. Verified at pre-round prep. v1 uses `LocalSessionManager::default()` per Resolution B.

3. **`axum::Router::nest_service`** exists in axum 0.7 (verified by reading axum 0.7 source at workplan-write — `Router::nest_service` is the same method-name in 0.7 and 0.8). The compose-with-StreamableHttpService verification is workplan-time-deferred to Task 12.4 (cargo build is the load-bearing check).

4. **`async_trait`**: workplan-time prediction is that it's in the dep graph (transitive via tokio or rmcp); if absent, fallback to manual `Pin<Box<dyn Future + Send>>` returns. Either way, no top-level Cargo.toml addition.

### Cross-platform claims

1. **No filesystem operations** in HTTP-MCP route handler. Origin validation is header-string operations (case-sensitive ASCII compare against the allowed-origin list); no path manipulation, no socket binding, no temp files.

2. **Loopback addresses across platforms**: the allowed-origin list (`http://localhost[:port]`, `http://127.0.0.1[:port]`, `http://[::1][:port]`, `null`) covers Linux, macOS, and Windows loopback configurations. No platform-specific origin variants needed.

---

## Tasks

The 8-task decomposition matches step 10's and step 11's density. Per the round-1/2/3 default-not-batch rule (now 11-of-11 consecutive clean steps including step 11 and the round-3 boundary), tasks ship as solo agents. Each task ships its own commit per the playbook's TASK AGENT § Reporting; risk grades and dependencies noted at each task header.

### Task 12.1 — Spec promotion + ADR-0013 + ADR-0012/0008 amendments + arch + config sync

**Risk**: low. Doc-only by design; mechanical-mostly. Lands first because tasks 12.2–12.5 reference the promoted spec, the new ADR, and the `[mcp.http]` config block. **Subsumes pre-existing todos 83 and 84** — those todos describe exactly this canon work; they close as duplicates of this task at boundary (or in this task's results comment).

**Scope**:

- **Promote the spec** (Resolution H):
  - Move `notes/proposals/mcp-streamable-http.md` content → new file `docs/specs/mcp-streamable-http.md`. Bump `Version: 0.1.1` → `Version: 1.0.0`; `Status: Draft` → `Status: Approved`; `Date: 2026-04-28` (workplan-write date — verify against the actual ship date and adjust at task time).
  - Rewrite cross-references from `../../docs/...` to LDS-relative per `_template.md` conventions: `../../docs/decisions/0012-...` → `../decisions/0012-...`; `../../docs/decisions/0005-...` → `../decisions/0005-...`; `../../docs/decisions/0008-...` → `../decisions/0008-...`; `../../docs/decisions/0004-...` → `../decisions/0004-...`; `../../docs/architecture/overview.md` → `../architecture/overview.md`; `../../docs/specs/{filesystem,content,semantic}-search.md` → `./{filesystem,content,semantic}-search.md`; `../../docs/specs/vault-management.md` → `./vault-management.md`. Remove the preamble note at the bottom of § Related Documents that flagged this rewrite step.
  - Add a § Revision History entry per Resolution H step 1.
  - Update § Open Questions to reflect resolutions: remove resolved OQ 1 (`rmcp 1.5 ...`); preserve OQ 2 (sessions), OQ 3 (resumable SSE), OQ 4 (CORS), OQ 5 (`mcp.http.path`) with their workplan-time-deferral rationales updated to reference Resolutions B–E in this workplan.
  - **Add a forward-reference in spec § Implementation Notes**: a sentence at the end of the section pointing to `notes/roadmap/archive/step-12-workplan.md` (post-archive) for the workplan-time resolutions of OQs 1–5.
  - Move the original `notes/proposals/mcp-streamable-http.md` (post-promotion, no longer the canonical location) → `notes/proposals/archive/mcp-streamable-http.md`. Move `notes/proposals/mcp-streamable-http-stories.md` → `notes/proposals/archive/mcp-streamable-http-stories.md` (no content change).

- **Write ADR-0013** at `docs/decisions/0013-mcp-transport-streamable-http.md`. Use the outline in Resolution F as the skeleton; flesh per the project's existing ADR style (see [ADR-0012](../../docs/decisions/0012-mcp-transport-stdio-v0.md) for the closest-shaped example — same author, same scope-shape).

- **Amend ADR-0012**:
  - § Decision: append a fourth bullet (or extend bullet 2) listing HTTP transport as a shipped third transport on `hmnd`. Phrase as a dated forward-edit: `**HTTP MCP transport ships in round 4** (ADR-0013). The full v0-deferred MCP transport landscape is now stdio-on-hmn (this ADR § Resolution 1, shipped round 2), HTTP-on-hmnd (ADR-0013, shipped round 4), socket-on-hmnd (this ADR § Decision 2, deferred).` Verify exact wording at task time — the round-3-step-10 soft-flag self-correction at boundary pattern applies.
  - § Amendments: append a 2026-XX-XX entry: `**HTTP-MCP transport added in round 4 (ADR-0013).** ADR-0013 introduces Streamable HTTP MCP on `hmnd` as the round-4 shipping gate. The v0 stdio-shipped / socket-deferred decision recorded in this ADR is unchanged; ADR-0013 adds the third transport without superseding the present scope.`

- **Amend ADR-0008** § Amendments: append a one-line entry referencing ADR-0013: `**HTTP-MCP joins socket-MCP as the second `hmnd`-resident MCP transport (ADR-0013, round 4).** The "each transport's binary matches its lifetime" principle from ADR-0012's amendment applies unchanged: stdio-MCP → `hmn` (short-lived adapter); HTTP-MCP → `hmnd` (long-lived listener); socket-MCP (deferred) → `hmnd` (long-lived listener). No decision change to the two-binaries shape.`

- **Amend `docs/specs/vault-management.md` § MCP Tool Surface** (Resolution I, verify-then-amend): if the wording check at task-time confirms the existing § MCP Tool Surface prose is ambiguous about the stdio-vs-HTTP split, append the one-sentence forward-reference per Resolution I and bump the spec to v1.1.0 with a § Revision History entry. If verification finds the wording sufficient, record a verify-before-editing pass in the task's results comment and skip the edit.

- **Sync `docs/architecture/overview.md`**:
  - § Container Descriptions row "Search API" (line ~122 — verify at task time): change "two transports" → "three transports" (HTTP, stdio MCP, HTTP MCP).
  - § Consumers (line ~56–65): the existing "AI agents" row covers MCP transports generically; add a phrase or sub-bullet noting HTTP MCP is now an option alongside stdio MCP for browser-hosted hosts and remote scenarios. Cross-reference ADR-0013.
  - § Search API (line ~212–220): rewrite the lead from "All three operations are exposed identically over HTTP (Axum) and MCP (rmcp)" to enumerate the three transports: HTTP `/search/*` (Axum), stdio MCP (`hmn mcp` shim), HTTP MCP (`/mcp` endpoint). Cross-reference the new spec at `docs/specs/mcp-streamable-http.md` and ADR-0013.
  - § External Communication table (line ~256–262): change the "Inbound MCP transport (stdio or socket, TBD)" row to enumerate the now-shipped surface: `MCP over stdio (via `hmn mcp`)` and `MCP over HTTP (`/mcp` on the same listener as `/search/*` and `/vaults/*`)`. The deferred Unix-socket transport stays a third row (or footnote) for forward-compat.

- **Sync `docs/reference/configuration.md`**: add a new `[mcp.http]` section after the existing `[mcp]` section. Two options as a table:

  | Option | Type | Required | Default | Description |
  |--------|------|----------|---------|-------------|
  | `mcp.http.enabled` | bool | no | `true` | Whether to mount `/mcp` on the daemon's HTTP listener (loopback by default; inherits the listener's full trust posture). Set to `false` to disable HTTP-MCP without disabling stdio MCP. |
  | `mcp.http.path` | string | no | `"/mcp"` | Route prefix. **Reserved-for-forward-compat in v1**: any value other than `"/mcp"` produces a startup error. Future versions may allow customization. |

  Plus a `> **HTTP-MCP trust posture inheritance**` callout pointing at ADR-0013 + § `[http]` for the underlying listener config.

- **Update `notes/backlog.md`**: remove (or mark resolved) the round-4-candidate entry for "MCP Streamable HTTP transport" since round 4 *is* this round. The other round-4 candidates carry forward. Add any boundary follow-ups surfaced by the task's verification work.

- **Close pre-existing todos 83 and 84** with a comment on each referencing this task's commit and stating the work was subsumed into step 12 task 12.1. Don't delete the todos; mark them complete (`todo_complete(id, true)`).

**Tests**: doc-only; no code tests in this task. Verify post-edit:
- `cargo doc --no-deps` runs cleanly (the Cargo workspace doesn't render markdown, but this catches any rustdoc cross-link breakage in `src/lib.rs` if markdown paths get referenced from doc-comments — unlikely but cheap to check).
- No broken cross-references in the promoted spec: spot-check 5 random cross-refs by clicking through them in a markdown viewer (or use `find docs/ -name "*.md" -exec grep -l 'docs/specs/mcp-streamable-http' {} \;` to verify reverse-links from arch/config land correctly).

**Files touched**:
- New: `docs/specs/mcp-streamable-http.md`, `docs/decisions/0013-mcp-transport-streamable-http.md`, `notes/proposals/archive/mcp-streamable-http.md`, `notes/proposals/archive/mcp-streamable-http-stories.md`.
- Removed: `notes/proposals/mcp-streamable-http.md` (now in archive), `notes/proposals/mcp-streamable-http-stories.md` (now in archive).
- Edited: `docs/decisions/0012-mcp-transport-stdio-v0.md`, `docs/decisions/0008-two-binary-daemon-plus-cli.md`, `docs/specs/vault-management.md` (verify-then-amend), `docs/architecture/overview.md`, `docs/reference/configuration.md`, `notes/backlog.md`.

**Dependencies**: none. Lands first; the canonical foundation for tasks 12.2–12.8.

**Soft-flag-ready territory**:
- The `vault-management.md § MCP Tool Surface` verify-then-amend may resolve as no-op (existing wording already sufficient). Surface as a `coordinator-only` soft flag noting the verify-before-editing pass per round-3-step-10 pattern.
- The `notes/proposals/archive/` directory may not exist yet — verify at task time by listing `notes/proposals/`. If not present, `mkdir -p notes/proposals/archive/` is the natural first action; surface as a `coordinator-only` flag if any other archive convention is in flight.
- ADR-0013 numbering: verify no ADR-0013 already exists in `docs/decisions/`. The current highest is 0012; if a parallel branch landed an 0013 in the meantime, escalate (numbering collision needs human input on whether to renumber the new one).
- Closing todos 83 / 84 is a Solo-todo operation; the task agent posts completion comments referencing the step-12 task-12.1 commit before marking each `completed`.

### Task 12.2 — `HypomnemaBackend` trait extraction + `HypomnemaMcpServer` refactor + `DaemonClient` impl

**Risk**: medium. **Load-bearing for tasks 12.3–12.5.** Reshapes `HypomnemaMcpServer.client: DaemonClient` to `backend: Arc<dyn HypomnemaBackend + Send + Sync>`; introduces the trait surface; preserves the stdio MCP path byte-for-byte. The `HypomnemaBackend` trait is the structural foundation for the in-process backend Task 12.3 implements.

**Scope**:

- **Define `HypomnemaBackend` trait** in a new module `src/mcp/backend.rs` (re-exported from `src/mcp/mod.rs`). Use the sketch in Resolution G; pin the exact method signatures at task-time against [`src/client.rs`](../../src/client.rs) lines 50–131. Use `#[async_trait::async_trait]` if `async_trait` is available; otherwise return `Pin<Box<dyn Future<Output=...> + Send>>` manually. **No new top-level Cargo.toml crates** — verify `async_trait` availability before committing to a syntax.

- **Implement `HypomnemaBackend` for `DaemonClient`** in `src/client.rs` (or a new `src/client_backend.rs` if the impl block reads cleaner separately — task-time decision). Each method delegates to the existing inherent `DaemonClient::{search_filesystem, ...}` method with the same body. Zero behavior change. The `is_connect_error` trait method delegates to the existing `crate::client::is_connect_error` free function.

- **Refactor `HypomnemaMcpServer`** in [`src/mcp/server.rs`](../../src/mcp/server.rs):
  ```rust
  // BEFORE (line 14-19):
  #[derive(Clone)]
  pub struct HypomnemaMcpServer {
      pub client: DaemonClient,
      pub default_vault_name: String,
      pub enable_write_tools: bool,
  }

  // AFTER:
  #[derive(Clone)]
  pub struct HypomnemaMcpServer {
      pub backend: Arc<dyn HypomnemaBackend + Send + Sync>,
      pub default_vault_name: String,
      pub enable_write_tools: bool,
  }
  ```
  Update all `self.client.<method>(...)` call sites inside `#[tool]` method bodies to `self.backend.<method>(...)`. Update `envelope_from_anyhow(client: &DaemonClient, err: &anyhow::Error)` to `envelope_from_anyhow(backend: &dyn HypomnemaBackend, err: &anyhow::Error)` — the function calls only `is_connect_error` and `base_url`-style introspection. Drop `base_url` from the trait if it's not actually used inside `envelope_from_anyhow` (verify at task time); keep on `DaemonClient` as inherent.

- **Update construction sites** of `HypomnemaMcpServer`:
  - `src/bin/hmn.rs::cmd_mcp` constructs `HypomnemaMcpServer { client: DaemonClient::from_config(...), ... }` (verify at task time). Refactor to `HypomnemaMcpServer { backend: Arc::new(DaemonClient::from_config(...)), ... }`.
  - Test fixtures in `src/mcp/server.rs::tests` (line ~360 — `server_against` and `server_against_with_writes`): same refactor.

- **Stdio path preservation**: `tests/mcp.rs` (the round-2 step-8 mock-daemon test) continues to pass against `HypomnemaMcpServer` constructed with `Arc::new(DaemonClient)`. **No round-2 / round-3 test changes anticipated** — the trait-extraction is structurally invisible to the stdio path. If any test reaches into `server.client` directly (e.g., to inspect `base_url`), refactor to `server.backend` plus a downcast-to-DaemonClient if needed (or expose the field). Verify `cargo test` is green before committing.

**Tests** (extend `src/mcp/server.rs::tests` and `src/mcp/backend.rs::tests`):

- New: `daemon_client_implements_hypomnema_backend` — compile-time assertion via `fn assert_impl<T: HypomnemaBackend>(_: &T) {}` against a `DaemonClient`. Catches trait-bound regression.
- New: `hypomnema_mcp_server_holds_arc_dyn_backend` — type-check that `HypomnemaMcpServer::backend` is `Arc<dyn HypomnemaBackend + Send + Sync>`. Catches accidental concrete-typing regression.
- Existing: `cargo test` is green; `tests/mcp.rs` passes unchanged.

**Files touched**:
- New: `src/mcp/backend.rs` (trait definition + impl-on-DaemonClient if it doesn't go in `src/client.rs`).
- Edited: `src/mcp/mod.rs` (re-export), `src/mcp/server.rs` (struct field rename + envelope_from_anyhow signature + construction-site refactor in tests), `src/bin/hmn.rs` (Arc-wrap construction site).
- Possibly: `src/client.rs` (impl HypomnemaBackend if it lands inline rather than in `src/mcp/backend.rs`; verify cleanest shape at task time).

**Dependencies**: 12.1 (the spec promotion provides the canonical reference for what HTTP-MCP needs the trait for; not a hard build-time dep, but ordering keeps the workplan coherent).

**Soft-flag-ready territory**:
- The `async_trait` vs. manual-Pin-Box decision is task-time judgment. If `async_trait` is already in the dep graph (verify with `cargo tree | grep async_trait`), use it; otherwise the manual-Pin-Box shape is acceptable. Surface as a `next-task-agent` soft flag for Task 12.3 if the choice affects how the in-process impl is written.
- The `envelope_from_anyhow` refactor may need to introspect `base_url` for the `daemon_unreachable` envelope's message ("`<configured URL>` did not respond: ..."). The in-process backend has no base URL — this is the tension the trait must resolve. Two options: (a) `base_url` is a trait method that returns `Option<&str>` (None for in-process); (b) `daemon_unreachable` is irrelevant for in-process (the daemon is always reachable from itself), so the in-process impl returns `false` from `is_connect_error` and `envelope_from_anyhow` never synthesizes the unreachable envelope for it. Default to (b); surface as `next-task-agent` if (a) reads cleaner at task time.
- Existing `tests/mcp.rs` may directly construct `DaemonClient` and pass it bare to `HypomnemaMcpServer { client: ... }`. The refactor wraps with `Arc::new(...)`. If `tests/mcp.rs` has helper functions that construct the server, refactor the helpers; surface as `coordinator-only` if the test surface is non-trivial.

### Task 12.3 — `InProcessBackend` full implementation

**Risk**: medium-high. The first non-`DaemonClient` implementation of `HypomnemaBackend`. Each method calls the daemon's substrates directly — `VaultManager` for vault ops, the cross-vault search helpers for search ops. The blast radius depends on how cleanly the existing `src/api/search.rs` and `src/api/vaults.rs` handlers separate business logic from HTTP framing.

**Scope**:

- **Define `InProcessBackend`** in a new module `src/mcp/backend_in_process.rs` (or as a sub-module of `src/mcp/backend.rs` — task-time decision):

  ```rust
  pub struct InProcessBackend {
      pub vault_manager: Arc<VaultManager>,
      pub embedder: Arc<dyn Embedder>,
      // ... whatever else the search handlers consume
  }
  ```

  Construct from `ApiState` (or its component Arcs) in `src/bin/hmnd.rs`'s daemon-startup path.

- **Implement the 12 methods**:
  - **Search** (3 methods): each method calls the existing cross-vault fan-out logic in `src/api/search.rs`. The current `pub async fn filesystem(State(s): State<ApiState>, Json(input): Json<FilesystemQueryJson>) -> Result<Json<FilesystemSearchResponse>, ApiError>` shape is axum-handler-coupled; **extract a free function** like `pub async fn run_filesystem_search(manager: &VaultManager, input: &FilesystemQueryJson) -> Result<FilesystemSearchResponse, ApiError>` that both the axum handler and `InProcessBackend::search_filesystem` call. The error type is `ApiError` (or whatever the search handlers use); convert to `anyhow::Error` at the trait boundary.
  - **Vault read** (2 methods): `list_vaults` calls `VaultManager::list()` (or equivalent — verify the exact method); `get_vault` calls `VaultManager::get(name_or_id)`.
  - **Vault write** (7 methods): each calls the corresponding `VaultManager::{create, terminate, pause, resume, reset, rename, rescan}` method per `docs/specs/vault-management.md` § Operations. Argument shapes match the trait signatures.
  - **`is_connect_error`**: returns `false`. The in-process backend cannot fail a connect — there is no connection.

- **Error mapping**: `ApiError` (or whichever error type the handlers produce) → `anyhow::Error` for the trait return shape. The MCP server's `envelope_from_anyhow` already handles the structured-error mapping; the trait return is `anyhow::Result<...>` so the existing envelope path applies unchanged. Verify at task time that `ApiError` carries enough context to produce a useful structured envelope (`ApiError::VaultNotFound` should round-trip through `anyhow::Error::msg(...)` → `envelope_from_anyhow` → `vault_not_found` code) — if not, extend the trait return with a richer error type or wire a custom anyhow extension. **Default**: anyhow round-trip works for round 4; a richer error type is round-5+ if it surfaces as friction.

- **Free-function extraction in `src/api/search.rs` and `src/api/vaults.rs`**: each axum handler keeps its current signature but the body delegates to a `pub(crate) async fn run_<op>(...) -> Result<<resp>, ApiError>` free function. The handler wraps the result in `Json(...)`. **No behavioral change** — the handler is structurally a one-liner around the free function. `InProcessBackend` calls the free function directly. Verify at task time that the handler-coupled logic doesn't reach for `axum::extract::State` outside the handler — if it does, the State extraction stays in the handler and the free function takes the unpacked arguments.

- **Test surface**: unit tests for each `InProcessBackend` method against a fixture VaultManager + sample vault. Reuse the round-3 test harness in `src/control_plane/tests.rs` (pattern: in-memory or temp-dir-backed VaultManager).

**Tests** (new module `src/mcp/backend_in_process.rs::tests` or extend `src/mcp/backend.rs::tests`):

- `in_process_search_filesystem_returns_same_shape_as_http_handler` — construct an `InProcessBackend` and an `ApiState` against the same VaultManager; call both; assert response shapes are byte-equal.
- `in_process_search_content_returns_same_shape_as_http_handler` — same, for content search.
- `in_process_search_semantic_returns_same_shape_as_http_handler` — same, for semantic search. May require a mock embedder — reuse the round-2 step-7 / round-3 test harness pattern.
- `in_process_list_vaults_returns_registry_rows` — populate registry, call list_vaults, assert all rows.
- `in_process_get_vault_returns_404_envelope_for_unknown` — get_vault on unknown name; assert error round-trips with `vault_not_found` code.
- `in_process_vault_lifecycle_round_trip` — create → pause → resume → rescan → terminate; assert each method returns the spec response shape.
- `in_process_is_connect_error_always_false` — sanity check.

**Files touched**:
- New: `src/mcp/backend_in_process.rs` (or sub-module of `backend.rs`).
- Edited: `src/api/search.rs` (extract `run_*` free functions), `src/api/vaults.rs` (extract `run_*` free functions if needed; vault handlers may already be thin enough that the trait method calls VaultManager directly without extraction).
- Edited: `src/mcp/mod.rs` (re-export `InProcessBackend`).

**Dependencies**: 12.2.

**Soft-flag-ready territory**:
- The free-function extraction in `src/api/search.rs` may surface ripples into request validation, error mapping, and partial-results envelope construction. These were spec-pinned in step 10 (cross-vault semantics) and step 11 (lifecycle ops); the extraction shouldn't change behavior. Surface as `coordinator-only` if any sub-extraction reveals a deeper-than-anticipated coupling.
- `Embedder` is an `Arc<dyn Embedder>` in the daemon's dep graph; verify at task time that `InProcessBackend` constructs cleanly with a clone. If `dyn Embedder` requires more setup (e.g., the embedding-client's HTTP timeouts, the embedding-service connection pool) than the trait surface suggests, surface as `next-task-agent` for Task 12.4 (which constructs the backend at startup).
- Vault-management response types (`VaultRowJson`, `RescanResponseJson`, `TerminateVaultResponse`, `VaultListResponse`) live in `src/api/types.rs` and are already serde + schemars-derived from step 10/11. The trait methods return those types directly — no new serde shapes. If any vault-management method on `VaultManager` returns a richer internal type that needs converting to the JSON shape, surface as `coordinator-only` (a routine handler-internal conversion).

### Task 12.4 — `[mcp.http]` config + HTTP-MCP route mount + Origin middleware + rmcp feature

**Risk**: medium-high. **The wiring task.** Mounts a new transport on the daemon's HTTP listener. Per the round-1/2/3 precedent (now 6-of-6), wiring tasks are the natural smoke point — but smoke for this round lives in Task 12.7. Task 12.4's quality gate is unit tests + cargo build against the rmcp feature.

**Scope**:

- **Cargo.toml**: add `transport-streamable-http-server` to the existing `rmcp` features list (Resolution A). Run `cargo build` to verify the feature exists at rmcp 1.5.0 and pulls in compatible transitive deps. **No top-level crate addition.**

- **Config struct** in `src/config.rs`: add `[mcp.http]` block. Pin the exact field names in the existing `Mcp` config struct:

  ```rust
  // Sketch — verify at task time against existing Mcp struct shape
  #[derive(Debug, Deserialize, Serialize, Clone)]
  pub struct McpHttpConfig {
      #[serde(default = "default_mcp_http_enabled")]
      pub enabled: bool,
      #[serde(default = "default_mcp_http_path")]
      pub path: String,
  }
  fn default_mcp_http_enabled() -> bool { true }
  fn default_mcp_http_path() -> String { "/mcp".to_string() }
  ```

  Wire `http: McpHttpConfig` as a field on `Mcp`. Default both fields per Resolution E.

- **Config validation** in `Config::validate()` (or wherever startup validation lives): if `config.mcp.http.path != "/mcp"`, return a structured error with the exact message `mcp.http.path must be "/mcp" in this version of Hypomnema`. Per Resolution E.

- **New module `src/api/mcp_http.rs`**:
  - `pub fn router(state: McpHttpState) -> Router<()>` — returns the axum sub-router that mounts the `/mcp` route (or returns an empty router if `enabled = false`; the daemon-level wiring decides whether to attach).
  - **Mount shape**: try `Router::nest_service("/mcp", StreamableHttpService::new(factory, manager.into(), config))` first per Resolution A. If `cargo build` fails on axum 0.7 / rmcp 1.5 incompatibility (e.g., a `Service::Error` bound mismatch), fall back to the hand-rolled axum handler form per Resolution A. Document the choice (and the diagnostic, if any) in the task's results comment.
  - **Service factory**: `StreamableHttpService::new` takes a factory `impl Fn() -> impl ServerHandler + Clone` (verify exact bound at task time against rmcp 1.5 docs). The factory constructs `HypomnemaMcpServer` with `backend: Arc<InProcessBackend>` — the in-process backend constructed once at startup and shared via `Arc`.
  - **Origin-validation middleware**: a hand-rolled axum middleware (`tower::Layer` or `axum::middleware::from_fn`). Reads the `Origin` request header; if present and not in the allow-list (`null`, `http://localhost`, `http://localhost:<port>`, `http://127.0.0.1`, `http://127.0.0.1:<port>`, `http://[::1]`, `http://[::1]:<port>`), returns HTTP 403 with body `Origin not allowed: <value>`. If absent, accept (curl, non-browser clients). The middleware applies only to the `/mcp` route, not to other routes. Reuse the existing axum middleware pattern in `src/api/` if any (verify at task time; if absent, this is the first middleware in the project).

- **`hmnd` startup wiring** in `src/bin/hmnd.rs`:
  - Construct `InProcessBackend` from `ApiState`'s component Arcs (after the `vault_manager` and embedder are constructed at lines ~109–118).
  - If `config.mcp.http.enabled`, construct the HTTP-MCP router via `mcp_http::router(...)` and merge into the main `app: Router` via `Router::merge(...)` (or `app.nest_service("/mcp", ...)` if mount is via nest_service). Verify the existing router-construction shape at task time — `api::router(api_state)` already builds the full router; either extend it or merge a parallel router.
  - Log at startup: `tracing::info!(path = %config.mcp.http.path, enabled = config.mcp.http.enabled, "hmnd: mcp http transport mounted")` (or similar — match the existing tracing style in `src/bin/hmnd.rs:137–141`).

- **Stdout-reservation invariant DOES NOT apply** (per ADR-0012 § Resolution 4 + per spec § Implementation Notes): the stdout reservation is for stdio MCP only; HTTP MCP uses HTTP framing and SSE, with logging on hmnd's existing tracing setup. Verify Task 12.4 doesn't introduce any `println!` or other stdout writes in the MCP route handler.

**Tests** (new tests in `src/api/tests.rs` or a new `src/api/mcp_http.rs::tests` module):

- **Origin validation unit tests** (axum's `tower::ServiceExt::oneshot` pattern):
  - `mcp_http_origin_loopback_v4_accepted` — POST with `Origin: http://127.0.0.1:7777`; assert 200 (or whatever rmcp returns for an unknown method, but NOT 403).
  - `mcp_http_origin_loopback_v6_accepted` — POST with `Origin: http://[::1]:7777`; assert not 403.
  - `mcp_http_origin_localhost_accepted` — POST with `Origin: http://localhost`; assert not 403.
  - `mcp_http_origin_null_accepted` — POST with `Origin: null`; assert not 403.
  - `mcp_http_origin_missing_accepted` — POST with no `Origin` header; assert not 403.
  - `mcp_http_origin_remote_rejected` — POST with `Origin: http://example.com`; assert 403 with body containing `Origin not allowed: http://example.com`.
- **Config validation unit tests** (in `src/config.rs::tests`):
  - `mcp_http_path_default_accepts` — config with `mcp.http.path = "/mcp"`; assert validation succeeds.
  - `mcp_http_path_other_rejected` — config with `mcp.http.path = "/foo"`; assert validation produces the exact error message per Resolution E.
  - `mcp_http_enabled_default_true` — minimal config; assert `mcp.http.enabled == true`.
  - `mcp_http_enabled_explicit_false` — config with `mcp.http.enabled = false`; assert preserved.
- **Disabled path test**:
  - `mcp_http_disabled_returns_404` — start a daemon with `mcp.http.enabled = false`; POST to `/mcp`; assert 404 (axum's standard not-found). `/search/filesystem` continues to serve.
- **rmcp feature build verification** is implicit in `cargo build` (the feature flag missing → compile error).

**Files touched**:
- New: `src/api/mcp_http.rs`.
- Edited: `Cargo.toml` (rmcp features list — add one feature flag), `src/config.rs` (add `McpHttpConfig`; extend `Mcp` struct; add validation), `src/api/mod.rs` (mount the `/mcp` sub-router conditionally), `src/api/tests.rs` (new tests), `src/bin/hmnd.rs` (construct `InProcessBackend` + mount HTTP-MCP router conditionally on `mcp.http.enabled`).

**Dependencies**: 12.3 (constructs InProcessBackend from this task's wiring).

**Soft-flag-ready territory**:
- The `nest_service` vs. hand-rolled-handler decision is task-time. If `nest_service` doesn't compose cleanly, surface as `next-task-agent` for Task 12.5 (the test scaffolding may need to construct the service differently). Default-shape: `nest_service` per the proposal's rmcp example.
- `StreamableHttpService::new` signature in rmcp 1.5 is verified at task time. If the factory bound differs from the canonical example (e.g., requires `impl Service` rather than `impl ServerHandler` factory), surface as `coordinator-only` and adjust.
- The `Origin: null` allowance is per the MCP spec's recommendation (browser sandbox-Iframe / file:// origins). If a security-conscious operator surfaces in the round-4 boundary review wanting strict no-`null`, that's a future-amendment trigger; for v1, accept `null`.
- Origin-list parsing: the spec is loopback origins only (`http://localhost[:port]`, `http://127.0.0.1[:port]`, `http://[::1][:port]`). Strict regex match vs. parsed-URL hostname check is task-time judgment. **Default**: parsed URL via `http::Uri` (accept any port; reject any non-loopback host); surface as `next-task-agent` if a hand-rolled string match reads cleaner.

### Task 12.5 — Integration tests + story coverage over multi-vault fixture

**Risk**: medium-high. **Integration tests are the primary regression net for the round-4 shipping criteria.** The five stories in `notes/proposals/archive/mcp-streamable-http-stories.md` (post-12.1 archive) translate directly to test cases here. Manual smoke is **Task 12.7** — this task's gate is automated tests + 3× flake-check.

**Scope**:

- **New `tests/mcp_http.rs`** integration test file. Reuse the round-3 multi-vault test fixture from `tests/vault_control_plane.rs` (extract a helper module if convenient — `tests/common/multi_vault.rs` or similar; task-time decision).

- **Test cases** (one or more per story):

  **Story 1 — Agent host invokes a search tool over HTTP-MCP**:
  - `initialize_returns_serverinfo_hypomnema` — POST `http://127.0.0.1:<port>/mcp` with a valid JSON-RPC `initialize`; assert HTTP 200, response body parses as JSON-RPC success, `result.serverInfo.name == "hypomnema"`, `result.serverInfo.version == env!("CARGO_PKG_VERSION")`.
  - `tools_list_returns_all_twelve_tools` — POST `tools/list`; assert 12 tools: `search_filesystem`, `search_content`, `search_semantic`, `vault_list`, `vault_status`, `vault_create`, `vault_pause`, `vault_resume`, `vault_reset`, `vault_rename`, `vault_rescan`, `vault_terminate`. Names match the canonical names from ADR-0004 + vault-management.md § MCP Tool Surface.
  - `tools_call_search_filesystem_matches_http` — for a known fixture vault, POST `tools/call` for `search_filesystem` with a fixture query; in the same test process, POST `/search/filesystem` with the equivalent request; assert the `structuredContent` field of the MCP response is the same JSON as the HTTP `/search/filesystem` body. Per Story 1's stricter version: "test computes the expected response by issuing the equivalent `/search/filesystem` request from the same test process — not by hand-encoding the expected payload."
  - `tools_call_search_content_matches_http` — same equivalence shape, content search.
  - `tools_call_search_semantic_matches_http` — same equivalence shape, semantic search. Mock embedder per the round-2 / round-3 test harness.
  - `tools_call_vault_list_matches_http` — equivalence with `GET /vaults`.
  - `tools_call_vault_status_matches_http` — equivalence with `GET /vaults/{name_or_id}` for a known vault.
  - `tools_call_vault_pause_then_resume_round_trip` — equivalence with the round-3 step-11 lifecycle round-trip; ensures HTTP-MCP can drive write tools when `enable_write_tools = true`.
  - `mcp_http_disabled_returns_404` (already covered in Task 12.4 unit; restate at integration level if it's not redundant).

  **Story 2 — Origin validation**:
  - `origin_remote_rejected_403` — POST with `Origin: http://example.com`; assert 403, body contains `Origin not allowed: http://example.com`. Verify no tool-call audit log entry is emitted (tracing-test helper or simply absence of the daemon's per-call log line).
  - `origin_loopback_variants_accepted` — POST with each of `http://localhost`, `http://localhost:1234`, `http://127.0.0.1`, `http://127.0.0.1:7777`, `http://[::1]`, `http://[::1]:7777`, `null`, and missing `Origin`; assert no 403 (the actual response code depends on the JSON-RPC body — `initialize` returns 200, malformed bodies return whatever rmcp returns, which is fine — the test just asserts not-403).
  - **Negative-fingerprint grep**: `rg 'allow_any_origin|cors_allow_all|Access-Control-Allow-Origin: \*' src/` returns zero matches in the MCP handler module. Codify as `tests/mcp_http.rs::origin_negative_fingerprint_grep` using `std::process::Command` on `rg` (or skip the runtime grep and rely on a CI grep — task-time decision; the test-suite-internal grep is more robust against rebase drift).

  **Story 3 — Trust-posture inheritance**:
  - `mcp_http_no_authorization_required` — POST with no `Authorization` header; assert response is normal (matching `/search/*` behavior).
  - `mcp_http_with_authorization_header_proceeds_normally` — POST with a deliberately-malformed `Authorization: Bearer fake-token`; assert response is processed (matching `/search/*` ignoring `Authorization` in v1).
  - **Negative-fingerprint greps**: `rg 'reqwest::Client|hyper::Client|TcpListener::bind' src/api/mcp_http*.rs` returns zero matches; `rg 'rustls|server_config|ServerCertVerifier|verify_token|api_key' src/api/mcp_http*.rs` returns zero matches. Codify as test-suite internal greps per Story 2's pattern.

  **Story 4 — Graceful shutdown with open SSE**:
  - `sigterm_with_open_sse_closes_stream_cleanly` — open a GET `/mcp` with `Accept: text/event-stream`; while the stream is open, send the daemon-wide shutdown signal (the test harness's mock-of-SIGTERM equivalent, e.g., dropping the `shutdown_tx::Sender` per the existing test pattern in `tests/`); assert the SSE stream closes within a bounded timeout (e.g., 2 seconds — matches the existing graceful-shutdown timeout).
  - `mcp_http_handler_module_has_no_signal_handling` — codify as a test-suite-internal grep `rg 'select!|tokio::signal' src/api/mcp_http.rs` returns zero matches (the shutdown path is the daemon-wide one; the MCP route handler doesn't have its own signal handling per Story 4's third criterion).

  **Story 5 — Stdio + HTTP-MCP coexistence**:
  - `stdio_and_http_serve_same_tools_list` — start a daemon with HTTP-MCP enabled; spawn an `hmn mcp` subprocess pointing at the same daemon; both initialize; both `tools/list`; assert tool-list arrays are identical (same names, same descriptions, same input schemas).
  - `stdio_and_http_brand_identity_match` — both transports return `serverInfo.name == "hypomnema"`.
  - `concurrent_calls_dont_block_each_other` — issue 5 parallel `tools/call`s on stdio while issuing 5 parallel calls on HTTP-MCP; assert all 10 complete within a reasonable timeout (e.g., 10 seconds for a 100-result fixture); per-call latency similar with and without the parallel load.

- **3× consecutive flake-check** clean run on the new tests (`cargo test --test mcp_http` × 3) — round-1/2/3 anti-flake convention. Document the run in the task's results comment.

- **Negative-fingerprint grep helpers** — choose between in-test `std::process::Command::new("rg")` invocations or codifying the asserts as documentation-only (operator-side `cargo run --bin grep-checks` or similar). **Default**: in-test, so the assertions ride the `cargo test` cadence and don't drift; if `rg` isn't always on the test runner's PATH, use `grep -E` as a fallback.

**Files touched**:
- New: `tests/mcp_http.rs`, possibly `tests/common/multi_vault.rs` (helper extraction if Task 12.5 task-agent finds the existing `tests/vault_control_plane.rs` setup is too coupled to copy-paste).
- Edited: possibly `tests/vault_control_plane.rs` (refactor to extract the helper); verify at task time whether the surface is non-trivial.

**Dependencies**: 12.4.

**Soft-flag-ready territory**:
- The pre-existing outbox flake (`tests/outbox.rs::rename_emits_deleted_then_created_lines`) may surface in the full-suite quality-gate sweep. Coordinator-only soft flag: scope is round-5+ flake-hardening pass; not step-12's surface. Per the silence-as-data rule, if the flake doesn't reproduce here either, that's data for the round-4 retro.
- The `initialize` JSON-RPC response shape from `rmcp::transport::streamable_http_server` is verified at task time against the rmcp 1.5 example; the test asserts on the structurally-relevant fields (`serverInfo.name`, `serverInfo.version`) and tolerates the rest.
- SSE close-on-shutdown timing is a test-harness coordination point. If the existing `with_graceful_shutdown` integration takes longer than 2s in tests (e.g., due to the consumer-task drain), the test timeout extends; surface as `coordinator-only`.
- Story 5 `concurrent_calls_dont_block_each_other` is the closest test to a real load test; if it surfaces a real concurrency issue under the 5+5 parallel-call shape, that's a task-time escalation candidate per the round-3-step-6 act-now-vs-defer-to-boundary decision rule.

### Task 12.6 — Manual-testing runbook refresh (Option A)

**Risk**: low. Doc-only by design (no production code touched). Refreshes `notes/manual-testing/` from "step 8" reality to round-4 reality. Lands before Task 12.7 so the smoke matrix can drive against the refreshed runbook rather than ad-hoc commands.

**Scope** (per [`roadmap-4.md`](./roadmap-4.md) § Manual-testing drift Option A):

- **Add a second fixture vault** at `notes/manual-testing/fixtures/sample-vault-2/` with distinguishable content (e.g., a different topic area, distinct file names, separate `README.md`). Update `fixtures/README.md` (or add `fixtures/README-2.md` adjacent) with the expected-results contract for the second vault. The two vaults exist to exercise multi-vault behavior end-to-end (cross-vault search, partial-results diagnostic, vault-management ops).

- **Refresh `00-setup.md`**:
  - Replace v0 single-vault `[vault] = "..."` config with the multi-vault flow: empty `vaults.sqlite`, then `hmn vault create --name=sample ~/path/to/sample-vault` and `hmn vault create --name=sample-2 ~/path/to/sample-vault-2`. Note `default_vault_name = "sample"` for the runbook's default-target ergonomics.
  - Update the install steps for shipped reality (sqlite-vec extension, TEI sidecar, etc. — anything that's drifted since step 8).

- **Refresh 01-04 sections** for shipped reality:
  - `01-running-the-daemon.md` — covers `hmnd` startup with the multi-vault registry; `/health` and `/status` now show vault counts; `hmn status` shows the multi-vault summary.
  - `02-watcher-and-outbox.md` — per-vault outbox at `<data_dir>/vaults/<id>/outbox.jsonl`; multi-vault watcher behavior.
  - `03-search.md` — cross-vault search by default; `--vaults` filter; partial-results diagnostic when a vault is paused/errored.
  - `04-mcp.md` — `hmn mcp` (stdio MCP) updated for the round-3 vault-management tool surface (12 tools total). Cross-reference the new `06-mcp-http.md`.

- **New `05-vault-management.md`**: covers all 9 lifecycle ops via `hmn vault {create,list,status,pause,resume,reset,rename,rescan,terminate}`. Examples + expected outputs against the two fixture vaults.

- **New `06-mcp-http.md`** (the round-4 transport runbook):
  - Starting the daemon with `mcp.http.enabled = true` (the default).
  - curl + JSON-RPC `initialize` to `http://127.0.0.1:7777/mcp`; expected `serverInfo.name == "hypomnema"`.
  - curl + JSON-RPC `tools/list`; expected 12 tools.
  - curl + JSON-RPC `tools/call` for each of `search_filesystem`, `search_content`, `search_semantic`, `vault_list`, `vault_status` against the multi-vault fixture; expected response shapes.
  - Origin validation: `curl -H 'Origin: http://example.com' ...` → 403.
  - Disabling HTTP-MCP: `mcp.http.enabled = false`; `/mcp` returns 404; `/search/*` continues to serve.
  - Optional: an MCP-HTTP-capable client (Iris, or a browser-hosted host) initializes and invokes a tool — left as an operator-supplied step if Iris isn't available locally.

- **Update `README.md`**:
  - Bump the title from "step 8" to "step 12 / round 4".
  - Update the surface-coverage table: mark `hmn vault …` subcommands ✅ (round 3); add row for HTTP-MCP transport ✅ (round 4 — `06`).
  - Update the version-skew warning: through round 4, docs and shipped code align (configuration.md and cli.md describe what's actually shipped; there's no longer a future-state preview).

**Tests**: doc-only; no code tests. Verify the refreshed runbook is internally consistent (cross-references resolve; expected outputs are plausible against the fixtures).

**Files touched**:
- New: `notes/manual-testing/fixtures/sample-vault-2/` (a small vault with ~10 markdown files), `notes/manual-testing/05-vault-management.md`, `notes/manual-testing/06-mcp-http.md`.
- Edited: `notes/manual-testing/README.md`, `notes/manual-testing/00-setup.md`, `notes/manual-testing/01-running-the-daemon.md`, `notes/manual-testing/02-watcher-and-outbox.md`, `notes/manual-testing/03-search.md`, `notes/manual-testing/04-mcp.md`, `notes/manual-testing/fixtures/README.md`.

**Dependencies**: none functional. Can run in parallel with 12.2–12.5 in principle (it's doc-only); coordinator may sequence after 12.4 so the runbook can describe the actual `[mcp.http]` config block as it landed. **Default sequencing**: after 12.4, before 12.7.

**Soft-flag-ready territory**:
- The fixture-vault content choice (which files, what topic, what queries to demonstrate) is task-time judgment; default to a small, varied set — README.md, a few notes, one frontmatter-rich file. Surface as `coordinator-only` if the fixture engineering balloons.
- If the existing `notes/manual-testing/04-mcp.md` references prose that's drifted further than a refresh can patch (e.g., references commands that don't exist anymore), it may be cleaner to rewrite from scratch than to edit. Surface as `coordinator-only` with the chosen approach.

### Task 12.7 — Manual smoke matrix (round-4 shipping gate)

**Risk**: medium-high. **Manual smoke verification is load-bearing here** per the round-1/2/3 precedent (now 6-of-6 for medium-high-risk wiring tasks). Composes 12.1–12.6 into the round-4 shipping gate test matrix. The roadmap specifies this as "exercising the HTTP-MCP transport from a real MCP-HTTP-capable client (Iris, browser-hosted host, or curl + JSON-RPC) against a daemon serving multiple vaults — not just the loopback Rust integration test."

**Scope**: run the refreshed `notes/manual-testing/06-mcp-http.md` runbook end-to-end against a multi-vault daemon, document each step's transcript verbatim in the task's results comment per the round-2/3 precedent.

**Smoke matrix** (each item documented in the task's results comment with the full transcript):

1. **Multi-vault setup**: empty `<data_dir>`. Daemon starts, idles. Create vault A (small — sample-vault), vault B (medium — sample-vault-2). Wait for indexing convergence.
2. **Default daemon startup with HTTP-MCP enabled**: verify `/mcp` mounts; daemon log shows `mcp http transport mounted` (or similar) at info level; curl + JSON-RPC `initialize` returns 200 with `serverInfo.name == "hypomnema"` and `serverInfo.version == 0.3.0` (the round-4 ship version — verify against `Cargo.toml` post-bump).
3. **`tools/list` over HTTP-MCP**: curl returns all 12 tools with the canonical names and descriptions.
4. **Search-tool equivalence**: for each of `search_filesystem`, `search_content`, `search_semantic`, run `tools/call` against the multi-vault daemon; cross-check by issuing the equivalent `/search/*` POST. Diff the JSON bodies; assert byte-equal (modulo `partial_results` ordering, which is typically empty for an all-active multi-vault setup).
5. **Vault-management equivalence**: `vault_list` matches `GET /vaults`; `vault_status` for each vault matches `GET /vaults/{id}`; `vault_pause` then `vault_resume` round-trips cleanly; outbox tail on the affected vault shows the lifecycle events.
6. **Origin validation**: `curl -H 'Origin: http://example.com' -X POST http://127.0.0.1:7777/mcp -d '{...initialize...}'` → 403, body `Origin not allowed: http://example.com`. Repeat with `Origin: http://localhost:1234`, `Origin: http://127.0.0.1`, `Origin: null`, no Origin header — all accepted.
7. **Stdio + HTTP coexistence**: spawn `hmn mcp` against the same daemon; from the agent host, run `tools/list` over stdio; from a parallel terminal, run `tools/list` over HTTP-MCP; assert outputs match. Run `tools/call search_filesystem` on each transport sequentially; both succeed.
8. **Graceful shutdown with open SSE**: open a GET `/mcp` with `Accept: text/event-stream` (curl `--no-buffer` or `httpie --stream`); while the stream is open, SIGTERM the daemon; verify the SSE stream closes cleanly within ~2s; verify subsequent `hmnd` start binds the same `config.http.bind` without an "address in use" error.
9. **`mcp.http.enabled = false`**: stop the daemon; edit config; restart; verify `/mcp` returns 404; verify `/search/*` continues to serve normally (regression check).
10. **`mcp.http.path` rejection**: edit config to `mcp.http.path = "/foo"`; restart daemon; verify daemon exits non-zero at startup with the message `mcp.http.path must be "/mcp" in this version of Hypomnema`.
11. **(Optional, only if available) Iris integration**: configure Iris (or another MCP-HTTP-capable host) to point at `http://127.0.0.1:7777/mcp`; run a search and a vault-list call; verify the host reports both successfully.
12. **Negative-fingerprint grep verification**: from the repo root, run `rg 'allow_any_origin|cors_allow_all|Access-Control-Allow-Origin: \*' src/`, `rg 'reqwest::Client|hyper::Client|TcpListener::bind' src/api/mcp_http*.rs`, `rg 'rustls|server_config|ServerCertVerifier|verify_token|api_key' src/api/mcp_http*.rs` — all return zero matches. Document the grep output in the results comment.
13. **Quality-gate sweep**: `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, all green; 3× consecutive flake-check on `cargo test` (or at minimum `cargo test --test mcp_http`).

Document each smoke step's transcript verbatim in the task's results comment per the round-2/3 precedent.

**Files touched**: none — this task runs the smoke matrix and documents results. Any code-level fixes that surface during smoke land as their own commit (or a Task 12.7 follow-up commit if the issue is fixable inside the smoke session).

**Dependencies**: 12.5 (integration tests), 12.6 (refreshed runbook).

**Soft-flag-ready territory**:
- The pre-existing outbox flake (`tests/outbox.rs::rename_emits_deleted_then_created_lines`) — silence-as-data per the round-3 step-11 retro pattern. If it doesn't reproduce in step 12's quality-gate sweep, that's data for the round-4 retro.
- If smoke step 11 (Iris integration) reveals a real client-side incompatibility (e.g., Iris expects a `Mcp-Session-Id` header that v1 doesn't issue), surface as `coordinator-only` for the round-4 retro: it's a future-amendment trigger, not a step-12 escalation.
- Smoke transcripts are inline in the results comment per the round-2 step-8 / round-3 step-9–11 precedent. If a smoke transcript exceeds the comment's reasonable size budget, link to a scratchpad with the full transcript and inline the salient excerpts.
- If smoke step 8 (graceful shutdown) reveals a regression from round-3 (e.g., the SSE close timing is slow because StreamableHttpService doesn't observe the watch-channel propagation cleanly), surface as a task-time escalation candidate per the round-3-step-6 act-now-vs-defer-to-boundary decision rule.

### Task 12.8 — Reference docs verify + roadmap-4 status + boundary prep

**Risk**: low. Doc-only by design; lands at boundary so any forward-noted soft-flag corrections from earlier tasks can be incorporated. Includes the round-4-shipping-gate boundary preparations (version-bump notes, round-4 archival notes, round-5 backlog seed).

**Scope**:

- **Verify `docs/reference/configuration.md`** `[mcp.http]` block from Task 12.1 is consistent with what shipped from Task 12.4. Apply the round-3-step-10 "soft-flag self-correction at boundary" pattern: read the current file, compare against shipped reality, only edit if drift is real.

- **Verify `docs/architecture/overview.md` § Search API "three transports"** framing from Task 12.1 is consistent with shipped reality. Same verify-before-editing pattern.

- **Verify `docs/specs/mcp-streamable-http.md`** v1.0.0 from Task 12.1 is consistent with shipped reality. If any concrete behavior differs from the spec (e.g., the spec says `Origin: null` is accepted but shipped code rejects it — anticipated to be N/A; this is a defensive verification), reconcile by amending the spec and bumping to v1.0.1 with a § Revision History entry.

- **Update `notes/roadmap/roadmap-4.md` § Step 12 status**:
  - Add `**Status**: Shipped <date>` at top of Step 12 section.
  - Cross-reference the workplan archive path: `notes/roadmap/archive/step-12-workplan.md`.
  - **Round-4 shipping note**: this is the round-4 shipping gate; the round-4 boundary moves the round-4 roadmap to `notes/roadmap/archive/roadmap-4.md` per the round-1/2/3 archival precedent.

- **Update `notes/backlog.md`**:
  - Round-4-candidates entries that didn't ship in this round move to round-5-candidates (or stay in the unprefixed candidates list; coordinator decides at task time based on whether the human wants to commit to a round-5 boundary now).
  - Add any round-4 boundary follow-ups surfaced by Tasks 12.1–12.7's soft flags.
  - The "MCP Streamable HTTP transport" entry was retired by Task 12.1; verify it's not present.

- **Verify `notes/manual-testing/` runbook** is consistent with shipped reality (Task 12.6 already did the bulk; this is a verify-before-editing pass per the round-3-step-10 pattern). If any commands in `06-mcp-http.md` need to update for shipped reality (e.g., the exact error message in mcp.http.path validation differs from what 12.6 wrote), reconcile.

- **Version-bump prep note**: `Cargo.toml` is at `version = "0.2.0"` at workplan-write. Round-4 shipping gate aligns with `v0.3.0` per the boundary ritual call. The version bump itself is a boundary-ritual action (alongside the milestone tag), not a Task 12.8 action — Task 12.8 prepares the docs to be consistent with `v0.3.0` (release notes-style content can live in a CHANGELOG or in the round-4 archival note; coordinator decides at task time based on whether the project has adopted a CHANGELOG.md yet — round-3 boundary flagged this; round-4 boundary is another natural moment to settle).

**Tests**: doc-only; no code tests in this task. `cargo doc --no-deps` runs cleanly post-edit.

**Files touched**: `notes/roadmap/roadmap-4.md`, `notes/backlog.md`, possibly `docs/reference/configuration.md`, `docs/architecture/overview.md`, `docs/specs/mcp-streamable-http.md` (verify-then-amend per the soft-flag self-correction rule). The workplan archive itself and the round-4 archival are part of the post-task boundary ritual run by the coordinator after this task ships.

**Dependencies**: 12.1–12.7. Lands last.

**Soft-flag-ready territory**:
- Forward-noted soft-flag reconciliations from earlier tasks (likely 3–5 of them per the round-3 stable pattern). Apply the round-3-step-10 "soft-flag self-correction at boundary" rule: verify the prose is current before editing; the prior task's observation may have been the drift.
- Version-bump policy is a workplan-time decision left for the boundary ritual (the human + coordinator decide together). If `v0.3.0` is the call, Task 12.8 records the call in roadmap-4.md's § After round 4 section; if a different version policy emerges, Task 12.8 picks up that signal.
- CHANGELOG.md adoption is a project-wide policy question that may or may not surface at this boundary; coordinator-only soft flag if it does (decides whether to start a CHANGELOG now or defer to round 5).

---

## Shipping criteria

The step ships when **all** of these hold:

- [ ] All step-9 / step-10 / step-11 integration tests pass unchanged: `tests/scan.rs`, `tests/watch.rs`, `tests/outbox.rs`, `tests/embedding.rs`, `tests/mcp.rs`, `tests/multi_vault_internal.rs`, `tests/vault_control_plane.rs`, plus skeleton/config tests. Existing single-vault, multi-vault, and stdio-MCP behavior is fully preserved.
- [ ] `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test` all green.
- [ ] POST `http://127.0.0.1:7777/mcp` with a valid JSON-RPC `initialize` returns HTTP 200, `serverInfo.name == "hypomnema"`, `serverInfo.version == <crate version>`.
- [ ] POST `tools/list` returns all 12 tools: `search_filesystem`, `search_content`, `search_semantic`, `vault_create`, `vault_list`, `vault_status`, `vault_pause`, `vault_resume`, `vault_reset`, `vault_rename`, `vault_rescan`, `vault_terminate` (the last 7 gated by `[mcp] enable_write_tools` per round-3 step-10's decision; HTTP-MCP inherits the same gating).
- [ ] POST `tools/call` for `search_filesystem`, `search_content`, `search_semantic` against a known fixture vault returns the same `*SearchResponse` JSON in `structuredContent` that POST `/search/{filesystem,content,semantic}` returns for an equivalent request. Test computes the expected response by issuing the equivalent `/search/*` request from the same test process — not by hand-encoding the expected payload — so the criterion would not pass if the MCP path returned a constant or a divergent fixture.
- [ ] Same equivalence holds for the 9 vault-management tools against the round-3 cross-vault test harness.
- [ ] POST with `Origin: http://example.com` returns HTTP 403 `Origin not allowed: http://example.com`. POST with loopback origins (`http://localhost[:port]`, `http://127.0.0.1[:port]`, `http://[::1][:port]`, `null`) or no `Origin` header is accepted.
- [ ] Setting `mcp.http.enabled = false` produces a daemon that responds 404 to `/mcp` while continuing to serve `/search/*` and `/vaults/*` (independently mountable).
- [ ] Setting `mcp.http.path` to anything other than `"/mcp"` produces a startup error with the exact message `mcp.http.path must be "/mcp" in this version of Hypomnema`.
- [ ] SIGTERM with an open SSE GET to `/mcp` closes the stream cleanly within the daemon's existing `with_graceful_shutdown` window. No new shutdown machinery added in the MCP route handler (verified by greppable absence of `select!` or signal handling in `src/api/mcp_http.rs`).
- [ ] Stdio MCP (`hmn mcp`) and HTTP-MCP run against the same daemon simultaneously; both return identical tool lists; tool calls from one transport don't block the other.
- [ ] Manual smoke matrix (Task 12.7) run before the round-4 shipping tag: at minimum the curl + JSON-RPC pass through `initialize` → `tools/list` → `tools/call` for each search tool + `vault_list` + `vault_status`; if available, a real MCP-HTTP-capable client doing the same. Smoke transcripts inline in the smoke task's results comment per round-3 precedent.
- [ ] Negative-fingerprint greps from the (archived) stories file pass:
  - `rg 'allow_any_origin|cors_allow_all|Access-Control-Allow-Origin: \*' src/` returns zero matches in the MCP handler module.
  - `rg 'reqwest::Client|hyper::Client|TcpListener::bind' src/api/mcp_http*.rs` returns zero matches.
  - `rg 'rustls|server_config|ServerCertVerifier|verify_token|api_key' src/api/mcp_http*.rs` returns zero matches.
- [ ] Origin-validation positive grep: `rg 'Origin' src/api/mcp_http*.rs` returns at least one match (the validator).
- [ ] Manual-testing runbook refreshed for shipped reality: multi-vault `00-setup.md` through `04-mcp.md`; new `05-vault-management.md` and `06-mcp-http.md`; `README.md` surface-coverage table marks `hmn vault …` and HTTP-MCP shipped.
- [ ] 3× consecutive flake-check clean run on `cargo test` (matching round-1/2/3 anti-flake convention).
- [ ] Spec promoted to `docs/specs/mcp-streamable-http.md` v1.0.0 with all 5 open questions resolved per Resolutions A–E. Stories file archived. ADR-0013 written; ADR-0012 + ADR-0008 amended. Architecture overview lists three MCP transports. `[mcp.http]` config block documented in `docs/reference/configuration.md`. `notes/backlog.md` MCP-Streamable-HTTP entry retired.
- [ ] `notes/roadmap/roadmap-4.md` § Step 12 marked shipped; backlog has any round-4 boundary follow-ups; round-4 archival prep noted.
- [ ] One commit per task per the playbook (Task 12.7's smoke can use the round-3 step-9 / step-10 / step-11 single-commit-with-inline-transcripts pattern).

---

## Step boundary follow-ups (anticipated)

- **CORS for browser-hosted hosts** (Resolution D — deferred): when a concrete browser-hosted host consumer surfaces, decide what to allow. `tower-http::cors` is the natural future shape. Round-5+.
- **Resumable SSE / long-running tools** (Resolution C — deferred): when a tool needs server-pushed messages or progress. Round-5+.
- **Stateful sessions over HTTP-MCP** (Resolution B — deferred): when a tool needs session-scoped state across calls. Round-5+.
- **`mcp.http.path` configurability** (Resolution E — deferred): if reverse-proxy use cases surface. Round-5+.
- **MCP write-tool gating granularity** (carried forward from round-3 step-10): per-tool gating vs. the single `enable_write_tools` flag. Round-5+ if a use-case surfaces.
- **Outbox flake-hardening** (carried forward from rounds 2 + 3 — `tests/outbox.rs::rename_emits_deleted_then_created_lines`): silence-as-data through round 3 + (anticipated) round 4. If reproduces during step 12's quality-gate sweep, escalates from round-5-candidate to in-round investigation; if silent again, that's continued data for the silence-as-data record.
- **`flake.nix` dylib provisioning** (carried forward from step-6 boundary): operator prereq for sqlite-vec dylib remains in `docs/reference/configuration.md`. Round-5 candidate.
- **Cross-platform rename safety** (carried forward from step-9 boundary): documented same-filesystem assumption. If a Windows operator surfaces, revisit.
- **CHANGELOG.md adoption**: round-4 shipping gate is another natural moment to settle. Round-5 candidate if not adopted at boundary.
- **Multi-model embedding per vault**: spec § Open Questions; round-5+ if a use-case surfaces.
- **Compose-style declarative layer** (deferred from round-3 step-11): vault-management.md § Compose-Style Declarative Layer (deferred) covers the surface; round-5 workplan pins format + merging rules.
- **Agent-host integration / MCP-tool discoverability** (round-4 backlog): now that HTTP-MCP gives browser-hosted hosts a real surface, the natural round-5 follow-on is discoverability + a Hypomnema agent skill. Round-5 candidate.
- **Public-presence / brand work**: round-4 shipping tag is a candidate moment to invest now that HTTP-MCP makes the project newly demonstrable to browser-hosted clients. Round-5 candidate.

---

## Notes on workplan-write deferred-decision handling

The five Open Questions per [`notes/proposals/mcp-streamable-http.md`](../proposals/mcp-streamable-http.md) § Open Questions are resolved in § Deferred-decision resolutions above:

- **Resolution A** — rmcp 1.5 transport availability + axum-0.7 mount shape: resolved at pre-round prep (rmcp ships the feature); workplan-time fallback if `nest_service` doesn't compose on axum 0.7 is a hand-rolled axum handler.
- **Resolution B** — Session management: v1 stateless; `LocalSessionManager` default.
- **Resolution C** — Resumable SSE: defer.
- **Resolution D** — CORS: defer; v1 sets no CORS headers.
- **Resolution E** — `mcp.http.path` configurability: v1 rejects values other than `"/mcp"` with startup error.
- **Resolution F** (workplan-surfaced supplement) — ADR strategy: write ADR-0013; amend ADR-0012 + ADR-0008.
- **Resolution G** (workplan-surfaced supplement) — `HypomnemaBackend` trait + tower-http: 12-method trait, two impls; no tower-http promotion.
- **Resolution H** (workplan-surfaced supplement) — Spec/stories promotion targets: spec only; stories archive alongside the proposal.
- **Resolution I** (workplan-surfaced supplement) — `vault-management.md § MCP Tool Surface` wording: verify-then-amend with a one-sentence forward-reference; bump to v1.1.0 if amended.

The promoted spec at `docs/specs/mcp-streamable-http.md` v1.0.0 ships with these resolutions baked in: § Open Questions retains only the deferred ones (Resolutions B, C, D, E reasoning; Resolution A removed since it's resolved). Future rounds can pull deferred-OQ resolutions without a canon rewrite — the round-3 LDS pattern.

---

## Notes on round-4 shipping gate

This step is the round-4 shipping gate. The boundary ritual is the **full milestone-tag + per-step + end-of-round retro variant** per the prompt and per [`notes/project-planning-workflow-notes.md`](../project-planning-workflow-notes.md) § Step boundary ritual + § End-of-round retrospective.

Boundary ritual sequence (run by coordinator after Task 12.8 ships):

1. **Mark step 12 shipped** in `notes/roadmap/roadmap-4.md` § Step 12 with shipping date.
2. **Tag the milestone in git** — likely `v0.3.0` (round-4 bumps minor for the new transport surface, matching the round-3 `v0.1.0 → v0.2.0` precedent). The version-bump call is the human's; coordinator drafts the tag message and asks before tagging.
3. **Bump `Cargo.toml` version**: `0.2.0` → `0.3.0` (or whatever the version-bump call resolves to).
4. **Capture any ADRs that hardened during the build** beyond ADR-0013. Likely candidates: none anticipated (ADR-0013 covers the canon-touching decision; the trait-extraction in 12.2 is structural-not-canonical; manual-testing refresh is a docs surface). If anything surfaces during build, surface as `coordinator-only` soft flag.
5. **Per-step retro for step 12** in `notes/project-planning-workflow-notes.md` § Step 12. Apply the retro template; capture structured eval + free-form notes.
6. **End-of-round retro for round 4** in the same file. Round scope: roadmap step 12 only — single-step round (HTTP-MCP transport). 8 task agents, 1 coordinator, 1 orchestrator. Apply the round-1/2/3 end-of-round retro shape; the round's structural question is "did the single-step round shape work for a focused-feature shipping gate?"
7. **Archive round-4 roadmap**: `notes/roadmap/roadmap-4.md` → `notes/roadmap/archive/roadmap-4.md` per the round-1/2/3 archival precedent.
8. **Archive step-12 workplan**: `notes/roadmap/step-12-workplan.md` → `notes/roadmap/archive/step-12-workplan.md` per the step-archival policy.
9. **Update `notes/backlog.md`** with round-4 boundary follow-ups (already partially seeded by Task 12.8).
10. **Round-5 roadmap?** Whether round 5 begins immediately is the human's call. If yes, the next conversational turn after the round-4 retro lands creates `notes/roadmap/roadmap-5.md`. If no, the project rests at v0.3.0 with HTTP-MCP shipped — a natural moment for public-presence / brand work, agent-host integration design, or a deliberate gap.

The round-4 end-of-round retro answers (per [`roadmap-4.md`](./roadmap-4.md) implicitly via the round-1/2/3 pattern): "did the roadmap → workplan → build cadence still work at round-4 shape (single-step round, focused on a single transport addition)? What surprised us about MCP HTTP that the proposal did not predict? Did the trait-extraction's blast radius hold inside its sized expectations?"

The cadence has held for 11 consecutive clean steps (rounds 1+2 = 8; round 3 = 3); step 12 is the round-4 data point. The end-of-round retro consolidates round-4's structural observations: workplan-time deferral resolution still tractable for a feature-shipping round; ADR-amendment-vs-new-ADR pattern; trait extraction of the MCP server's backend abstraction as round-4's structural reshape; manual-testing-runbook drift as a recurring round-boundary obligation (round 3 retroactively, round 4 contemporaneously); HTTP-MCP-as-loopback-only inheritance of trust posture as the third worked example of ADR-0005's local-everything boundary.
