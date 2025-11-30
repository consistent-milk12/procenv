//! Error types for configuration loading.
//!
//! This module contains the [`Error`] enum and related functionality for
//! handling configuration errors with rich diagnostics via [`miette`].
//!
//! # Error Variants
//!
//! | Variant | When It Occurs |
//! |---------|----------------|
//! | [`Error::Missing`] | Required environment variable not set |
//! | [`Error::InvalidUtf8`] | Variable contains non-UTF8 bytes |
//! | [`Error::Parse`] | Value failed to parse as expected type |
//! | [`Error::Multiple`] | Multiple configuration errors accumulated |
//! | [`Error::File`] | Configuration file error (with `file` feature) |
//! | [`Error::InvalidProfile`] | Invalid profile name specified |
//! | [`Error::Provider`] | Custom provider operation failed |
//! | [`Error::Validation`] | Validation constraint violated (with `validator` feature) |
//! | [`Error::Cli`] | CLI argument parsing failed (with `clap` feature) |
//!
//! # Error Accumulation
//!
//! Unlike typical Rust error handling that fails on the first error, procenv
//! accumulates all errors encountered during configuration loading. This allows
//! users to see every configuration issue at once rather than fixing them one
//! by one.
//!
//! ```rust,ignore
//! match Config::from_env() {
//!     Ok(config) => { /* use config */ }
//!     Err(Error::Multiple { errors }) => {
//!         // All errors reported together
//!         for error in errors {
//!             eprintln!("{}", error);
//!         }
//!     }
//!     Err(e) => eprintln!("{}", e),
//! }
//! ```
//!
//! # Secret Masking
//!
//! Fields marked with `secret` have their values redacted in error messages
//! and Debug output to prevent accidental exposure of sensitive data:
//!
//! ```text
//! failed to parse API_KEY: expected String, got <redacted>
//! ```

use std::error::Error as StdError;
use std::fmt::{self, Debug, Display, Formatter};

use miette::Diagnostic;

#[cfg(feature = "file")]
use crate::file;

#[cfg(feature = "validator")]
use crate::validation::ValidationFieldError;

