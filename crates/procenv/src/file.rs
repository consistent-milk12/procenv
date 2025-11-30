//! File-based configuration support.
//!
//! This module provides utilities for loading configuration from files
//! and merging multiple configuration sources with proper layering.
//! It is enabled with the `file` feature flag.
//!
//! # Supported Formats
//!
//! | Format | Feature Flag | Extensions |
//! |--------|--------------|------------|
//! | JSON | `file` (always) | `.json` |
//! | TOML | `toml` | `.toml` |
//! | YAML | `yaml` | `.yaml`, `.yml` |
//!
//! # Layering Priority
//!
//! Configuration sources are merged in this order (lowest to highest priority):
//!
//! 1. **Compiled defaults** - `#[env(default = "...")]`
//! 2. **Config files** - In order specified (later files override earlier)
//! 3. **Dotenv files** - `.env` file values
//! 4. **Environment variables** - Highest priority, always wins
//!
//! # Example Usage
//!
//! ## With Derive Macro
//!
//! ```rust,ignore
//! use procenv::EnvConfig;
//! use serde::Deserialize;
//!
//! #[derive(EnvConfig, Deserialize)]
//! #[env_config(
//!     file_optional = "config.toml",
//!     file_optional = "config.local.toml",
//!     prefix = "APP_"
//! )]
//! struct Config {
//!     #[env(var = "DATABASE_URL")]
//!     database_url: String,
//!
//!     #[env(var = "PORT", default = "8080")]
//!     port: u16,
//! }
//!
//! // Loads: defaults -> config.toml -> config.local.toml -> env vars
//! let config = Config::from_config()?;
//! ```
//!
//! ## With ConfigBuilder (Manual)
//!
//! ```rust,ignore
//! use procenv::file::{ConfigBuilder, FileFormat};
//! use serde::Deserialize;
//!
//! #[derive(Deserialize)]
//! struct DatabaseConfig {
//!     host: String,
//!     port: u16,
//! }
//!
//! let config: DatabaseConfig = ConfigBuilder::new()
//!     .file_optional("config.toml")
//!     .file_optional("config.local.toml")
//!     .env_prefix("DB_")
//!     .build()?;
//! ```
//!
//! # Error Handling
//!
//! Parse errors include source location information when available,
//! enabling rich diagnostic output via [`miette`]:
//!
//! ```text
//! Error: TOML parse error in config.toml
//!    ╭─[config.toml:3:8]
//!    │
//!  3 │ port = "not_a_number"
//!    │        ^^^^^^^^^^^^^^ expected integer, found string
//!    ╰────
//!   help: check for missing quotes, invalid values, or syntax errors
//! ```

// FileError is intentionally large to provide rich miette diagnostics with source spans
#![allow(clippy::result_large_err)]

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use miette::{Diagnostic, NamedSource, SourceSpan};
use serde::{Serialize, de::DeserializeOwned};
use serde_json as SJSON;

#[cfg(feature = "yaml")]
use serde_saphyr as YAML;

#[cfg(feature = "toml")]
use toml as TOML;

use crate::Error;

// =============================================================================
// File Format Detection and Parsing
// =============================================================================

/// Supported configuration file formats.
///
/// The format is automatically detected from the file extension when using
/// [`FileUtils::parse_file`] or [`ConfigBuilder::file`]. You can also
/// explicitly specify a format with [`FileUtils::parse_str`].
///
/// # Feature Flags
///
/// - `file` - Enables JSON support (always included)
/// - `toml` - Enables TOML support
/// - `yaml` - Enables YAML support
///
/// # Example
///
/// ```rust,ignore
/// use procenv::file::FileFormat;
/// use std::path::Path;
///
/// // Automatic detection from path
/// assert_eq!(FileFormat::from_path(Path::new("config.json")), Some(FileFormat::Json));
/// assert_eq!(FileFormat::from_path(Path::new("config.toml")), Some(FileFormat::Toml));
/// assert_eq!(FileFormat::from_path(Path::new("config.yaml")), Some(FileFormat::Yaml));
/// ```
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileFormat {
    /// JSON format (`.json` extension).
    ///
    /// Always available with the `file` feature.
    Json,

    /// TOML format (`.toml` extension).
    ///
    /// Requires the `toml` feature flag.
    #[cfg(feature = "toml")]
    Toml,

    /// YAML format (`.yaml` or `.yml` extension).
    ///
    /// Requires the `yaml` feature flag.
    #[cfg(feature = "yaml")]
    Yaml,
}

impl FileFormat {
    /// Detects the file format from the file extension.
    ///
    /// Returns `None` if the extension is not recognized or if the
    /// required feature flag is not enabled.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use procenv::file::FileFormat;
    /// use std::path::Path;
    ///
    /// let format = FileFormat::from_path(Path::new("config.toml"));
    /// assert_eq!(format, Some(FileFormat::Toml));
    /// ```
    pub fn from_path(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?;

        match ext.to_lowercase().as_str() {
            "json" => Some(FileFormat::Json),

            #[cfg(feature = "toml")]
            "toml" => Some(FileFormat::Toml),

            #[cfg(feature = "yaml")]
            "yaml" | "yml" => Some(FileFormat::Yaml),

            _ => None,
        }
    }

    /// Get the format name for error messages.
    pub fn name(&self) -> &'static str {
        match self {
            FileFormat::Json => "JSON",

            #[cfg(feature = "toml")]
            FileFormat::Toml => "TOML",

            #[cfg(feature = "yaml")]
            FileFormat::Yaml => "YAML",
        }
    }
}

