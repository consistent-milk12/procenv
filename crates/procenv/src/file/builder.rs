//! Configuration builder for layered loading.

use std::path::{Path, PathBuf};

use serde::{Serialize, de::DeserializeOwned};
use serde_json as SJSON;

use crate::Error;

use super::error::FileError;
use super::origin::OriginTracker;
use super::utils::FileUtils;

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
    /// Direct field-to-env-var mappings for custom var names (`field_path`, `env_var`)
    env_mappings: Vec<(String, String)>,
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigBuilder {
    /// Creates a new configuration builder with empty defaults.
    #[must_use]
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
    /// # Panics
    ///
    /// Panics if the defaults cannot be serialized to JSON. This can happen if:
    /// - The type contains non-serializable fields (e.g., function pointers)
    /// - Custom `Serialize` implementation returns an error
    ///
    /// Use [`try_defaults()`](Self::try_defaults) for fallible serialization.
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
    #[must_use]
    pub fn defaults<T: Serialize>(self, defaults: T) -> Self {
        self.try_defaults(defaults).unwrap_or_else(|e| {
            panic!(
                "ConfigBuilder::defaults() serialization failed: {e}. \
                 Use try_defaults() for fallible handling."
            )
        })
    }

    /// Sets default values from a serializable struct (fallible version).
    ///
    /// Unlike [`defaults()`](Self::defaults), this method returns a `Result`
    /// instead of panicking on serialization failure.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the defaults cannot be serialized to JSON.
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
    ///     .try_defaults(Defaults { port: 8080, debug: false })?;
    /// ```
    pub fn try_defaults<T: Serialize>(
        mut self,
        defaults: T,
    ) -> Result<Self, DefaultsSerializationError> {
        self.base = SJSON::to_value(&defaults).map_err(|e| DefaultsSerializationError {
            type_name: std::any::type_name::<T>().to_string(),
            inner: e.to_string(),
        })?;
        Ok(self)
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    /// * `field_path` - The JSON path to the field (e.g., "`database_url`" or "database.port")
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
    #[must_use]
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

    /// Merges all configuration sources and returns the raw JSON value.
    ///
    /// Unlike [`build()`](Self::build), this method does NOT deserialize
    /// the result. Use this when you want to extract values manually
    /// (e.g., for macro-generated field-by-field extraction).
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
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let (value, origins) = ConfigBuilder::new()
    ///     .file("config.toml")
    ///     .env_prefix("APP_")
    ///     .into_value()?;
    ///
    /// // Now extract fields manually
    /// let port = value.get("port").and_then(|v| v.as_u64());
    /// ```
    pub fn into_value(self) -> Result<(SJSON::Value, OriginTracker), FileError> {
        self.merge()
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
    ///
    /// # Errors
    ///
    /// Returns an error if a required file is missing, a file has invalid syntax,
    /// or the merged configuration cannot be deserialized to `T`.
    pub fn build_with_origins<T: DeserializeOwned>(self) -> Result<(T, OriginTracker), Error> {
        use serde::de::IntoDeserializer;

        let (merged, origins) = self.merge()?;

        // Use serde_path_to_error to get exact path on failure
        let deserializer = merged.into_deserializer();

        let result = serde_path_to_error::deserialize(deserializer).map_err(|e| {
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
                message: format!("at `{path}`: {inner_msg}"),
                help: "check that the config file values match the expected types".to_string(),
            })
        })?;

        Ok((result, origins))
    }
}

/// Error returned when [`ConfigBuilder::try_defaults()`] fails to serialize.
///
/// This error occurs when the provided defaults struct cannot be converted
/// to a JSON value. Common causes include:
///
/// - Non-serializable field types (e.g., function pointers, raw pointers)
/// - Custom `Serialize` implementations that return errors
/// - Recursive or cyclic data structures without proper handling
#[derive(Debug, Clone)]
pub struct DefaultsSerializationError {
    /// The type name that failed to serialize (from `std::any::type_name`).
    pub type_name: String,
    /// The underlying serialization error message.
    pub inner: String,
}

impl std::fmt::Display for DefaultsSerializationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "failed to serialize defaults of type '{}': {}",
            self.type_name, self.inner
        )
    }
}

impl std::error::Error for DefaultsSerializationError {}
