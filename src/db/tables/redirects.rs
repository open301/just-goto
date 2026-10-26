//! Redirect records, filtered lookups, mutations, and transactional bulk inserts.

use rusqlite::OptionalExtension;
use rusqlite::params;

/// A stored redirect with its creator, timestamps, enabled state, and HTTP status.
pub struct Redirect {
    /// Unique redirect ID.
    pub id: i64,
    /// Unique ASCII slug used by the public redirect route.
    pub slug: String,
    /// HTTP or HTTPS redirect destination.
    pub destination: String,
    /// Creator account ID, or `None` when that account was deleted.
    pub created_by: Option<i64>,
    /// Creation time as Unix seconds.
    pub created_at: i64,
    /// Most recent update as Unix seconds.
    pub updated_at: i64,
    /// Whether public requests may resolve this redirect.
    pub is_enabled: bool,
    /// HTTP redirect status returned when resolving the slug.
    pub redirect_code: i64,
}

/// Validated redirect values ready for insertion during a CSV import.
#[derive(Clone)]
pub struct NewRedirect {
    /// Unique ASCII slug used by the public redirect route.
    pub slug: String,
    /// HTTP or HTTPS redirect destination.
    pub destination: String,
    /// Whether public requests may resolve this redirect.
    pub is_enabled: bool,
    /// HTTP redirect status returned when resolving the slug.
    pub redirect_code: i64,
}

/// Lists redirects by ID using optional slug, enabled-state, and creator filters with SQL pagination.
///
/// # Errors
///
/// Returns a SQLite error if the filtered query fails or a redirect record cannot be decoded.
pub fn list_redirects(
    conn: &rusqlite::Connection,
    slug: Option<&str>,
    is_enabled: Option<bool>,
    created_by: Option<i64>,
    limit: i64,
    offset: i64,
) -> Result<Vec<Redirect>, rusqlite::Error> {
    let slug = slug.map(crate::db::search::escape_like_pattern);
    let mut stmt = conn.prepare(
        "SELECT id, slug, destination, created_by, created_at, updated_at,
                is_enabled, redirect_code
         FROM redirects
         WHERE (?1 IS NULL OR slug LIKE '%' || ?1 || '%' ESCAPE '\\')
           AND (?2 IS NULL OR is_enabled = ?2)
           AND (?3 IS NULL OR created_by = ?3)
         ORDER BY id
         LIMIT ?4 OFFSET ?5",
    )?;
    let rows = stmt.query_map(
        params![slug, is_enabled, created_by, limit, offset],
        |row| {
            Ok(Redirect {
                id: row.get(0)?,
                slug: row.get(1)?,
                destination: row.get(2)?,
                created_by: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
                is_enabled: row.get(6)?,
                redirect_code: row.get(7)?,
            })
        },
    )?;
    rows.collect()
}

/// Checks whether any redirect already occupies the supplied slug.
///
/// # Errors
///
/// Returns a SQLite error if the slug lookup fails.
pub fn check_slug_exists(conn: &rusqlite::Connection, slug: &str) -> Result<bool, rusqlite::Error> {
    let mut stmt = conn.prepare("SELECT COUNT(*) FROM redirects WHERE slug = ?1")?;
    let count: i64 = stmt.query_row([slug], |row| row.get(0))?;
    Ok(count > 0)
}

