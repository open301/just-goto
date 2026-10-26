//! User account management and password-change HTTP endpoints.

use axum::{
    Json, Router,
    extract::{Path, Query},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, put},
};
use serde::{Deserialize, Serialize};

use crate::{
    api::request::{Admin, ApiError, ApiState, Authenticated},
    db::tables,
    utils,
};

/// Username, initial password, and role supplied when creating an account.
#[derive(Deserialize, utoipa::ToSchema)]
#[schema(as = CreateUserRequest)]
struct CreateRequest {
    /// Account username, trimmed to 1–64 characters without internal whitespace or control characters.
    #[schema(example = "example-user")]
    username: String,
    /// Initial password, at least 12 bytes in UTF-8.
    #[schema(format = Password, write_only)]
    password: String,
    /// One of admin, editor, or viewer (case-insensitive).
    #[schema(example = "viewer")]
    role: String,
}

/// Optional username and replacement role supplied in an account update.
#[derive(Deserialize, utoipa::ToSchema)]
struct UpdateUserRequest {
    /// Replacement username, trimmed to 1–64 characters without internal whitespace or control characters.
    #[schema(example = "example-user")]
    username: Option<String>,
    /// Replacement role: admin, editor, or viewer (case-insensitive).
    #[schema(example = "viewer")]
    role: Option<String>,
}

/// Replacement password and the current password when changing the caller's own credentials.
#[derive(Deserialize, utoipa::ToSchema)]
struct UpdatePasswordRequest {
    /// Required when changing your own password; omitted for an admin reset of another account.
    #[schema(format = Password, write_only)]
    current_password: Option<String>,
    /// Replacement password, at least 12 bytes in UTF-8.
    #[schema(format = Password, write_only)]
    new_password: String,
}

/// Confirmation that an account was created.
#[derive(Serialize, utoipa::ToSchema)]
#[schema(as = CreateUserResponse)]
struct CreateResponse {
    /// Human-readable outcome of the operation.
    message: String,
}

/// Confirmation that account details were updated.
#[derive(Serialize, utoipa::ToSchema)]
struct UpdateUserResponse {
    /// Human-readable outcome of the operation.
    message: String,
}

/// Confirmation that a password was changed and existing sessions were revoked.
#[derive(Serialize, utoipa::ToSchema)]
struct UpdatePasswordResponse {
    /// Human-readable outcome of the operation.
    message: String,
}

/// Confirmation that an account was deleted.
#[derive(Serialize, utoipa::ToSchema)]
struct DeleteUserResponse {
    /// Human-readable outcome of the operation.
    message: String,
}

/// Optional pagination, username, and role filters for account listings.
#[derive(Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
struct ListUsersQuery {
    /// Page size. Defaults to 50; values are clamped to 1–100.
    #[param(default = 50)]
    limit: Option<u32>,
    /// Number of users to skip. Defaults to 0.
    #[param(default = 0)]
    offset: Option<u32>,
    /// Substring matched against usernames using SQLite LIKE.
    username: Option<String>,
    /// Filter by admin, editor, or viewer (case-insensitive).
    role: Option<String>,
}

/// An account listing and its human-readable outcome.
#[derive(Serialize, utoipa::ToSchema)]
struct ListUsersResponse {
    /// Human-readable outcome of the operation.
    message: String,
    /// Accounts matching the requested filters; empty when the listing fails.
    users: Vec<UserResponse>,
}

/// Account details and assigned roles, excluding password and session secrets.
#[derive(Serialize, utoipa::ToSchema)]
struct UserResponse {
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

/// Lookup result containing the requested account when available.
#[derive(Serialize, utoipa::ToSchema)]
struct GetUserResponse {
    /// Human-readable outcome of the operation.
    message: String,
    /// Requested account; omitted when no account details are available.
    #[serde(skip_serializing_if = "Option::is_none")]
    user: Option<UserResponse>,
}

/// Registers account collection, account details, and password-change routes beneath the users prefix.
pub fn routes() -> Router<ApiState> {
    Router::new()
        .route("/", get(list_users).post(create_user))
        .route(
            "/{id}",
            get(get_user).patch(update_user).delete(delete_user),
        )
        .route("/{id}/password", put(update_password))
}

/// Delete a user.
///
/// Requires admin access. The final admin account cannot be deleted. Deletion revokes the user's sessions.
///
/// # Errors
///
/// Returns HTTP 404 for an unknown account, HTTP 409 when deleting the final administrator, or HTTP 500 if an account or role query fails.
#[utoipa::path(
    delete, path = "/api/v1/users/{id}", tag = "Users",
    params(("id" = i64, Path, description = "User ID")),
    security(("bearer_auth" = [])),
    responses(
        (status = 400, description = "Invalid path or query parameters", body = String, content_type = "text/plain"),
        (status = 200, description = "User deleted successfully", body = DeleteUserResponse),
        (status = 404, description = "User not found", body = crate::api::request::ErrorResponse),
        (status = 409, description = "The final admin account cannot be deleted", body = crate::api::request::ErrorResponse),
        (status = 401, description = "Missing, invalid, expired, or revoked bearer token", body = crate::api::request::ErrorResponse),
        (status = 403, description = "Insufficient permissions", body = crate::api::request::ErrorResponse),
        (status = 500, description = "Database or server error", body = crate::api::request::ErrorResponse)
    )
)]
async fn delete_user(
    Admin(auth): Admin,
    Path(user_id): Path<i64>,
) -> Result<impl IntoResponse, ApiError> {
    let mut conn = auth.conn;
    tables::users::delete_user(&mut conn, user_id)
        .map_err(|error| account_mutation_error(error, "deleted", "Failed to delete user"))?;
    Ok((
        StatusCode::OK,
        Json(DeleteUserResponse {
            message: "User deleted successfully".to_string(),
        }),
    ))
}