/// Error type for file parsing operations with rich diagnostics.
///
/// This enum represents all file-related errors that can occur during
/// configuration loading. It integrates with [`miette`] to provide
/// beautiful terminal output with source code snippets and precise
/// error locations.
///
/// # Diagnostic Features
///
/// - **Source spans** - Points to the exact location of syntax errors
/// - **Help text** - Provides suggestions for fixing common issues
/// - **Error codes** - Unique codes for each error type
///
/// # Example Output
///
/// ```text
/// Error: TOML parse error in config.toml
///    ╭─[config.toml:5:12]
///    │
///  5 │ port = "8080"
///    │        ^^^^^^ expected integer
///    ╰────
///   help: check for missing quotes, invalid values, or syntax errors
/// ```
#[derive(Debug, Diagnostic, thiserror::Error)]
pub enum FileError {
    /// Configuration file not found
    #[error("configuration file not found: {path}")]
    #[diagnostic(
        code(procenv::file::not_found),
        help("ensure the file exists at the specified path")
    )]
    NotFound {
        /// Path to the missing file
        path: String,
    },

    /// Failed to read file
    #[error("failed to read configuration file: {path}")]
    #[diagnostic(
        code(procenv::file::read_error),
        help("check file permissions and ensure it's readable")
    )]
    ReadError {
        /// Path to the file
        path: String,

        /// The underlying I/O error
        #[source]
        source: std::io::Error,
    },

    /// Unknown file format
    #[error("unknown configuration file format: .{extension}")]
    #[diagnostic(
        code(procenv::file::unknown_format),
        help("supported formats: .json, .toml, .yaml, .yml")
    )]
    UnknownFormat {
        /// The file extension that wasn't recognized
        extension: String,
    },

    /// Parse error with source location
    #[error("{format} parse error in {path}")]
    #[diagnostic(code(procenv::file::parse_error))]
    Parse {
        /// Format name (JSON, TOML, YAML/YML)
        format: &'static str,

        /// Path to the file
        path: String,

        /// The source file content for display
        #[source_code]
        src: NamedSource<String>,

        /// The location of the error
        #[label("{message}")]
        span: SourceSpan,

        /// Description of what went wrong
        message: String,

        /// Suggestion for how to fix
        #[help]
        help: String,
    },

    /// Parse error without source location (fallback)
    #[error("{format} parse error: {message}")]
    #[diagnostic(code(procenv::file::parse_error))]
    ParseNoSpan {
        /// Format name
        format: &'static str,

        // Error message
        message: String,

        /// Suggestion for how to fix
        #[help]
        help: String,
    },

    /// Type mismatch error with source location
    #[error("type mismatch at `{path_str}` in {file_path}")]
    TypeMismatch {
        /// The JSON path where the error occurred (e.g., "database.port")
        path_str: String,

        /// The to the file
        file_path: String,

        /// The source file content for display
        #[source_code]
        src: NamedSource<String>,

        /// The location of the error
        #[label("{message}")]
        span: SourceSpan,

        /// Description of what went wrong
        message: String,

        /// Suggestion for how to fix
        #[help]
        help: String,
    },
}

// =============================================================================
// File Format Detection and Parsing
// =============================================================================

/// Tracks the origin of a value for error reporting.
#[derive(Clone, Debug)]
struct ValueOrigin {
    /// Path to the source file
    file_path: String,

    /// Original file content
    content: String,

    /// File format
    format: FileFormat,
}

/// Tracks the origin of configuration values for precise error reporting.
///
/// This struct maintains a mapping of configuration paths (e.g., `"database.port"`)
/// to their source files. It's used internally by [`ConfigBuilder`] to provide
/// accurate source locations in error messages.
///
/// When a type mismatch or parse error occurs during deserialization,
/// `OriginTracker` enables the error message to point to the exact file
/// and location where the problematic value was defined.
///
/// # Example
///
/// ```rust,ignore
/// let (config, origins) = ConfigBuilder::new()
///     .file("config.toml")
///     .build_with_origins()?;
///
/// // Check where a specific field came from
/// if let Some(path) = origins.get_file_source("database.port") {
///     println!("database.port defined in: {}", path.display());
/// }
/// ```
#[derive(Clone, Debug, Default)]
pub struct OriginTracker {
    /// Maps JSON paths (e.g., `"database.port"`) to their source file.
    origins: HashMap<String, ValueOrigin>,

    /// List of all source files in priority order (last = highest priority).
    sources: Vec<ValueOrigin>,
}

impl OriginTracker {
    fn new() -> Self {
        Self::default()
    }

    /// Record a source file.
    fn add_source(&mut self, file_path: String, content: String, format: FileFormat) {
        self.sources.push(ValueOrigin {
            file_path,
            content,
            format,
        });
    }

    /// Record origins for all paths in a value, attributing to the most recent source.
    fn track_value(&mut self, value: &SJSON::Value, prefix: &str) {
        if self.sources.is_empty() {
            return;
        }

        let source = self.sources.last().unwrap().clone();
        self.track_value_recursive(value, prefix, &source);
    }

    fn track_value_recursive(&mut self, value: &SJSON::Value, prefix: &str, source: &ValueOrigin) {
        match value {
            SJSON::Value::Object(map) => {
                for (key, val) in map {
                    let path = if prefix.is_empty() {
                        key.clone()
                    } else {
                        format!("{prefix}.{key}")
                    };

                    self.origins.insert(path.clone(), source.clone());
                    self.track_value_recursive(val, &path, source);
                }
            }

            SJSON::Value::Array(arr) => {
                for (i, val) in arr.iter().enumerate() {
                    let path = format!("{prefix}[{i}]");

                    self.origins.insert(path.clone(), source.clone());
                    self.track_value_recursive(val, &path, source);
                }
            }

            _ => {
                // Leaf value - already tracked by parent
            }
        }
    }

