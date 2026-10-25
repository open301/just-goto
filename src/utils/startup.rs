//! Initializes the database schema and initial administrator account.

use crate::db::{migrations::run_migrations, open_database};
use crate::utils::admin::ensure_admin_account;

/// Opens the database, applies migrations, and ensures the initial administrator exists.
///
/// # Errors
///
/// Returns an error if database setup, migration execution, or administrator initialization fails.
pub fn initialize_app(
    path: &str,
    admin_username: &str,
    admin_password: &str,
) -> Result<(), String> {
    let mut conn = open_database::open_database(path)?;
    run_migrations(&mut conn).map_err(|e| e.to_string())?;
    ensure_admin_account(&mut conn, admin_username, admin_password)?;
    Ok(())
}
