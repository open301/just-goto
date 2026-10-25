//! Cross-route authentication and error contracts, plus public GET /health.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::json;

use super::support::{PROTECTED_ROUTES, TestApi, json_response};
use crate::api::server::router;

/// Checks every protected method and route for HTTP 401 and the shared error body
/// when its token is missing, unknown, or expired.
///
/// # Panics
///
/// Panics if fixture setup fails or any protected route accepts a missing, invalid, or expired token.
#[tokio::test]
async fn every_protected_route_rejects_missing_invalid_and_expired_tokens() {
    let api = TestApi::new();
    for &(method, path) in PROTECTED_ROUTES {
        for (token, message) in [
            (None, "Missing or invalid authorization token"),
            (Some("unknown"), "Invalid authorization token"),
            (Some("expired"), "Invalid authorization token"),
        ] {
            let (status, body) = api.request(method, path, token, json!({})).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {path}");
            assert_eq!(body, json!({"message": message}), "{method} {path}");
        }
    }
}

/// Uses an unusable database path to prove malformed authorization is rejected first.
///
/// # Panics
///
/// Panics if a malformed authorization header fails to return the expected HTTP 401 response without database access.
#[tokio::test]
async fn malformed_authorization_is_rejected_before_opening_the_database() {
    let app = router("/dev/null/not-a-database");
    for header in ["Basic credentials", "Bearer", "Bearer ", "bearer token"] {
        let request = Request::builder()
            .uri("/api/v1/auth/me")
            .header("authorization", header)
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            json_response(&app, request).await,
            (
                StatusCode::UNAUTHORIZED,
                json!({"message": "Missing or invalid authorization token"})
            )
        );
    }
}

/// Confirms health needs no connection while database-dependent routes return the
/// shared HTTP 500 response when a connection cannot be opened.
///
/// # Panics
///
/// Panics if public health accesses SQLite or database-dependent routes fail to return the expected shared connection error.
#[tokio::test]
async fn public_health_does_not_need_a_database_and_database_failures_use_shared_errors() {
    let app = router("/dev/null/not-a-database");
    for (path, expected) in [
        ("/health", StatusCode::OK),
        ("/existing", StatusCode::INTERNAL_SERVER_ERROR),
        ("/api/v1/auth/me", StatusCode::INTERNAL_SERVER_ERROR),
    ] {
        let request = Request::builder()
            .uri(path)
            .header("authorization", "Bearer token")
            .body(Body::empty())
            .unwrap();
        let (status, body) = json_response(&app, request).await;
        assert_eq!(status, expected, "{path}");
        if status.is_server_error() {
            assert_eq!(body, json!({"message": "Failed to open database"}));
        }
    }
}

/// Removes role and session tables to verify authorization-query failures return
/// the shared HTTP 500 message rather than authentication or permission errors.
///
/// # Panics
///
/// Panics if arranging the database failure fails or broken authorization queries do not return the shared HTTP 500 response.
#[tokio::test]
async fn authorization_query_failures_return_server_errors() {
    let api = TestApi::new();
    api.conn().execute("DROP TABLE user_roles", []).unwrap();
    let (status, body) = api
        .request("GET", "/api/v1/users", Some("admin"), json!({}))
        .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body, json!({"message": "Authorization check failed"}));
    api.conn().execute("DROP TABLE sessions", []).unwrap();
    let (status, body) = api
        .request("GET", "/api/v1/auth/me", Some("admin"), json!({}))
        .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body, json!({"message": "Authorization check failed"}));
}

/// Database failures retain each redirect route's HTTP 500 message and collection response shape.
///
/// # Panics
///
/// Panics if arranging the database failure fails or a redirect route returns an unexpected error message or collection body.
#[tokio::test]
async fn redirect_query_failures_preserve_error_messages_and_collection_bodies() {
    let api = TestApi::new();
    api.conn().execute("DROP TABLE redirects", []).unwrap();
    for (method, path, expected) in [
        ("GET", "/existing", json!({"message": "Database error"})),
        (
            "GET",
            "/api/v1/redirects/1",
            json!({"message": "Database error"}),
        ),
        (
            "POST",
            "/api/v1/redirects",
            json!({"message": "Database error"}),
        ),
        (
            "PATCH",
            "/api/v1/redirects/1",
            json!({"message": "Database error"}),
        ),
        (
            "DELETE",
            "/api/v1/redirects/1",
            json!({"message": "Failed to delete redirect"}),
        ),
        (
            "GET",
            "/api/v1/redirects",
            json!({"message": "Database error", "redirects": []}),
        ),
    ] {
        let (status, body) = api
            .request(
                method,
                path,
                Some("admin"),
                json!({"slug": "candidate", "destination": "https://example.com"}),
            )
            .await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{method} {path}");
        assert_eq!(body, expected, "{method} {path}");
    }
}
