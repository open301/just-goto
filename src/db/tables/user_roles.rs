//! Role identifiers and queries over user-role assignments.

use rusqlite::{Connection, Result};

/// Supported account roles, with discriminants matching the seeded database role IDs.
#[repr(i64)]
#[derive(Clone, Copy)]
pub enum Role {
    /// May manage user accounts and create, update, delete, or import redirects.
    Admin = 1,
    /// May create, update, delete, or import redirects.
    Editor = 2,
    /// May browse redirects without permission to modify them.
    Viewer = 3,
}

/// Returns the canonical lowercase name of a role.
pub fn role_to_string(role: Role) -> &'static str {
    match role {
        Role::Admin => "admin",
        Role::Editor => "editor",
        Role::Viewer => "viewer",
    }
}

/// Parses admin, editor, or viewer case-insensitively, returning `None` for an unknown role.
pub fn string_to_role(role_str: &str) -> Option<Role> {
    match role_str.to_ascii_lowercase().as_str() {
        "admin" => Some(Role::Admin),
        "editor" => Some(Role::Editor),
        "viewer" => Some(Role::Viewer),
        _ => None,
    }
}

/// Assigns an existing role to a user, leaving an existing identical assignment unchanged.
///
/// # Errors
///
/// Returns a SQLite error if the assignment fails, including an invalid user or role ID.
pub fn add_user_role(conn: &Connection, user_id: i64, role: Role) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO user_roles (user_id, role_id) VALUES (?1, ?2)",
        (user_id, role as i64),
    )?;

    Ok(())
}

/// Checks whether the `user_roles` table assigns the requested role to a user.
///
/// # Errors
///
/// Returns a SQLite error if the role-assignment lookup fails.
pub fn check_user_role(conn: &Connection, user_id: i64, role: Role) -> Result<bool> {
    let mut stmt =
        conn.prepare("SELECT COUNT(*) FROM user_roles WHERE user_id = ?1 AND role_id = ?2")?;
    let count: i64 = stmt.query_row([user_id, role as i64], |row| row.get(0))?;
    Ok(count > 0)
}

/// Joins `roles` and `user_roles` to list a user's role names in role-ID order.
///
/// # Errors
///
/// Returns a SQLite error if the joined lookup fails or a role name cannot be decoded.
pub fn get_user_roles(conn: &Connection, user_id: i64) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT roles.name FROM roles
         INNER JOIN user_roles ON roles.id = user_roles.role_id
         WHERE user_roles.user_id = ?1
         ORDER BY roles.id",
    )?;
    let rows = stmt.query_map([user_id], |row| row.get(0))?;
    rows.collect()
}

/// Counts user-role assignments for the supplied role, including the administrator-preservation check.
///
/// # Errors
///
/// Returns a SQLite error if the assignment count cannot be queried.
pub fn count_users_with_role(conn: &Connection, role: Role) -> Result<i64> {
    let mut stmt = conn.prepare("SELECT COUNT(*) FROM user_roles WHERE role_id = ?1")?;
    stmt.query_row([role as i64], |row| row.get(0))
}

/// Checks whether removing this user's admin role would leave no administrator; callers must hold an immediate write transaction until the mutation commits.
///
/// # Errors
///
/// Returns a SQLite error if the user's role or administrator count cannot be queried.
pub fn is_final_admin(conn: &Connection, user_id: i64) -> Result<bool> {
    Ok(check_user_role(conn, user_id, Role::Admin)?
        && count_users_with_role(conn, Role::Admin)? <= 1)
}
