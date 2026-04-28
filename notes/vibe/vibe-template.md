# [Project Name] — Generative Visual Identity

> **Purpose:** This document defines the visual language for AI-generated images. Use it to produce consistent, on-brand imagery across [primary use cases].
>
> **Output Type:** [Raster / Vector / Hybrid]
>
> **Target Generator(s):** 
> - Testing: [Gemini / Midjourney / etc.]
> - Production: [DALL-E 3 / Recraft / etc.]
> - Vector conversion: [Recraft / N/A]
>
> **Last Updated:** [Date]

---

## 1. Brand Essence

### Core Aesthetic (3-5 Adjectives)

<!-- For each adjective, define what it means AND what it doesn't mean in this context -->

| Adjective | What It Means Here | What It Doesn't Mean |
|-----------|-------------------|---------------------|
| [e.g., Modern] | [Clean geometry, generous whitespace, subtle gradients] | [Neon, chrome, harsh angles] |
| [Adjective 2] | [Definition] | [Anti-definition] |
| [Adjective 3] | [Definition] | [Anti-definition] |
| [Adjective 4] | [Definition] | [Anti-definition] |
| [Adjective 5] | [Definition] | [Anti-definition] |

### Emotional Intent

<!-- Complete this sentence: "When someone sees these images, they should feel..." -->

[Describe the emotional response you want. Be specific. Example: "A sense of calm focus — like the feeling of a clear morning before the day begins. Not sleepy or passive, but quietly energized."]

### Visual Metaphors

<!-- Abstract concepts that inform the visual direction -->

| Concept | Visual Expression |
|---------|------------------|
| [e.g., Clarity] | [Open skies, clean surfaces, single focal points] |
| [e.g., Growth] | [Organic shapes, upward movement, natural light] |
| [Concept 3] | [Visual expression] |

---

## 2. Visual Language

### Colors

#### Primary Palette

<!-- Include hex codes and emotional associations -->

| Color | Hex | Usage | Emotional Association |
|-------|-----|-------|----------------------|
| [Name] | #XXXXXX | [When to use] | [What it conveys] |
| [Name] | #XXXXXX | [When to use] | [What it conveys] |
| [Name] | #XXXXXX | [When to use] | [What it conveys] |

#### Color Rules

- **Do:** [e.g., "Use muted, desaturated tones. Favor warm neutrals."]
- **Don't:** [e.g., "Avoid pure black, neon colors, or high-saturation primaries."]

#### Prompt Keywords for Color

```
[Keywords that reliably produce your color palette]
Example: "muted earth tones, warm neutrals, soft sage green accents, cream and sand palette"
```

### Lighting

#### Preferred Lighting Styles

| Style | When to Use | Keywords |
|-------|-------------|----------|
| [e.g., Golden hour] | [Primary choice for warmth] | [soft golden light, warm afternoon sun] |
| [e.g., Soft diffused] | [For calm, neutral subjects] | [overcast lighting, soft shadows, even illumination] |
| [Style 3] | [Usage] | [Keywords] |

#### Lighting Anti-patterns

- Avoid: [e.g., "harsh midday sun, dramatic noir lighting, clinical studio lighting, flash photography look"]

### Composition

#### Framing Rules

- **Focal Point:** [e.g., "Off-center, following rule of thirds. Never dead center."]
- **Negative Space:** [e.g., "Generous. At least 40% of frame should be breathing room."]
- **Cropping:** [e.g., "Favor wide shots over tight crops. Show context."]

#### Composition Keywords

```
[Keywords that produce your composition style]
Example: "asymmetrical composition, generous negative space, rule of thirds, environmental context"
```

### Textures & Materials

#### Preferred

| Material/Texture | Visual Quality | Keywords |
|-----------------|----------------|----------|
| [e.g., Natural wood] | [Warm, organic grain] | [light oak, natural wood grain, warm timber] |
| [e.g., Linen/cotton] | [Soft, tactile, imperfect] | [linen texture, soft fabric, natural fibers] |
| [Material 3] | [Quality] | [Keywords] |

#### Avoid

- [e.g., "Glossy plastic, metallic chrome, synthetic materials, overly polished surfaces"]

### Subject Treatment

#### People (if applicable)

- **Appearance:** [e.g., "Candid, in motion or mid-task. Never posed or looking at camera."]
- **Styling:** [e.g., "Natural, minimal styling. Muted, professional clothing."]
- **Keywords:** [e.g., "candid moment, natural pose, lifestyle photography, unposed"]

#### Objects

- **Isolation:** [e.g., "Objects with subtle context, not floating in void."]
- **Arrangement:** [e.g., "Organic placement, slight imperfection, lived-in feel."]
- **Keywords:** [e.g., "styled flat lay, natural arrangement, soft shadows, contextual placement"]

#### Environments

- **Character:** [e.g., "Bright, airy spaces. Natural light. Plants and organic elements."]
- **Keywords:** [e.g., "bright interior, natural daylight, minimalist space, biophilic design"]

---

## 3. Prompt Engineering Guide

### Style Keywords That Work

<!-- Tested keywords that reliably produce on-brand results -->

```
[Core style string that can be appended to most prompts]

Example: "soft natural lighting, muted earth tone palette, minimalist composition, 
gentle shadows, warm and inviting atmosphere, professional photography style"
```

### Output Type Modifiers

<!-- Add these to your prompts based on your output type -->

