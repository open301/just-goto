//! Shared request setup. Put the required extractor before body extractors such as `Json`:
//! - `Database`: public route that needs SQLite (for example, login or resolving a slug).
//! - `Authenticated`: any signed-in user, with the validated user ID and session token.
//! - `Admin` / `EditorOrAdmin`: authentication plus the required role.
//!
//! Protected routes reuse the same connection for authorization and their own queries.
//! Routes without these extractors (for example, health) never open the database.
//! The database path comes from startup configuration rather than reloading the environment.
//!
//! Rejections use JSON `{ "message": "..." }`, including on collection routes:
//! 401 for missing/invalid sessions, 403 for insufficient roles, and 500 for database failures.
//! A handler returning `Result<impl IntoResponse, ApiError>` can use `?` for database errors
//! and `auth.require_self_or_admin(target_user_id)?` for checks on a specific user.

use axum::{
    Json,
    extract::FromRequestParts,
    http::{StatusCode, header::AUTHORIZATION, request::Parts},
    response::{IntoResponse, Response},
};
use rusqlite::Connection;
use serde::Serialize;

use crate::db::{open_database::open_database, tables};
use tables::user_roles::Role;

/// Startup configuration shared by routes; connections are opened only when extracted.
#[derive(Clone)]
pub struct ApiState {
    /// Configured SQLite database path shared with request extractors.
    db_path: String,
}

/// Stores the database configuration used by the request extractors.
impl ApiState {
    /// Retains the configured database path without opening a connection.
    pub fn new(db_path: impl Into<String>) -> Self {
        Self {
            db_path: db_path.into(),
        }
    }
}

/// An HTTP failure returned as a JSON object containing only a `message` field.
#[derive(Debug)]
pub struct ApiError {
    /// HTTP status sent when this error becomes a response.
    status: StatusCode,
    /// Client-facing description that excludes underlying database details.
    message: String,
}

/// Creates errors with messages suitable for API responses.
impl ApiError {
    /// Pairs an HTTP status with the message that will be sent to the client.
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    /// Maps authorization-query failures to HTTP 500 without exposing SQLite details.
    fn authorization_failed(_: rusqlite::Error) -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Authorization check failed",
        )
    }
}

/// Allows handlers returning `ApiError` to propagate database failures with `?`.
impl From<rusqlite::Error> for ApiError {
    /// Converts a SQLite failure to HTTP 500 with the shared database-error message.
    fn from(_: rusqlite::Error) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "Database error")
    }
}

/// Gives handler errors and extractor rejections the same JSON response format.
impl IntoResponse for ApiError {
    /// Serializes the message and uses the stored HTTP status for the response.
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorResponse {
                message: self.message,
            }),
        )
            .into_response()
    }
}

/// JSON body shared by authentication, authorization, and database errors.
#[derive(Serialize, utoipa::ToSchema)]
pub struct ErrorResponse {
    /// A description of the failure suitable for the API client.
    message: String,
}

/// A request-owned SQLite connection for a route that does not require authentication.
pub struct Database(pub Connection);

/// Opens connections using the database path supplied when the router was built.
impl FromRequestParts<ApiState> for Database {
    type Rejection = ApiError;

    /// Opens SQLite with foreign keys and WAL enabled, returning HTTP 500 on failure.
    async fn from_request_parts(_: &mut Parts, state: &ApiState) -> Result<Self, ApiError> {
        open_database(&state.db_path).map(Self).map_err(|_| {
            ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "Failed to open database")
        })
    }
}

/// A validated session and its user, together with the connection used to check them.
pub struct Authenticated {
    /// Reused by role checks and the handler's database queries.
    pub conn: Connection,
    /// The existing user associated with the unexpired session.
    pub user_id: i64,
    /// The supplied bearer token, retained for operations such as logout.
    pub token: String,
}

