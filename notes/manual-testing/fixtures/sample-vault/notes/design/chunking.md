# Chunking Markdown for semantic search

How Hypomnema slices a Markdown file into semantic chunks before
embedding.

## Why chunk at all

Whole-file embeddings smear distinct topics together. A note that
discusses both "the watcher" and "the embedding service" produces a
single vector that's the average of both ideas, which ranks poorly on a
query that's about either one alone. Splitting on heading boundaries
gives one vector per coherent section, so similarity scores reflect
which section best matches the query rather than which file does.

## Heading-aware boundaries

The chunker uses `pulldown-cmark` to walk the file as a stream of
events. H1, H2, and H3 headings start new chunks; deeper headings stay
inside their parent's chunk. Each chunk records its `heading_path` —
the list of headings from the document root down to the chunk's section
— so search results can show the operator where in the file the match
came from.

A chunk that exceeds the soft size cap (~2000 bytes) is broken at the
next paragraph boundary, never mid-paragraph. Code blocks are preserved
intact; we never split a fenced block across chunks.

## Frontmatter handling

YAML frontmatter delimited by `---` at the start of the file is
extracted before chunking and stripped from the body. The byte offsets
the chunker emits are still relative to the original file, so a
"jump-to-location" feature in a downstream consumer maps cleanly to the
on-disk content.
