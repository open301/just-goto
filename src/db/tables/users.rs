//! User account queries and transactional account or password updates.

use std::fmt;

use rusqlite::OptionalExtension;
use rusqlite::params;

use crate::db::tables::user_roles::{self, Role};

/// Account details returned by database queries, excluding the password hash.
pub struct User {
    /// Unique account ID.
    pub id: i64,
    /// Unique account username.
    pub username: String,
    /// Account creation time as Unix seconds.
    pub created_at: i64,
    /// Most recent account update as Unix seconds.
    pub updated_at: i64,
}

/// Account details and all role names returned by a paginated listing.
pub struct UserWithRoles {
    /// Account details without password data.
    pub user: User,
    /// Assigned role names in role-ID order.
    pub roles: Vec<String>,
}

/// Account creation failures with enough context to preserve client-facing error messages.
#[derive(Debug)]
pub enum CreateUserError {
    /// Another account already uses the requested username.
    UsernameExists,
    /// Starting the transaction or checking username availability failed.
    Database,
    /// Inserting the account or committing the transaction failed.
    CreateAccount,
    /// Assigning the requested role failed.
    AssignRole,
}

/// Formats account creation failures using the established client-facing and startup error messages.
impl fmt::Display for CreateUserError {
    /// Writes a human-readable description of the account creation failure.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::UsernameExists => "Username already exists",
            Self::Database => "Database error",
            Self::CreateAccount => "Failed to create user",
            Self::AssignRole => "Failed to assign user role",
        };
        f.write_str(message)
    }
}

/// Creates an account and assigns its role atomically, serializing username checks with concurrent writers.
///
/// # Errors
///
/// Returns a creation error if the username is occupied or transaction setup, account insertion, role assignment, or commit fails; failed transactions leave no new account.
pub fn create_user_with_role(
    conn: &mut rusqlite::Connection,
    username: &str,
    password_hash: &str,
    role: Role,
) -> Result<i64, CreateUserError> {
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|_| CreateUserError::Database)?;
    if check_username_exists(&tx, username).map_err(|_| CreateUserError::Database)? {
        return Err(CreateUserError::UsernameExists);
    }
    let user_id = create_user_account(&tx, username, password_hash)
        .map_err(|_| CreateUserError::CreateAccount)?;
    user_roles::add_user_role(&tx, user_id, role).map_err(|_| CreateUserError::AssignRole)?;
    tx.commit().map_err(|_| CreateUserError::CreateAccount)?;
    Ok(user_id)
}

/// Checks whether any account exists before deciding whether to bootstrap an administrator.
///
/// # Errors
///
/// Returns a SQLite error if the account count cannot be queried.
pub fn check_if_account_exists(conn: &rusqlite::Connection) -> Result<bool, rusqlite::Error> {
    let mut stmt = conn.prepare("SELECT COUNT(*) FROM users")?;
    let count: i64 = stmt.query_row([], |row| row.get(0))?;
    Ok(count > 0)
}

/// Creates an account from a username and an already hashed password, returning the new user ID.
///
/// # Errors
///
/// Returns a SQLite error if insertion fails, including a duplicate username.
pub fn create_user_account(
    conn: &rusqlite::Connection,
    username: &str,
    password_hash: &str,
) -> Result<i64, rusqlite::Error> {
    conn.execute(
        "INSERT INTO users (username, password_hash) VALUES (?1, ?2)",
        (username, password_hash),
    )?;

    Ok(conn.last_insert_rowid())
}

/// Checks whether an account already uses the supplied username.
///
/// # Errors
///
/// Returns a SQLite error if the username lookup fails.
pub fn check_username_exists(
    conn: &rusqlite::Connection,
    username: &str,
) -> Result<bool, rusqlite::Error> {
    let mut stmt = conn.prepare("SELECT COUNT(*) FROM users WHERE username = ?1")?;
    let count: i64 = stmt.query_row([username], |row| row.get(0))?;
    Ok(count > 0)
}

