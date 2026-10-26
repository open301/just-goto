//! Public frontend routes and embedded asset contracts.

use axum::{
    body::to_bytes,
    http::{StatusCode, header},
};

use super::support::public_get;

/// HTML bytes expected from the public application entry point.
const INDEX_CONTENT: &[u8] = include_bytes!("../../../web/dist/index.html");
/// JavaScript bundle bytes expected from the embedded asset route.
const JAVASCRIPT_CONTENT: &[u8] = include_bytes!("../../../web/dist/assets/app.js");
/// CSS bundle bytes expected from the embedded asset route.
const STYLESHEET_CONTENT: &[u8] = include_bytes!("../../../web/dist/assets/app.css");

/// GET / and /assets/app.{js,css} serve embedded files without authentication or SQLite.
///
/// # Panics
///
/// Panics if a frontend route returns unexpected bytes, response headers, or HTML asset references.
#[tokio::test]
async fn frontend_and_its_assets_are_embedded_and_public() {
    for (path, content_type, contents) in [
        ("/", "text/html; charset=utf-8", INDEX_CONTENT),
        (
            "/assets/app.js",
            "text/javascript; charset=utf-8",
            JAVASCRIPT_CONTENT,
        ),
        (
            "/assets/app.css",
            "text/css; charset=utf-8",
            STYLESHEET_CONTENT,
        ),
    ] {
        let response = public_get(path).await;
        assert_eq!(response.status(), StatusCode::OK, "{path}");
        assert_eq!(response.headers()[header::CONTENT_TYPE], content_type);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-cache");
        assert_eq!(
            response.headers()[header::X_CONTENT_TYPE_OPTIONS],
            "nosniff"
        );
        let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        assert_eq!(body.as_ref(), contents);
    }
    let html = std::str::from_utf8(INDEX_CONTENT).unwrap();
    assert!(html.contains("/assets/app.js"));
    assert!(html.contains("/assets/app.css"));
    assert!(!html.contains("/src/"));
}

/// Unknown GET /assets/* paths return 404 without serving the HTML page.
///
/// # Panics
///
/// Panics if an unknown asset path returns anything other than HTTP 404.
#[tokio::test]
async fn unknown_assets_do_not_fall_back_to_html_or_access_the_database() {
    for path in ["/assets/missing.js", "/assets/nested/file.css"] {
        let response = public_get(path).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