/// Errors that can occur when loading configuration from environment variables.
///
/// This enum represents all possible failure modes when loading configuration.
/// It integrates with [`miette`] to provide rich diagnostic output with error
/// codes, help text, and source locations where applicable.
///
/// # Error Accumulation
///
/// The generated `from_env()` method accumulates all errors rather than
/// failing on the first one. When multiple errors occur, they are wrapped
/// in the [`Error::Multiple`] variant, allowing users to see all issues at once.
///
/// # Example
///
/// ```rust,ignore
/// match Config::from_env() {
///     Ok(config) => { /* success */ }
///     Err(Error::Missing { var, .. }) => {
///         eprintln!("Missing required variable: {}", var);
///     }
///     Err(Error::Parse { var, expected_type, .. }) => {
///         eprintln!("{} must be a valid {}", var, expected_type);
///     }
///     Err(Error::Multiple { errors }) => {
///         eprintln!("{} configuration errors found", errors.len());
///     }
///     Err(e) => {
///         // Pretty-print any error with miette
///         eprintln!("{:?}", miette::Report::from(e));
///     }
/// }
/// ```
///
/// # Diagnostic Codes
///
/// Each variant has a unique diagnostic code for easy identification:
///
/// | Code | Meaning |
/// |------|---------|
/// | `procenv::missing_var` | Required environment variable not set |
/// | `procenv::invalid_utf8` | Variable contains non-UTF8 bytes |
/// | `procenv::parse_error` | Value failed to parse as expected type |
/// | `procenv::multiple_errors` | Multiple configuration errors occurred |
/// | `procenv::invalid_profile` | Invalid profile name specified |
#[derive(Diagnostic)]
pub enum Error {
    /// A required environment variable was not set.
    #[diagnostic(
        code(procenv::missing_var),
        url("https://docs.rs/procenv"),
        severity(Error)
    )]
    Missing {
        /// The name of the missing environment variable.
        /// Uses String to support runtime-constructed var names (e.g., with prefixes).
        var: String,

        /// Dynamic help message (allows customization per-field).
        #[help]
        help: String,
    },

    /// An environment variable contains invalid UTF-8.
    #[diagnostic(
        code(procenv::invalid_utf8),
        help("ensure the variable contains valid UTF-8 text")
    )]
    InvalidUtf8 {
        /// The name of the environment variable with invalid UTF-8.
        /// Uses String to support runtime-constructed var names (e.g., with prefixes).
        var: String,
    },

    /// An environment variable value could not be parsed into the expected type.
    #[diagnostic(code(procenv::parse_error))]
    Parse {
        /// The name of the environment variable.
        /// Uses String to support runtime-constructed var names (e.g., with prefixes).
        var: String,

        /// The raw string value that failed to parse.
        value: String,

        /// Whether this field is marked as secret.
        secret: bool,

        /// The expected type name (for diagnostic messages).
        expected_type: String,

        /// Dynamic help text generated based on expected_type.
        #[help]
        help: String,

        /// The underlying parse error from `FromStr`.
        ///
        /// Note: We use a plain field (not `#[diagnostic_source]`) because std
        /// parse errors don't implement Diagnostic. The error chain is still
        /// displayed via std::error::Error::source() when using miette::Report.
        source: Box<dyn StdError + Send + Sync>,
    },

    /// Multiple configuration errors occurred.
    ///
    /// Uses miette's `#[related]` to render all errors together
    /// in a visually grouped format.
    #[diagnostic(
        code(procenv::multiple_errors),
        help("fix all listed configuration errors")
    )]
    Multiple {
        /// All accumulated errors.
        /// miette renders these as related diagnostics.
        #[related]
        errors: Vec<Error>,
    },

    /// An error occurred while loading a configuration file.
    ///
    /// This variant wraps `FileError` with diagnostic transparency,
    /// so miette will display the rich source-code snippets from FileError.
    #[cfg(feature = "file")]
    #[diagnostic(transparent)]
    File {
        /// The underlying file error with source location.
        #[diagnostic_source]
        source: file::FileError,
    },

    /// An invalid profile was specified.
    ///
    /// This occurs when the profile environment variable contains a value
    /// that is not in the list of valid profiles.
    #[diagnostic(code(procenv::invalid_profile), severity(Error))]
    InvalidProfile {
        /// The invalid profile value that was provided.
        profile: String,

        /// The environment variable that contained the invalid profile.
        var: &'static str,

        /// List of valid profile names.
        valid_profiles: Vec<&'static str>,

        /// Dynamic help message listing valid profiles.
        #[help]
        help: String,
    },

    /// An error occured in a configuration provider.
    #[diagnostic(code(procenv::provider_error))]
    Provider {
        /// The provider that failed.
        provider: String,

        /// Error message.
        message: String,

        /// Help text.
        #[help]
        help: String,
    },

    /// A validation error occurred after loading configuration.
    ///
    /// This variant wraps errors from the `validator` crate and provides
    /// structured information about which fields failed validation.
    #[cfg(feature = "validator")]
    #[diagnostic(
        code(procenv::validation_error),
        help("fix the validation errors listed above")
    )]
    Validation {
        /// The validation errors from the validator crate.
        ///
        /// Each entry maps a field name to a list of validation error messages.
        #[related]
        errors: Vec<ValidationFieldError>,
    },

    /// An error occurred while parsing CLI arguments.
    ///
    /// This variant wraps errors from the `clap` crate when CLI argument
    /// parsing fails.
    #[cfg(feature = "clap")]
    #[diagnostic(
        code(procenv::cli_error),
        help("check the CLI arguments and try again")
    )]
    Cli {
        /// The error message from clap.
        message: String,
    },

    /// A requested configuration key was not found.
    #[diagnostic(code(procenv::key_not_found))]
    KeyNotFound {
        /// The key that was requested.
        key: String,

        /// List of available keys.
        available: Vec<String>,

        /// Help message.
        #[help]
        help: String,
    },

    /// A type mismatch occurred during runtime value access.
    #[diagnostic(code(procenv::type_mismatch))]
    TypeMismatch {
        /// The key being accessed.
        key: String,

        /// The expected type.
        expected: &'static str,

        /// The actual type found.
        found: &'static str,

        /// Help message.
        #[help]
        help: String,
    },
}

#[cfg(feature = "file")]
impl From<file::FileError> for Error {
    fn from(source: file::FileError) -> Self {
        Error::File { source }
    }
}

