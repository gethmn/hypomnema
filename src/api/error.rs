use axum::Json;
use axum::extract::{FromRequest, OptionalFromRequest, Request};
use axum::http::StatusCode;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use serde::de::DeserializeOwned;

use super::types::{ErrorBody, ErrorEnvelope};
use crate::control_plane::ControlPlaneError;
use crate::search::SemanticSearchError;

pub(crate) struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl ApiError {
    pub(crate) fn invalid_request(message: impl Into<String>) -> Self {
        ApiError {
            status: StatusCode::BAD_REQUEST,
            code: "invalid_request",
            message: message.into(),
        }
    }

    fn internal() -> Self {
        ApiError {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internal",
            message: "internal server error".to_string(),
        }
    }

    pub(crate) fn vault_not_found(name_or_id: &str, hint: Option<&str>) -> Self {
        let message = match hint {
            Some(h) => format!("vault {name_or_id} not found (did you mean {h}?)"),
            None => format!("vault {name_or_id} not found"),
        };
        ApiError {
            status: StatusCode::NOT_FOUND,
            code: "vault_not_found",
            message,
        }
    }

    pub(crate) fn vault_path_conflict(message: impl Into<String>) -> Self {
        ApiError {
            status: StatusCode::CONFLICT,
            code: "vault_path_conflict",
            message: message.into(),
        }
    }

    pub(crate) fn vault_name_conflict(message: impl Into<String>) -> Self {
        ApiError {
            status: StatusCode::CONFLICT,
            code: "vault_name_conflict",
            message: message.into(),
        }
    }

    pub(crate) fn vault_path_invalid(message: impl Into<String>) -> Self {
        ApiError {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "vault_path_invalid",
            message: message.into(),
        }
    }

    pub(crate) fn vault_errored(message: impl Into<String>) -> Self {
        ApiError {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code: "vault_errored",
            message: message.into(),
        }
    }

    pub(crate) fn vault_not_active(message: impl Into<String>) -> Self {
        ApiError {
            status: StatusCode::CONFLICT,
            code: "vault_not_active",
            message: message.into(),
        }
    }

    pub(crate) fn vault_op_conflict(message: impl Into<String>) -> Self {
        ApiError {
            status: StatusCode::CONFLICT,
            code: "vault_op_conflict",
            message: message.into(),
        }
    }

    pub(crate) fn registry_corrupt() -> Self {
        ApiError {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "registry_corrupt",
            message: "vault registry is corrupt; restore from backup".to_string(),
        }
    }
}

impl From<ControlPlaneError> for ApiError {
    fn from(err: ControlPlaneError) -> Self {
        match err {
            ControlPlaneError::VaultNotFound { name_or_id, hint } => {
                ApiError::vault_not_found(&name_or_id, hint.as_deref())
            }
            ControlPlaneError::VaultPathConflict {
                existing_name,
                path,
            } => ApiError::vault_path_conflict(format!(
                "path {} is already registered as vault {}",
                path.display(),
                existing_name
            )),
            ControlPlaneError::VaultNameConflict {
                existing_path,
                name,
            } => ApiError::vault_name_conflict(format!(
                "name {} is already in use by vault at {}",
                name,
                existing_path.display()
            )),
            ControlPlaneError::VaultPathInvalid { detail } => ApiError::vault_path_invalid(detail),
            ControlPlaneError::VaultErrored {
                name_or_id,
                last_error,
            } => {
                let message = match last_error {
                    Some(e) => format!("vault {name_or_id} is errored: {e}"),
                    None => format!("vault {name_or_id} is errored"),
                };
                ApiError::vault_errored(message)
            }
            ControlPlaneError::VaultOpConflict { detail } => ApiError::vault_op_conflict(detail),
            ControlPlaneError::RegistryCorrupt { detail } => {
                tracing::error!(detail = %detail, "vault registry is corrupt");
                ApiError::registry_corrupt()
            }
            ControlPlaneError::Internal(e) => {
                tracing::error!(error = ?e, "internal API error from control plane");
                ApiError::internal()
            }
        }
    }
}

impl From<SemanticSearchError> for ApiError {
    fn from(err: SemanticSearchError) -> Self {
        match err {
            SemanticSearchError::EmbeddingUnavailable { detail } => ApiError {
                status: StatusCode::SERVICE_UNAVAILABLE,
                code: "embedding_unavailable",
                message: detail,
            },
            SemanticSearchError::InvalidPrefix(detail) => ApiError {
                status: StatusCode::BAD_REQUEST,
                code: "invalid_prefix",
                message: detail,
            },
            SemanticSearchError::Internal(e) => {
                tracing::error!(error = ?e, "internal API error from semantic search");
                ApiError::internal()
            }
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = ErrorEnvelope {
            error: ErrorBody {
                code: self.code.to_string(),
                message: self.message,
            },
        };
        (self.status, Json(body)).into_response()
    }
}

// Mirrors the wire-shape that `DaemonClient::decode_response` produces for
// failed HTTP calls (`anyhow!("{code}: {message}")`). This lets the
// in-process backend surface the same anyhow display that
// `mcp::server::envelope_from_anyhow` already knows how to split into a
// structured envelope, so HTTP and in-process backends route identical
// `{code, message}` payloads to MCP hosts.
impl From<ApiError> for anyhow::Error {
    fn from(err: ApiError) -> Self {
        anyhow::anyhow!("{}: {}", err.code, err.message)
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(err: anyhow::Error) -> Self {
        // The query layer encodes user-facing failure modes as anyhow chains
        // whose Display starts with a stable lowercase token followed by
        // ": <detail>". Match that prefix to map to the wire-level error
        // code; everything else is an internal error.
        let display = format!("{err:#}");
        if let Some(rest) = display.strip_prefix("invalid_glob") {
            return ApiError {
                status: StatusCode::BAD_REQUEST,
                code: "invalid_glob",
                message: trim_token_detail(rest),
            };
        }
        if let Some(rest) = display.strip_prefix("invalid_regex") {
            return ApiError {
                status: StatusCode::BAD_REQUEST,
                code: "invalid_regex",
                message: trim_token_detail(rest),
            };
        }
        if let Some(rest) = display.strip_prefix("invalid_prefix") {
            return ApiError {
                status: StatusCode::BAD_REQUEST,
                code: "invalid_prefix",
                message: trim_token_detail(rest),
            };
        }
        tracing::error!(error = ?err, "internal API error");
        ApiError::internal()
    }
}

fn trim_token_detail(rest: &str) -> String {
    rest.trim_start_matches(':').trim().to_string()
}

// JSON body extractor whose rejection is `ApiError`. Wraps `axum::Json` and
// converts deserialization / content-type / payload errors into the project's
// error envelope (`code = "invalid_request"`, 400) instead of axum's default
// plain-text response.
pub(crate) struct ApiJson<T>(pub T);

impl<S, T> FromRequest<S> for ApiJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match <Json<T> as FromRequest<S>>::from_request(req, state).await {
            Ok(Json(value)) => Ok(ApiJson(value)),
            Err(rej) => Err(ApiError::invalid_request(rej.body_text())),
        }
    }
}

impl<S, T> OptionalFromRequest<S> for ApiJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Option<Self>, Self::Rejection> {
        if req.headers().get(header::CONTENT_TYPE).is_some() {
            match <Json<T> as FromRequest<S>>::from_request(req, state).await {
                Ok(Json(value)) => Ok(Some(ApiJson(value))),
                Err(rej) => Err(ApiError::invalid_request(rej.body_text())),
            }
        } else {
            Ok(None)
        }
    }
}
