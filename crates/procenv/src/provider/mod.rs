//! Provider abstraction for extensible configuration sources.
//!
//! This module defines the [`Provider`] trait that allows custom configuration
//! sources like Vault, AWS Secrets Manager, or Consul to be integrated
//! with procenv's configuration loading system.
//!
//! # Built-in Providers
//!
//! - [`EnvProvider`] - Loads from environment variables
//! - [`DotenvProvider`] - Loads from `.env` files
//! - [`FileProvider`] - Loads from config files (TOML/JSON/YAML)
//!
//! # Custom Providers
//!
//! Implement the [`Provider`] trait to create custom sources:
//!
//! ```rust,ignore
//! use procenv::provider::{Provider, ProviderResult, ProviderValue, ProviderSource};
//!
//! struct VaultProvider { /* ... */ }
//!
//! impl Provider for VaultProvider {
//!     fn name(&self) -> &str { "vault" }
//!
//!     fn get(&self, key: &str) -> ProviderResult<ProviderValue> {
//!         // Fetch from Vault...
//!         Ok(Some(ProviderValue {
//!             value: "secret-value".to_string(),
//!             source: ProviderSource::custom("vault", Some(format!("secret/app/{}", key))),
//!             secret: true,
//!         }))
//!     }
//! }
//! ```

#[cfg(feature = "async")]
mod adapter;
#[cfg(feature = "dotenv")]
mod dotenv;
mod env;
#[cfg(feature = "file")]
mod file;

#[cfg(feature = "dotenv")]
pub use self::dotenv::DotenvProvider;
#[cfg(feature = "async")]
pub use adapter::BlockingAdapter;
pub use env::EnvProvider;
#[cfg(feature = "file")]
pub use file::FileProvider;

use std::collections::HashMap;
use std::error::Error as StdError;
use std::fmt::{self, Display, Formatter, Result as FmtResult};
use std::path::PathBuf;

use miette::Diagnostic;
use thiserror::Error as ThisError;

use crate::Source;

// ============================================================================
// Provider Priority Registry
// ============================================================================

/// Centralized provider priority constants.
///
/// Lower values = higher priority. Providers are queried in priority order,
/// and the first one to return a value wins.
///
/// # Priority Hierarchy
///
/// ```text
/// CLI (10) > Environment (20) > Dotenv (30) > Profile (40) > ConfigFile (50) > Custom (100) > Default (1000)
/// ```
///
/// # Usage
///
/// When implementing a custom provider, use these constants or values
/// relative to them to ensure consistent priority ordering:
///
/// ```rust,ignore
/// impl Provider for MyProvider {
///     fn priority(&self) -> u32 {
///         // Between environment and dotenv
///         priority::ENVIRONMENT + 5
///     }
/// }
/// ```
pub mod priority {
    /// CLI arguments have highest priority (overrides everything).
    pub const CLI: u32 = 10;

    /// Environment variables (process env).
    pub const ENVIRONMENT: u32 = 20;

    /// Dotenv files (.env, .env.local, etc.).
    pub const DOTENV: u32 = 30;

    /// Profile-specific defaults (#[profile(dev = "...")]).
    pub const PROFILE: u32 = 40;

    /// Configuration files (TOML, JSON, YAML).
    pub const CONFIG_FILE: u32 = 50;

    /// Default priority for custom providers.
    pub const CUSTOM: u32 = 100;

    /// Compile-time defaults (#[env(default = "...")]).
    /// Lowest priority - only used when nothing else provides a value.
    pub const DEFAULT: u32 = 1000;
}

// ============================================================================
// Provider Value Types
// ============================================================================

/// A configuration value retrieved from a provider.
///
/// Contains the raw string value along with metadata about its source
/// and whether it should be treated as secret.
#[derive(Clone, Debug)]
pub struct ProviderValue {
    /// The raw string value (will be parsed by the config loader).
    pub value: String,

    /// Where this value originated from.
    pub source: ProviderSource,

    /// Whether this value is secret and should be masked in errors/ logs.
    pub secret: bool,
}

impl ProviderValue {
    /// Creates a new provider value.
    #[must_use]
    pub fn new(value: impl Into<String>, source: ProviderSource) -> Self {
        Self {
            value: value.into(),
            source,
            secret: false,
        }
    }