/// Checks permissions using the user ID established during authentication.
impl Authenticated {
    /// Requires at least one matching role in `user_roles`.
    ///
    /// Reuses this request's connection and validated user ID. An empty role list never grants access.
    ///
    /// # Errors
    ///
    /// Returns HTTP 403 if no requested role matches, or HTTP 500 if a role query fails.
    fn require_any_role(&self, roles: &[Role]) -> Result<(), ApiError> {
        for &role in roles {
            if tables::user_roles::check_user_role(&self.conn, self.user_id, role)
                .map_err(ApiError::authorization_failed)?
            {
                return Ok(());
            }
        }

        Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "Insufficient permissions",
        ))
    }

    /// Allows access to the authenticated user's own account or requires the admin role.
    ///
    /// Does not check whether the target account exists.
    ///
    /// # Errors
    ///
    /// Returns HTTP 403 when accessing another account without admin access, or HTTP 500 if the role query fails.
    pub fn require_self_or_admin(&self, user_id: i64) -> Result<(), ApiError> {
        if self.user_id == user_id {
            Ok(())
        } else {
            self.require_any_role(&[Role::Admin])
        }
    }
}

/// Authenticates requests before the handler or its body extractor runs.
impl FromRequestParts<ApiState> for Authenticated {
    type Rejection = ApiError;

    /// Reads a nonempty `Bearer ` token, then checks `users` and `sessions`.
    ///
    /// Missing headers, invalid header text, a prefix other than `Bearer `, or an empty
    /// token return HTTP 401 before opening SQLite. Unknown or expired sessions and
    /// sessions without an existing user also return HTTP 401; connection or query
    /// failures return HTTP 500. The returned connection is reused for subsequent
    /// role checks and handler queries.
    async fn from_request_parts(parts: &mut Parts, state: &ApiState) -> Result<Self, ApiError> {
        let token = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .filter(|token| !token.is_empty())
            .ok_or_else(|| {
                ApiError::new(
                    StatusCode::UNAUTHORIZED,
                    "Missing or invalid authorization token",
                )
            })?
            .to_owned();

        let Database(conn) = Database::from_request_parts(parts, state).await?;
        let user_id = tables::multi_tables::get_user_id_from_token(&conn, &token)
            .map_err(ApiError::authorization_failed)?
            .ok_or_else(|| {
                ApiError::new(StatusCode::UNAUTHORIZED, "Invalid authorization token")
            })?;

        Ok(Self {
            conn,
            user_id,
            token,
        })
    }
}

/// A validated request whose user has the admin role.
pub struct Admin(pub Authenticated);

/// Adds an admin-role requirement to the shared authentication extractor.
impl FromRequestParts<ApiState> for Admin {
    type Rejection = ApiError;

    /// Authenticates once, then checks admin access using the same connection.
    ///
    /// Propagates authentication failures, returns HTTP 403 for a missing role,
    /// and returns HTTP 500 if the role query fails.
    async fn from_request_parts(parts: &mut Parts, state: &ApiState) -> Result<Self, ApiError> {
        let auth = Authenticated::from_request_parts(parts, state).await?;
        auth.require_any_role(&[Role::Admin])?;
        Ok(Self(auth))
    }
}

/// A validated request whose user may create, update, delete, or import redirects.
pub struct EditorOrAdmin(pub Authenticated);

/// Adds an editor-or-admin requirement to the shared authentication extractor.
impl FromRequestParts<ApiState> for EditorOrAdmin {
    type Rejection = ApiError;

    /// Authenticates once and accepts either the editor role or the admin role.
    ///
    /// Reuses the authentication connection, propagates authentication failures,
    /// and returns HTTP 403 for insufficient roles or HTTP 500 for role-query failures.
    async fn from_request_parts(parts: &mut Parts, state: &ApiState) -> Result<Self, ApiError> {
        let auth = Authenticated::from_request_parts(parts, state).await?;
        auth.require_any_role(&[Role::Editor, Role::Admin])?;
        Ok(Self(auth))
    }
}
