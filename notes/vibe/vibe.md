# Generative Visual Identity Workflow

> **What this is:** A guided workflow for developing a "Generative Visual Identity" — a markdown document that captures your aesthetic intent in a way Claude can use to generate consistent, on-brand image prompts for AI image generators (Gemini, Midjourney, DALL-E, etc.)

---

## How to Use This Workflow

### Trigger Commands

Use these phrases to navigate the workflow:

| Command | What it does |
|---------|--------------|
| `@vibe.md phase 1` | Start or continue **Phase 1: Discovery** — defining your aesthetic foundation |
| `@vibe.md phase 2` | Start or continue **Phase 2: Prompt Testing** — testing and refining prompt language |
| `@vibe.md phase 3` | Start or continue **Phase 3: Codification** — producing the final artifact |
| `@vibe.md status` | Show current progress and what's been captured so far |
| `@vibe.md save` | Save current progress to `vibe-progress.md` |
| `@vibe.md generate [use case]` | Generate a test prompt based on current progress (e.g., "generate blog header about productivity") |
| `@vibe.md finalize` | Export the final design system artifact |
| `@vibe.md evolve [filename]` | Start evolving an existing visual identity (see "Evolving an Existing Identity" below) |

**After finalizing (Phase 4),** use your `[project]-visual-identity.md` file directly — see "Phase 4: Using Your Visual Identity" below.

### File Structure

**During development (Phases 1-3):**
```
your-project/
├── vibe.md              ← This file (instructions)
├── vibe-template.md     ← Template for the final artifact
├── vibe-progress.md     ← Your working progress (auto-created)
└── [project]-visual-identity.md  ← Final artifact (created in Phase 3)
```

**For ongoing use (Phase 4):**
```
your-project/
└── [project]-visual-identity.md  ← Only this file needed for image generation
```

**To evolve later:**
```
your-project/
├── vibe.md              ← Add back temporarily
├── [project]-visual-identity.md  ← Your existing identity (the baseline)
└── vibe-progress.md     ← Created fresh to track evolution
```

> **Note:** The progress file is disposable. Once you finalize, you can delete `vibe-progress.md` — your visual identity file contains everything that matters. If you return to evolve, Claude will read your identity file directly and create a fresh progress file for tracking changes.

> **Tip:** Keep `vibe.md` and `vibe-template.md` in a central location (like a "tools" or "templates" folder) rather than copying them into every project. You only need them during development or evolution — add them to a project temporarily when needed, then remove them when done.

---

## Phase 1: Discovery

**Goal:** Extract and articulate your aesthetic intent.

### What Claude Will Ask About

1. **Project Context**
   - What is this design system for? (blog, product, brand, personal project)
   - Who is the audience?
   - What's the content type? (photography, illustrations, abstract, mixed)

2. **Output Type & Generator**
   - What final format do you need? (raster, vector, or hybrid)
   - Which image generator(s) will you use?
   - See `ai-image-generation-comparison.md` for detailed recommendations

3. **Emotional Territory**
   - What 3-5 adjectives describe the feeling you want?
   - What should someone feel when they see these images?
   - What visual metaphors resonate? (e.g., "open sky = possibility")

4. **Reference Points**
   - Existing brands, artists, or styles you admire
   - Things you explicitly want to avoid
   - Any existing assets or brand guidelines to consider

5. **Practical Constraints**
   - Primary use cases (blog headers, social posts, presentations, etc.)
   - Aspect ratios needed
   - Any technical requirements (file formats, sizes)

### Output Type Decision

Early in Phase 1, determine your output type:

| Output Type | What It Means | Recommended Generator(s) |
|-------------|---------------|--------------------------|
| **Raster only** | Photos, illustrations, marketing images | Gemini (testing) → DALL-E 3 (production) |
| **Vector only** | Logos, icons, SVG assets | Recraft |
| **Hybrid** | Explore in raster, finalize in vector | Gemini/DALL-E → Recraft (style reference) |

**If hybrid:** You'll develop prompts for raster exploration first, then translate winning concepts to vector in Phase 4. Your prompts should include "vector-friendly" modifiers (flat colors, clean edges) to make the transition smoother.

### Phase 1 Outputs

By the end of Phase 1, you should have captured:
- [ ] Project name and context
- [ ] Output type (raster / vector / hybrid)
- [ ] Target generator(s) for testing and production
- [ ] 3-5 core aesthetic adjectives with definitions
- [ ] Emotional intent statement
- [ ] Visual metaphors list
- [ ] Reference artists/brands/styles
- [ ] Anti-patterns (what to avoid)
- [ ] Primary use cases and formats

