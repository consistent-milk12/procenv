//! Centralized registry of diagnostic error codes used throughout procenv.
//!
//! These constants document all error codes used in `#[diagnostic(code(...))]`
//! attributes. While Rust's proc-macro system requires literal strings in
//! attributes, this module provides a single source of truth for:
//!
//! - Error code documentation
//! - Programmatic error code matching
//! - External tooling integration
//!
//! # Error Code Format
//!
//! All codes follow the pattern `procenv::<category>` where category describes
//! the error type:
//!
//! | Code | Description |
//! |------|-------------|
//! | `procenv::missing_var` | Required variable not set |
//! | `procenv::invalid_utf8` | Variable contains non-UTF8 bytes |
//! | `procenv::parse_error` | Value failed type conversion |
//! | `procenv::multiple_errors` | Multiple errors occurred |
//! | `procenv::invalid_profile` | Invalid profile name |
//! | `procenv::provider_error` | Provider operation failed |
//! | `procenv::validation_error` | Validation constraint violated |
//! | `procenv::cli_error` | CLI argument parsing failed |
//! | `procenv::file_*` | File-related errors |
//!
//! # Example
//!
//! ```rust,ignore
//! use procenv::diagnostic_codes;
//!
//! // Match on error codes programmatically
//! if error.code() == Some(&diagnostic_codes::MISSING_VAR.into()) {
//!     println!("A required variable is missing");
//! }
//! ```

/// Required environment variable not set.
pub const MISSING_VAR: &str = "procenv::missing_var";

/// Environment variable contains invalid UTF-8.
pub const INVALID_UTF8: &str = "procenv::invalid_utf8";

/// Value failed to parse as expected type.
pub const PARSE_ERROR: &str = "procenv::parse_error";

/// Multiple configuration errors occurred.
pub const MULTIPLE_ERRORS: &str = "procenv::multiple_errors";

/// Invalid profile name specified.
pub const INVALID_PROFILE: &str = "procenv::invalid_profile";

/// Provider operation failed.
pub const PROVIDER_ERROR: &str = "procenv::provider_error";

/// Validation constraint violated.
#[cfg(feature = "validator")]
pub const VALIDATION_ERROR: &str = "procenv::validation_error";

/// Individual field validation error.
#[cfg(feature = "validator")]
pub const FIELD_VALIDATION_ERROR: &str = "procenv::field_validation_error";

/// CLI argument parsing failed.
#[cfg(feature = "clap")]
pub const CLI_ERROR: &str = "procenv::cli_error";

/// Configuration file not found.
#[cfg(feature = "file")]
pub const FILE_NOT_FOUND: &str = "procenv::file::not_found";

/// Configuration file parsing failed.
#[cfg(feature = "file")]
pub const FILE_PARSE_ERROR: &str = "procenv::file::parse_error";

/// Required field missing from file.
#[cfg(feature = "file")]
pub const FILE_MISSING_FIELD: &str = "procenv::file::missing_field";

/// File field type mismatch.
#[cfg(feature = "file")]
pub const FILE_TYPE_ERROR: &str = "procenv::file::type_error";

/// Provider key not found.
pub const PROVIDER_NOT_FOUND: &str = "procenv::provider::not_found";

/// Provider connection error.
pub const PROVIDER_CONNECTION: &str = "procenv::provider::connection";

/// Provider invalid value.
pub const PROVIDER_INVALID_VALUE: &str = "procenv::provider::invalid_value";

/// Provider unavailable.
pub const PROVIDER_UNAVAILABLE: &str = "procenv::provider::unavailable";
