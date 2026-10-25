//! /api/v1/redirects, CSV imports, and public /{slug}: permissions and resolution.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::json;
use tower::ServiceExt;

use super::support::TestApi;
use crate::db::tables;

/// Allows authenticated reads regardless of role, restricts writes to editors or
/// admins, and checks that new redirects record the authenticated creator.
///
/// # Panics
///
/// Panics if fixture setup fails or redirect read permissions, write permissions, or stored creator IDs differ from the contract.
#[tokio::test]
async fn redirect_read_and_write_permissions_are_preserved() {
    let api = TestApi::new();
    for (user_id, token) in [
        (1, "admin"),
        (2, "editor"),
        (3, "viewer"),
        (5, "unassigned"),
    ] {
        for path in ["/api/v1/redirects", "/api/v1/redirects/1"] {
            assert_eq!(
                api.request("GET", path, Some(token), json!({})).await.0,
                StatusCode::OK
            );
        }
        let (status, body) = api
            .request(
                "POST",
                "/api/v1/redirects",
                Some(token),
                json!({"slug": token, "destination": "https://example.com"}),
            )
            .await;
        if matches!(token, "admin" | "editor") {
            assert_eq!(status, StatusCode::CREATED);
            assert_eq!(body["redirect"]["created_by"], user_id);
            let id = body["redirect"]["id"].as_i64().unwrap();
            let path = format!("/api/v1/redirects/{id}");
            assert_eq!(
                api.request("PATCH", &path, Some(token), json!({"is_enabled": false}))
                    .await
                    .0,
                StatusCode::OK
            );
            assert_eq!(
                api.request("DELETE", &path, Some(token), json!({})).await.0,
                StatusCode::OK
            );
        } else {
            assert_eq!(status, StatusCode::FORBIDDEN);
            for method in ["PATCH", "DELETE"] {
                assert_eq!(
                    api.request(method, "/api/v1/redirects/1", Some(token), json!({}))
                        .await
                        .0,
                    StatusCode::FORBIDDEN
                );
            }
        }
    }
}

/// Exercises multipart CSV uploads through the real route to check editor-or-admin
/// access and the creator ID saved on each imported redirect.
///
/// # Panics
///
/// Panics if fixture setup fails, upload permissions are incorrect, or an imported redirect has the wrong creator.
#[tokio::test]
async fn imports_use_editor_or_admin_access_and_record_the_authenticated_creator() {
    let api = TestApi::new();
    for (token, user_id) in [
        ("admin", 1),
        ("editor", 2),
        ("viewer", 3),
        ("unassigned", 5),
    ] {
        let (status, body) = api
            .import_csv(
                token,
                &format!("slug,destination\nimport-{token},https://example.com"),
            )
            .await;
        if matches!(token, "admin" | "editor") {
            assert_eq!(status, StatusCode::CREATED);
            assert_eq!(body["imported"], 1);
            let redirect = tables::redirects::get_enabled_redirect_by_slug(
                &api.conn(),
                &format!("import-{token}"),
            )
            .unwrap()
            .unwrap();
            assert_eq!(redirect.created_by, Some(user_id));
        } else {
            assert_eq!(status, StatusCode::FORBIDDEN);
        }
    }
}

/// Resolves a public slug without a token and checks its redirect status and destination.
///
/// # Panics
///
/// Panics if the public slug does not resolve to its stored status and destination without authentication.
#[tokio::test]
async fn public_redirect_resolution_uses_the_configured_database_without_authentication() {
    let api = TestApi::new();
    let response = api
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/existing")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FOUND);
    assert_eq!(
        response.headers()["location"],
        "https://example.com/existing"
    );
}

/// Service route names remain reserved across redirect creation, updates, and CSV imports.
///
/// # Panics
///
/// Panics if a JSON write or CSV import accepts a reserved slug or returns an unexpected validation message.
#[tokio::test]
async fn service_slugs_are_reserved_for_creates_updates_and_imports() {
    let api = TestApi::new();
    for slug in [
        "health",
        "auth",
        "users",
        "redirects",
        "swagger-ui",
        "assets",
        "api",
        "api-docs",
    ] {
        for (method, path) in [
            ("POST", "/api/v1/redirects"),
            ("PATCH", "/api/v1/redirects/1"),
        ] {
            let (status, body) = api
                .request(
                    method,
                    path,
                    Some("admin"),
                    json!({"slug": slug, "destination": "https://example.com"}),
                )
                .await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{method} {slug}");
            assert_eq!(body["message"], "Slug is reserved by the API");
        }
        let (status, body) = api
            .import_csv(
                "admin",
                &format!("slug,destination\n{slug},https://example.com"),
            )
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["imported"], 0);
        assert_eq!(body["errors"][0]["message"], "Slug is reserved by the API");
    }
}

