//! Heading-aware Markdown chunking for Hypomnema.
//!
//! Walks a file with `pulldown-cmark`, splits it into chunks at H1/H2/H3
//! boundaries (with size-based breaks at paragraph ends as a fallback), and
//! returns the chunk text plus metadata. Pure logic; no I/O.

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use sha2::{Digest, Sha256};

/// Soft target for chunk size in bytes (~500 tokens at 4 bytes/token).
/// After a chunk crosses this length, the chunker breaks at the next
/// paragraph (or other block-level) end.
pub const CHUNK_TARGET_BYTES: usize = 2000;

/// Hard cap for chunk size in bytes (~800 tokens at 4 bytes/token).
/// Reserved for the "single very long paragraph" case; current logic
/// breaks at the next block-level end past this length, never mid-block.
pub const CHUNK_HARD_CAP_BYTES: usize = 3200;

/// One emitted chunk, ready to embed and persist.
///
/// `start_byte`/`end_byte` are offsets into the **original file**, not the
/// post-frontmatter body slice — so step 7's "jump to this location" lands
/// in the right place when the file has frontmatter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    pub chunk_index: u32,
    pub heading_path: String,
    pub content: String,
    pub content_hash: String,
    pub start_byte: usize,
    pub end_byte: usize,
}

/// Split YAML frontmatter (delimited by `---\n` ... `\n---\n`) off the top
/// of the file. Returns `(frontmatter_text, body_text)`. If no frontmatter
/// is present, returns `(None, full_content)`.
pub fn split_frontmatter(content: &str) -> (Option<&str>, &str) {
    if !content.starts_with("---\n") {
        return (None, content);
    }
    let rest = &content[4..];
    match rest.find("\n---\n") {
        Some(end) => (Some(&rest[..end]), &rest[end + 5..]),
        None => (None, content),
    }
}

/// Chunk a Markdown file. Empty bodies (including frontmatter-only files)
/// produce zero chunks.
pub fn chunk_file(file_content: &str) -> Vec<Chunk> {
    let (frontmatter, body) = split_frontmatter(file_content);
    let body_offset_in_file = match frontmatter {
        Some(fm) => 4 + fm.len() + 5, // "---\n" + fm + "\n---\n"
        None => 0,
    };
    let mut chunker = Chunker::new(body, body_offset_in_file);
    chunker.walk();
    chunker.chunks
}

struct OpenChunk {
    start: usize,
    heading_path: String,
}

/// Tracks the most recent H1, H2, H3 heading text. Slot 0 = H1, 1 = H2, 2 = H3.
///
/// **Orphan-H3 behavior** (pinned per workplan resolution C and skill § Heading
/// stack): when an H3 appears without a preceding H2, slot 1 is left as the
/// empty string. `render_path` joins the slots with `/`, so the path becomes
/// e.g. `"Setup//Prereqs"`. Empty trailing slots are trimmed; empty middle
/// slots are preserved between slashes.
struct Chunker<'a> {
    body: &'a str,
    body_offset_in_file: usize,
    heading_stack: [String; 3],
    chunks: Vec<Chunk>,
    next_index: u32,
    open_chunk: Option<OpenChunk>,
    code_block_depth: u32,
    capturing_heading: bool,
    heading_text_buf: String,
}

impl<'a> Chunker<'a> {
    fn new(body: &'a str, body_offset_in_file: usize) -> Self {
        Self {
            body,
            body_offset_in_file,
            heading_stack: [String::new(), String::new(), String::new()],
            chunks: Vec::new(),
            next_index: 0,
            open_chunk: None,
            code_block_depth: 0,
            capturing_heading: false,
            heading_text_buf: String::new(),
        }
    }

    fn walk(&mut self) {
        let parser = Parser::new_ext(self.body, Options::empty());
        for (event, range) in parser.into_offset_iter() {
            self.handle_event(event, range);
        }
        if let Some(open) = self.open_chunk.take() {
            self.emit(open, self.body.len());
        }
    }

