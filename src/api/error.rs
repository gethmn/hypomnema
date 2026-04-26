use axum::Json;
use axum::async_trait;
use axum::extract::{FromRequest, Request};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::de::DeserializeOwned;

use super::types::{ErrorBody, ErrorEnvelope};
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

#[async_trait]
impl<S, T> FromRequest<S> for ApiJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match Json::<T>::from_request(req, state).await {
            Ok(Json(value)) => Ok(ApiJson(value)),
            Err(rej) => Err(ApiError::invalid_request(rej.body_text())),
        }
    }
}
