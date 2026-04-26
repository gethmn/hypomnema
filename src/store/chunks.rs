//! Chunk + vector storage for the embedding pipeline.
//!
//! `rewrite_chunks_for_file` is the load-bearing write: it deletes any
//! existing `chunks` and matching `chunks_vec` rows for `file_path` and
//! inserts the supplied chunks-with-embeddings in order. The caller wraps
//! this in a SQL transaction that also updates the `files` row for
//! `file_path`, so a crash mid-write leaves either the entire prior state
//! or the entire new state — never a half-written file.
//!
//! Vector blob layout follows `.claude/skills/sqlite-vec-extension`
//! (lines 78–88 for delete-then-insert, line 72 for the `bytemuck::cast_slice`
//! convention).

use anyhow::{Context, Result};
use rusqlite::{Transaction, params};

use crate::chunk::Chunk;

pub fn rewrite_chunks_for_file(
    tx: &Transaction<'_>,
    file_path: &str,
    chunks_with_embeddings: &[(Chunk, Vec<f32>)],
    now_iso: &str,
) -> Result<()> {
    tx.execute(
        "DELETE FROM chunks_vec WHERE chunk_id IN (SELECT id FROM chunks WHERE file_path = ?1)",
        params![file_path],
    )
    .with_context(|| format!("deleting chunks_vec rows for {file_path}"))?;
    tx.execute(
        "DELETE FROM chunks WHERE file_path = ?1",
        params![file_path],
    )
    .with_context(|| format!("deleting chunks rows for {file_path}"))?;

    for (chunk, embedding) in chunks_with_embeddings {
        tx.execute(
            "INSERT INTO chunks (file_path, chunk_index, heading_path, content, content_hash, start_byte, end_byte, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                file_path,
                chunk.chunk_index,
                chunk.heading_path,
                chunk.content,
                chunk.content_hash,
                chunk.start_byte as i64,
                chunk.end_byte as i64,
                now_iso,
            ],
        )
        .with_context(|| format!("inserting chunk {} for {file_path}", chunk.chunk_index))?;
        let chunk_id = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO chunks_vec (chunk_id, embedding) VALUES (?1, ?2)",
            params![chunk_id, bytemuck::cast_slice::<f32, u8>(embedding)],
        )
        .with_context(|| format!("inserting chunks_vec for chunk {chunk_id} of {file_path}"))?;
    }

    Ok(())
}
