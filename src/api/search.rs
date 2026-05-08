use std::collections::HashSet;
use std::sync::Arc;

use axum::http::StatusCode;
use axum::{Json, extract::State};

use super::ApiState;
use super::error::{ApiError, ApiJson};
use super::types::{
    ContentGetError, ContentGetErrorDetail, ContentGetRequest, ContentGetResponse,
    ContentGetResultItem, ContentGetSuccess, ContentMatchJson, ContentQueryJson, ContentResultJson,
    ContentSearchResponse, FailedVault, FilesystemQueryJson, FilesystemResultJson,
    FilesystemSearchResponse, PartialResults, SemanticDocumentResultJson,
    SemanticEvidenceChunkJson, SemanticQueryJson, SemanticResultItem, SemanticResultJson,
    SemanticSearchResponse, SkippedVault,
};
use crate::api::VaultEntry;
use crate::config::SemanticSearchConfig;
use crate::control_plane::{VaultManager, VaultScopeRow};
use crate::search::{
    ContentMode, ContentQuery, ContentResult, FilesystemQuery, FilesystemResult, SemanticQuery,
    SemanticResult, SemanticSearchError, content_get_by_paths, search_content, search_filesystem,
    search_semantic,
};
use crate::vault_registry::{VaultId, VaultStatus};

const DEFAULT_LIMIT: usize = 100;
const DEFAULT_SEMANTIC_LIMIT: usize = 10;
const DEFAULT_MAX_MATCHES_PER_FILE: usize = 5;
const DEFAULT_PREVIEW_BYTES: usize = 600;
const SEMANTIC_PREVIEW_BYTES_MAX: usize = 2000;

enum IncludeText {
    Preview,
    Full,
    None,
}

fn parse_include_text(opt: Option<&str>) -> Result<IncludeText, ApiError> {
    match opt {
        None | Some("preview") => Ok(IncludeText::Preview),
        Some("full") => Ok(IncludeText::Full),
        Some("none") => Ok(IncludeText::None),
        _ => Err(ApiError::invalid_request(
            "include_text must be one of preview, full, none",
        )),
    }
}

fn resolve_preview_bytes(opt: Option<usize>) -> Result<usize, ApiError> {
    match opt {
        None => Ok(DEFAULT_PREVIEW_BYTES),
        Some(0) => Err(ApiError::invalid_request(
            "preview_bytes must be greater than 0",
        )),
        Some(n) => Ok(n.min(SEMANTIC_PREVIEW_BYTES_MAX)),
    }
}

pub(crate) async fn filesystem(
    State(s): State<ApiState>,
    ApiJson(req): ApiJson<FilesystemQueryJson>,
) -> Result<Json<FilesystemSearchResponse>, ApiError> {
    run_filesystem_search(&s.vault_manager, &req)
        .await
        .map(Json)
}

pub(crate) async fn run_filesystem_search(
    manager: &VaultManager,
    req: &FilesystemQueryJson,
) -> Result<FilesystemSearchResponse, ApiError> {
    if let Some(filter) = req.vaults.as_deref() {
        validate_filter_non_empty(filter)?;
    }
    let limit = req.limit.unwrap_or(DEFAULT_LIMIT);
    let q_template = FilesystemQuery {
        prefix: req.prefix.clone(),
        glob: req.glob.clone(),
        max_depth: req.max_depth,
        limit,
    };

    let scope = manager.search_scope().await?;
    let plan = filter_scope(scope, req.vaults.as_deref());

    let mut all_results: Vec<FilesystemResultJson> = Vec::new();
    let mut any_truncated = false;
    let mut skipped: Vec<SkippedVault> = Vec::new();
    let mut failed: Vec<FailedVault> = plan.unknown_failures;

    for row in plan.in_scope {
        match (row.status, row.entry) {
            (VaultStatus::Active, Some(entry)) => {
                match search_filesystem(entry.store.pool(), q_template.clone()).await {
                    Ok((rows, truncated)) => {
                        any_truncated |= truncated;
                        for r in rows {
                            all_results.push(filesystem_to_json(r, &entry));
                        }
                    }
                    Err(e) => {
                        // Per-vault search errors should not surface as a
                        // request-level invalid_glob/invalid_regex/
                        // invalid_prefix failure when other vaults can
                        // satisfy the request, so we distinguish:
                        // request-validation errors bubble up; storage-class
                        // errors become per-vault `failed` entries.
                        if anyhow_is_request_validation(&e) {
                            return Err(ApiError::from(e));
                        }
                        failed.push(FailedVault {
                            vault: entry.id.to_string(),
                            vault_name: entry.name.clone(),
                            code: "vault_search_failed".to_string(),
                            message: format!("{e:#}"),
                        });
                    }
                }
            }
            (VaultStatus::Paused, _) => skipped.push(skipped_for_paused(&row.id, &row.name)),
            (VaultStatus::Errored, _) => skipped.push(skipped_for_errored(
                &row.id,
                &row.name,
                row.last_error.as_deref(),
            )),
            (VaultStatus::Active, None) => failed.push(no_runner_failure(&row.id, &row.name)),
        }
    }

    sort_by_path_then_vault(&mut all_results, |r| (&r.path, r.vault.as_deref()));
    let was_capped = all_results.len() > limit;
    if was_capped {
        all_results.truncate(limit);
    }

    Ok(FilesystemSearchResponse {
        results: all_results,
        truncated: any_truncated || was_capped,
        partial_results: build_partial_results(skipped, failed),
    })
}

