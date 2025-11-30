//! # procenv
//!
//! A procedural macro library for type-safe environment variable configuration in Rust.
//!
//! `procenv` eliminates boilerplate code when loading configuration from environment variables
//! by generating type-safe loading logic at compile time. It provides comprehensive error handling,
//! secret masking, and support for multiple configuration sources including `.env` files,
//! config files (TOML/JSON/YAML), CLI arguments, and environment variable profiles.
//!
//! ## Features
//!
//! - **Type-safe parsing** - Automatic conversion using `FromStr` or serde deserialization
//! - **Error accumulation** - Reports all configuration errors at once, not just the first
//! - **Secret masking** - Protects sensitive values in `Debug` output and error messages
//! - **Multiple sources** - Supports env vars, `.env` files, config files, and CLI arguments
//! - **Source attribution** - Tracks where each value originated for debugging
//! - **Profile support** - Environment-specific defaults (dev, staging, prod)
//! - **Rich diagnostics** - Beautiful error messages via [`miette`]
//!
//! ## Quick Start
//!
//! ```rust,ignore
//! use procenv::EnvConfig;
//!
//! #[derive(EnvConfig)]
//! struct Config {
//!     #[env(var = "DATABASE_URL")]
//!     db_url: String,
//!
//!     #[env(var = "PORT", default = "8080")]
//!     port: u16,
//!
//!     #[env(var = "API_KEY", secret)]
//!     api_key: String,
//!
//!     #[env(var = "DEBUG_MODE", optional)]
//!     debug: Option<bool>,
//! }
//!
//! fn main() -> Result<(), procenv::Error> {
//!     let config = Config::from_env()?;
//!     println!("Server running on port {}", config.port);
//!     Ok(())
//! }
//! ```
//!
//! ## Field Attributes
//!
//! | Attribute | Description |
//! |-----------|-------------|
//! | `var = "NAME"` | Environment variable name (required) |
//! | `default = "value"` | Default value if env var is missing |
//! | `optional` | Field becomes `Option<T>`, `None` if missing |
//! | `secret` | Masks value in Debug output and errors |
//! | `no_prefix` | Skip struct-level prefix for this field |
//! | `flatten` | Embed nested config struct |
//! | `format = "json"` | Parse value as JSON/TOML/YAML |
//!
//! ## Struct Attributes
//!
//! ```rust,ignore
//! #[derive(EnvConfig)]
//! #[env_config(
//!     prefix = "APP_",                           // Prefix all env vars
//!     dotenv,                                    // Load .env file
//!     file_optional = "config.toml",             // Optional config file
//!     profile_env = "APP_ENV",                   // Profile selection var
//!     profiles = ["dev", "staging", "prod"]      // Valid profiles
//! )]
//! struct Config {
//!     // ...
//! }
//! ```
//!
//! ## Generated Methods
//!
//! The derive macro generates several methods on your struct:
//!
//! | Method | Description |
//! |--------|-------------|
//! | `from_env()` | Load from environment variables |
//! | `from_env_with_sources()` | Load with source attribution |
//! | `from_config()` | Load from files + env vars (layered) |
//! | `from_config_with_sources()` | Layered loading with source attribution |
//! | `from_args()` | Load from CLI arguments + env |
//! | `env_example()` | Generate `.env.example` template |
//!
//! ## Feature Flags
//!
//! | Feature | Description | Default |
//! |---------|-------------|---------|
//! | `file` | Config file support | No |
//! | `toml` | TOML file parsing | No |
//! | `yaml` | YAML file parsing | No |
//! | `secrecy` | [`secrecy`] crate integration | No |
//!
//! ## Error Handling
//!
//! All errors are reported through the [`Error`] type, which integrates with
//! [`miette`] for rich terminal diagnostics:
//!
//! ```rust,ignore
//! match Config::from_env() {
//!     Ok(config) => { /* use config */ }
//!     Err(e) => {
//!         // Pretty-print with miette for beautiful error output
//!         eprintln!("{:?}", miette::Report::from(e));
//!     }
//! }
//! ```