### Phase 1 Prompts for Claude

When starting Phase 1, Claude should:

```
1. Check if vibe-progress.md exists and load any existing work
2. If new project: Ask about project context first
3. Guide through emotional territory questions
4. Capture reference points and anti-patterns
5. Summarize and confirm before moving to Phase 2
```

---

## Phase 2: Prompt Testing

**Goal:** Translate aesthetic intent into reliable prompt language through iteration.

### The Testing Loop

```
┌─────────────────────────────────────────────────────┐
│  1. Claude generates a test prompt based on        │
│     Phase 1 discoveries                            │
│                    ↓                               │
│  2. You run the prompt in your image generator    │
│     (Gemini, Midjourney, etc.)                    │
│                    ↓                               │
│  3. You describe results or share images          │
│     - What worked?                                │
│     - What missed the mark?                       │
│                    ↓                               │
│  4. Claude refines the prompt vocabulary          │
│                    ↓                               │
│  5. Repeat until consistent results               │
└─────────────────────────────────────────────────────┘
```

### What to Test

For each major category, test prompts and record what works:

| Category | Test Prompts For | Record |
|----------|-----------------|--------|
| **Color** | Overall palette, specific color usage | Keywords that produce right colors |
| **Lighting** | Mood lighting, shadows, highlights | Lighting descriptors that work |
| **Composition** | Framing, negative space, focal points | Composition language that works |
| **Texture** | Materials, surfaces, detail level | Texture/material keywords |
| **Style** | Overall aesthetic, rendering style | Style keywords and artist references |
| **Subjects** | People, objects, environments | Subject treatment language |

### Iteration Vocabulary

Build a shared vocabulary for refinement requests:

**Adjustments you might request:**
- "warmer/cooler color temperature"
- "increase/decrease contrast"
- "more/less negative space"
- "simplify/add detail"
- "increase/decrease line weight"
- "more/less saturated"
- "tighter/looser framing"
- "add/remove [specific element]"

### Phase 2 Outputs

By the end of Phase 2, you should have:
- [ ] Tested prompt templates for each use case
- [ ] Verified style keywords that produce consistent results
- [ ] Documented negative prompts (what to exclude)
- [ ] Established refinement vocabulary
- [ ] At least 3 successful prompt examples with descriptions

### Phase 2 Prompts for Claude

When in Phase 2, Claude should:

```
1. Load Phase 1 discoveries from vibe-progress.md
2. Generate test prompts starting with highest-priority use case
3. After each test, ask:
   - "What worked in the result?"
   - "What missed the mark?"
   - "On a scale of 1-5, how on-brand was it?"
4. Document successful keywords and patterns
5. Build the prompt vocabulary incrementally
```

---

## Phase 3: Codification

**Goal:** Produce the final, usable design system artifact.

### Artifact Structure

The final artifact follows the template in `vibe-template.md`:

1. **Brand Essence** — Core adjectives, emotional intent, metaphors
2. **Visual Language** — Colors, lighting, composition, textures, subjects
3. **Prompt Engineering Guide** — Tested keywords, formats, negative prompts
4. **Iteration Protocol** — Refinement vocabulary, quality checkpoints
5. **Example Prompt Library** — Working prompts for each use case

### Quality Checklist

Before finalizing:
- [ ] All sections filled with specific, tested content
- [ ] At least 3 example prompts that reliably produce on-brand results
- [ ] Negative prompts documented for common failure modes
- [ ] Refinement vocabulary is practical and tested
- [ ] Document can stand alone (no context needed)

### Phase 3 Prompts for Claude

When in Phase 3, Claude should:

```
1. Load all progress from vibe-progress.md
2. Populate vibe-template.md with discovered content
3. Review each section for completeness
4. Generate the final artifact as [project]-visual-identity.md
5. Provide the file for download/use
```

---

## Progress File Format

The `vibe-progress.md` file tracks state between sessions:

```markdown
# Vibe Progress: [Project Name]

## Meta
- **Current Phase:** 1 | 2 | 3
- **Last Updated:** [date]
- **Target Image Generator:** Gemini | Midjourney | DALL-E | Other

## Phase 1: Discovery
### Project Context
[captured content]

### Aesthetic Foundation
[captured content]

### References & Anti-patterns
[captured content]

## Phase 2: Testing
### Tested Prompts
[prompt] → [result description] → [rating] → [learnings]

### Working Keywords
[category]: [keywords that work]

### Negative Prompts
[things to always exclude]

## Phase 3: Codification
### Draft Sections
[work in progress on final artifact]

## Session Notes
### [Date]
[notes from this session]
```