/// Fetches the account ID and password hash in one lookup, returning `None` for an absent username.
///
/// # Errors
///
/// Returns a SQLite error if the lookup fails or the account ID or password hash cannot be decoded.
pub fn get_login_credentials(
    conn: &rusqlite::Connection,
    username: &str,
) -> Result<Option<(i64, String)>, rusqlite::Error> {
    conn.query_row(
        "SELECT id, password_hash FROM users WHERE username = ?1",
        [username],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .optional()
}

/// Fetches account details without password data, returning `None` for an unknown user ID.
///
/// # Errors
///
/// Returns a SQLite error if the lookup fails or the account record cannot be decoded.
pub fn get_user_by_id(
    conn: &rusqlite::Connection,
    user_id: i64,
) -> Result<Option<User>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id, username, created_at, updated_at
         FROM users WHERE id = ?1",
    )?;
    stmt.query_row([user_id], |row| {
        Ok(User {
            id: row.get(0)?,
            username: row.get(1)?,
            created_at: row.get(2)?,
            updated_at: row.get(3)?,
        })
    })
    .optional()
}

/// Lists accounts and all their roles in one query, paginating filtered accounts before joining their roles in role-ID order.
///
/// # Errors
///
/// Returns a SQLite error if the joined query fails or an account or role record cannot be decoded.
pub fn list_users(
    conn: &rusqlite::Connection,
    username: Option<&str>,
    role: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<UserWithRoles>, rusqlite::Error> {
    let username = username.map(crate::db::search::escape_like_pattern);
    let mut stmt = conn.prepare(
        "WITH page AS (
             SELECT DISTINCT users.id, users.username, users.created_at, users.updated_at
             FROM users
             LEFT JOIN user_roles ON users.id = user_roles.user_id
             LEFT JOIN roles ON roles.id = user_roles.role_id
             WHERE (?1 IS NULL OR users.username LIKE '%' || ?1 || '%' ESCAPE '\\')
               AND (?2 IS NULL OR roles.name = ?2)
             ORDER BY users.id
             LIMIT ?3 OFFSET ?4
         )
         SELECT page.id, page.username, page.created_at, page.updated_at, roles.id, roles.name
         FROM page
         LEFT JOIN user_roles ON page.id = user_roles.user_id
         LEFT JOIN roles ON roles.id = user_roles.role_id
         ORDER BY page.id, roles.id",
    )?;
    let mut rows = stmt.query(params![username, role, limit, offset])?;
    let mut users: Vec<UserWithRoles> = Vec::new();
    while let Some(row) = rows.next()? {
        let id = row.get(0)?;
        if users.last().is_none_or(|user| user.user.id != id) {
            users.push(UserWithRoles {
                user: User {
                    id,
                    username: row.get(1)?,
                    created_at: row.get(2)?,
                    updated_at: row.get(3)?,
                },
                roles: Vec::new(),
            });
        }
        if let Some(listed_user) = users.last_mut()
            && row.get::<_, Option<i64>>(4)?.is_some()
        {
            listed_user.roles.push(row.get(5)?);
        }
    }
    Ok(users)
}

/// Checks whether a username belongs to any account except the supplied user ID.
///
/// # Errors
///
/// Returns a SQLite error if the duplicate-username lookup fails.
pub fn check_username_exists_for_other_user(
    conn: &rusqlite::Connection,
    username: &str,
    user_id: i64,
) -> Result<bool, rusqlite::Error> {
    let mut stmt = conn.prepare("SELECT COUNT(*) FROM users WHERE username = ?1 AND id != ?2")?;
    let count: i64 = stmt.query_row((username, user_id), |row| row.get(0))?;
    Ok(count > 0)
}