#![allow(unused, reason = "False warnings")]

pub use procenv_macro::EnvConfig;

#[cfg(feature = "secrecy")]
pub use secrecy::{ExposeSecret, ExposeSecretMut, SecretBox, SecretString};

// File configuration support (Phase 13)
#[cfg(feature = "file")]
pub mod file;
#[cfg(feature = "file")]
pub use file::{ConfigBuilder, FileFormat, FileUtils, OriginTracker};

// Provider extensibility (Phase C)
pub mod loader;
pub mod provider;
// pub mod value;

#[cfg(feature = "dotenv")]
pub use provider::DotenvProvider;
#[cfg(feature = "file")]
pub use provider::FileProvider;
#[cfg(feature = "async")]
pub use provider::{AsyncProvider, BlockingAdapter, BoxFuture};
pub use provider::{
    EnvProvider, Provider, ProviderError, ProviderResult, ProviderSource, ProviderValue,
};

pub use loader::ConfigLoader;

use std::fmt::{self, Display, Formatter};
use std::path::PathBuf;
use std::{error::Error as StdError, fmt::Debug};

use miette::{Diagnostic, Severity};

// ============================================================================
// Diagnostic Code Registry
// ============================================================================

/// Centralized registry of diagnostic error codes used throughout procenv.
///
/// These constants document all error codes used in `#[diagnostic(code(...))]`
/// attributes. While Rust's proc-macro system requires literal strings in
/// attributes, this module provides a single source of truth for:
///
/// - Error code documentation
/// - Programmatic error code matching
/// - External tooling integration
///
/// # Error Code Format
///
/// All codes follow the pattern `procenv::<category>` where category describes
/// the error type:
///
/// | Code | Description |
/// |------|-------------|
/// | `procenv::missing_var` | Required variable not set |
/// | `procenv::invalid_utf8` | Variable contains non-UTF8 bytes |
/// | `procenv::parse_error` | Value failed type conversion |
/// | `procenv::multiple_errors` | Multiple errors occurred |
/// | `procenv::invalid_profile` | Invalid profile name |
/// | `procenv::provider_error` | Provider operation failed |
/// | `procenv::validation_error` | Validation constraint violated |
/// | `procenv::cli_error` | CLI argument parsing failed |
/// | `procenv::file_*` | File-related errors |
///
/// # Example
///
/// ```rust,ignore
/// use procenv::diagnostic_codes;
///
/// // Match on error codes programmatically
/// if error.code() == Some(&diagnostic_codes::MISSING_VAR.into()) {
///     println!("A required variable is missing");
/// }
/// ```
pub mod diagnostic_codes {
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
}

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

#[cfg(feature = "validator")]
#[derive(Debug, Diagnostic)]
#[diagnostic(code(procenv::field_validation_error))]
pub struct ValidationFieldError {
    /// The field name that failed validation.
    pub field: String,

    /// The validation rule that failed (e.g., "email", "range", "url").
    pub code: String,

    /// Human-readable error message.
    #[help]
    pub message: String,

    /// Additional parameters from the validation rule (e.g., min/max values).
    pub params: Option<String>,
}