pub(crate) async fn content(
    State(s): State<ApiState>,
    ApiJson(req): ApiJson<ContentQueryJson>,
) -> Result<Json<ContentSearchResponse>, ApiError> {
    run_content_search(&s.vault_manager, &req).await.map(Json)
}

pub(crate) async fn run_content_search(
    manager: &VaultManager,
    req: &ContentQueryJson,
) -> Result<ContentSearchResponse, ApiError> {
    if let Some(filter) = req.vaults.as_deref() {
        validate_filter_non_empty(filter)?;
    }

    // Resolve and validate mode (Task 3.2)
    let resolved_mode = resolve_content_mode(req)?;

    let limit = req.limit.unwrap_or(DEFAULT_LIMIT);
    let q_template = ContentQuery {
        query: req.query.clone(),
        mode: resolved_mode,
        regex: req.regex,
        case_sensitive: req.case_sensitive,
        prefix: req.prefix.clone(),
        include_matches: req.include_matches,
        max_matches_per_file: req
            .max_matches_per_file
            .unwrap_or(DEFAULT_MAX_MATCHES_PER_FILE),
        limit,
    };

    let scope = manager.search_scope().await?;
    let plan = filter_scope(scope, req.vaults.as_deref());

    let mut all_results: Vec<ContentResultJson> = Vec::new();
    let mut any_truncated = false;
    let mut skipped: Vec<SkippedVault> = Vec::new();
    let mut failed: Vec<FailedVault> = plan.unknown_failures;

    for row in plan.in_scope {
        match (row.status, row.entry) {
            (VaultStatus::Active, Some(entry)) => {
                match search_content(entry.store.pool(), q_template.clone()).await {
                    Ok((rows, truncated)) => {
                        any_truncated |= truncated;
                        for r in rows {
                            all_results.push(content_to_json(r, &entry));
                        }
                    }
                    Err(e) => {
                        if anyhow_is_request_validation(&e) {
                            return Err(ApiError::from(e));
                        }
                        failed.push(FailedVault {
                            vault: entry.id.to_string(),
                            vault_name: entry.name.clone(),
                            code: "vault_search_failed".to_string(),
                            message: format!("{e:#}"),
                        });
                    }
                }
            }
            (VaultStatus::Paused, _) => skipped.push(skipped_for_paused(&row.id, &row.name)),
            (VaultStatus::Errored, _) => skipped.push(skipped_for_errored(
                &row.id,
                &row.name,
                row.last_error.as_deref(),
            )),
            (VaultStatus::Active, None) => failed.push(no_runner_failure(&row.id, &row.name)),
        }
    }

    // Ranked mode: merge by (score ASC, path ASC, vault_id ASC), re-rank (Task 4.4)
    if q_template.mode == ContentMode::Ranked {
        all_results.sort_by(|a, b| {
            let score_a = a.score.unwrap_or(0.0);
            let score_b = b.score.unwrap_or(0.0);
            score_a
                .partial_cmp(&score_b)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.path.cmp(&b.path))
                .then_with(|| a.vault.as_deref().cmp(&b.vault.as_deref()))
        });
        let was_capped = all_results.len() > limit;
        if was_capped {
            all_results.truncate(limit);
        }
        // Re-assign consumer-facing rank after global sort
        for (i, r) in all_results.iter_mut().enumerate() {
            r.rank = Some((i + 1) as u32);
        }
        return Ok(ContentSearchResponse {
            results: all_results,
            truncated: any_truncated || was_capped,
            partial_results: build_partial_results(skipped, failed),
        });
    }

    sort_by_path_then_vault(&mut all_results, |r| (&r.path, r.vault.as_deref()));
    let was_capped = all_results.len() > limit;
    if was_capped {
        all_results.truncate(limit);
    }

    Ok(ContentSearchResponse {
        results: all_results,
        truncated: any_truncated || was_capped,
        partial_results: build_partial_results(skipped, failed),
    })
}

