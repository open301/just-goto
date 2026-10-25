//! Checks the served documentation against the public route and authentication contracts.

use std::collections::BTreeSet;

use axum::{body::to_bytes, http::StatusCode};
use serde_json::{Value, json};

use super::support::{PROTECTED_ROUTES, public_get};

/// Reads the public JSON specification exactly as an API client or Swagger UI would.
///
/// # Panics
///
/// Panics if the public specification cannot be served or its status, content type, body size, or JSON document is invalid.
async fn served_document() -> Value {
    let response = public_get("/api-docs/openapi.json").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "application/json");
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

/// Requires every existing operation to be documented with the appropriate authentication policy.
///
/// # Panics
///
/// Panics if a route is missing from the specification, operation IDs are duplicated, or documented authentication differs from the router contract.
#[tokio::test]
async fn all_routes_are_documented_with_operation_level_bearer_authentication() {
    let document = served_document().await;
    assert_eq!(document["openapi"], "3.1.0");
    assert_eq!(document["info"]["title"], "JustGoto API");
    assert!(document.get("security").is_none());
    assert_eq!(
        document["components"]["securitySchemes"]["bearer_auth"],
        json!({"type": "http", "scheme": "bearer"})
    );

    let mut expected = BTreeSet::new();
    let mut operation_ids = BTreeSet::new();
    for &(method, path) in PROTECTED_ROUTES {
        let path = path
            .replace("/users/3", "/users/{id}")
            .replace("/redirects/1", "/redirects/{id}");
        expected.insert((path.to_string(), method.to_ascii_lowercase()));
        let operation = &document["paths"][&path][method.to_ascii_lowercase()];
        assert_eq!(
            operation["security"],
            json!([{"bearer_auth": []}]),
            "{method} {path}"
        );
        assert!(operation["responses"]["401"].is_object(), "{method} {path}");
    }
    for (method, path) in [
        ("get", "/"),
        ("get", "/health"),
        ("get", "/{slug}"),
        ("post", "/api/v1/auth/login"),
    ] {
        expected.insert((path.to_string(), method.to_string()));
        let operation = &document["paths"][path][method];
        assert!(operation.is_object(), "{method} {path}");
        assert!(operation.get("security").is_none(), "{method} {path}");
    }
    let actual: BTreeSet<_> = document["paths"]
        .as_object()
        .unwrap()
        .iter()
        .flat_map(|(path, item)| {
            item.as_object()
                .unwrap()
                .iter()
                .map(|(method, operation)| {
                    assert!(
                        operation_ids.insert(operation["operationId"].as_str().unwrap().to_owned())
                    );
                    (path.clone(), method.clone())
                })
                .collect::<Vec<_>>()
        })
        .collect();
    assert_eq!(actual, expected);
    for (method, description) in [
        ("delete", "The final admin account cannot be deleted"),
        (
            "patch",
            "Username already exists or the final admin account cannot be demoted",
        ),
    ] {
        assert_eq!(
            document["paths"]["/api/v1/users/{id}"][method]["responses"]["409"]["description"],
            description,
        );
    }
}

/// Follows every local schema reference to detect missing or incorrectly named component schemas.
///
/// # Panics
///
/// Panics if a schema reference is not local or cannot be resolved in the supplied document.
fn check_references(value: &Value, document: &Value) {
    match value {
        Value::Object(object) => {
            if let Some(reference) = object.get("$ref").and_then(Value::as_str) {
                let pointer = reference.strip_prefix('#').expect("local schema reference");
                assert!(
                    document.pointer(pointer).is_some(),
                    "Unresolved reference: {reference}"
                );
            }
            for value in object.values() {
                check_references(value, document);
            }
        },
        Value::Array(values) => {
            for value in values {
                check_references(value, document);
            }
        },
        _ => {},
    }
}

