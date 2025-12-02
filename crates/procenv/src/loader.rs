//! Configuration loader that orchestrates multiple providers.
//!
//! The [`ConfigLoader`] is the main entry point for loading configuration
//! using the provider-based system. It chains multiple providers together
//! and accumulates all errors rather than failing on the first one.
//!
//! # Example
//!
//! ```rust,ignore
//! use procenv::loader::ConfigLoader;
//! use procenv::provider::{EnvProvider, FileProvider};
//!
//! let loader = ConfigLoader::new()
//!     .with_provider(Box::new(EnvProvider::with_prefix("APP_")))
//!     .with_provider(Box::new(FileProvider::from_file("config.toml")?));
//!
//! // Get individual values
//! let port = loader.get_parsed::<u16>("PORT")?;
//!
//! // Or load a full struct
//! let config: MyConfig = loader.load()?;
//! ```

use std::collections::HashMap;
use std::string::String;

use crate::provider::{Provider, ProviderError, ProviderSource, ProviderValue};
use crate::{ConfigSources, Error, Source, ValueSource};

/// Orchestrates configuration loading from multiple providers.
///
/// Providers are queried in priority order (lower priority number = higher priority).
/// The first provider to return a value for a key wins. Errors from providers
/// are accumulated and reported together.
pub struct ConfigLoader {
    providers: Vec<Box<dyn Provider>>,
    cache: HashMap<String, ProviderValue>,
    sources: ConfigSources,
    errors: Vec<Error>,
    /// Whether providers have been sorted by priority.
    sorted: bool,
}

