//! Generates OpenAPI from the route annotations and serves bundled Swagger UI assets.

use axum::Router;
use utoipa::{
    Modify, OpenApi,
    openapi::{
        OpenApi as Document,
        security::{Http, HttpAuthScheme, SecurityScheme},
    },
};
use utoipa_swagger_ui::{Config, SwaggerUi};

use crate::api::{request::ApiState, routes};

/// Top-level API metadata and public service operations.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "JustGoto API",
        description = "Manage URL redirects and user accounts. Log in with /api/v1/auth/login, then use the returned opaque session token with Bearer authentication. Sessions expire after 24 hours. User management requires admin access; redirect writes and imports require editor or admin access."
    ),
    paths(crate::api::frontend::index),
    modifiers(&BearerAuthentication),
    tags(
        (name = "Service", description = "Public service information and health"),
        (name = "Authentication", description = "Login, logout, and the current user"),
        (name = "Users", description = "User management and password changes"),
        (name = "Redirects", description = "Redirect management and public resolution"),
        (name = "Imports", description = "Bulk redirect imports from CSV")
    )
)]
struct ApiDoc;

/// Registers the opaque bearer-token scheme used by protected operations.
struct BearerAuthentication;

/// Adds security metadata without imposing authentication on public operations.
impl Modify for BearerAuthentication {
    /// Makes Swagger UI's Authorize control send the session token in the header.
    fn modify(&self, openapi: &mut Document) {
        openapi
            .components
            .get_or_insert_with(Default::default)
            .add_security_scheme(
                "bearer_auth",
                SecurityScheme::Http(Http::new(HttpAuthScheme::Bearer)),
            );
    }
}

/// Combines each route module's operations and schemas into one OpenAPI document.
pub fn document() -> Document {
    let mut document = ApiDoc::openapi();
    document.merge(routes::auth::ApiDoc::openapi());
    document.merge(routes::users::ApiDoc::openapi());
    document.merge(routes::redirects::document());
    document.merge(routes::health::ApiDoc::openapi());
    document
}

/// Serves the spec and embedded UI without authentication or a database connection.
pub fn routes() -> Router<ApiState> {
    SwaggerUi::new("/swagger-ui")
        .url("/api-docs/openapi.json", document())
        .config(Config::default().validator_url("none"))
        .into()
}
