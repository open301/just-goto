//! Authenticated redirect management and public slug resolution endpoints.

use axum::{
    Json, Router,
    body::Body,
    extract::{Path, Query},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use serde::{Deserialize, Serialize};

use crate::{
    api::request::{ApiError, ApiState, Authenticated, Database, EditorOrAdmin},
    db::tables,
    utils::redirects::{
        DEFAULT_REDIRECT_CODE, validate_destination, validate_redirect_code, validate_slug,
    },
};

mod imports;

/// Optional pagination and filters for listing redirects.
#[derive(Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
struct ListRedirectsQuery {
    /// Page size. Defaults to 50; values are clamped to 1–100.
    #[param(default = 50)]
    limit: Option<u32>,
    /// Number of redirects to skip. Defaults to 0.
    #[param(default = 0)]
    offset: Option<u32>,
    /// Substring matched against slugs using SQLite LIKE.
    slug: Option<String>,
    /// Filter by enabled or disabled state.
    enabled: Option<bool>,
    /// Filter by the creator user ID.
    created_by: Option<i64>,
}

/// Required redirect values and optional creation defaults.
#[derive(Deserialize, utoipa::ToSchema)]
struct CreateRedirectRequest {
    /// Unique ASCII slug of 1–128 letters, digits, hyphens, or underscores. Reserved: health, auth, users, redirects, swagger-ui, assets, api, api-docs.
    #[schema(
        example = "example",
        min_length = 1,
        max_length = 128,
        pattern = "^[A-Za-z0-9_-]+$"
    )]
    slug: String,
    /// Absolute HTTP or HTTPS destination, at most 2048 bytes.
    #[schema(example = "https://example.com")]
    destination: String,
    /// HTTP redirect status: 301, 302, 307, or 308. Defaults to 302.
    #[schema(example = 302, default = 302)]
    redirect_code: Option<i64>,
    /// Whether the public redirect resolves. Defaults to true.
    #[schema(example = true, default = true)]
    is_enabled: Option<bool>,
}

/// Optional redirect fields supplied in a partial update.
#[derive(Deserialize, utoipa::ToSchema)]
struct UpdateRedirectRequest {
    /// Replacement slug, with the same constraints and reserved names as creation.
    #[schema(
        example = "example",
        min_length = 1,
        max_length = 128,
        pattern = "^[A-Za-z0-9_-]+$"
    )]
    slug: Option<String>,
    /// Replacement absolute HTTP or HTTPS destination, at most 2048 bytes.
    #[schema(example = "https://example.com")]
    destination: Option<String>,
    /// Replacement HTTP redirect status: 301, 302, 307, or 308.
    #[schema(example = 302)]
    redirect_code: Option<i64>,
    /// Whether the public redirect resolves.
    #[schema(example = true)]
    is_enabled: Option<bool>,
}

/// Redirect details exposed by authenticated management endpoints.
#[derive(Serialize, utoipa::ToSchema)]
struct RedirectResponse {
    /// Redirect ID.
    id: i64,
    /// Public redirect slug.
    slug: String,
    /// HTTP or HTTPS destination.
    destination: String,
    /// Creator user ID, or null if that user was deleted.
    created_by: Option<i64>,
    /// Creation as Unix time in seconds.
    created_at: i64,
    /// Last update as Unix time in seconds.
    updated_at: i64,
    /// Whether the public redirect resolves.
    is_enabled: bool,
    /// HTTP status used when resolving the redirect.
    redirect_code: i64,
}

/// A redirect listing and its human-readable outcome.
#[derive(Serialize, utoipa::ToSchema)]
struct ListRedirectsResponse {
    /// Human-readable outcome of the operation.
    message: String,
    /// Redirects matching the requested filters; empty when the listing fails.
    redirects: Vec<RedirectResponse>,
}