---

## Tips for Best Results

### During Discovery (Phase 1)
- Be specific about what you mean by abstract terms ("modern" means different things to different people)
- Share examples liberally — "like X but with Y"
- Don't worry about prompt language yet; focus on intent

### During Testing (Phase 2)
- Test one variable at a time when possible
- Keep notes on what the AI generator interprets differently than expected
- Save your favorite outputs with the exact prompt that made them

### During Codification (Phase 3)
- Write for your future self who has forgotten the context
- Include enough examples that patterns are clear
- Test the final document by generating new prompts from it

---

## Example Session Flow

```
You: @vibe.md phase 1

Claude: I'll help you develop a generative visual identity. Let's start 
with the basics.

What project is this design system for? Tell me about:
- What you're creating (blog, product, brand, etc.)
- Who will see these images
- What kinds of images you'll need most often

You: It's for my personal blog about mindfulness and productivity. 
Audience is professionals 30-45. Mostly need blog header images.

Claude: Great foundation. Now let's explore the feeling you want...
[continues through Phase 1 questions]

You: @vibe.md save

Claude: Progress saved to vibe-progress.md. You're in Phase 1 with:
- Project: Personal blog (mindfulness/productivity)
- Audience: Professionals 30-45
- Primary use: Blog headers
[... summary of captured content]

Next session, say "@vibe.md phase 1" to continue where we left off.
```

---

## Evolving an Existing Identity

**Scenario:** You finalized your visual identity months ago, have been using it successfully, but now want to make significant changes — not just tweaks, but a real evolution of the style.

### What You Need

- Your existing `[project]-visual-identity.md` file
- This `vibe.md` file (add it back to your project temporarily)
- Optionally: `vibe-template.md` if you want to create a v2 from scratch

**You do NOT need** your original `vibe-progress.md` — that was just a working scratchpad. Your finalized visual identity contains everything that matters.

### The Evolve Command

```
@vibe.md evolve myproject-visual-identity.md
```

This tells Claude to:

1. **Read your existing visual identity** as the baseline (this replaces Phase 1 — you already know who you are)
2. **Ask what you want to evolve** — what's not working? what's changed? what new direction?
3. **Jump into Phase 2 testing** with your existing identity as context
4. **Track only the changes** in a fresh progress file
5. **Update your identity file** or create a v2 when done

### Example Workflow

```
You: @vibe.md evolve mindful-blog-visual-identity.md

Claude: I've read your existing visual identity for "Mindful Blog". Here's 
what I see as your current foundation:

- Core aesthetic: Calm, Focused, Warm, Minimal, Organic
- Primary palette: Sage green, warm cream, soft terracotta
- Lighting: Golden hour, soft diffused
- Style: Lifestyle photography with generous negative space

What would you like to evolve? Some options:
1. Shift the overall mood/feeling
2. Update the color palette
3. Change the visual style (e.g., photography → illustration)
4. Add new use cases or formats
5. Refine what's not working well
6. Something else

You: The style feels too safe now. I want to push toward something 
more dynamic and editorial — still calm but with more visual tension.

Claude: Interesting evolution. Let's explore "calm but with visual tension."
That might mean:
- Keeping your muted palette but adding one bold accent
- Introducing more dramatic lighting contrasts
- Playing with asymmetry and unconventional crops
- Adding motion blur or implied movement

Let's test some prompts. I'll generate one that keeps your foundation 
but pushes toward this new direction...

[Proceeds into Phase 2 testing loop]
```

### Minor vs. Major Changes

| Change Type | Approach |
|-------------|----------|
| **Minor tweaks** | Edit your visual identity file directly. No process needed. |
| **New use case** | Add a section to your file, test a few prompts, document what works. |
| **Palette refresh** | Could go either way — if you know what you want, just update. If exploring, use `evolve`. |
| **Major evolution** | Use `@vibe.md evolve` to systematically test and validate the new direction. |
| **Complete rebrand** | Start fresh with `@vibe.md phase 1` — treat it as a new identity. |

### After Evolving

When you're happy with the evolution:

