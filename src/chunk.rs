//! Heading-aware Markdown chunking for Hypomnema.
//!
//! Walks a file with `pulldown-cmark`, splits it into chunks at H1/H2/H3
//! boundaries (with size-based breaks at paragraph ends as a fallback), and
//! returns the chunk text plus metadata. Pure logic; no I/O.

use std::fmt::Write as _;

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
    pub boundary_start: String,
    pub boundary_end: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkDiagnostics {
    pub fenced_code_blocks: usize,
    pub fenced_code_bytes: usize,
    pub fenced_code_languages: Vec<String>,
    pub code_heavy: bool,
    pub thematic_breaks: usize,
}

/// Encodes a byte slice as a lowercase hex string.
fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        write!(s, "{b:02x}").expect("writing to String is infallible");
    }
    s
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
    boundary_start: String,
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
    pending_boundary_start: Option<String>,
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
            pending_boundary_start: None,
        }
    }

    fn walk(&mut self) {
        let parser = Parser::new_ext(self.body, Options::empty());
        for (event, range) in parser.into_offset_iter() {
            self.handle_event(event, range);
        }
        if let Some(open) = self.open_chunk.take() {
            self.emit(open, self.body.len(), "document_end");
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
                    self.emit(
                        open,
                        range.start,
                        &format!("heading:{}", heading_label(*level)),
                    );
                }
                // A heading always opens its chunk with a heading boundary. A
                // pending thematic break (e.g. `---` immediately before this
                // heading) is already recorded as the previous chunk's
                // boundary_end, so clear it rather than mislabeling this chunk.
                self.pending_boundary_start = None;
                self.open_chunk = Some(OpenChunk {
                    start: range.start,
                    heading_path: String::new(),
                    boundary_start: format!("heading:{}", heading_label(*level)),
                });
                opened_via_heading = true;
            }
        }
        if matches!(event, Event::Rule) {
            if let Some(open) = self.open_chunk.take() {
                self.emit(open, range.start, "thematic_break");
            }
            self.pending_boundary_start = Some("thematic_break".to_string());
            return;
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
                self.maybe_size_break(range.end, "size_after_code_block");
            }
            Event::End(TagEnd::Paragraph)
            | Event::End(TagEnd::Item)
            | Event::End(TagEnd::List(_))
            | Event::End(TagEnd::BlockQuote(_)) => {
                self.maybe_size_break(range.end, "size_after_block");
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
                boundary_start: self
                    .pending_boundary_start
                    .take()
                    .unwrap_or_else(|| "document_start".to_string()),
            });
        }
    }

    fn maybe_size_break(&mut self, end: usize, reason: &str) {
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
                self.emit(open, end, reason);
            }
        }
    }

    fn emit(&mut self, open: OpenChunk, end: usize, boundary_end: &str) {
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
        let content_hash = format!("sha256:{}", hex_encode(&hasher.finalize()));
        self.chunks.push(Chunk {
            chunk_index: self.next_index,
            heading_path: open.heading_path,
            content,
            content_hash,
            start_byte: self.body_offset_in_file + open.start,
            end_byte: self.body_offset_in_file + end,
            boundary_start: open.boundary_start,
            boundary_end: boundary_end.to_string(),
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

fn heading_label(level: HeadingLevel) -> &'static str {
    match level {
        HeadingLevel::H1 => "h1",
        HeadingLevel::H2 => "h2",
        HeadingLevel::H3 => "h3",
        HeadingLevel::H4 => "h4",
        HeadingLevel::H5 => "h5",
        HeadingLevel::H6 => "h6",
    }
}

fn render_path(stack: &[String; 3]) -> String {
    let last_filled = stack.iter().rposition(|s| !s.is_empty());
    match last_filled {
        None => String::new(),
        Some(i) => stack[..=i].join("/"),
    }
}

pub fn diagnose_chunk(content: &str) -> ChunkDiagnostics {
    let mut fenced_code_blocks = 0usize;
    let mut fenced_code_bytes = 0usize;
    let mut fenced_code_languages: Vec<String> = Vec::new();
    let mut thematic_breaks = 0usize;
    let mut in_fence: Option<(String, usize)> = None;

    let mut offset = 0usize;
    for line in content.split_inclusive('\n') {
        let line_start = offset;
        let line_end = line_start + line.len();
        // Fences and thematic breaks accept at most 3 leading spaces; 4+ spaces
        // (or a leading tab) make the line indented code, which must not be read
        // as a block marker. `None` => skip marker classification for this line.
        let marker = block_marker_candidate(line);

        if let Some((fence, start)) = &in_fence {
            if marker.is_some_and(|t| is_closing_fence(t, fence)) {
                fenced_code_blocks += 1;
                fenced_code_bytes += line_end.saturating_sub(*start);
                in_fence = None;
            }
        } else if let Some(trimmed) = marker {
            if let Some((fence, lang)) = opening_fence(trimmed) {
                if !lang.is_empty() && !fenced_code_languages.iter().any(|l| l == &lang) {
                    fenced_code_languages.push(lang);
                }
                in_fence = Some((fence, line_start));
            } else if is_thematic_break_line(trimmed) {
                thematic_breaks += 1;
            }
        }

        offset = line_end;
    }

    if let Some((_marker, start)) = in_fence {
        fenced_code_blocks += 1;
        fenced_code_bytes += content.len().saturating_sub(start);
    }

    ChunkDiagnostics {
        fenced_code_blocks,
        fenced_code_bytes,
        fenced_code_languages,
        code_heavy: !content.is_empty() && fenced_code_bytes * 2 > content.len(),
        thematic_breaks,
    }
}

/// Returns the trimmed line for block-marker matching only when its leading
/// indentation is ≤3 spaces. A leading tab or 4+ spaces denotes indented code
/// (CommonMark), so such lines never count as fences or thematic breaks.
fn block_marker_candidate(line: &str) -> Option<&str> {
    let mut spaces = 0usize;
    for ch in line.chars() {
        match ch {
            ' ' => spaces += 1,
            '\t' => return None,
            _ => break,
        }
        if spaces > 3 {
            return None;
        }
    }
    Some(line.trim())
}

fn opening_fence(trimmed: &str) -> Option<(String, String)> {
    let marker = if trimmed.starts_with("```") {
        "`"
    } else if trimmed.starts_with("~~~") {
        "~"
    } else {
        return None;
    };
    let marker_ch = marker.chars().next().expect("marker is non-empty");
    let count = trimmed.chars().take_while(|c| *c == marker_ch).count();
    if count < 3 {
        return None;
    }
    let lang = trimmed[count..]
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_string();
    Some((marker.repeat(count), lang))
}

fn is_closing_fence(trimmed: &str, marker: &str) -> bool {
    let Some(ch) = marker.chars().next() else {
        return false;
    };
    trimmed.len() >= marker.len() && trimmed.chars().all(|c| c == ch)
}

fn is_thematic_break_line(trimmed: &str) -> bool {
    if trimmed.len() < 3 {
        return false;
    }
    let compact: String = trimmed.chars().filter(|c| !c.is_whitespace()).collect();
    compact.len() >= 3
        && (compact.chars().all(|c| c == '-')
            || compact.chars().all(|c| c == '*')
            || compact.chars().all(|c| c == '_'))
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
        let bytes = hasher.finalize();
        format!("sha256:{}", hex_encode(&bytes))
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
    fn thematic_break_splits_sections_under_same_heading() {
        let input = "# A\n\n## B\n\nFirst direction.\n\n---\n\nDifferent direction.\n";
        let chunks = chunk_file(input);
        assert_eq!(chunks.len(), 3, "got {chunks:#?}");
        assert_eq!(chunks[0].heading_path, "A");
        assert_eq!(chunks[1].heading_path, "A/B");
        assert_eq!(chunks[2].heading_path, "A/B");
        assert_eq!(chunks[1].boundary_end, "thematic_break");
        assert_eq!(chunks[2].boundary_start, "thematic_break");
        assert!(chunks[1].content.contains("First direction"));
        assert!(chunks[2].content.contains("Different direction"));
        assert!(!chunks[1].content.contains("---"));
        assert!(!chunks[2].content.contains("---"));
        assert_indices_contiguous(&chunks);
    }

    #[test]
    fn thematic_break_immediately_before_heading_keeps_heading_boundary() {
        // `---` directly before a heading closes the prior chunk (boundary_end
        // = thematic_break) but the heading-opened chunk must report a heading
        // boundary_start, not inherit the pending thematic break.
        let input = "# Doc\n\n## A\n\nFirst.\n\n---\n\n## B\n\nSecond.\n";
        let chunks = chunk_file(input);
        let a = chunks
            .iter()
            .find(|c| c.content.contains("First"))
            .expect("chunk with First");
        let b = chunks
            .iter()
            .find(|c| c.content.contains("Second"))
            .expect("chunk with Second");
        assert_eq!(a.boundary_end, "thematic_break");
        assert_eq!(b.boundary_start, "heading:h2");
    }

    #[test]
    fn frontmatter_delimiter_is_not_body_thematic_break() {
        let input = "---\ntitle: A\n---\n# A\n\nBody.\n";
        let chunks = chunk_file(input);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].heading_path, "A");
        assert_eq!(chunks[0].boundary_start, "heading:h1");
        assert!(!chunks[0].content.contains("title: A"));
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

    #[test]
    fn fenced_code_is_kept_and_classified() {
        let input = "# A\n\n```toml\n[tool]\nname = \"hmn\"\n```\n\nProse.\n";
        let chunks = chunk_file(input);
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].content.contains("```toml"));
        let diagnostics = diagnose_chunk(&chunks[0].content);
        assert_eq!(diagnostics.fenced_code_blocks, 1);
        assert!(diagnostics.fenced_code_bytes > 0);
        assert_eq!(diagnostics.fenced_code_languages, vec!["toml"]);
    }

    #[test]
    fn diagnose_ignores_indented_code_block_markers() {
        // 4-space indented lines are indented code in CommonMark, not block
        // markers; they must not inflate thematic_breaks or fenced_code_blocks.
        let content = "Prose paragraph.\n\n    ---\n\n    ```rust\n    let x = 1;\n    ```\n";
        let d = diagnose_chunk(content);
        assert_eq!(d.thematic_breaks, 0, "indented --- is code, not a break");
        assert_eq!(d.fenced_code_blocks, 0, "indented ``` is code, not a fence");

        // A non-indented thematic break (≤3 leading spaces) is still counted.
        assert_eq!(diagnose_chunk("a\n\n---\n\nb\n").thematic_breaks, 1);
        // A fence indented by ≤3 spaces is still recognized.
        assert_eq!(
            diagnose_chunk("   ```rust\n   code\n   ```\n").fenced_code_blocks,
            1
        );
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