/// Creation result containing the newly stored redirect when available.
#[derive(Serialize, utoipa::ToSchema)]
struct CreateRedirectResponse {
    /// Human-readable outcome of the operation.
    message: String,
    /// Requested redirect; omitted when no redirect details are available.
    #[serde(skip_serializing_if = "Option::is_none")]
    redirect: Option<RedirectResponse>,
}

/// Lookup result containing the requested redirect when available.
#[derive(Serialize, utoipa::ToSchema)]
struct GetRedirectResponse {
    /// Human-readable outcome of the operation.
    message: String,
    /// Requested redirect; omitted when no redirect details are available.
    #[serde(skip_serializing_if = "Option::is_none")]
    redirect: Option<RedirectResponse>,
}

/// Update result containing the stored redirect after changes.
#[derive(Serialize, utoipa::ToSchema)]
struct UpdateRedirectResponse {
    /// Human-readable outcome of the operation.
    message: String,
    /// Requested redirect; omitted when no redirect details are available.
    #[serde(skip_serializing_if = "Option::is_none")]
    redirect: Option<RedirectResponse>,
}

/// Confirmation that the requested redirect was deleted.
#[derive(Serialize, utoipa::ToSchema)]
struct DeleteRedirectResponse {
    /// Human-readable outcome of the operation.
    message: String,
}

/// Registers redirect collection, individual redirect, and CSV import routes beneath the redirects prefix.
pub fn routes() -> Router<ApiState> {
    Router::new()
        .route("/", get(list_redirects).post(create_redirect))
        .route(
            "/{id}",
            get(get_redirect)
                .patch(update_redirect)
                .delete(delete_redirect),
        )
        .nest("/import", imports::routes())
}

/// Resolve a public redirect.
///
/// No authentication is required. Only enabled redirects are resolved. The response has an empty
/// body and a Location header.
///
/// # Errors
///
/// Returns HTTP 404 for a missing or disabled slug, or HTTP 500 for a database failure, invalid stored redirect status, or invalid Location header.
#[utoipa::path(
    get, path = "/{slug}", tag = "Redirects",
    params(("slug" = String, Path, description = "Enabled redirect slug")),
    responses(
        (status = 301, description = "Moved permanently", headers(("Location" = String, description = "Redirect destination"))),
        (status = 302, description = "Found", headers(("Location" = String, description = "Redirect destination"))),
        (status = 307, description = "Temporary redirect", headers(("Location" = String, description = "Redirect destination"))),
        (status = 308, description = "Permanent redirect", headers(("Location" = String, description = "Redirect destination"))),
        (status = 404, description = "Redirect not found or disabled", body = crate::api::request::ErrorResponse),
        (status = 500, description = "Database or server error", body = crate::api::request::ErrorResponse)
    )
)]
pub async fn resolve_redirect(
    Database(conn): Database,
    Path(slug): Path<String>,
) -> Result<Response, ApiError> {
    let redirect = tables::redirects::get_enabled_redirect_by_slug(&conn, &slug)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "Redirect not found"))?;
    let status = StatusCode::from_u16(redirect.redirect_code as u16)
        .ok()
        .filter(StatusCode::is_redirection)
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Redirect has an invalid status code",
            )
        })?;

    Response::builder()
        .status(status)
        .header(header::LOCATION, redirect.destination)
        .body(Body::empty())
        .map_err(|_| {
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Redirect has an invalid destination",
            )
        })
}