    /// Marks this value as secret (will be masked in errors).
    #[must_use]
    pub fn with_secret(mut self, secret: bool) -> Self {
        self.secret = secret;

        self
    }
}

// ============================================================================
// Provider Source (Extensible)
// ============================================================================

/// This enum wraps the built-in [`Source`] types and allows custom
/// providers to define their own source attribution.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ProviderSource {
    /// A built-in source type (environment, file, CLI, etc.).
    BuiltIn(Source),

    /// A custom provider source.
    Custom {
        /// The provider name (e.g., "vault", "aws-ssm", "consul").
        provider: String,

        /// Optional path or key within the provider.
        path: Option<String>,
    },
}

impl ProviderSource {
    /// Creates a custom provider source.
    #[must_use]
    pub fn custom(provider: impl Into<String>, path: Option<String>) -> Self {
        Self::Custom {
            provider: provider.into(),
            path,
        }
    }

    /// Creates a source from environment.
    #[must_use]
    pub fn environment() -> Self {
        Self::BuiltIn(Source::Environment)
    }

    /// Creates a source from a config file.
    #[must_use]
    pub fn config_file(path: Option<PathBuf>) -> Self {
        Self::BuiltIn(Source::ConfigFile(path))
    }

    /// Creates a source from a dotenv file.
    #[must_use]
    pub fn dotenv_file(path: Option<PathBuf>) -> Self {
        Self::BuiltIn(Source::DotenvFile(path))
    }

    /// Creates a default source.
    #[must_use]
    pub fn default_value() -> Self {
        Self::BuiltIn(Source::Default)
    }

    /// Converts this provider source to a [`Source`] for compatibility.
    #[must_use]
    pub fn to_source(&self) -> Source {
        match self {
            ProviderSource::BuiltIn(s) => s.clone(),

            ProviderSource::Custom { provider, .. } => Source::CustomProvider(provider.clone()),
        }
    }
}

impl Display for ProviderSource {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            ProviderSource::BuiltIn(source) => write!(f, "{source}"),

            ProviderSource::Custom {
                provider,
                path: Some(path),
            } => {
                write!(f, "{provider} ({path})")
            }

            ProviderSource::Custom {
                provider,
                path: None,
            } => {
                write!(f, "{provider}")
            }
        }
    }
}

// ============================================================================
// Provider Errors
// ============================================================================

/// Errors that can occur during provider operations.
#[derive(Debug, ThisError, Diagnostic)]
#[non_exhaustive]
pub enum ProviderError {
    /// The requested key was not found in this provider.
    #[error("key '{key}' not found in provider '{provider}'")]
    #[diagnostic(code(procenv::provider::not_found))]
    NotFound {
        /// The key that was not found.
        key: String,
        /// The provider name.
        provider: String,
    },

    /// Provider connection or authentication error.
    #[error("provider '{provider}' connection error: {message}")]
    #[diagnostic(
        code(procenv::provider::connection),
        help("check provider configuration and connectivity")
    )]
    Connection {
        /// The provider name.
        provider: String,
        /// The error message.
        message: String,
        /// The underlying error source.
        #[source]
        source: Option<Box<dyn StdError + Send + Sync>>,
    },

    /// Value format error (e.g., not valid UTF-8).
    #[error("invalid value for '{key}' from provider '{provider}': {message}")]
    #[diagnostic(code(procenv::provider::invalid_value))]
    InvalidValue {
        /// The key with the invalid value.
        key: String,
        /// The provider name.
        provider: String,
        /// The error message.
        message: String,
    },

    /// Provider is not available.
    #[error("provider '{provider}' is not available: {message}")]
    #[diagnostic(
        code(procenv::provider::unavailable),
        help("ensure the provider is properly configured and accessible")
    )]
    Unavailable {
        /// The provider name.
        provider: String,
        /// The error message.
        message: String,
    },

    /// Generic provider error.
    #[error("provider '{provider}' error: {message}")]
    #[diagnostic(code(procenv::provider::error))]
    Other {
        /// The provider name.
        provider: String,
        /// The error message.
        message: String,
        /// The underlying error source.
        #[source]
        source: Option<Box<dyn StdError + Send + Sync>>,
    },
}