#[cfg(feature = "validator")]
impl ValidationFieldError {
    /// Create a new validation field error.
    pub fn new(
        field: impl Into<String>,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            field: field.into(),
            code: code.into(),
            message: message.into(),
            params: None,
        }
    }

    /// Add parameters to the error (e.g., "min: 1, max: 100").
    pub fn with_params(mut self, params: impl Into<String>) -> Self {
        self.params = Some(params.into());

        self
    }

    /// Extract the human-readable message from a validation error.
    ///
    /// Returns the custom message if set, otherwise generates a default
    /// message using the validation code.
    fn extract_message(error: &::validator::ValidationError) -> String {
        error
            .message
            .as_ref()
            .map(|m| m.to_string())
            .unwrap_or_else(|| format!("validation failed: {}", error.code))
    }

    /// Extract validation parameters as a formatted string.
    ///
    /// Filters out the "value" parameter (which contains the actual value)
    /// and formats remaining parameters as "key: value" pairs.
    fn extract_params(error: &::validator::ValidationError) -> Option<String> {
        if error.params.is_empty() {
            return None;
        }

        let param_strs: Vec<String> = error
            .params
            .iter()
            .filter(|(k, _)| *k != "value")
            .map(|(k, v)| format!("{k}: {v}"))
            .collect();

        if param_strs.is_empty() {
            None
        } else {
            Some(param_strs.join(", "))
        }
    }

    /// Create a ValidationFieldError from a validator error.
    fn from_validator_error(field: &str, error: &::validator::ValidationError) -> Self {
        let message = Self::extract_message(error);
        let params = Self::extract_params(error);

        let mut err = Self::new(field.to_string(), error.code.to_string(), message);

        if let Some(p) = params {
            err = err.with_params(p);
        }

        err
    }

    /// Convert validator crate errors to our error type.
    pub fn validation_errors_to_procenv(
        errors: ::validator::ValidationErrors,
    ) -> Vec<ValidationFieldError> {
        // Collect flat field errors
        let flat_errors = errors
            .field_errors()
            .into_iter()
            .flat_map(|(field, field_errors)| {
                field_errors
                    .iter()
                    .map(move |error| Self::from_validator_error(&field, error))
            });

        // Collect nested struct errors with prefixed field paths
        let nested_errors = errors.errors().into_iter().filter_map(|(field, nested)| {
            if let ::validator::ValidationErrorsKind::Struct(nested) = nested {
                let nested_field_errors = Self::validation_errors_to_procenv(*nested.clone());
                Some(nested_field_errors.into_iter().map(move |mut err| {
                    err.field = format!("{}.{}", field, err.field);
                    err
                }))
            } else {
                None
            }
        });

        flat_errors.chain(nested_errors.flatten()).collect()
    }
}

#[cfg(feature = "validator")]
impl Display for ValidationFieldError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "field `{}` failed validation: {}",
            self.field, self.message
        )
    }
}

#[cfg(feature = "validator")]
impl StdError for ValidationFieldError {}

/// Standalone function to convert validator crate errors to procenv errors.
///
/// This is a convenience wrapper around [`ValidationFieldError::validation_errors_to_procenv`].
/// It's exported at the crate root for use in generated code.
#[cfg(feature = "validator")]
pub fn validation_errors_to_procenv(
    errors: ::validator::ValidationErrors,
) -> Vec<ValidationFieldError> {
    ValidationFieldError::validation_errors_to_procenv(errors)
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
}

// ============================================================================
// Source Attribution
// ============================================================================

