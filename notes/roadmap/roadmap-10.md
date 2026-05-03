# Hypomnema Roadmap — Round 10: v0 Polish (Health + VCS-Aware Ignores)

**Scope**: Optional polish round before v0.5.0 / v0 stable cut. Two read-only / additive operational improvements bundled into a single step. No new daemon dependencies beyond a `.gitignore`-parsing crate.

**Status**: In progress. Round shape decided 2026-05-02 from `notes/scratchpads/v0 Polish Round Scope` (Solo scratchpad #17, Option B). Workplan pending.

**Process**: Same as rounds 1–9. One step. Coordinator + researcher + ephemeral builders. See [`notes/playbook/`](../playbook/) for the orchestration contract.

**Source of truth for scope**: Solo scratchpad #17 "v0 Polish Round Scope" (tasks 1 and 2; task 3 — metrics — explicitly out of scope for this round).

**Why this round**:

- Round 9 closed the search-and-retrieve story (`content_get` + ranked search). v0 is functionally complete; what remains is operational polish that makes a v0.5.0 release feel like a stable artifact rather than a feature snapshot.
- **Health endpoint** is the smallest possible operational surface: container/orchestrator readiness probes (k8s liveness, systemd notify-style external checks) currently have nothing to call. The route slot already exists in `src/api/health.rs` per the original v0.1 expansion plan; this step fills it.
- **VCS-aware ignores** closes a real ergonomic gap: today the daemon honors only `ignore_patterns` from config, so a vault that is also a Git repository indexes `.git/`, `node_modules/`, `target/`, and any other `.gitignore`-listed paths unless the operator manually mirrors them into config. For developer-vault use cases this is a recurring footgun.
- **Risk is bounded**. Health is a pure new HTTP handler with no shared-state writes. VCS ignores is additive at the watcher/scan filter boundary with documented precedence; the existing `ignore_patterns` path stays authoritative on conflict.
- **No blocking questions**. Both tasks have clear shape from the scratchpad; deferred decisions are workplan-time.

**Skills carrying forward**:

- `filesystem-watching` (VCS-ignores task touches the watcher event filter and initial scan; the `notify` + debouncer pattern stays unchanged but the filter layer grows)
- `rusqlite-in-async` (health task may read DB connectivity; any DB touch goes through `spawn_blocking`)

**New deps**: One `.gitignore`-parsing crate for the VCS-ignores task. The researcher should evaluate `ignore` (BurntSushi, used by `ripgrep` — handles the full hierarchical `.gitignore` semantics) vs. the smaller `gitignore` crate at workplan-write time and pin the choice. Per AGENTS.md "ask vs proceed", a new dep is worth flagging at workplan review; the orchestrator (this round) is pre-approving the *category* (one `.gitignore` parser) but the specific crate is a workplan-time decision the human will see at the workplan-ready handoff.

**Out of scope for round 10** (explicitly): Prometheus-style `/metrics` endpoint and any metrics-collection plumbing. Stays as a candidate stretch in `notes/scratchpads/v0 Polish Round Scope` task 3 for a future small round.

---

## Phasing

One step containing two tasks. Tasks are independent and can build in either order; the workplan should pick a sequencing rationale.

| Step | Contents | Risk |
| ---- | -------- | ---- |
| 21 | Health endpoint + VCS-aware ignores | Low |

---

## Step 21 — Health Endpoint and VCS-Aware Ignores

**Goal**: Ship two additive, read-only operational improvements: (1) a working `GET /health` endpoint that orchestration layers can probe, and (2) `.gitignore`-aware path filtering in the watcher and initial scan, layered behind the existing `ignore_patterns` config.

### Task 21.1 — Health Endpoint (`GET /health`)

**Shipping criteria** (from scratchpad; full breakdown in `step-21-workplan.md`):

- `GET /health` implemented in the existing `src/api/health.rs` route slot.
- Response body: `{ status: "healthy" | "degraded" | "unhealthy", vaults_active: N, vaults_errored: N, uptime_seconds: U }` with at minimum these fields; researcher may extend with per-vault snapshot if it composes cleanly with the existing control-plane state.
- Status mapping: `200 OK` on `healthy`; `503 Service Unavailable` on `degraded` or `unhealthy`.
- Health signals minimally cover: file-watcher operational state, database connectivity (cheap probe — `SELECT 1` via `spawn_blocking`), embedding-service reachability *only when* embeddings are configured for at least one vault (don't make embedding optional configs flap the endpoint).
- No new background tasks introduced by the health probe; uptime is computed from a daemon-start instant captured at boot.
- Read-only; not gated by `[mcp] enable_write_tools`. No MCP tool surface (HTTP only — health probes are an HTTP idiom).
- Negative fingerprint: handler does not block the runtime — any DB touch is wrapped in `spawn_blocking`.
- Spec: a short `docs/specs/health-endpoint.md` (or section in an existing operational spec, researcher's call) canonically describes the wire shape and status mapping.
- `cargo test` green; `cargo clippy -- -D warnings` clean.
- Manual-testing fixture exercises healthy and degraded paths.

**Deferred decisions to resolve at workplan-time**:

1. **Per-vault snapshot inclusion**: Whether the response body carries a per-vault array (`vaults: [{ name, status, last_indexed_at? }]`) or stays summary-only. Default to summary-only unless the researcher finds a clear use case.
2. **Embedding-service health policy**: When embeddings are configured for any vault, an unreachable embedding service should map to `degraded` (not `unhealthy`) — daemon still serves search and retrieval. Confirm semantics in spec.
3. **Spec home**: New `docs/specs/health-endpoint.md` vs. section in an existing spec. Default to a small dedicated spec; the surface is small but distinctly operational.

### Task 21.2 — VCS-Aware Ignores

**Shipping criteria** (from scratchpad; full breakdown in `step-21-workplan.md`):

- Vault-root `.gitignore` is parsed and applied to both the watcher event filter and the initial-scan path.
- Nested `.gitignore` files are honored per Git semantics (a `.gitignore` inside a subdirectory applies to files under that subdirectory; researcher confirms the parser crate supports this without per-directory custom plumbing).
- Precedence chain (documented and tested): per-directory `.gitignore` → vault-root `.gitignore` → daemon-config `ignore_patterns`. **Daemon config wins on conflict** — operator-set `ignore_patterns` can override a `.gitignore` exclusion if needed (e.g. an operator wants `.env.example` indexed even though `.gitignore` excludes `.env*`). Researcher resolves the exact override semantics (negation patterns vs allowlist) at workplan-time.
- `.git/` itself is always excluded (defensive default; never indexed regardless of config).
- Symlinks: existing watcher behavior preserved. Document any divergence in spec.
- Default behavior: VCS-aware ignores are **on by default** for any vault that has a `.gitignore` at the root; opt-out via a new config knob if the researcher's analysis warrants one. Default-on is the higher-value choice; opt-out is a workplan-time deferred decision.
- Watcher event-filter and initial-scan code paths share the ignore-evaluation logic — no duplicated rule application.
- Negative fingerprint: a vault containing a representative `.gitignore` (with `node_modules/`, `target/`, `*.log`, and a negation `!important.log`) reindexes correctly and event filtering matches.
- Spec: `docs/specs/vault-ignores.md` (or amendment to existing vault-management or watcher spec — researcher's call) canonically describes the precedence chain and override semantics.
- `cargo test` green; `cargo clippy -- -D warnings` clean.
- Manual-testing fixture covers a vault with a non-trivial `.gitignore`, including a negation rule.

**Deferred decisions to resolve at workplan-time**:

1. **Crate choice**: `ignore` (full ripgrep-grade hierarchical semantics, larger surface) vs. `gitignore` (smaller, simpler). Default to `ignore` unless the researcher finds a meaningful trade-off.
2. **Opt-out config knob**: Whether to introduce a new boolean (`respect_gitignore: bool`, default `true`) or treat VCS-awareness as unconditional. Default to introducing the knob — operators may want to disable it for vaults where `.gitignore` is not aligned with desired index scope.
3. **Conflict-resolution semantics**: Exact rules when a `.gitignore` exclusion and a daemon `ignore_patterns` intersect — daemon config wins, but the *shape* of "wins" (re-include via negation, allowlist append, etc.) is a workplan-time call.
4. **Nested-`.gitignore` re-evaluation on edit**: If a `.gitignore` itself is edited at runtime, does the watcher re-evaluate the affected subtree or require a daemon restart? Default to "documented as restart-required for v0"; researcher can recommend otherwise if cheap.
5. **Spec home**: New `docs/specs/vault-ignores.md` vs. amendment. Default to new dedicated spec given the precedence chain is non-trivial.

### Cross-task shipping criteria

- Both tasks ship within a single round-10 gate review; they don't gate each other but archive together.
- Workplan picks a sequencing rationale (parallel vs. health-first vs. ignores-first). Health-first is the simpler default — smallest surface, no new dep — but the workplan author may swap.
- Round-10 retro entry appended to `notes/project-planning-workflow-notes.md`.

**Risk**: Low. Both tasks are additive. Health is a pure new endpoint. VCS-ignores is an additive filter layer with documented precedence and an opt-out path. The one elevated-risk surface is the watcher event filter (any change there can affect indexing correctness), but the change is layered, not invasive — existing `ignore_patterns` evaluation is preserved.

**Coverage**: Maps to scratchpad #17 tasks 1 and 2. Task 3 (metrics) is explicitly deferred to a future round.

---

## Step Sequencing

Single step. Workplan-write decides intra-step task order. Coordinator orchestrates per-task builders; gate review verifies both tasks together.

1. Coordinator spawns researcher and requests `step-21-workplan.md`.
2. Researcher resolves deferred decisions and produces full task breakdown, dep choices, and testing strategy.
3. Coordinator surfaces workplan to human for review.
4. On `build/go/approved`, coordinator orchestrates builders per task.
5. Gate verifies all shipping criteria + negative fingerprints + deferred decisions resolved.
6. Round 10 ships; archive workplan + roadmap; tag v0.5.0 if the human chooses to cut a release at this gate.

---

## Out of scope for round 10

These stay in `notes/scratchpads/v0 Polish Round Scope` (#17) or `notes/backlog.md` and are explicitly not part of this round:

- Prometheus-style `/metrics` endpoint and metrics collection (scratchpad task 3 — future small round).
- Per-vault tokenizer or embedding overrides.
- Any write-side surface to the vault.
- Health endpoint as MCP tool (HTTP-only is intentional — health probes are an HTTP idiom).
- Hot-reload of daemon `ignore_patterns` config (orthogonal to VCS-aware ignores).
- `.dockerignore` / other VCS ignore variants beyond `.gitignore` (Git-native is the live ask; others can be added later if a use case surfaces).

---

## Notes on round-10 philosophy

This is a deliberate small polish round, not a feature round. The goal is to make v0.5.0 feel like a stable release artifact: an operator running `hmnd` against a real Git-managed vault gets sensible default ignore behavior, and an orchestration layer running the daemon has a probe to call. Both tasks anchor to existing patterns (HTTP route slot, watcher filter layer) and avoid introducing new abstractions. If the workplan-time analysis surfaces a reason to split this into two steps, that's a coordinator-to-orchestrator escalation rather than a quiet expansion.