impl ProviderError {
    /// Returns the provider name from the error.
    #[must_use]
    pub fn provider_name(&self) -> &str {
        match self {
            ProviderError::NotFound { provider, .. }
            | ProviderError::Connection { provider, .. }
            | ProviderError::InvalidValue { provider, .. }
            | ProviderError::Unavailable { provider, .. }
            | ProviderError::Other { provider, .. } => provider,
        }
    }

    /// Creates a connection error.
    pub fn connection(provider: impl Into<String>, message: impl Into<String>) -> Self {
        ProviderError::Connection {
            provider: provider.into(),
            message: message.into(),
            source: None,
        }
    }

    /// Creates a connection error with source.
    pub fn connection_with_source(
        provider: impl Into<String>,
        message: impl Into<String>,
        source: impl StdError + Send + Sync + 'static,
    ) -> Self {
        ProviderError::Connection {
            provider: provider.into(),
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }
}

/// Result type for provider operations.
///
/// - `Ok(Some(value))` - Key found, value returned
/// - `Ok(None)` - Key not found in this provider (try next)
/// - `Err(e)` - An error occurred
pub type ProviderResult<T> = Result<Option<T>, ProviderError>;

// ============================================================================
// Provider Trait (Sync)
// ============================================================================

/// Trait for synchronous configuration providers.
///
/// Implement this trait to create custom configuration sources that can
/// be used with procenv's [`ConfigLoader`](crate::loader::ConfigLoader).
///
/// # Priority
///
/// Providers are queried in priority order (lower number = higher priority).
/// Built-in priorities:
/// - CLI: 10
/// - Environment: 20
/// - Dotenv: 30
/// - Profile: 40
/// - `ConfigFile`: 50
/// - Custom providers: 100 (default)
/// - Defaults: 1000
///
/// # Example
///
/// ```rust,ignore
/// use procenv::provider::{Provider, ProviderResult, ProviderValue, ProviderSource};
///
/// struct MyProvider {
///     values: HashMap<String, String>,
/// }
///
/// impl Provider for MyProvider {
///     fn name(&self) -> &str { "my-provider" }
///
///     fn get(&self, key: &str) -> ProviderResult<ProviderValue> {
///         match self.values.get(key) {
///             Some(value) => Ok(Some(ProviderValue::new(
///                 value.clone(),
///                 ProviderSource::custom("my-provider", None),
///             ))),
///             None => Ok(None),
///         }
///     }
/// }
/// ```
pub trait Provider: Send + Sync {
    /// Returns the provider's name for error messages and source attribution.
    fn name(&self) -> &str;

    /// Gets a single configuration value by key.
    ///
    /// # Returns
    ///
    /// - `Ok(Some(value))` if the key was found
    /// - `Ok(None)` if the key was not found (loader will try next provider)
    /// - `Err(e)` if an error occurred
    ///
    /// # Errors
    ///
    /// Returns an error if the provider encounters an error fetching the value.
    fn get(&self, key: &str) -> ProviderResult<ProviderValue>;

    /// Gets multiple configuration values in a single call.
    ///
    /// This can be overridden for efficiency when fetching multiple keys
    /// from a remote provider. The default implementation calls `get()`
    /// for each key.
    fn get_many(&self, keys: &[&str]) -> HashMap<String, ProviderResult<ProviderValue>> {
        keys.iter()
            .map(|k| ((*k).to_string(), self.get(k)))
            .collect()
    }

    /// Checks if this provider is currently available.
    ///
    /// This can be used to skip providers that require network connectivity
    /// or are misconfigured. The default implementation returns `true`.
    fn is_available(&self) -> bool {
        true
    }

    /// Returns the priority of this provider (lower = higher priority).
    ///
    /// The default is [`priority::CUSTOM`] (100). Override to change when
    /// this provider is queried relative to others in the chain.
    ///
    /// See the [`priority`] module for standard priority constants.
    fn priority(&self) -> u32 {
        priority::CUSTOM
    }