1. **Update in place:** Modify your existing `[project]-visual-identity.md` with the new tested content
2. **Or create v2:** Generate `[project]-visual-identity-v2.md` if you want to keep the original
3. **Archive the progress file:** Or delete it — it's served its purpose
4. **Remove vibe.md** from your project until you need it again

---

## Phase 4: Using Your Visual Identity

**Goal:** Generate on-brand images using your completed design system.

Once you've finalized your visual identity document (`[project]-visual-identity.md`), it becomes a standalone skill you can use in any project.

### Setup for Image Generation

1. Add your completed `[project]-visual-identity.md` to a Claude project
2. You no longer need `vibe.md`, `vibe-template.md`, or `vibe-progress.md` — just the final artifact

### Trigger Commands for Generation

| Command | What it does |
|---------|--------------|
| `@[project]-visual-identity.md generate [description]` | Generate a prompt for the described image |
| `@[project]-visual-identity.md header for [topic]` | Generate a blog header prompt |
| `@[project]-visual-identity.md social for [topic]` | Generate a social media image prompt |
| `@[project]-visual-identity.md refine [feedback]` | Refine the last prompt based on your feedback |
| `@[project]-visual-identity.md batch [list of topics]` | Generate multiple prompts at once |

### Example Workflow

```
You: @mindful-blog-visual-identity.md header for "morning routines that actually stick"

Claude: Based on your visual identity, here's a prompt for Gemini:

[Generates prompt using your documented style keywords, color palette, 
composition rules, lighting preferences, and format specs for blog headers]

---

You run it in Gemini, come back with feedback...

You: @mindful-blog-visual-identity.md refine — I like it but the colors 
are too saturated and the plant in the corner feels cluttered

Claude: Here's a refined prompt:

[Adjusts using your documented refinement vocabulary, adds relevant 
negative prompts, simplifies composition per your anti-patterns]
```

### What Claude Does During Generation

When generating prompts from your visual identity, Claude will:

1. **Read your core style string** and include it as the foundation
2. **Apply format specs** for the requested use case (aspect ratio, composition notes)
3. **Include your negative prompts** to prevent common failure modes
4. **Use your tested keywords** rather than generic alternatives
5. **Match the subject treatment** to your documented preferences
6. **Provide the prompt** ready to paste into your image generator

### Iteration Loop

```
┌─────────────────────────────────────────────────────┐
│  1. Request an image for a topic/use case          │
│                    ↓                               │
│  2. Claude generates prompt from your identity doc │
│                    ↓                               │
│  3. You run prompt in Gemini/Midjourney/etc.      │
│                    ↓                               │
│  4. Describe results: "love it" or "needs X"      │
│                    ↓                               │
│  5. Claude refines using your iteration protocol  │
│                    ↓                               │
│  6. Repeat until satisfied                        │
└─────────────────────────────────────────────────────┘
```

### Vector Finalization (Hybrid Workflow)

If you explored concepts in raster and want production-ready vectors:

**Option A: Style Reference (Recommended)**
1. Take your approved raster image
2. Go to Recraft
3. Upload as "style reference"
4. Enter your original prompt + vector modifiers
5. Generate native SVG inspired by your raster

**Option B: Direct Vectorization**
1. Upload your approved raster to Recraft
2. Use the "Vectorize" function to trace it exactly
3. Best for images already designed with flat colors and clean edges

**Prompt Modification for Vector:**
```
Original (raster):
"friendly robot mascot, blue and orange, modern style"

Modified (vector):
"friendly robot mascot, blue and orange, modern style, 
flat vector illustration, minimal gradients, clean geometric shapes, icon style"
```

**Cost:** Style reference = $0.08/vector | Direct vectorization = $0.01/image

See `ai-image-generation-comparison.md` for the full hybrid workflow details.

### Updating Your Visual Identity

If you discover new patterns or want to evolve your style:

1. Note what's changed or what new keywords work
2. Update the relevant sections in your `[project]-visual-identity.md` directly
3. Or, if it's a major evolution, start a new `@vibe.md phase 2` session to test and codify changes

---

## Companion Files

- **vibe-template.md** — The blank template for your final artifact
- **vibe-progress.md** — Your working progress (created automatically)
- **ai-image-generation-comparison.md** — Generator selection guide with pricing, capabilities, and workflow recommendations (optional but recommended)

When you're ready to begin, say **"@vibe.md phase 1"** to start discovering your visual identity.

Once complete, your `[project]-visual-identity.md` file works standalone for ongoing image generation.
