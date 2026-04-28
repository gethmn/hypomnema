# Vibe Progress: Hypomnema

## Meta
- **Current Phase:** 3 → 4 (Codification drafted; ready for Use)
- **Artifact:** [`hypomnema-visual-identity.md`](./hypomnema-visual-identity.md) — v1.0
- **Last Updated:** 2026-04-26
- **Target Image Generators:**
  - Raster exploration: Gemini (Nano Banana 2), FLUX.2 Pro, FLUX.2 Flex (via OpenRouter)
  - Vector finalization: Recraft (limited free tier) or hand-redrawn from raster
- **Output Type:** Hybrid leaning vector — primary deliverables are vector logo/logotype; raster fine for exploration

---

## Phase 1: Discovery

### Project Context
- **Project:** Hypomnema — a local daemon that indexes a directory of Markdown files and exposes filesystem / content / semantic search plus a change-event stream to consumers (most often AI agents over MCP).
- **Status:** Pre-v0. This is a foundational vibe-setting moment for the project's visible identity.
- **Audience:** End users of Hypomnema — developers and knowledge workers who keep a vault of Markdown notes and want agents/scripts to search and react to them.
- **Etymology / conceptual seed:**
  - From the ancient Greek ὑπόμνημα (plural *hypomnemata*): a personal notebook of accumulated external material — quotations, observations, reading notes — gathered for later rereading and self-constitution.
  - Canonical example: Marcus Aurelius's *Meditations*.
  - Foucault revived the term for the practice of constituting oneself through accumulated written material.
  - Tagline: *"a material memory of things read, heard, or thought."*
  - Pronunciation: *hi-POM-nih-muh* (English) / *hoo-POM-nay-mah* (Greek).
- **CLI binary:** `hmn` — short, lowercase, mononym-feeling.

### Use Cases & Formats
- **Highest priority:** Logo / logotype for the GitHub Organization and main repository.
- **Secondary:** General "vibe root" that can extend to icons and small illustrations later.
- **Future:** Website hero, website / GitHub icons, README/docs rendered on GitHub, eventual project domain.
- **Aspect ratios:** Logo must work square, circle, and squircle (favicon/avatar/app-icon-friendly). Logotype likely horizontal companion lockup.

