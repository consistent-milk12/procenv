//! Source attribution for configuration values.
//!
//! This module provides types for tracking where configuration values originated,
//! enabling debugging and auditing of configuration loading.

use std::fmt::{self, Display, Formatter};
use std::path::PathBuf;

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
    fn test_source_custom_provider() {
        let s1 = Source::CustomProvider("vault".to_string());
        let s2 = Source::CustomProvider("vault".to_string());
        assert_eq!(s1, s2);
        assert_eq!(s1.to_string(), "Custom provider (vault)");
    }
}