    /// Find the origin of a value at the given path.
    fn find_origin(&self, path: &str) -> Option<&ValueOrigin> {
        // Try exact match first
        if let Some(origin) = self.origins.get(path) {
            return Some(origin);
        }

        // Try parent paths (for missing fields, the parent object might be tracked)
        let mut current = path.to_string();

        while let Some(dot_pos) = current.rfind('.') {
            current = current[..dot_pos].to_string();

            if let Some(origin) = self.origins.get(&current) {
                return Some(origin);
            }
        }

        // Fall back to most recent source
        self.sources.last()
    }

    /// Check if a field came from a configuration file.
    ///
    /// Returns the file path if the field was explicitly loaded from a file.
    /// Does not return a fallback - only returns Some if the field is actually tracked.
    pub fn get_file_source(&self, field_name: &str) -> Option<PathBuf> {
        // Only return if we have an exact match or parent match in origins
        // Do NOT use the fallback to most recent source
        if let Some(origin) = self.origins.get(field_name) {
            return Some(PathBuf::from(&origin.file_path));
        }

        // Try parent paths (for nested fields)
        let mut current = field_name.to_string();
        while let Some(dot_pos) = current.rfind('.') {
            current = current[..dot_pos].to_string();
            if let Some(origin) = self.origins.get(&current) {
                return Some(PathBuf::from(&origin.file_path));
            }
        }

        None
    }

    /// Check if any files were loaded.
    pub fn has_file_sources(&self) -> bool {
        !self.sources.is_empty()
    }

    /// Get all tracked field paths.
    pub fn tracked_fields(&self) -> impl Iterator<Item = &str> {
        self.origins.keys().map(|s| s.as_str())
    }
}

// ============================================================================
// Configuration Builder
// ============================================================================

