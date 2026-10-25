//! Parses CSV uploads; applies shared redirect validation; imports valid rows in one transaction.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use crate::{
    db::tables,
    utils::redirects::{
        DEFAULT_REDIRECT_CODE, INVALID_REDIRECT_CODE, validate_destination, validate_redirect_code,
        validate_slug,
    },
};

/// Maximum number of CSV data rows accepted in one upload.
const MAX_IMPORT_ROWS: usize = 10_000;
/// CSV columns that must appear exactly once in the header.
const REQUIRED_COLUMNS: [&str; 2] = ["slug", "destination"];
/// Additional CSV columns whose missing or blank values use creation defaults.
const OPTIONAL_COLUMNS: [&str; 2] = ["redirect_code", "is_enabled"];

/// CSV records indexed by their validated column names.
struct ParsedCsv {
    /// Parsed data rows keyed by their header names.
    rows: Vec<BTreeMap<String, String>>,
}

/// A failure affecting the entire CSV upload rather than an individual data row.
pub enum ImportError {
    /// CSV header bytes could not be parsed.
    InvalidHeader(String),
    /// A CSV data record could not be parsed or has the wrong column count.
    InvalidRow(String),
    /// No CSV data records followed the header.
    EmptyRows,
    /// The upload exceeds the maximum number of data rows.
    TooManyRows,
    /// A header column has no non-whitespace name.
    EmptyColumnName,
    /// A header column name appears more than once.
    DuplicateColumnName,
    /// The header omits the named required column.
    MissingRequiredColumn(&'static str),
    /// The header contains the named unsupported column.
    UnknownColumn(String),
    /// The database could not import the prepared redirect rows.
    Database,
}

impl ImportError {
    /// Reports whether an import exceeded the permitted number of data rows.
    pub fn is_payload_too_large(&self) -> bool {
        matches!(self, Self::TooManyRows)
    }

    /// Reports whether inserting the prepared redirects failed.
    pub fn is_database_error(&self) -> bool {
        matches!(self, Self::Database)
    }
}

impl fmt::Display for ImportError {
    /// Returns a human-friendly error message
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidHeader(error) => write!(formatter, "Invalid CSV header: {error}"),
            Self::InvalidRow(error) => write!(formatter, "Invalid CSV row: {error}"),
            Self::EmptyRows => formatter.write_str("CSV file does not contain any data rows"),
            Self::TooManyRows => {
                write!(formatter, "CSV files are limited to {MAX_IMPORT_ROWS} rows")
            },
            Self::EmptyColumnName => formatter.write_str("CSV column names cannot be empty"),
            Self::DuplicateColumnName => formatter.write_str("CSV column names must be unique"),
            Self::MissingRequiredColumn(column) => {
                write!(formatter, "CSV file is missing required column '{column}'")
            },
            Self::UnknownColumn(column) => write!(
                formatter,
                "Unexpected CSV column '{column}'; expected slug, destination, redirect_code or is_enabled"
            ),
            Self::Database => formatter.write_str("Failed to import redirects"),
        }
    }
}

/// A validated redirect together with its original one-based CSV data-row number.
struct PreparedRow {
    /// One-based data-row number, excluding the CSV header.
    row_number: usize,
    /// Validated values to be inserted for this row.
    redirect: tables::redirects::NewRedirect,
}

/// The location, available slug, and reason a CSV data row was skipped.
pub struct RowError {
    /// One-based data-row number, excluding the CSV header.
    pub row: usize,
    /// Trimmed slug when the row supplied a nonempty value.
    pub slug: Option<String>,
    /// Reason this data row could not be imported.
    pub message: String,
}

/// Counts and row errors describing a completed CSV import.
pub struct ImportOutcome {
    /// Total number of parsed data rows, including skipped rows.
    pub total_rows: usize,
    /// Number of redirect rows inserted successfully.
    pub imported: usize,
    /// Validation and duplicate-slug errors, sorted by data-row number.
    pub errors: Vec<RowError>,
}

/// Parses a CSV upload and imports valid rows for the authenticated creator, reporting invalid rows and duplicate slugs separately.
///
/// # Errors
///
/// Returns an import error for malformed CSV, invalid headers, empty input, more than 10,000 rows, or a failed database transaction.
pub fn import_csv(
    conn: &mut rusqlite::Connection,
    created_by: i64,
    bytes: &[u8],
) -> Result<ImportOutcome, ImportError> {
    // Frontend checks improve the user experience, but this parser is the integrity boundary.
    let parsed = parse_csv(bytes)?;
    let (prepared_rows, mut errors) = prepare_rows(&parsed);
    let redirects: Vec<tables::redirects::NewRedirect> = prepared_rows
        .iter()
        .map(|row| row.redirect.clone())
        .collect();
    let insert_results = tables::redirects::import_redirects(conn, created_by, &redirects)
        .map_err(|_| ImportError::Database)?;

    let mut imported = 0;
    for (prepared, inserted) in prepared_rows.iter().zip(insert_results) {
        if inserted {
            imported += 1;
        } else {
            errors.push(RowError {
                row: prepared.row_number,
                slug: Some(prepared.redirect.slug.clone()),
                message: "Slug already exists".to_string(),
            });
        }
    }
    errors.sort_by_key(|error| error.row);

    Ok(ImportOutcome {
        total_rows: parsed.rows.len(),
        imported,
        errors,
    })
}

