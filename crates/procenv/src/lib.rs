#![allow(unused, reason = "False warnings")]

pub use procenv_macro::EnvConfig;

#[cfg(feature = "secrecy")]
pub use secrecy::{ExposeSecret, ExposeSecretMut, SecretBox, SecretString};

// File configuration support (Phase 13)
#[cfg(feature = "file")]
pub mod file;
#[cfg(feature = "file")]
pub use file::{ConfigBuilder, FileFormat, FileUtils, OriginTracker};

use std::fmt::{self, Display, Formatter};
use std::path::PathBuf;
use std::{error::Error as StdError, fmt::Debug};

use miette::{Diagnostic, Severity};

/// Errors that can occur when loading configuration from environment variables.
///
/// Each variant includes a unique error code for easy reference and
/// contextual help messages to guide users toward resolution.
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
        var: &'static str,

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
        var: &'static str,
    },

    /// An environment variable value could not be parsed into the expected type.
    #[diagnostic(code(procenv::parse_error))]
    Parse {
        /// The name of the environment variable.
        var: &'static str,

        /// The raw string value that failed to parse.
        value: String,

        /// Whether this field is marked as secret.
        secret: bool,

        /// The expected type name (for diagnostic messages).
        expected_type: &'static str,

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
    pub fn missing(var: &'static str) -> Self {
        Error::Missing {
            var,
            help: format!("set {} in your environment or .env file", var),
        }
    }

    /// Creates a Parse error with appropriate help text.
    pub fn parse(
        var: &'static str,
        value: String,
        secret: bool,
        expected_type: &'static str,
        source: Box<dyn StdError + Send + Sync>,
    ) -> Self {
        Error::Parse {
            var,
            value,
            secret,
            expected_type,
            help: format!("expected a valid {}", expected_type),
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

/// Where a configuration value originated from.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Source {
    /// Value came from a CLI argument
    Cli,

    /// Value came from an environment variable
    Environment,

    /// Value came from a .env file (with optional path)
    DotenvFile(Option<PathBuf>),

    /// Value came from a configuration file (TOML, JSON, YAML)
    ConfigFile(Option<PathBuf>),

    /// Value came from a profile-specific default (Phase 16)
    Profile(String),

    /// Value used the default specified in the attribute
    Default,

    /// Value was not set (for optional fields)
    NotSet,
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
        }
    }
}

/// Source information for a single configuration value.
#[derive(Clone, Debug)]
pub struct ValueSource {
    /// The environment variable name
    pub var_name: &'static str,

    /// Where the value came from
    pub source: Source,
}

impl ValueSource {
    pub fn new(var_name: &'static str, source: Source) -> Self {
        Self { var_name, source }
    }
}

impl Display for ValueSource {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.var_name, self.source)
    }
}

/// Collection of source attributions for all config fields.
#[derive(Clone, Debug, Default)]
pub struct ConfigSources {
    entries: Vec<(String, ValueSource)>,
}

impl ConfigSources {
    /// Create a new empty ConfigSources.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Add a source entry for a field.
    pub fn add(&mut self, field_name: impl Into<String>, source: ValueSource) {
        self.entries.push((field_name.into(), source));
    }

    /// Extend with entries from a nested config (with field prefix).
    ///
    /// Creates dotted paths for nested fields, e.g., `database.port`.
    pub fn extend_nested(&mut self, prefix: &str, nested: ConfigSources) {
        for (field_name, source) in nested.entries {
            // Create dotted path: "database" + "port" -> "database.port"
            let dotted_path = format!("{}.{}", prefix, field_name);
            self.entries.push((dotted_path, source));
        }
    }

    /// Get all entries as a slice.
    pub fn entries(&self) -> &[(String, ValueSource)] {
        &self.entries
    }

    /// Get the source for a specific field.
    pub fn get(&self, field_name: &str) -> Option<&ValueSource> {
        self.entries
            .iter()
            .find(|(name, _)| name == field_name)
            .map(|(_, source)| source)
    }

    /// Returns an iterator over field names and their sources.
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
        let vs = ValueSource::new("DATABASE_URL", Source::Environment);
        assert_eq!(vs.var_name, "DATABASE_URL");
        assert_eq!(vs.source, Source::Environment);
    }

    #[test]
    fn test_value_source_display() {
        let vs = ValueSource::new("PORT", Source::Default);
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
            ValueSource::new("DATABASE_URL", Source::Environment),
        );
        sources.add("port", ValueSource::new("PORT", Source::Default));

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
        sources.add("field1", ValueSource::new("VAR1", Source::Environment));
        sources.add("field2", ValueSource::new("VAR2", Source::Default));

        let entries: Vec<_> = sources.iter().collect();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].0, "field1");
        assert_eq!(entries[1].0, "field2");
    }

    #[test]
    fn test_config_sources_extend_nested() {
        let mut parent = ConfigSources::new();
        parent.add("name", ValueSource::new("APP_NAME", Source::Environment));

        let mut nested = ConfigSources::new();
        nested.add(
            "host",
            ValueSource::new("DB_HOST", Source::DotenvFile(None)),
        );
        nested.add("port", ValueSource::new("DB_PORT", Source::Default));

        parent.extend_nested("database", nested);

        // Should have 3 entries total
        assert_eq!(parent.entries().len(), 3);
    }

    #[test]
    fn test_config_sources_display() {
        let mut sources = ConfigSources::new();
        sources.add(
            "database_url",
            ValueSource::new("DATABASE_URL", Source::Environment),
        );
        sources.add("port", ValueSource::new("PORT", Source::Default));

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
}