/// Resolve the wire-level `mode` + legacy `regex` flag into a `ContentMode`.
/// Returns `invalid_request` for conflicting inputs; `invalid_query` for
/// empty ranked query.
fn resolve_content_mode(req: &ContentQueryJson) -> Result<ContentMode, ApiError> {
    match (req.mode.as_deref(), req.regex) {
        // Conflict: both mode and legacy regex flag set
        (Some(m), true) if m != "regex" => {
            return Err(ApiError::invalid_request(
                "cannot combine `regex: true` with `mode` (unless mode is \"regex\")",
            ));
        }
        // Conflict: ranked + case_sensitive
        (Some("ranked"), _) if req.case_sensitive => {
            return Err(ApiError::invalid_request(
                "ranked search is case-insensitive; cannot combine with case_sensitive: true",
            ));
        }
        // Ranked with empty query
        (Some("ranked"), _) if req.query.is_empty() => {
            return Err(ApiError::new(
                "invalid_query",
                "ranked search requires a non-empty query",
                axum::http::StatusCode::BAD_REQUEST,
            ));
        }
        _ => {}
    }
    let mode = match req.mode.as_deref() {
        Some("ranked") => ContentMode::Ranked,
        Some("regex") => ContentMode::Regex,
        Some("substring") | None if req.regex => ContentMode::Regex,
        Some("substring") | None => ContentMode::Substring,
        Some(other) => {
            return Err(ApiError::invalid_request(format!(
                "unknown mode \"{other}\"; expected \"substring\", \"regex\", or \"ranked\""
            )));
        }
    };
    Ok(mode)
}

pub(crate) async fn semantic(
    State(s): State<ApiState>,
    ApiJson(req): ApiJson<SemanticQueryJson>,
) -> Result<Json<SemanticSearchResponse>, ApiError> {
    run_semantic_search(&s.vault_manager, &req, &s.semantic_config)
        .await
        .map(Json)
}

pub(crate) async fn run_semantic_search(
    manager: &VaultManager,
    req: &SemanticQueryJson,
    semantic_cfg: &SemanticSearchConfig,
) -> Result<SemanticSearchResponse, ApiError> {
    if let Some(filter) = req.vaults.as_deref() {
        validate_filter_non_empty(filter)?;
    }

    // Validate and resolve granularity: request -> config -> built-in default.
    let effective_granularity =
        resolve_effective_granularity(req.granularity.as_deref(), semantic_cfg)?;

    // Validate chunks_per_document; resolve effective value for document mode.
    // Accepted but ignored in chunk mode (workplan decision 2).
    let effective_chunks_per_document =
        resolve_effective_chunks_per_document(req.chunks_per_document, semantic_cfg)?;

    let include_text = parse_include_text(req.include_text.as_deref())?;
    let preview_bytes = resolve_preview_bytes(req.preview_bytes)?;
    let limit = req.limit.unwrap_or(DEFAULT_SEMANTIC_LIMIT);
    let is_document_mode = effective_granularity == "document";

    // Chunk mode uses the user's `limit` directly as candidate depth.
    // Document mode requests a deeper per-vault candidate set so grouping has
    // enough chunks to draw from:
    //   min(limit * document_candidate_multiplier, document_candidate_limit)
    let candidate_limit = if is_document_mode {
        (limit as u64)
            .saturating_mul(semantic_cfg.document_candidate_multiplier as u64)
            .min(semantic_cfg.document_candidate_limit as u64)
            .max(1) as usize
    } else {
        limit
    };
    let q_template = SemanticQuery {
        query: req.query.clone(),
        prefix: req.prefix.clone(),
        candidate_limit,
        min_similarity: req.min_similarity.unwrap_or(0.0).clamp(0.0, 1.0),
    };

    let embedder = manager.embedder();
    let dimension = manager.embedding_dimension();

    let scope = manager.search_scope().await?;
    let plan = filter_scope(scope, req.vaults.as_deref());

    // Collected per-vault candidate rows. We hold onto the originating vault
    // entry for each row so the document-mode grouping below can populate
    // `vault` / `vault_name` and use the vault id as part of the grouping key.
    let mut per_vault_rows: Vec<(Arc<VaultEntry>, Vec<SemanticResult>)> = Vec::new();
    let mut any_hint: Option<String> = None;
    let mut any_truncated = false;
    let mut skipped: Vec<SkippedVault> = Vec::new();
    let mut failed: Vec<FailedVault> = plan.unknown_failures;

    for row in plan.in_scope {
        match (row.status, row.entry) {
            (VaultStatus::Active, Some(entry)) => {
                let res = search_semantic(
                    entry.store.pool(),
                    embedder.clone(),
                    dimension,
                    q_template.clone(),
                )
                .await;
                match res {
                    Ok((rows, hint, vault_truncated)) => {
                        any_truncated |= vault_truncated;
                        if any_hint.is_none() && hint.is_some() {
                            any_hint = hint;
                        }
                        per_vault_rows.push((entry, rows));
                    }
                    Err(e) => match e {
                        SemanticSearchError::EmbeddingUnavailable { .. }
                        | SemanticSearchError::InvalidPrefix(_) => return Err(ApiError::from(e)),
                        SemanticSearchError::Internal(inner) => {
                            failed.push(FailedVault {
                                vault: entry.id.to_string(),
                                vault_name: entry.name.clone(),
                                code: "vault_search_failed".to_string(),
                                message: format!("{inner:#}"),
                            });
                        }
                    },
                }
            }
            (VaultStatus::Paused, _) => skipped.push(skipped_for_paused(&row.id, &row.name)),
            (VaultStatus::Errored, _) => skipped.push(skipped_for_errored(
                &row.id,
                &row.name,
                row.last_error.as_deref(),
            )),
            (VaultStatus::Active, None) => failed.push(no_runner_failure(&row.id, &row.name)),
        }
    }

    let (all_results, was_capped) = if is_document_mode {
        build_document_results(
            per_vault_rows,
            limit,
            effective_chunks_per_document as usize,
            &include_text,
            preview_bytes,
        )
    } else {
        build_chunk_results(per_vault_rows, limit, &include_text, preview_bytes)
    };

    let hint = if all_results.is_empty() {
        any_hint
    } else {
        None
    };

    Ok(SemanticSearchResponse {
        results: all_results,
        truncated: any_truncated || was_capped,
        hint,
        partial_results: build_partial_results(skipped, failed),
    })
}

