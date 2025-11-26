//! Code generation orchestration for EnvConfig derive macro.
//!
//! This module is the conductor of the macro - it:
//! 1. Validates that the input is a struct with named fields
//! 2. Parses each field into a `FieldGenerator`
//! 3. Generates the `from_env()` method implementation
//! 4. Generates a custom `Debug` implementation with secret masking
//!
//! ## Error Accumulation Pattern
//!
//! Instead of failing on the first error, we collect all errors into a
//! `Vec<Error>` and return them together. This gives users a complete
//! picture of what's missing or misconfigured, rather than making them
//! fix one error at a time.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as QuoteStream;
use quote::{format_ident, quote};
use syn::punctuated::Punctuated;
use syn::token::Comma;
use syn::{
    Data, DeriveInput, Error as SynError, Field, Fields, Generics, Ident, ImplGenerics,
    Result as SynResult, TypeGenerics, WhereClause,
};

use crate::field::{FieldFactory, FieldGenerator};
use crate::parse::{DotenvConfig, EnvConfigAttr};

/// The main orchestrator for macro expansion.
///
/// This is a zero-sized type that serves as a namespace for the
/// expansion functions. All methods are associated functions.
pub struct Expander;

impl Expander {
    /// Main entry point for expanding the derive macro.
    ///
    /// Takes the parsed `DeriveInput` and returns the generated `TokenStream`.
    ///
    /// # Errors
    ///
    /// Returns a `syn::Error` if:
    /// - The input is not a struct (enums and unions are not supported)
    /// - The struct has unnamed fields (tuple structs) or no fields (unit structs)
    /// - Any field is missing the `#[env(...)]` attribute
    /// - Any attribute has invalid syntax
    pub fn expand(input: DeriveInput) -> SynResult<TokenStream> {
        let struct_name = &input.ident;
        let generics = &input.generics;

        // Parse struct-level #[env_config(...)] attribute
        let env_config_attr = EnvConfigAttr::parse_from_struct(&input)?;

        // Validate and extract the struct's named fields
        let fields = Self::extract_struct_fields(&input)?;

        // Parse each field into a FieldGenerator trait object
        // This is where attribute parsing and field classification happens
        let generators: Vec<Box<dyn FieldGenerator>> = fields
            .iter()
            .map(|f| FieldFactory::parse_field(f, env_config_attr.prefix.as_deref()))
            .collect::<SynResult<Vec<_>>>()?;

        let from_env_impl =
            Self::generate_from_env_impl(struct_name, generics, &generators, &env_config_attr);

        let debug_impl = Self::generate_debug_impl(struct_name, generics, &generators);

        let env_example_impl = Self::generate_env_example_impl(struct_name, generics, &generators);

        let sources_impl =
            Self::generate_from_env_with_sources_impl(struct_name, &generators, &env_config_attr);

        // Generate file config method if files are configured
        let file_config_impl = if !env_config_attr.files.is_empty() {
            Self::generate_from_config_impl(struct_name, generics, &env_config_attr)
        } else {
            quote! {}
        };

        let combined = quote! {
            #from_env_impl
            #debug_impl
            #env_example_impl
            #sources_impl
            #file_config_impl
        };

        Ok(combined.into())
    }

    /// Extract named fields from the struct, rejecting invalid types.
    ///
    /// We only support structs with named fields because:
    /// - We need fields names to generate meaningful error messages
    /// - Environment variables map naturally to named fields
    /// - Tuple structs would require positional attribute syntax
    fn extract_struct_fields(input: &DeriveInput) -> SynResult<&Punctuated<Field, Comma>> {
        match &input.data {
            Data::Struct(data_struct) => match &data_struct.fields {
                // Named fields: struct Foo { bar: i32 }
                Fields::Named(fields_named) => Ok(&fields_named.named),

                // Tuple struct: struct struct Foo(i32)
                Fields::Unnamed(_) => Err(SynError::new_spanned(
                    input,
                    "EnvConfig does not support tuple structs",
                )),

                // Unit struct: struct Foo;
                Fields::Unit => Err(SynError::new_spanned(
                    input,
                    "EnvConfig does not support unit structs",
                )),
            },

            // enum Foo { ... }
            Data::Enum(_) => Err(SynError::new_spanned(
                input,
                "EnvConfig can only be derived for structs, not enums",
            )),

            Data::Union(_) => Err(SynError::new_spanned(
                input,
                "EnvConfig can only be derived for structs, not unions",
            )),
        }
    }