// Manual Display impl for secret masking
// Note: For fancy formatted output, use `miette::Report::from(error)`
impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Error::Missing { var, .. } => {
                write!(f, "missing required environment variable: {}", var)
            }

            Error::InvalidUtf8 { var } => {
                write!(f, "environment variable {} contains invalid UTF-8", var)
            }

            Error::Parse {
                var,
                value,
                secret,
                expected_type,
                ..
            } => {
                if *secret {
                    write!(
                        f,
                        "failed to parse {}: expected {}, got <redacted>",
                        var, expected_type
                    )
                } else {
                    write!(
                        f,
                        "failed to parse {}: expected {}, got {:?}",
                        var, expected_type, value
                    )
                }
            }

            Error::Multiple { errors } => {
                write!(f, "{} configuration error(s) occurred", errors.len())
            }

            #[cfg(feature = "file")]
            Error::File { source } => {
                write!(f, "configuration file error: {}", source)
            }

            Error::InvalidProfile { profile, var, .. } => {
                write!(f, "invalid profile '{}' for {}", profile, var)
            }

            Error::Provider {
                provider, message, ..
            } => {
                write!(f, "error connecting to {provider}: {message}")
            }

            #[cfg(feature = "validator")]
            Error::Validation { errors } => {
                write!(f, "{} validation error(s) occurred", errors.len())
            }

            #[cfg(feature = "clap")]
            Error::Cli { message } => {
                write!(f, "CLI argument error: {}", message)
            }

            Error::KeyNotFound { key, .. } => {
                write!(f, "configuration key not found: {}", key)
            }

            Error::TypeMismatch {
                key,
                expected,
                found,
                ..
            } => {
                write!(
                    f,
                    "type mismatch for '{}': expected {}, found {}",
                    key, expected, found
                )
            }
        }
    }
}

// Manual Debug impl for secret masking
impl Debug for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Error::Missing { var, help } => f
                .debug_struct("Missing")
                .field("var", var)
                .field("help", help)
                .finish(),

            Error::InvalidUtf8 { var } => f.debug_struct("InvalidUtf8").field("var", var).finish(),

            Error::Parse {
                var,
                value,
                secret,
                expected_type,
                help,
                source,
            } => {
                let mut debug = f.debug_struct("Parse");
                debug.field("var", var);

                if *secret {
                    debug.field("value", &"<redacted>");
                } else {
                    debug.field("value", value);
                }

                debug
                    .field("secret", secret)
                    .field("expected_type", expected_type)
                    .field("help", help)
                    .field("source", source)
                    .finish()
            }

            Error::Multiple { errors } => {
                f.debug_struct("Multiple").field("errors", errors).finish()
            }

            #[cfg(feature = "file")]
            Error::File { source } => f.debug_struct("File").field("source", source).finish(),

            Error::InvalidProfile {
                profile,
                var,
                valid_profiles,
                help,
            } => f
                .debug_struct("InvalidProfile")
                .field("profile", profile)
                .field("var", var)
                .field("valid_profiles", valid_profiles)
                .field("help", help)
                .finish(),

            Error::Provider {
                provider,
                message,
                help,
            } => f
                .debug_struct("Provider")
                .field("provider", provider)
                .field("message", message)
                .field("help", help)
                .finish(),

            #[cfg(feature = "validator")]
            Error::Validation { errors } => f
                .debug_struct("Validation")
                .field("errors", errors)
                .finish(),

            #[cfg(feature = "clap")]
            Error::Cli { message } => f.debug_struct("Cli").field("message", message).finish(),

            Error::KeyNotFound {
                key,
                available,
                help,
            } => f
                .debug_struct("KeyNotFound")
                .field("key", key)
                .field("available", available)
                .field("help", help)
                .finish(),

            Error::TypeMismatch {
                key,
                expected,
                found,
                help,
            } => f
                .debug_struct("TypeMismatch")
                .field("key", key)
                .field("expected", expected)
                .field("found", found)
                .field("help", help)
                .finish(),
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Error::Parse { source, .. } => Some(source.as_ref()),
            #[cfg(feature = "file")]
            Error::File { source } => Some(source),
            _ => None,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Constructor helpers for ergonomic error creation
