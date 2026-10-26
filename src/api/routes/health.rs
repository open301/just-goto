//! Public health endpoint that does not require a database connection.

use crate::api::request::ApiState;
use axum::{Json, Router, http::StatusCode, response::IntoResponse, routing::get};
use serde::Serialize;

/// Public response confirming that the HTTP service is running.
#[derive(Serialize, utoipa::ToSchema)]
struct HealthResponse {
    /// Human-readable outcome of the operation.
    message: String,
}

/// Registers the public health check beneath the health prefix.
pub fn routes() -> Router<ApiState> {
    Router::new().route("/", get(health))
}

/// Check service health.
///
/// Does not require authentication or access the database.
#[utoipa::path(
    get, path = "/health", tag = "Service",
    responses(
        (status = 200, description = "Service is healthy", body = HealthResponse)
    )
)]
async fn health() -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(HealthResponse {
            message: "Service is healthy".to_string(),
        }),
    )
}

/// OpenAPI operations and schemas generated from this module's route annotations.
#[derive(utoipa::OpenApi)]
#[openapi(paths(health))]
pub(in crate::api) struct ApiDoc;