    fn handle_event(&mut self, event: Event<'_>, range: std::ops::Range<usize>) {
        // First pass: heading-start may close the current chunk and open a new
        // one. Other events just ensure a chunk is open at this byte position.
        let mut opened_via_heading = false;
        if let Event::Start(Tag::Heading { level, .. }) = &event {
            self.capturing_heading = true;
            self.heading_text_buf.clear();
            if level_index(*level) < 3 {
                if let Some(open) = self.open_chunk.take() {
                    self.emit(open, range.start);
                }
                self.open_chunk = Some(OpenChunk {
                    start: range.start,
                    heading_path: String::new(),
                });
                opened_via_heading = true;
            }
        }
        if !opened_via_heading {
            self.ensure_open(range.start);
        }

        // Second pass: state updates and size-based break checks.
        match event {
            Event::End(TagEnd::Heading(level)) => {
                self.capturing_heading = false;
                let text = std::mem::take(&mut self.heading_text_buf);
                let idx = level_index(level);
                if idx < 3 {
                    self.heading_stack[idx] = text;
                    for slot in self.heading_stack.iter_mut().skip(idx + 1) {
                        slot.clear();
                    }
                    if let Some(open) = self.open_chunk.as_mut() {
                        open.heading_path = render_path(&self.heading_stack);
                    }
                }
            }
            Event::Start(Tag::CodeBlock(_)) => {
                self.code_block_depth += 1;
            }
            Event::End(TagEnd::CodeBlock) => {
                self.code_block_depth = self.code_block_depth.saturating_sub(1);
                self.maybe_size_break(range.end);
            }
            Event::End(TagEnd::Paragraph)
            | Event::End(TagEnd::Item)
            | Event::End(TagEnd::List(_))
            | Event::End(TagEnd::BlockQuote(_)) => {
                self.maybe_size_break(range.end);
            }
            Event::Text(ref t) | Event::Code(ref t) => {
                if self.capturing_heading {
                    self.heading_text_buf.push_str(t);
                }
            }
            _ => {}
        }
    }

    fn ensure_open(&mut self, start: usize) {
        if self.open_chunk.is_none() {
            self.open_chunk = Some(OpenChunk {
                start,
                heading_path: render_path(&self.heading_stack),
            });
        }
    }

    fn maybe_size_break(&mut self, end: usize) {
        if self.code_block_depth > 0 {
            return;
        }
        let len = self
            .open_chunk
            .as_ref()
            .map(|o| end.saturating_sub(o.start))
            .unwrap_or(0);
        if len > CHUNK_TARGET_BYTES {
            if let Some(open) = self.open_chunk.take() {
                self.emit(open, end);
            }
        }
    }

    fn emit(&mut self, open: OpenChunk, end: usize) {
        let end = end.min(self.body.len());
        if end <= open.start {
            return;
        }
        let content = self.body[open.start..end].to_string();
        if content.is_empty() {
            return;
        }
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        let content_hash = format!("sha256:{:x}", hasher.finalize());
        self.chunks.push(Chunk {
            chunk_index: self.next_index,
            heading_path: open.heading_path,
            content,
            content_hash,
            start_byte: self.body_offset_in_file + open.start,
            end_byte: self.body_offset_in_file + end,
        });
        self.next_index += 1;
    }
}

fn level_index(level: HeadingLevel) -> usize {
    match level {
        HeadingLevel::H1 => 0,
        HeadingLevel::H2 => 1,
        HeadingLevel::H3 => 2,
        HeadingLevel::H4 => 3,
        HeadingLevel::H5 => 4,
        HeadingLevel::H6 => 5,
    }
}

