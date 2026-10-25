//! /api/v1/users routes: account permissions and password changes.

use axum::http::StatusCode;
use serde_json::json;

use super::support::{PASSWORD, TestApi};

/// A failed role assignment rolls back the new account, whether a trigger rejects it or the role no longer exists.
///
/// # Panics
///
/// Panics if arranging a database failure fails, creation changes its error contract, or a failed creation leaves an orphaned account.
#[tokio::test]
async fn account_creation_rolls_back_when_role_assignment_fails() {
    for failure in [
        "CREATE TRIGGER reject_role BEFORE INSERT ON user_roles BEGIN SELECT RAISE(ABORT, 'role assignment rejected'); END;",
        "DELETE FROM roles WHERE id = 3;",
    ] {
        let api = TestApi::new();
        let conn = api.conn();
        conn.execute_batch(failure).unwrap();
        assert_eq!(
            api.request(
                "POST",
                "/api/v1/users",
                Some("admin"),
                json!({"username": "no-orphan", "password": PASSWORD, "role": "viewer"})
            )
            .await,
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"message": "Failed to assign user role"})
            ),
        );
        assert!(!crate::db::tables::users::check_username_exists(&conn, "no-orphan").unwrap());
    }
}

/// Concurrent account creations serialize the username check and commit exactly one account with its role.
///
/// # Panics
///
/// Panics if fixture setup or thread execution fails, duplicate creation is not reported, or the saved account lacks its role.
#[test]
fn concurrent_account_creation_reports_a_duplicate_without_an_orphan() {
    use crate::db::tables::{
        user_roles::Role,
        users::{self, CreateUserError},
    };
    let api = TestApi::new();
    let barrier = std::sync::Barrier::new(2);
    let connections = [api.conn(), api.conn()];
    let results = std::thread::scope(|scope| {
        let handles: Vec<_> = connections
            .into_iter()
            .map(|mut conn| {
                let barrier = &barrier;
                scope.spawn(move || {
                    barrier.wait();
                    users::create_user_with_role(&mut conn, "concurrent", "unused", Role::Viewer)
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, Err(CreateUserError::UsernameExists)))
            .count(),
        1
    );
    let count: i64 = api.conn().query_row(
        "SELECT COUNT(*) FROM users INNER JOIN user_roles ON users.id = user_roles.user_id WHERE users.username = 'concurrent' AND user_roles.role_id = 3",
        [], |row| row.get(0),
    ).unwrap();
    assert_eq!(count, 1);
}

/// Listing paginates accounts before expanding all their roles, preserving user-ID and role-ID order and roleless accounts.
///
/// # Panics
///
/// Panics if fixture setup fails or pagination, filtering, role order, or roleless-account handling changes.
#[tokio::test]
async fn user_listing_preserves_pagination_and_role_order() {
    let api = TestApi::new();
    api.conn()
        .execute_batch("INSERT INTO user_roles (user_id, role_id) VALUES (1, 3), (1, 2), (3, 2);")
        .unwrap();
    for (query, expected) in [
        (
            "?limit=2",
            json!([[1, ["admin", "editor", "viewer"]], [2, ["editor"]]]),
        ),
        (
            "?limit=2&offset=1",
            json!([[2, ["editor"]], [3, ["editor", "viewer"]]]),
        ),
        (
            "?role=VIEWER&limit=2",
            json!([
                [1, ["admin", "editor", "viewer"]],
                [3, ["editor", "viewer"]]
            ]),
        ),
        (
            "?role=viewer&limit=1&offset=1",
            json!([[3, ["editor", "viewer"]]]),
        ),
        ("?username=unassigned", json!([[5, []]])),
        ("?offset=5", json!([])),
    ] {
        let (status, body) = api
            .request(
                "GET",
                &format!("/api/v1/users{query}"),
                Some("admin"),
                json!({}),
            )
            .await;
        assert_eq!(status, StatusCode::OK);
        let actual: Vec<_> = body["users"]
            .as_array()
            .unwrap()
            .iter()
            .map(|user| json!([user["id"], user["roles"]]))
            .collect();
        assert_eq!(json!(actual), expected, "{query}");
    }
}

/// Listing preserves database-error responses when role joins or decoding fail and authorization-error responses when assignments cannot be read.
///
/// # Panics
///
/// Panics if arranging a failure fails, a role-query failure is swallowed, or the HTTP 500 response contract changes.
#[tokio::test]
async fn user_listing_reports_role_query_failures() {
    for (failure, expected) in [
        (
            "PRAGMA foreign_keys = OFF; DROP TABLE roles;",
            json!({"message": "Database error", "users": []}),
        ),
        (
            "UPDATE roles SET name = x'80' WHERE id = 3;",
            json!({"message": "Database error", "users": []}),
        ),
        (
            "DROP TABLE user_roles;",
            json!({"message": "Authorization check failed"}),
        ),
    ] {
        let api = TestApi::new();
        let conn = api.conn();
        conn.execute_batch(failure).unwrap();
        assert!(crate::db::tables::users::list_users(&conn, None, None, 50, 0).is_err());
        assert_eq!(
            api.request("GET", "/api/v1/users", Some("admin"), json!({}))
                .await,
            (StatusCode::INTERNAL_SERVER_ERROR, expected)
        );
    }
}

/// POST, PATCH, GET, and DELETE /api/v1/users preserve the admin account-management workflow.
///
/// # Panics
///
/// Panics if account creation, validation, role replacement, username trimming, or deletion differs from the expected admin workflow.
#[tokio::test]
async fn admins_can_create_update_and_delete_accounts() {
    let api = TestApi::new();
    let payload = json!({"username": "new-user", "password": PASSWORD, "role": "viewer"});
    assert_eq!(
        api.request("POST", "/api/v1/users", Some("admin"), payload.clone())
            .await,
        (
            StatusCode::CREATED,
            json!({"message": "User created successfully"})
        ),
    );
    assert_eq!(
        api.request("POST", "/api/v1/users", Some("admin"), payload)
            .await,
        (
            StatusCode::CONFLICT,
            json!({"message": "Username already exists"})
        ),
    );
    let (status, body) = api
        .request(
            "GET",
            "/api/v1/users?username=new-user",
            Some("admin"),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let path = format!("/api/v1/users/{}", body["users"][0]["id"]);
    for (payload, status, message) in [
        (
            json!({}),
            StatusCode::BAD_REQUEST,
            "At least one field is required",
        ),
        (
            json!({"username": " "}),
            StatusCode::BAD_REQUEST,
            "Username cannot be empty",
        ),
        (
            json!({"username": "admin"}),
            StatusCode::CONFLICT,
            "Username already exists",
        ),
        (
            json!({"role": "owner"}),
            StatusCode::BAD_REQUEST,
            "Invalid role",
        ),
        (
            json!({"username": " renamed ", "role": "EDITOR"}),
            StatusCode::OK,
            "User updated successfully",
        ),
    ] {
        assert_eq!(
            api.request("PATCH", &path, Some("admin"), payload).await,
            (status, json!({"message": message})),
        );
    }
    let (status, body) = api.request("GET", &path, Some("admin"), json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["user"]["username"], "renamed");
    assert_eq!(body["user"]["roles"], json!(["editor"]));
    assert_eq!(
        api.request("DELETE", &path, Some("admin"), json!({}))
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        api.request("GET", &path, Some("admin"), json!({})).await,
        (StatusCode::NOT_FOUND, json!({"message": "User not found"})),
    );
}

/// Checks admin-only user management, self-or-admin access to user details, and
/// the existing protection against deleting the final admin account.
///
/// # Panics
///
/// Panics if account permissions differ from the role contract or the final administrator can be deleted.
#[tokio::test]
async fn user_management_requires_admin_and_user_details_allow_self_or_admin() {
    let api = TestApi::new();
    for &(method, path) in &[
        ("GET", "/api/v1/users"),
        ("POST", "/api/v1/users"),
        ("PATCH", "/api/v1/users/3"),
        ("DELETE", "/api/v1/users/3"),
    ] {
        for token in ["editor", "viewer", "unassigned"] {
            assert_eq!(
                api.request(method, path, Some(token), json!({})).await.0,
                StatusCode::FORBIDDEN,
                "{method} {path} as {token}"
            );
        }
    }
    assert_eq!(
        api.request("GET", "/api/v1/users", Some("admin"), json!({}))
            .await
            .0,
        StatusCode::OK
    );
    for (token, expected) in [
        ("admin", StatusCode::OK),
        ("viewer", StatusCode::OK),
        ("editor", StatusCode::FORBIDDEN),
        ("other", StatusCode::FORBIDDEN),
    ] {
        let (status, body) = api
            .request("GET", "/api/v1/users/3", Some(token), json!({}))
            .await;
        assert_eq!(status, expected);
        if status == StatusCode::OK {
            assert_eq!(body["user"]["username"], "viewer");
        }
    }
    assert_eq!(
        api.request("DELETE", "/api/v1/users/1", Some("admin"), json!({}))
            .await
            .0,
        StatusCode::CONFLICT
    );
}

/// Checks current-password and password-length validation, invalidates the old
/// session after a password change, and verifies login with the replacement password.
///
/// # Panics
///
/// Panics if password validation, session revocation, or login with the replacement password differs from the contract.
#[tokio::test]
async fn own_password_change_requires_current_password_and_revokes_sessions() {
    let api = TestApi::new();
    for (payload, expected) in [
        (
            json!({"new_password": "replacement-password"}),
            StatusCode::BAD_REQUEST,
        ),
        (
            json!({"current_password": "wrong", "new_password": "replacement-password"}),
            StatusCode::UNAUTHORIZED,
        ),
        (
            json!({"current_password": PASSWORD, "new_password": "short"}),
            StatusCode::BAD_REQUEST,
        ),
        (
            json!({"current_password": PASSWORD, "new_password": "replacement-password"}),
            StatusCode::OK,
        ),
    ] {
        assert_eq!(
            api.request("PUT", "/api/v1/users/3/password", Some("viewer"), payload)
                .await
                .0,
            expected
        );
    }
    assert_eq!(
        api.request("GET", "/api/v1/auth/me", Some("viewer"), json!({}))
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    let (status, body) = api
        .request(
            "POST",
            "/api/v1/auth/login",
            None,
            json!({"username": "viewer", "password": "replacement-password"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let token = body["token"].as_str().unwrap();
    assert_eq!(body["user"]["id"], 3);
    assert!(body["expires_at"].is_i64());
    let (status, body) = api
        .request("GET", "/api/v1/auth/me", Some(token), json!({}))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["user"]["roles"], json!(["viewer"]));
}

/// Rejects non-admin password resets on another account and checks that an admin
/// reset succeeds without the current password and revokes the target's session.
///
/// # Panics
///
/// Panics if password-reset permissions are incorrect or the target account retains its old session.
#[tokio::test]
async fn only_admin_can_reset_another_users_password_without_the_current_password() {
    let api = TestApi::new();
    for (token, expected) in [
        ("editor", StatusCode::FORBIDDEN),
        ("other", StatusCode::FORBIDDEN),
        ("admin", StatusCode::OK),
    ] {
        assert_eq!(
            api.request(
                "PUT",
                "/api/v1/users/3/password",
                Some(token),
                json!({"new_password": "replacement-password"})
            )
            .await
            .0,
            expected
        );
    }
    assert_eq!(
        api.request("GET", "/api/v1/auth/me", Some("viewer"), json!({}))
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
}

/// Final-admin conflicts preserve roles, usernames, and sessions while harmless updates and non-admin changes succeed.
///
/// # Panics
///
/// Panics if a final administrator can be demoted or deleted, a rejected update changes account data, or an allowed account change fails.
#[tokio::test]
async fn final_admin_is_preserved_across_account_mutations() {
    let api = TestApi::new();
    for role in ["editor", "viewer"] {
        assert_eq!(
            api.request(
                "PATCH",
                "/api/v1/users/1",
                Some("admin"),
                json!({"username": "should-not-change", "role": role}),
            )
            .await,
            (
                StatusCode::CONFLICT,
                json!({"message": "The final admin account cannot be demoted"})
            ),
        );
    }
    assert_eq!(
        api.request("DELETE", "/api/v1/users/1", Some("admin"), json!({}))
            .await,
        (
            StatusCode::CONFLICT,
            json!({"message": "The final admin account cannot be deleted"})
        ),
    );
    let (status, body) = api
        .request("GET", "/api/v1/users/1", Some("admin"), json!({}))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["user"]["username"], "admin");
    assert_eq!(body["user"]["roles"], json!(["admin"]));
    for payload in [
        json!({"username": " renamed-admin "}),
        json!({"role": "admin"}),
    ] {
        assert_eq!(
            api.request("PATCH", "/api/v1/users/1", Some("admin"), payload)
                .await
                .0,
            StatusCode::OK
        );
    }
    for role in ["editor", "viewer", "admin"] {
        assert_eq!(
            api.request(
                "PATCH",
                "/api/v1/users/2",
                Some("admin"),
                json!({"role": role})
            )
            .await
            .0,
            StatusCode::OK
        );
    }
    assert_eq!(
        api.request(
            "PATCH",
            "/api/v1/users/1",
            Some("admin"),
            json!({"role": "viewer"})
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        api.request("DELETE", "/api/v1/users/1", Some("editor"), json!({}))
            .await
            .0,
        StatusCode::OK
    );
    for method in ["PATCH", "DELETE"] {
        assert_eq!(
            api.request(
                method,
                "/api/v1/users/999",
                Some("editor"),
                json!({"role": "viewer"})
            )
            .await,
            (StatusCode::NOT_FOUND, json!({"message": "User not found"}))
        );
    }
}

/// Concurrent database mutations on separate connections cannot demote or delete both remaining administrators.
///
/// # Panics
///
/// Panics if fixture setup, thread execution, or a database mutation fails unexpectedly, or concurrent mutations leave no administrator.
#[test]
fn concurrent_admin_deletions_and_demotions_preserve_one_admin() {
    use crate::db::tables::{
        user_roles::{self, Role},
        users::{self, AccountMutationError},
    };
    for (delete_first, delete_second) in
        [(false, false), (true, true), (false, true), (true, false)]
    {
        let api = TestApi::new();
        user_roles::add_user_role(&api.conn(), 2, Role::Admin).unwrap();
        let first = api.conn();
        let second = api.conn();
        let barrier = std::sync::Barrier::new(2);
        let results = std::thread::scope(|scope| {
            let mutate = |mut conn: rusqlite::Connection, user_id, delete| {
                barrier.wait();
                if delete {
                    users::delete_user(&mut conn, user_id)
                } else {
                    users::update_user_account(
                        &mut conn,
                        user_id,
                        Some("updated-admin"),
                        Some(Role::Viewer),
                    )
                }
            };
            let first = scope.spawn(move || mutate(first, 1, delete_first));
            let second = scope.spawn(move || mutate(second, 2, delete_second));
            [first.join().unwrap(), second.join().unwrap()]
        });
        assert_eq!(
            results.iter().filter(|result| result.is_ok()).count(),
            1,
            "{results:?}"
        );
        assert_eq!(
            results
                .iter()
                .filter(|result| matches!(result, Err(AccountMutationError::FinalAdmin)))
                .count(),
            1,
            "{results:?}"
        );
        let conn = api.conn();
        assert_eq!(
            user_roles::count_users_with_role(&conn, Role::Admin).unwrap(),
            1
        );
        let remaining_id: i64 = conn
            .query_row(
                "SELECT user_id FROM user_roles WHERE role_id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            users::get_user_by_id(&conn, remaining_id)
                .unwrap()
                .unwrap()
                .username,
            if remaining_id == 1 { "admin" } else { "editor" }
        );
    }
}

/// Creation and updates share username normalization, length limits, character constraints, and trimmed uniqueness checks.
///
/// # Panics
///
/// Panics if invalid usernames are accepted, normalized usernames are not stored, or duplicate and boundary handling differs across writes.
#[tokio::test]
async fn usernames_are_validated_and_trimmed_consistently() {
    let api = TestApi::new();
    for (username, message) in [
        (" ".to_string(), "Username cannot be empty"),
        ("a".repeat(65), "Username must be at most 64 characters"),
        ("é".repeat(65), "Username must be at most 64 characters"),
        (
            "two words".to_string(),
            "Username cannot contain whitespace or control characters",
        ),
        (
            "two\twords".to_string(),
            "Username cannot contain whitespace or control characters",
        ),
        (
            "two\nwords".to_string(),
            "Username cannot contain whitespace or control characters",
        ),
        (
            "two\u{a0}words".to_string(),
            "Username cannot contain whitespace or control characters",
        ),
        (
            "control\0name".to_string(),
            "Username cannot contain whitespace or control characters",
        ),
        (
            "control\u{7f}name".to_string(),
            "Username cannot contain whitespace or control characters",
        ),
    ] {
        for (method, path) in [("POST", "/api/v1/users"), ("PATCH", "/api/v1/users/3")] {
            assert_eq!(
                api.request(
                    method,
                    path,
                    Some("admin"),
                    json!({"username": username, "password": PASSWORD, "role": "viewer"})
                )
                .await,
                (StatusCode::BAD_REQUEST, json!({"message": message})),
                "{method} {username:?}"
            );
        }
    }
    for (method, path) in [("POST", "/api/v1/users"), ("PATCH", "/api/v1/users/3")] {
        assert_eq!(
            api.request(
                method,
                path,
                Some("admin"),
                json!({"username": " admin ", "password": PASSWORD, "role": "viewer"})
            )
            .await,
            (
                StatusCode::CONFLICT,
                json!({"message": "Username already exists"})
            )
        );
    }
    let boundary = "é".repeat(64);
    assert_eq!(
        api.request(
            "POST",
            "/api/v1/users",
            Some("admin"),
            json!({"username": format!(" {boundary} "), "password": PASSWORD, "role": "viewer"})
        )
        .await
        .0,
        StatusCode::CREATED
    );
    assert!(crate::db::tables::users::check_username_exists(&api.conn(), &boundary).unwrap());
    assert_eq!(
        api.request(
            "POST",
            "/api/v1/auth/login",
            None,
            json!({"username": boundary, "password": PASSWORD})
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        api.request(
            "PATCH",
            "/api/v1/users/3",
            Some("admin"),
            json!({"username": format!(" {boundary} ")})
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        api.request(
            "PATCH",
            "/api/v1/users/3",
            Some("admin"),
            json!({"username": " viewer "})
        )
        .await
        .0,
        StatusCode::OK
    );
}

/// Account creation and password resets enforce the same 12-byte minimum, including UTF-8 boundaries.
///
/// # Panics
///
/// Panics if an undersized password succeeds, a boundary password fails, or a rejected reset revokes sessions.
#[tokio::test]
async fn password_minimum_is_consistent_across_creation_and_reset() {
    let api = TestApi::new();
    for password in [
        String::new(),
        "x".to_string(),
        "x".repeat(11),
        "é".repeat(5),
    ] {
        for (method, path, payload) in [
            (
                "POST",
                "/api/v1/users",
                json!({"username": "new-user", "password": password, "role": "viewer"}),
            ),
            (
                "PUT",
                "/api/v1/users/3/password",
                json!({"new_password": password}),
            ),
        ] {
            assert_eq!(
                api.request(method, path, Some("admin"), payload).await,
                (
                    StatusCode::BAD_REQUEST,
                    json!({"message": "Password must be at least 12 characters"})
                )
            );
        }
    }
    assert_eq!(
        api.request("GET", "/api/v1/auth/me", Some("viewer"), json!({}))
            .await
            .0,
        StatusCode::OK
    );
    for (username, password) in [
        ("ascii-boundary", "x".repeat(12)),
        ("utf8-boundary", "é".repeat(6)),
    ] {
        assert_eq!(
            api.request(
                "POST",
                "/api/v1/users",
                Some("admin"),
                json!({"username": username, "password": password, "role": "viewer"})
            )
            .await
            .0,
            StatusCode::CREATED
        );
        assert_eq!(
            api.request(
                "PUT",
                "/api/v1/users/3/password",
                Some("admin"),
                json!({"new_password": password})
            )
            .await
            .0,
            StatusCode::OK
        );
    }
}

/// Username substring filters treat percent signs, underscores, and backslashes literally.
///
/// # Panics
///
/// Panics if account creation fails or a literal search matches a wildcard decoy or loses an escaped character.
#[tokio::test]
async fn username_search_escapes_like_wildcards() {
    let api = TestApi::new();
    let conn = api.conn();
    for username in [
        "literal_name",
        "literal%name",
        r"literal\name",
        "literalXname",
        r"combo%\_tail",
    ] {
        conn.execute(
            "INSERT INTO users (username, password_hash) VALUES (?1, 'unused')",
            [username],
        )
        .unwrap();
    }
    for (query, expected) in [
        ("literal%5F", "literal_name"),
        ("literal%25", "literal%name"),
        ("literal%5C", r"literal\name"),
        ("%25%5C%5F", r"combo%\_tail"),
    ] {
        let (status, body) = api
            .request(
                "GET",
                &format!("/api/v1/users?username={query}"),
                Some("admin"),
                json!({}),
            )
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["users"].as_array().unwrap().len(), 1, "{body}");
        assert_eq!(body["users"][0]["username"], expected);
    }
    assert_eq!(
        api.request(
            "GET",
            "/api/v1/users?username=literal",
            Some("admin"),
            json!({})
        )
        .await
        .1["users"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
}
