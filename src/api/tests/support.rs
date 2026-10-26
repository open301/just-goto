//! Shared fixtures and request helpers for API route tests.

use std::{path::PathBuf, sync::OnceLock};

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use rusqlite::Connection;
use serde_json::Value;
use tower::ServiceExt;

use crate::{
    api::server::router,
    db::{migrations::run_migrations, open_database::open_database, tables},
    utils::{passwords::hash_password, tokens::generate_session_token},
};

/// Known password used to seed and authenticate the isolated test accounts.
pub(super) const PASSWORD: &str = "original-password";
/// Protected methods and concrete fixture paths, shared with the OpenAPI coverage checks.
pub(super) const PROTECTED_ROUTES: &[(&str, &str)] = &[
    // Authentication: current user and logout.
    ("GET", "/api/v1/auth/me"),
    ("POST", "/api/v1/auth/logout"),
    // Users: collection, individual accounts, and password changes.
    ("GET", "/api/v1/users"),
    ("POST", "/api/v1/users"),
    ("GET", "/api/v1/users/3"),
    ("PATCH", "/api/v1/users/3"),
    ("DELETE", "/api/v1/users/3"),
    ("PUT", "/api/v1/users/3/password"),
    // Redirects: collection, individual redirects, and CSV import.
    ("GET", "/api/v1/redirects"),
    ("POST", "/api/v1/redirects"),
    ("GET", "/api/v1/redirects/1"),
    ("PATCH", "/api/v1/redirects/1"),
    ("DELETE", "/api/v1/redirects/1"),
    ("POST", "/api/v1/redirects/import"),
];

/// A router backed by a temporary database containing known users and sessions.
pub(super) struct TestApi {
    /// Temporary directory containing the fixture database and its auxiliary SQLite files.
    directory: PathBuf,
    /// Real application router configured to use the isolated fixture database.
    pub(super) app: Router,
}

/// Builds fixtures and sends requests without starting a network listener.
impl TestApi {
    /// Creates and migrates an isolated database, then seeds users with IDs 1–5:
    /// admin, editor, viewer, another viewer, and a user without roles.
    ///
    /// Each username is also its valid test token. The fixture additionally includes
    /// an expired viewer session and an enabled redirect created by the admin.
    ///
    /// # Panics
    ///
    /// Panics if random token generation, temporary-directory creation, path conversion, migrations, password hashing, or fixture seeding fails.
    pub(super) fn new() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "justgoto-api-{}",
            generate_session_token().unwrap()
        ));
        std::fs::create_dir(&directory).unwrap();
        let db_path = directory.join("test.sqlite3");
        let mut conn = open_database(db_path.to_str().unwrap()).unwrap();
        run_migrations(&mut conn).unwrap();

        static PASSWORD_HASH: OnceLock<String> = OnceLock::new();
        let password_hash = PASSWORD_HASH.get_or_init(|| hash_password(PASSWORD).unwrap());
        for (username, role) in [
            ("admin", Some(tables::user_roles::Role::Admin)),
            ("editor", Some(tables::user_roles::Role::Editor)),
            ("viewer", Some(tables::user_roles::Role::Viewer)),
            ("other", Some(tables::user_roles::Role::Viewer)),
            ("unassigned", None),
        ] {
            let id = tables::users::create_user_account(&conn, username, password_hash).unwrap();
            if let Some(role) = role {
                tables::user_roles::add_user_role(&conn, id, role).unwrap();
            }
            let expires_at = conn
                .query_row("SELECT unixepoch() + 86400", [], |row| row.get(0))
                .unwrap();
            tables::sessions::create_session(&conn, id, username, expires_at).unwrap();
        }
        tables::sessions::create_session(&conn, 3, "expired", 1).unwrap();
        conn.execute(
            "INSERT INTO redirects (slug, destination, created_by) VALUES (?1, ?2, 1)",
            ("existing", "https://example.com/existing"),
        )
        .unwrap();

        Self {
            app: router(db_path.to_str().unwrap()),
            directory,
        }
    }

    /// Opens the fixture's database for inspecting results or arranging failure cases.
    ///
    /// # Panics
    ///
    /// Panics if the fixture database path is not valid UTF-8 or the database connection cannot be opened.
    pub(super) fn conn(&self) -> Connection {
        open_database(self.directory.join("test.sqlite3").to_str().unwrap()).unwrap()
    }

    /// Uploads CSV through POST /api/v1/redirects/import using the same multipart file field as clients.
    ///
    /// # Panics
    ///
    /// Panics if the multipart request cannot be built or the route does not return a readable JSON response.
    pub(super) async fn import_csv(&self, token: &str, csv: &str) -> (StatusCode, Value) {
        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/redirects/import")
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "multipart/form-data; boundary=test-boundary")
            .body(Body::from(format!(
                "--test-boundary\r\nContent-Disposition: form-data; name=\"file\"; filename=\"links.csv\"\r\nContent-Type: text/csv\r\n\r\n{csv}\r\n--test-boundary--\r\n"
            )))
            .unwrap();
        json_response(&self.app, request).await
    }

    /// Sends JSON through the real router, optionally adding a bearer token.
    /// Returns the HTTP status and parsed JSON response for assertions.
    ///
    /// # Panics
    ///
    /// Panics if request metadata cannot be encoded or the route does not return a readable JSON response.
    pub(super) async fn request(
        &self,
        method: &str,
        path: &str,
        token: Option<&str>,
        payload: Value,
    ) -> (StatusCode, Value) {
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header("content-type", "application/json");
        if let Some(token) = token {
            request = request.header("authorization", format!("Bearer {token}"));
        }
        json_response(
            &self.app,
            request.body(Body::from(payload.to_string())).unwrap(),
        )
        .await
    }
}

/// Cleans up the SQLite database and its auxiliary files when the fixture goes out of scope.
impl Drop for TestApi {
    /// Removes the fixture's temporary directory; a cleanup failure fails the test.
    ///
    /// # Panics
    ///
    /// Panics if the temporary fixture directory and its database files cannot be removed.
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.directory).unwrap();
    }
}

/// Executes one request, requires a JSON response, and reads at most 1 MiB of its body.
///
/// # Panics
///
/// Panics if routing fails, the content type is not JSON, the body exceeds 1 MiB or cannot be read, or its JSON cannot be parsed.
pub(super) async fn json_response(app: &Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    assert_eq!(response.headers()["content-type"], "application/json");
    let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    (status, serde_json::from_slice(&body).unwrap())
}

/// Requests a public route and fails if it unexpectedly requires a database.
///
/// # Panics
///
/// Panics if the GET request cannot be built or the router cannot serve it.
pub(super) async fn public_get(path: &str) -> axum::response::Response {
    router("/dev/null/not-a-database")
        .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
        .await
        .unwrap()
}
