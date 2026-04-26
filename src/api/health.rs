use axum::{Json, response::IntoResponse};

use super::types::HealthResponse;

pub(crate) async fn health() -> impl IntoResponse {
    Json(HealthResponse {
        status: "ok".to_string(),
    })
}
