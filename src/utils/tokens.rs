//! Tokens are used for session authentication and are stored in the database as SHA-256 digests.

use sha2::{Digest, Sha256};

/// Generates a 256-bit random bearer token encoded as lowercase hexadecimal.
///
/// # Errors
///
/// Returns an error if the operating system cannot supply cryptographically secure random bytes.
pub fn generate_session_token() -> Result<String, String> {
    let mut token_bytes = [0u8; 32];
    getrandom::fill(&mut token_bytes).map_err(|e| e.to_string())?;

    Ok(bytes_to_hex(&token_bytes))
}

/// Returns a SHA-256 token digest as lowercase hexadecimal for database storage and lookup.
pub fn hash_session_token(token: &str) -> String {
    let hash = Sha256::digest(token.as_bytes());
    bytes_to_hex(&hash)
}

/// Encodes each byte as two lowercase hexadecimal digits, preserving leading zeroes.
fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
