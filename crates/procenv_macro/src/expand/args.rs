//! CLI argument integration code generation.
//!
//! This module generates the `from_args()` method for loading configuration
//! from command-line arguments using the `clap` crate.
//!
//! # Generated Methods
//!
//! - [`generate_from_args_impl`] - Main implementation including:
//!   - `from_args()` - Load from `std::env::args()`
//!   - `from_args_from(iter)` - Load from custom iterator (for testing)
//!   - `from_args_with_sources()` - With source attribution
//!
//! # Priority Order
//!
//! CLI arguments have the highest priority:
//!
//! 1. **CLI arguments** (highest) - `--port 8080`
//! 2. **Environment variables** - `PORT=8080`
//! 3. **Dotenv files** - `.env` contents
//! 4. **Defaults** (lowest) - `default = "8080"`
//!
//! # Field Configuration
//!
//! Fields must have CLI config to be included:
//!
//! ```rust,ignore
//! #[env(var = "PORT", default = "8080", arg = "port", short = 'p')]
//! port: u16,
//! ```
//!
//! Generates a clap argument:
//! ```rust,ignore
//! Arg::new("port")
//!     .long("port")
//!     .short('p')
//!     .value_name("port")
//! ```
//!
//! # CLI-Aware Loading
//!
//! The generated loaders check CLI first, then fall back to environment:
//!
//! ```rust,ignore
//! let port = if let Some(cli_val) = __port_cli {
//!     cli_val.parse()?  // From CLI
//! } else {
//!     std::env::var("PORT")?.parse()?  // From env
//! };
//! ```

use proc_macro2::TokenStream as QuoteStream;
use quote::{format_ident, quote};
use syn::Ident;

use crate::field::FieldGenerator;
use crate::parse::EnvConfigAttr;

use super::env::{generate_dotenv_load, generate_field_loader, generate_profile_setup};

/// Generate the from_args() method for CLI argument integration.
pub fn generate_from_args_impl(
    struct_name: &Ident,
    generators: &[Box<dyn FieldGenerator>],
    env_config: &EnvConfigAttr,
) -> QuoteStream {
    // Collect clap Arg definitions for CLI-enabled fields
    let clap_args: Vec<QuoteStream> = generators
        .iter()
        .filter_map(|g| g.generate_clap_arg())
        .collect();

    // Collect CLI value extractions
    let cli_extractions: Vec<QuoteStream> = generators
        .iter()
        .filter_map(|g| g.generate_cli_extraction())
        .collect();

    // Generate loaders that check CLI first, then env
    let loaders: Vec<QuoteStream> = generators
        .iter()
        .map(|g| generate_cli_aware_loader(g.as_ref(), env_config))
        .collect();

    // Generate profile setup code
    let profile_setup = generate_profile_setup(env_config);

    // Generate source tracking with CLI awareness
    let source_tracking: Vec<QuoteStream> = generators
        .iter()
        .map(|g| generate_cli_aware_source_tracking(g.as_ref()))
        .collect();

    // Generate assignments
    let assignments: Vec<QuoteStream> =
        generators.iter().map(|g| g.generate_assignment()).collect();

    // Dotenv loading
    let dotenv_load = generate_dotenv_load(&env_config.dotenv);

    let dotenv_loaded_flag = if env_config.dotenv.is_some() {
        quote! { let __dotenv_loaded = true; }
    } else {
        quote! { let __dotenv_loaded = false; }
    };

    // Collect env var names for pre-dotenv check
    let env_var_names: Vec<_> = generators.iter().filter_map(|g| g.env_var_name()).collect();

    quote! {
        impl #struct_name {
            /// Load configuration from CLI arguments and environment.
            ///
            /// Priority: CLI arguments > Environment variables > .env files > Defaults
            pub fn from_args() -> std::result::Result<Self, ::procenv::Error> {
                let (config, _) = Self::from_args_with_sources()?;
                std::result::Result::Ok(config)
            }

            /// Load configuration from a custom argument iterator.
            pub fn from_args_from<I, T>(args: I) -> std::result::Result<Self, ::procenv::Error>
            where
                I: IntoIterator<Item = T>,
                T: Into<std::ffi::OsString> + Clone,
            {
                let (config, _) = Self::from_args_from_with_sources(args)?;
                std::result::Result::Ok(config)
            }

            /// Load configuration from CLI arguments with source attribution.
            pub fn from_args_with_sources() -> std::result::Result<(Self, ::procenv::ConfigSources), ::procenv::Error> {
                let __cmd = ::clap::Command::new(env!("CARGO_PKG_NAME"))
                    .version(env!("CARGO_PKG_VERSION"))
                    #(.arg(#clap_args))*;

                let __matches = __cmd.get_matches();

                Self::__from_args_matches(__matches)
            }

            /// Load configuration from a custom argument iterator with source attribution.
            pub fn from_args_from_with_sources<I, T>(args: I) -> std::result::Result<(Self, ::procenv::ConfigSources), ::procenv::Error>
            where
                I: IntoIterator<Item = T>,
                T: Into<std::ffi::OsString> + Clone,
            {
                let __cmd = ::clap::Command::new(env!("CARGO_PKG_NAME"))
                    .version(env!("CARGO_PKG_VERSION"))
                    #(.arg(#clap_args))*;

                let __matches = __cmd.try_get_matches_from(args)
                    .map_err(|e| ::procenv::Error::Cli { message: e.to_string() })?;

                Self::__from_args_matches(__matches)
            }

            /// Internal helper to process clap matches into config.
            fn __from_args_matches(__matches: ::clap::ArgMatches) -> std::result::Result<(Self, ::procenv::ConfigSources), ::procenv::Error> {

                // Extract CLI values
                #(#cli_extractions)*

                // Track pre-dotenv env vars
                let __pre_dotenv_vars: std::collections::HashSet<&str> = [
                    #(#env_var_names),*
                ]
                .iter()
                .filter(|var| std::env::var(var).is_ok())
                .copied()
                .collect();

                // Load dotenv
                #dotenv_load
                #dotenv_loaded_flag

                // Define external prefix as None for regular from_args calls
                let __external_prefix: std::option::Option<&str> = std::option::Option::None;

                // Error accumulator
                let mut __errors: std::vec::Vec<::procenv::Error> = std::vec::Vec::new();
                let mut __sources = ::procenv::ConfigSources::new();

                // Read and validate profile (if configured)
                #profile_setup

                // Load each field (CLI first, then env)
                #(#loaders)*

                // Track sources
                #(#source_tracking)*

                // Check for errors
                if !__errors.is_empty() {
                    return std::result::Result::Err(if __errors.len() == 1 {
                        __errors.pop().unwrap()
                    } else {
                        ::procenv::Error::Multiple { errors: __errors }
                    });
                }

                std::result::Result::Ok((
                    Self {
                        #(#assignments),*
                    },
                    __sources
                ))
            }
        }
    }
}

