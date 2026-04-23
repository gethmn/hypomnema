---
name: markdown-chunking
description: Use when implementing, modifying, or debugging the Markdown chunking logic in Hypomnema. Covers the pulldown-cmark event-driven approach, heading-aware chunk boundaries, frontmatter extraction, size caps, and the metadata each chunk should carry. Apply whenever code touches the chunk module, chunk boundaries, or how a file becomes searchable pieces.
---

# Chunking Markdown for Hypomnema

Chunking is a small module with outsize impact: bad chunks mean bad semantic search, even with a perfect embedding model. The v0 strategy is heading-aware with a size cap — each chunk is a contiguous piece of content under one heading path, up to a maximum size, with a clean break at a paragraph boundary when the cap is reached.

## Why pulldown-cmark

It's a streaming parser that emits events (`Event::Start(Tag::Heading)`, `Event::Text`, etc.) rather than building a full AST. That shape is exactly right for "walk the document and identify boundaries as I go."

## Frontmatter first

Split YAML frontmatter off the top before parsing. pulldown-cmark doesn't handle it natively and will read the `---` as a thematic break if you don't.

```rust
fn split_frontmatter(content: &str) -> (Option<&str>, &str) {
    if !content.starts_with("---\n") {
        return (None, content);
    }
    let rest = &content[4..];
    match rest.find("\n---\n") {
        Some(end) => (Some(&rest[..end]), &rest[end + 5..]),
        None => (None, content),
    }
}
```

Parse frontmatter separately — extract fields (tags, title) as file-level metadata. The body goes into the chunker.

## Chunk boundaries

A new chunk starts at:

1. The start of the body (after frontmatter).
2. Any heading at level 1, 2, or 3. Level 4+ headings stay within the parent chunk.
3. A size threshold (current chunk reached the target size) — break at the next paragraph boundary, not mid-paragraph.

## Chunk metadata

Each chunk carries:

- `file_path` — relative path from the vault root
- `heading_path` — slash-separated heading breadcrumb, e.g. `"Architecture/Load-bearing rules"`
- `start_byte`, `end_byte` — offsets into the original file, useful later for "jump to this location"
- `content` — the chunk text
- `content_hash` — SHA-256 of the chunk text (not the whole file)

Per-chunk hashes let us detect which specific chunks changed between re-scans. For v0 we re-embed the whole file on change anyway, but the hashes are free to compute and worth having.

## Size targets

Target ~500 tokens per chunk, hard cap at ~800. For v0, approximate tokens as `bytes / 4` — good enough for chunking decisions, not used for anything that requires real tokenization. If we later need real token counts, add a tokenizer then.

## Heading stack

Track the heading hierarchy as you go, so the `heading_path` for each chunk reflects the full breadcrumb:

- H1 → stack = `["Intro"]`, heading_path = `"Intro"`
- H2 under that → stack = `["Intro", "Context"]`, heading_path = `"Intro/Context"`
- New H1 → stack = `["Setup"]`, heading_path = `"Setup"`
- H3 under that → stack = `["Setup", "", "Prereqs"]` — fill gaps with empty strings or skip levels; decide once and be consistent

The edge case that bites people: an H3 appearing without a parent H2. Decide your behavior (probably: pretend it's under the last-seen H2, or under the H1 if no H2 exists) and document it in a comment.

## Tests to write

These are the cases that catch real bugs:

- Empty file → zero chunks.
- Frontmatter only, no body → zero chunks.
- No headings → one chunk for the whole body.
- Single H1 at the top → one chunk (the heading is part of its own chunk, not a separator before content).
- H1 / H2 / H1 sequence → three chunks with correct heading paths.
- Content exceeding max size under one heading → multiple chunks, all with the same heading_path, breaking at paragraph boundaries.
- H4 under H2 → H4 stays inside the H2's chunk, no new chunk created.
- Code blocks spanning many lines → preserved as a unit, not split mid-block.
- H3 with no parent H2 → whatever behavior you chose, tested.

## Smells

- Regex-based chunking: you will regret this the first time someone uses an unusual heading style or embeds code that looks like a heading.
- Splitting on blank lines: correlates with paragraphs but isn't the same thing.
- Storing the whole file as one chunk: defeats semantic search's ability to return specific sections.
- Losing byte offsets: needed later for "jump to this location" features, and cheap to track.
- Hashing whole files instead of per-chunk: means any change re-indexes the whole file, which is fine for v0 but a pointless cost if chunk hashes are free.
- Chunking inside `spawn_blocking`: chunking is pure CPU over strings, runs fine on the async runtime. Reserve `spawn_blocking` for SQL and file I/O.