/// Delete a redirect.
///
/// Requires editor or admin access.
///
/// # Errors
///
/// Returns HTTP 404 when the redirect does not exist, or HTTP 500 if deletion fails.
#[utoipa::path(
    delete, path = "/api/v1/redirects/{id}", tag = "Redirects",
    params(("id" = i64, Path, description = "Redirect ID")),
    security(("bearer_auth" = [])),
    responses(
        (status = 400, description = "Invalid path or query parameters", body = String, content_type = "text/plain"),
        (status = 200, description = "Redirect deleted successfully", body = DeleteRedirectResponse),
        (status = 404, description = "Redirect not found", body = crate::api::request::ErrorResponse),
        (status = 401, description = "Missing, invalid, expired, or revoked bearer token", body = crate::api::request::ErrorResponse),
        (status = 403, description = "Insufficient permissions", body = crate::api::request::ErrorResponse),
        (status = 500, description = "Database or server error", body = crate::api::request::ErrorResponse)
    )
)]
async fn delete_redirect(
    EditorOrAdmin(auth): EditorOrAdmin,
    Path(redirect_id): Path<i64>,
) -> Result<impl IntoResponse, ApiError> {
    let deleted = tables::redirects::delete_redirect(&auth.conn, redirect_id).map_err(|_| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to delete redirect",
        )
    })?;
    if !deleted {
        return Err(ApiError::new(StatusCode::NOT_FOUND, "Redirect not found"));
    }
    Ok((
        StatusCode::OK,
        Json(DeleteRedirectResponse {
            message: "Redirect deleted successfully".to_string(),
        }),
    ))
}

/// Update a redirect.
///
/// Requires editor or admin access and at least one field.
///
/// # Errors
///
/// Returns HTTP 400 for missing or invalid fields, HTTP 404 for an unknown redirect, HTTP 409 for an occupied slug, or HTTP 500 if a database operation fails.
#[utoipa::path(
    patch, path = "/api/v1/redirects/{id}", tag = "Redirects",
    request_body = UpdateRedirectRequest,
    params(("id" = i64, Path, description = "Redirect ID")),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Redirect updated successfully", body = UpdateRedirectResponse),
        (status = 400, description = "Invalid or missing update fields", content((crate::api::request::ErrorResponse = "application/json"), (String = "text/plain"))),
        (status = 404, description = "Redirect not found", body = crate::api::request::ErrorResponse),
        (status = 409, description = "Slug already exists", body = crate::api::request::ErrorResponse),
        (status = 401, description = "Missing, invalid, expired, or revoked bearer token", body = crate::api::request::ErrorResponse),
        (status = 403, description = "Insufficient permissions", body = crate::api::request::ErrorResponse),
        (status = 500, description = "Database or server error", body = crate::api::request::ErrorResponse),
        (status = 413, description = "Request body exceeds the 2 MiB limit", body = String, content_type = "text/plain"),
        (status = 415, description = "Expected application/json content type", body = String, content_type = "text/plain"),
        (status = 422, description = "JSON does not match the request schema", body = String, content_type = "text/plain")
    )
)]
async fn update_redirect(
    EditorOrAdmin(auth): EditorOrAdmin,
    Path(redirect_id): Path<i64>,
    Json(payload): Json<UpdateRedirectRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = auth.conn;
    tables::redirects::get_redirect_by_id(&conn, redirect_id)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "Redirect not found"))?;

    if payload.slug.is_none()
        && payload.destination.is_none()
        && payload.redirect_code.is_none()
        && payload.is_enabled.is_none()
    {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "At least one field is required",
        ));
    }

    let slug = payload.slug.as_deref().map(str::trim);
    if let Some(slug) = slug {
        validate_slug(slug).map_err(|message| ApiError::new(StatusCode::BAD_REQUEST, message))?;
        if tables::redirects::check_slug_exists_for_other_redirect(&conn, slug, redirect_id)? {
            return Err(ApiError::new(StatusCode::CONFLICT, "Slug already exists"));
        }
    }
    let destination = payload.destination.as_deref();
    if let Some(destination) = destination {
        validate_destination(destination)
            .map_err(|message| ApiError::new(StatusCode::BAD_REQUEST, message))?;
    }
    if let Some(code) = payload.redirect_code {
        validate_redirect_code(code)
            .map_err(|message| ApiError::new(StatusCode::BAD_REQUEST, message))?;
    }

    let updated = tables::redirects::update_redirect(
        &conn,
        redirect_id,
        slug,
        destination,
        payload.redirect_code,
        payload.is_enabled,
    )
    .map_err(|_| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to update redirect",
        )
    })?;
    if !updated {
        return Err(ApiError::new(StatusCode::NOT_FOUND, "Redirect not found"));
    }
    let retrieval_error = || {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to retrieve updated redirect",
        )
    };
    let redirect = tables::redirects::get_redirect_by_id(&conn, redirect_id)
        .map_err(|_| retrieval_error())?
        .ok_or_else(retrieval_error)?;

    Ok((
        StatusCode::OK,
        Json(UpdateRedirectResponse {
            message: "Redirect updated successfully".to_string(),
            redirect: Some(redirect_to_response(redirect)),
        }),
    ))
}

