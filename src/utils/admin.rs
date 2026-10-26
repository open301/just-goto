//! Creates the initial administrator when no user accounts exist.

use rusqlite::Connection;

use crate::db::tables::{user_roles, users};
use crate::utils::passwords::hash_password;

/// Creates the initial administrator only when the database contains no user accounts.
///
/// # Errors
///
/// Returns a descriptive message if account lookup, password hashing, account creation, or admin-role assignment fails.
pub fn ensure_admin_account(
    conn: &mut Connection,
    admin_username: &str,
    admin_password: &str,
) -> Result<(), String> {
    if users::check_if_account_exists(conn)
        .map_err(|e| format!("Failed to check user accounts: {}", e))?
    {
        return Ok(());
    }

    println!("No user accounts exist. Creating admin account...");

    let password_hash =
        hash_password(admin_password).map_err(|e| format!("Failed to hash password: {}", e))?;

    users::create_user_with_role(
        conn,
        admin_username,
        &password_hash,
        user_roles::Role::Admin,
    )
    .map_err(|error| format!("Failed to create admin account: {error}"))?;

    Ok(())
}
