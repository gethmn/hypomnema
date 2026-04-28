# Hypomnema Roadmap — Round 4: MCP Streamable HTTP Transport

**Scope**: Add the third standard MCP transport — *Streamable HTTP* — alongside the existing stdio transport (round 2) and the deferred Unix-socket transport ([ADR-0012](../../docs/decisions/0012-mcp-transport-stdio-v0.md)). The transport mounts on `hmnd`'s existing Axum router as a single `/mcp` route accepting JSON-RPC over POST and SSE over GET, serves the same MCP tool surface that stdio-MCP serves today (3 search tools + 9 vault-management tools), and inherits the daemon's HTTP listener trust posture (loopback-only, no auth, no TLS — extending [ADR-0005](../../docs/decisions/0005-local-everything.md)). This unblocks browser-hosted agent hosts and hosts that prefer not to spawn `hmn mcp` subprocesses.

The round is deliberately **single-feature**. Other round-4 candidates (Compose declarative layer, CHANGELOG.md adoption, MCP write-tool gating granularity, outbox flake-hardening, multi-model embedding per vault, cross-vault search pagination + streaming, agent-host integration / MCP-tool discoverability, public-presence/brand work) stay in [`notes/backlog.md`](../backlog.md) § Round-4 candidates and become candidates for round 5+.

**Status**: Not started. Round 3 shipped `v0.2.0` on 2026-04-28; this round queues directly behind it. Workplans are created **just before** each step is implemented, per the round-1/2/3 cadence.

**Process**: Same as rounds 1–3. Each step gets a short workplan (`step-NN-workplan.md`) created immediately before that step is built. Deferred decisions are pulled forward to workplan-time. The orchestration shape (orchestrator + per-step coordinator + ephemeral task agents, see [`notes/coordinator-playbook.md`](../coordinator-playbook.md)) carries forward unchanged.

**Round-3 lessons feeding into this round** (see [`notes/project-planning-workflow-notes.md`](../project-planning-workflow-notes.md) § End-of-round retrospective for full text):