/// Get a redirect.
///
/// Available to any authenticated user, including for disabled redirects.
///
/// # Errors
///
/// Returns HTTP 404 for an unknown redirect, or HTTP 500 if its lookup fails.
#[utoipa::path(
    get, path = "/api/v1/redirects/{id}", tag = "Redirects",
    params(("id" = i64, Path, description = "Redirect ID")),
    security(("bearer_auth" = [])),
    responses(
        (status = 400, description = "Invalid path or query parameters", body = String, content_type = "text/plain"),
        (status = 200, description = "Redirect retrieved successfully", body = GetRedirectResponse),
        (status = 404, description = "Redirect not found", body = crate::api::request::ErrorResponse),
        (status = 401, description = "Missing, invalid, expired, or revoked bearer token", body = crate::api::request::ErrorResponse),
        (status = 500, description = "Database or server error", body = crate::api::request::ErrorResponse)
    )
)]
async fn get_redirect(
    auth: Authenticated,
    Path(redirect_id): Path<i64>,
) -> Result<impl IntoResponse, ApiError> {
    let redirect = tables::redirects::get_redirect_by_id(&auth.conn, redirect_id)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "Redirect not found"))?;
    Ok((
        StatusCode::OK,
        Json(GetRedirectResponse {
            message: "Redirect retrieved successfully".to_string(),
            redirect: Some(redirect_to_response(redirect)),
        }),
    ))
}

/// Create a redirect.
///
/// Requires editor or admin access. Records the authenticated user as the creator.
///
/// # Errors
///
/// Returns HTTP 400 for invalid redirect fields, HTTP 409 for an occupied slug, or HTTP 500 if creating or retrieving the redirect fails.
#[utoipa::path(
    post, path = "/api/v1/redirects", tag = "Redirects",
    request_body = CreateRedirectRequest,
    security(("bearer_auth" = [])),
    responses(
        (status = 201, description = "Redirect created successfully", body = CreateRedirectResponse),
        (status = 400, description = "Invalid slug, destination, or redirect code", content((crate::api::request::ErrorResponse = "application/json"), (String = "text/plain"))),
        (status = 409, description = "Slug already exists", body = crate::api::request::ErrorResponse),
        (status = 401, description = "Missing, invalid, expired, or revoked bearer token", body = crate::api::request::ErrorResponse),
        (status = 403, description = "Insufficient permissions", body = crate::api::request::ErrorResponse),
        (status = 500, description = "Database or server error", body = crate::api::request::ErrorResponse),
        (status = 413, description = "Request body exceeds the 2 MiB limit", body = String, content_type = "text/plain"),
        (status = 415, description = "Expected application/json content type", body = String, content_type = "text/plain"),
        (status = 422, description = "JSON does not match the request schema", body = String, content_type = "text/plain")
    )
)]
async fn create_redirect(
    EditorOrAdmin(auth): EditorOrAdmin,
    Json(payload): Json<CreateRedirectRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = auth.conn;
    let slug = payload.slug.trim();
    validate_slug(slug).map_err(|message| ApiError::new(StatusCode::BAD_REQUEST, message))?;
    validate_destination(&payload.destination)
        .map_err(|message| ApiError::new(StatusCode::BAD_REQUEST, message))?;
    let redirect_code = payload.redirect_code.unwrap_or(DEFAULT_REDIRECT_CODE);
    validate_redirect_code(redirect_code)
        .map_err(|message| ApiError::new(StatusCode::BAD_REQUEST, message))?;
    if tables::redirects::check_slug_exists(&conn, slug)? {
        return Err(ApiError::new(StatusCode::CONFLICT, "Slug already exists"));
    }

    let redirect_id = tables::redirects::create_redirect(
        &conn,
        slug,
        &payload.destination,
        auth.user_id,
        payload.is_enabled.unwrap_or(true),
        redirect_code,
    )
    .map_err(|_| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to create redirect",
        )
    })?;
    let retrieval_error = || {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to retrieve created redirect",
        )
    };
    let redirect = tables::redirects::get_redirect_by_id(&conn, redirect_id)
        .map_err(|_| retrieval_error())?
        .ok_or_else(retrieval_error)?;

    Ok((
        StatusCode::CREATED,
        Json(CreateRedirectResponse {
            message: "Redirect created successfully".to_string(),
            redirect: Some(redirect_to_response(redirect)),
        }),
    ))
}