    /// Generate the `from_env()` method implementation.
    ///
    /// This generates code that:
    /// 1. Optionally loads .env file(s) if configured
    /// 2. Reads and validates profile if configured
    /// 3. Creates an error accumulator vector
    /// 4. Loads each field (calling each FieldGenerator's `generate_loader()`)
    /// 5. If any errors occurred, returns the (single or Multiple variant)
    /// 6. Otherwise constructs and returns the struct
    fn generate_from_env_impl(
        struct_name: &Ident,
        generics: &Generics,
        fields: &[Box<dyn FieldGenerator>],
        env_config_attr: &EnvConfigAttr,
    ) -> QuoteStream {
        // Split generics for the impl block
        // This handles generic structs `struct Config<T> { ... }`
        let (impl_generics, type_generics, where_clause): (
            ImplGenerics<'_>,
            TypeGenerics<'_>,
            Option<&WhereClause>,
        ) = generics.split_for_impl();

        // Generate loader code for each field
        // Each loader reads an env var and stores result in a local Option<T>
        let loaders: Vec<QuoteStream> = fields
            .iter()
            .map(|f| Self::generate_field_loader(f.as_ref(), env_config_attr))
            .collect();

        // Generate assignment code for each field
        // This creates the struct initialization: `field: field.unwrap()`
        let assignments: Vec<QuoteStream> = fields
            .iter()
            .map(|f: &Box<dyn FieldGenerator + 'static>| f.generate_assignment())
            .collect();

        // Generate dotenv loading code (if configured)
        let dotenv_load = Self::generate_dotenv_load(&env_config_attr.dotenv);

        // Generate profile setup code (if configured)
        let profile_setup = Self::generate_profile_setup(env_config_attr);

        quote! {
            impl #impl_generics #struct_name #type_generics #where_clause {
                /// Load configuration from environment variables.
                ///
                /// This method reads all configured environment variables and
                /// attempts to parse them into their respective types.
                ///
                /// # Errors
                /// Returns an error if any required variables are missing or
                /// if any values fail to parse. All errors are accumulated
                /// and returned together.
                pub fn from_env() -> std::result::Result<Self, ::procenv::Error> {
                    // Load .env file(s) if configured (errors are silently ignored)
                    #dotenv_load

                    // Accumulator for all errors encountered during loading
                    let mut __errors: std::vec::Vec<::procenv::Error> = std::vec::Vec::new();

                    // Read and validate profile (if configured)
                    #profile_setup

                    // Load each field - errors are pushed to __errors
                    #(#loaders)*

                    // If any errors occurred, return them
                    if !__errors.is_empty() {
                        return std::result::Result::Err(if __errors.len() == 1 {
                            // Single error - return it directly for cleaner output
                            __errors.pop().unwrap()
                        } else {
                            // Multiple errors - wrap in Multiple variant
                            ::procenv::Error::Multiple { errors: __errors }
                        });
                    }

                    // All fields loaded successfully - construct the struct
                    std::result::Result::Ok(Self {
                        #(#assignments),*
                    })
                }
            }
        }
    }

