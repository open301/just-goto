//! /api/v1/auth routes: login failures and session revocation after logout or account deletion.

use axum::http::StatusCode;
use serde_json::json;

use super::support::{PASSWORD, TestApi};

/// Successful login purges expired sessions for every account while preserving unexpired tokens and issuing a usable new token.
///
/// # Panics
///
/// Panics if fixture setup or session inspection fails, expired rows survive login, active sessions disappear, or the new token cannot authenticate.
#[tokio::test]
async fn login_purges_expired_sessions() {
    let api = TestApi::new();
    let conn = api.conn();
    conn.execute(
        "INSERT INTO sessions (user_id, hash_string, expires_at) VALUES (2, 'boundary', unixepoch())",
        [],
    )
    .unwrap();
    let expired_before: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sessions WHERE expires_at <= unixepoch()",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(expired_before, 2);
    let (status, body) = api
        .request(
            "POST",
            "/api/v1/auth/login",
            None,
            json!({"username": "viewer", "password": PASSWORD}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let expired_after: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sessions WHERE expires_at <= unixepoch()",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(expired_after, 0);
    for token in [
        "admin",
        "editor",
        "viewer",
        "other",
        "unassigned",
        body["token"].as_str().unwrap(),
    ] {
        assert_eq!(
            api.request("GET", "/api/v1/auth/me", Some(token), json!({}))
                .await
                .0,
            StatusCode::OK
        );
    }
    assert_eq!(
        api.request("GET", "/api/v1/auth/me", Some("expired"), json!({}))
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
}

/// POST /api/v1/auth/login rejects unknown users and wrong passwords without leaking session fields.
///
/// # Panics
///
/// Panics if fixture setup fails or invalid credentials do not return the expected HTTP 401 message-only response.
#[tokio::test]
async fn invalid_credentials_keep_the_login_error_response() {
    let api = TestApi::new();
    for (username, password) in [("unknown", PASSWORD), ("viewer", "wrong-password")] {
        assert_eq!(
            api.request(
                "POST",
                "/api/v1/auth/login",
                None,
                json!({"username": username, "password": password})
            )
            .await,
            (
                StatusCode::UNAUTHORIZED,
                json!({"message": "Invalid username or password"})
            ),
        );
    }
}

/// Confirms logout invalidates a token and deleting a user invalidates that user's session.
///
/// # Panics
///
/// Panics if fixture setup fails, logout or deletion fails, or a revoked token can still authenticate.
#[tokio::test]
async fn logout_and_deleted_users_cannot_reuse_their_tokens() {
    let api = TestApi::new();
    assert_eq!(
        api.request("POST", "/api/v1/auth/logout", Some("viewer"), json!({}))
            .await
            .0,
        StatusCode::OK
    );
    for (method, path) in [("GET", "/api/v1/auth/me"), ("POST", "/api/v1/auth/logout")] {
        assert_eq!(
            api.request(method, path, Some("viewer"), json!({})).await.0,
            StatusCode::UNAUTHORIZED
        );
    }
    assert_eq!(
        api.request("DELETE", "/api/v1/users/4", Some("admin"), json!({}))
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        api.request("GET", "/api/v1/auth/me", Some("other"), json!({}))
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
}

/// Login preserves the database-error contract for lookup failures and malformed password hashes.
///
/// # Panics
///
/// Panics if arranging a failure fails, login returns credentials instead of HTTP 500, or its error message changes.
#[tokio::test]
async fn login_database_and_hash_failures_keep_the_shared_error_response() {
    let api = TestApi::new();
    api.conn()
        .execute(
            "UPDATE users SET password_hash = 'malformed' WHERE username = 'viewer'",
            [],
        )
        .unwrap();
    assert_eq!(
        api.request(
            "POST",
            "/api/v1/auth/login",
            None,
            json!({"username": "viewer", "password": PASSWORD})
        )
        .await,
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"message": "Database error"})
        )
    );
    api.conn()
        .execute_batch("PRAGMA foreign_keys = OFF; DROP TABLE users;")
        .unwrap();
    for username in ["viewer", "unknown"] {
        assert_eq!(
            api.request(
                "POST",
                "/api/v1/auth/login",
                None,
                json!({"username": username, "password": PASSWORD})
            )
            .await,
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"message": "Database error"})
            )
        );
    }
}

/// The absent-account hash remains valid and uses the same Argon2 parameters and output length as newly created password hashes.
///
/// # Panics
///
/// Panics if hashing or dummy verification fails or the dummy verification cost drifts from the production password configuration.
#[test]
fn dummy_login_hash_matches_production_verification_parameters() {
    use crate::utils::passwords::{DUMMY_PASSWORD_HASH, hash_password, verify_password};
    let production = hash_password(PASSWORD).unwrap();
    let production_parts: Vec<_> = production.split('$').collect();
    let dummy_parts: Vec<_> = DUMMY_PASSWORD_HASH.split('$').collect();
    assert_eq!(&dummy_parts[..4], &production_parts[..4]);
    assert_eq!(dummy_parts[5].len(), production_parts[5].len());
    assert!(!verify_password(PASSWORD, DUMMY_PASSWORD_HASH).unwrap());
}
