//! Database connection settings and migration contracts for fresh and existing databases.

use super::{
    migrations::{MIGRATIONS, run_migrations},
    open_database::open_database,
};

/// Opened connections enable foreign keys, normal synchronous durability, and a five-second SQLite write-lock timeout.
///
/// # Panics
///
/// Panics if opening or inspecting a connection fails or its pragma values differ from the required configuration.
#[test]
fn opened_connections_set_busy_timeout_and_foreign_keys() {
    for _ in 0..2 {
        let conn = open_database(":memory:").unwrap();
        for (pragma, expected) in [
            ("busy_timeout", 5000),
            ("foreign_keys", 1),
            ("synchronous", 1),
        ] {
            let actual: i64 = conn
                .pragma_query_value(None, pragma, |row| row.get(0))
                .unwrap();
            assert_eq!(actual, expected, "{pragma}");
        }
    }
}

/// `initialize_app` migrates the database, creates the initial administrator, and closes its startup connection.
///
/// # Panics
///
/// Panics if temporary directory creation, startup initialization, account verification, or cleanup fails.
#[test]
fn initialize_app_creates_schema_and_initial_admin() {
    use crate::utils::{startup::initialize_app, tokens::generate_session_token};

    let directory = std::env::temp_dir().join(format!(
        "justgoto-startup-{}",
        generate_session_token().unwrap()
    ));
    std::fs::create_dir(&directory).unwrap();
    let db_path = directory.join("startup.sqlite3");
    let db_path_str = db_path.to_str().unwrap();

    assert_eq!(
        initialize_app(db_path_str, "admin", "admin-password"),
        Ok(())
    );

    let conn = open_database(db_path_str).unwrap();
    let (user_id, _): (i64, String) = super::tables::users::get_login_credentials(&conn, "admin")
        .unwrap()
        .unwrap();
    assert!(
        super::tables::user_roles::check_user_role(
            &conn,
            user_id,
            super::tables::user_roles::Role::Admin,
        )
        .unwrap()
    );
    drop(conn);
    std::fs::remove_dir_all(&directory).unwrap();
}

/// Failed initial-admin role assignment leaves the database empty so startup can retry successfully.
///
/// # Panics
///
/// Panics if database setup fails, a rejected role assignment leaves an account, or administrator initialization cannot be retried.
#[test]
fn initial_admin_creation_rolls_back_and_can_be_retried() {
    use crate::utils::admin::ensure_admin_account;

    let mut conn = open_database(":memory:").unwrap();
    run_migrations(&mut conn).unwrap();
    conn.execute_batch(
        "CREATE TRIGGER reject_admin_role BEFORE INSERT ON user_roles BEGIN SELECT RAISE(ABORT, 'role assignment rejected'); END;",
    ).unwrap();
    assert!(ensure_admin_account(&mut conn, "admin", "admin-password").is_err());
    assert!(!super::tables::users::check_if_account_exists(&conn).unwrap());
    conn.execute_batch("DROP TRIGGER reject_admin_role;")
        .unwrap();
    ensure_admin_account(&mut conn, "admin", "admin-password").unwrap();
    assert!(
        super::tables::user_roles::check_user_role(
            &conn,
            1,
            super::tables::user_roles::Role::Admin
        )
        .unwrap()
    );
}

/// Fresh databases and databases with the first two migrations acquire the same indexes and retain the runner-owned migration schema.
///
/// # Panics
///
/// Panics if migration setup, schema inspection, or uniqueness checks fail, or migrations do not preserve existing records.
#[test]
fn migrations_create_indexes_on_fresh_and_existing_databases() {
    for existing in [false, true] {
        let mut conn = open_database(":memory:").unwrap();
        if existing {
            conn.execute_batch(
                "CREATE TABLE _migrations (
                    version INTEGER PRIMARY KEY,
                    name TEXT NOT NULL,
                    applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                );",
            )
            .unwrap();
            for migration in &MIGRATIONS[..2] {
                conn.execute_batch(migration.sql).unwrap();
                conn.execute(
                    "INSERT INTO _migrations (version, name) VALUES (?1, ?2)",
                    (migration.version, migration.name),
                )
                .unwrap();
            }
            conn.execute(
                "INSERT INTO users (username, password_hash) VALUES ('existing', 'hash')",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO sessions (user_id, hash_string, expires_at) VALUES (1, 'existing', 1)",
                [],
            )
            .unwrap();
            // An already present index must not prevent the upgrade.
            conn.execute_batch("CREATE INDEX sessions_user_id_idx ON sessions(user_id);")
                .unwrap();
        }
        run_migrations(&mut conn).unwrap();
        run_migrations(&mut conn).unwrap();

        for (table, column, index, unique) in [
            ("sessions", "hash_string", "sessions_hash_string_idx", true),
            ("sessions", "user_id", "sessions_user_id_idx", false),
            ("sessions", "expires_at", "sessions_expires_at_idx", false),
            ("user_roles", "role_id", "user_roles_role_id_idx", false),
            ("redirects", "created_by", "redirects_created_by_idx", false),
        ] {
            let actual_unique: bool = conn
                .query_row(
                    "SELECT \"unique\" FROM pragma_index_list(?1) WHERE name = ?2",
                    (table, index),
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(actual_unique, unique, "{index}");
            let columns: Vec<String> = conn
                .prepare("SELECT name FROM pragma_index_info(?1) ORDER BY seqno")
                .unwrap()
                .query_map([index], |row| row.get(0))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap();
            assert_eq!(columns, [column], "{index}");
        }

        let schema: Vec<(String, String, Option<String>, bool)> = conn
            .prepare("SELECT name, type, dflt_value, pk FROM pragma_table_info('_migrations') ORDER BY cid")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            schema,
            vec![
                ("version".into(), "INTEGER".into(), None, true),
                ("name".into(), "TEXT".into(), None, false),
                (
                    "applied_at".into(),
                    "TEXT".into(),
                    Some("CURRENT_TIMESTAMP".into()),
                    false
                ),
            ]
        );
        let versions: Vec<i64> = conn
            .prepare("SELECT version FROM _migrations ORDER BY version")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(versions, [0, 1, 2]);

        if existing {
            let count: i64 = conn
                .query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))
                .unwrap();
            assert_eq!(count, 1);
            let error = conn.execute(
                "INSERT INTO sessions (user_id, hash_string, expires_at) VALUES (1, 'existing', 2)",
                [],
            ).unwrap_err();
            assert!(
                matches!(error, rusqlite::Error::SqliteFailure(failure, _) if failure.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE)
            );
        }
    }
}
