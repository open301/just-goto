//! Queries that join multiple tables; each function identifies the tables it uses.

use rusqlite::OptionalExtension;

use crate::utils::tokens::hash_session_token;

/// Joins `users` and `sessions` to resolve an unexpired token to an existing account, returning `None` for an unknown or expired session.
///
/// # Errors
///
/// Returns a SQLite error if the joined session lookup fails or the user ID cannot be decoded.
pub fn get_user_id_from_token(
    conn: &rusqlite::Connection,
    token: &str,
) -> Result<Option<i64>, rusqlite::Error> {
    let token_hash = hash_session_token(token);
    let mut stmt = conn.prepare(
        "SELECT users.id FROM users
         INNER JOIN sessions ON users.id = sessions.user_id
         WHERE sessions.hash_string = ?1
         AND sessions.expires_at > unixepoch()",
    )?;
    let user_id: Option<i64> = stmt.query_row([token_hash], |row| row.get(0)).optional()?;
    Ok(user_id)
}