/// Builder for layered configuration loading.
///
/// `ConfigBuilder` provides a fluent API for loading configuration from
/// multiple sources with proper layering. Sources are merged in order,
/// with later sources overriding earlier ones.
///
/// # Layering Order
///
/// 1. **Defaults** - Initial values set via [`defaults()`](Self::defaults)
/// 2. **Config files** - Added via [`file()`](Self::file) or [`file_optional()`](Self::file_optional)
/// 3. **Environment variables** - Filtered by [`env_prefix()`](Self::env_prefix)
///
/// # Example
///
/// ```rust,ignore
/// use procenv::file::ConfigBuilder;
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct Config {
///     database_url: String,
///     port: u16,
///     debug: bool,
/// }
///
/// let config: Config = ConfigBuilder::new()
///     .defaults(serde_json::json!({
///         "port": 8080,
///         "debug": false
///     }))
///     .file_optional("config.toml")
///     .file_optional("config.local.toml")
///     .env_prefix("APP_")
///     .build()?;
/// ```
///
/// # Deep Merging
///
/// Nested objects are merged recursively. For example, if `config.toml` has:
///
/// ```toml
/// [database]
/// host = "localhost"
/// port = 5432
/// ```
///
/// And `config.local.toml` has:
///
/// ```toml
/// [database]
/// port = 5433
/// ```
///
/// The result will have `database.host = "localhost"` and `database.port = 5433`.
pub struct ConfigBuilder {
    base: SJSON::Value,
    files: Vec<(PathBuf, bool)>,
    env_prefix: Option<String>,
    env_separator: String,
    origins: OriginTracker,
    /// Direct field-to-env-var mappings for custom var names (field_path, env_var)
    env_mappings: Vec<(String, String)>,
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigBuilder {
    /// Creates a new configuration builder with empty defaults.
    pub fn new() -> Self {
        Self {
            base: SJSON::Value::Object(SJSON::Map::new()),
            files: Vec::new(),
            env_prefix: None,
            env_separator: "_".to_string(),
            origins: OriginTracker::new(),
            env_mappings: Vec::new(),
        }
    }

    /// Sets default values from a serializable struct.
    ///
    /// These defaults are the base layer and will be overridden by
    /// config files and environment variables.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// #[derive(Serialize)]
    /// struct Defaults {
    ///     port: u16,
    ///     debug: bool,
    /// }
    ///
    /// let builder = ConfigBuilder::new()
    ///     .defaults(Defaults { port: 8080, debug: false });
    /// ```
    pub fn defaults<T: Serialize>(mut self, defaults: T) -> Self {
        if let Ok(value) = SJSON::to_value(defaults) {
            self.base = value;
        }

        self
    }

    /// Sets default values from a raw JSON value.
    ///
    /// This is useful when you want to construct defaults dynamically
    /// or when working with `serde_json::json!` macro.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let builder = ConfigBuilder::new()
    ///     .defaults_value(serde_json::json!({
    ///         "port": 8080,
    ///         "debug": false
    ///     }));
    /// ```
    pub fn defaults_value(mut self, value: SJSON::Value) -> Self {
        self.base = value;

        self
    }

    /// Adds a required configuration file.
    ///
    /// If this file does not exist, [`build()`](Self::build) will return
    /// a [`FileError::NotFound`] error.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the configuration file
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let builder = ConfigBuilder::new()
    ///     .file("config.toml");  // Must exist
    /// ```
    pub fn file<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.files.push((path.as_ref().to_path_buf(), true));

        self
    }

    /// Adds an optional configuration file.
    ///
    /// If this file does not exist, it is silently skipped.
    /// This is useful for local override files that may not exist.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the configuration file
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let builder = ConfigBuilder::new()
    ///     .file("config.toml")
    ///     .file_optional("config.local.toml");  // OK if missing
    /// ```
    pub fn file_optional<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.files.push((path.as_ref().to_path_buf(), false));

        self
    }

    /// Sets the environment variable prefix for overlay.
    ///
    /// Only environment variables starting with this prefix will be
    /// considered. The prefix is stripped before matching to config keys.
    ///
    /// # Example
    ///
    /// With prefix `"APP_"`:
    /// - `APP_DATABASE_HOST` → `database.host`
    /// - `APP_PORT` → `port`
    /// - `OTHER_VAR` → ignored
    pub fn env_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.env_prefix = Some(prefix.into());

        self
    }

    /// Set the environment variable separator for nested keys.
    ///
    /// When reading environment variables for nested configuration, this separator
    /// is used to split the variable name into nested keys. Default is "_".
    ///
    /// # Example
    ///
    /// With separator "_" and prefix "APP_":
    /// - `APP_DATABASE_HOST` becomes `database.host`
    pub fn env_separator(mut self, separator: impl Into<String>) -> Self {
        self.env_separator = separator.into();

        self
    }

    /// Register a direct mapping from a field path to an environment variable.
    ///
    /// This allows overriding specific fields with custom environment variables
    /// that don't follow the prefix/separator convention. These mappings have
    /// the highest priority and are applied after all other env var processing.
    ///
    /// # Arguments
    ///
    /// * `field_path` - The JSON path to the field (e.g., "database_url" or "database.port")
    /// * `env_var` - The environment variable name to read
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let config: MyConfig = ConfigBuilder::new()
    ///     .file("config.toml")
    ///     .env_prefix("APP_")
    ///     .env_mapping("database_url", "DATABASE_URL")  // Custom var name
    ///     .env_mapping("api_key", "SECRET_API_KEY")     // Different naming
    ///     .build()?;
    /// ```
    pub fn env_mapping(
        mut self,
        field_path: impl Into<String>,
        env_var: impl Into<String>,
    ) -> Self {
        self.env_mappings.push((field_path.into(), env_var.into()));
        self
    }

    /// Merges all configuration sources and returns the raw JSON value.
    ///
    /// This is a lower-level method that returns the merged JSON value
    /// along with origin tracking information. Use [`build()`](Self::build)
    /// for type-safe deserialization.
    ///
    /// # Returns
    ///
    /// A tuple of:
    /// - The merged JSON value
    /// - An [`OriginTracker`] with source information for each path
    ///
    /// # Errors
    ///
    /// Returns a [`FileError`] if a required file is missing or cannot be parsed.
    pub fn merge(mut self) -> Result<(SJSON::Value, OriginTracker), FileError> {
        // Layer files
        for (path, required) in self.files.clone() {
            if let Some((file_value, content, format)) =
                FileUtils::parse_file_with_content(&path, required)?
            {
                // Track origins before merging
                self.origins
                    .add_source(path.display().to_string(), content, format);
                self.origins.track_value(&file_value, "");

                FileUtils::deep_merge(&mut self.base, file_value);
            }
        }

        // Layer environment variables using prefix/separator convention
        if let Some(prefix) = &self.env_prefix {
            let env_value = FileUtils::env_to_value(prefix, &self.env_separator);

            if let SJSON::Value::Object(map) = &env_value
                && !map.is_empty()
            {
                FileUtils::deep_merge(&mut self.base, env_value);
            }
        }

        // Layer direct env mappings (highest priority for env overrides)
        // These handle custom var names and no_prefix fields
        for (field_path, env_var) in &self.env_mappings {
            if let Ok(value) = std::env::var(env_var) {
                let typed_value = FileUtils::coerce_value(&value);
                let parts: Vec<&str> = field_path.split('.').collect();

                if let SJSON::Value::Object(ref mut map) = self.base {
                    FileUtils::insert_nested(map, &parts, typed_value);
                }
            }
        }

        Ok((self.base, self.origins))
    }

    /// Builds the configuration by merging sources and deserializing.
    ///
    /// This is the primary method for loading configuration. It:
    /// 1. Merges defaults, config files, and environment variables
    /// 2. Deserializes the result into the target type
    ///
    /// # Type Parameters
    ///
    /// * `T` - The configuration struct type (must implement `DeserializeOwned`)
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - A required file is missing
    /// - A file has invalid syntax
    /// - The merged config cannot be deserialized to `T`
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use procenv::file::ConfigBuilder;
    ///
    /// let config: MyConfig = ConfigBuilder::new()
    ///     .file("config.toml")
    ///     .env_prefix("APP_")
    ///     .build()?;
    /// ```
    pub fn build<T: DeserializeOwned>(self) -> Result<T, Error> {
        let (result, _origins) = self.build_with_origins()?;
        Ok(result)
    }

    /// Build the configuration and return origin tracking information.
    ///
    /// This method is useful when you need to know where each configuration
    /// value came from (which file, environment variable, or default).
    ///
    /// # Returns
    ///
    /// A tuple of:
    /// - The deserialized configuration struct
    /// - An `OriginTracker` that records which file each value came from
    pub fn build_with_origins<T: DeserializeOwned>(self) -> Result<(T, OriginTracker), Error> {
        use serde::de::IntoDeserializer;

        let (merged, origins) = self.merge()?;

        // Use serde_path_to_error to get exact path on failure
        let deserialier: SJSON::Value = merged.into_deserializer();

        let result = serde_path_to_error::deserialize(deserialier).map_err(|e| {
            let path = e.path().to_string();
            let inner_msg = e.inner().to_string();

            // Try to find the origin and create a span error
            if let Some(origin) = origins.find_origin(&path)
                && let Some(file_error) = FileUtils::type_mismatch_error(&path, &inner_msg, origin)
            {
                return Error::from(file_error);
            }

            // Fallback to no span
            Error::from(FileError::ParseNoSpan {
                format: "JSON",
                message: format!("at `{}`: {}", path, inner_msg),
                help: "check that the config file values match the expected types".to_string(),
            })
        })?;

        Ok((result, origins))
    }
}