/// Indicates where a configuration value originated from.
///
/// This enum is used for source attribution, allowing you to track
/// whether a value came from an environment variable, a config file,
/// CLI arguments, or other sources. This is useful for debugging
/// configuration issues and understanding the layering behavior.
///
/// # Priority Order
///
/// When using `from_config()` or `from_args()`, sources are checked
/// in priority order (highest to lowest):
///
/// 1. **CLI arguments** - `--port 8080`
/// 2. **Environment variables** - `PORT=8080`
/// 3. **Dotenv files** - `.env` file
/// 4. **Profile defaults** - `#[profile(dev = "...")]`
/// 5. **Config files** - `config.toml`
/// 6. **Macro defaults** - `#[env(default = "...")]`
///
/// # Example
///
/// ```rust,ignore
/// let (config, sources) = Config::from_env_with_sources()?;
///
/// for (field, source) in sources.iter() {
///     match source.source {
///         Source::Environment => println!("{}: from env", field),
///         Source::DotenvFile(_) => println!("{}: from .env", field),
///         Source::Default => println!("{}: using default", field),
///         _ => {}
///     }
/// }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Source {
    /// Value was provided via a CLI argument (e.g., `--port 8080`).
    ///
    /// This has the highest priority and overrides all other sources.
    Cli,

    /// Value was read directly from an environment variable.
    ///
    /// This indicates the variable was set in the process environment
    /// before any `.env` file loading occurred.
    Environment,

    /// Value was loaded from a `.env` file.
    ///
    /// The optional [`PathBuf`] contains the path to the file if known.
    /// When multiple `.env` files are loaded, later files override earlier ones.
    DotenvFile(Option<PathBuf>),

    /// Value was loaded from a configuration file (TOML, JSON, or YAML).
    ///
    /// The optional [`PathBuf`] contains the path to the file.
    /// This source is used when `#[env_config(file = "...")]` is configured.
    ConfigFile(Option<PathBuf>),

    /// Value came from a profile-specific default.
    ///
    /// The string contains the profile name (e.g., "dev", "prod").
    /// Profile defaults are specified with `#[profile(dev = "...")]`.
    Profile(String),

    /// Value came from the compile-time default in the attribute.
    ///
    /// This is the fallback when no environment variable, file, or
    /// profile provides a value. Specified with `#[env(default = "...")]`.
    Default,

    /// No value was provided (for optional fields).
    ///
    /// This only applies to fields marked with `optional` that have
    /// no value from any source. The field value will be `None`.
    NotSet,

    /// Value came from a custom provider.
    ///
    /// The string contains the provider name (e.g., "valut", "aws-ssm").
    CustomProvider(String),
}

impl Display for Source {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Source::Cli => write!(f, "CLI argument"),

            Source::Environment => write!(f, "Environment variable"),

            Source::DotenvFile(Some(path)) => write!(f, ".env file ({})", path.display()),

            Source::DotenvFile(None) => write!(f, ".env file"),

            Source::ConfigFile(Some(path)) => write!(f, "Config file ({})", path.display()),

            Source::ConfigFile(None) => write!(f, "Config file"),

            Source::Profile(name) => write!(f, "Profile ({})", name),

            Source::Default => write!(f, "Default value"),

            Source::NotSet => write!(f, "Not set"),

            Source::CustomProvider(name) => write!(f, "Custom provider ({name})"),
        }
    }
}

/// Source information for a single configuration value.
///
/// This struct pairs an environment variable name with its [`Source`],
/// indicating where the value was loaded from. It's used as an entry
/// in [`ConfigSources`] for per-field source attribution.
///
/// # Example
///
/// ```rust,ignore
/// let source = ValueSource::new("DATABASE_URL", Source::Environment);
/// println!("{}", source);  // "DATABASE_URL: Environment variable"
/// ```
#[derive(Clone, Debug)]
pub struct ValueSource {
    /// The environment variable name (e.g., `"DATABASE_URL"`).
    pub var_name: String,

    /// Where the value originated from.
    pub source: Source,
}

impl ValueSource {
    /// Creates a new `ValueSource` with the given variable name and source.
    ///
    /// # Arguments
    ///
    /// * `var_name` - The environment variable name (accepts `&str`, `String`, etc.)
    /// * `source` - Where the value originated from
    pub fn new(var_name: impl Into<String>, source: Source) -> Self {
        Self {
            var_name: var_name.into(),
            source,
        }
    }
}

impl Display for ValueSource {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.var_name, self.source)
    }
}

