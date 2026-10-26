//! Argon2 password hashing and stored-hash verification.

use argon2::{
    Argon2,
    password_hash::{Error, PasswordHasher, PasswordVerifier, phc::PasswordHash},
};

/// Valid PHC hash used for absent accounts so every login performs Argon2id verification with the default memory, iteration, and parallelism costs.
pub const DUMMY_PASSWORD_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHQxMjM0NTY3OA$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

/// Requires at least 12 bytes in UTF-8 for initial and replacement passwords.
///
/// # Errors
///
/// Returns a validation message if the password is shorter than 12 bytes.
pub fn validate_password(password: &str) -> Result<(), &'static str> {
    if password.len() < 12 {
        Err("Password must be at least 12 characters")
    } else {
        Ok(())
    }
}

/// Hashes a password using the default Argon2 parameters and a generated salt, returning its PHC representation.
///
/// # Errors
///
/// Returns an error if password hashing or salt generation fails.
pub fn hash_password(password: &str) -> Result<String, String> {
    let hash = Argon2::default()
        .hash_password(password.as_bytes())
        .map_err(|e| e.to_string())?
        .to_string();

    Ok(hash)
}

/// Hashes a password on Tokio's blocking thread pool so Argon2id computation does not block async worker threads.
///
/// # Errors
///
/// Returns an error if the blocking task fails to join or password hashing fails.
pub async fn hash_password_blocking(password: String) -> Result<String, String> {
    tokio::task::spawn_blocking(move || hash_password(&password))
        .await
        .map_err(|e| e.to_string())?
}

/// Verifies a password against a stored PHC hash, returning `false` for a password mismatch.
///
/// # Errors
///
/// Returns an error if the stored hash is malformed, its parameters are unsupported, or verification otherwise fails.
pub fn verify_password(password: &str, stored_hash: &str) -> Result<bool, String> {
    let parsed = PasswordHash::new(stored_hash).map_err(|e| e.to_string())?;

    match Argon2::default().verify_password(password.as_bytes(), &parsed) {
        Ok(()) => Ok(true),

        Err(Error::PasswordInvalid) => Ok(false),

        Err(e) => Err(e.to_string()),
    }
}

/// Verifies a password against a stored PHC hash on Tokio's blocking thread pool so Argon2id computation does not block async worker threads.
///
/// # Errors
///
/// Returns an error if the blocking task fails to join or password verification fails.
pub async fn verify_password_blocking(
    password: String,
    stored_hash: String,
) -> Result<bool, String> {
    tokio::task::spawn_blocking(move || verify_password(&password, &stored_hash))
        .await
        .map_err(|e| e.to_string())?
}