/// Inserts a redirect with its creator, enabled state, and HTTP status, returning its new ID.
///
/// # Errors
///
/// Returns a SQLite error if insertion fails, including duplicate slugs or invalid foreign keys.
pub fn create_redirect(
    conn: &rusqlite::Connection,
    slug: &str,
    destination: &str,
    created_by: i64,
    is_enabled: bool,
    redirect_code: i64,
) -> Result<i64, rusqlite::Error> {
    conn.execute(
        "INSERT INTO redirects
         (slug, destination, created_by, is_enabled, redirect_code)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![slug, destination, created_by, is_enabled, redirect_code],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Inserts prepared redirects in one transaction and reports whether each row was inserted; existing slugs are ignored.
///
/// # Errors
///
/// Returns a SQLite error if the transaction, prepared insert, or commit fails; no partial transaction is committed.
pub fn import_redirects(
    conn: &mut rusqlite::Connection,
    created_by: i64,
    redirects: &[NewRedirect],
) -> Result<Vec<bool>, rusqlite::Error> {
    // Keep all valid rows atomic. Duplicate slugs are deliberately ignored so the importer can
    // report them alongside validation errors without rolling back unrelated rows.
    let tx = conn.transaction()?;
    let results = {
        let mut statement = tx.prepare(
            "INSERT OR IGNORE INTO redirects
             (slug, destination, created_by, is_enabled, redirect_code)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        let mut results = Vec::with_capacity(redirects.len());
        for redirect in redirects {
            let inserted = statement.execute(params![
                redirect.slug,
                redirect.destination,
                created_by,
                redirect.is_enabled,
                redirect.redirect_code,
            ])?;
            results.push(inserted > 0);
        }
        results
    };
    tx.commit()?;
    Ok(results)
}

/// Looks up an enabled or disabled redirect by ID, returning `None` when it does not exist.
///
/// # Errors
///
/// Returns a SQLite error if the lookup fails or the redirect record cannot be decoded.
pub fn get_redirect_by_id(
    conn: &rusqlite::Connection,
    redirect_id: i64,
) -> Result<Option<Redirect>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id, slug, destination, created_by, created_at, updated_at,
                is_enabled, redirect_code
         FROM redirects WHERE id = ?1",
    )?;
    stmt.query_row([redirect_id], |row| {
        Ok(Redirect {
            id: row.get(0)?,
            slug: row.get(1)?,
            destination: row.get(2)?,
            created_by: row.get(3)?,
            created_at: row.get(4)?,
            updated_at: row.get(5)?,
            is_enabled: row.get(6)?,
            redirect_code: row.get(7)?,
        })
    })
    .optional()
}

/// Checks whether a slug belongs to any redirect except the supplied ID.
///
/// # Errors
///
/// Returns a SQLite error if the duplicate-slug lookup fails.
pub fn check_slug_exists_for_other_redirect(
    conn: &rusqlite::Connection,
    slug: &str,
    redirect_id: i64,
) -> Result<bool, rusqlite::Error> {
    let mut stmt = conn.prepare("SELECT COUNT(*) FROM redirects WHERE slug = ?1 AND id != ?2")?;
    let count: i64 = stmt.query_row((slug, redirect_id), |row| row.get(0))?;
    Ok(count > 0)
}

/// Updates supplied redirect fields and its modification timestamp, returning whether an ID matched.
///
/// # Errors
///
/// Returns a SQLite error if the update fails, including slug conflicts or invalid field constraints.
pub fn update_redirect(
    conn: &rusqlite::Connection,
    redirect_id: i64,
    slug: Option<&str>,
    destination: Option<&str>,
    redirect_code: Option<i64>,
    is_enabled: Option<bool>,
) -> Result<bool, rusqlite::Error> {
    let updated = conn.execute(
        "UPDATE redirects
         SET slug = COALESCE(?1, slug),
             destination = COALESCE(?2, destination),
             redirect_code = COALESCE(?3, redirect_code),
             is_enabled = COALESCE(?4, is_enabled),
             updated_at = unixepoch()
         WHERE id = ?5",
        params![slug, destination, redirect_code, is_enabled, redirect_id],
    )?;
    Ok(updated > 0)
}

/// Deletes a redirect by ID and returns whether a row was removed.
///
/// # Errors
///
/// Returns a SQLite error if the deletion fails.
pub fn delete_redirect(
    conn: &rusqlite::Connection,
    redirect_id: i64,
) -> Result<bool, rusqlite::Error> {
    let deleted = conn.execute("DELETE FROM redirects WHERE id = ?1", [redirect_id])?;
    Ok(deleted > 0)
}

/// Looks up a public redirect by slug, returning `None` for missing or disabled redirects.
///
/// # Errors
///
/// Returns a SQLite error if the lookup fails or the redirect record cannot be decoded.
pub fn get_enabled_redirect_by_slug(
    conn: &rusqlite::Connection,
    slug: &str,
) -> Result<Option<Redirect>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id, slug, destination, created_by, created_at, updated_at,
                is_enabled, redirect_code
         FROM redirects
         WHERE slug = ?1 AND is_enabled = 1",
    )?;
    stmt.query_row([slug], |row| {
        Ok(Redirect {
            id: row.get(0)?,
            slug: row.get(1)?,
            destination: row.get(2)?,
            created_by: row.get(3)?,
            created_at: row.get(4)?,
            updated_at: row.get(5)?,
            is_enabled: row.get(6)?,
            redirect_code: row.get(7)?,
        })
    })
    .optional()
}
