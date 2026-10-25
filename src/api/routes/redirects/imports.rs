//! Multipart CSV upload handling and OpenAPI schemas for bulk redirect imports.

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Multipart},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
};
use serde::Serialize;

use crate::{
    api::request::{ApiError, ApiState, EditorOrAdmin},
    utils::imports::redirects as redirect_imports,
};

/// Maximum multipart request size in bytes, including file metadata and boundaries.
const MAX_UPLOAD_SIZE: usize = 10 * 1024 * 1024;

/// Describes the file field read manually from the multipart upload.
#[derive(utoipa::ToSchema)]
#[allow(dead_code)] // This schema documents Multipart; it is not deserialized into a Rust value.
struct CsvUpload {
    /// CSV file containing the redirect rows.
    #[schema(value_type = String, format = Binary)]
    file: Vec<u8>,
}

/// The source row, optional slug, and reason a redirect was skipped during import.
#[derive(Serialize, utoipa::ToSchema)]
struct ImportRowErrorResponse {
    /// One-based data row number, excluding the CSV header.
    row: usize,
    /// Slug supplied by the row, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    slug: Option<String>,
    /// Reason this row was skipped.
    message: String,
}

/// CSV import totals and validation errors, including partial-success outcomes.
#[derive(Serialize, utoipa::ToSchema)]
struct ImportLinksResponse {
    /// Summary of the import outcome.
    message: String,
    /// Import format, currently csv.
    #[schema(example = "csv")]
    format: &'static str,
    /// Total number of data rows parsed.
    total_rows: usize,
    /// Number of redirects created.
    imported: usize,
    /// Number of rows skipped, equal to the length of errors.
    skipped: usize,
    /// Validation and duplicate-slug errors for skipped rows.
    errors: Vec<ImportRowErrorResponse>,
}

/// Registers the CSV upload route with a 10 MiB multipart body limit.
pub fn routes() -> Router<ApiState> {
    Router::new()
        .route("/", post(import_links))
        .layer(DefaultBodyLimit::max(MAX_UPLOAD_SIZE))
}

/// Import redirects from CSV.
///
/// Requires editor or admin access. Upload one file field containing a nonempty CSV with required
/// slug and destination columns and optional redirect_code and is_enabled columns. The multipart
/// body is limited to 10 MiB and the CSV to 10,000 data rows. Valid rows are imported; invalid rows
/// and duplicate slugs are skipped. Error row numbers start at 1 after the header.
///
/// # Errors
///
/// Returns HTTP 400 for an invalid upload or malformed CSV, HTTP 413 for more than 10,000 data rows, or HTTP 500 if importing valid rows fails. Row-level validation failures are reported in the normal import response.
#[utoipa::path(
    post, path = "/api/v1/redirects/import", tag = "Imports",
    request_body(content = CsvUpload, content_type = "multipart/form-data"),
    security(("bearer_auth" = [])),
    responses(
        (status = 201, description = "At least one redirect was imported; inspect errors for skipped rows", body = ImportLinksResponse),
        (status = 422, description = "No redirects were imported; inspect errors for each skipped row", body = ImportLinksResponse),
        (status = 400, description = "Missing or invalid CSV upload, malformed multipart request, or upload exceeding 10 MiB", content((crate::api::request::ErrorResponse = "application/json"), (String = "text/plain"))),
        (status = 413, description = "CSV exceeds 10,000 data rows", body = crate::api::request::ErrorResponse),
        (status = 401, description = "Missing, invalid, expired, or revoked bearer token", body = crate::api::request::ErrorResponse),
        (status = 403, description = "Insufficient permissions", body = crate::api::request::ErrorResponse),
        (status = 500, description = "Database or server error", body = crate::api::request::ErrorResponse)
    )
)]
async fn import_links(
    EditorOrAdmin(auth): EditorOrAdmin,
    multipart: Multipart,
) -> Result<Response, ApiError> {
    let file = read_csv_file(multipart).await?;
    let mut conn = auth.conn;
    let user_id = auth.user_id;
    let outcome = tokio::task::spawn_blocking(move || {
        redirect_imports::import_csv(&mut conn, user_id, &file)
    })
    .await
    .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "Database error"))?
    .map_err(import_error_from_domain)?;

    let status = if outcome.imported > 0 {
        StatusCode::CREATED
    } else {
        StatusCode::UNPROCESSABLE_ENTITY
    };

    let message = if outcome.errors.is_empty() {
        "All redirects imported successfully"
    } else if outcome.imported > 0 {
        "Import completed with skipped rows"
    } else {
        "No redirects were imported"
    };

    let skipped = outcome.errors.len();
    let errors = outcome
        .errors
        .into_iter()
        .map(|error| ImportRowErrorResponse {
            row: error.row,
            slug: error.slug,
            message: error.message,
        })
        .collect();

    Ok((
        status,
        Json(ImportLinksResponse {
            message: message.to_string(),
            format: "csv",
            total_rows: outcome.total_rows,
            imported: outcome.imported,
            skipped,
            errors,
        }),
    )
        .into_response())
}

/// Reads the first multipart field named `file`, accepting a nonempty upload with an optional `.csv` filename.
///
/// # Errors
///
/// Returns HTTP 400 if the multipart upload cannot be read, the file field is missing or empty, or its supplied filename is not CSV.
async fn read_csv_file(mut multipart: Multipart) -> Result<Vec<u8>, ApiError> {
    while let Some(field) = multipart.next_field().await.map_err(|error| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("Failed to read multipart upload: {error}"),
        )
    })? {
        if field.name() != Some("file") {
            continue;
        }

        // The extension provides a clear early error; the importer still validates the contents.
        if let Some(filename) = field.file_name()
            && !filename.to_ascii_lowercase().ends_with(".csv")
        {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "Only CSV files are supported",
            ));
        }

        let bytes = field.bytes().await.map_err(|error| {
            ApiError::new(
                StatusCode::BAD_REQUEST,
                format!("Failed to read import file: {error}"),
            )
        })?;
        if bytes.is_empty() {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "Import file cannot be empty",
            ));
        }
        return Ok(bytes.to_vec());
    }

    Err(ApiError::new(
        StatusCode::BAD_REQUEST,
        "A CSV file field is required",
    ))
}

/// Maps CSV-wide import errors to HTTP 400, row-limit errors to HTTP 413, and database errors to HTTP 500.
fn import_error_from_domain(error: redirect_imports::ImportError) -> ApiError {
    let status = if error.is_payload_too_large() {
        StatusCode::PAYLOAD_TOO_LARGE
    } else if error.is_database_error() {
        StatusCode::INTERNAL_SERVER_ERROR
    } else {
        StatusCode::BAD_REQUEST
    };
    ApiError::new(status, error.to_string())
}

/// OpenAPI operations and schemas generated from this module's route annotations.
#[derive(utoipa::OpenApi)]
#[openapi(paths(import_links))]
pub(super) struct ApiDoc;
