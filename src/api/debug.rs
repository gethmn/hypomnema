use std::collections::HashMap;
use std::path::Component;

use anyhow::{Context, Result};
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use rusqlite::{OptionalExtension, params};
use tokio::task;

use super::ApiState;
use super::error::{ApiError, ApiJson};
use super::types::{
    DebugChunkDiagnosticsJson, DebugChunkJson, DebugChunkerInfo, DebugChunksDiff,
    DebugChunksRequest, DebugChunksResponse, DebugChunksSummary,
};
use crate::api::VaultEntry;
use crate::chunk::{self, CHUNK_HARD_CAP_BYTES, CHUNK_TARGET_BYTES, Chunk};
use crate::control_plane::VaultManager;
use crate::store::SqlitePool;
use crate::vault_registry::VaultStatus;

const PREVIEW_BYTES: usize = 600;

enum DebugMode {
    Indexed,
    Preview,
    Diff,
}

enum ShowText {
    Preview,
    Full,
    None,
}

struct IndexedFileChunks {
    content_hash: String,
    indexed_at: String,
    content: String,
    chunks: Vec<StoredChunk>,
}

struct StoredChunk {
    chunk_index: u32,
    heading_path: String,
    content: String,
    content_hash: String,
    start_byte: usize,
    end_byte: usize,
}

pub(crate) async fn chunks(
    State(s): State<ApiState>,
    ApiJson(req): ApiJson<DebugChunksRequest>,
) -> Result<Json<DebugChunksResponse>, ApiError> {
    run_debug_chunks(&s.vault_manager, &req).await.map(Json)
}

pub(crate) async fn run_debug_chunks(
    manager: &VaultManager,
    req: &DebugChunksRequest,
) -> Result<DebugChunksResponse, ApiError> {
    validate_path(&req.path)?;
    let mode = parse_mode(req.mode.as_deref())?;
    let show_text = parse_show_text(req.show_text.as_deref())?;
    let (entry, indexed) = resolve_indexed_file(manager, req).await?;

    let preview_chunks = chunk::chunk_file(&indexed.content);
    let indexed_json = indexed_chunks_to_json(&indexed.chunks, &preview_chunks, &show_text);
    let preview_json = match mode {
        DebugMode::Indexed => None,
        DebugMode::Preview | DebugMode::Diff => Some(
            preview_chunks
                .iter()
                .map(|chunk| chunk_to_json(chunk, &show_text))
                .collect(),
        ),
    };
    let diff = match mode {
        DebugMode::Diff => Some(build_diff(&indexed.chunks, &preview_chunks)),
        DebugMode::Indexed | DebugMode::Preview => None,
    };
    let summary = summarize(&indexed_json);

    Ok(DebugChunksResponse {
        vault: entry.id.to_string(),
        vault_name: entry.name.clone(),
        path: req.path.clone(),
        content_hash: indexed.content_hash,
        indexed_at: indexed.indexed_at,
        chunker: chunker_info(),
        indexed: indexed_json,
        preview: preview_json,
        diff,
        summary,
    })
}

fn parse_mode(raw: Option<&str>) -> Result<DebugMode, ApiError> {
    match raw {
        None | Some("indexed") => Ok(DebugMode::Indexed),
        Some("preview") => Ok(DebugMode::Preview),
        Some("diff") => Ok(DebugMode::Diff),
        Some(_) => Err(ApiError::invalid_request(
            "mode must be one of indexed, preview, diff",
        )),
    }
}

fn parse_show_text(raw: Option<&str>) -> Result<ShowText, ApiError> {
    match raw {
        None | Some("preview") => Ok(ShowText::Preview),
        Some("full") => Ok(ShowText::Full),
        Some("none") => Ok(ShowText::None),
        Some(_) => Err(ApiError::invalid_request(
            "show_text must be one of preview, full, none",
        )),
    }
}

