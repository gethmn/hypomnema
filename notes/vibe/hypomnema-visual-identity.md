# Hypomnema — Generative Visual Identity

> **Purpose:** This document defines the visual language for Hypomnema's logo, logotype, and generated imagery. Use it to produce consistent, on-brand marks across the GitHub Organization, repository, README/docs, website, and favicons.
>
> **Output Type:** Hybrid leaning vector — final deliverables are vector logo/logotype; raster is acceptable for exploration and hero imagery.
>
> **Target Generator(s):**
> - Exploration (raster): Gemini (Nano Banana 2), FLUX.2 Pro, FLUX.2 Flex — via OpenRouter
> - Vector finalization: Recraft (style-reference free tier) or hand-redrawn from approved raster
> - Production rule: AI output is direction-finding, not final art. Bauhaus-grade modernist construction is generally outside what current generators nail; the final mark is hand-finished in vector.
>
> **Last Updated:** 2026-04-26

---

## 1. Brand Essence

### Core Aesthetic (4 Adjectives)

| Adjective | What It Means Here | What It Doesn't Mean |
|-----------|-------------------|---------------------|
| **Structured** | Internal logic is visible. Mathematical relationships, geometric primitives, decompositions that *mean something*. The viewer can see how the form is built. | "Tidy." Decorative grids. Visible scaffolding for its own sake. |
| **Reliable** | The mark behaves the same in every context. Same identity and silhouette at 16×16 as at 512×512 (with documented scale-conditional swaps to the Path B fallback at favicon size). Recognizable across square / circle / squircle. No surprises. | Rigid. One-size-fits-all. Refusing to adapt to scale. |
| **Simple** | Minimum parts; each part load-bearing. If something can be removed without loss, it has been. | Empty. Generic minimalism. Whitespace as a substitute for design. |
| **Pure** | Formal restraint. No skeuomorphism, no weathering, no faux-texture, no warmth applied as a finish. The form is the form. Modernist asceticism — closer to Rams than to Aurelius. | Cold. Sterile. Devoid of intent. |

### Emotional Intent

> *This is a quiet, well-engineered piece of infrastructure that takes its own etymology seriously — the kind of tool a careful person made for careful people.*

The mark should read as **disciplined**, not decorative. As **considered**, not casual. The viewer doesn't need to know the Greek to feel the rigor; if they do know, the dual-reading should feel earned, not announced.

### Visual Metaphors

| Concept | Visual Expression |
|---------|------------------|
| **Substrate** | The container is what holds the inscription. Hypomnema is the readable layer beneath the user's notes. Expressed as the square/circle/squircle field. |
| **Inscription** | The glyph is what has been *written* on the substrate. Not carved, not chiseled — placed precisely. Expressed as the white knockout glyph, sharp-edged and modernist-sans. |
| **Catalog discipline** | Single primitive + single glyph; the system is the rule, not the variation. Expressed as scale-aware rendering: same form at every size, with detail layers added only as scale permits. |
| **Modernist monogram** | The Paul Rand / Vignelli / Müller-Brockmann tradition: a few primitives, mathematically related, each doing one job. Expressed in stem rhythm and arch construction. |

### Reference Pocket

Conceptual references that ground the visual language. The first four are **safe to use as prompt keywords**; the last two are **conceptual touchstones only** — invoking them in prompts pulls in unwanted vocabulary (poster art, serifs).

| Reference | Use For | Safe in Prompts? |
|-----------|---------|------------------|
| **Dieter Rams** (Braun) | Restraint, "less but better" | Yes |
| **Massimo Vignelli** (Unimark / NYC subway / IBM-era identity) | Modernist authority | Yes |
| **Josef Müller-Brockmann** (Swiss school) | Grid discipline | Yes |
| **Paul Rand** | Modernist monogram tradition; IBM 8-bar is our lockup model | Yes |
| **Bauhaus / Swiss style** (generally) | Conceptual heritage | No — bare "Bauhaus" pulls toward poster art. Use Vignelli / Müller-Brockmann instead in prompts. |
| **Penguin Classics spines** | The *restraint* and authority of catalog typesetting | No — pulls in serifs we explicitly exclude. Reference for thinking only. |