/// Flatten per-vault candidate rows into chunk-mode results, sorted by score
/// desc then vault id, capped at `limit`. Returns `(results, was_capped)`.
fn build_chunk_results(
    per_vault_rows: Vec<(Arc<VaultEntry>, Vec<SemanticResult>)>,
    limit: usize,
    include_text: &IncludeText,
    preview_bytes: usize,
) -> (Vec<SemanticResultItem>, bool) {
    let mut all_results: Vec<SemanticResultItem> = Vec::new();
    for (entry, rows) in per_vault_rows {
        for r in rows {
            all_results.push(SemanticResultItem::Chunk(semantic_to_json(
                r,
                &entry,
                include_text,
                preview_bytes,
            )));
        }
    }
    // Score-desc with vault-id tie-break, per spec § Cross-Vault Search
    // Semantics § Result ordering — semantic-search.
    all_results.sort_by(|a, b| {
        let sa = semantic_result_item_score(a);
        let sb = semantic_result_item_score(b);
        sb.partial_cmp(&sa)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| semantic_result_item_vault(a).cmp(&semantic_result_item_vault(b)))
    });
    let was_capped = all_results.len() > limit;
    if was_capped {
        all_results.truncate(limit);
    }
    (all_results, was_capped)
}

/// Group per-vault candidate rows into documents keyed by
/// `(vault_id, file_path, content_hash)`, sort/cap evidence chunks within
/// each document, then sort and cap documents across vaults. Returns
/// `(results, was_capped)`.
fn build_document_results(
    per_vault_rows: Vec<(Arc<VaultEntry>, Vec<SemanticResult>)>,
    limit: usize,
    chunks_per_document: usize,
    include_text: &IncludeText,
    preview_bytes: usize,
) -> (Vec<SemanticResultItem>, bool) {
    use std::collections::BTreeMap;

    // Use a BTreeMap so iteration order is deterministic in the unlikely event
    // that multiple documents tie on (score, vault_id, file_path) — though the
    // explicit sort below is the load-bearing source of order.
    type DocKey = (String, String, String); // (vault_id, file_path, content_hash)
    struct DocAcc {
        entry: Arc<VaultEntry>,
        rows: Vec<SemanticResult>,
    }
    let mut docs: BTreeMap<DocKey, DocAcc> = BTreeMap::new();

    for (entry, rows) in per_vault_rows {
        for r in rows {
            let key = (
                entry.id.to_string(),
                r.file_path.clone(),
                r.content_hash.clone(),
            );
            docs.entry(key)
                .or_insert_with(|| DocAcc {
                    entry: entry.clone(),
                    rows: Vec::new(),
                })
                .rows
                .push(r);
        }
    }

    let mut docs_out: Vec<SemanticDocumentResultJson> = Vec::with_capacity(docs.len());
    for (_key, mut acc) in docs {
        // Score desc, then chunk_index asc, deterministic ties.
        acc.rows.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.chunk_index.cmp(&b.chunk_index))
        });
        let best_score = acc.rows.first().map(|r| r.score).unwrap_or(0.0);
        let file_path = acc.rows[0].file_path.clone();
        let content_hash = acc.rows[0].content_hash.clone();
        acc.rows.truncate(chunks_per_document);
        let chunks: Vec<SemanticEvidenceChunkJson> = acc
            .rows
            .into_iter()
            .map(|r| evidence_chunk_to_json(r, include_text, preview_bytes))
            .collect();
        docs_out.push(SemanticDocumentResultJson {
            score: best_score,
            file_path,
            content_hash,
            chunks,
            vault: Some(acc.entry.id.to_string()),
            vault_name: Some(acc.entry.name.clone()),
        });
    }

    // Cross-vault document merge: score desc, then vault id asc, then file
    // path asc — workplan Task 3 shipping criterion.
    docs_out.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.vault.as_deref().cmp(&b.vault.as_deref()))
            .then_with(|| a.file_path.cmp(&b.file_path))
    });
    let was_capped = docs_out.len() > limit;
    if was_capped {
        docs_out.truncate(limit);
    }
    let results = docs_out
        .into_iter()
        .map(SemanticResultItem::Document)
        .collect();
    (results, was_capped)
}