- **Spec-fleshout-at-workplan-write paid off across round 3** (step 10 fleshed `vault-management.md` from outline to v1.0.0 covering all 9 ops; step 11 shipped against fully-pinned wire shapes with zero rework). Round 4 applies the same pattern: the proposal at `notes/proposals/mcp-streamable-http.md` stays Draft until the step-12 workplan-write phase, at which point it promotes to `docs/specs/mcp-streamable-http.md` (LDS canon) alongside any required ADR + architecture + reference doc work. The proposal's 5 open questions are pulled forward to workplan-time per the round-2/3 discipline.
- **Workplan-prose-vs-load-bearing-decision drift is the stable round-3+ source of `coordinator-only` soft flags** (~0.5 flag-per-task across 24 round-3 tasks, all defer-to-boundary, zero human round-trips). Set the same expectation for round 4. Codification in playbook § COORDINATOR Per-task execution loop step 6.1 routing is a pre-round prep item below.
- **Manual smoke verification on every medium-high-risk wiring task** — now 6-of-6 across rounds 1–3. Step 12 is a wiring task by shape (new transport mounted on existing router); a smoke task is a default inclusion in the workplan.
- **Real-external-dependency pass before the shipping tag** — round 2 caught 2 issues (`7379dd0`, `fcc4aa3`) on real-shape deployments after `v0.1.0`; round 3's smoke matrix exercised 3-vault concurrent operation. Round 4's analogue is exercising the HTTP-MCP transport from a real MCP-HTTP-capable client (Iris, browser-hosted host, or curl + JSON-RPC) against a daemon serving multiple vaults — not just the loopback Rust integration test.
- **MSRV cross-check at workplan self-review** — applies if any new top-level crate gets added. Round 4 likely adds **zero** new top-level deps (rmcp + axum + tower-http already vendored); the load-bearing question is whether `rmcp = "1.5"` ships a server-side Streamable HTTP transport feature (verified in pre-round prep below).
- **Soft-flag self-correction at boundary** (round-3 step-10 new pattern): if a forward-noted soft flag asks the next task agent to reconcile prose, the agent verifies the prose is actually wrong before editing. Carries forward.
- **API-error stall + tail-peek diagnostic** (round-3 step-9 new pattern): if a coordinator wake-up case-4 routing fires but the rendered tail shows a transport-layer error, treat the case-4 input as a resume request rather than a status-check. Carries forward.
- **Silence-as-data for non-recurring flakes** (round-3 step-11 new pattern): if a forward-noted flake doesn't reproduce across the round's full-suite sweeps, that's data for the next round's boundary retro, not a resolution. Carries forward — applies to the outbox flake (`tests/outbox.rs::rename_emits_deleted_then_created_lines`) which still hasn't been seen across step 9/10/11.
- **Skills carrying forward**: `rusqlite-in-async` (any vault-management tool path through HTTP-MCP still routes through the daemon's connection pool), `filesystem-watching` (no direct relevance this round), `markdown-chunking` (no direct relevance), `sqlite-vec-extension` (no direct relevance). No new skills anticipated; write one if the round-4 build experience suggests it (e.g. an `mcp-http-transport` skill if the rmcp + axum integration pattern proves codifiable).

**Specs amended or created this round** (all promoted at step-12 workplan-write):

- **`docs/specs/mcp-streamable-http.md`** — new spec, promoted from `notes/proposals/mcp-streamable-http.md` v0.1.0 → v1.0.0. The proposal's 5 Open Questions are pulled forward to the workplan; the spec ships with them resolved. Cross-reference paths rewrite from `../../docs/...` to LDS-relative per the proposal's preamble note.
- **New ADR** (likely `0013-mcp-transport-streamable-http.md`) **or amendment to ADR-0012**. ADR-0012 currently enumerates only stdio (shipped) + Unix socket (deferred). Adding HTTP as a third transport is canon-touching; the workplan-write phase decides whether to amend 0012 or write 0013. Cleaner is probably 0013, leaving 0012 as the historical record of the v0 stdio decision.
- **`docs/architecture/overview.md` § Search API** — currently lists HTTP `/search/*` and stdio-MCP as two transports; add HTTP-MCP as a third peer with the in-process-tools data flow.
- **`docs/reference/configuration.md`** — add `[mcp.http]` block: `enabled` (default `true`), `path` (default `"/mcp"`, rejected if anything else in v1).
- **No amendments to the four search specs or `change-events.md`** — HTTP-MCP serves the same wire shapes the existing stdio-MCP transport already serves; specs are transport-agnostic by design.
- **Possible amendment to `docs/specs/vault-management.md`** — the spec already pre-commits to "same handlers, three transports" for vault-management MCP tools; if any wording needs to land HTTP-MCP as a concrete third transport (vs. forward-compat reference), that's a small touch-up at workplan-write. Verify at workplan-write whether the existing wording is sufficient.

**Implementation surface across the round**:

- New module: `src/api/mcp_http.rs` (or `src/api/mcp/`) — Axum route handler for `/mcp`, Origin-validation middleware, JSON-RPC + SSE plumbing.
- New rmcp feature flag in `Cargo.toml` (verified in pre-round prep) — likely a single feature add to enable the rmcp Streamable HTTP server transport. **Or**, if rmcp 1.5 doesn't ship one, custom scaffolding over rmcp's core types — the workplan re-validates scope against budget.
- New `SearchBackend` (or equivalent) abstraction in the MCP layer — two implementations: in-process (used by `hmnd`'s `/mcp` route, no network hop) and HTTP-shim (used by `hmn mcp`'s existing wrapper). Trait shape resolved at workplan-time.
- Wiring on `hmnd`'s existing Axum router (`src/bin/hmnd.rs`): mount `/mcp` alongside `/search/*`, `/health`, `/status`. No new listener, no parallel HTTP infrastructure.
- Origin-header validation: accept loopback origins (`http://localhost[:port]`, `http://127.0.0.1[:port]`, `http://[::1][:port]`, `null`); reject others with HTTP 403; accept missing `Origin` (curl/non-browser).
- Test surface: ~5–10 integration tests covering the 5 stories from `mcp-streamable-http-stories.md`; reuse the round-3 multi-vault test fixtures; manual smoke via curl + JSON-RPC and (if available) Iris or another MCP-HTTP-capable client.

**No top-level crate additions anticipated.** rmcp, axum, tokio, serde — all already in tree. `tower-http` (for `cors`/`request-id` if needed) is already a transitive dep via axum but verify direct-use vs. transitive at workplan-write.

---

## Phasing decision

Two illustrative options:

- **1-step (proposed)**. Step 12 covers the whole round: spec promotion + ADR + arch + ref docs (workplan-write phase), then route + middleware + in-process tools + tests + manual smoke (build phase). The whole round = round-4 shipping gate. Estimated workplan size: ~700–900 lines, comparable to round-1 step-5 (1100 lines) and round-2 step-8 (1100 lines, the round-2 MCP wrapper) — and **strictly less than step 8** in implementation scope, since HTTP-MCP reuses the same `HypomnemaMcpServer` and tool handlers that step 8 introduced.
- **2-step alternative**. Step 12 = spec/ADR + scaffolding (route, Origin validator, in-process search-tool plumbing, happy-path integration tests for `tools/list` + one `tools/call`). Step 13 = full vault-management MCP coverage over HTTP, full integration test matrix across the 5 stories, manual smoke, reference docs, round-4 shipping gate.

**Decision**: 1-step. The implementation surface is genuinely small (proposal § Implementation Notes: "no new authentication or TLS code; if you find yourself writing it, you have drifted from the spec"), the handlers are already written and tested at the stdio-MCP layer, and round 3 demonstrated that 9–22-file-fanout tasks fit cleanly inside one step. The 2-step option remains a valid redirect — at step-12 workplan-write the author can split if (a) the rmcp HTTP feature isn't available and custom scaffolding ends up sized like round-2 step 8's MCP wrapper, or (b) the SearchBackend trait extraction is wider than anticipated. Reviewing this roadmap, the user can also redirect by saying "split step 12 into 12 + 13" — the 1-step phasing is opinion, not commitment.

The shipping-criteria framing below is independent of which phasing wins; only the workplan boundaries change.

---

## Pre-round prep (before step 12 starts)

Two small items the round depends on, neither of which is a step in itself:

1. ~~**Verify rmcp Streamable HTTP server transport availability.**~~ **Done 2026-04-28**: `rmcp = "1.5.0"` ships the `transport-streamable-http-server` feature; `StreamableHttpService` is a tower service mounted via `Router::new().nest_service("/mcp", service)` per the canonical [`counter_streamhttp.rs`](https://github.com/modelcontextprotocol/rust-sdk/blob/main/examples/servers/src/counter_streamhttp.rs) example. No custom scaffolding needed; scope unchanged. Caveat: rmcp's example is axum 0.8; we're on axum 0.7 — `StreamableHttpService` is axum-version-agnostic (tower service over standard `http`/`http-body` types), `nest_service` exists in both, so 0.7 should work, but verify via `cargo build` at workplan-time before committing. Recorded in the proposal at [`notes/proposals/mcp-streamable-http.md`](../proposals/mcp-streamable-http.md) § Open Questions item 1 (resolved) + § Revision History (0.1.1).

2. ~~**Codify the four new round-3 patterns in `notes/coordinator-playbook.md`.**~~ **Done 2026-04-28**: four edits landed:
   - § COORDINATOR Wake-up routing case 4 → API-error-stall-recovery: peek the rendered tail before sending the status check; if the tail shows a transport-layer error, the status-check input doubles as a resume request.
   - § COORDINATOR Per-task execution loop step 6.1 → calibration note: workplan-prose-vs-load-bearing-decision drift is the dominant round-3+ source of `coordinator-only` flags (~0.5 flag-per-task), expected and routed normally without escalation.
   - § TASK AGENT new subsection "Verify forward-noted reconciliations before applying" → soft-flag self-correction at boundary: when consuming a forward-noted reconciliation, verify the claimed drift is actually present before editing.
   - § COORDINATOR Post-build evaluation → silence-as-data on forward-noted flakes: when a forward-noted flake doesn't reproduce in the step's quality-gate sweeps, that's data for the round-boundary retro, not a resolution.

Both can run before step 12's workplan-write, in either order, by either the orchestrator or a small task agent. Neither is part of the step-12 workplan itself.

---

## Step 12 — MCP Streamable HTTP transport (round-4 shipping gate)

**Status**: Shipped 2026-04-28. Workplan archived at `notes/roadmap/archive/step-12-workplan.md`. Round-4 boundary: milestone tag `v0.3.0`; this roadmap archived at `notes/roadmap/archive/roadmap-4.md`.

**Goal**: `hmnd` exposes the full MCP tool surface (3 search tools + 9 vault-management tools) over a single HTTP endpoint at `/mcp` on its existing Axum router. An MCP-HTTP-capable agent host can issue `initialize`, `tools/list`, and `tools/call` against the daemon and get back the same response shapes that stdio-MCP returns today, with `serverInfo.name == "hypomnema"`. Origin-header validation defends against DNS rebinding from a malicious browser page. Stdio-MCP and HTTP-MCP coexist without interference (both transports serve the same tools against the same daemon state). The transport ships as the round-4 shipping gate — `v0.3.0`.

**Shipping criteria** (derived from [`notes/proposals/mcp-streamable-http-stories.md`](../proposals/mcp-streamable-http-stories.md), promoted to `docs/specs/mcp-streamable-http-stories.md` at workplan-write, or merged into the spec as § Acceptance Criteria — workplan-time decision):

- POST `http://127.0.0.1:7777/mcp` with a valid JSON-RPC `initialize` returns HTTP 200, `serverInfo.name == "hypomnema"`, `serverInfo.version == <crate version>`.
- POST `tools/list` returns all 12 tools: `search_filesystem`, `search_content`, `search_semantic`, `vault_create`, `vault_list`, `vault_status`, `vault_pause`, `vault_resume`, `vault_reset`, `vault_rename`, `vault_rescan`, `vault_terminate` (the last 7 gated by `[mcp] enable_write_tools` per round-3 step 10's decision; HTTP-MCP inherits the same gating).
- POST `tools/call` for `search_filesystem` against a known fixture vault returns the same `FilesystemSearchResponse` JSON in `structuredContent` that POST `/search/filesystem` returns for an equivalent request. Test computes the expected response by issuing the equivalent `/search/filesystem` request from the same test process — not by hand-encoding the expected payload — so the criterion would not pass if the MCP path returned a constant or a divergent fixture.
- Same equivalence holds for `search_content` and `search_semantic`.
- Same equivalence holds for the 9 vault-management tools against the round-3 cross-vault test harness.
- POST with `Origin: http://example.com` returns HTTP 403 `Origin not allowed: http://example.com`. POST with loopback origins or no `Origin` header is accepted.
- Setting `mcp.http.enabled = false` produces a daemon that responds 404 to `/mcp` while continuing to serve `/search/*` (independently mountable).
- Setting `mcp.http.path` to anything other than `"/mcp"` produces a startup error with a clear message.
- SIGTERM with an open SSE GET to `/mcp` closes the stream cleanly within the daemon's existing `with_graceful_shutdown` window. No new shutdown machinery added in the MCP route handler.
- Stdio MCP (`hmn mcp`) and HTTP-MCP run against the same daemon simultaneously; both return identical tool lists; tool calls from one transport don't block the other.
- Manual smoke matrix run before the round-4 shipping tag: at minimum a curl + JSON-RPC pass through `initialize` → `tools/list` → `tools/call` for each search tool + `vault_list` + `vault_status`; if available, a real MCP-HTTP-capable client (Iris, browser-hosted host) doing the same. Smoke transcripts inline in the smoke task's results comment per round-3 precedent.
- Negative-fingerprint greps from the stories file pass:
  - `rg 'allow_any_origin|cors_allow_all|Access-Control-Allow-Origin: \*' src/` returns zero matches in the MCP handler module.
  - `rg 'reqwest::Client|hyper::Client|TcpListener::bind' src/api/mcp_http*.rs` returns zero matches.
  - `rg 'rustls|server_config|ServerCertVerifier|verify_token|api_key' src/api/mcp_http*.rs` returns zero matches.

**Deferred decisions to resolve at workplan-time** (this is where most round-4 deferred decisions land — round-3 spec-fleshout-at-workplan-write discipline):

- **All 5 open questions from the proposal**:
  1. rmcp 1.5 Streamable HTTP server transport availability — answer comes from pre-round prep item 1; the workplan documents what was found and what the implementation path is.
  2. Session management (`Mcp-Session-Id`) — v1 ships stateless. The spec amends if a future round needs sessions.
  3. Resumable SSE streams (`Last-Event-ID`) — defer; v1 has no notifications to send for read-only search + idempotent vault ops.
  4. CORS — v1 sets no CORS headers. First real browser-host consumer's requirements decide what to allow. Defer.
  5. `mcp.http.path` configurability — v1 rejects any value other than `"/mcp"` with a startup error. Future versions may allow customization.
- **ADR amendment vs. new ADR** — amend ADR-0012 to add HTTP as a third transport, or write a new ADR-0013. The cleaner option is probably 0013 (leaves 0012 as the historical record of the v0 stdio decision; 0013 documents the round-4 transport addition). Workplan-time decision.
- **`SearchBackend` (or equivalent) trait shape** — proposal § Implementation Notes flags this as a workplan-time decision. The trait abstracts the search backend so `hmnd`'s `/mcp` handler can call it in-process while `hmn mcp`'s existing wrapper continues to call it via the HTTP shim. Two implementations, one trait. The workplan pins the trait surface (likely 3 methods, one per search mode).
- **Whether vault-management MCP tools also extract through the same `SearchBackend`-style trait** — or whether the HTTP-MCP route just wires the existing rmcp `HypomnemaMcpServer` directly. The stdio MCP transport wraps `HypomnemaMcpServer` as an rmcp service over stdio; if rmcp 1.5 ships an HTTP transport feature, the same service trait should bind to both transports without an additional abstraction. Workplan-time decision once the rmcp answer lands.
- **MCP write-tool gating posture under HTTP-MCP** — round-3 step 10 settled `[mcp] enable_write_tools` as a single flag covering all 7 vault write tools. HTTP-MCP inherits this — does HTTP-MCP need its own gating key, or does the existing flag govern both transports? Default and recommended: the existing flag governs both (no new config surface). Verify at workplan-write that this is what gets shipped.
- **Promotion target for the proposal**: spec only (`docs/specs/mcp-streamable-http.md`) vs. spec + companion stories file (`docs/specs/mcp-streamable-http-stories.md`) — round-3 didn't promote story files to LDS canon (stories stayed in `notes/proposals/` and got archived alongside the proposal). Likely-same here: stories file archives with the proposal; spec absorbs acceptance criteria as a section. Workplan-time decision.
- **`docs/specs/vault-management.md` wording amendment vs. no-op** — verify at workplan-write whether the existing "same handlers, three transports" pre-commit is sufficient or whether HTTP-MCP needs a concrete sentence-level mention.

**New deps**: none anticipated. Possible new feature flag on `rmcp` (e.g. `transport-streamable-http-server`; verify exact name at pre-round prep). Possible direct-dep promotion of `tower-http` if Origin validation wants the framework's middleware shape vs. a hand-rolled tower layer — workplan-time decision.

**Risk**: medium. New transport surface, but **strictly less than the existing HTTP `/search/*` API in scope** (per proposal § Overview): same handlers, behind an MCP framing, with no new behavior, no new tool surface, no new trust posture. The load-bearing risk surfaces are (a) the rmcp HTTP feature shape — if absent, custom scaffolding adds meaningful scope; (b) the SearchBackend trait extraction's blast radius into `hmn mcp`'s existing wrapper — if the trait change ripples wider than the proposal anticipates, the workplan absorbs the ripple or splits to step 13; (c) Origin-validation correctness against real browser-host edge cases — covered by integration tests + manual smoke against a real client. Manual smoke verification is the natural quality gate per round-3 precedent.

**Cross-references**:
- [`notes/proposals/mcp-streamable-http.md`](../proposals/mcp-streamable-http.md) — full proposal, promoted at workplan-write
- [`notes/proposals/mcp-streamable-http-stories.md`](../proposals/mcp-streamable-http-stories.md) — 5 user stories with negative-fingerprint greps
- [ADR-0012 § MCP transport](../../docs/decisions/0012-mcp-transport-stdio-v0.md) — present scope this round extends
- [ADR-0005 § Local Everything](../../docs/decisions/0005-local-everything.md) — trust boundary the HTTP listener inherits
- [ADR-0008 § Two binaries](../../docs/decisions/0008-two-binary-daemon-plus-cli.md) — binary-placement principle (long-lived listener → `hmnd`)
- [ADR-0004 § Three search modes as peers](../../docs/decisions/0004-three-search-modes-as-peers.md) — canonical tool names
- [`docs/architecture/overview.md` § Search API](../../docs/architecture/overview.md#search-api) — current two-transport surface this round extends to three
- [`docs/specs/vault-management.md`](../../docs/specs/vault-management.md) — pre-commits to "same handlers, three transports"; HTTP-MCP fulfills the third
- [`docs/specs/{filesystem,content,semantic}-search.md`](../../docs/specs/) — wire shapes the new transport reuses unchanged
- [`docs/specs/change-events.md`](../../docs/specs/change-events.md) — out of scope (events flow through outbox, not MCP)

---

## Out of scope for round 4

These remain in [`notes/backlog.md`](../backlog.md) § Round-4 candidates and become candidates for round 5+ unless pulled in at the round-4 boundary:

- **Compose-style declarative layer** (deferred from round-3 step 11). Surface pinned in `docs/specs/vault-management.md` § Compose-Style Declarative Layer (deferred).
- **CHANGELOG.md adoption**. Round-3 boundary called this out; carrying into round-5 candidate slot. The round-4 shipping tag (`v0.3.0`) is another natural moment to settle.
- **MCP write-tool gating granularity** (per-tool gating vs. the single `enable_write_tools` flag).
- **Outbox flake-hardening** (`tests/outbox.rs::rename_emits_deleted_then_created_lines`) — silence-as-data through round 3; still owed.
- **Multi-model embedding per vault**.
- **Cross-vault search pagination + streaming**.
- **Agent-host integration / MCP-tool discoverability** — closely related to this round (`search_semantic` discoverability, possibly a Hypomnema agent skill); could become a natural round-5 follow-on once HTTP-MCP gives browser-hosted hosts a real surface to discover against.
- **Public-presence / brand work** — visual identity, GitHub org branding, README hero, project website. The round-4 shipping tag is a candidate moment to invest, since HTTP-MCP makes the project newly demonstrable to browser-hosted clients.

---

## Manual-testing drift (carryforward from round 3)

[`notes/manual-testing/`](../manual-testing/) is at "step 8" — its README explicitly says "When round 3 (multi-vault) lands, this directory will need updating alongside." Round 3 has shipped (steps 9–11) but the runbook has not been refreshed: `00-setup.md` still uses the v0 single-vault `vault = "..."` config key, the fixture under [`fixtures/sample-vault/`](../manual-testing/fixtures/sample-vault/) is still a single committed Markdown vault, and the surface-coverage table marks `hmn vault …` subcommands as "❌ not yet shipped." The shipped code through step 11 has full multi-vault control plane (HTTP + CLI + MCP), per-vault watcher/indexer/outbox, cross-vault search with `vaults` filter, and the v0 `hmnd scan` subcommand removed.

**Round-4 scope decision (workplan-time, step 12)**:
- **Option A**: fold a manual-testing refresh into step 12. Adds a second fixture vault to exercise multi-vault behavior end-to-end (cross-vault search, partial-results diagnostic, vault-management ops as smoke targets); refreshes 00-setup through 04-mcp; adds a `05-vault-management.md` and (new for round 4) `06-mcp-http.md`. Cost: ~1–2 task-agent slots in step-12's workplan, plus fixture engineering. Benefit: the manual-testing surface stays current with shipped reality, and the round-4 shipping-gate manual smoke can drive against the refreshed runbook (rather than ad-hoc commands).
- **Option B**: defer to a dedicated post-round-4 cleanup (or a small follow-on round). The round stays single-feature on HTTP-MCP per the original scope decision; manual-testing drift is acknowledged but not addressed in round 4.

Default: **lean Option A** unless step-12 workplan-write reveals enough scope on the HTTP-MCP-only path that the refresh meaningfully expands the workplan. The runbook touches no production code; refresh tasks are bisect-clean and parallelizable. Decide at workplan-write phase.

**Round-4 retro reminder**: regardless of whether step 12 folds in the refresh, the round-4 end-of-round retro must evaluate manual-testing drift as a structural item — what shipped this round, what's now stale, what's the plan. Going-forward rounds inherit the same discipline (see [`notes/backlog.md`](../backlog.md) § Process / playbook).

---

## Notes on the round-4 shipping gate

The round-4 shipping gate is a working HTTP-MCP transport demonstrated end-to-end:

1. `hmnd` running with default config; `mcp.http.enabled` defaults to `true`; `/mcp` mounts on `127.0.0.1:7777`.
2. A real MCP-HTTP-capable client (or curl + JSON-RPC) completes `initialize` and observes `serverInfo.name == "hypomnema"`.
3. Same client invokes `search_filesystem`, `search_content`, `search_semantic`, `vault_list`, and `vault_status` against a multi-vault daemon and gets back results equivalent to the existing `/search/*` and `/vaults/*` HTTP endpoints.
4. Origin validation rejects a non-loopback Origin with HTTP 403.
5. Stopping the daemon mid-SSE-stream closes the stream cleanly.
6. The full integration test suite (round-3's 27 tests + round-4's new MCP-HTTP tests) is green; 3× consecutive flake-check on the new test file is clean.
7. Reference docs (`docs/reference/configuration.md`) document `[mcp.http]`. Architecture overview lists three transports, not two.
8. The proposal at `notes/proposals/mcp-streamable-http.md` has been archived to `notes/proposals/archive/`; the spec at `docs/specs/mcp-streamable-http.md` is the canonical source.
9. New ADR (or amended ADR-0012) records the canon decision.
10. Round tag: `v0.3.0`.

After the gate hits, round 4 archives at this boundary alongside `step-12-workplan.md`, the active dir resets, and round 5's roadmap gets written when the human picks the next focus from the backlog.