    /// Returns whether this provider should be skipped when a key is not found.
    ///
    /// If `true` (default), a `Ok(None)` result causes the loader to try
    /// the next provider. If `false`, the loader stops and uses the default.
    fn fallthrough(&self) -> bool {
        true
    }
}

// ============================================================================
// Async Provider Trait
// ============================================================================

#[cfg(feature = "async")]
use std::future::Future;
#[cfg(feature = "async")]
use std::pin::Pin;

/// Boxed future type for async provider methods.
#[cfg(feature = "async")]
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Trait for asynchronous configuration providers.
///
/// This is the async equivalent of [`Provider`], suitable for providers
/// that need to make network calls (e.g., Vault, AWS Secrets Manager).
///
/// Use [`BlockingAdapter`] to convert an `AsyncProvider` to a sync `Provider`
/// when needed.
///
/// # Example
///
/// ```rust,ignore
/// use procenv::provider::{AsyncProvider, BoxFuture, ProviderResult, ProviderValue};
///
/// struct VaultProvider { client: VaultClient }
///
/// impl AsyncProvider for VaultProvider {
///     fn name(&self) -> &str { "vault" }
///
///     fn get<'a>(&'a self, key: &'a str) -> BoxFuture<'a, ProviderResult<ProviderValue>> {
///         Box::pin(async move {
///             let secret = self.client.read_secret(key).await?;
///             Ok(Some(ProviderValue::new(secret, ProviderSource::custom("vault", Some(key.into())))))
///         })
///     }
/// }
/// ```
#[cfg(feature = "async")]
pub trait AsyncProvider: Send + Sync {
    /// Returns the provider's name.
    fn name(&self) -> &str;

    /// Gets a configuration value asynchronously.
    fn get<'a>(&'a self, key: &'a str) -> BoxFuture<'a, ProviderResult<ProviderValue>>;

    /// Gets multiple values asynchronously.
    fn get_many<'a>(
        &'a self,
        keys: &'a [&'a str],
    ) -> BoxFuture<'a, HashMap<String, ProviderResult<ProviderValue>>> {
        Box::pin(async move {
            let mut results = HashMap::new();
            for key in keys {
                results.insert((*key).to_string(), self.get(key).await);
            }
            results
        })
    }

    /// Checks if the provider is available asynchronously.
    #[allow(clippy::elidable_lifetime_names)]
    fn is_available<'a>(&'a self) -> BoxFuture<'a, bool> {
        Box::pin(async { true })
    }

    /// Returns the priority of this provider.
    ///
    /// See the [`priority`] module for standard priority constants.
    fn priority(&self) -> u32 {
        priority::CUSTOM
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    struct TestProvider {
        values: HashMap<String, String>,
    }

    impl TestProvider {
        fn new() -> Self {
            let mut values = HashMap::new();
            values.insert("key1".to_string(), "value1".to_string());
            values.insert("key2".to_string(), "value2".to_string());
            Self { values }
        }
    }

    impl Provider for TestProvider {
        fn name(&self) -> &'static str {
            "test"
        }

        fn get(&self, key: &str) -> ProviderResult<ProviderValue> {
            match self.values.get(key) {
                Some(v) => Ok(Some(ProviderValue::new(
                    v.clone(),
                    ProviderSource::custom("test", None),
                ))),
                None => Ok(None),
            }
        }
    }

    #[test]
    fn test_provider_get() {
        let provider = TestProvider::new();

        let result = provider.get("key1").unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().value, "value1");

        let result = provider.get("missing").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_provider_get_many() {
        let provider = TestProvider::new();
        let results = provider.get_many(&["key1", "key2", "missing"]);

        assert_eq!(results.len(), 3);
        assert!(results.get("key1").unwrap().as_ref().unwrap().is_some());
        assert!(results.get("missing").unwrap().as_ref().unwrap().is_none());
    }

    #[test]
    fn test_provider_source_display() {
        let env = ProviderSource::environment();
        assert_eq!(env.to_string(), "Environment variable");

        let custom = ProviderSource::custom("vault", Some("secret/app/db".into()));
        assert_eq!(custom.to_string(), "vault (secret/app/db)");
    }

    #[test]
    fn test_provider_value_builder() {
        let value = ProviderValue::new("test", ProviderSource::environment()).with_secret(true);

        assert_eq!(value.value, "test");
        assert!(value.secret);
    }
}