fn evidence_chunk_to_json(
    r: SemanticResult,
    include_text: &IncludeText,
    preview_bytes: usize,
) -> SemanticEvidenceChunkJson {
    let (text, text_kind, text_truncated) = match include_text {
        IncludeText::None => (None, None, None),
        IncludeText::Full => (Some(r.text), Some("full".to_string()), Some(false)),
        IncludeText::Preview => {
            let (preview, was_truncated) = make_preview(&r.text, preview_bytes);
            (
                Some(preview),
                Some("preview".to_string()),
                Some(was_truncated),
            )
        }
    };
    SemanticEvidenceChunkJson {
        score: r.score,
        chunk_index: r.chunk_index,
        heading_path: r.heading_path.split('/').map(String::from).collect(),
        text,
        text_kind,
        text_truncated,
    }
}

pub(crate) async fn content_get(
    State(s): State<ApiState>,
    ApiJson(req): ApiJson<ContentGetRequest>,
) -> Result<Json<ContentGetResponse>, ApiError> {
    run_content_get(&s.vault_manager, &req).await.map(Json)
}

pub(crate) async fn run_content_get(
    manager: &VaultManager,
    req: &ContentGetRequest,
) -> Result<ContentGetResponse, ApiError> {
    // 1. Validate request
    if req.paths.is_empty() {
        return Err(ApiError::invalid_request("paths must not be empty"));
    }
    for path in &req.paths {
        validate_retrieval_path(path)?;
    }
    if let Some(vaults) = &req.vaults {
        if vaults.is_empty() {
            return Err(ApiError::invalid_request(
                "vaults must not be empty if provided",
            ));
        }
    }

    // 2. Normalize paths: strip leading "./"
    let normalized_paths: Vec<String> = req
        .paths
        .iter()
        .map(|p| normalize_retrieval_path(p))
        .collect();

    // 3. Fan-out across vaults
    let scope = manager.search_scope().await?;
    let plan = filter_scope(scope, req.vaults.as_deref());

    let mut all_results: Vec<ContentGetResultItem> = Vec::new();
    let mut skipped: Vec<SkippedVault> = Vec::new();
    let mut failed: Vec<FailedVault> = plan.unknown_failures;

    for row in plan.in_scope {
        match (row.status, row.entry) {
            (VaultStatus::Active, Some(entry)) => {
                match content_get_by_paths(entry.store.pool(), normalized_paths.clone()).await {
                    Ok(rows) => {
                        for (path, opt_row) in rows {
                            let item = match opt_row {
                                Some(r) => ContentGetResultItem::Success(ContentGetSuccess {
                                    path,
                                    content: r.content,
                                    content_hash: r.content_hash,
                                    size: r.size,
                                    mtime: r.mtime,
                                    vault: entry.id.to_string(),
                                    vault_name: entry.name.clone(),
                                }),
                                None => ContentGetResultItem::Error(ContentGetError {
                                    path,
                                    vault: entry.id.to_string(),
                                    vault_name: entry.name.clone(),
                                    error: ContentGetErrorDetail {
                                        code: "path_not_found".to_string(),
                                        message: "path not found in vault index".to_string(),
                                    },
                                }),
                            };
                            all_results.push(item);
                        }
                    }
                    Err(e) => {
                        failed.push(FailedVault {
                            vault: entry.id.to_string(),
                            vault_name: entry.name.clone(),
                            code: "vault_search_failed".to_string(),
                            message: format!("{e:#}"),
                        });
                    }
                }
            }
            (VaultStatus::Paused, _) => skipped.push(skipped_for_paused(&row.id, &row.name)),
            (VaultStatus::Errored, _) => skipped.push(skipped_for_errored(
                &row.id,
                &row.name,
                row.last_error.as_deref(),
            )),
            (VaultStatus::Active, None) => failed.push(no_runner_failure(&row.id, &row.name)),
        }
    }

    // 4. Sort by (path ASC, vault_id ASC)
    all_results.sort_by(|a, b| {
        let (path_a, vault_a) = result_item_sort_key(a);
        let (path_b, vault_b) = result_item_sort_key(b);
        path_a.cmp(path_b).then_with(|| vault_a.cmp(vault_b))
    });

    Ok(ContentGetResponse {
        results: all_results,
        partial_results: build_partial_results(skipped, failed),
    })
}

