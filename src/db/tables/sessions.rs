//! Session creation and revocation using hashed bearer tokens.

use crate::utils::tokens::hash_session_token;

/// Purges expired sessions, then hashes the supplied bearer token and stores its user ID and Unix expiration time.
///
/// # Errors
///
/// Returns a SQLite error if expired-session cleanup or session insertion fails, including an invalid user ID.
pub fn create_session(
    conn: &rusqlite::Connection,
    user_id: i64,
    token: &str,
    expires_at: i64,
) -> Result<(), rusqlite::Error> {
    let token_hash = hash_session_token(token);
    conn.execute("DELETE FROM sessions WHERE expires_at <= unixepoch()", [])?;
    conn.execute(
        "INSERT INTO sessions (user_id, hash_string, expires_at) VALUES (?1, ?2, ?3)",
        (user_id, token_hash, expires_at),
    )?;
    Ok(())
}

/// Revokes the supplied bearer token by deleting its stored hash, returning whether it existed.
///
/// # Errors
///
/// Returns a SQLite error if the session deletion fails.
pub fn delete_session(conn: &rusqlite::Connection, token: &str) -> Result<bool, rusqlite::Error> {
    let token_hash = hash_session_token(token);
    let deleted = conn.execute("DELETE FROM sessions WHERE hash_string = ?1", [token_hash])?;
    Ok(deleted > 0)
}