    /// Generate code to setup profile from env var and validate it.
    fn generate_profile_setup(env_config_attr: &EnvConfigAttr) -> QuoteStream {
        let Some(profile_env) = &env_config_attr.profile_env else {
            // No profile configured - just define __profile as None
            return quote! {
                let __profile: std::option::Option<std::string::String> = std::option::Option::None;
            };
        };

        // Generate profile validation if profiles list is provided
        let validation = if let Some(profiles) = &env_config_attr.profiles {
            let profile_strs: Vec<&str> = profiles.iter().map(|s| s.as_str()).collect();
            quote! {
                // Validate profile against allowed list
                if let std::option::Option::Some(ref p) = __profile {
                    let valid_profiles: &[&str] = &[#(#profile_strs),*];
                    if !valid_profiles.contains(&p.as_str()) {
                        __errors.push(::procenv::Error::invalid_profile(
                            p.clone(),
                            #profile_env,
                            valid_profiles.to_vec(),
                        ));
                    }
                }
            }
        } else {
            quote! {}
        };

        quote! {
            // Read profile from environment variable
            let __profile: std::option::Option<std::string::String> = std::env::var(#profile_env).ok();
            #validation
        }
    }

    /// Generate field loader with profile and format support.
    ///
    /// If the field has profile values, generates code that checks profile first,
    /// then falls back to env var. Profile values act as defaults that can still
    /// be overridden by environment variables.
    ///
    /// If the field has format specified, uses serde deserialization instead of FromStr.
    fn generate_field_loader(
        field: &dyn FieldGenerator,
        _env_config_attr: &EnvConfigAttr,
    ) -> QuoteStream {
        // Determine which loader to use based on format
        let base_loader = if let Some(format) = field.format_config() {
            Self::generate_format_aware_loader(field, format)
        } else {
            field.generate_loader()
        };

        // Check if this field has profile-specific values
        let Some(profile_config) = field.profile_config() else {
            // No profile config - use the base loader directly
            return base_loader;
        };

        // Field has profile values - generate profile-aware loader
        // Profile values serve as fallback defaults (env vars still take priority)
        let name = field.name();
        let profile_used_ident = format_ident!("__{}_from_profile", name);

        // Generate match arms for each profile
        let match_arms: Vec<QuoteStream> = profile_config
            .values
            .iter()
            .map(|(profile_name, value)| {
                quote! {
                    std::option::Option::Some(#profile_name) => std::option::Option::Some(#value),
                }
            })
            .collect();

        // Get env var name for checking env override
        let env_var = field.env_var_name().unwrap_or("");

        quote! {
            // Determine profile default value (if profile matches)
            let __profile_default: std::option::Option<&str> = match __profile.as_deref() {
                #(#match_arms)*
                _ => std::option::Option::None,
            };

            // Check if env var is explicitly set (takes priority over profile)
            let __env_explicitly_set = std::env::var(#env_var).is_ok();
            let #profile_used_ident = __profile_default.is_some() && !__env_explicitly_set;

            // If profile provides a value and env is not set, temporarily set the env var
            // so the standard loader can use it
            if let (std::option::Option::Some(profile_val), false) = (__profile_default, __env_explicitly_set) {
                // SAFETY: We're in single-threaded config loading context
                unsafe {
                    std::env::set_var(#env_var, profile_val);
                }
            }

            // Run the appropriate loader (format-aware or standard)
            #base_loader

            // Clean up: remove the temporarily set env var if we set it from profile
            if #profile_used_ident {
                // SAFETY: We're in single-threaded config loading context
                unsafe {
                    std::env::remove_var(#env_var);
                }
            }
        }
    }

    /// Generate loader code that uses serde deserialization instead of FromStr.
    ///
    /// This delegates to the field's `generate_format_loader()` method so that
    /// each field type (required/default/optional) maintains its proper semantics.
    fn generate_format_aware_loader(field: &dyn FieldGenerator, format: &str) -> QuoteStream {
        field.generate_format_loader(format)
    }

    /// Generate code to load .env file(s) based on configuration.
    ///
    /// The generated code silently ignores errors (file not found, parse errors)
    /// because missing .env files are expected in production environments.
    fn generate_dotenv_load(dotenv_config: &Option<DotenvConfig>) -> QuoteStream {
        match dotenv_config {
            None => {
                // No dotenv configured - generate nothing
                quote! {}
            }

            Some(DotenvConfig::Default) => {
                // #[env_config(dotenv)] - search for .env from current dir upward
                quote! {
                    let _ = ::dotenvy::dotenv();
                }
            }

            Some(DotenvConfig::Custom(path)) => {
                // #[env_config(dotenv = ".env.local")] - load specific file
                quote! {
                    let _ = ::dotenvy::from_filename(#path);
                }
            }

            Some(DotenvConfig::Multiple(paths)) => {
                // #[env_config(dotenv = [".env", ".env.local"])] - load multiple files
                // Later files override earlier ones
                let load_calls: Vec<QuoteStream> = paths
                    .iter()
                    .map(|p| {
                        quote! {
                            let _ = ::dotenvy::from_filename(#p);
                        }
                    })
                    .collect();

                quote! {
                    #(#load_calls)*
                }
            }
        }
    }

    /// Generate a custom `Debug` implementation with secret masking.
    ///
    /// This generates code that shows "***" for fields marked with `secret`,
    /// preventing accidental exposure of sensitive data in logs or debug output.
    fn generate_debug_impl(
        struct_name: &Ident,
        generics: &Generics,
        fields: &[Box<dyn FieldGenerator>],
    ) -> QuoteStream {
        let (impl_generics, type_generics, where_clause) = generics.split_for_impl();

        // Get struct name as string for debug_struct()
        let struct_name_str = struct_name.to_string();

        // Generate .field() calls for each field
        let field_entries: Vec<QuoteStream> = fields
            .iter()
            .map(|f| {
                let name = f.name();
                let name_str = name.to_string();

                if f.is_secrecy_type() {
                    // Secrecy types handle their own Debug - just referene the field
                    quote! { .field(#name_str, &self.#name) }
                } else if f.is_secret() {
                    // Manual secret field - show placeholder
                    quote! { .field(#name_str, &"[REDACTED]") }
                } else {
                    // Normal field - show actual value
                    quote! { .field(#name_str, &self.#name) }
                }
            })
            .collect();

        quote! {
            impl #impl_generics std::fmt::Debug for #struct_name #type_generics #where_clause {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    f.debug_struct(#struct_name_str)
                        #(#field_entries)*
                        .finish()
                }
            }
        }
    }

    /// Generate the `env_example()` method for .env.example generation.
    ///
    /// This generates a method that returns a String containing a properly
    /// formatted .env.example file content with all environment variables,
    /// their types, defaults, and documentation.
    fn generate_env_example_impl(
        struct_name: &Ident,
        generics: &Generics,
        fields: &[Box<dyn FieldGenerator>],
    ) -> QuoteStream {
        let (impl_generics, type_generics, where_clause) = generics.split_for_impl();

        // Collect fragments for each field
        // - Regular fields: string literal with formatted entry
        // - Flattened fields: call to nested type's env_example()
        let mut fragments: Vec<QuoteStream> = Vec::new();

        for field in fields {
            if field.is_flatten() {
                // Flattened field - generate call to nested type
                let fragment = field.generate_example_fragment();
                fragments.push(quote! {
                    parts.push(#fragment);
                });
            } else {
                // Regular field - format entries at compile time
                let entries = field.example_entries();
                for entry in entries {
                    let formatted = entry.format();
                    fragments.push(quote! {
                        parts.push(#formatted.to_string());
                    });
                }
            }
        }

        quote! {
            impl #impl_generics #struct_name #type_generics #where_clause {
                /// Generate a .env.example file content.
                ///
                /// Returns a String containing all environment variables needed
                /// by this config struct, with documentation, types, and defaults.
                ///
                /// # Example
                ///
                /// ```ignore
                /// std::fs::write(".env.example", Config::env_example())?;
                /// ```
                pub fn env_example() -> std::string::String {
                    let mut parts: std::vec::Vec<std::string::String> = std::vec::Vec::new();

                    // Header
                    parts.push("# Auto-generated by procenv".to_string());
                    parts.push("".to_string());

                    // Entries (from this struct and any nested configs)
                    parts.push(Self::env_example_entries());

                    parts.join("\n")
                }

                /// Generate .env.example entries without header.
                ///
                /// This is used internally for nested configs to avoid
                /// duplicate headers. Use `env_example()` for the full output.
                pub fn env_example_entries() -> std::string::String {
                    let mut parts: std::vec::Vec<std::string::String> = std::vec::Vec::new();

                    #(#fragments)*

                    parts.join("\n")
                }
            }
        }
    }

    /// Generate the `from_config()` method for file-based configuration loading.
    ///
    /// This generates a method that uses ConfigBuilder to:
    /// 1. Load config files (in order, with layering)
    /// 2. Overlay environment variables
    /// 3. Deserialize to the struct
    ///
    /// Requires the struct to implement serde::Deserialize.
    fn generate_from_config_impl(
        struct_name: &Ident,
        generics: &Generics,
        env_config_attr: &EnvConfigAttr,
    ) -> QuoteStream {
        let (impl_generics, type_generics, where_clause) = generics.split_for_impl();

        // Generate file loading code
        let file_loads: Vec<QuoteStream> = env_config_attr
            .files
            .iter()
            .map(|f| {
                let path = &f.path;
                if f.required {
                    quote! {
                        builder = builder.file(#path);
                    }
                } else {
                    quote! {
                        builder = builder.file_optional(#path);
                    }
                }
            })
            .collect();

        // Generate env prefix setup
        let env_prefix = if let Some(prefix) = &env_config_attr.prefix {
            quote! { builder = builder.env_prefix(#prefix); }
        } else {
            quote! {}
        };

        // Generate dotenv loading (before ConfigBuilder so env vars are available)
        let dotenv_load = Self::generate_dotenv_load(&env_config_attr.dotenv);

        quote! {
            impl #impl_generics #struct_name #type_generics #where_clause
            where
                Self: ::serde::de::DeserializeOwned,
            {
                /// Load configuration from files and environment variables.
                ///
                /// This method loads configuration using the layered approach:
                /// 1. Config files (in order specified, later files override)
                /// 2. Environment variables (highest priority)
                ///
                /// # Errors
                ///
                /// Returns an error if:
                /// - A required config file is missing
                /// - A config file has invalid syntax
                /// - The merged config cannot be deserialized to this struct
                ///
                /// # Example
                ///
                /// ```ignore
                /// let config = Config::from_config()?;
                /// ```
                pub fn from_config() -> std::result::Result<Self, ::procenv::Error> {
                    // Load .env file(s) first so env vars are available
                    #dotenv_load

                    let mut builder = ::procenv::ConfigBuilder::new();

                    // Add config files
                    #(#file_loads)*

                    // Set env prefix for env var overlay
                    #env_prefix

                    // Build and deserialize
                    builder.build()
                }
            }
        }
    }

    /// Generate the from_args() method for CLI argument integration.
    ///
    /// This generates a method that:
    /// 1. Parses CLI arguments using clap
    /// 2. Uses CLI values with highest priority
    /// 3. Falls back to env vars, dotenv, then defaults
    fn generate_from_args_impl(
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
            .map(|g| Self::generate_cli_aware_loader(g.as_ref()))
            .collect();

        // Generate source tracking with CLI awareness
        let source_tracking: Vec<QuoteStream> = generators
            .iter()
            .map(|g| Self::generate_cli_aware_source_tracking(g.as_ref()))
            .collect();

        // Generate assignments
        let assignments: Vec<QuoteStream> =
            generators.iter().map(|g| g.generate_assignment()).collect();

        // Dotenv loading
        let dotenv_load = Self::generate_dotenv_load(&env_config.dotenv);

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
                ///
                /// # Errors
                ///
                /// Returns an error if required values are missing or fail to parse.
                pub fn from_args() -> std::result::Result<Self, ::procenv::Error> {
                    let (config, _) = Self::from_args_with_sources()?;
                    std::result::Result::Ok(config)
                }

                /// Load configuration from CLI arguments with source attribution.
                ///
                /// Returns both the config and information about where each value came from.
                pub fn from_args_with_sources() -> std::result::Result<(Self, ::procenv::ConfigSources), ::procenv::Error> {
                    // Build clap Command
                    let __cmd = ::clap::Command::new(env!("CARGO_PKG_NAME"))
                        .version(env!("CARGO_PKG_VERSION"))
                        #(.arg(#clap_args))*;

                    // Parse arguments
                    let __matches = __cmd.get_matches();

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

                    // Error accumulator
                    let mut __errors: std::vec::Vec<::procenv::Error> = std::vec::Vec::new();
                    let mut __sources = ::procenv::ConfigSources::new();

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
    fn generate_cli_aware_loader(field: &dyn FieldGenerator) -> QuoteStream {
        let name = field.name();
        let cli_var = format_ident!("__{}_cli", name);
        let from_cli_var = format_ident!("__{}_from_cli", name);

        // Check if this field has CLI config
        if field.cli_config().is_some() {
            let env_loader = field.generate_loader();

            // For CLI-enabled fields: check CLI first, then env
            let cli_arg_name = format!("--{}", field.cli_config().unwrap().long.as_ref().unwrap());

            quote! {
                let #from_cli_var: bool;
                let #name = if let std::option::Option::Some(ref cli_val) = #cli_var {
                    #from_cli_var = true;
                    // Parse CLI value
                    match cli_val.parse() {
                        std::result::Result::Ok(v) => std::option::Option::Some(v),
                        std::result::Result::Err(e) => {
                            __errors.push(::procenv::Error::parse(
                                #cli_arg_name,
                                cli_val.clone(),
                                false,
                                std::any::type_name_of_val(&e),
                                std::boxed::Box::new(e),
                            ));
                            std::option::Option::None
                        }
                    }
                } else {
                    #from_cli_var = false;
                    // Fall back to env var loading
                    #env_loader
                    #name
                };
            }
        } else {
            // Non-CLI fields: just use normal env loader
            field.generate_loader()
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

    /// Generate the from_env_with_sources() implementation
    fn generate_from_env_with_sources_impl(
        struct_name: &Ident,
        generators: &[Box<dyn FieldGenerator>],
        env_config: &EnvConfigAttr,
    ) -> QuoteStream {
        // Collect all env var names for pre-dotenv check
        let env_var_names: Vec<_> = generators.iter().filter_map(|g| g.env_var_name()).collect();

        // Generate pre-dotenv var collection
        let pre_dotenv_collection = quote! {
            let __pre_dotenv_vars: std::collections::HashSet<&str> = [
                #(#env_var_names),*
            ]
            .iter()
            .filter(|var| std::env::var(var).is_ok())
            .copied()
            .collect();
        };

        // Dotenv loading (same as from_env)
        let dotenv_load = Self::generate_dotenv_load(&env_config.dotenv);

        // Track if dotenv was loaded
        let dotenv_loaded_flag = if env_config.dotenv.is_some() {
            quote! { let __dotenv_loaded = true; }
        } else {
            quote! { let __dotenv_loaded = false; }
        };

        // Generate profile setup code (if configured)
        let profile_setup = Self::generate_profile_setup(env_config);

        // Generate loaders (using format-aware loader for fields with format)
        let loaders: Vec<QuoteStream> = generators
            .iter()
            .map(|g: &Box<dyn FieldGenerator + 'static>| {
                Self::generate_field_loader(g.as_ref(), env_config)
            })
            .collect();

        // Generate source tracking
        let source_tracking: Vec<QuoteStream> = generators
            .iter()
            .map(|g: &Box<dyn FieldGenerator + 'static>| g.generate_source_tracking())
            .collect();

        // Generate assignments
        let assignments: Vec<QuoteStream> = generators
            .iter()
            .map(|g: &Box<dyn FieldGenerator + 'static>| g.generate_assignment())
            .collect();

        // Check if any fields have CLI config
        let has_cli_fields = generators.iter().any(|g| g.cli_config().is_some());

        // Generate from_args() if any CLI fields exist
        let from_args_impl = if has_cli_fields {
            Self::generate_from_args_impl(struct_name, generators, env_config)
        } else {
            quote! {}
        };

        quote! {
            #from_args_impl

            impl #struct_name {
                /// Load configuration from environment with source attribution.
                ///
                /// Returns both the config and information about where each value came from.
                pub fn from_env_with_sources() -> std::result::Result<(Self, ::procenv::ConfigSources), ::procenv::Error> {
                    #pre_dotenv_collection

                    #dotenv_load

                    #dotenv_loaded_flag

                    let mut __errors: std::vec::Vec<::procenv::Error> = std::vec::Vec::new();
                    let mut __sources = ::procenv::ConfigSources::new();

                    // Read and validate profile (if configured)
                    #profile_setup

                    #(#loaders)*

                    #(#source_tracking)*

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

                /// Load configuration and return only the source attribution.
                pub fn sources() -> std::result::Result<::procenv::ConfigSources, ::procenv::Error> {
                    let (_, sources) = Self::from_env_with_sources()?;
                    std::result::Result::Ok(sources)
                }
            }
        }
    }
}
