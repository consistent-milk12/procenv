//! # procenv_macro
//!
//! This crate provides the `#[derive(EnvConfig)]` procedural macro.
//! It is a proc-macro crate, meaning it can only export procedural macros.
//!
//! ## Module Structure
//!
//! - `parse` - Attribute parsing for `#[env(...)]`
//! - `field` - Field type processing and code generation
//! - `expand` - Macro expansion orchestration

use proc_macro::TokenStream;
use syn::{DeriveInput, parse_macro_input};

// Internal modules - not exposed publicly
mod expand;
mod field;
mod parse;

/// Derive macro for loading configuration from environment variables.
///
/// This macro generates a `from_env()` method that reads environment variables
/// and constructs the struct, with full error accumulation support.
///
/// # Attributes
///
/// - `#[env(var = "NAME")]` - Required field, errors if missing
/// - `#[env(var = "NAME", default = "value")]` - Uses default if missing
/// - `#[env(var = "NAME", optional)]` - Field is `Option<T>`, None if missing
/// - `#[env(var = "NAME", secret)]` - Masks value in Debug/error output
///
/// # Example
///
/// ```ignore
/// #[derive(EnvConfig)]
/// struct Config {
///     #[env(var = "DATABASE_URL")]
///     db_url: String,
///
///     #[env(var = "PORT", default = "8080")]
///     port: u16,
///
///     #[env(var = "API_KEY", secret)]
///     api_key: String,
///
///     #[env(var = "DEBUG_MODE", optional)]
///     debug: Option<bool>,
/// }
///
/// fn main() -> Result<(), procenv::Error> {
///     let config = Config::from_env()?;
///     println!("Connected to: {}", config.db_url);
///     Ok(())
/// }
/// ```
///
/// # Generated Code
///
/// The macro generates:
/// 1. `impl Config { pub fn from_env() -> Result<Self, procenv::Error> }`
/// 2. `impl Debug for Config` (with secret masking)
#[proc_macro_derive(EnvConfig, attributes(env, env_config, profile))]
pub fn derive_env_config(input: TokenStream) -> TokenStream {
    // Parse the input TokenStream into syn's DeriveInput AST
    // This gives us structured access to the struct definition
    let input = parse_macro_input!(input as DeriveInput);

    // Delegate to the Expander which orchestrates code generation
    // On error, convert to a compile_error!() invocation for better error messages
    expand::Expander::expand(input).unwrap_or_else(|err| err.to_compile_error().into())
}