fn result_item_sort_key(item: &ContentGetResultItem) -> (&str, &str) {
    match item {
        ContentGetResultItem::Success(s) => (s.path.as_str(), s.vault.as_str()),
        ContentGetResultItem::Error(e) => (e.path.as_str(), e.vault.as_str()),
    }
}

fn normalize_retrieval_path(path: &str) -> String {
    // Strip a leading "./" component and collapse any duplicate internal slashes.
    let stripped = path.strip_prefix("./").unwrap_or(path);
    // Collapse duplicate slashes (simple pass — do not shell out)
    let mut result = String::with_capacity(stripped.len());
    let mut prev_slash = false;
    for ch in stripped.chars() {
        if ch == '/' {
            if !prev_slash {
                result.push(ch);
            }
            prev_slash = true;
        } else {
            result.push(ch);
            prev_slash = false;
        }
    }
    result
}

pub(crate) fn validate_retrieval_path(path: &str) -> Result<(), ApiError> {
    if path.is_empty() {
        return Err(ApiError::new(
            "invalid_path",
            "path must not be empty",
            StatusCode::UNPROCESSABLE_ENTITY,
        ));
    }
    if path.starts_with('/') {
        return Err(ApiError::new(
            "invalid_path",
            "path must be vault-relative, not absolute",
            StatusCode::UNPROCESSABLE_ENTITY,
        ));
    }
    if path.split('/').any(|seg| seg == "..") {
        return Err(ApiError::new(
            "invalid_path",
            "path must not contain .. segments",
            StatusCode::UNPROCESSABLE_ENTITY,
        ));
    }
    Ok(())
}

// ===== Helpers =====

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
        score: r.score,
        rank: r.rank,
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

fn semantic_to_json(
    r: SemanticResult,
    vault: &VaultEntry,
    include_text: &IncludeText,
    preview_bytes: usize,
) -> SemanticResultJson {
    let (text, text_kind, text_truncated) = match include_text {
        IncludeText::None => (None, None, None),
        IncludeText::Full => (Some(r.text), Some("full".to_string()), Some(false)),
        IncludeText::Preview => {
            let (preview, was_truncated) = make_preview(&r.text, preview_bytes);
            (
                Some(preview),
                Some("preview".to_string()),
                Some(was_truncated),
            )
        }
    };
    SemanticResultJson {
        score: r.score,
        file_path: r.file_path,
        chunk_index: r.chunk_index,
        heading_path: r.heading_path.split('/').map(String::from).collect(),
        text,
        text_kind,
        text_truncated,
        content_hash: r.content_hash,
        vault: Some(vault.id.to_string()),
        vault_name: Some(vault.name.clone()),
    }
}

fn semantic_result_item_score(item: &SemanticResultItem) -> f32 {
    match item {
        SemanticResultItem::Chunk(c) => c.score,
        SemanticResultItem::Document(d) => d.score,
    }
}

fn semantic_result_item_vault(item: &SemanticResultItem) -> Option<&str> {
    match item {
        SemanticResultItem::Chunk(c) => c.vault.as_deref(),
        SemanticResultItem::Document(d) => d.vault.as_deref(),
    }
}

fn resolve_effective_granularity(
    req_granularity: Option<&str>,
    cfg: &SemanticSearchConfig,
) -> Result<String, ApiError> {
    match req_granularity {
        Some("document") => Ok("document".to_string()),
        Some("chunk") => Ok("chunk".to_string()),
        Some(other) => Err(ApiError::invalid_request(format!(
            "granularity must be \"document\" or \"chunk\", got \"{other}\""
        ))),
        None => Ok(cfg.default_granularity.clone()),
    }
}

