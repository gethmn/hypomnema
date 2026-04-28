use axum::{Json, extract::State};

use super::ApiState;
use super::error::{ApiError, ApiJson};
use super::types::{
    ContentMatchJson, ContentQueryJson, ContentResultJson, ContentSearchResponse,
    FilesystemQueryJson, FilesystemResultJson, FilesystemSearchResponse, SemanticQueryJson,
    SemanticResultJson, SemanticSearchResponse,
};
use crate::api::VaultEntry;
use crate::search::{
    ContentQuery, ContentResult, FilesystemQuery, FilesystemResult, SemanticQuery, SemanticResult,
    search_content, search_filesystem, search_semantic,
};

const DEFAULT_LIMIT: usize = 100;
const DEFAULT_MAX_MATCHES_PER_FILE: usize = 5;

pub(crate) async fn filesystem(
    State(s): State<ApiState>,
    ApiJson(req): ApiJson<FilesystemQueryJson>,
) -> Result<Json<FilesystemSearchResponse>, ApiError> {
    let limit = req.limit.unwrap_or(DEFAULT_LIMIT);
    let q_template = FilesystemQuery {
        prefix: req.prefix,
        glob: req.glob,
        max_depth: req.max_depth,
        limit,
    };

    let mut all_results: Vec<FilesystemResultJson> = Vec::new();
    let mut any_truncated = false;
    for vault in s.vaults.iter() {
        let (rows, truncated) = search_filesystem(vault.store.pool(), q_template.clone()).await?;
        any_truncated |= truncated;
        for r in rows {
            all_results.push(filesystem_to_json(r, vault));
        }
    }
    let (results, response_truncated) = merge_and_truncate(all_results, limit, any_truncated);
    Ok(Json(FilesystemSearchResponse {
        results,
        truncated: response_truncated,
    }))
}

pub(crate) async fn content(
    State(s): State<ApiState>,
    ApiJson(req): ApiJson<ContentQueryJson>,
) -> Result<Json<ContentSearchResponse>, ApiError> {
    let limit = req.limit.unwrap_or(DEFAULT_LIMIT);
    let q_template = ContentQuery {
        query: req.query,
        regex: req.regex,
        case_sensitive: req.case_sensitive,
        prefix: req.prefix,
        include_matches: req.include_matches,
        max_matches_per_file: req
            .max_matches_per_file
            .unwrap_or(DEFAULT_MAX_MATCHES_PER_FILE),
        limit,
    };

    let mut all_results: Vec<ContentResultJson> = Vec::new();
    let mut any_truncated = false;
    for vault in s.vaults.iter() {
        let (rows, truncated) = search_content(vault.store.pool(), q_template.clone()).await?;
        any_truncated |= truncated;
        for r in rows {
            all_results.push(content_to_json(r, vault));
        }
    }
    let (results, response_truncated) = merge_and_truncate(all_results, limit, any_truncated);
    Ok(Json(ContentSearchResponse {
        results,
        truncated: response_truncated,
    }))
}

fn filesystem_to_json(r: FilesystemResult, vault: &VaultEntry) -> FilesystemResultJson {
    FilesystemResultJson {
        path: r.path,
        size: r.size,
        mtime: r.mtime,
        content_hash: r.content_hash,
        vault: Some(vault.id.to_string()),
        vault_name: Some(vault.name.clone()),
    }
}

pub(crate) async fn semantic(
    State(s): State<ApiState>,
    ApiJson(req): ApiJson<SemanticQueryJson>,
) -> Result<Json<SemanticSearchResponse>, ApiError> {
    let limit = req.limit.unwrap_or(DEFAULT_LIMIT);
    let q_template = SemanticQuery {
        query: req.query,
        prefix: req.prefix,
        limit,
        min_similarity: req.min_similarity.unwrap_or(0.0).clamp(0.0, 1.0),
    };

    let mut all_results: Vec<SemanticResultJson> = Vec::new();
    let mut any_hint: Option<String> = None;
    for vault in s.vaults.iter() {
        let (rows, hint) = search_semantic(
            vault.store.pool(),
            s.embedder.clone(),
            s.embedding_dimension,
            q_template.clone(),
        )
        .await
        .map_err(ApiError::from)?;
        if any_hint.is_none() && hint.is_some() {
            any_hint = hint;
        }
        for r in rows {
            all_results.push(semantic_to_json(r, vault));
        }
    }

    // Step-9 N=1 passthrough; cross-vault score-merge semantics land in
    // step 10 (Resolution F preview). Sort by score desc as a deterministic
    // default for the multi-vault case while keeping the single-vault order
    // identical (search_semantic already returns ASC by distance == DESC by
    // score, and stable sort preserves ties).
    all_results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    if all_results.len() > limit {
        all_results.truncate(limit);
    }
    // Step 9 only emits the hint when the result set is empty (matches v0
    // semantic.rs contract). Suppress the hint when we have results.
    let hint = if all_results.is_empty() {
        any_hint
    } else {
        None
    };

    Ok(Json(SemanticSearchResponse {
        results: all_results,
        hint,
    }))
}

fn semantic_to_json(r: SemanticResult, vault: &VaultEntry) -> SemanticResultJson {
    SemanticResultJson {
        score: r.score,
        file_path: r.file_path,
        chunk_index: r.chunk_index,
        heading_path: r.heading_path.split('/').map(String::from).collect(),
        text: r.text,
        vault: Some(vault.id.to_string()),
        vault_name: Some(vault.name.clone()),
    }
}

fn content_to_json(r: ContentResult, vault: &VaultEntry) -> ContentResultJson {
    ContentResultJson {
        path: r.path,
        match_count: r.match_count,
        matches: r
            .matches
            .into_iter()
            .map(|m| ContentMatchJson {
                line: m.line,
                text: m.text,
            })
            .collect(),
        vault: Some(vault.id.to_string()),
        vault_name: Some(vault.name.clone()),
    }
}

// Cross-vault merge for the truncate-aware list responses.
//
// Step-9 invariant: with N=1 active vaults the merge is a passthrough — the
// per-vault search already truncated at `limit` and reported `truncated`.
// With N>=2, the gathered list can exceed `limit` even when no per-vault
// search hit its own cap, so we re-truncate at the global limit and surface
// `truncated = true` whenever any per-vault search reported truncation OR
// the merged list itself was capped.
fn merge_and_truncate<T>(
    mut all: Vec<T>,
    limit: usize,
    per_vault_truncated: bool,
) -> (Vec<T>, bool) {
    let was_capped = all.len() > limit;
    if was_capped {
        all.truncate(limit);
    }
    (all, per_vault_truncated || was_capped)
}
