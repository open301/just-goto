//! Literal substring patterns for SQLite LIKE queries.

/// Escapes SQLite LIKE wildcards and the backslash escape character without adding surrounding wildcards.
pub fn escape_like_pattern(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}