---

## 2. Visual Language

### The Symbol (Primary Mark)

The Hypomnema symbol is a custom dual-reading glyph: a single shape that simultaneously reads as `h` / `m` / `n` (Latin) and `μ` (Greek lowercase mu — first letter of μνήμη, "memory").

The Latin reading is not arbitrary — `hmn` is the project's CLI binary name. The mark is therefore a literal monogram of the tool's invocation alongside the Greek root of "memory." One glyph, two literacies, both load-bearing.

**Construction:**
- **2 arches, 3 stems.** Two clean modernist arches connecting three vertical stems.
- **Leftmost stem extends both above x-height and below baseline.**
  - Above x-height: reads as the ascender of Latin `h`.
  - Below baseline: reads as the descender of Greek `μ`.
- **Maximum overlap.** Letters share stems and arches; the glyph is one form, not three letters tracked together.
  - `h` = left tall stem + arch 1
  - `m` = the entire 3-stem-2-arch silhouette
  - `n` = right 2 stems + arch 2
  - `μ` = the entire form, with the descender foregrounded
- **No internal articulation.** Single solid shape. No bevels, no double-strokes, no thin internal outlines. Counter-spaces (white knockouts) do all the letterform work.
- **Monoline weight.** Stems and arches share a uniform stroke weight. Modernist geometric construction — think Avenir or Futura, not Helvetica's slight modulation.

**System variant — edge-extending stems:**
Documented variant in which the leftmost stem's ascender and descender extend fully to the container's top and bottom edges (rather than terminating short with negative space above/below). Carries the same dual-reading; trades airy proportion for structural authority. Use when the mark needs to feel anchored (large display use, app icons), reserve the standard form for default use.

**Fallback (Path B):**
If the dual-reading glyph fails legibility tests at a given scale, the fallback is the same 2-arch / 3-stem `m`-construction with the leftmost stem extending **only above** x-height (no descender). Reads cleanly as `h` / `m` / `n` without attempting the Greek dual-read. This is a known-safe fallback, not a downgrade — it keeps the same DNA.

### Colors

#### Primary Palette

| Color | Hex | Usage | Emotional Association |
|-------|-----|-------|----------------------|
| **Slate Blue** | `#5A7185` | Symbol field (filled mode); wordmark; primary brand color | Cool, considered, infrastructural |
| **Slate Gray** | `#4A5568` | Symbol stroke (outlined mode); secondary text | Restraint, neutrality |
| **Paper White** | `#FFFFFF` | Symbol glyph (knockout in filled mode); negative space | Clarity, the inscription itself |

#### Color Rules

- **Do:** Hold to cool slate / cool gray / almost-blue. Monochrome-friendly. The brand is the geometry, not the color — the same form must read in pure black-on-white and white-on-black.
- **Don't:** Warm tones, saturated primaries, gradients, or two-color systems. Departures from cool require justification (and probably belong in hero imagery, not the mark).

#### Prompt Keywords for Color

```
slate blue, cool gray, almost-blue, monochrome-friendly, flat color, no gradients, white knockout
```

### Treatment Modes

The symbol has two treatment modes. **Filled is primary; outlined is secondary.**

| Mode | When to Use | Spec |
|------|-------------|------|
| **Filled (knockout)** | Default. Avatars, favicons, hero, anywhere the mark sits alone. | Slate-blue field, white knockout glyph. Counter-spaces carry the letterform. |
| **Outlined** | Secondary. Small inline contexts, low-contrast surfaces, line-only contexts (e.g., monochrome print). | Thin slate-gray stroke for both container and glyph. Monoline weight. |

**Why filled is primary:** Round 2/3 testing showed counter-spaces (white knockouts) articulate the dual-reading better than outlined-stroke construction. Outlined mode tends to attenuate the glyph's discipline.

