//! Production frontend files are compiled into the executable; serving them needs no disk access.

use axum::{
    Router,
    extract::Path,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};

use super::request::ApiState;

/// Production HTML embedded when the Rust binary is built.
const INDEX: &[u8] = include_bytes!("../../web/dist/index.html");
/// Production JavaScript bundle embedded when the Rust binary is built.
const JAVASCRIPT: &[u8] = include_bytes!("../../web/dist/assets/app.js");
/// Production CSS bundle embedded when the Rust binary is built.
const STYLESHEET: &[u8] = include_bytes!("../../web/dist/assets/app.css");

/// Registers the public application entry point and embedded asset routes.
pub fn routes() -> Router<ApiState> {
    Router::new()
        .route("/", get(index))
        .route("/assets/{*path}", get(asset))
}

/// Serves the React application without authentication or a database connection.
#[utoipa::path(
    get, path = "/", tag = "Service",
    responses((status = 200, description = "JustGoto web interface", body = String, content_type = "text/html"))
)]
pub async fn index() -> Response {
    file("text/html; charset=utf-8", INDEX)
}

/// Serves the known JavaScript or CSS bundle and returns HTTP 404 for any other asset path.
async fn asset(Path(path): Path<String>) -> Response {
    match path.as_str() {
        "app.js" => file("text/javascript; charset=utf-8", JAVASCRIPT),
        "app.css" => file("text/css; charset=utf-8", STYLESHEET),
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Builds an embedded-file response with its MIME type, cache revalidation, and content-type sniffing protection.
fn file(content_type: &'static str, body: &'static [u8]) -> Response {
    (
        [
            (header::CONTENT_TYPE, content_type),
            // Stable asset names must be revalidated across binary upgrades.
            (header::CACHE_CONTROL, "no-cache"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        body,
    )
        .into_response()
}