/// Collection of source attributions for all configuration fields.
///
/// This struct tracks where each configuration value originated from,
/// enabling debugging and auditing of configuration loading. It's returned
/// by methods like `from_env_with_sources()` and `from_config_with_sources()`.
///
/// # Example
///
/// ```rust,ignore
/// use procenv::EnvConfig;
///
/// #[derive(EnvConfig)]
/// #[env_config(dotenv)]
/// struct Config {
///     #[env(var = "DATABASE_URL")]
///     db_url: String,
///
///     #[env(var = "PORT", default = "8080")]
///     port: u16,
/// }
///
/// let (config, sources) = Config::from_env_with_sources()?;
///
/// // Print all sources
/// println!("{}", sources);
///
/// // Check a specific field
/// if let Some(source) = sources.get("port") {
///     match source.source {
///         Source::Default => println!("Using default port"),
///         Source::Environment => println!("Port from environment"),
///         _ => {}
///     }
/// }
///
/// // Iterate over all sources
/// for (field, source) in sources.iter() {
///     println!("{}: {} [{}]", field, source.source, source.var_name);
/// }
/// ```
///
/// # Display Output
///
/// When printed, `ConfigSources` produces a formatted table:
///
/// ```text
/// Configuration Source:
/// --------------------------------------------------
///   db_url  <- Environment variable [DATABASE_URL]
///   port    <- Default value [PORT]
/// ```
#[derive(Clone, Debug, Default)]
pub struct ConfigSources {
    entries: Vec<(String, ValueSource)>,
}

impl ConfigSources {
    /// Creates a new empty `ConfigSources` collection.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Adds a source entry for a field.
    ///
    /// # Arguments
    ///
    /// * `field_name` - The struct field name (e.g., `"db_url"`)
    /// * `source` - The [`ValueSource`] containing variable name and origin
    pub fn add(&mut self, field_name: impl Into<String>, source: ValueSource) {
        self.entries.push((field_name.into(), source));
    }

    /// Extends with entries from a nested configuration struct.
    ///
    /// Creates dotted paths for nested fields. For example, if the prefix
    /// is `"database"` and the nested config has a field `"port"`, the
    /// resulting path will be `"database.port"`.
    ///
    /// # Arguments
    ///
    /// * `prefix` - The parent field name
    /// * `nested` - Source entries from the nested config
    pub fn extend_nested(&mut self, prefix: &str, nested: ConfigSources) {
        for (field_name, source) in nested.entries {
            let dotted_path = format!("{}.{}", prefix, field_name);
            self.entries.push((dotted_path, source));
        }
    }

    /// Returns all entries as a slice.
    ///
    /// Each entry is a tuple of `(field_name, ValueSource)`.
    pub fn entries(&self) -> &[(String, ValueSource)] {
        &self.entries
    }

    /// Looks up the source for a specific field by name.
    ///
    /// Returns `None` if the field is not found.
    ///
    /// # Arguments
    ///
    /// * `field_name` - The field name to look up (e.g., `"db_url"` or `"database.port"`)
    pub fn get(&self, field_name: &str) -> Option<&ValueSource> {
        self.entries
            .iter()
            .find(|(name, _)| name == field_name)
            .map(|(_, source)| source)
    }

    /// Returns an iterator over field names and their sources.
    ///
    /// This is useful for iterating through all configuration sources
    /// without consuming the collection.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &ValueSource)> {
        self.entries
            .iter()
            .map(|(name, source)| (name.as_str(), source))
    }
}

impl Display for ConfigSources {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(f, "Configuration Source:")?;
        writeln!(f, "{}", "-".repeat(50))?;

        // Fins max field name length for alignment
        let max_len = self
            .entries
            .iter()
            .map(|(name, _)| name.len())
            .max()
            .unwrap_or(0);

        for (field_name, source) in &self.entries {
            writeln!(
                f,
                "  {:<width$}  <- {} [{}]",
                field_name,
                source.source,
                source.var_name,
                width = max_len,
            )?;
        }