async fn resolve_indexed_file(
    manager: &VaultManager,
    req: &DebugChunksRequest,
) -> Result<(std::sync::Arc<VaultEntry>, IndexedFileChunks), ApiError> {
    let scope = manager.search_scope().await?;
    let active_entries: Vec<_> = scope
        .into_iter()
        .filter_map(|row| match (row.status, row.entry) {
            (VaultStatus::Active, Some(entry)) => Some(entry),
            _ => None,
        })
        .collect();

    let candidates: Vec<_> = if let Some(vault) = &req.vault {
        active_entries
            .into_iter()
            .filter(|entry| entry.name == *vault || entry.id.to_string() == *vault)
            .collect()
    } else {
        active_entries
    };

    if candidates.is_empty() {
        return match req.vault.as_deref() {
            Some(vault) => Err(ApiError::vault_not_found(vault, None)),
            None => Err(ApiError::new(
                "vault_not_found",
                "no active vaults are available",
                StatusCode::NOT_FOUND,
            )),
        };
    }

    let mut found = Vec::new();
    for entry in candidates {
        match load_indexed_chunks(entry.store.pool(), req.path.clone()).await {
            Ok(Some(indexed)) => found.push((entry, indexed)),
            Ok(None) => {}
            Err(err) => {
                tracing::error!(error = ?err, path = %req.path, "debug chunks lookup failed");
                return Err(ApiError::new(
                    "debug_chunks_failed",
                    "failed to read indexed chunks",
                    StatusCode::INTERNAL_SERVER_ERROR,
                ));
            }
        }
    }

    match found.len() {
        0 => Err(ApiError::new(
            "path_not_found",
            "path not found in active vault index",
            StatusCode::NOT_FOUND,
        )),
        1 => Ok(found.remove(0)),
        _ => Err(ApiError::invalid_request(
            "path exists in multiple active vaults; pass --vault",
        )),
    }
}