/// Reads trimmed CSV records with a fixed column count and removes an optional UTF-8 BOM from the first header.
///
/// # Errors
///
/// Returns an import error if headers or records are malformed, required columns are missing, or the row-count constraints are violated.
fn parse_csv(bytes: &[u8]) -> Result<ParsedCsv, ImportError> {
    let mut reader = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .flexible(false)
        .from_reader(bytes);
    let mut headers: Vec<String> = reader
        .headers()
        .map_err(|error| ImportError::InvalidHeader(error.to_string()))?
        .iter()
        .map(str::to_string)
        .collect();

    // Spreadsheet exports commonly prefix the first header with a UTF-8 byte-order mark.
    if let Some(first_header) = headers.first_mut() {
        *first_header = first_header.trim_start_matches('\u{feff}').to_string();
    }
    validate_columns(&headers)?;

    let mut rows = Vec::new();
    for record in reader.records() {
        if rows.len() >= MAX_IMPORT_ROWS {
            return Err(ImportError::TooManyRows);
        }
        let record = record.map_err(|error| ImportError::InvalidRow(error.to_string()))?;
        let row = headers
            .iter()
            .cloned()
            .zip(record.iter().map(str::to_string))
            .collect();
        rows.push(row);
    }

    if rows.is_empty() {
        return Err(ImportError::EmptyRows);
    }

    Ok(ParsedCsv { rows })
}

/// Checks that CSV headers are nonempty and unique, contain required columns, and use only supported names.
///
/// # Errors
///
/// Returns an import error for empty, repeated, missing required, or unknown column names.
fn validate_columns(columns: &[String]) -> Result<(), ImportError> {
    if columns.is_empty() || columns.iter().any(|column| column.trim().is_empty()) {
        return Err(ImportError::EmptyColumnName);
    }
    let unique_columns: BTreeSet<&str> = columns.iter().map(String::as_str).collect();
    if unique_columns.len() != columns.len() {
        return Err(ImportError::DuplicateColumnName);
    }

    for required in REQUIRED_COLUMNS {
        if !unique_columns.contains(required) {
            return Err(ImportError::MissingRequiredColumn(required));
        }
    }
    for column in columns {
        if !REQUIRED_COLUMNS.contains(&column.as_str())
            && !OPTIONAL_COLUMNS.contains(&column.as_str())
        {
            return Err(ImportError::UnknownColumn(column.clone()));
        }
    }

    Ok(())
}

/// Separates valid redirects from row validation errors while retaining one-based data-row numbers.
fn prepare_rows(parsed: &ParsedCsv) -> (Vec<PreparedRow>, Vec<RowError>) {
    let mut prepared = Vec::new();
    let mut errors = Vec::new();

    for (index, row) in parsed.rows.iter().enumerate() {
        // Data rows are one-based in API responses; the CSV header is not counted.
        let row_number = index + 1;
        match prepare_row(row) {
            Ok(redirect) => prepared.push(PreparedRow {
                row_number,
                redirect,
            }),
            Err((slug, message)) => errors.push(RowError {
                row: row_number,
                slug,
                message,
            }),
        }
    }

    (prepared, errors)
}

/// Validates one CSV row and applies default redirect status and enabled-state values to empty optional cells.
///
/// # Errors
///
/// Returns the available slug and a validation message if the slug, destination, redirect status, or enabled-state value is invalid.
fn prepare_row(
    row: &BTreeMap<String, String>,
) -> Result<tables::redirects::NewRedirect, (Option<String>, String)> {
    let slug = row
        .get("slug")
        .map(|value| value.trim())
        .unwrap_or_default();
    let slug_for_error = (!slug.is_empty()).then(|| slug.to_string());
    validate_slug(slug).map_err(|message| (slug_for_error, message.to_string()))?;

    let destination = row
        .get("destination")
        .map(|value| value.trim())
        .unwrap_or_default();
    validate_destination(destination)
        .map_err(|message| (Some(slug.to_string()), message.to_string()))?;

    // Missing optional columns and blank optional cells use single-link creation defaults.
    let redirect_code = match optional_value(row, "redirect_code") {
        Some(value) => value
            .parse::<i64>()
            .map_err(|_| (Some(slug.to_string()), INVALID_REDIRECT_CODE.to_string()))?,
        None => DEFAULT_REDIRECT_CODE,
    };
    validate_redirect_code(redirect_code)
        .map_err(|message| (Some(slug.to_string()), message.to_string()))?;

    let is_enabled = match optional_value(row, "is_enabled") {
        Some(value) => parse_boolean(value).ok_or_else(|| {
            (
                Some(slug.to_string()),
                "Enabled value must be true, false, yes, no, 1 or 0".to_string(),
            )
        })?,
        None => true,
    };

    Ok(tables::redirects::NewRedirect {
        slug: slug.to_string(),
        destination: destination.to_string(),
        is_enabled,
        redirect_code,
    })
}

/// Returns a trimmed nonempty CSV cell, treating missing or blank optional cells as absent.
fn optional_value<'a>(row: &'a BTreeMap<String, String>, column: &str) -> Option<&'a str> {
    row.get(column)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
}

/// Parses true/false, yes/no, or 1/0 case-insensitively, returning `None` for unsupported values.
fn parse_boolean(value: &str) -> Option<bool> {
    match value.to_ascii_lowercase().as_str() {
        "true" | "yes" | "1" => Some(true),
        "false" | "no" | "0" => Some(false),
        _ => None,
    }
}