/// Account mutation failures that distinguish integrity conflicts from database errors.
#[derive(Debug)]
pub enum AccountMutationError {
    /// The target account does not exist.
    NotFound,
    /// The mutation would remove the final administrator.
    FinalAdmin,
    /// Another account already uses the requested username.
    UsernameExists,
    /// A database operation failed.
    Database,
}

/// Propagates SQLite failures through account mutations without exposing database details.
impl From<rusqlite::Error> for AccountMutationError {
    /// Converts any SQLite failure to the database error variant.
    fn from(_: rusqlite::Error) -> Self {
        Self::Database
    }
}

/// Updates the username and roles in an immediate transaction, preserving the final administrator and checking username uniqueness before committing.
///
/// # Errors
///
/// Returns an account mutation error for an unknown user, an occupied username, final-admin demotion, or a failed database operation.
pub fn update_user_account(
    conn: &mut rusqlite::Connection,
    user_id: i64,
    username: Option<&str>,
    role: Option<Role>,
) -> Result<(), AccountMutationError> {
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    get_user_by_id(&tx, user_id)?.ok_or(AccountMutationError::NotFound)?;
    if matches!(role, Some(Role::Editor | Role::Viewer))
        && user_roles::is_final_admin(&tx, user_id)?
    {
        return Err(AccountMutationError::FinalAdmin);
    }

    if let Some(username) = username {
        if check_username_exists_for_other_user(&tx, username, user_id)? {
            return Err(AccountMutationError::UsernameExists);
        }
        tx.execute(
            "UPDATE users SET username = ?1, updated_at = unixepoch() WHERE id = ?2",
            (username, user_id),
        )?;
    }

    if let Some(role) = role {
        tx.execute("DELETE FROM user_roles WHERE user_id = ?1", [user_id])?;
        tx.execute(
            "INSERT INTO user_roles (user_id, role_id) VALUES (?1, ?2)",
            (user_id, role as i64),
        )?;
        tx.execute(
            "UPDATE users SET updated_at = unixepoch() WHERE id = ?1",
            [user_id],
        )?;
    }

    tx.commit()?;
    Ok(())
}

/// Fetches the stored password hash for a user ID, returning `None` for an unknown account.
///
/// # Errors
///
/// Returns a SQLite error if the lookup fails or the stored hash cannot be decoded.
pub fn get_password_hash_by_user_id(
    conn: &rusqlite::Connection,
    user_id: i64,
) -> Result<Option<String>, rusqlite::Error> {
    let mut stmt = conn.prepare("SELECT password_hash FROM users WHERE id = ?1")?;
    stmt.query_row([user_id], |row| row.get(0)).optional()
}

/// Replaces the stored password hash and revokes every session for the account in one transaction.
///
/// # Errors
///
/// Returns a SQLite error if the password update, session revocation, or transaction commit fails.
pub fn update_user_password(
    conn: &mut rusqlite::Connection,
    user_id: i64,
    password_hash: &str,
) -> Result<(), rusqlite::Error> {
    let tx = conn.transaction()?;
    tx.execute(
        "UPDATE users SET password_hash = ?1, updated_at = unixepoch() WHERE id = ?2",
        (password_hash, user_id),
    )?;
    tx.execute("DELETE FROM sessions WHERE user_id = ?1", [user_id])?;
    tx.commit()
}

/// Deletes an account and its dependent roles and sessions in an immediate transaction that preserves the final administrator.
///
/// # Errors
///
/// Returns an account mutation error if the account is absent, it is the final administrator, or a database operation fails.
pub fn delete_user(
    conn: &mut rusqlite::Connection,
    user_id: i64,
) -> Result<(), AccountMutationError> {
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    if user_roles::is_final_admin(&tx, user_id)? {
        return Err(AccountMutationError::FinalAdmin);
    }
    if tx.execute("DELETE FROM users WHERE id = ?1", [user_id])? == 0 {
        return Err(AccountMutationError::NotFound);
    }
    tx.commit()?;
    Ok(())
}