/// Change or reset a password.
///
/// Changing your own password requires current_password, including for admins. Admins may reset
/// another user's password without it. The new password must be at least 12 bytes; all sessions for
/// the target user are revoked.
///
/// # Errors
///
/// Returns HTTP 400 for missing or invalid password fields, HTTP 401 for an incorrect current password, HTTP 403 for another account without admin access, HTTP 404 for an unknown account, or HTTP 500 if hashing, verification, or database access fails.
#[utoipa::path(
    put, path = "/api/v1/users/{id}/password", tag = "Users",
    request_body = UpdatePasswordRequest,
    params(("id" = i64, Path, description = "User ID")),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Password changed and existing sessions revoked", body = UpdatePasswordResponse),
        (status = 400, description = "Missing current password or new password shorter than 12 bytes", content((crate::api::request::ErrorResponse = "application/json"), (String = "text/plain"))),
        (status = 401, description = "Missing, invalid, expired, or revoked bearer token; or incorrect current password", body = crate::api::request::ErrorResponse),
        (status = 403, description = "Insufficient permissions", body = crate::api::request::ErrorResponse),
        (status = 404, description = "User not found", body = crate::api::request::ErrorResponse),
        (status = 500, description = "Database or server error", body = crate::api::request::ErrorResponse),
        (status = 413, description = "Request body exceeds the 2 MiB limit", body = String, content_type = "text/plain"),
        (status = 415, description = "Expected application/json content type", body = String, content_type = "text/plain"),
        (status = 422, description = "JSON does not match the request schema", body = String, content_type = "text/plain")
    )
)]
async fn update_password(
    auth: Authenticated,
    Path(user_id): Path<i64>,
    Json(payload): Json<UpdatePasswordRequest>,
) -> Result<impl IntoResponse, ApiError> {
    auth.require_self_or_admin(user_id)?;
    let authenticated_user_id = auth.user_id;
    let mut conn = auth.conn;

    let stored_password_hash = tables::users::get_password_hash_by_user_id(&conn, user_id)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "User not found"))?;

    if authenticated_user_id == user_id {
        let current_password = payload.current_password.ok_or_else(|| {
            ApiError::new(StatusCode::BAD_REQUEST, "Current password is required")
        })?;

        let valid =
            utils::passwords::verify_password_blocking(current_password, stored_password_hash)
                .await
                .map_err(|_| {
                    ApiError::new(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Failed to verify password",
                    )
                })?;
        if !valid {
            return Err(ApiError::new(
                StatusCode::UNAUTHORIZED,
                "Current password is incorrect",
            ));
        }
    }

    utils::passwords::validate_password(&payload.new_password)
        .map_err(|message| ApiError::new(StatusCode::BAD_REQUEST, message))?;

    let password_hash = utils::passwords::hash_password_blocking(payload.new_password)
        .await
        .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "Failed to hash password"))?;
    tables::users::update_user_password(&mut conn, user_id, &password_hash).map_err(|_| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to update password",
        )
    })?;

    Ok((
        StatusCode::OK,
        Json(UpdatePasswordResponse {
            message: "Password updated successfully; existing sessions were revoked".to_string(),
        }),
    ))
}