/// POST, PATCH, and CSV imports reject the same invalid redirect fields with the same messages.
///
/// # Panics
///
/// Panics if creation, updates, and CSV imports disagree on invalid redirect fields or their validation messages.
#[tokio::test]
async fn redirect_validation_is_consistent_across_writes_and_imports() {
    let api = TestApi::new();
    let cases = [
        (
            "slug",
            String::new(),
            "Slug must be 1-128 characters using letters, numbers, '-' or '_'",
        ),
        (
            "slug",
            "invalid/slug".to_string(),
            "Slug must be 1-128 characters using letters, numbers, '-' or '_'",
        ),
        (
            "slug",
            "é".to_string(),
            "Slug must be 1-128 characters using letters, numbers, '-' or '_'",
        ),
        (
            "slug",
            "a".repeat(129),
            "Slug must be 1-128 characters using letters, numbers, '-' or '_'",
        ),
        ("slug", "health".to_string(), "Slug is reserved by the API"),
        (
            "destination",
            "ftp://example.com".to_string(),
            "Destination must be an absolute HTTP or HTTPS URL",
        ),
        (
            "destination",
            format!("https://example.com/{}", "a".repeat(2048)),
            "Destination must be an absolute HTTP or HTTPS URL",
        ),
        (
            "redirect_code",
            "303".to_string(),
            "Redirect code must be one of: [301, 302, 307, 308]",
        ),
    ];
    for (field, value, message) in cases {
        let mut payload = json!({"slug": "candidate", "destination": "https://example.com"});
        payload[field] = if field == "redirect_code" {
            json!(303)
        } else {
            json!(value)
        };
        for (method, path) in [
            ("POST", "/api/v1/redirects"),
            ("PATCH", "/api/v1/redirects/1"),
        ] {
            let (status, body) = api
                .request(method, path, Some("admin"), payload.clone())
                .await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{method}: {field}={value}");
            assert_eq!(body, json!({"message": message}));
        }
        let csv = format!(
            "slug,destination,redirect_code\n{},{},{}",
            payload["slug"].as_str().unwrap(),
            payload["destination"].as_str().unwrap(),
            payload
                .get("redirect_code")
                .map_or(String::new(), |code| code.to_string()),
        );
        let (status, body) = api.import_csv("admin", &csv).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{field}={value}");
        assert_eq!(body["imported"], 0);
        assert_eq!(body["errors"][0]["row"], 1);
        assert_eq!(body["errors"][0]["message"], message);
    }
}

/// Valid boundary values, default statuses, and mixed CSV results survive validation consolidation.
///
/// # Panics
///
/// Panics if valid boundary values, default fields, supported statuses, or mixed CSV import results differ from the contract.
#[tokio::test]
async fn redirect_boundaries_defaults_and_partial_imports_are_preserved() {
    let api = TestApi::new();
    let slug = "a".repeat(128);
    let destination = format!("https://example.com/{}", "a".repeat(2028));
    assert_eq!(destination.len(), 2048);
    let (status, body) = api
        .request(
            "POST",
            "/api/v1/redirects",
            Some("admin"),
            json!({"slug": slug, "destination": destination}),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["redirect"]["redirect_code"], 302);
    assert_eq!(body["redirect"]["is_enabled"], true);
    let path = format!("/api/v1/redirects/{}", body["redirect"]["id"]);
    for code in [301, 302, 307, 308] {
        let (status, body) = api
            .request(
                "PATCH",
                &path,
                Some("admin"),
                json!({"redirect_code": code}),
            )
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["redirect"]["redirect_code"], code);
    }
    let (status, body) = api.import_csv(
        "editor",
        "slug,destination,redirect_code,is_enabled\nimported,https://example.com,,\nexisting,https://example.com,301,true\nbad,ftp://example.com,302,true\ndisabled,https://example.com,308,no",
    ).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["total_rows"], 4);
    assert_eq!(body["imported"], 2);
    assert_eq!(body["skipped"], 2);
    assert_eq!(body["errors"][0]["row"], 2);
    assert_eq!(body["errors"][0]["message"], "Slug already exists");
    assert_eq!(body["errors"][1]["row"], 3);
    let conn = api.conn();
    let imported = tables::redirects::get_enabled_redirect_by_slug(&conn, "imported")
        .unwrap()
        .unwrap();
    assert_eq!(imported.redirect_code, 302);
    assert_eq!(imported.created_by, Some(2));
    assert!(
        tables::redirects::get_enabled_redirect_by_slug(&conn, "disabled")
            .unwrap()
            .is_none()
    );
}

