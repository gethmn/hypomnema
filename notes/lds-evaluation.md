# LDS Evaluation — Forward-Looking Implementation Planning

**Captured**: 2026-04-24, while setting up the initial implementation kick-off plan for steps 1–5 of Hypomnema.

**Purpose**: A durable record of the inconsistencies and gaps I (Claude) observed in LDS while attempting to use it for forward-looking implementation planning. Not a proposal to change LDS — Beau's stance is that incorporating short-term planning into LDS is **not a goal** right now. This is a snapshot taken while the friction was fresh, so we know what to call out if we ever revisit.

**LDS reference**: [`docs/DOCUMENTATION-GUIDE.md`](../docs/DOCUMENTATION-GUIDE.md) is the entry point.

---

## 1. The "Implementation" layer is overloaded

`DOCUMENTATION-GUIDE.md` line 150 describes Layer 7 (Implementation) as:

> **Purpose**: *How to build* the system. Short-lived; absorbed into code and tests as work progresses.

Yet [`docs/implementation/tech-stack.md`](../docs/implementation/tech-stack.md) holds the **8-step build priority** — which is a long-lived roadmap, not throwaway scaffolding. The same file also serves as a **reference** for the chosen crates, project structure, and pitfalls.

Three distinct things live in one layer with one stated purpose:
- A reference inventory of the tech stack (crate-by-crate)
- A static description of the project structure
- A forward-looking, ordered list of build steps

The first two match the layer's stated purpose. The third does not — it's a roadmap, with the same long lifespan as the project itself.

---

## 2. No layer for forward-looking execution plans

LDS has seven canonical layers (Decisions, Vision, Architecture, Specifications, Reference, Behaviors, Implementation). None is a natural home for:

- A roadmap (what gets built next, in what order)
- A workplan (concrete tasks for the step being implemented)
- A backlog (deferred work waiting to be picked up)
- A "current state" indicator (what step are we on)

The closest fit is Implementation, which is why the 8-step priority ended up there — but that creates the overload described in #1. There is no terminology in LDS for "we will build X next" as distinct from "X is how the system is structured."

---

## 3. No current-state tracker

There is no document that says "step 3 is done; step 4 is in progress; steps 5–8 are queued." Progress against the roadmap is inferable only from:

- Git history (commits referencing step numbers, e.g. f9d00da, 73abe49)
- The presence/absence of code modules
- Conversation memory

For a small one-person project this is fine, but it is a real gap if the project grows or if a new contributor (human or agent) tries to orient.

---

## 4. Open Questions are per-spec, not aggregated

Each spec under [`docs/specs/`](../docs/specs/) and [`docs/product/vision.md`](../docs/product/vision.md) carries its own "Open Questions" section. While drafting the roadmap I found roughly fifteen TBDs scattered across:

- `vision.md` lines ~107–117 (eight open questions)
- `specs/change-events.md` lines ~97–100 (four)
- `specs/content-search.md` line 86 (one)
- `specs/filesystem-search.md` lines 91–93 (three)
- `specs/semantic-search.md` lines 97–98 (two)

There is no aggregated index of open questions. To plan a step that touches multiple specs, you must grep across the spec layer manually. A thin "open-questions index" — even just an auto-rolled-up table — would help.

---

## 5. Plan-file location is undefined

Ephemeral plan documents (the `.claude/plans/i-d-like-to-put-wobbly-parrot.md` for this very session, for example) have no prescribed home in LDS. `.claude/plans/` is harness-specific (Claude Code) and not a project artifact. If a human contributor wrote a plan, where would it go?

Adjacent question: where do **archived** plans go after the work ships? LDS doesn't say.

---

## 6. Skills are technical, not procedural

The four skills under `.claude/skills/` (rusqlite-in-async, sqlite-vec-extension, markdown-chunking, filesystem-watching) are all **subsystem pattern guides** — how to call rusqlite from async, how to chunk markdown, etc. They are excellent for the patterns they cover.

There is no analogous skill for **process patterns**:
- How a feature moves from idea to spec to implementation
- How to plan a step
- How to recognize step completion
- How to write a workplan

This may be intentional (process is project-specific, not pattern-specific) but it means the planning workflow we are currently inventing has no precedent within the project's own knowledge system.

---

## 7. Terminology gap

The words "roadmap" and "workplan" do not appear in `DOCUMENTATION-GUIDE.md` or `tech-stack.md`. There is no shared vocabulary for the kind of artifact this session is producing. Beau noted: "I'm not sure that's intended to be an actual forward-looking plan (roadmap, executable work plan, system for keeping track of work that is done already and work that still needs to be done, etc.)."

Without shared terminology, every conversation that needs a forward-looking plan has to re-invent both the artifact shape and the words for it. This evaluation file and [`project-planning-workflow-notes.md`](./project-planning-workflow-notes.md) are an attempt to seed that vocabulary outside LDS.

---

## What LDS does well, for context

To keep this evaluation honest:

- **ADRs** were exceptionally useful when bootstrapping the roadmap. The 8 existing ADRs answered "why X" for nearly every constraint I had to honor.
- **Specs** with their Open Questions sections — even though scattered (#4) — were the clearest source of the TBDs the roadmap had to allocate.
- **Vision's Non-Goals section** prevented me from over-scoping. Had this been buried in a longer doc, I'd have planned for things v0 explicitly excludes.
- **Skills** make subsystem implementation low-risk. The patterns are concrete enough that I can hand them to a fresh agent and trust the result.

LDS works well for the doc shapes it was designed for. The gaps above are about a use case (forward-looking execution planning) that LDS, by design, does not cover.

---

## Action items

None right now — Beau's stated preference is to keep these observations as notes, not promote them to ADRs or doc changes. If we ever decide LDS should grow a planning layer, this file becomes the input.