// ─────────────────────────────────────────────────────────────────────────────

impl Error {
    /// Creates a Missing error with a standard help message.
    ///
    /// Accepts any type that can be converted to String, including
    /// `&str`, `String`, or runtime-constructed var names.
    pub fn missing(var: impl Into<String>) -> Self {
        let var = var.into();
        let help = format!("set {} in your environment or .env file", var);
        Error::Missing { var, help }
    }

    /// Creates a Parse error with appropriate help text.
    ///
    /// Accepts any type that can be converted to String for var and expected_type,
    /// allowing runtime-constructed var names and type names.
    pub fn parse(
        var: impl Into<String>,
        value: impl Into<String>,
        secret: bool,
        expected_type: impl Into<String>,
        source: Box<dyn StdError + Send + Sync>,
    ) -> Self {
        let var = var.into();
        let value = value.into();
        let expected_type = expected_type.into();
        let help = format!("expected a valid {}", expected_type);
        Error::Parse {
            var,
            value,
            secret,
            expected_type,
            help,
            source,
        }
    }

    /// Collects multiple errors into a single Multiple error.
    /// Returns None if the input is empty.
    pub fn multiple(errors: Vec<Error>) -> Option<Self> {
        if errors.is_empty() {
            None
        } else if errors.len() == 1 {
            // Unwrap single error instead of wrapping
            errors.into_iter().next()
        } else {
            Some(Error::Multiple { errors })
        }
    }

    /// Creates an InvalidProfile error.
    pub fn invalid_profile(
        profile: String,
        var: &'static str,
        valid_profiles: Vec<&'static str>,
    ) -> Self {
        let valid_list = valid_profiles.join(", ");
        Error::InvalidProfile {
            profile,
            var,
            help: format!("valid profiles are: {}", valid_list),
            valid_profiles,
        }
    }

    /// Creates a KeyNotFound error.
    pub fn key_not_found(key: impl Into<String>, available: Vec<String>) -> Self {
        let available_str = if available.is_empty() {
            "none".to_string()
        } else {
            available.join(", ")
        };
        Error::KeyNotFound {
            key: key.into(),
            help: format!("available keys: {}", available_str),
            available,
        }
    }

    /// Creates a TypeMismatch error.
    pub fn type_mismatch(
        key: impl Into<String>,
        expected: &'static str,
        found: &'static str,
    ) -> Self {
        Error::TypeMismatch {
            key: key.into(),
            expected,
            found,
            help: format!(
                "the value is stored as {}, try accessing it as that type",
                found
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_missing() {
        let err = Error::missing("DATABASE_URL");
        let display = err.to_string();
        assert!(display.contains("DATABASE_URL"));
        assert!(display.contains("missing"));
    }

    #[test]
    fn test_error_parse_non_secret() {
        let err = Error::parse(
            "PORT",
            "invalid".to_string(),
            false,
            "u16",
            Box::new(std::fmt::Error),
        );
        let display = err.to_string();
        assert!(display.contains("PORT"));
        assert!(display.contains("invalid"));
        assert!(display.contains("u16"));
    }

    #[test]
    fn test_error_parse_secret_redacted() {
        let err = Error::parse(
            "API_KEY",
            "secret-value".to_string(),
            true,
            "String",
            Box::new(std::fmt::Error),
        );
        let display = err.to_string();
        assert!(display.contains("API_KEY"));
        assert!(display.contains("<redacted>"));
        assert!(!display.contains("secret-value"));
    }

    #[test]
    fn test_error_multiple() {
        let errors = vec![Error::missing("VAR1"), Error::missing("VAR2")];
        let err = Error::multiple(errors).unwrap();

        if let Error::Multiple { errors } = err {
            assert_eq!(errors.len(), 2);
        } else {
            panic!("Expected Multiple variant");
        }
    }

    #[test]
    fn test_error_multiple_single_unwraps() {
        let errors = vec![Error::missing("VAR1")];
        let err = Error::multiple(errors).unwrap();

        // Single error should be unwrapped, not wrapped in Multiple
        assert!(matches!(err, Error::Missing { .. }));
    }

    #[test]
    fn test_error_multiple_empty_returns_none() {
        let result = Error::multiple(vec![]);
        assert!(result.is_none());
    }
}
