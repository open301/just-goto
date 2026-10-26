//! Username validation shared by account creation and updates.

/// Trims a username and requires 1–64 characters without internal whitespace or control characters.
///
/// # Errors
///
/// Returns a validation message if the trimmed username is empty, too long, or contains whitespace or control characters.
pub fn validate_username(username: &str) -> Result<&str, &'static str> {
    let username = username.trim();
    if username.is_empty() {
        return Err("Username cannot be empty");
    }
    if username.chars().count() > 64 {
        return Err("Username must be at most 64 characters");
    }
    if username
        .chars()
        .any(|character| character.is_whitespace() || character.is_control())
    {
        return Err("Username cannot contain whitespace or control characters");
    }
    Ok(username)
}
