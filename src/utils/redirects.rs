//! Validation methods shared by redirect creation, updates, and CSV imports.

use axum::http::{HeaderValue, uri::Authority};

/// Default redirect code used when none is supplied by the user. 302 is temporary redirect rather than permanent.
pub const DEFAULT_REDIRECT_CODE: i64 = 302;
/// Validation message shared by JSON writes and CSV status parsing.
pub const INVALID_REDIRECT_CODE: &str = "Redirect code must be one of: [301, 302, 307, 308]";

/// Checks slug syntax before rejecting names occupied by service routes.
///
/// # Errors
///
/// Returns a validation message if the slug is empty, exceeds 128 ASCII characters, contains unsupported characters, or uses a reserved route name.
pub fn validate_slug(slug: &str) -> Result<(), &'static str> {
    if slug.is_empty()
        || slug.len() > 128
        || !slug
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err("Slug must be 1-128 characters using letters, numbers, '-' or '_'");
    }
    if matches!(
        slug,
        "health" | "auth" | "users" | "redirects" | "swagger-ui" | "assets" | "api" | "api-docs"
    ) {
        return Err("Slug is reserved by the API");
    }
    Ok(())
}

/// Requires an absolute HTTP(S) destination with a valid authority and a safe Location header without normalizing the value.
///
/// # Errors
///
/// Returns a validation message if the destination exceeds 2048 bytes, lacks an HTTP or HTTPS prefix or valid host and port, contains whitespace or control characters, or cannot be encoded as a header.
pub fn validate_destination(destination: &str) -> Result<(), &'static str> {
    let invalid = "Destination must be an absolute HTTP or HTTPS URL";
    if destination.len() > 2048
        || destination
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
        || destination.contains('\\')
        || HeaderValue::from_str(destination).is_err()
    {
        return Err(invalid);
    }
    let remainder = destination
        .strip_prefix("http://")
        .or_else(|| destination.strip_prefix("https://"))
        .ok_or(invalid)?;
    let authority = remainder.split(['/', '?', '#']).next().unwrap_or_default();
    if !valid_authority(authority) {
        return Err(invalid);
    }
    Ok(())
}

/// Validates a parsed authority's DNS or IP host and optional nonzero numeric port, including localhost and bracketed IPv6 addresses.
fn valid_authority(value: &str) -> bool {
    let Ok(authority) = value.parse::<Authority>() else {
        return false;
    };
    let host = authority.host();
    let valid_host = if let Some(address) = host.strip_prefix('[').and_then(|s| s.strip_suffix(']'))
    {
        address.parse::<std::net::Ipv6Addr>().is_ok()
    } else {
        let name = host.strip_suffix('.').unwrap_or(host);
        !name.is_empty()
            && name.split('.').all(|label| {
                !label.is_empty()
                    && !label.starts_with('-')
                    && !label.ends_with('-')
                    && label
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
            })
    };
    let host_port = value.rsplit('@').next().unwrap_or_default();
    let suffix = host_port.strip_prefix(host).unwrap_or_default();
    valid_host
        && (suffix.is_empty()
            || suffix.strip_prefix(':').is_some_and(|port| {
                !port.is_empty()
                    && port.bytes().all(|byte| byte.is_ascii_digit())
                    && port.parse::<u16>().is_ok_and(|port| port > 0)
            }))
}

/// Allows only the redirect status codes supported by writes and CSV imports.
///
/// # Errors
///
/// Returns a validation message unless the status is 301, 302, 307, or 308.
pub fn validate_redirect_code(code: i64) -> Result<(), &'static str> {
    if matches!(code, 301 | 302 | 307 | 308) {
        Ok(())
    } else {
        Err(INVALID_REDIRECT_CODE)
    }
}