fn resolve_effective_chunks_per_document(
    req_value: Option<u32>,
    cfg: &SemanticSearchConfig,
) -> Result<u32, ApiError> {
    match req_value {
        Some(n) if !(1..=100).contains(&n) => Err(ApiError::invalid_request(format!(
            "chunks_per_document must be in 1..=100, got {n}"
        ))),
        Some(n) => Ok(n),
        None => Ok(cfg.default_chunks_per_document),
    }
}

fn validate_filter_non_empty(filter: &[String]) -> Result<(), ApiError> {
    if filter.is_empty() {
        return Err(ApiError::invalid_request("vaults filter must be non-empty"));
    }
    Ok(())
}

struct ScopePlan {
    in_scope: Vec<VaultScopeRow>,
    unknown_failures: Vec<FailedVault>,
}

/// Apply the request-side `vaults` filter to the manager-supplied scope
/// list. Match each filter entry against `name` first, then `id` (per spec
/// § Cross-Vault Search Semantics § `vaults` filter). Unknown filter
/// entries are surfaced as `partial_results.failed` with code
/// `vault_not_found`. With no filter, every scope row is in-scope.
fn filter_scope(scope: Vec<VaultScopeRow>, filter: Option<&[String]>) -> ScopePlan {
    let Some(filter) = filter else {
        return ScopePlan {
            in_scope: scope,
            unknown_failures: Vec::new(),
        };
    };
    let mut in_scope: Vec<VaultScopeRow> = Vec::new();
    let mut included: HashSet<VaultId> = HashSet::new();
    let mut unknown_failures: Vec<FailedVault> = Vec::new();
    for token in filter {
        let Some(idx) = scope
            .iter()
            .position(|r| &r.name == token)
            .or_else(|| scope.iter().position(|r| r.id.as_str() == token))
        else {
            unknown_failures.push(FailedVault {
                vault: token.clone(),
                vault_name: token.clone(),
                code: "vault_not_found".to_string(),
                message: format!("vault {token} not found"),
            });
            continue;
        };
        let row = &scope[idx];
        if included.insert(row.id.clone()) {
            in_scope.push(clone_scope_row(row));
        }
    }
    ScopePlan {
        in_scope,
        unknown_failures,
    }
}

fn clone_scope_row(row: &VaultScopeRow) -> VaultScopeRow {
    VaultScopeRow {
        id: row.id.clone(),
        name: row.name.clone(),
        status: row.status,
        last_error: row.last_error.clone(),
        entry: row.entry.as_ref().map(Arc::clone),
    }
}

fn skipped_for_paused(id: &VaultId, name: &str) -> SkippedVault {
    SkippedVault {
        vault: id.to_string(),
        vault_name: name.to_string(),
        status: "paused".to_string(),
        reason: "vault is paused".to_string(),
    }
}

fn skipped_for_errored(id: &VaultId, name: &str, last_error: Option<&str>) -> SkippedVault {
    let detail = last_error.and_then(|s| if s.is_empty() { None } else { Some(s) });
    let reason = match detail {
        Some(e) => format!("vault is errored: {e}"),
        None => "vault is errored (no last_error recorded)".to_string(),
    };
    SkippedVault {
        vault: id.to_string(),
        vault_name: name.to_string(),
        status: "errored".to_string(),
        reason,
    }
}

fn no_runner_failure(id: &VaultId, name: &str) -> FailedVault {
    FailedVault {
        vault: id.to_string(),
        vault_name: name.to_string(),
        code: "vault_search_failed".to_string(),
        message: "no live runner for active vault".to_string(),
    }
}

fn build_partial_results(
    skipped: Vec<SkippedVault>,
    failed: Vec<FailedVault>,
) -> Option<PartialResults> {
    if skipped.is_empty() && failed.is_empty() {
        None
    } else {
        Some(PartialResults { skipped, failed })
    }
}

fn sort_by_path_then_vault<T, F>(items: &mut [T], mut key_fn: F)
where
    F: for<'a> FnMut(&'a T) -> (&'a String, Option<&'a str>),
{
    items.sort_by(|a, b| {
        let (path_a, vault_a) = key_fn(a);
        let (path_b, vault_b) = key_fn(b);
        path_a.cmp(path_b).then_with(|| vault_a.cmp(&vault_b))
    });
}