async fn load_indexed_chunks(pool: SqlitePool, path: String) -> Result<Option<IndexedFileChunks>> {
    task::spawn_blocking(move || {
        let conn = pool
            .get()
            .context("acquiring connection for debug chunks")?;
        let file = conn
            .query_row(
                "SELECT content_hash, indexed_at, content FROM files WHERE path = ?1",
                params![path],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()
            .context("querying file for debug chunks")?;
        let Some((content_hash, indexed_at, content)) = file else {
            return Ok(None);
        };
        let mut stmt = conn
            .prepare(
                "SELECT chunk_index, heading_path, content, content_hash, start_byte, end_byte
                 FROM chunks
                 WHERE file_path = ?1
                 ORDER BY chunk_index ASC",
            )
            .context("preparing chunks query for debug chunks")?;
        let chunks = stmt
            .query_map(params![path], |row| {
                Ok(StoredChunk {
                    chunk_index: row.get::<_, i64>(0)? as u32,
                    heading_path: row.get(1)?,
                    content: row.get(2)?,
                    content_hash: row.get(3)?,
                    start_byte: row.get::<_, i64>(4)? as usize,
                    end_byte: row.get::<_, i64>(5)? as usize,
                })
            })
            .context("querying chunks for debug chunks")?
            .collect::<Result<Vec<_>, _>>()
            .context("collecting chunks for debug chunks")?;
        Ok(Some(IndexedFileChunks {
            content_hash,
            indexed_at,
            content,
            chunks,
        }))
    })
    .await
    .context("spawn_blocking join error in debug chunks")?
}

fn indexed_chunks_to_json(
    chunks: &[StoredChunk],
    preview_chunks: &[Chunk],
    show_text: &ShowText,
) -> Vec<DebugChunkJson> {
    // Index preview chunks by (chunk_index, content_hash) so each stored chunk
    // matches in O(1) instead of a linear scan with full-content comparisons.
    // content_hash equality stands in for content equality (both are sha256).
    let preview_by_key: HashMap<(u32, &str), &Chunk> = preview_chunks
        .iter()
        .map(|chunk| ((chunk.chunk_index, chunk.content_hash.as_str()), chunk))
        .collect();
    chunks
        .iter()
        .map(|stored| {
            let matching_preview = preview_by_key
                .get(&(stored.chunk_index, stored.content_hash.as_str()))
                .copied();
            stored_to_json(stored, matching_preview, show_text)
        })
        .collect()
}

fn stored_to_json(
    stored: &StoredChunk,
    matching_preview: Option<&Chunk>,
    show_text: &ShowText,
) -> DebugChunkJson {
    let diagnostics = chunk::diagnose_chunk(&stored.content);
    let warnings = warnings_for(&diagnostics);
    let (text, text_kind, text_truncated) = text_payload(&stored.content, show_text);
    DebugChunkJson {
        chunk_index: stored.chunk_index,
        start_byte: stored.start_byte,
        end_byte: stored.end_byte,
        byte_len: stored.content.len(),
        heading_path: split_heading_path(&stored.heading_path),
        boundary_start: matching_preview
            .map(|chunk| chunk.boundary_start.clone())
            .unwrap_or_else(|| "indexed_unknown".to_string()),
        boundary_end: matching_preview
            .map(|chunk| chunk.boundary_end.clone())
            .unwrap_or_else(|| "indexed_unknown".to_string()),
        content_hash: stored.content_hash.clone(),
        text,
        text_kind,
        text_truncated,
        diagnostics: diagnostics_to_json(diagnostics),
        warnings,
    }
}

fn chunk_to_json(chunk: &Chunk, show_text: &ShowText) -> DebugChunkJson {
    let diagnostics = chunk::diagnose_chunk(&chunk.content);
    let warnings = warnings_for(&diagnostics);
    let (text, text_kind, text_truncated) = text_payload(&chunk.content, show_text);
    DebugChunkJson {
        chunk_index: chunk.chunk_index,
        start_byte: chunk.start_byte,
        end_byte: chunk.end_byte,
        byte_len: chunk.content.len(),
        heading_path: split_heading_path(&chunk.heading_path),
        boundary_start: chunk.boundary_start.clone(),
        boundary_end: chunk.boundary_end.clone(),
        content_hash: chunk.content_hash.clone(),
        text,
        text_kind,
        text_truncated,
        diagnostics: diagnostics_to_json(diagnostics),
        warnings,
    }
}

fn diagnostics_to_json(diagnostics: chunk::ChunkDiagnostics) -> DebugChunkDiagnosticsJson {
    DebugChunkDiagnosticsJson {
        fenced_code_blocks: diagnostics.fenced_code_blocks,
        fenced_code_bytes: diagnostics.fenced_code_bytes,
        fenced_code_languages: diagnostics.fenced_code_languages,
        code_heavy: diagnostics.code_heavy,
        thematic_breaks: diagnostics.thematic_breaks,
    }
}

fn warnings_for(diagnostics: &chunk::ChunkDiagnostics) -> Vec<String> {
    let mut warnings = Vec::new();
    if diagnostics.code_heavy {
        warnings.push("code-heavy chunk".to_string());
    }
    if diagnostics.thematic_breaks > 0 {
        warnings.push("contains thematic break markup from an older index".to_string());
    }
    warnings
}

fn text_payload(
    content: &str,
    show_text: &ShowText,
) -> (Option<String>, Option<String>, Option<bool>) {
    match show_text {
        ShowText::None => (None, None, None),
        ShowText::Full => (
            Some(content.to_string()),
            Some("full".to_string()),
            Some(false),
        ),
        ShowText::Preview => {
            let (preview, truncated) = make_preview(content, PREVIEW_BYTES);
            (Some(preview), Some("preview".to_string()), Some(truncated))
        }
    }
}

fn make_preview(s: &str, max_bytes: usize) -> (String, bool) {
    if s.len() <= max_bytes {
        return (s.to_string(), false);
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    (s[..end].to_string(), true)
}

fn split_heading_path(path: &str) -> Vec<String> {
    path.split('/').map(String::from).collect()
}

fn build_diff(indexed: &[StoredChunk], preview: &[Chunk]) -> DebugChunksDiff {
    let mut changed_chunks = Vec::new();
    let max_shared = indexed.len().min(preview.len());
    for idx in 0..max_shared {
        let stored = &indexed[idx];
        let chunk = &preview[idx];
        if stored.chunk_index != chunk.chunk_index
            || stored.content_hash != chunk.content_hash
            || stored.start_byte != chunk.start_byte
            || stored.end_byte != chunk.end_byte
        {
            // Report the chunk's index, not the loop position, to stay
            // consistent with added_preview_chunks / removed_indexed_chunks.
            changed_chunks.push(stored.chunk_index);
        }
    }
    DebugChunksDiff {
        chunk_count_changed: indexed.len() != preview.len(),
        indexed_chunk_count: indexed.len(),
        preview_chunk_count: preview.len(),
        changed_chunks,
        added_preview_chunks: preview
            .iter()
            .skip(indexed.len())
            .map(|chunk| chunk.chunk_index)
            .collect(),
        removed_indexed_chunks: indexed
            .iter()
            .skip(preview.len())
            .map(|chunk| chunk.chunk_index)
            .collect(),
    }
}

fn summarize(chunks: &[DebugChunkJson]) -> DebugChunksSummary {
    let chunk_count = chunks.len();
    let min_bytes = chunks.iter().map(|chunk| chunk.byte_len).min().unwrap_or(0);
    let max_bytes = chunks.iter().map(|chunk| chunk.byte_len).max().unwrap_or(0);
    let total: usize = chunks.iter().map(|chunk| chunk.byte_len).sum();
    let avg_bytes = if chunk_count == 0 {
        0.0
    } else {
        total as f64 / chunk_count as f64
    };
    DebugChunksSummary {
        chunk_count,
        min_bytes,
        max_bytes,
        avg_bytes,
        code_heavy_chunks: chunks
            .iter()
            .filter(|chunk| chunk.diagnostics.code_heavy)
            .count(),
        // Each internal thematic break appears as one chunk's boundary_end and
        // the next chunk's boundary_start; count boundary_end to avoid
        // double-counting, plus the leading-break case where the first chunk
        // starts at a break with no preceding chunk to close.
        thematic_break_boundaries: {
            let ended_at_break = chunks
                .iter()
                .filter(|chunk| chunk.boundary_end == "thematic_break")
                .count();
            let leading_break = chunks
                .first()
                .is_some_and(|chunk| chunk.boundary_start == "thematic_break");
            ended_at_break + usize::from(leading_break)
        },
    }
}

fn chunker_info() -> DebugChunkerInfo {
    DebugChunkerInfo {
        version: "markdown-heading-v1".to_string(),
        rules: vec![
            "split on H1/H2/H3 headings".to_string(),
            "split on Markdown thematic breaks".to_string(),
            "split after block boundaries once target bytes is exceeded".to_string(),
            "keep fenced code blocks intact and classify them".to_string(),
        ],
        target_bytes: CHUNK_TARGET_BYTES,
        hard_cap_bytes: CHUNK_HARD_CAP_BYTES,
    }
}

fn validate_path(path: &str) -> Result<(), ApiError> {
    if path.is_empty() {
        return Err(ApiError::new(
            "invalid_path",
            "path must not be empty",
            StatusCode::UNPROCESSABLE_ENTITY,
        ));
    }
    let p = std::path::Path::new(path);
    if p.is_absolute() {
        return Err(ApiError::new(
            "invalid_path",
            "path must be vault-relative",
            StatusCode::UNPROCESSABLE_ENTITY,
        ));
    }
    if p.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(ApiError::new(
            "invalid_path",
            "path must not contain .. segments",
            StatusCode::UNPROCESSABLE_ENTITY,
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn debug_chunk(chunk_index: u32, boundary_start: &str, boundary_end: &str) -> DebugChunkJson {
        DebugChunkJson {
            chunk_index,
            start_byte: 0,
            end_byte: 1,
            byte_len: 1,
            heading_path: Vec::new(),
            boundary_start: boundary_start.to_string(),
            boundary_end: boundary_end.to_string(),
            content_hash: "sha256:00".to_string(),
            text: None,
            text_kind: None,
            text_truncated: None,
            diagnostics: DebugChunkDiagnosticsJson {
                fenced_code_blocks: 0,
                fenced_code_bytes: 0,
                fenced_code_languages: Vec::new(),
                code_heavy: false,
                thematic_breaks: 0,
            },
            warnings: Vec::new(),
        }
    }

    #[test]
    fn thematic_break_boundaries_counts_each_internal_break_once() {
        // One internal break: chunk 0 ends at it, chunk 1 starts at it.
        let chunks = vec![
            debug_chunk(0, "document_start", "thematic_break"),
            debug_chunk(1, "thematic_break", "document_end"),
        ];
        assert_eq!(summarize(&chunks).thematic_break_boundaries, 1);
    }

    #[test]
    fn thematic_break_boundaries_counts_leading_break() {
        // Leading break: first chunk starts at a break with nothing to close.
        let chunks = vec![debug_chunk(0, "thematic_break", "document_end")];
        assert_eq!(summarize(&chunks).thematic_break_boundaries, 1);
    }

    #[test]
    fn changed_chunks_reports_chunk_index_not_position() {
        // A single stored/preview chunk at position 0 whose chunk_index is 5
        // and whose content_hash differs must report 5, not 0.
        let indexed = vec![StoredChunk {
            chunk_index: 5,
            heading_path: String::new(),
            content: "a".to_string(),
            content_hash: "sha256:a".to_string(),
            start_byte: 0,
            end_byte: 1,
        }];
        let preview = vec![Chunk {
            chunk_index: 5,
            heading_path: String::new(),
            content: "b".to_string(),
            content_hash: "sha256:b".to_string(),
            start_byte: 0,
            end_byte: 1,
            boundary_start: "document_start".to_string(),
            boundary_end: "document_end".to_string(),
        }];
        assert_eq!(build_diff(&indexed, &preview).changed_chunks, vec![5]);
    }

    #[test]
    fn indexed_chunks_match_preview_by_hash_for_boundaries() {
        let stored = vec![
            StoredChunk {
                chunk_index: 0,
                heading_path: String::new(),
                content: "a".to_string(),
                content_hash: "sha256:a".to_string(),
                start_byte: 0,
                end_byte: 1,
            },
            StoredChunk {
                chunk_index: 1,
                heading_path: String::new(),
                content: "b".to_string(),
                content_hash: "sha256:b".to_string(),
                start_byte: 1,
                end_byte: 2,
            },
        ];
        let preview = vec![
            Chunk {
                chunk_index: 0,
                heading_path: String::new(),
                content: "a".to_string(),
                content_hash: "sha256:a".to_string(),
                start_byte: 0,
                end_byte: 1,
                boundary_start: "document_start".to_string(),
                boundary_end: "heading:h2".to_string(),
            },
            // Same index but diverged content/hash → must not match.
            Chunk {
                chunk_index: 1,
                heading_path: String::new(),
                content: "B!".to_string(),
                content_hash: "sha256:diverged".to_string(),
                start_byte: 1,
                end_byte: 3,
                boundary_start: "heading:h2".to_string(),
                boundary_end: "document_end".to_string(),
            },
        ];
        let out = indexed_chunks_to_json(&stored, &preview, &ShowText::None);
        // Exact (index, hash) match pulls boundary metadata from the preview.
        assert_eq!(out[0].boundary_start, "document_start");
        assert_eq!(out[0].boundary_end, "heading:h2");
        // Diverged chunk has no exact match → marked indexed_unknown.
        assert_eq!(out[1].boundary_start, "indexed_unknown");
        assert_eq!(out[1].boundary_end, "indexed_unknown");
    }
}
