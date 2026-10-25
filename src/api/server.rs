//! Composes the application router and runs its HTTP listener.

use crate::api::{docs, frontend, request::ApiState, routes};
use axum::{Router, routing::get};

/// Builds the application with an embedded frontend, API documentation, and a database path.
pub fn router(db_path: impl Into<String>) -> Router {
    let api_routes = Router::new()
        .nest("/auth", routes::auth::routes())
        .nest("/redirects", routes::redirects::routes())
        .nest("/users", routes::users::routes());

    Router::new()
        .merge(frontend::routes())
        .nest("/health", routes::health::routes())
        .nest("/api/v1", api_routes)
        .route("/{slug}", get(routes::redirects::resolve_redirect))
        .merge(docs::routes())
        .with_state(ApiState::new(db_path))
}

/// Binds the configured address and serves the application until the HTTP server stops.
///
/// # Errors
///
/// Returns an I/O error if the listener cannot bind or the HTTP server fails.
pub async fn start_server(host: &str, port: u16, db_path: &str) -> Result<(), std::io::Error> {
    let app = router(db_path);

    let address = format!("{host}:{port}");

    let listener = tokio::net::TcpListener::bind(&address).await?;

    println!("Listening on http://{address}");

    axum::serve(listener, app).await
}