### Aesthetic Foundation
- **Adjectives:** *structured, reliable, simple, pure*. These pull the visual language toward Bauhaus / Swiss / Dieter Rams modernism — engineered precision — rather than weathered/lapidary/antiquarian surfaces. The Greek conceptual soul remains; the antique rendering vocabulary does not.
- **Direction shortlist (refined):** Combination of Direction 1 (Tablet / Inscribed Fragment) and Direction 3 (`hmn` Monogrammatic Stack), in that priority. Direction 2 (Crystallized Notebook) is ruled out — see below.
- **Color temperament:** Cool. Slate, cool grays, almost-blue. Monochrome-friendly. Departures from cool require justification.
- **Geometric preference:** Strong. Modernist precise geometry (cleanly constructed forms with mathematical relationships). *Not* irregular faceted polygons — that language belongs to the user's Scind/Xcind work and re-using it here would imply a "house style" the user explicitly does not want across unrelated projects.
- **System ambition (from the start):** Logo must work as square, circle, and squircle, and remain recognizable at favicon scale (16×16 as the discipline, even though it's never fully achievable). Silhouette carries the brand; fine detail is a larger-scale layer only.

### Reference Points (positive)
- **Scind / Xcind logo system** (the user's own recent work — three images). Key takeaways:
  - Single faceted-polygon DNA that decomposes into letterforms (I, X) at meaningful break lines.
  - Scale-aware rendering system: tiny = solid silhouette → medium = bold white-separated facets → large = outlined-and-faceted detail.
  - Same form rendered in monochrome (black / white-line-on-black) and in color (orange/amber). The brand is the geometry, not the color.
  - System over single mark.
- **Implicit positive references** (from Phase 1 setup): Penguin Classics / Loeb spines (lapidary, restrained), Foucault's hypomnemata practice (personal notebook), classical inscription.

### Anti-patterns (confirmed)
- No brain / neural-network / mesh / synapse imagery.
- No generic "open book" or "scroll" iconography.
- No cute mascots or illustrated characters.
- No faceted gem / cut-stone / crystal language (reserved for Scind/Xcind; would imply a "house style" the user does not want across unrelated projects).
- No skeuomorphism — the mark is not pretending to be a physical tablet, chip, card, or carved stone.
- No weathering, chiseling, distressed surfaces, antique texture.
- No ornamental classicism — the etymology is the soul, not the surface.

### Aesthetic Adjectives (with definitions)
- **structured** — internal logic is visible. Mathematical relationships, geometric primitives, decompositions that *mean something*. Not "tidy" — *constructed*. The viewer can see how the form is built.
- **reliable** — the mark behaves the same in every context. Same proportions at 16×16 as at 512×512. Same silhouette in monochrome and in color. No surprises across square / circle / squircle.
- **simple** — minimum parts; each part load-bearing. Nothing decorative. If something can be removed without loss, it has been.
- **pure** — formal restraint. No skeuomorphism, no weathering, no faux-texture, no warmth applied as a finish. The form is the form. Modernist asceticism — closer to Rams than to Aurelius.

### Emotional Intent
> *This is a quiet, well-engineered piece of infrastructure that takes its own etymology seriously — the kind of tool a careful person made for careful people.*

### Visual Metaphors (consolidated)
- **Substrate** — the container is what holds the inscription. Hypomnema is the readable layer beneath the user's notes.
- **Inscription** — the glyph is what has been *written* on the substrate. Not carved, not chiseled — placed precisely.
- **Catalog discipline** — single primitive + single glyph; the system is the rule, not the variation.
- **Modernist monogram** — the IBM / Vignelli / Müller-Brockmann tradition: a few primitives, mathematically related, doing one job each.

### Style Reference Pocket
Names to invoke during prompt iteration when the generator drifts:
- Dieter Rams (Braun) — restraint, "less but better"
- Massimo Vignelli (Unimark / NYC subway / IBM-era identity work) — modernist authority
- Josef Müller-Brockmann (Swiss school) — grid discipline
- Paul Rand — modernist monogram tradition
- Bauhaus / Swiss style generally
- Penguin Classics spines — the *restraint*, not the literal serif vocabulary

### Glyph Direction Priorities
Decided exploration order, highest-ambition first:

1. **D — Constructed dual-reading glyph (μ ↔ `hmn`)**: a custom mark whose bones read as μ to a Greek-aware viewer and as a fused `hm` / `hmn` ligature to anyone else. Highest reward, hardest to pull off.
2. **A — μ alone (mu)**: first letter of μνήμη ("memory"); also reads as Latin `m` connecting to `hmn`. Modernist geometric construction. Falls back here if D fails.
3. **C — `hmn` monogram alone**: three Latin letters fused, modernist-monogram tradition. Compatible as wordmark in lockup; harder to keep recognizable at 16×16.
4. **B — υ (upsilon)**: deprioritized but not ruled out. Most literal etymological choice, but reads as `u` / `v` to Western viewers without semantic depth.

### Lockup Plan
**Symbol-plus-wordmark** is in scope from the start:
- The symbol carries the avatar / favicon / square uses.
- The wordmark `hypomnema` (or possibly `hmn`) typeset in a clean modernist sans sits beside the symbol for horizontal lockups, headers, and contexts where there's room.
- Inspiration model: IBM 8-bar mark + wordmark.

### Phase 1 Output Checklist
- [x] Project name and context
- [x] Output type (hybrid leaning vector)
- [x] Target generators (Gemini / FLUX.2 / Recraft)
- [x] Aesthetic adjectives with definitions (structured, reliable, simple, pure)
- [x] Emotional intent statement
- [x] Visual metaphors
- [x] Reference artists/brands/styles
- [x] Anti-patterns
- [x] Primary use cases and formats

---

## Phase 2: Testing

### Generator Strategy
- **Lead with FLUX.2 Pro or Gemini (Nano Banana 2)** for raster exploration of glyph candidates. Iteration is fast and cheap.
- **Treat AI output as direction-finding**, not final art. Bauhaus-grade modernist construction is generally outside what any current generator nails — the final mark will likely be hand-redrawn in vector. Prompts are for exploring proportions, weights, and variants, not for producing the deliverable.
- **Reserve Recraft free-tier** for late-stage attempts at native vector once a clear direction emerges, and for vector translation of a chosen raster mark via "style reference."
- **Always request:** flat color, no gradients, no shadows, no 3D, no texture, no perspective. Pure 2D vector-friendly construction. White or single-color flat background.

### Tested Prompts

#### Round 1 (2026-04-26)

**Test D-1 — Dual-reading μ ↔ `hmn`**
- Nano Banana (#4): `h-m-p`-shape with descender. Concept right (dual-read attempted via descender), execution puts the tail in the wrong place. Salvageable concept.
- FLUX.2 Pro (#5): `m`-with-right-stem-descender in thin-line square. Reads as μ rotated. Square container correct. Bland but disciplined.
- FLUX.2 Flex (#6): Two-arch `hm`. Drifted toward Path B (hmn-ligature) territory.

**Test A-1 — μ alone**
- Nano Banana (#7): **Treatment win.** Slate-filled square + white knockout glyph. Glyph itself is a happy accident (lemniscate-with-stem) but the treatment is system-grade — preserve it. Color: slate-blue field with white knockout.
- FLUX.2 Pro (#8): Bland `pa`-shape. Skip.
- FLUX.2 Flex (#9): Serif μ. **Anti-pattern confirmed: serif is wrong.** Greek-letter prompts pull toward classical Greek typography; must explicitly call out modernist sans.

**Test C-1 — `hmn` monogram**
- Nano Banana (#10): `h+m` shared-stem ligature, only two letters but cleanly modernist.
- FLUX.2 Pro (#11): **The breakthrough.** Thin-outlined-square container + three-arch shared-stem `hmn` ligature in slate-gray monoline. Subtle dual-reading (geometry first, letters second). This is the foundation.
- FLUX.2 Flex (#12): Same idea but middle arch overlaps and breaks the illusion.

#### Round 2 (2026-04-26)

**Test R1 — Path B refined, outlined mode**
- FLUX.2 Flex (#13a): Clean `hm` (2 letters), thin-stroke square, monoline. Disciplined.
- FLUX.2 Pro (#13b): Awkward `hmm` with a non-articulating extra stem. Pro lost discipline this round.
- Nano Banana (#13c): Confident `hm`, bolder stroke, darker slate, heavier square outline.
- **Pattern:** All three landed on 2 letters, not 3. The third letter consistently drops out of the outlined-mode ligature.

**Test R2 — Path A refined, filled/knockout mode**
- FLUX.2 Flex (#14a): Lemniscate-with-stem (Path C aesthetic).
- FLUX.2 Pro (#14b): Clean modernist μ with descender correctly on the left, plus a spurious second character. The μ itself is the cleanest we've produced.
- Nano Banana (#14c): Another lemniscate-with-stem. Confirms: **this aesthetic is repeatable from Nano Banana on this prompt class.** User flagged "calling to me" twice — surfaces as legitimate Path C.

**Test R3 — Path B in filled mode (system test)**
- FLUX.2 Flex (#15a): Clean `hmn` (3 letters), white knockout, slate-blue field.
- FLUX.2 Pro (#15b): **Cleanest expression of Path B yet** — `hmn` properly distinguished, beautifully proportioned, ready to commit.
- Nano Banana (#15c): Clean `hm` (2 letters) in same treatment.

#### Round 2 Conclusions
- **Major correction (user-flagged):** The intended ligation rhythm is **2 arches with 3 stems and maximum overlap** (per Round 1 #16, Nano Banana C-1), not 3 arches with 4 stems (as I had been prompting). With maximum overlap:
  - `h` = left tall stem + arch 1
  - `m` = the entire 3-stem-2-arch silhouette (with the leftmost stem's ascender being typographic license)
  - `n` = right 2 stems + arch 2
  - All three letters share the same arches; the brand mark is a single `m`-glyph with a left-stem ascender.
- **Path D resurrected:** Because μ is structurally `m` with a left descender, and `hmn` is structurally `m` with a left ascender, **the dual-reading glyph is `m` with the left stem extending both above x-height AND below baseline**. Same glyph, two literacies. This is the real D-spec; Round 1's attempt missed it by assuming 3 arches.
- **Path C surfaced as a serious side-direction:** lemniscate-with-descender (two closed circular bowls + left descender stem). Structurally a cousin of μ but drawn with closed bowls instead of open arches. Worth one focused exploration before ruling in or out.
- **Filled mode > outlined mode for legibility.** The counter-spaces (white knockouts) do the letterform work that the outlined mode struggles to articulate. System will likely be filled-primary, outlined-secondary.
- **Generator behavior refined:**
  - **FLUX.2 Pro** = highest discipline ceiling but variable round-to-round (R1 lost discipline, R2/R3 nailed it).
  - **Nano Banana 2** = repeatable creative aesthetic; "lemniscate-with-stem" is its consistent signature on Greek-letter prompts.
  - **FLUX.2 Flex** = surprisingly strong this round; promoted to peer rather than deprioritized.

#### Round 3 (2026-04-26)

**Test T1 — Lockup with corrected `hmn` ligature**
- FLUX.2 Flex (#17a): Slate-blue square + white knockout 2-arch `hm` glyph + dark slate "hypomnema" wordmark. Generally clean; some accidental internal lines in glyph.
- FLUX.2 Pro (#17b): Best balance. Color-matched slate-blue across symbol field and wordmark. Glyph reads cleanly. Some thin slate-blue artifacts inside the white knockout — **these are generator quirks, not design features. Drop them.**
- Nano Banana (#17c): Solid lockup, but reverted to 3 arches in the symbol — didn't follow the corrected 2-arch spec.
- **Wordmark observation:** All three got "hypomnema" spelled correctly. Lockup spatial relationship works: comfortable spacing, slate-blue color family carries from field to wordmark.

**Test T2 — Path C exploration: RULED OUT**
- All three generators produced essentially what we asked for: vertical white stem + closed circular bowl(s), white-on-slate-blue. **The result is conceptually thin** — reads as "infinity symbol with a vertical bar," not as a brand mark. The original Path C appeal (in #7, #14) was the happy ambiguity of "is this μ or ∞?" When asked for directly, the ambiguity vanishes and the mark becomes a math formula.
- **Diagnostic insight:** The "calling-to-me" reaction was actually an early sighting of Path D's silhouette (the long left stem). Path C is dead; Path D inherits its appeal.

**Test T3 — Path D resurrection: SUCCESS (the breakthrough)**
- FLUX.2 Flex (#19a): **Exactly right.** Two arches, three stems, leftmost stem extending generously both above x-height (h ascender) and below baseline (μ descender). Clean monoline modernist construction. White knockout on slate-blue. **The dual-reading glyph we've been chasing.**
- FLUX.2 Pro (#19b): Mostly correct but the ascender extension is too subtle relative to the descender, weakening the dual-read. Reads more as "μ" than "μ ↔ hmn."
- Nano Banana (#19c): Didn't land. Tried to interpret "left descender" as a μ-with-bowl variant (added a small bowl on the left stem), confusing the silhouette.

#### Round 3 Conclusions
- **Path D commits.** The symbol is a 2-arch / 3-stem `m`-construction whose leftmost stem extends both above x-height and below baseline. Reads as h (left half), m (whole), n (right half), and μ (when the descender is foregrounded). Single glyph, four literacies.
- **Path B is the fallback** if the dual-reading glyph fails to scale or causes legibility issues. The 2-arch `hm`/`hmn` form (left ascender only) is a clean, safer alternative we know works.
- **Path C is ruled out** as a primary direction.
- **No internal articulation in the glyph** — single solid shape, no thin internal outlines, no bevels, no double-strokes. The artifact lines from T1's Pro result are generator quirks to be removed in the final hand-redrawn vector.
- **Wordmark vocabulary established:** "hypomnema" set in geometric modernist sans (Avenir / Univers / DIN family), all lowercase, in the same slate-blue as the symbol's field color, x-height matched to the symbol's interior glyph height.

#### Round 4 (2026-04-26)

**Test U1 — Path D in lockup**
- All three generators (#20) struggled to produce the dual-reading glyph reliably in lockup form. Re-rolls (#23) produced "interesting variations" where ascender/descender extended fully to the container edges — the user noted these as a worth-considering system variant.

**Test U2 — Path D outlined mode (#21)**
- Generators struggled. Outlined mode further attenuates the silhouette discipline already established as filled-primary.

**Test U3 — Path D bare glyph (#22)** + **Recraft pass (#24)**
- Bare-glyph attempts inconsistent. Recraft style-reference pass produced a cleaner vector but still off-spec on the dual-extension stem.

#### Round 4 Conclusions
- **Generator reliability ceiling reached for Path D.** This was anticipated in the Phase 2 generator strategy ("Bauhaus-grade modernist construction is generally outside what any current generator nails — the final mark will likely be hand-redrawn"). Round 4 confirms it empirically rather than disproving Path D.
- **Path D remains committed.** User's call: "Not sure I'm ready to give up on D, yet… if we like this direction I can probably get the rest of the way there on my own." The hand-finished vector is the intended workflow; AI output is direction-finding.
- **System variant logged:** "edge-extending stems" — a variation where the leftmost stem's ascender and descender extend fully to the container's top and bottom edges rather than terminating with negative space. Surfaced spontaneously from regenerations (#23) and felt right. Carry forward into Phase 3 codification as a documented variant of the primary mark.
- **Phase 2 complete.** Path D + system variant + filled-primary treatment + slate-blue/white knockout palette + symbol+wordmark lockup are all decided. Ready for Phase 3.

#### Round 1 Conclusions
- Path A (single μ) and Path B (`hmn` ligature) are both viable; Path B is what we already responded to, Path A is conceptually purest. Keep both alive into Round 2.
- "μ ↔ `hmn`" is geometrically not a clean dual-read (3 arches don't compress to μ's 2). The cleaner dual-read is **μ ↔ Latin `m`-with-descender**, which is Path A.
- **Container language found:** thin-stroke outlined square frame. Non-skeuomorphic, modernist, system-tile.
- **Treatment system found:** outlined-container/stroke-glyph mode (#11) + filled-container/knockout-glyph mode (#7).
- **Generator behavior:**
  - **FLUX.2 Pro** = restraint + discipline (best for refining toward final).
  - **Nano Banana 2** = creative surprise (best for exploration / treatment ideas).
  - **FLUX.2 Flex** = noisier middle ground; deprioritize.

### Working Keywords
- "monoline modernist sans" — works (got us #10, #11)
- "thin-stroke outlined square container" — works (got us #11)
- "shared stems" / "ligature" — works for `hmn` ligature
- "Paul Rand", "Vignelli", "Müller-Brockmann" — works for restraint/authority
- "slate gray on white" — works for color discipline
- "knockout" / "filled container with white glyph" — to test for inverse mode

### Keywords to Drop / Avoid
- "Adrian Frutiger" — pulls toward serif/classical type
- Just "Greek lowercase μ" without modernist qualifiers — pulls toward classical Greek typography (serify, scholarly)
- "Bauhaus" alone — sometimes pulls toward poster art rather than mark-making

### Negative Prompts
- Avoid: gradients, drop shadows, glow, bevel, 3D, perspective, depth, glossiness
- Avoid: chiseled, carved, weathered, distressed, aged, antique, ornate, classical fluting
- Avoid: faceted gemstone, crystal, cut stone (reserved language)
- Avoid: brain, synapse, neural network, mesh, network graph
- Avoid: open book, scroll, parchment surface
- Avoid: mascot, character, face, eyes, smile

---

## Phase 3: Codification
- **Status:** v1.0 drafted 2026-04-26.
- **Artifact:** [`hypomnema-visual-identity.md`](./hypomnema-visual-identity.md)
- **Sections:** Brand Essence (4 adjectives + emotional intent + visual metaphors), Visual Language (symbol spec + edge-extending variant + Path B fallback + colors + treatment modes + containers + wordmark + lockup + anti-patterns), Prompt Engineering Guide (working/dropped keywords, output modifiers, format specs, vector finalization, generator-specific notes, negatives), Iteration Protocol (refinement vocabulary, quality checkpoints, common failure modes from Rounds 1–4), Example Prompt Library (5 working prompts: filled symbol, outlined symbol, lockup, edge-extending variant, favicon fallback) + Quick Reference Card.
- **Next:** Phase 4 (Use) — produce the actual marks. Hand-finish the vector from a Path D raster reference; deploy as GitHub avatar, favicon, README banner.

---

## Session Notes

### 2026-04-26
- Kicked off Phase 1.
- Established project context, output type, generator set, and primary use cases (logo/logotype first).
- Next: emotional territory + references + anti-patterns.