/// Update a user.
///
/// Requires admin access and at least one field. A supplied role replaces the existing roles.
///
/// # Errors
///
/// Returns HTTP 400 for invalid update fields, HTTP 404 for an unknown account, HTTP 409 for an occupied username or demotion of the final administrator, or HTTP 500 if a database operation fails.
#[utoipa::path(
    patch, path = "/api/v1/users/{id}", tag = "Users",
    request_body = UpdateUserRequest,
    params(("id" = i64, Path, description = "User ID")),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "User updated successfully", body = UpdateUserResponse),
        (status = 400, description = "Invalid or missing update fields", content((crate::api::request::ErrorResponse = "application/json"), (String = "text/plain"))),
        (status = 404, description = "User not found", body = crate::api::request::ErrorResponse),
        (status = 409, description = "Username already exists or the final admin account cannot be demoted", body = crate::api::request::ErrorResponse),
        (status = 401, description = "Missing, invalid, expired, or revoked bearer token", body = crate::api::request::ErrorResponse),
        (status = 403, description = "Insufficient permissions", body = crate::api::request::ErrorResponse),
        (status = 500, description = "Database or server error", body = crate::api::request::ErrorResponse),
        (status = 413, description = "Request body exceeds the 2 MiB limit", body = String, content_type = "text/plain"),
        (status = 415, description = "Expected application/json content type", body = String, content_type = "text/plain"),
        (status = 422, description = "JSON does not match the request schema", body = String, content_type = "text/plain")
    )
)]
async fn update_user(
    Admin(auth): Admin,
    Path(user_id): Path<i64>,
    Json(payload): Json<UpdateUserRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let mut conn = auth.conn;
    if payload.username.is_none() && payload.role.is_none() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "At least one field is required",
        ));
    }
    let username = payload
        .username
        .as_deref()
        .map(utils::users::validate_username)
        .transpose()
        .map_err(|message| ApiError::new(StatusCode::BAD_REQUEST, message))?;
    let role = payload
        .role
        .as_deref()
        .map(|role| {
            tables::user_roles::string_to_role(role)
                .ok_or_else(|| ApiError::new(StatusCode::BAD_REQUEST, "Invalid role"))
        })
        .transpose()?;
    tables::users::update_user_account(&mut conn, user_id, username, role)
        .map_err(|error| account_mutation_error(error, "demoted", "Failed to update user"))?;

    Ok((
        StatusCode::OK,
        Json(UpdateUserResponse {
            message: "User updated successfully".to_string(),
        }),
    ))
}

/// Get a user.
///
/// Requires access to your own account or the admin role.
///
/// # Errors
///
/// Returns HTTP 403 for another account without admin access, HTTP 404 for an unknown account, or HTTP 500 if account or role queries fail.
#[utoipa::path(
    get, path = "/api/v1/users/{id}", tag = "Users",
    params(("id" = i64, Path, description = "User ID")),
    security(("bearer_auth" = [])),
    responses(
        (status = 400, description = "Invalid path or query parameters", body = String, content_type = "text/plain"),
        (status = 200, description = "User retrieved successfully", body = GetUserResponse),
        (status = 404, description = "User not found", body = crate::api::request::ErrorResponse),
        (status = 401, description = "Missing, invalid, expired, or revoked bearer token", body = crate::api::request::ErrorResponse),
        (status = 403, description = "Insufficient permissions", body = crate::api::request::ErrorResponse),
        (status = 500, description = "Database or server error", body = crate::api::request::ErrorResponse)
    )
)]
async fn get_user(
    auth: Authenticated,
    Path(user_id): Path<i64>,
) -> Result<impl IntoResponse, ApiError> {
    auth.require_self_or_admin(user_id)?;
    let conn = auth.conn;
    let user = tables::users::get_user_by_id(&conn, user_id)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "User not found"))?;
    let roles = tables::user_roles::get_user_roles(&conn, user_id)?;

    Ok((
        StatusCode::OK,
        Json(GetUserResponse {
            message: "User retrieved successfully".to_string(),
            user: Some(UserResponse {
                id: user.id,
                username: user.username,
                roles,
                created_at: user.created_at,
                updated_at: user.updated_at,
            }),
        }),
    ))
}

/// List users.
///
/// Requires admin access. Results are ordered by user ID.
#[utoipa::path(
    get, path = "/api/v1/users", tag = "Users",
    params(ListUsersQuery),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Users retrieved successfully", body = ListUsersResponse),
        (status = 400, description = "Invalid role filter or query parameters", content((crate::api::request::ErrorResponse = "application/json"), (String = "text/plain"))),
        (status = 401, description = "Missing, invalid, expired, or revoked bearer token", body = crate::api::request::ErrorResponse),
        (status = 403, description = "Insufficient permissions", body = crate::api::request::ErrorResponse),
        (status = 500, description = "Database or server error", body = crate::api::request::ErrorResponse)
    )
)]
async fn list_users(Admin(auth): Admin, Query(query): Query<ListUsersQuery>) -> impl IntoResponse {
    let conn = auth.conn;

    let limit = query.limit.unwrap_or(50).clamp(1, 100) as i64;
    let offset = query.offset.unwrap_or(0) as i64;
    let role = match query.role.as_deref() {
        Some(role) => match tables::user_roles::string_to_role(role) {
            Some(role) => Some(tables::user_roles::role_to_string(role)),
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(ListUsersResponse {
                        message: "Invalid role".to_string(),
                        users: Vec::new(),
                    }),
                );
            },
        },
        None => None,
    };

    let users =
        match tables::users::list_users(&conn, query.username.as_deref(), role, limit, offset) {
            Ok(users) => users,
            Err(_) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ListUsersResponse {
                        message: "Database error".to_string(),
                        users: Vec::new(),
                    }),
                );
            },
        };

    let response_users = users
        .into_iter()
        .map(|listed_user| UserResponse {
            id: listed_user.user.id,
            username: listed_user.user.username,
            roles: listed_user.roles,
            created_at: listed_user.user.created_at,
            updated_at: listed_user.user.updated_at,
        })
        .collect();

    (
        StatusCode::OK,
        Json(ListUsersResponse {
            message: "Users retrieved successfully".to_string(),
            users: response_users,
        }),
    )
}