**If targeting raster (for later vectorization):**
```
flat illustration style, solid colors, clean edges, 
minimal gradients, icon design, vector-friendly
```

**If targeting native vector (Recraft):**
```
minimalist icon, flat design, single color palette,
geometric shapes, scalable logo style
```

**If targeting photorealism:**
```
photorealistic, professional photography, studio lighting,
high detail, 8k resolution
```

**Your project's output type:** [Raster / Vector / Hybrid]

**Your standard output modifier:**
```
[The modifier string you use for your output type]
```

### Format Specifications

| Use Case | Aspect Ratio | Style Notes | Example Prompt Suffix |
|----------|-------------|-------------|----------------------|
| Blog header | 16:9 | [Wide, horizontal, text-safe left side] | [--ar 16:9 if Midjourney] |
| Social square | 1:1 | [Centered composition, bold subject] | [--ar 1:1] |
| Pinterest | 2:3 | [Vertical, scrollable, stacked elements] | [--ar 2:3] |
| [Use case] | [Ratio] | [Notes] | [Suffix] |

### Vector Finalization (Hybrid Workflow Only)

<!-- Skip this section if your output type is raster-only or vector-only -->

**When converting approved rasters to vectors:**

| Approach | When to Use | Prompt Modification | Cost |
|----------|-------------|---------------------|------|
| **Style Reference** | Capture the "vibe" in clean vectors | Add: "flat vector illustration, minimal gradients, clean geometric shapes" | $0.08/vector |
| **Direct Trace** | Exact reproduction of approved design | N/A (upload and vectorize) | $0.01/image |

**Your raster→vector prompt modification:**
```
[Your standard prompt additions when converting to vector]
```

### Negative Prompts

<!-- Things to ALWAYS exclude -->

```
[Standard negative prompt to append]

Example: "no text, no logos, no watermarks, no borders, no harsh shadows, 
no oversaturated colors, no artificial lighting, no stock photo feel"
```

### Generator-Specific Notes

<!-- Any quirks or tips for your target generator -->

**[Generator Name] Tips:**
- [e.g., "Gemini responds well to emotional descriptors before visual ones"]
- [e.g., "Use 'photograph of' for realism, 'illustration of' for stylized"]
- [e.g., "Avoid starting with articles (a, an, the) — jump into the subject"]

---

## 4. Iteration Protocol

### Refinement Vocabulary

Use these standardized terms when requesting changes:

| Adjustment | What to Say | Effect |
|------------|-------------|--------|
| Warmer colors | "warmer color temperature" | Shifts palette toward yellows/oranges |
| Cooler colors | "cooler color temperature" | Shifts palette toward blues |
| More contrast | "increase contrast" | Deeper shadows, brighter highlights |
| Less contrast | "softer contrast, flatter lighting" | More even tones |
| More space | "more negative space, pull back" | Subject smaller in frame |
| Less space | "tighter framing, closer crop" | Subject fills more frame |
| Simplify | "simplify composition, fewer elements" | Remove visual clutter |
| Add detail | "more intricate detail, add texture" | Increase visual complexity |
| Bolder lines | "increase line weight, bolder strokes" | For illustrations |
| Softer lines | "decrease line weight, delicate lines" | For illustrations |

### Quality Checkpoints

Before accepting an image as on-brand, verify:

- [ ] **Color:** Does the palette match? No jarring off-brand colors?
- [ ] **Lighting:** Does it feel [your lighting style]?
- [ ] **Composition:** Proper negative space? Focal point placement?
- [ ] **Mood:** Does it evoke [your emotional intent]?
- [ ] **Technical:** Right aspect ratio? No artifacts or distortions?

### Common Failure Modes & Fixes

| Problem | Likely Cause | Fix |
|---------|-------------|-----|
| [e.g., Too saturated] | [Generator default] | [Add "muted, desaturated" to prompt] |
| [e.g., Centered subject] | [Didn't specify composition] | [Add "rule of thirds, asymmetrical"] |
| [Problem 3] | [Cause] | [Fix] |

---

## 5. Example Prompt Library

### Template Structure

```
[Subject/Scene], [Style Keywords], [Lighting], [Composition], [Color/Mood], [Technical]
```

### Working Examples

#### Example 1: [Use Case, e.g., Blog Header — Productivity Topic]

**Prompt:**
```
[Full prompt that works]
```

**Expected Result:**
[Describe what this produces]

**When to Use:**
[What topics/contexts this fits]

---

#### Example 2: [Use Case]

**Prompt:**
```
[Full prompt that works]
```

**Expected Result:**
[Describe what this produces]

**When to Use:**
[What topics/contexts this fits]

---

#### Example 3: [Use Case]

**Prompt:**
```
[Full prompt that works]
```

**Expected Result:**
[Describe what this produces]

**When to Use:**
[What topics/contexts this fits]

---

## Quick Reference Card

### Copy-Paste Essentials

**Core Style String:**
```
[Your tested style keywords in one block]
```

**Standard Negative Prompt:**
```
[Your standard exclusions]
```

**Aspect Ratios:**
- Blog headers: [ratio]
- Social posts: [ratio]
- [Other]: [ratio]

---

## Revision History

| Date | Changes | Version |
|------|---------|---------|
| [Date] | Initial creation | 1.0 |

---

*Generated using the Generative Visual Identity Workflow. For questions about this document or to refine the system, reference vibe.md.*
