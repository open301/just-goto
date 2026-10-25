//! Session login, logout, and current-account HTTP endpoints.

use axum::{
    Json, Router,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{
    api::request::{ApiError, ApiState, Authenticated, Database},
    db::tables,
    utils,
};

/// Credentials supplied when creating a bearer-token session.
#[derive(Deserialize, utoipa::ToSchema)]
struct LoginRequest {
    /// Account username.
    username: String,
    /// Account password.
    #[schema(format = Password, write_only)]
    password: String,
}

/// Login result with optional session details and account information.
#[derive(Serialize, utoipa::ToSchema)]
struct LoginResponse {
    /// Result of the login attempt.
    message: String,
    /// Opaque session token to pass as `Authorization: Bearer <token>`.
    #[serde(skip_serializing_if = "Option::is_none")]
    token: Option<String>,
    /// Session expiration as Unix time in seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    expires_at: Option<i64>,
    /// User associated with the session.
    #[serde(skip_serializing_if = "Option::is_none")]
    user: Option<LoginUserResponse>,
}

/// Account identity and roles returned after successful login.
#[derive(Serialize, utoipa::ToSchema)]
struct LoginUserResponse {
    /// User ID.
    id: i64,
    /// Account username.
    username: String,
    /// Assigned role names: admin, editor, or viewer.
    roles: Vec<String>,
}

/// Confirmation that the supplied bearer-token session was revoked.
#[derive(Serialize, utoipa::ToSchema)]
struct LogoutResponse {
    /// Result of the logout operation.
    message: String,
}

/// Lookup result containing the authenticated account when available.
#[derive(Serialize, utoipa::ToSchema)]
struct MeResponse {
    /// Result of the lookup.
    message: String,
    /// Authenticated user account.
    #[serde(skip_serializing_if = "Option::is_none")]
    user: Option<MeUserResponse>,
}

/// Current account details and roles, excluding password and session secrets.
#[derive(Serialize, utoipa::ToSchema)]
struct MeUserResponse {
    /// User ID.
    id: i64,
    /// Account username.
    username: String,
    /// Current assigned role names.
    roles: Vec<String>,
    /// Account creation as Unix time in seconds.
    created_at: i64,
    /// Last account update as Unix time in seconds.
    updated_at: i64,
}

/// Registers login, logout, and current-user routes beneath the authentication prefix.
pub fn routes() -> Router<ApiState> {
    Router::new()
        .route("/login", post(login))
        .route("/logout", post(logout))
        .route("/me", get(me))
}

/// Get the current user.
///
/// Returns the authenticated user and their current roles.
///
/// # Errors
///
/// Returns HTTP 404 if the authenticated account disappears, or HTTP 500 if account or role queries fail.
#[utoipa::path(
    get, path = "/api/v1/auth/me", tag = "Authentication",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "User retrieved successfully", body = MeResponse),
        (status = 404, description = "User not found", body = crate::api::request::ErrorResponse),
        (status = 401, description = "Missing, invalid, expired, or revoked bearer token", body = crate::api::request::ErrorResponse),
        (status = 500, description = "Database or server error", body = crate::api::request::ErrorResponse)
    )
)]
async fn me(auth: Authenticated) -> Result<impl IntoResponse, ApiError> {
    let conn = auth.conn;
    let user = tables::users::get_user_by_id(&conn, auth.user_id)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "User not found"))?;
    let roles = tables::user_roles::get_user_roles(&conn, auth.user_id)?;

    Ok((
        StatusCode::OK,
        Json(MeResponse {
            message: "User retrieved successfully".to_string(),
            user: Some(MeUserResponse {
                id: user.id,
                username: user.username,
                roles,
                created_at: user.created_at,
                updated_at: user.updated_at,
            }),
        }),
    ))
}

/// Log out the current session.
///
/// Revokes only the supplied bearer token.
///
/// # Errors
///
/// Returns HTTP 401 if the session has already been revoked, or HTTP 500 if revoking it fails.
#[utoipa::path(
    post, path = "/api/v1/auth/logout", tag = "Authentication",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Logged out successfully", body = LogoutResponse),
        (status = 401, description = "Missing, invalid, expired, or revoked bearer token", body = crate::api::request::ErrorResponse),
        (status = 500, description = "Database or server error", body = crate::api::request::ErrorResponse)
    )
)]
async fn logout(auth: Authenticated) -> Result<impl IntoResponse, ApiError> {
    if !tables::sessions::delete_session(&auth.conn, &auth.token)? {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "Invalid authorization token",
        ));
    }
    Ok((
        StatusCode::OK,
        Json(LogoutResponse {
            message: "Logged out successfully".to_string(),
        }),
    ))
}

/// Log in and create a session.
///
/// Returns an opaque bearer token valid for 24 hours. No existing token is required.
///
/// # Errors
///
/// Returns HTTP 401 for invalid credentials, or HTTP 500 if database access, password verification, token generation, or session creation fails.
#[utoipa::path(
    post, path = "/api/v1/auth/login", tag = "Authentication",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Logged in successfully", body = LoginResponse),
        (status = 401, description = "Invalid username or password", body = crate::api::request::ErrorResponse),
        (status = 500, description = "Database or server error", body = crate::api::request::ErrorResponse),
        (status = 400, description = "Malformed JSON", body = String, content_type = "text/plain"),
        (status = 413, description = "Request body exceeds the 2 MiB limit", body = String, content_type = "text/plain"),
        (status = 415, description = "Expected application/json content type", body = String, content_type = "text/plain"),
        (status = 422, description = "JSON does not match the request schema", body = String, content_type = "text/plain")
    )
)]
async fn login(
    Database(conn): Database,
    Json(payload): Json<LoginRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let invalid_credentials =
        || ApiError::new(StatusCode::UNAUTHORIZED, "Invalid username or password");
    let credentials = tables::users::get_login_credentials(&conn, &payload.username)?;
    let (user_id, password_hash) = match credentials {
        Some((user_id, hash)) => (Some(user_id), hash),
        None => (None, utils::passwords::DUMMY_PASSWORD_HASH.to_string()),
    };
    let password_valid =
        utils::passwords::verify_password_blocking(payload.password, password_hash)
            .await
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "Database error"))?;
    let user_id = user_id
        .filter(|_| password_valid)
        .ok_or_else(invalid_credentials)?;
    let roles = tables::user_roles::get_user_roles(&conn, user_id)?;

    let session_error = || {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to create session",
        )
    };
    let token = utils::tokens::generate_session_token().map_err(|_| session_error())?;
    let expires_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| session_error())?
        .as_secs() as i64
        + 86_400;
    tables::sessions::create_session(&conn, user_id, &token, expires_at)
        .map_err(|_| session_error())?;

    Ok((
        StatusCode::OK,
        Json(LoginResponse {
            message: "Logged in successfully".to_string(),
            token: Some(token),
            expires_at: Some(expires_at),
            user: Some(LoginUserResponse {
                id: user_id,
                username: payload.username,
                roles,
            }),
        }),
    ))
}

/// OpenAPI operations and schemas generated from this module's route annotations.
#[derive(utoipa::OpenApi)]
#[openapi(paths(login, logout, me))]
pub(in crate::api) struct ApiDoc;