/// Invalid authorities, whitespace, and header injection are rejected before any redirect write or CSV import.
///
/// # Panics
///
/// Panics if an invalid destination is stored, a rejected update changes the existing destination, or CSV errors differ from JSON validation.
#[tokio::test]
async fn unsafe_and_hostless_destinations_are_rejected_at_write_time() {
    let api = TestApi::new();
    for destination in [
        "https://",
        "http://",
        "https:///path",
        "https://?query",
        "https://#fragment",
        "https://:443/path",
        "https://user@/path",
        "https://[invalid]/path",
        "https://[::1/path",
        "https://example.com:not-a-port",
        "https://example.com:0/path",
        "https://example.com:65536",
        "https://example.com:",
        "https://example..com",
        "https://-invalid.example",
        "https://.",
        "https://example.com/a b",
        "https://example.com/a\tb",
        "https://example.com/a\nb",
        "https://example.com/a\r\nX-Injected:yes",
        "https://example.com/a\0b",
        "https://example.com/a\u{7f}b",
        "https://example.com/a\u{a0}b",
        "https://example.com/a\u{85}b",
        r"https://example.com\path",
    ] {
        for (method, path) in [
            ("POST", "/api/v1/redirects"),
            ("PATCH", "/api/v1/redirects/1"),
        ] {
            assert_eq!(
                api.request(
                    method,
                    path,
                    Some("admin"),
                    json!({"slug": "candidate", "destination": destination})
                )
                .await,
                (
                    StatusCode::BAD_REQUEST,
                    json!({"message": "Destination must be an absolute HTTP or HTTPS URL"})
                ),
                "{method} {destination:?}"
            );
        }
        let mut csv = csv::Writer::from_writer(Vec::new());
        csv.write_record(["slug", "destination"]).unwrap();
        csv.write_record(["candidate", destination]).unwrap();
        let csv = String::from_utf8(csv.into_inner().unwrap()).unwrap();
        let (status, body) = api.import_csv("admin", &csv).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{destination:?}");
        assert_eq!(body["imported"], 0);
        assert_eq!(
            body["errors"][0]["message"],
            "Destination must be an absolute HTTP or HTTPS URL"
        );
    }
    for destination in [
        " https://example.com",
        "https://example.com ",
        "https://example.com\r\n",
    ] {
        for (method, path) in [
            ("POST", "/api/v1/redirects"),
            ("PATCH", "/api/v1/redirects/1"),
        ] {
            assert_eq!(
                api.request(
                    method,
                    path,
                    Some("admin"),
                    json!({"slug": "candidate", "destination": destination})
                )
                .await
                .0,
                StatusCode::BAD_REQUEST
            );
        }
    }
    let conn = api.conn();
    assert!(!tables::redirects::check_slug_exists(&conn, "candidate").unwrap());
    assert_eq!(
        tables::redirects::get_redirect_by_id(&conn, 1)
            .unwrap()
            .unwrap()
            .destination,
        "https://example.com/existing"
    );
}

/// Valid destinations preserve their exact Location value, including ports, IPv6, queries, fragments, and percent-encoded characters.
///
/// # Panics
///
/// Panics if a valid URL is rejected or public resolution does not return its original Location header.
#[tokio::test]
async fn valid_http_destinations_resolve_without_header_errors() {
    let api = TestApi::new();
    for (index, destination) in [
        "http://localhost:8080/path",
        "https://127.0.0.1:443/",
        "https://[2001:db8::1]:8443/path",
        "https://example.com?key=value#fragment",
        "https://user:password@example.com/path",
        "https://example.com./path",
        "https://example.com/a%20b?encoded=%0D%0A",
    ]
    .into_iter()
    .enumerate()
    {
        let slug = format!("valid-{index}");
        assert_eq!(
            api.request(
                "POST",
                "/api/v1/redirects",
                Some("admin"),
                json!({"slug": slug, "destination": destination})
            )
            .await
            .0,
            StatusCode::CREATED
        );
        let response = api
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/{slug}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FOUND);
        assert_eq!(response.headers()["location"], destination);
    }
}

/// Slug substring filters escape literal LIKE metacharacters, including legacy values outside today's slug policy.
///
/// # Panics
///
/// Panics if fixture insertion fails or a literal search matches a wildcard decoy or loses an escaped character.
#[tokio::test]
async fn redirect_search_escapes_like_wildcards() {
    let api = TestApi::new();
    let conn = api.conn();
    for slug in [
        "literal_slug",
        "literal%slug",
        r"literal\slug",
        "literalXslug",
        r"combo%\_tail",
    ] {
        conn.execute("INSERT INTO redirects (slug, destination, created_by) VALUES (?1, 'https://example.com', 1)", [slug]).unwrap();
    }
    for (query, expected) in [
        ("literal%5F", "literal_slug"),
        ("literal%25", "literal%slug"),
        ("literal%5C", r"literal\slug"),
        ("%25%5C%5F", r"combo%\_tail"),
    ] {
        let (status, body) = api
            .request(
                "GET",
                &format!("/api/v1/redirects?slug={query}"),
                Some("viewer"),
                json!({}),
            )
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["redirects"].as_array().unwrap().len(), 1, "{body}");
        assert_eq!(body["redirects"][0]["slug"], expected);
    }
    assert_eq!(
        api.request(
            "GET",
            "/api/v1/redirects?slug=literal",
            Some("viewer"),
            json!({})
        )
        .await
        .1["redirects"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
}