        Ok(())
    }
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_display() {
        assert_eq!(Source::Environment.to_string(), "Environment variable");
        assert_eq!(Source::Default.to_string(), "Default value");
        assert_eq!(Source::NotSet.to_string(), "Not set");
        assert_eq!(Source::DotenvFile(None).to_string(), ".env file");
        assert_eq!(
            Source::DotenvFile(Some(PathBuf::from(".env.local"))).to_string(),
            ".env file (.env.local)"
        );
    }

    #[test]
    fn test_source_equality() {
        assert_eq!(Source::Environment, Source::Environment);
        assert_eq!(Source::Default, Source::Default);
        assert_ne!(Source::Environment, Source::Default);
        assert_eq!(Source::DotenvFile(None), Source::DotenvFile(None));
    }

    #[test]
    fn test_value_source_new() {
        let vs = ValueSource::new("DATABASE_URL".to_string(), Source::Environment);
        assert_eq!(vs.var_name, "DATABASE_URL");
        assert_eq!(vs.source, Source::Environment);
    }

    #[test]
    fn test_value_source_display() {
        let vs = ValueSource::new("PORT".to_string(), Source::Default);
        assert_eq!(vs.to_string(), "PORT: Default value");
    }

    #[test]
    fn test_config_sources_new() {
        let sources = ConfigSources::new();
        assert!(sources.entries().is_empty());
    }

    #[test]
    fn test_config_sources_add_and_get() {
        let mut sources = ConfigSources::new();
        sources.add(
            "db_url",
            ValueSource::new("DATABASE_URL".to_string(), Source::Environment),
        );
        sources.add(
            "port",
            ValueSource::new("PORT".to_string(), Source::Default),
        );

        assert_eq!(sources.entries().len(), 2);

        let db_source = sources.get("db_url").unwrap();
        assert_eq!(db_source.var_name, "DATABASE_URL");
        assert_eq!(db_source.source, Source::Environment);

        let port_source = sources.get("port").unwrap();
        assert_eq!(port_source.var_name, "PORT");
        assert_eq!(port_source.source, Source::Default);

        assert!(sources.get("nonexistent").is_none());
    }

    #[test]
    fn test_config_sources_iter() {
        let mut sources = ConfigSources::new();
        sources.add(
            "field1",
            ValueSource::new("VAR1".to_string(), Source::Environment),
        );
        sources.add(
            "field2",
            ValueSource::new("VAR2".to_string(), Source::Default),
        );

        let entries: Vec<_> = sources.iter().collect();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].0, "field1");
        assert_eq!(entries[1].0, "field2");
    }

    #[test]
    fn test_config_sources_extend_nested() {
        let mut parent = ConfigSources::new();
        parent.add(
            "name",
            ValueSource::new("APP_NAME".to_string(), Source::Environment),
        );

        let mut nested = ConfigSources::new();
        nested.add(
            "host",
            ValueSource::new("DB_HOST".to_string(), Source::DotenvFile(None)),
        );
        nested.add(
            "port",
            ValueSource::new("DB_PORT".to_string(), Source::Default),
        );

        parent.extend_nested("database", nested);

        // Should have 3 entries total
        assert_eq!(parent.entries().len(), 3);
    }

    #[test]
    fn test_config_sources_display() {
        let mut sources = ConfigSources::new();
        sources.add(
            "database_url",
            ValueSource::new("DATABASE_URL".to_string(), Source::Environment),
        );
        sources.add(
            "port".to_string(),
            ValueSource::new("PORT".to_string(), Source::Default),
        );

        let display = sources.to_string();
        assert!(display.contains("Configuration Source"));
        assert!(display.contains("database_url"));
        assert!(display.contains("Environment variable"));
        assert!(display.contains("[DATABASE_URL]"));
        assert!(display.contains("port"));
        assert!(display.contains("Default value"));
    }

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

    #[test]
    fn test_source_custom_provider() {
        let s1 = Source::CustomProvider("vault".to_string());
        let s2 = Source::CustomProvider("vault".to_string());
        assert_eq!(s1, s2);
        assert_eq!(s1.to_string(), "Custom provider (vault)");
    }
}