/// Utilities for file parsing and value manipulation.
///
/// This struct provides static methods for:
/// - Parsing configuration files in various formats
/// - Converting between formats
/// - Deep merging JSON values
/// - Coercing string values to appropriate JSON types
///
/// Most users will interact with these utilities indirectly through
/// [`ConfigBuilder`] or the derive macro. Direct use is available
/// for advanced use cases.
pub struct FileUtils;

impl FileUtils {
    /// Converts a byte offset to a [`SourceSpan`] with a reasonable length.
    ///
    /// Used internally for error reporting to highlight the relevant
    /// portion of a configuration file.
    pub(crate) fn offset_to_span(offset: usize, content: &str) -> SourceSpan {
        let remaining = &content[offset.min(content.len())..];
        let len = remaining
            .find(|c: char| c.is_whitespace() || (c == ',') || (c == '}') || (c == ']'))
            .unwrap_or(remaining.len().min(20))
            .max(1);

        SourceSpan::new(offset.into(), len)
    }

    /// Convert line/column (1-indexed) to byte offset.
    pub(crate) fn line_col_to_offset(content: &str, line: usize, col: usize) -> usize {
        let mut offset = 0;

        for (i, l) in content.lines().enumerate() {
            if (i + 1) == line {
                return offset + col.saturating_sub(1);
            }

            offset += l.len() + 1;
        }

        offset
    }

    /// Search for a field using multiple patterns and return the value offset.
    ///
    /// This is a helper function that extracts the common pattern matching
    /// logic used across different file formats.
    ///
    /// # Arguments
    /// - `content` - The content to search in
    /// - `base_offset` - Offset adjustment for nested content
    /// - `patterns` - Patterns to search for (e.g., `["field:", "field :"]`)
    /// - `value_finder` - Predicate to find where the value starts after the delimiter
    fn find_value_with_patterns<F>(
        content: &str,
        base_offset: usize,
        patterns: &[String],
        value_finder: F,
    ) -> Option<usize>
    where
        F: Fn(char) -> bool,
    {
        for pattern in patterns {
            if let Some(pos) = content.find(pattern.as_str()) {
                let after_delimiter = pos + pattern.len();
                let remaining = &content[after_delimiter..];

                if let Some(value_start) = remaining.find(&value_finder) {
                    return Some(base_offset + after_delimiter + value_start);
                }
            }
        }
        None
    }

    /// Find a field's value location in file content.
    ///
    /// Handles nested paths like "database.port" by finding the parent section first.
    pub(crate) fn find_field_offset(
        content: &str,
        field: &str,
        format: FileFormat,
    ) -> Option<usize> {
        // Handle nested paths like "database.port"
        let parts: Vec<&str> = field.split('.').collect();
        let leaf_field = parts.last()?;

        // Determine search region based on parent path
        let search_content = if parts.len() > 1 {
            let parent_parts = &parts[..parts.len() - 1];
            Self::find_section_content(content, parent_parts, format)?
        } else {
            content
        };

        // Calculate offset adjustment (search_content might be a slice of content)
        let base_offset = content.len() - search_content.len();

        match format {
            FileFormat::Json => {
                let patterns = vec![
                    format!("\"{leaf_field}\":"),
                    format!("\"{leaf_field}\" :"),
                ];
                Self::find_value_with_patterns(
                    search_content,
                    base_offset,
                    &patterns,
                    |c: char| !c.is_whitespace(),
                )
            }

            #[cfg(feature = "toml")]
            FileFormat::Toml => {
                let patterns = vec![
                    format!("{leaf_field} ="),
                    format!("{leaf_field}="),
                ];
                Self::find_value_with_patterns(
                    search_content,
                    base_offset,
                    &patterns,
                    |c: char| !c.is_whitespace(),
                )
            }

            #[cfg(feature = "yaml")]
            FileFormat::Yaml => {
                let patterns = vec![
                    format!("{leaf_field}:"),
                    format!("{leaf_field} :"),
                ];
                Self::find_value_with_patterns(
                    search_content,
                    base_offset,
                    &patterns,
                    |c: char| !c.is_whitespace() && c != '\n',
                )
            }
        }
    }