/// Converts a stored redirect into its API representation without exposing database-specific types.
fn redirect_to_response(redirect: tables::redirects::Redirect) -> RedirectResponse {
    RedirectResponse {
        id: redirect.id,
        slug: redirect.slug,
        destination: redirect.destination,
        created_by: redirect.created_by,
        created_at: redirect.created_at,
        updated_at: redirect.updated_at,
        is_enabled: redirect.is_enabled,
        redirect_code: redirect.redirect_code,
    }
}

/// List redirects.
///
/// Available to any authenticated user. Results are ordered by redirect ID.
#[utoipa::path(
    get, path = "/api/v1/redirects", tag = "Redirects",
    params(ListRedirectsQuery),
    security(("bearer_auth" = [])),
    responses(
        (status = 400, description = "Invalid path or query parameters", body = String, content_type = "text/plain"),
        (status = 200, description = "Redirects retrieved successfully", body = ListRedirectsResponse),
        (status = 401, description = "Missing, invalid, expired, or revoked bearer token", body = crate::api::request::ErrorResponse),
        (status = 500, description = "Database or server error", body = crate::api::request::ErrorResponse)
    )
)]
async fn list_redirects(
    auth: Authenticated,
    Query(query): Query<ListRedirectsQuery>,
) -> impl IntoResponse {
    let conn = auth.conn;

    let limit = query.limit.unwrap_or(50).clamp(1, 100) as i64;
    let offset = query.offset.unwrap_or(0) as i64;
    let redirects = match tables::redirects::list_redirects(
        &conn,
        query.slug.as_deref(),
        query.enabled,
        query.created_by,
        limit,
        offset,
    ) {
        Ok(redirects) => redirects,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ListRedirectsResponse {
                    message: "Database error".to_string(),
                    redirects: Vec::new(),
                }),
            );
        },
    };

    let redirects = redirects.into_iter().map(redirect_to_response).collect();

    (
        StatusCode::OK,
        Json(ListRedirectsResponse {
            message: "Redirects retrieved successfully".to_string(),
            redirects,
        }),
    )
}

/// OpenAPI operations and schemas generated from this module's route annotations.
#[derive(utoipa::OpenApi)]
#[openapi(paths(
    create_redirect,
    list_redirects,
    get_redirect,
    update_redirect,
    delete_redirect,
    resolve_redirect
))]
pub(super) struct ApiDoc;

/// Combines redirect operations with the nested CSV import documentation.
pub(in crate::api) fn document() -> utoipa::openapi::OpenApi {
    use utoipa::OpenApi;
    let mut document = ApiDoc::openapi();
    document.merge(imports::ApiDoc::openapi());
    document
}