fn render_path(stack: &[String; 3]) -> String {
    let last_filled = stack.iter().rposition(|s| !s.is_empty());
    match last_filled {
        None => String::new(),
        Some(i) => stack[..=i].join("/"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_indices_contiguous(chunks: &[Chunk]) {
        for (i, c) in chunks.iter().enumerate() {
            assert_eq!(
                c.chunk_index, i as u32,
                "chunk {i} has chunk_index={}, expected {}",
                c.chunk_index, i
            );
        }
    }

    fn expected_hash(s: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(s.as_bytes());
        format!("sha256:{:x}", hasher.finalize())
    }

    // --- Skill § Tests to write ---

    #[test]
    fn empty_file_produces_zero_chunks() {
        let chunks = chunk_file("");
        assert!(chunks.is_empty(), "expected 0 chunks, got {chunks:?}");
    }

    #[test]
    fn frontmatter_only_no_body_produces_zero_chunks() {
        let input = "---\ntitle: Foo\ntags: [a, b]\n---\n";
        let chunks = chunk_file(input);
        assert!(chunks.is_empty(), "expected 0 chunks, got {chunks:?}");
    }

    #[test]
    fn no_headings_produces_one_chunk_for_whole_body() {
        let input =
            "Just a paragraph of prose with no headings at all.\n\nAnd a second paragraph.\n";
        let chunks = chunk_file(input);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].chunk_index, 0);
        assert_eq!(chunks[0].heading_path, "");
        assert_eq!(chunks[0].start_byte, 0);
        assert_eq!(chunks[0].content, input);
    }

    #[test]
    fn single_h1_at_top_produces_one_chunk() {
        let input = "# Title\n\nSome body text under the title.\n";
        let chunks = chunk_file(input);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].heading_path, "Title");
        assert!(chunks[0].content.starts_with("# Title"));
        assert!(chunks[0].content.contains("Some body text"));
    }

    #[test]
    fn h1_h2_h1_sequence_produces_three_chunks_with_correct_paths() {
        let input = "# Alpha\n\nFirst section.\n\n## Beta\n\nSecond section.\n\n# Gamma\n\nThird section.\n";
        let chunks = chunk_file(input);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].heading_path, "Alpha");
        assert_eq!(chunks[1].heading_path, "Alpha/Beta");
        assert_eq!(chunks[2].heading_path, "Gamma");
        assert_indices_contiguous(&chunks);
    }

    #[test]
    fn content_exceeding_max_under_one_heading_breaks_at_paragraph_boundary() {
        let mut body = String::from("# Long\n\n");
        // ~250 paragraphs of 30 bytes each ≈ 7500 bytes — well past the 2000-byte
        // target, with paragraph boundaries every 30 bytes for the chunker to
        // break at.
        for i in 0..250 {
            body.push_str(&format!("Paragraph number {i:03} with some words.\n\n"));
        }
        let chunks = chunk_file(&body);
        assert!(
            chunks.len() >= 2,
            "expected multiple chunks, got {}",
            chunks.len()
        );
        for c in &chunks {
            assert_eq!(c.heading_path, "Long");
            // Each chunk should be at most ~target + one paragraph slack;
            // the loose ceiling is the hard cap.
            assert!(
                c.content.len() <= CHUNK_HARD_CAP_BYTES + 200,
                "chunk too large: {} bytes",
                c.content.len()
            );
            // Each chunk should end at a paragraph boundary (after "\n\n" or
            // at body end).
            assert!(
                c.content.ends_with("\n\n") || c.content.ends_with('\n'),
                "chunk does not end at a clean boundary: {:?}",
                &c.content[c.content.len().saturating_sub(20)..]
            );
        }
    }

    #[test]
    fn h4_under_h2_stays_in_h2_chunk() {
        let input = "# A\n\n## B\n\nIntro under B.\n\n#### D\n\nMore under B (via D).\n";
        let chunks = chunk_file(input);
        // Expected: chunk0 under "A", chunk1 under "A/B" containing both the
        // intro paragraph and the H4 + its content. No separate chunk for H4.
        assert_eq!(chunks.len(), 2, "got {chunks:#?}");
        assert_eq!(chunks[0].heading_path, "A");
        assert_eq!(chunks[1].heading_path, "A/B");
        assert!(chunks[1].content.contains("#### D"));
        assert!(chunks[1].content.contains("More under B"));
    }

    #[test]
    fn code_blocks_spanning_many_lines_preserved_as_unit() {
        // Code block of ~3000 bytes — past the target, but should not split.
        let mut input = String::from("# Code\n\n```\n");
        for i in 0..200 {
            input.push_str(&format!("line {i:03} content content content\n"));
        }
        input.push_str("```\n");
        let chunks = chunk_file(&input);
        // Expect 1 chunk: heading + entire code block. The code block is
        // preserved as a unit.
        let code_chunks: Vec<_> = chunks
            .iter()
            .filter(|c| c.content.contains("```"))
            .collect();
        assert_eq!(
            code_chunks.len(),
            1,
            "code fences must appear in exactly one chunk; got {}: {:#?}",
            code_chunks.len(),
            code_chunks
        );
        let code_chunk = code_chunks[0];
        // Both fences live in the same chunk.
        let opens = code_chunk.content.matches("```").count();
        assert_eq!(opens, 2, "both code fences must be in the same chunk");
        // And every body line must be there too.
        assert!(code_chunk.content.contains("line 000"));
        assert!(code_chunk.content.contains("line 199"));
    }

    #[test]
    fn h3_with_no_parent_h2_uses_documented_orphan_behavior() {
        let input = "# Setup\n\nSome intro.\n\n### Prereqs\n\nDetails.\n";
        let chunks = chunk_file(input);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].heading_path, "Setup");
        assert_eq!(chunks[1].heading_path, "Setup//Prereqs");
    }

    // --- Task-specific tests ---

    #[test]
    fn chunk_index_is_zero_based_and_contiguous() {
        let input = "# A\n\nx\n\n## B\n\ny\n\n# C\n\nz\n";
        let chunks = chunk_file(input);
        assert_eq!(chunks.len(), 3);
        assert_indices_contiguous(&chunks);
        assert_eq!(chunks[0].chunk_index, 0);
        assert_eq!(chunks[1].chunk_index, 1);
        assert_eq!(chunks[2].chunk_index, 2);
    }

    #[test]
    fn byte_offsets_account_for_frontmatter() {
        let fm = "title: hi\ntags: [a]";
        let body = "# Heading\n\nbody.\n";
        let input = format!("---\n{fm}\n---\n{body}");
        let body_offset = 4 + fm.len() + 5;
        let chunks = chunk_file(&input);
        assert_eq!(chunks.len(), 1);
        let c = &chunks[0];
        assert_eq!(
            c.start_byte, body_offset,
            "start_byte should be at offset {body_offset} (after frontmatter), got {}",
            c.start_byte
        );
        assert!(c.end_byte > c.start_byte);
        // The slice from start_byte..end_byte in the original file equals the chunk content.
        assert_eq!(&input[c.start_byte..c.end_byte], c.content);
    }

    #[test]
    fn orphan_h3_uses_documented_behavior() {
        // Orphan H3 anywhere — even with no H1 above — should keep empty
        // segments between filled levels.
        let input = "### Lone\n\nbody.\n";
        let chunks = chunk_file(input);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].heading_path, "//Lone");
    }

    #[test]
    fn code_block_not_split_mid_block() {
        // Single code block alone, larger than the target threshold.
        let mut input = String::from("```\n");
        for i in 0..150 {
            input.push_str(&format!("line {i:03} aaaa bbbb cccc dddd eeee\n"));
        }
        input.push_str("```\n");
        assert!(
            input.len() > CHUNK_TARGET_BYTES,
            "test fixture must exceed CHUNK_TARGET_BYTES to be meaningful"
        );
        let chunks = chunk_file(&input);
        // Expect one chunk holding the entire fenced block.
        assert_eq!(
            chunks.len(),
            1,
            "code block was split across {} chunks",
            chunks.len()
        );
        let c = &chunks[0];
        let opens = c.content.matches("```").count();
        assert_eq!(opens, 2, "both code fences must be in the same chunk");
        assert!(c.content.contains("line 000"));
        assert!(c.content.contains("line 149"));
        // content_hash is over the chunk text, not the whole file. Here the
        // chunk text equals the input, but the formula must be SHA-256 of
        // chunk content:
        assert_eq!(c.content_hash, expected_hash(&c.content));
    }

    // --- Sanity tests over the helpers ---

    #[test]
    fn split_frontmatter_recognizes_frontmatter() {
        let (fm, body) = split_frontmatter("---\nfoo: 1\n---\nrest\n");
        assert_eq!(fm, Some("foo: 1"));
        assert_eq!(body, "rest\n");
    }

    #[test]
    fn split_frontmatter_returns_none_when_absent() {
        let (fm, body) = split_frontmatter("no fm here\n");
        assert!(fm.is_none());
        assert_eq!(body, "no fm here\n");
    }

    #[test]
    fn split_frontmatter_returns_none_when_unterminated() {
        let input = "---\nstart but no end\n";
        let (fm, body) = split_frontmatter(input);
        assert!(fm.is_none());
        assert_eq!(body, input);
    }

    #[test]
    fn render_path_handles_trailing_empties() {
        assert_eq!(
            render_path(&["A".into(), String::new(), String::new()]),
            "A"
        );
        assert_eq!(render_path(&["A".into(), "B".into(), String::new()]), "A/B");
        assert_eq!(render_path(&["A".into(), "B".into(), "C".into()]), "A/B/C");
        assert_eq!(
            render_path(&["A".into(), String::new(), "C".into()]),
            "A//C"
        );
        assert_eq!(
            render_path(&[String::new(), String::new(), "C".into()]),
            "//C"
        );
        assert_eq!(
            render_path(&[String::new(), String::new(), String::new()]),
            ""
        );
    }
}