### Containers (Aspect Ratios)

The mark must work in **square**, **circle**, and **squircle** containers, all with the symbol centered and proportionally scaled to the same optical weight.

| Container | Use Case | Notes |
|-----------|----------|-------|
| **Square** | Default. GitHub org/repo avatar, favicons, README badges. | Sharp 90° corners. |
| **Circle** | Profile contexts that round automatically (some social platforms). | Symbol must remain centered when the container is masked to a circle. |
| **Squircle** | iOS/macOS app-icon contexts, modern UI. | Apple-style superellipse rounding. |

**Scale discipline (16×16 favicon as the test):** At 16×16 the standard form may lose the descender/ascender extension. The fallback (Path B, ascender only) is permitted at favicon scale. Below 16×16 is not a target; the mark is not designed to be legible at sub-favicon sizes.

### The Wordmark

**Spelling:** `hypomnema` — all lowercase.

**Typeface family:** Geometric modernist sans. In order of preference:
1. **Avenir** — preferred default (geometric humanist sans, 1988)
2. **Univers** — alternate when slightly more humanist warmth is wanted
3. **DIN** — alternate when more engineered/industrial feel is wanted

Avoid: serifs, slabs, monospace, anything decorative. Helvetica is acceptable but reads as more generic-corporate than the project warrants.

**Note for prompts:** Invoke "Avenir" or "Avenir-style geometric sans" by typeface name. **Do not invoke "Adrian Frutiger" by designer name** — it pulls generators toward his serif and classical-typography work. See § 3 "Keywords to Drop / Avoid."

**Color:** Slate Blue (`#5A7185`) — color-matched to the symbol's field.

**Sizing:** x-height of the wordmark matches the interior (arch-cap to baseline) height of the symbol.

### Lockup

**Model:** Paul Rand IBM 8-bar — symbol + wordmark in horizontal lockup, with the wordmark to the right of the symbol.

**Spacing:** The space between symbol and wordmark equals the width of one symbol stem.

**Vertical alignment:** Wordmark x-height aligned to the symbol's interior cap line (top of the arches). The symbol's ascender extends above the wordmark; the descender extends below.

**Single-mark uses:** When space is tight (favicon, avatar, app icon), use the symbol alone in its container. The wordmark is for headers, footers, hero contexts, README banners.

### Anti-patterns (do not do)