/// Checks schema references, password handling, and the binary multipart field used by the file picker.
///
/// # Panics
///
/// Panics if schemas contain unresolved references or password and multipart-upload metadata differs from the documented contract.
#[tokio::test]
async fn schemas_resolve_and_multipart_uploads_have_a_binary_file_field() {
    let document = served_document().await;
    check_references(&document, &document);
    let schemas = &document["components"]["schemas"];
    assert_eq!(
        schemas["CreateUserRequest"]["properties"]["password"]["writeOnly"],
        true
    );
    assert_eq!(
        schemas["LoginRequest"]["properties"]["password"]["format"],
        "password"
    );
    assert_eq!(schemas["ErrorResponse"]["required"], json!(["message"]));
    assert_eq!(schemas["CsvUpload"]["required"], json!(["file"]));
    assert_eq!(schemas["CsvUpload"]["properties"]["file"]["type"], "string");
    assert_eq!(
        schemas["CsvUpload"]["properties"]["file"]["format"],
        "binary"
    );
    let import = &document["paths"]["/api/v1/redirects/import"]["post"];
    assert_eq!(
        import["requestBody"]["content"]["multipart/form-data"]["schema"]["$ref"],
        "#/components/schemas/CsvUpload"
    );
    for status in ["201", "422"] {
        assert_eq!(
            import["responses"][status]["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/ImportLinksResponse"
        );
    }
}

/// Ensures path and query names match the router and the handler's actual filters.
///
/// # Panics
///
/// Panics if documented parameter names, locations, or required flags differ from the router and handler contracts.
#[tokio::test]
async fn path_and_query_parameters_match_the_routes() {
    let document = served_document().await;
    for path in [
        "/api/v1/users/{id}",
        "/api/v1/users/{id}/password",
        "/api/v1/redirects/{id}",
        "/{slug}",
    ] {
        for operation in document["paths"][path].as_object().unwrap().values() {
            let parameters = operation["parameters"].as_array().unwrap();
            assert_eq!(parameters.len(), 1, "{path}");
            assert_eq!(
                parameters[0]["name"],
                if path == "/{slug}" { "slug" } else { "id" }
            );
            assert_eq!(parameters[0]["in"], "path");
            assert_eq!(parameters[0]["required"], true);
        }
    }
    for (path, names) in [
        ("/api/v1/users", vec!["limit", "offset", "role", "username"]),
        (
            "/api/v1/redirects",
            vec!["created_by", "enabled", "limit", "offset", "slug"],
        ),
    ] {
        let parameters = document["paths"][path]["get"]["parameters"]
            .as_array()
            .unwrap();
        let mut actual = Vec::new();
        for parameter in parameters {
            assert_eq!(parameter["in"], "query");
            assert_eq!(parameter["required"], false);
            actual.push(parameter["name"].as_str().unwrap());
        }
        actual.sort();
        assert_eq!(actual, names);
    }
}

/// Confirms Swagger UI and its local JavaScript/CSS load without authentication or SQLite.
///
/// # Panics
///
/// Panics if Swagger UI redirects incorrectly, an embedded asset cannot be read, or its content and initialization settings are unexpected.
#[tokio::test]
async fn swagger_ui_redirects_and_serves_embedded_assets() {
    let redirect = public_get("/swagger-ui").await;
    assert_eq!(redirect.status(), StatusCode::SEE_OTHER);
    assert_eq!(redirect.headers()["location"], "/swagger-ui/");
    for (path, content_type, marker) in [
        ("/swagger-ui/", "text/html", "Swagger UI"),
        ("/swagger-ui/swagger-ui.css", "text/css", ".swagger-ui"),
        (
            "/swagger-ui/swagger-ui-bundle.js",
            "javascript",
            "SwaggerUIBundle",
        ),
        (
            "/swagger-ui/swagger-ui-standalone-preset.js",
            "javascript",
            "SwaggerUIStandalonePreset",
        ),
        (
            "/swagger-ui/swagger-initializer.js",
            "javascript",
            "/api-docs/openapi.json",
        ),
    ] {
        let response = public_get(path).await;
        assert_eq!(response.status(), StatusCode::OK, "{path}");
        assert!(
            response.headers()["content-type"]
                .to_str()
                .unwrap()
                .contains(content_type),
            "{path}"
        );
        let body = to_bytes(response.into_body(), 8 * 1024 * 1024)
            .await
            .unwrap();
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert!(text.contains(marker), "{path}");
        if path.ends_with("swagger-initializer.js") {
            let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
            assert!(compact.contains("\"validatorUrl\":\"none\""));
        }
    }
}
