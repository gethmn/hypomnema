use std::path::PathBuf;

use axum::Json;
use axum::extract::{Path, State};

use super::ApiState;
use super::error::{ApiError, ApiJson};
use super::types::{
    CreateVaultRequest, RenameRequest, RescanResponseJson, ResetRequest, TerminateVaultResponse,
    VaultListResponse, VaultRowJson,
};
use crate::control_plane::CreateVaultRequest as ControlCreateRequest;

pub(crate) async fn create(
    State(s): State<ApiState>,
    ApiJson(req): ApiJson<CreateVaultRequest>,
) -> Result<Json<VaultRowJson>, ApiError> {
    let row = s
        .vault_manager
        .create(ControlCreateRequest {
            name: req.name,
            path: PathBuf::from(req.path),
        })
        .await?;
    Ok(Json(VaultRowJson::from(row)))
}

pub(crate) async fn list(State(s): State<ApiState>) -> Result<Json<VaultListResponse>, ApiError> {
    let rows = s.vault_manager.list().await?;
    Ok(Json(VaultListResponse {
        vaults: rows.into_iter().map(VaultRowJson::from).collect(),
    }))
}

pub(crate) async fn get(
    State(s): State<ApiState>,
    Path(name_or_id): Path<String>,
) -> Result<Json<VaultRowJson>, ApiError> {
    let row = s.vault_manager.get(&name_or_id).await?;
    Ok(Json(VaultRowJson::from(row)))
}

pub(crate) async fn terminate(
    State(s): State<ApiState>,
    Path(name_or_id): Path<String>,
) -> Result<Json<TerminateVaultResponse>, ApiError> {
    // Pre-resolve so the success response carries the canonical UUID even when
    // the caller addressed the vault by name. The TOCTOU window between this
    // resolve and the terminate is benign — a concurrent terminate that wins
    // turns ours into a 404 via the inner `terminate`'s own resolve.
    let id = s.vault_manager.resolve(&name_or_id)?;
    s.vault_manager.terminate(&name_or_id).await?;
    Ok(Json(TerminateVaultResponse {
        terminated: true,
        id: id.to_string(),
    }))
}

pub(crate) async fn pause(
    State(s): State<ApiState>,
    Path(name_or_id): Path<String>,
) -> Result<Json<VaultRowJson>, ApiError> {
    let row = s.vault_manager.pause(&name_or_id).await?;
    Ok(Json(VaultRowJson::from(row)))
}

pub(crate) async fn resume(
    State(s): State<ApiState>,
    Path(name_or_id): Path<String>,
) -> Result<Json<VaultRowJson>, ApiError> {
    let row = s.vault_manager.resume(&name_or_id).await?;
    Ok(Json(VaultRowJson::from(row)))
}

pub(crate) async fn reset(
    State(s): State<ApiState>,
    Path(name_or_id): Path<String>,
    body: Option<axum::Json<ResetRequest>>,
) -> Result<Json<VaultRowJson>, ApiError> {
    let rebuild = body.map(|axum::Json(b)| b.rebuild).unwrap_or(false);
    let row = s.vault_manager.reset(&name_or_id, rebuild).await?;
    Ok(Json(VaultRowJson::from(row)))
}

pub(crate) async fn rename(
    State(s): State<ApiState>,
    Path(name_or_id): Path<String>,
    ApiJson(req): ApiJson<RenameRequest>,
) -> Result<Json<VaultRowJson>, ApiError> {
    let row = s.vault_manager.rename(&name_or_id, &req.new_name).await?;
    Ok(Json(VaultRowJson::from(row)))
}

pub(crate) async fn rescan(
    State(s): State<ApiState>,
    Path(name_or_id): Path<String>,
) -> Result<Json<RescanResponseJson>, ApiError> {
    let resp = s.vault_manager.rescan(&name_or_id).await?;
    Ok(Json(RescanResponseJson::from(resp)))
}