- Brain / neural-network / mesh / synapse imagery
- Generic open-book or scroll iconography
- Cute mascots, illustrated characters, faces
- Faceted gem / cut-stone / crystal language (reserved for the user's Scind/Xcind work; do not reuse)
- Skeuomorphism — the mark is not pretending to be a tablet, chip, card, or carved stone
- Weathering, chiseling, distressed surfaces, antique texture
- Ornamental classicism — the etymology is the soul, not the surface
- Internal articulation in the glyph (bevels, double-strokes, thin internal outlines, multi-tone fills)
- Warm color tones unless explicitly justified

---

## 3. Prompt Engineering Guide

### Style Keywords That Work

```
monoline modernist sans, geometric construction, single solid shape,
white knockout on slate-blue field, flat color, no gradients,
2-arch 3-stem ligature, shared stems, Vignelli, Müller-Brockmann, Paul Rand
```

### Output Type Modifiers

**For raster exploration (FLUX.2 Pro / Nano Banana / Flex):**
```
flat vector-friendly illustration, solid colors, clean edges,
no gradients, no shadows, no 3D, icon design, logo mark,
white background or single-color flat field
```

**For native vector via Recraft style-reference:**
```
minimalist vector logo, flat design, single color palette,
geometric shapes, scalable mark, monoline construction
```

### Working Keywords

| Keyword | Purpose |
|---------|---------|
| `monoline modernist sans` | Locks the construction to clean, uniform-weight geometric sans |
| `thin-stroke outlined square container` | Produces the system tile (outlined mode) |
| `slate-blue field with white knockout glyph` | Produces filled-mode treatment |
| `shared stems` / `ligature` | Triggers the fused-letter construction |
| `Paul Rand`, `Vignelli`, `Müller-Brockmann` | Pulls toward modernist restraint and authority |
| `flat color, no gradients` | Holds the generator to vector-friendly output |

### Keywords to Drop / Avoid

| Keyword | Why It Fails |
|---------|--------------|
| `Adrian Frutiger` | Pulls toward serif/classical type vocabulary |
| `Greek lowercase μ` (alone) | Pulls toward classical Greek typography (serify, scholarly) |
| `Bauhaus` (alone) | Sometimes pulls toward poster art rather than mark-making |
| `engraved`, `chiseled`, `inscribed` | Pulls toward skeuomorphic stone/metal surfaces |
| `crystal`, `faceted`, `gem` | Reserved language for Scind/Xcind; would imply house style |

### Format Specifications

| Use Case | Aspect Ratio | Style Notes |
|----------|-------------|-------------|
| GitHub org/repo avatar | 1:1 | Symbol-only, filled mode, square container |
| Favicon | 1:1 (16×16) | Symbol-only, filled mode; fallback (Path B) permitted |
| README banner | ~3:1 | Lockup (symbol + wordmark) on white field |
| Website hero | 16:9 | Lockup or symbol with generous negative space |
| Social square | 1:1 | Lockup centered or symbol-only |

### Vector Finalization

**Hybrid raster-to-vector workflow:**

| Approach | When to Use | Notes |
|----------|-------------|-------|
| **Recraft style-reference** | Translate an approved raster to vectors with the same vibe | Upload best raster, prompt with `flat vector logo, monoline construction, single color, minimalist mark`. Free tier. |
| **Hand-redraw in vector** | Final production mark | Path D especially — the dual-extending stem is hard for any current generator. Hand-redraw from the closest raster reference. |

The committed final-art workflow is **hand-redraw**. AI is for exploring proportions, weights, and treatment variants — not for producing the deliverable.

### Negative Prompts

```
no gradients, no drop shadows, no glow, no bevel, no 3D,
no perspective, no depth, no glossiness, no texture,
no chiseled, no carved, no weathered, no distressed, no aged,
no antique, no ornate, no classical fluting,
no faceted gemstone, no crystal, no cut stone,
no brain, no synapse, no neural network, no mesh, no network graph,
no open book, no scroll, no parchment surface,
no mascot, no character, no face, no eyes, no smile
```

### Generator-Specific Notes

**FLUX.2 Pro:**
- Highest discipline ceiling. Best for refinement passes once a direction is committed.
- Variable round-to-round; sometimes loses discipline on first try. Re-roll if first result is bland or off-spec.
- Tends to add spurious thin slate artifacts inside white knockouts — these are quirks, not features. Drop them in vector translation.

**Gemini (Nano Banana 2):**
- Repeatable creative aesthetic. Best for exploring treatment ideas (color, container, knockout vs outline).
- Signature drift on Greek-letter prompts: produces "lemniscate-with-stem" (closed bowls instead of open arches). Useful when surfaced; hold the line on `2 arches, open` when not wanted.
- Sometimes ignores explicit arch/stem counts and reverts to a default 3-arch interpretation.

**FLUX.2 Flex:**
- Surprisingly strong on filled-mode prompts. Promoted to peer with Pro/Nano Banana, not deprioritized.
- Cleanest Path D execution in Round 3 (T3 #19a). Lead with Flex when prompting for the dual-reading glyph.

**Recraft:**
- Free tier; reserve for late-stage vector translation, not exploration.
- Style-reference mode produces cleaner vectors than direct trace; slight off-spec drift on the dual-extending stem is expected — finish by hand.

---

## 4. Iteration Protocol

### Refinement Vocabulary

| Adjustment | What to Say |
|------------|-------------|
| Make the descender longer | `extend the leftmost stem further below baseline` |
| Make the ascender longer | `extend the leftmost stem further above x-height` |
| Drop spurious internal lines | `single solid shape, no internal articulation, no double-strokes` |
| Switch treatment | `filled mode: slate-blue field, white knockout glyph` / `outlined mode: thin slate-gray stroke, no fill` |
| Hold to 2 arches | `exactly 2 arches and 3 stems, open arches, no closed bowls` |
| Tighten stem rhythm | `equal stem spacing, monoline weight, geometric construction` |
| Center in container | `symbol centered in container, equal padding all sides` |

### Quality Checkpoints

Before accepting a generated image as on-brand, verify:

- [ ] **Construction:** Exactly 2 arches and 3 stems? Leftmost stem extending both above and below (or above-only for fallback)?
- [ ] **Treatment:** Filled mode = slate-blue field + white knockout glyph; outlined mode = thin slate-gray monoline?
- [ ] **Articulation:** Single solid shape, no internal lines, no bevels, no double-strokes?
- [ ] **Proportion:** Stems equal-spaced, monoline weight, modernist construction (not handwritten, not calligraphic)?
- [ ] **Container:** Cleanly square / circle / squircle? Symbol centered with equal padding?
- [ ] **Color:** Hex within slate-blue / slate-gray / white range? No accidental warm tints?
- [ ] **Wordmark (lockup only):** "hypomnema" spelled correctly, lowercase, geometric sans, color-matched to field?
- [ ] **Mood:** Quiet, well-engineered, infrastructural — not decorative or whimsical?

### Common Failure Modes & Fixes

| Problem | Likely Cause | Fix |
|---------|-------------|-----|
| Generator produces 3 arches / 4 stems | Default Latin `m` interpretation | Add `exactly 2 arches and 3 stems, maximum overlap` |
| Closed bowls instead of open arches (lemniscate) | Nano Banana drift on Greek-letter prompts | Add `open arches, not closed bowls, modernist sans construction` |
| Serif on the glyph | Greek-letter prompt without modernist qualifier | Drop "Greek lowercase μ"; add `monoline modernist sans, Avenir-style geometric` |
| Spurious thin lines inside white knockout | FLUX.2 Pro quirk | Add `single solid shape, no internal articulation`; expect to clean up in vector regardless |
| Descender drops, ascender stays | Generator didn't parse dual-extension | Add `the leftmost stem extends BOTH above x-height AND below baseline` (caps for emphasis) |
| Wordmark mangled | Generator default | "hypomnema" generally renders OK across all three; if mangled, regenerate or hand-set the wordmark |
| Warm tint creeping in | Generator default for "logo" | Add `cool slate blue, no warm tones, hex around #5A7185` |

---

## 5. Example Prompt Library

### Template Structure

```
[Subject + construction], [color treatment], [style references],
[container], [output type modifier], [negatives]
```

### Working Examples

#### Example 1: Symbol alone, filled mode (default)

**Prompt:**
```
A modernist logo mark: a single solid shape forming a 2-arch, 3-stem ligature
in white knockout on a slate-blue (#5A7185) square field. The leftmost stem
extends both above x-height (h-ascender) and below baseline (μ-descender).
Reads simultaneously as Latin h/m/n and Greek lowercase mu. Monoline weight,
geometric construction. Style of Massimo Vignelli, Paul Rand, Müller-Brockmann.
Flat color, no gradients, no shadows, no internal articulation. Centered,
equal padding.
```

**Expected Result:** The committed Path D mark in filled mode. Use as default avatar / favicon source.

**When to Use:** Default symbol generation; GitHub org/repo avatar exploration.

---

#### Example 2: Symbol alone, outlined mode (secondary)

**Prompt:**
```
A modernist logo mark in outlined mode: thin slate-gray (#4A5568) monoline
stroke forming a 2-arch, 3-stem ligature inside a thin-stroke outlined square
container. The leftmost stem extends both above x-height and below baseline.
Reads as Latin h/m/n and Greek mu simultaneously. Geometric construction,
modernist sans. Style of Vignelli, Müller-Brockmann. Flat, no fill, no
gradients, no shadows. White background.
```

**Expected Result:** Outlined-mode variant. Use sparingly — secondary treatment.

**When to Use:** Inline contexts, monochrome print, line-only constraints.

---

#### Example 3: Lockup (symbol + wordmark)

**Prompt:**
```
A modernist horizontal logo lockup: a slate-blue (#5A7185) square symbol on
the left containing a white-knockout 2-arch 3-stem ligature glyph (leftmost
stem extends above and below). The wordmark "hypomnema" set in geometric
modernist sans (Avenir-style), all lowercase, in slate-blue, sits to the
right of the symbol. Wordmark x-height aligns to the symbol's interior cap
line. Spacing equals one stem-width. IBM 8-bar lockup model. Flat color,
no gradients. White background.
```

**Expected Result:** README banner / website header lockup.

**When to Use:** Anywhere the symbol has horizontal companion space — README, web header, social bio, footer.

---

#### Example 4: Symbol with edge-extending stems variant

**Prompt:**
```
A modernist logo mark: white knockout 2-arch 3-stem ligature on a slate-blue
(#5A7185) square field. The leftmost stem's ascender extends fully to the
top edge of the container; its descender extends fully to the bottom edge.
Other two stems terminate at x-height and baseline. Maximum overlap; reads
as h/m/n and Greek mu. Monoline weight, geometric construction. Flat color,
no internal articulation. Style of Vignelli, Paul Rand.
```

**Expected Result:** The system variant — anchored, structurally authoritative.

**When to Use:** Large display, app icons, contexts where the mark needs more visual weight than the standard form.

---

#### Example 5: Fallback (Path B), favicon-scale

**Prompt:**
```
A modernist logo mark for favicon use: a single solid shape forming a 2-arch,
3-stem ligature in white knockout on a slate-blue (#5A7185) square field.
The leftmost stem extends only above x-height (h-ascender). Reads as Latin
h/m/n. Monoline weight, geometric construction, Vignelli style. Flat color,
no internal articulation. Optimized for legibility at 16×16 pixels.
```

**Expected Result:** The Path B fallback — same DNA, descender omitted for scale.

**When to Use:** Favicon and sub-32px contexts where the descender would muddle.

---

## Quick Reference Card

### Copy-Paste Essentials

**Core Style String:**
```
modernist logo mark, 2-arch 3-stem ligature with shared stems,
leftmost stem extends above and below, white knockout on slate-blue
(#5A7185) field, monoline weight, geometric construction,
style of Vignelli and Paul Rand, flat color, no gradients,
single solid shape, no internal articulation
```

**Standard Negative Prompt:**
```
no gradients, no shadows, no 3D, no bevel, no glow, no perspective,
no texture, no chiseled, no carved, no weathered, no antique,
no ornament, no faceted gem, no crystal, no cut stone,
no brain, no neural network, no synapse, no mesh,
no open book, no scroll, no mascot, no face, no eyes
```

**Palette:**
- Slate Blue field: `#5A7185`
- Slate Gray stroke: `#4A5568`
- Paper White knockout: `#FFFFFF`

**Aspect Ratios:**
- Avatar / favicon: 1:1
- Lockup banner: ~3:1
- Hero: 16:9

**Generator Lead-Order (by use):**
- Path D dual-reading glyph → FLUX.2 Flex first, then Pro
- Filled-mode treatment exploration → Nano Banana 2
- Outlined-mode refinement → FLUX.2 Pro
- Vector finalization → Recraft style-reference, then hand-finish

---

## Revision History

| Date | Changes | Version |
|------|---------|---------|
| 2026-04-26 | Initial codification from Phase 2 testing (Rounds 1–4) | 1.0 |

---

*Generated using the Generative Visual Identity Workflow. For questions about this document or to refine the system, reference `vibe.md` and the Phase 1–2 progress in `vibe-progress.md`.*
