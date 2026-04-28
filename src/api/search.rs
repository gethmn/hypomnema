use std::collections::HashSet;
use std::sync::Arc;

use axum::{Json, extract::State};

use super::ApiState;
use super::error::{ApiError, ApiJson};
use super::types::{
    ContentMatchJson, ContentQueryJson, ContentResultJson, ContentSearchResponse, FailedVault,
    FilesystemQueryJson, FilesystemResultJson, FilesystemSearchResponse, PartialResults,
    SemanticQueryJson, SemanticResultJson, SemanticSearchResponse, SkippedVault,
};
use crate::api::VaultEntry;
use crate::control_plane::VaultScopeRow;
use crate::search::{
    ContentQuery, ContentResult, FilesystemQuery, FilesystemResult, SemanticQuery, SemanticResult,
    SemanticSearchError, search_content, search_filesystem, search_semantic,
};
use crate::vault_registry::{VaultId, VaultStatus};

const DEFAULT_LIMIT: usize = 100;
const DEFAULT_MAX_MATCHES_PER_FILE: usize = 5;

pub(crate) async fn filesystem(
    State(s): State<ApiState>,
    ApiJson(req): ApiJson<FilesystemQueryJson>,
) -> Result<Json<FilesystemSearchResponse>, ApiError> {
    if let Some(filter) = req.vaults.as_deref() {
        validate_filter_non_empty(filter)?;
    }
    let limit = req.limit.unwrap_or(DEFAULT_LIMIT);
    let q_template = FilesystemQuery {
        prefix: req.prefix,
        glob: req.glob,
        max_depth: req.max_depth,
        limit,
    };

    let scope = s.vault_manager.search_scope().await?;
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

    Ok(Json(FilesystemSearchResponse {
        results: all_results,
        truncated: any_truncated || was_capped,
        partial_results: build_partial_results(skipped, failed),
    }))
}

pub(crate) async fn content(
    State(s): State<ApiState>,
    ApiJson(req): ApiJson<ContentQueryJson>,
) -> Result<Json<ContentSearchResponse>, ApiError> {
    if let Some(filter) = req.vaults.as_deref() {
        validate_filter_non_empty(filter)?;
    }
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

    let scope = s.vault_manager.search_scope().await?;
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

    sort_by_path_then_vault(&mut all_results, |r| (&r.path, r.vault.as_deref()));
    let was_capped = all_results.len() > limit;
    if was_capped {
        all_results.truncate(limit);
    }

    Ok(Json(ContentSearchResponse {
        results: all_results,
        truncated: any_truncated || was_capped,
        partial_results: build_partial_results(skipped, failed),
    }))
}

pub(crate) async fn semantic(
    State(s): State<ApiState>,
    ApiJson(req): ApiJson<SemanticQueryJson>,
) -> Result<Json<SemanticSearchResponse>, ApiError> {
    if let Some(filter) = req.vaults.as_deref() {
        validate_filter_non_empty(filter)?;
    }
    let limit = req.limit.unwrap_or(DEFAULT_LIMIT);
    let q_template = SemanticQuery {
        query: req.query,
        prefix: req.prefix,
        limit,
        min_similarity: req.min_similarity.unwrap_or(0.0).clamp(0.0, 1.0),
    };

    let embedder = s.vault_manager.embedder();
    let dimension = s.vault_manager.embedding_dimension();

    let scope = s.vault_manager.search_scope().await?;
    let plan = filter_scope(scope, req.vaults.as_deref());

    let mut all_results: Vec<SemanticResultJson> = Vec::new();
    let mut any_hint: Option<String> = None;
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
                    Ok((rows, hint)) => {
                        if any_hint.is_none() && hint.is_some() {
                            any_hint = hint;
                        }
                        for r in rows {
                            all_results.push(semantic_to_json(r, &entry));
                        }
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

    // Score-desc with vault-id tie-break, per spec § Cross-Vault Search
    // Semantics § Result ordering — semantic-search.
    all_results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.vault.as_deref().cmp(&b.vault.as_deref()))
    });
    if all_results.len() > limit {
        all_results.truncate(limit);
    }
    let hint = if all_results.is_empty() {
        any_hint
    } else {
        None
    };

    Ok(Json(SemanticSearchResponse {
        results: all_results,
        hint,
        partial_results: build_partial_results(skipped, failed),
    }))
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
    }
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
}