/// Generate a loader that checks CLI value first, then falls back to env.
fn generate_cli_aware_loader(
    field: &dyn FieldGenerator,
    env_config: &EnvConfigAttr,
) -> QuoteStream {
    let name = field.name();
    let cli_var = format_ident!("__{}_cli", name);
    let from_cli_var = format_ident!("__{}_from_cli", name);

    // Check if this field has CLI config
    if field.cli_config().is_some() {
        // Use the profile-aware field loader for env fallback
        let env_loader = generate_field_loader(field, env_config);

        // For CLI-enabled fields: check CLI first, then env
        let cli_arg_name = format!("--{}", field.cli_config().unwrap().long.as_ref().unwrap());
        let type_name = field.type_name();
        let secret = field.is_secret();

        // Generate the parse expression based on format
        let (parse_expr, format_name) = if let Some(format) = field.format_config() {
            let expr = match format {
                "json" => quote! { ::serde_json::from_str(cli_val) },
                "toml" => quote! { ::toml::from_str(cli_val) },
                "yaml" => quote! { ::serde_saphyr::from_str(cli_val) },
                _ => quote! { cli_val.parse() },
            };
            (expr, format.to_uppercase())
        } else {
            (quote! { cli_val.parse() }, type_name.clone())
        };

        quote! {
            let #from_cli_var: bool;
            let #name = if let std::option::Option::Some(ref cli_val) = #cli_var {
                #from_cli_var = true;
                match #parse_expr {
                    std::result::Result::Ok(v) => std::option::Option::Some(v),
                    std::result::Result::Err(e) => {
                        __errors.push(::procenv::Error::parse(
                            #cli_arg_name,
                            cli_val.clone(),
                            #secret,
                            #format_name,
                            std::boxed::Box::new(e),
                        ));
                        std::option::Option::None
                    }
                }
            } else {
                #from_cli_var = false;
                #env_loader
                #name
            };
        }
    } else {
        // Non-CLI fields: use profile-aware field loader
        generate_field_loader(field, env_config)
    }
}

/// Generate source tracking that accounts for CLI values.
fn generate_cli_aware_source_tracking(field: &dyn FieldGenerator) -> QuoteStream {
    let name = field.name();
    let name_str = name.to_string();
    let from_cli_var = format_ident!("__{}_from_cli", name);
    let source_ident = format_ident!("__{}_source", name);

    if let Some(env_var) = field.env_var_name() {
        if field.cli_config().is_some() {
            // CLI-enabled field: check if value came from CLI
            quote! {
                let #source_ident = if #from_cli_var {
                    ::procenv::ValueSource::new(#env_var, ::procenv::Source::Cli)
                } else if #name.is_some() {
                    if __dotenv_loaded && !__pre_dotenv_vars.contains(#env_var) {
                        ::procenv::ValueSource::new(#env_var, ::procenv::Source::DotenvFile(None))
                    } else {
                        ::procenv::ValueSource::new(#env_var, ::procenv::Source::Environment)
                    }
                } else {
                    ::procenv::ValueSource::new(#env_var, ::procenv::Source::NotSet)
                };
                __sources.add(#name_str, #source_ident);
            }
        } else {
            // Non-CLI field: use standard source tracking
            field.generate_source_tracking()
        }
    } else {
        // Flatten fields or fields without env var
        field.generate_source_tracking()
    }
}
