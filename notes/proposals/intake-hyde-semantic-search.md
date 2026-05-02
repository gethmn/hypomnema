# Proposal Intake: HyDE Semantic Search

**Status**: Intake complete
**Date**: 2026-05-02
**Intake inputs**:

- `notes/proposals/hyde-semantic-search.md` — Primary proposal (Status: Draft, 2026-04-30). No peer stories file exists.

---

## Summary

HyDE (Hypothetical Document Embeddings) is a semantic-retrieval query-expansion technique: instead of embedding a short, question-shaped query directly, an LLM first writes a short hypothetical answer or note section that would satisfy the query; that generated text is then embedded and used as the kNN probe against the existing `chunks_vec` index. The semantic index does not change — chunks stay chunked and embedded as today. The architectural surface is small (one extra step in front of the existing embedding call) but the question of *where that step lives* is load-bearing: caller-side (agent generates the hypothetical document; daemon's `search_semantic` is unchanged) or daemon-side (daemon adds a `query_mode: "hyde"` field and owns a generation HTTP client, prompt, and timeout/error semantics). The intake's recommended default is **agent-side first, daemon-side conditional on empirical recall validation** — that ordering preserves ADR-0005 ("local everything") posture, avoids a heavyweight runtime dep before the recall claim is proven against real Hypomnema vaults, and matches the proposal's own recommended priority path.

## Source Inputs

| Source | Type | Role in intake |
|---|---|---|
| `notes/proposals/hyde-semantic-search.md` | proposal | primary — defines the technique, two design options (agent-side vs daemon-side), error shape if daemon-side, and five open questions |
| (no stories file) | — | absent — proposal does not have a peer stories artifact; coverage map below derives acceptance signals from proposal sections instead, and the absence is flagged in § Open Questions |
| `docs/specs/semantic-search.md` (v0.3.0) | spec | background — the canonical surface HyDE would amend; HyDE leaves request flow steps 3–6 unchanged and inserts a generation step in front of step 2 (the embedding call) |
| `src/search/semantic.rs` | code | background — current `search_semantic` body; sole entry point of the embed→kNN pipeline that HyDE intercepts |
| `src/embedding.rs` | code | background — pattern to mirror for any future `GenerationClient` (HTTP client, retry policy, timeout, typed error class, `Embedder`-style trait, stub for tests) |
| `notes/qmd-comparison.md` | comparison | background — qmd ships HyDE + a fine-tuned 1.7B query-expansion model; documents both the cross-pollination opportunity and the deliberate Hypomnema/qmd split (substrate-as-events vs search-quality) |
| `notes/proposals/fts5-bm25-content-search.md` | sibling proposal | background — the other live retrieval-quality proposal; HyDE is explicitly not a substitute for FTS5 (different mode: ranked lexical vs semantic expansion) |
| `notes/backlog.md` § Round-6 carry-over → "Multi-model embedding per vault" | backlog | background — the deferred per-vault embedding-model item interacts directly with daemon-side HyDE (per-vault embedding implies per-vault generation prompt/model in some shape); flagged in § Open Questions |
| `notes/proposals/archive/intake-search-result-payload-budget.md` | prior intake | background — round-8 intake template precedent for tone, density, and coverage-map detail |
| `notes/proposals/intake-content-retrieval.md` | prior intake | background — round-9-candidate intake; sibling of this one in the current proposal queue |
| ADR-0004 (Three Search Modes as Peers) | decision | background — HyDE is additive to `search_semantic`, not a fourth peer mode |
| ADR-0005 (Local Everything) | decision | background — load-bearing constraint on whether the daemon adopts a generation dependency |
| ADR-0007 (sqlite-vec over Alternatives) | decision | background — `chunks_vec` schema-baked dimension is unchanged by HyDE |

## Candidate Outcomes

- **Outcome: Improved semantic recall for abstract / question-shaped queries**
  - Source: Proposal § What It Would Bring (the four sample queries)
  - User-visible result: A query like `how do we prevent spurious reindexes?` surfaces chunks that mention `notify-debouncer-full`, `content_hash`, `chunks_vec` etc. — vocabulary the original query did not contain — at a higher rank than direct embedding of the question.
  - Verification signal: Side-by-side comparison on a fixed query set against a real Hypomnema vault; HyDE-mode results have higher overlap with human-judged relevant chunks than direct mode for the abstract-query subset, and roughly equivalent for the concrete-term subset.

- **Outcome: Documented agent-side HyDE recipe**
  - Source: Proposal § Proposed Direction step 1 ("Document an agent-side HyDE recipe in notes or manual testing")
  - User-visible result: An agent host (Claude Code, etc.) can follow a notes/-tracked recipe to drive HyDE today via the existing `search_semantic` surface — no daemon changes.
  - Verification signal: Recipe exists at a known path (`notes/recipes/hyde-semantic-search.md` or `notes/manual-testing/hyde-recipe.md`); manual smoke against a fixture vault demonstrates the recipe's flow end-to-end.

- **Outcome (conditional): Daemon-side `query_mode: "hyde"`**
  - Source: Proposal § Design Options § Option B + § Error Shape If Daemon-Side
  - User-visible result: Consumers send `query_mode: "hyde"` on `POST /search/semantic` (or via MCP `search_semantic`); daemon calls a configured generation endpoint, embeds the result, and runs the kNN. `query_mode: "direct"` (default) preserves current behavior. Generation outages surface as a structured `generation_unavailable` error with no silent fallback to direct mode.
  - Verification signal: Future-step shipping criteria when this step runs; not in scope for the recommended Step 1.

- **Outcome (conditional): Empirical evidence that HyDE helps in Hypomnema's vault shape**
  - Source: Proposal § Proposed Direction step 2 ("Try it against real Hypomnema vaults and compare results to direct semantic search")
  - User-visible result: A short evaluation note (numbers + qualitative observations) that decides whether daemon-side is worth the surface area.
  - Verification signal: Note exists; orchestrator and human can read it before committing to a Step 2 (daemon-side) workplan.

## Proposed Roadmap Shape

**Shape note**: This is a two-step roadmap with the second step explicitly conditional on the first. Step 1 is small and docs-shaped — it could land as a single-step round, fold into an unrelated round as a side deliverable, or live as a backlog item the next time someone wants to dogfood HyDE. Step 2 is large enough that it warrants a stories-drafting pre-step and a fresh re-intake before workplan-write.

### Step N — Agent-Side HyDE Recipe + Empirical Recall Note

**Goal**:
Publish a documented, agent-side HyDE recipe that uses the existing `search_semantic` surface unchanged, and produce a short empirical-recall note comparing HyDE-mode and direct-mode results on a real Hypomnema vault. No daemon code changes.

**Shipping criteria**:

- [ ] A canonical recipe document exists at a known path (recommended: `notes/recipes/hyde-semantic-search.md`, or fold into `notes/manual-testing/` if that better matches how the project tracks operator-facing recipes). Recipe documents: prompt template the agent uses to generate the hypothetical document, length / shape guidance for the generated paragraph, when HyDE helps vs hurts vs is neutral, and how to invoke `search_semantic` with the generated paragraph as `query`.
- [ ] Recipe explicitly notes that the daemon is unchanged (no new request fields, no new daemon-side dep, no spec amendment) and that switching to daemon-side support is a future, conditional step.
- [ ] An empirical-recall comparison exists (small, focused — single-vault, ~10–20 queries split between abstract/question-shaped and concrete-term), capturing: queries used, side-by-side top-K result diff, qualitative judgment on which mode produced the more relevant chunks, and a final go/no-go recommendation for Step 2.
- [ ] The empirical note explicitly addresses whether the recall improvement justifies the added surface area of a daemon-side generation dependency. A "no" or "not yet" outcome is an acceptable, valuable result — it closes the question rather than committing to Step 2 prematurely.
- [ ] No changes to `docs/specs/semantic-search.md`, `src/search/semantic.rs`, `src/embedding.rs`, `src/api/search.rs`, MCP server, CLI, or config schema. (Pure docs/notes deliverable. Negative-fingerprint check: `git diff --stat` for the round shows changes only under `notes/`.)
- [ ] If the recipe lands inside a roadmap step rather than as a standalone backlog item, the round retro records the empirical-recall outcome so future rounds can use it.

**Deferred decisions resolved in this step**:

- Decision: HyDE LLM call placement (caller-side vs daemon-side) — **default for Step 1 is caller-side**
  - Source: Proposal § Open Questions ("Is agent-side HyDE good enough, or does daemon-side support materially improve ergonomics?", "Should Hypomnema ever own a text-generation dependency, or should it stay retrieval-only?")
  - Why this step: Caller-side is the only choice that's actually shippable without a daemon-side spec amendment, an ADR-worthy dep decision, and a stories pass. It is also the only path that produces the empirical evidence needed to *decide* Step 2 confidently. Step 1 nails caller-side as the v0 answer; Step 2 (if ever taken) revisits with data in hand.
- Decision: Recipe document location (`notes/recipes/` vs `notes/manual-testing/` vs `docs/`)
  - Source: Project layout convention
  - Why this step: Pick one and document. `notes/` is correct (operator-facing, agent-host-facing recipe; not part of the LDS canonical surface). Within `notes/`, the choice is a small convention call for the workplan author.

**New deps**:

- (none — pure docs/notes deliverable)

**Risk**: low

Rationale: Zero code changes, zero contract changes, zero new dependencies. The only risk is that the empirical-recall pass returns inconclusive results, in which case Step 2 stays deferred — also a perfectly acceptable outcome.

**Source coverage**:

- Proposal § Summary: Step 1 (recipe captures the technique)
- Proposal § Current Baseline + § What It Would Bring: Step 1 (empirical note validates against real vault)
- Proposal § Design Options § Option A: Step 1 (Option A = the recipe)
- Proposal § Proposed Direction (steps 1 + 2): Step 1
- Proposal § Open Questions Q1 (agent-side good enough?): Step 1 produces evidence
- Proposal § Open Questions Q2 (Hypomnema ever owns generation?): Step 1 closes the v0 answer (no for now)

### Step N+1 (CONDITIONAL — gated on Step N outcome) — Daemon-Side `query_mode: "hyde"`

**Status**: **Do not draft a workplan for this step until** (a) Step N's empirical note recommends proceeding, (b) a stories file is written, and (c) the open questions tagged "blocks roadmap" below are resolved. This step appears in the intake to keep the multi-step shape visible and to flag the surface area the future round will face — not to commit to it.

**Goal**:
Add an optional `query_mode` field to semantic search; when `hyde`, the daemon calls a configured generation endpoint, embeds the generated hypothetical document, and runs the existing kNN. `query_mode: "direct"` (default) preserves current behavior bit-for-bit. No silent fallback from HyDE to direct mode on generation failure.

**Shipping criteria** (sketch — to be re-pinned at the future workplan-write):

- [ ] `SemanticQuery` and `SemanticQueryJson` accept `query_mode: "direct" | "hyde"` (default `"direct"`).
- [ ] `query_mode: "direct"` path is byte-for-byte identical to current behavior; existing tests stay green with no expectation changes for the direct path.
- [ ] `query_mode: "hyde"` calls a generation client → embeds the generated text → runs the existing kNN.
- [ ] New `[generation]` config section (or extension of `[embedding]`, decided at workplan-write) with at least: `endpoint`, `model`, `api_key`, `timeout_ms`, `max_retries`, `prompt_template_version`. Mirror `EmbeddingConfig` shape.
- [ ] `GenerationClient` HTTP client with typed errors (mirror `EmbeddingError` + `Embedder` trait pattern from `src/embedding.rs`); pure-async, never inside `spawn_blocking`.
- [ ] Stub generation backend for tests (mirror `StubEmbedder`) so semantic-search test surface stays deterministic.
- [ ] Versioned, documented prompt template stored in code (not config) so the prompt is reviewable in PR diffs.
- [ ] New error class `generation_unavailable` (HTTP 503 / structured MCP error) for unreachable / 5xx / empty-response generation; new `invalid_generation` (or reuse `generation_unavailable`) for empty or unusable generated text. No silent fallback.
- [ ] Spec amendment: `docs/specs/semantic-search.md` documents `query_mode`, the request-flow diff, the new errors, and the prompt-template version field.
- [ ] CLI: `hmn search semantic --mode hyde` (or `--query-mode hyde`) accepted; default `direct`.
- [ ] MCP `search_semantic` tool input schema includes `query_mode`; tool description distinguishes the two modes.
- [ ] Config docs (`docs/reference/configuration.md`) and CLI docs (`docs/reference/cli.md`) updated.
- [ ] Health probe: optional startup probe of the generation endpoint, mirroring `embed_health_probe` (warn-only, never fails the daemon).
- [ ] Observability: log lines distinguish HyDE-mode requests from direct-mode requests so an operator can grep usage; optional debug field exposing the generated hypothetical document gated behind a request flag (decided at workplan-write — Q in proposal § Open Questions).
- [ ] All stories from the future stories file pass acceptance criteria.
- [ ] Manual-testing fixture: HyDE-mode happy path, generation-unavailable failure, generation-timeout failure, dimension-mismatch interaction (HyDE generates text → embed succeeds but returns wrong dim → existing dimension-mismatch path).
- [ ] `cargo test` and `cargo clippy -- -D warnings` clean.

**Deferred decisions resolved in this step** (sketch):

- Decision: Generation endpoint config shape — separate `[generation]` section vs extension of `[embedding]`
  - Source: Proposal § Open Questions Q3
  - Why this step: Endpoint, model, prompt, and error semantics are distinct concerns from embedding. Recommend separate `[generation]` block; future config consumers can compose if they share a process. To be re-pinned at workplan time.
- Decision: Prompt template — what shape, what version field, where it lives
  - Source: Proposal § Open Questions Q4
  - Why this step: Prompt is part of the daemon's contract (it shapes results). Recommend storing the prompt template in code (not config) and versioning it; bump the version in the spec amendments table any time it changes.
- Decision: Should HyDE responses expose the generated hypothetical document for debugging
  - Source: Proposal § Open Questions Q5
  - Why this step: Recommend opt-in debug field (e.g. `include_generation: true`) gated behind a future request param; default off so the response shape stays small. To be re-pinned at workplan time.

**New deps**:

- New runtime dep: a configured generation HTTP endpoint (likely OpenAI-API-shaped `chat/completions` or `completions`). No new Rust crate needed if the existing `reqwest` client is reused.
- ADR-worthy: this step adds an LLM-call surface to the daemon. Per ADR-0005 ("local everything"), the default should be a local generation endpoint; like the embedding endpoint, hosted is configurable but not the default. **A new ADR is recommended** ("Generation endpoint is operator-supplied; daemon prefers local") to make the trade explicit and to document why the daemon-as-substrate framing now includes generation.

**Risk**: medium-high

Rationale: Adds a new hot-path runtime dep with its own failure modes, an ADR-worthy posture decision, a prompt-template surface that becomes part of the daemon contract, and a testing-determinism problem (the stub-generator pattern works but constrains how realistic integration tests can be). The pure-mechanics part of the change is small (new client, new field, new error class), but the design-surface implications are large enough that proceeding without empirical recall evidence from Step N is not justified.

**Source coverage** (sketch):

- Proposal § Design Options § Option B
- Proposal § Design Impact (the seven design questions)
- Proposal § Error Shape If Daemon-Side
- Proposal § Open Questions Q3, Q4, Q5

---

## Coverage Map

| Source item | Proposed step | Status | Notes |
|---|---|---|---|
| Proposal § Summary (technique definition) | Step N | planned | Recipe captures and documents the technique |
| Proposal § Current Baseline (current vs HyDE flow diagrams) | Step N | planned | Recipe diagrams the agent-side flow; future Step N+1 covers the daemon-side flow |
| Proposal § What It Would Bring (sample query types) | Step N | planned | Empirical-recall note's query set should include these shapes |
| Proposal § What It Would Bring (negative space — what HyDE is NOT a replacement for) | Step N | planned | Recipe documents the negative space so consumers don't reach for HyDE when substring or FTS5 is the right tool |
| Proposal § Design Options § Option A (agent-side) | Step N | planned | Option A IS the recipe |
| Proposal § Design Options § Option B (daemon-side) | Step N+1 | deferred-conditional | Sketched above; gated on Step N outcome + stories file + open-question resolution |
| Proposal § Design Impact (the seven design questions) | Step N+1 | deferred-conditional | All seven are daemon-side concerns |
| Proposal § Error Shape If Daemon-Side (the four-row error table) | Step N+1 | deferred-conditional | Spec amendment material for daemon-side step |
| Proposal § Proposed Direction step 1 (document recipe) | Step N | planned | Direct mapping |
| Proposal § Proposed Direction step 2 (try it against real vaults) | Step N | planned | The empirical-recall note IS step 2 |
| Proposal § Proposed Direction step 3 (draft daemon-side amendment if recall improves) | Step N+1 (conditional) | deferred-conditional | Only if Step N's empirical note recommends proceed |
| Proposal § Open Questions Q1 (agent-side good enough?) | Step N | planned | Step N produces the evidence |
| Proposal § Open Questions Q2 (should Hypomnema own a generation dep?) | Step N | partially-planned | Step N answers "no for now"; Step N+1 (if taken) revisits with data |
| Proposal § Open Questions Q3 (separate `[generation]` config block?) | Step N+1 | deferred-conditional | Future workplan decision |
| Proposal § Open Questions Q4 (stable prompt template) | Step N+1 | deferred-conditional | Future workplan decision |
| Proposal § Open Questions Q5 (expose generated hypothetical document) | Step N+1 | deferred-conditional | Future workplan decision |
| Multi-model-embedding-per-vault interaction | Step N+1 | deferred-conditional | Flagged in § Open Questions; informs daemon-side config shape |
| ADR posture for daemon-side generation dep | Step N+1 | deferred-conditional | New ADR recommended at Step N+1 workplan-write |

---

## Deferred / Out-of-Scope Items

- Item: Daemon-side `query_mode: "hyde"` field (Option B in the proposal)
  - Source: Proposal § Design Options § Option B
  - Reason: Adding a generation dep to the daemon is a meaningful architectural shift (ADR-0005 "local everything" tension; new ADR likely; new prompt/test/error surface). Should not be undertaken without empirical recall evidence from Step N AND a peer stories file.
  - Revisit trigger: Step N's empirical-recall note recommends proceeding AND a stories file is drafted AND the open questions tagged "blocks Step N+1 roadmap" below are resolved.

- Item: Local-generation model selection (which local LLM? GGUF via llama.cpp? a sidecar like TEI's chat counterpart?)
  - Source: Implied by ADR-0005 + Proposal § Design Impact
  - Reason: Step N+1 specifies the daemon talks to a configurable generation endpoint (mirroring the embedding-endpoint posture). Picking the *default* local model is a separate operational/recommendation question, not part of the daemon's code surface.
  - Revisit trigger: Step N+1 workplan-write or a "recommended local generation stack" docs item.

- Item: Cross-encoder reranking (the qmd cross-pollination companion to HyDE)
  - Source: `notes/qmd-comparison.md` § Cross-pollination opportunities
  - Reason: Heavier than HyDE (separate rerank service + per-result rerank loop). Out of scope for this proposal entirely; would be a separate future proposal if a use case surfaces.
  - Revisit trigger: A use case where post-kNN reranking is load-bearing.

- Item: HyDE for FTS5 / content-search (`search_content` ranked mode)
  - Source: Not in the proposal; possible adjacent question
  - Reason: HyDE's leverage is on *embedding-shaped* retrieval where document-vs-question representation mismatch is the failure mode. FTS5/BM25 is term-shaped; LLM-generated text would dilute term frequency and probably hurt ranked-mode quality. Out of scope.
  - Revisit trigger: None expected.

- Item: Multi-model-embedding-per-vault co-evolution with HyDE
  - Source: `notes/backlog.md` § Round-6 carry-over
  - Reason: If multi-model-embedding ever lands, daemon-side HyDE may need per-vault prompt templates or per-vault generation models (different vault content shape → different optimal hypothetical-doc style). The interaction is real but speculative; no need to design for it before either feature exists. Flagged in § Open Questions for awareness.
  - Revisit trigger: Whichever of multi-model-embedding-per-vault or daemon-side HyDE lands second — the second one's workplan should re-read this entry.

- Item: Stories file for the HyDE proposal
  - Source: Absence noted in § Source Inputs
  - Reason: Step N (recipe + empirical note) is small and docs-shaped enough that derived acceptance criteria from the proposal's own sections suffice. Step N+1 (daemon-side) needs proper stories before workplan-write — the open-question surface is too large to skip.
  - Revisit trigger: Before Step N+1 workplan-write.

---

## Open Questions

- Question: Is the absence of a peer stories file a blocker for Step N?
  - Why it matters: Coverage map without stories relies on the proposal's own sections as acceptance signals. For Step N (recipe + empirical note) this is workable because the deliverables are small and docs-shaped. For Step N+1 it would not be workable.
  - Blocks roadmap? **No** for Step N; **yes** for Step N+1.
  - Suggested owner: Workplan author for Step N (proceed); orchestrator + human at Step N+1 planning time (require stories first).
  - **Intake recommendation**: Proceed to Step N without stories; require a stories pass before Step N+1 workplan-write.

- Question: Where does the HyDE LLM call live — caller-side or daemon-side?
  - Why it matters: Load-bearing per the todo body. Caller-side preserves ADR-0005's "local everything" posture without committing the daemon to a generation runtime dep, and lets the agent's existing LLM do double duty. Daemon-side gives consumers a uniform surface (including non-LLM consumers) and centralizes prompt/model choice for reproducibility, but adds a hot-path dep with its own failure modes, prompt-as-contract surface, test-determinism problem, and ADR-worthy posture decision.
  - Blocks roadmap? **No** — the intake's recommended default closes the question for now.
  - Suggested owner: Resolved at intake.
  - **Intake recommendation**: **Caller-side as the v0 default.** Step N publishes the agent-side recipe; Step N+1 (daemon-side `query_mode`) is conditional on empirical recall evidence from Step N AND a fresh re-intake. Rationale:
    1. The proposal's own § Proposed Direction recommends this ordering.
    2. ADR-0005 frames the daemon's required-runtime-dep budget tightly. The embedding endpoint is the only such dep today; adding a generation endpoint doubles that surface and warrants an ADR. Doing the ADR before the recall claim is validated is premature.
    3. The qmd comparison (`notes/qmd-comparison.md`) frames Hypomnema deliberately as the "substrate-as-events" project and qmd as the "search-quality" project. Adopting qmd's heavyweight model stack — including HyDE — without empirical reason to believe it pays off in Hypomnema's vault shape would blur that intentional split.
    4. Agents already have an LLM and a prompt context. The marginal cost of agent-side HyDE is near-zero. The marginal cost of daemon-side HyDE is meaningful (new dep, new ADR, new error class, new test surface, prompt-as-contract, possible per-vault prompt explosion if multi-model-embedding ever lands).
    5. A "no" outcome from Step N's empirical pass is a legitimate, valuable result — it closes the daemon-side question rather than committing to a maintenance burden the data doesn't justify.

- Question: How does daemon-side HyDE interact with the deferred multi-model-embedding-per-vault backlog item?
  - Why it matters: If a future round adds per-vault embedding models (different model → different vector space), then the optimal hypothetical-document shape for the HyDE generation step also varies per vault. Daemon-side HyDE would then need either per-vault prompt templates, per-vault generation models, or a documented "HyDE assumes daemon-wide generation regardless of per-vault embedding" stance.
  - Blocks roadmap? **Yes for Step N+1**, **no for Step N**.
  - Suggested owner: Step N+1 workplan author (must read this entry before drafting).
  - **Intake recommendation**: Flag this in Step N+1's workplan-time deferred-decisions and in the Step N+1 spec amendment. Either feature landing first should re-read this entry to avoid quietly painting the other into a corner.

- Question: Does Step N belong inside a roadmap step at all, or as a backlog item / standalone notes deliverable?
  - Why it matters: Step N has no code changes and is small. It could land as an explicit roadmap step in a multi-step round, fold into an unrelated round as a side deliverable, or live as a backlog item the next time someone wants to dogfood HyDE.
  - Blocks roadmap? **No** — this is a roadmap-write call, not an intake call.
  - Suggested owner: Orchestrator + human at round-planning time.
  - **Intake note**: The intake structures Step N as a roadmap-step-shaped deliverable so the empirical-recall outcome gets recorded in a round retro and feeds future planning. If the orchestrator prefers a backlog item, that works too — the artifact (recipe + empirical note) is the same.

- Question: If Step N's empirical-recall pass shows mixed results (helps for some queries, hurts for others), is that a "proceed" or "do not proceed" signal for Step N+1?
  - Why it matters: HyDE's literature suggests it helps for some query shapes and hurts for others. A mixed outcome is the most likely Step N result. The decision rule for Step N+1 should be defined before Step N runs to avoid post-hoc rationalization in either direction.
  - Blocks roadmap? **No** for Step N (the empirical-recall note can capture the data either way), **yes** for Step N+1 (the workplan needs a clear "ship it because…" rationale).
  - Suggested owner: Step N workplan author or Step N+1 re-intake.
  - **Intake recommendation**: Default rule for Step N+1 trigger: proceed only if the empirical note shows HyDE *materially* helps a recurring query shape that consumers actually issue, AND the agent-side recipe has measurable ergonomic friction (e.g., consumers consistently ask for a daemon surface). Either condition alone is not enough.

---

## Recommendation

**Recommended next action: "draft stories first" for the daemon-side step (Step N+1); proceed with Step N (recipe + empirical note) as a small, low-risk, optionally-roadmap-scheduled deliverable when an orchestrator wants to slot it in.**

Concretely:

- [ ] If the orchestrator wants to schedule HyDE work in the next round: draft a small `notes/roadmap/roadmap-N.md` with a single-step shape — Step N as defined above. No `step-NN-workplan.md` is needed beyond what's already pinned in this intake's Step N shipping criteria; the workplan for a docs-only deliverable is essentially the criteria list itself.
- [ ] If the orchestrator does not want to schedule HyDE work next: this proposal becomes a backlog item (`notes/backlog.md`) referencing this intake. Step N can be picked up by anyone (human or agent) who wants to dogfood HyDE.
- [ ] Either way: do not draft a Step N+1 workplan. That step requires a stories file, an empirical-recall outcome from Step N that justifies the surface area, and resolution of the multi-model-embedding interaction question.

Rationale:

1. **Inputs are partially complete.** The proposal is well-drafted and the design space is narrow (two options, one of which is "do nothing daemon-side"). But the proposal itself recommends an empirical validation pass before committing to daemon-side, and there is no stories file to anchor a daemon-side workplan. Trying to plan Step N+1 now would either invent stories from the proposal's own sections (low confidence) or skip stories entirely (against the project's spec / intake / workplan rhythm).

2. **Step N is genuinely small and low-risk.** Pure docs/notes deliverable, zero code changes, zero contract changes, zero new dependencies. The only "shipping" work is writing the recipe and running 10–20 queries against a real vault. A "no, this doesn't help in our vault shape" outcome is a legitimate ship.

3. **Step N+1 is genuinely large and the right place to gate on data.** Adding an LLM call to the daemon's hot path is an ADR-worthy posture decision (per ADR-0005 "local everything"). It introduces a prompt-as-contract surface, a new error class, a stub-generation test pattern, and a hot-path dependency that can fail in ways the daemon currently does not have to handle. Doing this work without empirical evidence that HyDE materially helps in Hypomnema's vault shape — and without consumer pull for a daemon surface — would expand maintenance burden ahead of value.

4. **The current proposal queue context.** Two other proposals are also in intake state: `intake-content-retrieval.md` (recommended "start now" — strong fit for the next round) and `intake-fts5-bm25-content-search.md` (status not read here; sibling lexical-retrieval proposal). HyDE is *additive* to semantic search and orthogonal to both other proposals — it doesn't compete for the same code surface. Scheduling-wise, content-retrieval is the obvious round-9 candidate; HyDE Step N can slot in as a small side-deliverable in round-10+ or live as a backlog item until then.

5. **LDS layer impact (sketch)**:
   - Step N: `notes/` only — no LDS canonical surface touched.
   - Step N+1 (if/when taken): `docs/specs/semantic-search.md` amendment (the `query_mode` field, the request-flow diff, the new errors, the prompt-template version field), `docs/reference/configuration.md` and `docs/reference/cli.md` updates, a new ADR ("Generation endpoint is operator-supplied; daemon prefers local"), and code changes across `src/search/semantic.rs`, `src/api/search.rs`, `src/mcp/`, `src/bin/hmn.rs`, `src/config.rs`, and a new `src/generation.rs` module.

6. **Cross-proposal note**: the recommendation deliberately echoes the round-8 intake's tone — "do the small, well-understood thing first; defer the larger surface until evidence supports it." Round 8 shipped cleanly because the input was complete and the surface was localized. HyDE Step N matches that profile; HyDE Step N+1 does not yet.

---

## Human Review Notes

(append review decisions here)
