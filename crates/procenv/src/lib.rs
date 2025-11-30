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

// Secrecy integration
#[cfg(feature = "secrecy")]
pub use secrecy::{ExposeSecret, ExposeSecretMut, SecretBox, SecretString};

// ============================================================================
// Core Modules
// ============================================================================

// Diagnostic codes registry
pub mod diagnostic_codes;

// Error types
mod error;
pub use error::{Error, MaybeRedacted};

// Source attribution types
mod source;
pub use source::{ConfigSources, Source, ValueSource};

// Validation support (feature-gated)
#[cfg(feature = "validator")]
mod validation;
#[cfg(feature = "validator")]
pub use validation::{ValidationFieldError, validation_errors_to_procenv};

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