    /// Find the content region for a nested section.
    ///
    /// For TOML, finds `[section]` or `[parent.section]` and returns content until next section.
    /// For JSON/YAML, finds the parent object and returns its content.
    fn find_section_content<'a>(
        content: &'a str,
        parent_parts: &[&str],
        format: FileFormat,
    ) -> Option<&'a str> {
        match format {
            #[cfg(feature = "toml")]
            FileFormat::Toml => {
                // Look for [section] or [parent.child] header
                let section_name = parent_parts.join(".");
                let header = format!("[{}]", section_name);

                if let Some(section_start) = content.find(&header) {
                    let after_header = section_start + header.len();
                    // Find the end of this section (next [...] or end of file)
                    let section_content = &content[after_header..];
                    let section_end = section_content
                        .find("\n[")
                        .map(|pos| after_header + pos)
                        .unwrap_or(content.len());
                    return Some(&content[after_header..section_end]);
                }
                None
            }

            FileFormat::Json => {
                // For JSON, find the parent object by searching for "parent": {
                let parent_key = parent_parts.last()?;
                let pattern = format!("\"{}\"", parent_key);

                if let Some(key_pos) = content.find(&pattern) {
                    // Find the opening brace after the key
                    let after_key = &content[key_pos + pattern.len()..];
                    if let Some(brace_pos) = after_key.find('{') {
                        let obj_start = key_pos + pattern.len() + brace_pos;
                        return Some(&content[obj_start..]);
                    }
                }
                None
            }

            #[cfg(feature = "yaml")]
            FileFormat::Yaml => {
                // For YAML, find parent key and return indented content after it
                let parent_key = parent_parts.last()?;
                let pattern = format!("{}:", parent_key);

                if let Some(key_pos) = content.find(&pattern) {
                    return Some(&content[key_pos..]);
                }
                None
            }
        }
    }

    pub(crate) fn json_parse_error(e: SJSON::Error, content: &str, path: &Path) -> FileError {
        let line = e.line();
        let col = e.column();
        let offset = Self::line_col_to_offset(content, line, col);

        FileError::Parse {
            format: "JSON",
            path: path.display().to_string(),
            src: NamedSource::new(path.display().to_string(), content.to_string()),
            span: Self::offset_to_span(offset, content),
            message: e.to_string(),
            help: "check for missing commands, quotes, or brackets".to_string(),
        }
    }

    #[cfg(feature = "toml")]
    pub(crate) fn toml_parse_error(e: TOML::de::Error, content: &str, path: &Path) -> FileError {
        if let Some(span) = e.span() {
            FileError::Parse {
                format: "TOML",
                path: path.display().to_string(),
                src: NamedSource::new(path.display().to_string(), content.to_string()),
                span: SourceSpan::new(span.start.into(), span.end - span.start),
                message: e.message().to_string(),
                help: "check for missing quotes, invalid values, or syntax errors".to_string(),
            }
        } else {
            FileError::ParseNoSpan {
                format: "TOML",
                message: e.to_string(),
                help: "check for missing quotes, invalid values, or syntax errors".to_string(),
            }
        }
    }

    #[cfg(feature = "yaml")]
    pub(crate) fn yaml_parse_error(e: YAML::Error, content: &str, path: &Path) -> FileError {
        let msg = e.to_string();

        if let Some(loc) = Self::extract_yaml_location(&msg) {
            let offset = Self::line_col_to_offset(content, loc.0, loc.1);

            FileError::Parse {
                format: "YAML",
                path: path.display().to_string(),
                src: NamedSource::new(path.display().to_string(), content.to_string()),
                span: Self::offset_to_span(offset, content),
                message: msg.clone(),
                help: "check indentation and ensure proper YAML syntax".to_string(),
            }
        } else {
            FileError::ParseNoSpan {
                format: "YAML",
                message: msg,
                help: "check indentation and ensure proper YAML syntax".to_string(),
            }
        }
    }

    /// Try to extract line/column from YAML error message.
    #[cfg(feature = "yaml")]
    pub(crate) fn extract_yaml_location(msg: &str) -> Option<(usize, usize)> {
        let line_idx = msg.find("line ")?;
        let after_line = &msg[(line_idx + 5)..];
        let line_end = after_line.find(|c: char| !c.is_ascii_digit())?;
        let line = after_line[..line_end].parse::<usize>().ok()?;

        let col_idx = after_line.find("column ")?;
        let after_col = &after_line[(col_idx) + 7..];
        let col_end = after_col
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(after_col.len());
        let col = after_col[..col_end].parse::<usize>().ok()?;

        Some((line, col))
    }

    /// Create a type mismatch error with source location.
    fn type_mismatch_error(path: &str, message: &str, origin: &ValueOrigin) -> Option<FileError> {
        let offset = Self::find_field_offset(&origin.content, path, origin.format)?;

        Some(FileError::TypeMismatch {
            path_str: path.into(),
            file_path: origin.file_path.clone(),
            src: NamedSource::new(origin.file_path.clone(), origin.content.clone()),
            span: Self::offset_to_span(offset, &origin.content),
            message: message.to_string(),
            help: "check that the value matches the expected type".to_string(),
        })
    }

    // ============================================================================
    // File Parsing
    // ============================================================================

    /// Parses a configuration file into a JSON value.
    ///
    /// The file format is automatically detected from the extension.
    /// Supports `.json`, `.toml` (with feature), and `.yaml`/`.yml` (with feature).
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the configuration file
    /// * `required` - If `true`, returns an error when the file doesn't exist
    ///
    /// # Returns
    ///
    /// - `Ok(Some(value))` - File was parsed successfully
    /// - `Ok(None)` - File doesn't exist and `required` is `false`
    /// - `Err(...)` - File doesn't exist (when required) or parse error
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use procenv::file::FileUtils;
    /// use std::path::Path;
    ///
    /// // Required file (errors if missing)
    /// let value = FileUtils::parse_file(Path::new("config.toml"), true)?;
    ///
    /// // Optional file (returns None if missing)
    /// let value = FileUtils::parse_file(Path::new("config.local.toml"), false)?;
    /// ```
    pub fn parse_file(path: &Path, required: bool) -> Result<Option<SJSON::Value>, FileError> {
        Self::parse_file_with_content(path, required).map(|opt| opt.map(|(v, _, _)| v))
    }

    /// Parse a configuration file and return content for error reporting.
    fn parse_file_with_content(
        path: &Path,
        required: bool,
    ) -> Result<Option<(SJSON::Value, String, FileFormat)>, FileError> {
        let path_str = path.display().to_string();

        if !path.exists() {
            if required {
                return Err(FileError::NotFound { path: path_str });
            }
            return Ok(None);
        }

        let content = std::fs::read_to_string(path).map_err(|e| FileError::ReadError {
            path: path_str.clone(),
            source: e,
        })?;

        let format = FileFormat::from_path(path).ok_or_else(|| FileError::UnknownFormat {
            extension: path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("unknown")
                .to_string(),
        })?;

        let value = match format {
            FileFormat::Json => serde_json::from_str(&content)
                .map_err(|e| Self::json_parse_error(e, &content, path))?,

            #[cfg(feature = "toml")]
            FileFormat::Toml => {
                let toml_value: toml::Value = toml::from_str(&content)
                    .map_err(|e| Self::toml_parse_error(e, &content, path))?;
                Self::toml_to_json(toml_value)
            }

            #[cfg(feature = "yaml")]
            FileFormat::Yaml => serde_saphyr::from_str(&content)
                .map_err(|e| Self::yaml_parse_error(e, &content, path))?,
        };

        Ok(Some((value, content, format)))
    }

    /// Parses a configuration string with an explicit format.
    ///
    /// Unlike [`parse_file`](Self::parse_file), this method requires you to
    /// specify the format explicitly since there's no filename to detect from.
    ///
    /// # Arguments
    ///
    /// * `content` - The configuration content as a string
    /// * `format` - The format to parse as
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use procenv::file::{FileUtils, FileFormat};
    ///
    /// let toml_content = r#"
    /// [database]
    /// host = "localhost"
    /// port = 5432
    /// "#;
    ///
    /// let value = FileUtils::parse_str(toml_content, FileFormat::Toml)?;
    /// ```
    pub fn parse_str(content: &str, format: FileFormat) -> Result<SJSON::Value, FileError> {
        let dummy_path = Path::new("<string>");
        match format {
            FileFormat::Json => serde_json::from_str(content)
                .map_err(|e| Self::json_parse_error(e, content, dummy_path)),

            #[cfg(feature = "toml")]
            FileFormat::Toml => {
                let toml_value: toml::Value = toml::from_str(content)
                    .map_err(|e| Self::toml_parse_error(e, content, dummy_path))?;
                Ok(Self::toml_to_json(toml_value))
            }

            #[cfg(feature = "yaml")]
            FileFormat::Yaml => serde_saphyr::from_str(content)
                .map_err(|e| Self::yaml_parse_error(e, content, dummy_path)),
        }
    }

    // ============================================================================
    // Format Conversion
    // ============================================================================

    /// Convert a TOML Value to a JSON Value.
    #[cfg(feature = "toml")]
    fn toml_to_json(toml: TOML::Value) -> SJSON::Value {
        match toml {
            TOML::Value::String(s) => SJSON::Value::String(s),

            TOML::Value::Integer(i) => SJSON::Value::Number(i.into()),

            TOML::Value::Float(f) => {
                SJSON::Value::Number(SJSON::Number::from_f64(f).unwrap_or_else(|| 0.into()))
            }

            TOML::Value::Boolean(b) => SJSON::Value::Bool(b),

            TOML::Value::Datetime(dt) => SJSON::Value::String(dt.to_string()),

            TOML::Value::Array(arr) => {
                SJSON::Value::Array(arr.into_iter().map(Self::toml_to_json).collect())
            }

            TOML::Value::Table(table) => {
                let map: SJSON::Map<String, SJSON::Value> = table
                    .into_iter()
                    .map(|(k, v)| (k, Self::toml_to_json(v)))
                    .collect();
                SJSON::Value::Object(map)
            }
        }
    }

    // ============================================================================
    // Value Merging
    // ============================================================================

    /// Deep merges two JSON values, with overlay taking precedence.
    ///
    /// For objects, keys from `overlay` are recursively merged into `base`.
    /// For all other types (arrays, primitives), `overlay` replaces `base`.
    ///
    /// # Arguments
    ///
    /// * `base` - The base value (modified in place)
    /// * `overlay` - The overlay value (takes precedence)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use procenv::file::FileUtils;
    /// use serde_json::json;
    ///
    /// let mut base = json!({
    ///     "database": { "host": "localhost", "port": 5432 }
    /// });
    /// let overlay = json!({
    ///     "database": { "port": 5433 }
    /// });
    ///
    /// FileUtils::deep_merge(&mut base, overlay);
    ///
    /// // Result: { "database": { "host": "localhost", "port": 5433 } }
    /// ```
    pub fn deep_merge(base: &mut SJSON::Value, overlay: SJSON::Value) {
        match (base, overlay) {
            (SJSON::Value::Object(base_map), SJSON::Value::Object(overlay_map)) => {
                for (key, overlay_value) in overlay_map {
                    if let Some(base_value) = base_map.get_mut(&key) {
                        Self::deep_merge(base_value, overlay_value);
                    } else {
                        base_map.insert(key, overlay_value);
                    }
                }
            }
            (base, overlay) => {
                *base = overlay;
            }
        }
    }

    // ============================================================================
    // Environment Variable Conversion
    // ============================================================================

    /// Coerces a string value to an appropriate JSON type.
    ///
    /// Attempts to parse the string as (in order):
    /// 1. Boolean (`true`/`false`, case-insensitive)
    /// 2. Integer (`i64`)
    /// 3. Float (`f64`, only if contains `.`)
    /// 4. String (fallback)
    ///
    /// This is used when converting environment variables to JSON for
    /// merging with config file values.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use procenv::file::FileUtils;
    /// use serde_json::Value;
    ///
    /// assert_eq!(FileUtils::coerce_value("true"), Value::Bool(true));
    /// assert_eq!(FileUtils::coerce_value("42"), Value::Number(42.into()));
    /// assert_eq!(FileUtils::coerce_value("3.14"), Value::Number(/* 3.14 */));
    /// assert_eq!(FileUtils::coerce_value("hello"), Value::String("hello".into()));
    /// ```
    pub fn coerce_value(s: &str) -> SJSON::Value {
        if s.eq_ignore_ascii_case("true") {
            return SJSON::Value::Bool(true);
        }
        if s.eq_ignore_ascii_case("false") {
            return SJSON::Value::Bool(false);
        }

        if let Ok(i) = s.parse::<i64>() {
            return SJSON::Value::Number(i.into());
        }

        if s.contains('.')
            && let Ok(f) = s.parse::<f64>()
            && let Some(n) = SJSON::Number::from_f64(f)
        {
            return SJSON::Value::Number(n);
        }

        SJSON::Value::String(s.to_string())
    }

    /// Convert environment variables to a nested JSON Value.
    pub fn env_to_value(prefix: &str, separator: &str) -> SJSON::Value {
        let mut root = serde_json::Map::new();

        for (key, value) in std::env::vars() {
            if let Some(stripped) = key.strip_prefix(prefix) {
                let lowered = stripped.to_lowercase();
                let parts: Vec<&str> = lowered.split(separator).collect();
                let typed_value = Self::coerce_value(&value);
                Self::insert_nested(&mut root, &parts, typed_value);
            }
        }

        SJSON::Value::Object(root)
    }

    /// Insert a value into a nested map structure.
    ///
    /// Used for building nested JSON objects from flat key paths like "database.host".
    pub fn insert_nested(
        map: &mut SJSON::Map<String, SJSON::Value>,
        parts: &[&str],
        value: SJSON::Value,
    ) {
        if parts.is_empty() {
            return;
        }

        if parts.len() == 1 {
            map.insert(parts[0].to_string(), value);
        } else {
            let entry = map
                .entry(parts[0].to_string())
                .or_insert_with(|| SJSON::Value::Object(SJSON::Map::new()));

            if let SJSON::Value::Object(nested) = entry {
                Self::insert_nested(nested, &parts[1..], value);
            }
        }
    }
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coerce_value_bool() {
        assert_eq!(FileUtils::coerce_value("true"), SJSON::Value::Bool(true));
        assert_eq!(FileUtils::coerce_value("TRUE"), SJSON::Value::Bool(true));
        assert_eq!(FileUtils::coerce_value("false"), SJSON::Value::Bool(false));
        assert_eq!(FileUtils::coerce_value("FALSE"), SJSON::Value::Bool(false));
    }

    #[test]
    fn test_coerce_value_integer() {
        assert_eq!(
            FileUtils::coerce_value("42"),
            SJSON::Value::Number(42.into())
        );
        assert_eq!(
            FileUtils::coerce_value("-100"),
            SJSON::Value::Number((-100).into())
        );
        assert_eq!(FileUtils::coerce_value("0"), SJSON::Value::Number(0.into()));
    }

    #[test]
    fn test_coerce_value_float() {
        let val = FileUtils::coerce_value("3.14");
        if let SJSON::Value::Number(n) = val {
            assert!((n.as_f64().unwrap() - 3.14).abs() < 0.001);
        } else {
            panic!("Expected number");
        }
    }

    #[test]
    fn test_coerce_value_string() {
        assert_eq!(
            FileUtils::coerce_value("hello"),
            SJSON::Value::String("hello".to_string())
        );
        assert_eq!(
            FileUtils::coerce_value("hello world"),
            SJSON::Value::String("hello world".to_string())
        );
    }

    #[test]
    fn test_deep_merge_objects() {
        let mut base = SJSON::json!({
            "a": 1,
            "b": {"x": 10, "y": 20}
        });
        let overlay = SJSON::json!({
            "b": {"y": 200, "z": 30},
            "c": 3
        });

        FileUtils::deep_merge(&mut base, overlay);

        assert_eq!(base["a"], 1);
        assert_eq!(base["b"]["x"], 10);
        assert_eq!(base["b"]["y"], 200);
        assert_eq!(base["b"]["z"], 30);
        assert_eq!(base["c"], 3);
    }

    #[test]
    fn test_deep_merge_replace() {
        let mut base = SJSON::json!({"a": [1, 2, 3]});
        let overlay = SJSON::json!({"a": [4, 5]});

        FileUtils::deep_merge(&mut base, overlay);

        assert_eq!(base["a"], SJSON::json!([4, 5]));
    }

    #[test]
    fn test_insert_nested() {
        let mut map = SJSON::Map::new();
        FileUtils::insert_nested(
            &mut map,
            &["database", "host"],
            SJSON::Value::String("localhost".into()),
        );

        assert_eq!(
            map.get("database")
                .and_then(|v| v.get("host"))
                .and_then(|v| v.as_str()),
            Some("localhost")
        );
    }

    #[test]
    fn test_file_format_detection() {
        assert_eq!(
            FileFormat::from_path(Path::new("config.json")),
            Some(FileFormat::Json)
        );

        #[cfg(feature = "toml")]
        assert_eq!(
            FileFormat::from_path(Path::new("config.toml")),
            Some(FileFormat::Toml)
        );

        #[cfg(feature = "yaml")]
        {
            assert_eq!(
                FileFormat::from_path(Path::new("config.yaml")),
                Some(FileFormat::Yaml)
            );
            assert_eq!(
                FileFormat::from_path(Path::new("config.yml")),
                Some(FileFormat::Yaml)
            );
        }

        assert_eq!(FileFormat::from_path(Path::new("config.txt")), None);
    }

    #[test]
    fn test_find_field_offset_json() {
        let content = r#"{"name": "test", "port": "bad"}"#;
        let offset = FileUtils::find_field_offset(content, "port", FileFormat::Json);
        assert!(offset.is_some());
        let off = offset.unwrap();
        assert_eq!(&content[off..off + 5], "\"bad\"");
    }

    #[cfg(feature = "toml")]
    #[test]
    fn test_find_field_offset_toml() {
        let content = "name = \"test\"\nport = \"bad\"";
        let offset = FileUtils::find_field_offset(content, "port", FileFormat::Toml);
        assert!(offset.is_some());
        let off = offset.unwrap();
        assert_eq!(&content[off..off + 5], "\"bad\"");
    }
}