/// True when the anyhow chain returned from `search_filesystem` /
/// `search_content` is a request-validation failure (invalid glob/regex/
/// prefix) that should bubble up as a 400 rather than be appended to
/// `partial_results.failed`. Storage-class errors return `false` so the
/// caller can handle them per-vault.
fn anyhow_is_request_validation(err: &anyhow::Error) -> bool {
    let display = format!("{err:#}");
    display.starts_with("invalid_glob")
        || display.starts_with("invalid_regex")
        || display.starts_with("invalid_prefix")
        || display.starts_with("invalid_query")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SemanticSearchConfig;

    // ===== Step 25 Task 1: granularity and chunks_per_document validation =====

    fn default_cfg() -> SemanticSearchConfig {
        SemanticSearchConfig::default()
    }

    #[test]
    fn resolve_granularity_request_document() {
        let g = resolve_effective_granularity(Some("document"), &default_cfg())
            .map_err(anyhow::Error::from)
            .unwrap();
        assert_eq!(g, "document");
    }

    #[test]
    fn resolve_granularity_request_chunk() {
        let g = resolve_effective_granularity(Some("chunk"), &default_cfg())
            .map_err(anyhow::Error::from)
            .unwrap();
        assert_eq!(g, "chunk");
    }

    #[test]
    fn resolve_granularity_falls_back_to_config() {
        let mut cfg = default_cfg();
        cfg.default_granularity = "chunk".to_string();
        let g = resolve_effective_granularity(None, &cfg)
            .map_err(anyhow::Error::from)
            .unwrap();
        assert_eq!(g, "chunk");
    }

    #[test]
    fn resolve_granularity_rejects_invalid() {
        let err = resolve_effective_granularity(Some("paragraph"), &default_cfg())
            .expect_err("should reject unknown granularity");
        let msg = anyhow::Error::from(err).to_string();
        assert!(
            msg.contains("paragraph"),
            "error should mention the bad value: {msg}"
        );
    }

    #[test]
    fn resolve_chunks_per_document_uses_request() {
        let n = resolve_effective_chunks_per_document(Some(5), &default_cfg())
            .map_err(anyhow::Error::from)
            .unwrap();
        assert_eq!(n, 5);
    }

    #[test]
    fn resolve_chunks_per_document_falls_back_to_config() {
        let mut cfg = default_cfg();
        cfg.default_chunks_per_document = 7;
        let n = resolve_effective_chunks_per_document(None, &cfg)
            .map_err(anyhow::Error::from)
            .unwrap();
        assert_eq!(n, 7);
    }

    #[test]
    fn resolve_chunks_per_document_rejects_zero() {
        let err = resolve_effective_chunks_per_document(Some(0), &default_cfg())
            .expect_err("0 is out of range");
        let msg = anyhow::Error::from(err).to_string();
        assert!(msg.contains("chunks_per_document"), "{msg}");
    }

    #[test]
    fn resolve_chunks_per_document_rejects_over_100() {
        let err = resolve_effective_chunks_per_document(Some(101), &default_cfg())
            .expect_err("101 is out of range");
        let msg = anyhow::Error::from(err).to_string();
        assert!(msg.contains("chunks_per_document"), "{msg}");
    }

    #[test]
    fn resolve_chunks_per_document_accepts_boundary_values() {
        assert_eq!(
            resolve_effective_chunks_per_document(Some(1), &default_cfg())
                .map_err(anyhow::Error::from)
                .unwrap(),
            1
        );
        assert_eq!(
            resolve_effective_chunks_per_document(Some(100), &default_cfg())
                .map_err(anyhow::Error::from)
                .unwrap(),
            100
        );
    }

    // ===== Task 8.1: Unit tests for validate_retrieval_path =====

    #[test]
    fn test_validate_retrieval_path_valid() {
        assert!(validate_retrieval_path("notes/file.md").is_ok());
        assert!(validate_retrieval_path("a.md").is_ok());
        assert!(validate_retrieval_path("deeply/nested/file.md").is_ok());
        // Leading "./" is valid (normalized downstream, not rejected here)
        assert!(validate_retrieval_path("./notes/file.md").is_ok());
    }

    #[test]
    fn test_validate_retrieval_path_invalid_empty() {
        assert!(validate_retrieval_path("").is_err());
    }

    #[test]
    fn test_validate_retrieval_path_invalid_absolute() {
        assert!(validate_retrieval_path("/abs/path.md").is_err());
        assert!(validate_retrieval_path("/etc/passwd").is_err());
    }

    #[test]
    fn test_validate_retrieval_path_invalid_dotdot() {
        assert!(validate_retrieval_path("../escape.md").is_err());
        assert!(validate_retrieval_path("notes/../escape.md").is_err());
        assert!(validate_retrieval_path("a/../../etc/passwd").is_err());
    }
}
