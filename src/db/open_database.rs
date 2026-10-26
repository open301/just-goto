//! Opens SQLite connections with foreign keys, write-ahead logging, normal durability, and a five-second busy timeout.

use rusqlite::Connection;

/// Opens the configured SQLite database, enables foreign keys, write-ahead logging, and normal synchronous mode, and waits up to five seconds for write locks.
///
/// # Errors
///
/// Returns a descriptive message if the database cannot be opened or its connection pragmas cannot be applied.
pub fn open_database(path: &str) -> Result<Connection, String> {
    let conn = Connection::open(path).map_err(|e| format!("Failed to open database: {}", e))?;

    // Configure lock waiting before enabling foreign keys, WAL, and normal synchronous durability.
    conn.execute_batch(
        r#"
        PRAGMA busy_timeout = 5000;
        PRAGMA foreign_keys = ON;
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;
        "#,
    )
    .map_err(|e| format!("Failed to enable foreign keys: {}", e))?;

    Ok(conn)
}
