//! Tracks and applies embedded SQL migrations in version order.

use rusqlite::{Connection, Result};

/// An embedded SQL migration recorded in the database migration history.
pub struct Migration {
    /// Ordered migration identifier stored in `_migrations`.
    pub version: i64,
    /// Human-readable migration name recorded when it is applied.
    pub name: &'static str,
    /// SQL embedded into the binary and executed within the migration transaction.
    pub sql: &'static str,
}

/// Embedded migrations in the order required to create the application schema and seed its roles.
pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 0,
        name: "initial",
        sql: include_str!("../../sql/migration_000_init_db.sql"),
    },
    Migration {
        version: 1,
        name: "seed initial data structure",
        sql: include_str!("../../sql/migration_001_seed_initial_data.sql"),
    },
    Migration {
        version: 2,
        name: "add session and foreign key indexes",
        sql: include_str!("../../sql/migration_002_add_indexes.sql"),
    },
];

/// Applies unapplied migrations in version order, committing each migration with its history entry.
///
/// # Errors
///
/// Returns a SQLite error if migration history cannot be read or written, a migration statement fails, or its transaction cannot be committed.
pub fn run_migrations(conn: &mut Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS _migrations (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );
        "#,
    )?;

    for migration in MIGRATIONS {
        let applied: bool = conn.query_row(
            "SELECT EXISTS(
                SELECT 1
                FROM _migrations
                WHERE version = ?1
            )",
            [migration.version],
            |row| row.get(0),
        )?;

        if applied {
            continue;
        }

        let tx = conn.transaction()?;

        tx.execute_batch(migration.sql)?;

        tx.execute(
            "INSERT INTO _migrations (version, name) VALUES (?1, ?2)",
            (migration.version, migration.name),
        )?;

        tx.commit()?;

        println!(
            "Applied migration {}: {}",
            migration.version, migration.name
        );
    }

    Ok(())
}
