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
pub use error::Error;

// Source attribution types
mod source;
pub use source::{ConfigSources, Source, ValueSource};

// Validation support (feature-gated)
#[cfg(feature = "validator")]
mod validation;
#[cfg(feature = "validator")]
pub use validation::{validation_errors_to_procenv, ValidationFieldError};

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
