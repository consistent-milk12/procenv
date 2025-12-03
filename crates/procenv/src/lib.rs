//! # procenv
//!
//! A procedural macro library for type-safe environment variable configuration in Rust.
//!
//! `procenv` eliminates boilerplate code when loading configuration from environment variables
//! by generating type-safe loading logic at compile time. It provides comprehensive error handling,
//! secret masking, and support for multiple configuration sources including `.env` files,
//! config files (TOML/JSON/YAML), CLI arguments, and environment variable profiles.
//!
//! ## What Makes procenv Unique
//!
//! Unlike other configuration crates, procenv:
//!
//! - **Accumulates all errors** - Shows every configuration problem at once, not one at a time
//! - **Provides source spans** - File config errors point to the exact line/column
//! - **Masks secrets structurally** - Secret values are never stored in error messages
//! - **Integrates CLI arguments** - Generate clap arguments from the same struct
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
//! ## Error Output Example
//!
//! When multiple configuration errors occur, they're all reported together:
//!
//! ```text
//! procenv::multiple_errors
//!
//!   × 3 configuration error(s) occurred
//!
//! Error: procenv::missing_var
//!   × missing required environment variable: DATABASE_URL
//!   help: set DATABASE_URL in your environment or .env file
//!
//! Error: procenv::parse_error
//!   × failed to parse PORT: expected u16, got "not_a_number"
//!   help: expected a valid u16
//!
//! Error: procenv::missing_var
//!   × missing required environment variable: SECRET
//!   help: set SECRET in your environment or .env file
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
//! | `from_env_validated()` | Load + validate (requires `validator` feature) |
//! | `env_example()` | Generate `.env.example` template |
//! | `keys()` | List all field names |
//! | `get_str(&self, key)` | Get field value as string |
//! | `has_key(key)` | Check if field exists |
//!
//! ## Feature Flags
//!
//! | Feature | Description | Default |
//! |---------|-------------|---------|
//! | `dotenv` | Load `.env` files automatically | **Yes** |
//! | `secrecy` | [`SecretString`] support for sensitive fields | No |
//! | `clap` | CLI argument integration with [`clap`] | No |
//! | `file` | Base config file support (JSON) | No |
//! | `toml` | TOML file parsing (implies `file`) | No |
//! | `yaml` | YAML file parsing (implies `file`) | No |
//! | `file-all` | All file formats (toml + yaml + json) | No |
//! | `validator` | Validation via [`validator`] crate | No |
//! | `provider` | Custom provider extensibility | No |
//! | `watch` | Hot reload with file watching | No |
//! | `full` | Enable all features | No |
//!
//! ## Secret Handling
//!
//! procenv provides two-tier secret protection:
//!
//! 1. **Error-time protection** (always on): Secrets marked with `#[env(secret)]`
//!    are never stored in error messages. Uses [`MaybeRedacted`] internally.
//!
//! 2. **Runtime protection** (requires `secrecy` feature): Use [`SecretString`]
//!    for values that should be protected in memory and Debug output.
//!
//! ```rust,ignore
//! #[derive(EnvConfig)]
//! struct Config {
//!     #[env(var = "API_KEY", secret)]  // Protected in errors
//!     api_key: SecretString,            // Protected at runtime
//! }
//! ```
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

// Re-export the derive macro
pub use procenv_macro::EnvConfig;

// ============================================================================
// Re-exported Dependencies
// ============================================================================
// These are re-exported so users don't need to add them as direct dependencies.
// The macro generates code that references these via ::procenv::serde::, etc.

/// Re-export miette for error handling.
/// Users can use `procenv::miette` instead of adding miette as a dependency.
pub use miette;

/// Re-export serde when the feature is enabled.
/// Required for file-based configuration and the `format` attribute.
#[cfg(feature = "serde")]
pub use serde;

// Convenience re-exports for common serde types
#[cfg(feature = "serde")]
pub use serde::{Deserialize, Serialize};

/// Re-export serde_json when the serde feature is enabled.
#[cfg(feature = "serde")]
pub use serde_json;

/// Re-export clap when the feature is enabled.
/// Required for CLI argument integration.
#[cfg(feature = "clap")]
pub use clap;

/// Re-export validator when the feature is enabled.
/// Required for validation support.
#[cfg(feature = "validator")]
pub use validator;

/// Re-export secrecy when the feature is enabled.
/// Required for secret field types.
#[cfg(feature = "secrecy")]
pub use secrecy;

// Convenience re-exports for common secrecy types
#[cfg(feature = "secrecy")]
pub use secrecy::{ExposeSecret, ExposeSecretMut, SecretBox, SecretString};

/// Re-export toml when the feature is enabled.
#[cfg(feature = "toml")]
pub use toml;

/// Re-export serde-saphyr (yaml) when the feature is enabled.
#[cfg(feature = "yaml")]
pub use serde_saphyr as yaml;

/// Re-export dotenvy when the dotenv feature is enabled.
#[cfg(feature = "dotenv")]
pub use dotenvy;

// ============================================================================
// Core Modules
// ============================================================================

// Diagnostic codes registry
pub mod diagnostic_codes;

// Error types
mod error;
pub use error::{Error, MaybeRedacted};

/// A Result type that displays errors with miette's fancy formatting.
///
/// Use this as your main function return type for pretty error output:
///
/// ```rust,ignore
/// fn main() -> procenv::Result<()> {
///     let config = Config::from_env()?;
///     Ok(())
/// }
/// ```
///
/// This is equivalent to `miette::Result<T>` and will automatically
/// display configuration errors with colors, help text, and error codes.
pub type Result<T> = miette::Result<T>;

// Source attribution types
mod source;
pub use source::{ConfigSources, Source, ValueSource};

// Validation support (feature-gated)
#[cfg(feature = "validator")]
mod validation;
#[cfg(feature = "validator")]
pub use validation::{ValidationFieldError, validation_errors_to_procenv};
// Re-export Validate trait for macro-generated code
// This allows the macro to reference ::procenv::Validate instead of ::validator::Validate
// which provides clear compile errors when the validator feature isn't enabled
#[cfg(feature = "validator")]
pub use validator::Validate;

// ============================================================================
// File Configuration Support
// ============================================================================

#[cfg(feature = "file")]
pub mod file;
#[cfg(feature = "file")]
pub use file::{ConfigBuilder, FileFormat, FileUtils, OriginTracker};

// ============================================================================
// Provider Extensibility
// ============================================================================

pub mod loader;
pub mod provider;
pub mod value;

pub use value::ConfigValue;

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

// ============================================================================
// Hot Reload Support (Phase E)
// ============================================================================

#[cfg(feature = "watch")]
pub mod watch;

#[cfg(feature = "watch")]
pub use watch::{
    ChangeTrigger, ChangedField, ConfigChange, ConfigHandle, WatchBuilder, WatchCommand,
    WatchError, WatchedConfig,
};
