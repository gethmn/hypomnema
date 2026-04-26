use axum::{Json, extract::State};

use super::ApiState;
use super::error::{ApiError, ApiJson};
use super::types::{
    ContentMatchJson, ContentQueryJson, ContentResultJson, ContentSearchResponse,
    FilesystemQueryJson, FilesystemResultJson, FilesystemSearchResponse,
};
use crate::search::{
    ContentQuery, ContentResult, FilesystemQuery, FilesystemResult, search_content,
    search_filesystem,
};

const DEFAULT_LIMIT: usize = 100;
const DEFAULT_MAX_MATCHES_PER_FILE: usize = 5;

pub(crate) async fn filesystem(
    State(s): State<ApiState>,
    ApiJson(req): ApiJson<FilesystemQueryJson>,
) -> Result<Json<FilesystemSearchResponse>, ApiError> {
    let q = FilesystemQuery {
        prefix: req.prefix,
        glob: req.glob,
        max_depth: req.max_depth,
        limit: req.limit.unwrap_or(DEFAULT_LIMIT),
    };
    let (rows, truncated) = search_filesystem(s.pool.clone(), q).await?;
    let results = rows.into_iter().map(filesystem_to_json).collect();
    Ok(Json(FilesystemSearchResponse { results, truncated }))
}

pub(crate) async fn content(
    State(s): State<ApiState>,
    ApiJson(req): ApiJson<ContentQueryJson>,
) -> Result<Json<ContentSearchResponse>, ApiError> {
    let q = ContentQuery {
        query: req.query,
        regex: req.regex,
        case_sensitive: req.case_sensitive,
        prefix: req.prefix,
        include_matches: req.include_matches,
        max_matches_per_file: req
            .max_matches_per_file
            .unwrap_or(DEFAULT_MAX_MATCHES_PER_FILE),
        limit: req.limit.unwrap_or(DEFAULT_LIMIT),
    };
    let (rows, truncated) = search_content(s.pool.clone(), q).await?;
    let results = rows.into_iter().map(content_to_json).collect();
    Ok(Json(ContentSearchResponse { results, truncated }))
}

fn filesystem_to_json(r: FilesystemResult) -> FilesystemResultJson {
    FilesystemResultJson {
        path: r.path,
        size: r.size,
        mtime: r.mtime,
        content_hash: r.content_hash,
        vault: None,
    }
}

fn content_to_json(r: ContentResult) -> ContentResultJson {
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
        vault: None,
    }
}