/// Create a user.
///
/// Requires admin access. Assign one of the admin, editor, or viewer roles.
///
/// # Errors
///
/// Returns HTTP 400 for an invalid role, invalid username, or password shorter than 12 bytes, HTTP 409 for an occupied username, or HTTP 500 if password hashing, account creation, or role assignment fails.
#[utoipa::path(
    post, path = "/api/v1/users", tag = "Users",
    request_body = CreateRequest,
    security(("bearer_auth" = [])),
    responses(
        (status = 201, description = "User created successfully", body = CreateResponse),
        (status = 400, description = "Invalid role, username, or password", content((crate::api::request::ErrorResponse = "application/json"), (String = "text/plain"))),
        (status = 409, description = "Username already exists", body = crate::api::request::ErrorResponse),
        (status = 401, description = "Missing, invalid, expired, or revoked bearer token", body = crate::api::request::ErrorResponse),
        (status = 403, description = "Insufficient permissions", body = crate::api::request::ErrorResponse),
        (status = 500, description = "Database or server error", body = crate::api::request::ErrorResponse),
        (status = 413, description = "Request body exceeds the 2 MiB limit", body = String, content_type = "text/plain"),
        (status = 415, description = "Expected application/json content type", body = String, content_type = "text/plain"),
        (status = 422, description = "JSON does not match the request schema", body = String, content_type = "text/plain")
    )
)]
async fn create_user(
    Admin(auth): Admin,
    Json(payload): Json<CreateRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let mut conn = auth.conn;
    let role = tables::user_roles::string_to_role(&payload.role)
        .ok_or_else(|| ApiError::new(StatusCode::BAD_REQUEST, "Invalid role"))?;
    let username = utils::users::validate_username(&payload.username)
        .map_err(|message| ApiError::new(StatusCode::BAD_REQUEST, message))?
        .to_owned();
    utils::passwords::validate_password(&payload.password)
        .map_err(|message| ApiError::new(StatusCode::BAD_REQUEST, message))?;
    let password_hash = utils::passwords::hash_password_blocking(payload.password)
        .await
        .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "Failed to hash password"))?;
    tables::users::create_user_with_role(&mut conn, &username, &password_hash, role).map_err(
        |error| {
            use tables::users::CreateUserError;
            let status = match error {
                CreateUserError::UsernameExists => StatusCode::CONFLICT,
                CreateUserError::Database
                | CreateUserError::CreateAccount
                | CreateUserError::AssignRole => StatusCode::INTERNAL_SERVER_ERROR,
            };
            ApiError::new(status, error.to_string())
        },
    )?;

    Ok((
        StatusCode::CREATED,
        Json(CreateResponse {
            message: "User created successfully".to_string(),
        }),
    ))
}

/// Maps account-integrity failures to HTTP errors with operation-specific final-admin and database messages.
fn account_mutation_error(
    error: tables::users::AccountMutationError,
    action: &str,
    database_message: &str,
) -> ApiError {
    use tables::users::AccountMutationError;
    match error {
        AccountMutationError::NotFound => ApiError::new(StatusCode::NOT_FOUND, "User not found"),
        AccountMutationError::FinalAdmin => ApiError::new(
            StatusCode::CONFLICT,
            format!("The final admin account cannot be {action}"),
        ),
        AccountMutationError::UsernameExists => {
            ApiError::new(StatusCode::CONFLICT, "Username already exists")
        },
        AccountMutationError::Database => {
            ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, database_message)
        },
    }
}

/// OpenAPI operations and schemas generated from this module's route annotations.
#[derive(utoipa::OpenApi)]
#[openapi(paths(
    create_user,
    list_users,
    get_user,
    update_user,
    delete_user,
    update_password
))]
pub(in crate::api) struct ApiDoc;