impl ConfigLoader {
    /// Creates a new empty configuration loader.
    #[must_use]
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
            cache: HashMap::new(),
            sources: ConfigSources::new(),
            errors: Vec::new(),
            sorted: false,
        }
    }

    /// Adds a provider to the loader.
    ///
    /// Providers are automatically sorted by priority when values are retrieved.
    #[must_use]
    pub fn with_provider(mut self, provider: Box<dyn Provider>) -> Self {
        self.providers.push(provider);
        self.sorted = false; // Mark as unsorted when new provider added
        self
    }

    /// Adds an environment provider.
    #[must_use]
    pub fn with_env(self) -> Self {
        self.with_provider(Box::new(crate::provider::EnvProvider::new()))
    }

    /// Adds an environment provider with a prefix.
    #[must_use]
    pub fn with_env_prefix(self, prefix: impl Into<String>) -> Self {
        self.with_provider(Box::new(crate::provider::EnvProvider::with_prefix(prefix)))
    }

    /// Adds a dotenv provider from the default `.env` file.
    ///
    /// # Errors
    ///
    /// Returns an error if the `.env` file cannot be read.
    #[cfg(feature = "dotenv")]
    pub fn with_dotenv(self) -> Result<Self, std::io::Error> {
        let provider = crate::provider::DotenvProvider::new()?;
        Ok(self.with_provider(Box::new(provider)))
    }

    /// Adds a dotenv provider from a specific path.
    ///
    /// # Errors
    ///
    /// Returns an error if the specified file cannot be read.
    #[cfg(feature = "dotenv")]
    pub fn with_dotenv_path(
        self,
        path: impl Into<std::path::PathBuf>,
    ) -> Result<Self, std::io::Error> {
        let provider = crate::provider::DotenvProvider::from_path(path)?;
        Ok(self.with_provider(Box::new(provider)))
    }

    /// Adds a file provider from a required file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file does not exist or cannot be parsed.
    #[cfg(feature = "file")]
    #[allow(clippy::result_large_err)]
    pub fn with_file(
        self,
        path: impl AsRef<std::path::Path>,
    ) -> Result<Self, crate::file::FileError> {
        let provider = crate::provider::FileProvider::from_file(path)?;
        Ok(self.with_provider(Box::new(provider)))
    }

    /// Adds a file provider from an optional file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file exists but cannot be parsed.
    #[cfg(feature = "file")]
    #[allow(clippy::result_large_err)]
    pub fn with_file_optional(
        self,
        path: impl AsRef<std::path::Path>,
    ) -> Result<Self, crate::file::FileError> {
        if let Some(provider) = crate::provider::FileProvider::from_file_optional(path)? {
            Ok(self.with_provider(Box::new(provider)))
        } else {
            Ok(self)
        }
    }

    /// Sorts providers by priority (lower = higher priority).
    fn sort_providers(&mut self) {
        if !self.sorted {
            self.providers.sort_by_key(|p| p.priority());
            self.sorted = true;
        }
    }

    /// Gets a raw value from the provider chain.
    ///
    /// Returns `None` if no provider has the key. Errors are accumulated
    /// internally and can be retrieved with [`errors()`](Self::errors).
    pub fn get(&mut self, key: &str) -> Option<ProviderValue> {
        // Check cache first
        if let Some(cached) = self.cache.get(key) {
            return Some(cached.clone());
        }

        self.sort_providers();

        for provider in &self.providers {
            if !provider.is_available() {
                continue;
            }

            match provider.get(key) {
                Ok(Some(value)) => {
                    // Record source attribution
                    let source = value.source.to_source();
                    self.sources.add(key, ValueSource::new(key, source));

                    // Cache the value
                    self.cache.insert(key.to_string(), value.clone());

                    return Some(value);
                }
                Ok(None) => {
                    // Key not found in this provider, try next
                    if !provider.fallthrough() {
                        break;
                    }
                }
                Err(e) => {
                    // Accumulate error but continue trying other providers
                    self.errors.push(Self::provider_error_to_error(&e));
                }
            }
        }

        None
    }

    /// Gets a required value, recording a Missing error if not found.
    pub fn get_required(&mut self, key: &str, env_var_name: &'static str) -> Option<ProviderValue> {
        if let Some(v) = self.get(key) {
            Some(v)
        } else {
            self.errors.push(Error::missing(env_var_name));
            self.sources
                .add(key, ValueSource::new(env_var_name, Source::NotSet));
            None
        }
    }

    /// Gets a value with a default fallback.
    pub fn get_with_default(
        &mut self,
        key: &str,
        env_var_name: &str,
        default: &str,
    ) -> ProviderValue {
        if let Some(v) = self.get(key) {
            v
        } else {
            self.sources
                .add(key, ValueSource::new(env_var_name, Source::Default));
            ProviderValue {
                value: default.to_string(),
                source: ProviderSource::BuiltIn(Source::Default),
                secret: false,
            }
        }
    }

    /// Gets a value and parses it to the target type.
    ///
    /// # Errors
    ///
    /// Returns an error if the value cannot be parsed to the target type.
    #[allow(clippy::result_large_err)]
    pub fn get_parsed<T: std::str::FromStr>(&mut self, key: &str) -> Result<Option<T>, Error>
    where
        T::Err: std::error::Error + Send + Sync + 'static,
    {
        match self.get(key) {
            Some(pv) => pv.value.parse::<T>().map(Some).map_err(|e| {
                Error::parse(
                    key.to_string(),
                    pv.value.clone(),
                    pv.secret,
                    std::any::type_name::<T>(),
                    Box::new(e),
                )
            }),
            None => Ok(None),
        }
    }

    /// Gets a raw string value from the provider chain.
    pub fn get_str(&mut self, key: &str) -> Option<String> {
        self.get(key).map(|pv| pv.value)
    }

    /// Gets a value with its source attribution.
    pub fn get_with_source(&mut self, key: &str) -> Option<(String, crate::Source)> {
        self.get(key).map(|pv| (pv.value, pv.source.to_source()))
    }

    /// Gets a value as a `ConfigValue` (stored as string).
    pub fn get_value(&mut self, key: &str) -> Option<crate::ConfigValue> {
        self.get(key)
            .map(|pv| crate::ConfigValue::from_str_value(pv.value))
    }

    /// Gets a value as a `ConfigValue` with automatic type inference.
    pub fn get_value_infer(&mut self, key: &str) -> Option<crate::ConfigValue> {
        self.get(key)
            .map(|pv| crate::ConfigValue::from_str_infer(&pv.value))
    }

    /// Gets value, source, and secret flag together.
    pub fn get_full(&mut self, key: &str) -> Option<(String, crate::Source, bool)> {
        self.get(key)
            .map(|pv| (pv.value, pv.source.to_source(), pv.secret))
    }

    /// Lists all cached keys.
    #[must_use]
    pub fn cached_keys(&self) -> Vec<&str> {
        self.cache.keys().map(String::as_str).collect()
    }

    /// Checks if any errors have occurred.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Returns accumulated errors.
    #[must_use]
    pub fn errors(&self) -> &[Error] {
        &self.errors
    }

    /// Takes accumulated errors, leaving the loader empty.
    pub fn take_errors(&mut self) -> Vec<Error> {
        std::mem::take(&mut self.errors)
    }

    /// Returns the source attribution for loaded values.
    #[must_use]
    pub fn sources(&self) -> &ConfigSources {
        &self.sources
    }

    /// Consumes the loader and returns source attribution.
    #[must_use]
    pub fn into_sources(self) -> ConfigSources {
        self.sources
    }

    /// Finalizes loading and returns any accumulated errors.
    ///
    /// Call this after getting all required values to check for errors.
    ///
    /// # Errors
    ///
    /// Returns any accumulated errors from the loading process.
    ///
    #[allow(clippy::result_large_err)]
    pub fn finish(self) -> Result<ConfigSources, Error> {
        match self.errors.len() {
            0 => Ok(self.sources),
            1 => {
                // SAFETY: We just checked len() == 1, so into_iter().next() is guaranteed Some
                let err = self.errors.into_iter().next();
                Err(err.expect("len == 1 guarantees at least one element"))
            }
            _ => Err(Error::Multiple {
                errors: self.errors,
            }),
        }
    }

    /// Converts a `ProviderError` to the main Error type.
    fn provider_error_to_error(e: &ProviderError) -> Error {
        Error::Provider {
            provider: e.provider_name().to_string(),
            message: e.to_string(),
            help: "check provider configuration".to_string(),
        }
    }
}

impl Default for ConfigLoader {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::EnvProvider;

    #[test]
    fn test_loader_creation() {
        let loader = ConfigLoader::new();
        assert!(!loader.has_errors());
        assert!(loader.errors().is_empty());
    }

    #[test]
    fn test_loader_with_env() {
        let loader = ConfigLoader::new().with_env();

        assert_eq!(loader.providers.len(), 1);
    }

    #[test]
    fn test_loader_with_prefix() {
        let loader = ConfigLoader::new().with_env_prefix("APP_");

        assert_eq!(loader.providers.len(), 1);
    }

    #[test]
    fn test_get_with_default() {
        let mut loader = ConfigLoader::new();
        let value = loader.get_with_default("NONEXISTENT", "NONEXISTENT", "default_value");

        assert_eq!(value.value, "default_value");
        assert!(matches!(
            value.source,
            ProviderSource::BuiltIn(Source::Default)
        ));
    }
}
