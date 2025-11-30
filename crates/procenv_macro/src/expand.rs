//! Code generation orchestration for EnvConfig derive macro.
//!
//! This module is the conductor of the macro. The [`Expander`] struct
//! coordinates the code generation process:
//!
//! 1. **Validation** - Ensures input is a struct with named fields
//! 2. **Parsing** - Converts each field into a [`FieldGenerator`](crate::field::FieldGenerator)
//! 3. **Generation** - Produces all impl blocks for the struct
//!
//! # Generated Code
//!
//! The expander generates the following implementations:
//!
//! | Method | Generator Function |
//! |--------|-------------------|
//! | `from_env()` | [`generate_from_env_impl`](Expander::generate_from_env_impl) |
//! | `from_env_with_sources()` | [`generate_from_env_with_sources_impl`](Expander::generate_from_env_with_sources_impl) |
//! | `from_config()` | [`generate_from_config_impl`](Expander::generate_from_config_impl) |
//! | `from_args()` | [`generate_from_args_impl`](Expander::generate_from_args_impl) |
//! | `env_example()` | [`generate_env_example_impl`](Expander::generate_env_example_impl) |
//! | `impl Debug` | [`generate_debug_impl`](Expander::generate_debug_impl) |
//!
//! # Error Accumulation Pattern
//!
//! The generated `from_env()` method uses error accumulation rather than
//! failing on the first error. This provides users a complete picture of
//! all configuration issues at once:
//!
//! ```text
//! ┌─────────────────┐
//! │  __errors: Vec  │  Accumulator for all errors
//! └────────┬────────┘
//!          │
//!    ┌─────┴─────┬─────────────┬─────────────┐
//!    ▼           ▼             ▼             ▼
//! ┌──────┐   ┌──────┐     ┌──────┐     ┌──────┐
//! │Field1│   │Field2│     │Field3│     │FieldN│  Each field loads
//! │loader│   │loader│     │loader│     │loader│  independently
//! └──┬───┘   └──┬───┘     └──┬───┘     └──┬───┘
//!    │          │            │            │
//!    └──────────┴────────────┴────────────┘
//!                      │
//!                      ▼
//! ┌─────────────────────────────────────────────┐
//! │  If __errors.is_empty() → Ok(Self { ... })  │
//! │  Else → Err(Multiple { errors })            │
//! └─────────────────────────────────────────────┘
//! ```
//!
//! # Architecture
//!
//! The expander works with trait objects (`Box<dyn FieldGenerator>`) to
//! generate field-specific code polymorphically. This allows the same
//! expansion logic to handle required, default, optional, and flatten
//! fields uniformly.

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
/// `Expander` is a zero-sized type that serves as a namespace for code
/// generation functions. It coordinates the entire macro expansion process.
///
/// # Entry Point
///
/// The [`expand`](Self::expand) method is the main entry point, called from
/// the derive macro's proc_macro function. It:
///
/// 1. Parses struct-level attributes (`#[env_config(...)]`)
/// 2. Validates the input is a named struct
/// 3. Parses each field's attributes into [`FieldGenerator`](crate::field::FieldGenerator) objects
/// 4. Generates all implementation blocks
/// 5. Returns the combined TokenStream
///
/// # Generator Methods
///
/// Each generator method produces a specific impl block:
///
/// - [`generate_from_env_impl`](Self::generate_from_env_impl) - Core environment loading
/// - [`generate_from_env_with_sources_impl`](Self::generate_from_env_with_sources_impl) - Loading with source attribution
/// - [`generate_from_config_impl`](Self::generate_from_config_impl) - File-based configuration
/// - [`generate_from_args_impl`](Self::generate_from_args_impl) - CLI argument parsing
/// - [`generate_debug_impl`](Self::generate_debug_impl) - Debug with secret masking
/// - [`generate_env_example_impl`](Self::generate_env_example_impl) - `.env.example` generation
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

        // Always generate __config_defaults for nested struct support
        let config_defaults_impl =
            Self::generate_config_defaults_impl(struct_name, generics, &generators);

        // Generate file config method if files are configured
        let file_config_impl = if !env_config_attr.files.is_empty() {
            Self::generate_from_config_impl(struct_name, generics, &generators, &env_config_attr)
        } else {
            quote! {}
        };

        // Generate validation methods if validate attribute is set
        // Requires: #[env_config(validate)] and #[derive(Validate)]
        let validated_impl = if env_config_attr.validate {
            Self::generate_validated_impl(struct_name, generics, &generators, &env_config_attr)
        } else {
            quote! {}
        };

        // Generate external prefix method for flatten support
        let external_prefix_impl =
            Self::generate_from_env_with_external_prefix_impl(struct_name, generics, &generators, &env_config_attr);

        let combined = quote! {
            #from_env_impl
            #debug_impl
            #env_example_impl
            #sources_impl
            #config_defaults_impl
            #file_config_impl
            #validated_impl
            #external_prefix_impl
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
        let assignments: Vec<QuoteStream> =
            fields.iter().map(|f| f.generate_assignment()).collect();

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

                    // Define external prefix as None for regular from_env calls
                    // This is used by flatten fields which call __from_env_with_external_prefix
                    let __external_prefix: std::option::Option<&str> = std::option::Option::None;

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

        // Get env var name and type info for parsing
        let env_var = field.env_var_name().unwrap_or("");
        let ty = field.type_name();
        let secret = field.is_secret();
        let default_value = field.default_value();
        let used_default_ident = format_ident!("__{}_used_default", name);

        // Generate fallback code for when no env var and no profile match
        let no_value_handling = if let Some(default) = default_value {
            // Field has a default - use it
            quote! {
                #used_default_ident = true;
                (std::option::Option::Some(#default.to_string()), false)
            }
        } else {
            // Required field with no default - will error later
            quote! {
                (std::option::Option::None, false)
            }
        };

        // Generate error or None handling for missing value
        let missing_value_handling = if default_value.is_some() {
            // DefaultField: already handled by fallback, shouldn't reach here
            quote! { std::option::Option::None }
        } else {
            // RequiredField: error on missing
            quote! {
                __errors.push(::procenv::Error::missing(#env_var));
                std::option::Option::None
            }
        };

        quote! {
            // Track if we used the compile-time default (for source attribution)
            let mut #used_default_ident = false;

            // Determine profile default value (if profile matches)
            let __profile_default: std::option::Option<&str> = match __profile.as_deref() {
                #(#match_arms)*
                _ => std::option::Option::None,
            };

            // Get value to parse: env var > profile > default
            let (__value_to_parse, #profile_used_ident): (std::option::Option<std::string::String>, bool) =
                match std::env::var(#env_var) {
                    std::result::Result::Ok(val) => {
                        // Env var is set - use it (profile NOT used)
                        (std::option::Option::Some(val), false)
                    }
                    std::result::Result::Err(std::env::VarError::NotPresent) => {
                        // Env var not set - try profile default
                        match __profile_default {
                            std::option::Option::Some(profile_val) => {
                                (std::option::Option::Some(profile_val.to_string()), true)
                            }
                            std::option::Option::None => {
                                // No profile match - fall back to compile-time default or error
                                #no_value_handling
                            }
                        }
                    }
                    std::result::Result::Err(std::env::VarError::NotUnicode(_)) => {
                        __errors.push(::procenv::Error::InvalidUtf8 { var: #env_var.to_string() });
                        (std::option::Option::None, false)
                    }
                };

            // Parse the value (from env, profile, or default)
            let #name = match __value_to_parse {
                std::option::Option::Some(val) => {
                    match val.parse() {
                        std::result::Result::Ok(v) => std::option::Option::Some(v),
                        std::result::Result::Err(e) => {
                            __errors.push(::procenv::Error::parse(
                                #env_var,
                                if #secret { "[REDACTED]".to_string() } else { val },
                                #secret,
                                #ty,
                                std::boxed::Box::new(e),
                            ));
                            std::option::Option::None
                        }
                    }
                }
                std::option::Option::None => {
                    #missing_value_handling
                }
            };
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

    /// Generate the `__config_defaults()` method for nested struct defaults.
    ///
    /// This internal method is used by flatten fields to propagate their
    /// defaults up to the parent struct's `from_config()` method.
    fn generate_config_defaults_impl(
        struct_name: &Ident,
        generics: &Generics,
        generators: &[Box<dyn FieldGenerator>],
    ) -> QuoteStream {
        let (impl_generics, type_generics, where_clause) = generics.split_for_impl();

        // Generate default entries for regular fields
        let default_entries: Vec<QuoteStream> = generators
            .iter()
            .filter_map(|g| {
                // Skip flatten fields - handled separately
                if g.is_flatten() {
                    return None;
                }

                let field_name = g.name().to_string();
                g.default_value().map(|default| {
                    quote! {
                        __map.insert(
                            #field_name.to_string(),
                            ::procenv::FileUtils::coerce_value(#default)
                        );
                    }
                })
            })
            .collect();

        // Generate nested defaults for flatten fields
        let flatten_entries: Vec<QuoteStream> = generators
            .iter()
            .filter_map(|g| {
                if !g.is_flatten() {
                    return None;
                }

                let field_name = g.name().to_string();
                let ty = g.field_type()?;

                Some(quote! {
                    if let ::serde_json::Value::Object(nested) = <#ty>::__config_defaults() {
                        __map.insert(
                            #field_name.to_string(),
                            ::serde_json::Value::Object(nested)
                        );
                    }
                })
            })
            .collect();

        quote! {
            impl #impl_generics #struct_name #type_generics #where_clause {
                /// Returns default values for this config as a JSON object.
                ///
                /// This is an internal method used for nested configs.
                #[doc(hidden)]
                pub fn __config_defaults() -> ::serde_json::Value {
                    let mut __map = ::serde_json::Map::new();
                    #(#default_entries)*
                    #(#flatten_entries)*
                    ::serde_json::Value::Object(__map)
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
        generators: &[Box<dyn FieldGenerator>],
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

        // Generate direct env var mappings for fields with custom var names
        // This handles fields with `var = "CUSTOM_NAME"` and `no_prefix` attributes
        let env_mapping_calls: Vec<QuoteStream> = generators
            .iter()
            .filter_map(|g| {
                // Skip flatten fields - they handle their own env vars
                if g.is_flatten() {
                    return None;
                }

                let env_var = g.env_var_name()?;
                let field_name = g.name().to_string();

                // Generate mapping call for this field
                Some(quote! {
                    builder = builder.env_mapping(#field_name, #env_var);
                })
            })
            .collect();

        let env_mappings = quote! {
            // Register direct env var mappings for fields with custom names
            #(#env_mapping_calls)*
        };

        // Generate dotenv loading (before ConfigBuilder so env vars are available)
        let dotenv_load = Self::generate_dotenv_load(&env_config_attr.dotenv);

        // Collect all env var names for pre-dotenv check (for dotenv provenance tracking)
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

        // Track if dotenv was loaded
        let dotenv_loaded_flag = if env_config_attr.dotenv.is_some() {
            quote! { let __dotenv_loaded = true; }
        } else {
            quote! { let __dotenv_loaded = false; }
        };

        // Generate default values for fields that have them
        // This ensures #[env(default = "...")] works with from_config()
        let default_entries: Vec<QuoteStream> = generators
            .iter()
            .filter_map(|g| {
                // Skip flatten fields - handled separately below
                if g.is_flatten() {
                    return None;
                }

                let field_name = g.name().to_string();
                // Convert field_name to snake_case for JSON key
                let json_key = field_name.clone();

                // Use FileUtils::coerce_value to convert string to proper JSON type
                g.default_value().map(|default| {
                    quote! {
                        __defaults.insert(
                            #json_key.to_string(),
                            ::procenv::FileUtils::coerce_value(#default)
                        );
                    }
                })
            })
            .collect();

        // Generate nested defaults collection for flatten fields
        let flatten_default_entries: Vec<QuoteStream> = generators
            .iter()
            .filter_map(|g| {
                if !g.is_flatten() {
                    return None;
                }

                let field_name = g.name().to_string();
                let ty = g.field_type()?;

                // Merge nested type's defaults under this field's name
                Some(quote! {
                    if let ::serde_json::Value::Object(nested_map) = <#ty>::__config_defaults() {
                        __defaults.insert(
                            #field_name.to_string(),
                            ::serde_json::Value::Object(nested_map)
                        );
                    }
                })
            })
            .collect();

        // Determine if we need defaults setup
        let has_flatten = generators.iter().any(|g| g.is_flatten());
        let defaults_setup = if default_entries.is_empty() && !has_flatten {
            quote! {}
        } else {
            quote! {
                // Build defaults from #[env(default = "...")] attributes
                let mut __defaults = ::serde_json::Map::new();
                #(#default_entries)*
                #(#flatten_default_entries)*
                builder = builder.defaults_value(::serde_json::Value::Object(__defaults));
            }
        };

        // Generate source tracking entries for from_config_with_sources
        // Now with dotenv provenance tracking and nested field support
        let source_entries: Vec<QuoteStream> = generators
            .iter()
            .map(|g| {
                let field_name = g.name().to_string();
                let has_default = g.default_value().is_some();

                if g.is_flatten() {
                    // For flatten fields, iterate through tracked origins with this prefix
                    // to get individual nested field sources
                    quote! {
                        {
                            let prefix = concat!(#field_name, ".");
                            for tracked_path in __origins.tracked_fields() {
                                if tracked_path.starts_with(prefix) || tracked_path == #field_name {
                                    // Extract the nested field name (e.g., "database.port" -> "port")
                                    let nested_name = if tracked_path == #field_name {
                                        tracked_path.to_string()
                                    } else {
                                        tracked_path.to_string()
                                    };

                                    let source = if let Some(file_path) = __origins.get_file_source(tracked_path) {
                                        ::procenv::Source::ConfigFile(Some(file_path))
                                    } else {
                                        ::procenv::Source::NotSet
                                    };

                                    __sources.add(
                                        nested_name,
                                        ::procenv::ValueSource::new(tracked_path, source)
                                    );
                                }
                            }
                        }
                    }
                } else {
                    let env_var = g.env_var_name().unwrap_or("");

                    // Determine source based on: env var set > file origin > default
                    // Now with dotenv provenance tracking
                    quote! {
                        {
                            let source = if std::env::var(#env_var).is_ok() {
                                // Check if this was from dotenv or real environment
                                if __dotenv_loaded && !__pre_dotenv_vars.contains(#env_var) {
                                    ::procenv::Source::DotenvFile(None)
                                } else {
                                    ::procenv::Source::Environment
                                }
                            } else if let Some(file_path) = __origins.get_file_source(#field_name) {
                                ::procenv::Source::ConfigFile(Some(file_path))
                            } else if #has_default {
                                ::procenv::Source::Default
                            } else {
                                ::procenv::Source::NotSet
                            };
                            __sources.add(
                                #field_name,
                                ::procenv::ValueSource::new(#env_var, source)
                            );
                        }
                    }
                }
            })
            .collect();

        quote! {
            impl #impl_generics #struct_name #type_generics #where_clause
            where
                Self: ::serde::de::DeserializeOwned,
            {
                /// Load configuration from files and environment variables.
                ///
                /// This method loads configuration using the layered approach:
                /// 1. Macro defaults (from `#[env(default = "...")]`)
                /// 2. Config files (in order specified, later files override)
                /// 3. Environment variables (highest priority)
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

                    // Apply macro-level defaults
                    #defaults_setup

                    // Add config files
                    #(#file_loads)*

                    // Set env prefix for env var overlay
                    #env_prefix

                    // Register direct env var mappings (for custom var names/no_prefix)
                    #env_mappings

                    // Build and deserialize
                    builder.build()
                }

                /// Load configuration from files and environment variables with source attribution.
                ///
                /// This method works like `from_config()` but also returns information
                /// about where each configuration value came from.
                ///
                /// # Returns
                ///
                /// A tuple of:
                /// - The configuration struct
                /// - `ConfigSources` indicating the origin of each field
                ///
                /// # Example
                ///
                /// ```ignore
                /// let (config, sources) = Config::from_config_with_sources()?;
                /// println!("{}", sources);  // Shows where each value came from
                /// ```
                pub fn from_config_with_sources() -> std::result::Result<(Self, ::procenv::ConfigSources), ::procenv::Error> {
                    // Track pre-dotenv env vars for dotenv provenance attribution
                    #pre_dotenv_collection

                    // Load .env file(s) first so env vars are available
                    #dotenv_load

                    // Track if dotenv was loaded
                    #dotenv_loaded_flag

                    let mut builder = ::procenv::ConfigBuilder::new();

                    // Apply macro-level defaults
                    #defaults_setup

                    // Add config files
                    #(#file_loads)*

                    // Set env prefix for env var overlay
                    #env_prefix

                    // Register direct env var mappings (for custom var names/no_prefix)
                    #env_mappings

                    // Build and deserialize with origin tracking
                    let (__config, __origins) = builder.build_with_origins()?;

                    // Build source attribution
                    let mut __sources = ::procenv::ConfigSources::new();
                    #(#source_entries)*

                    std::result::Result::Ok((__config, __sources))
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

                /// Load configuration from a custom argument iterator.
                ///
                /// This is useful for testing or when you want to parse arguments
                /// from a source other than `std::env::args()`.
                ///
                /// Priority: CLI arguments > Environment variables > .env files > Defaults
                ///
                /// # Errors
                ///
                /// Returns an error if required values are missing, fail to parse,
                /// or if CLI argument parsing fails.
                pub fn from_args_from<I, T>(args: I) -> std::result::Result<Self, ::procenv::Error>
                where
                    I: IntoIterator<Item = T>,
                    T: Into<std::ffi::OsString> + Clone,
                {
                    let (config, _) = Self::from_args_from_with_sources(args)?;
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

                    // Parse arguments from std::env::args()
                    let __matches = __cmd.get_matches();

                    Self::__from_args_matches(__matches)
                }

                /// Load configuration from a custom argument iterator with source attribution.
                ///
                /// This is useful for testing or when you want to parse arguments
                /// from a source other than `std::env::args()`.
                ///
                /// Returns both the config and information about where each value came from.
                pub fn from_args_from_with_sources<I, T>(args: I) -> std::result::Result<(Self, ::procenv::ConfigSources), ::procenv::Error>
                where
                    I: IntoIterator<Item = T>,
                    T: Into<std::ffi::OsString> + Clone,
                {
                    // Build clap Command
                    let __cmd = ::clap::Command::new(env!("CARGO_PKG_NAME"))
                        .version(env!("CARGO_PKG_VERSION"))
                        #(.arg(#clap_args))*;

                    // Parse arguments from iterator
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
            // Use format-aware env loader if format is specified
            let env_loader = if let Some(format) = field.format_config() {
                field.generate_format_loader(format)
            } else {
                field.generate_loader()
            };

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
                    // Parse CLI value (using format-aware parsing if configured)
                    match #parse_expr {
                        std::result::Result::Ok(v) => std::option::Option::Some(v),
                        std::result::Result::Err(e) => {
                            __errors.push(::procenv::Error::parse(
                                #cli_arg_name,
                                if #secret {
                                    "[REDACTED]".to_string()
                                } else {
                                    cli_val.clone()
                                },
                                #secret,
                                #format_name,
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
            // Non-CLI fields: use format-aware loader if format is specified
            if let Some(format) = field.format_config() {
                field.generate_format_loader(format)
            } else {
                field.generate_loader()
            }
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
            .map(|g| Self::generate_field_loader(g.as_ref(), env_config))
            .collect();

        // Generate source tracking
        let source_tracking: Vec<QuoteStream> = generators
            .iter()
            .map(|g| g.generate_source_tracking())
            .collect();

        // Generate assignments
        let assignments: Vec<QuoteStream> =
            generators.iter().map(|g| g.generate_assignment()).collect();

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

                    // Define external prefix as None for regular from_env calls
                    // This is used by flatten fields which call __from_env_with_external_prefix
                    let __external_prefix: std::option::Option<&str> = std::option::Option::None;

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

    /// Generate the `from_env_validated` method for structs with validation.
    ///
    /// This generates a method that:
    /// 1. Loads all config values using `from_env()`
    /// 2. Runs validator crate validation
    /// 3. Runs any custom field validators
    /// 4. Accumulates all validation errors
    ///
    /// Only generated when `#[env_config(validate)]` is set.
    /// Requires the struct to also derive `validator::Validate`.
    fn generate_validated_impl(
        struct_name: &Ident,
        generics: &Generics,
        generators: &[Box<dyn FieldGenerator>],
        _env_config_attr: &EnvConfigAttr,
    ) -> QuoteStream {
        let (impl_generics, type_generics, where_clause) = generics.split_for_impl();

        // Generate custom field validation calls
        let custom_validations: Vec<QuoteStream> = generators
            .iter()
            .filter_map(|g| {
                let validate_fn = g.validate_fn()?;
                let field_name = g.name();
                let field_name_str = field_name.to_string();
                let fn_ident = format_ident!("{}", validate_fn);

                Some(quote! {
                    if let Err(e) = #fn_ident(&__config.#field_name) {
                        __validation_errors.push(::procenv::ValidationFieldError::new(
                            #field_name_str,
                            "custom",
                            e.message
                                .as_ref()
                                .map(|m| m.to_string())
                                .unwrap_or_else(|| e.code.to_string()),
                        ));
                    }
                })
            })
            .collect();

        let _has_custom_validations = !custom_validations.is_empty();

        // Main validated impl
        quote! {
            #[cfg(feature = "validator")]
            impl #impl_generics #struct_name #type_generics #where_clause
            where
                Self: ::validator::Validate,
            {
                /// Load configuration from environment variables with validation.
                ///
                /// This method:
                /// 1. Loads all configuration values using `from_env()`
                /// 2. Runs `validator` crate validation on the loaded struct
                /// 3. Runs any custom field validators specified with `#[env(validate = "...")]`
                /// 4. Returns all validation errors accumulated together
                ///
                /// # Errors
                ///
                /// Returns an error if:
                /// - Any required variables are missing
                /// - Any values fail to parse
                /// - Any validation rules fail
                ///
                /// # Example
                ///
                /// ```ignore
                /// use procenv::EnvConfig;
                /// use validator::Validate;
                ///
                /// #[derive(EnvConfig, Validate)]
                /// struct Config {
                ///     #[env(var = "PORT", default = "8080")]
                ///     #[validate(range(min = 1, max = 65535))]
                ///     port: u16,
                /// }
                ///
                /// let config = Config::from_env_validated()?;
                /// ```
                pub fn from_env_validated() -> std::result::Result<Self, ::procenv::Error> {
                    // First, load config (may return Missing/Parse errors)
                    let __config = Self::from_env()?;

                    // Collect validation errors
                    let mut __validation_errors: Vec<::procenv::ValidationFieldError> = Vec::new();

                    // Run validator crate validation
                    if let Err(e) = ::validator::Validate::validate(&__config) {
                        __validation_errors.extend(::procenv::validation_errors_to_procenv(e));
                    }

                    // Run custom field validations
                    #(#custom_validations)*

                    // Return validation errors if any
                    if !__validation_errors.is_empty() {
                        return Err(::procenv::Error::Validation {
                            errors: __validation_errors,
                        });
                    }

                    Ok(__config)
                }

                pub fn from_env_validated_with_sources() -> std::result::Result<(Self, ::procenv::ConfigSources), ::procenv::Error> {
                    // First, load config with sources
                    let (__config, __sources) = Self::from_env_with_sources()?;

                    // Collect validation errors
                    let mut __validation_errors: Vec<::procenv::ValidationFieldError> = Vec::new();

                    // Run validator crate validation
                    if let Err(e) = ::validator::Validate::validate(&__config) {
                        __validation_errors.extend(::procenv::validation_errors_to_procenv(e));
                    }

                    // Run custom field validations
                    #(#custom_validations)*

                    // Return validation errors if any
                    if !__validation_errors.is_empty() {
                        return Err(::procenv::Error::Validation {
                            errors: __validation_errors,
                        });
                    }

                    Ok((__config, __sources))
                }
            }
        }
    }

    /// Generate validated file config loading.
    ///
    /// Note: This function is not currently used but will be enabled in Phase B
    /// when validation support is fully implemented.
    #[allow(dead_code)]
    fn generate_from_config_validated_impl(
        struct_name: &Ident,
        generics: &Generics,
        generators: &[Box<dyn FieldGenerator>],
    ) -> QuoteStream {
        let (impl_generics, type_generics, where_clause) = generics.split_for_impl();

        // Generate custom field validation calls (same as above)
        let custom_validations: Vec<QuoteStream> = generators
            .iter()
            .filter_map(|g| {
                let validate_fn = g.validate_fn()?;
                let field_name = g.name();
                let field_name_str = field_name.to_string();
                let fn_ident = format_ident!("{validate_fn}");

                Some(quote! {
                    if let Err(error) = #fn_ident(&__config.#field_name) {
                        __validation_errors.push(::procenv::ValidationFieldError::new(
                            #field_name_str,
                            "custom",
                            error
                                .message
                                .as_ref()
                                .map(|m| m.to_string())
                                .unwrap_or_else(|| error.code.to_string()),
                        ));
                    }
                })
            })
            .collect();

        quote! {
            #[cfg(all(feature = "validator", feature = "file"))]
            impl #impl_generics #struct_name #type_generics #where_clause
            where
                Self: ::validator::Validate + ::serde::de::DeserializeOwned,
            {
                /// Load configuration from files with validation
                ///
                /// This method combines `from_config()` with validation.
                pub fn from_config_validated() -> std::result::Result<Self, ::procenv::Error> {
                    let __config = Self::from_config()?;

                    let mut __validation_errors: Vec<::procenv::ValidationFieldError> = Vec::new();

                    if let Err(e) = ::validator::Validate::validate(&__config) {
                        __validation_errors.extend(::procenv::validation_errors_to_procenv(e));
                    }

                    #(#custom_validations)*

                    if !__validation_errors.is_empty() {
                        return Err(
                            ::procenv::Error::Validation {
                                errors: __validation_errors,
                            }
                        );
                    }

                    Ok(__config)
                }

                /// Load configuration from files with validation and source attribution.
                pub fn from_config_validated_with_sources() -> std::result::Result<(Self, ::procenv::ConfigSources), ::procenv::Error> {
                    let (__config, __sources) = Self::from_config_with_sources()?;

                    let mut __validation_errors: Vec<::procenv::ValidationFieldError> = Vec::new();

                    if let Err(e) = ::validator::Validate::validate(&__config) {
                        __validation_errors.extend(::procenv::validation_errors_to_procenv(e));
                    }

                    #(#custom_validations)*

                    if !__validation_errors.is_empty() {
                        return Err(
                            ::procenv::Error::Validation {
                                errors: __validation_errors,
                            }
                        );
                    }

                    Ok((__config, __sources))
                }
            }
        }
    }

    /// Generate the `__from_env_with_external_prefix` method.
    ///
    /// This internal method allows loading a struct with an external prefix
    /// prepended to all env var names. Used by flatten fields to pass prefixes
    /// to nested structs.
    ///
    /// # Generated Code
    ///
    /// ```ignore
    /// impl Config {
    ///     #[doc(hidden)]
    ///     pub fn __from_env_with_external_prefix(
    ///         __external_prefix: Option<&str>
    ///     ) -> Result<(Self, ConfigSources), Error> {
    ///         // Uses prefixed env var names for loading
    ///     }
    /// }
    /// ```
    fn generate_from_env_with_external_prefix_impl(
        struct_name: &Ident,
        generics: &Generics,
        generators: &[Box<dyn FieldGenerator>],
        env_config: &EnvConfigAttr,
    ) -> QuoteStream {
        let (impl_generics, type_generics, where_clause) = generics.split_for_impl();

        // Collect all env var names for pre-dotenv check
        let env_var_names: Vec<_> = generators.iter().filter_map(|g| g.env_var_name()).collect();

        // Generate loaders using the prefixed version
        // Note: Fields with format config need special handling (serde deserialization)
        let loaders: Vec<QuoteStream> = generators
            .iter()
            .map(|g| {
                // Use format-aware loader if field has format config
                if let Some(format) = g.format_config() {
                    g.generate_format_loader(format)
                } else {
                    g.generate_loader_with_external_prefix()
                }
            })
            .collect();

        // Generate simplified source tracking that doesn't depend on profile tracking vars
        // (profile support for nested prefixed structs would require additional work)
        let source_tracking: Vec<QuoteStream> = generators
            .iter()
            .map(|g| Self::generate_simple_source_tracking(g.as_ref()))
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

        // Profile setup - note: profiles use fixed env var names (not prefixed)
        let profile_setup = Self::generate_profile_setup(env_config);

        quote! {
            impl #impl_generics #struct_name #type_generics #where_clause {
                /// Load configuration with an external prefix prepended to env var names.
                ///
                /// This is an internal method used by flatten fields to pass prefixes
                /// to nested structs. The prefix is prepended to all env var names
                /// at runtime.
                ///
                /// # Arguments
                ///
                /// * `__external_prefix` - Optional prefix to prepend to all env var names
                #[doc(hidden)]
                pub fn __from_env_with_external_prefix(
                    __external_prefix: std::option::Option<&str>
                ) -> std::result::Result<(Self, ::procenv::ConfigSources), ::procenv::Error> {
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

                    // Read and validate profile (if configured)
                    #profile_setup

                    // Load each field with prefixed env var names
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

    /// Generate simplified source tracking for __from_env_with_external_prefix.
    ///
    /// This version doesn't depend on profile tracking variables because
    /// the external prefix method uses simple loaders that don't track profiles.
    fn generate_simple_source_tracking(field: &dyn FieldGenerator) -> QuoteStream {
        let name = field.name();
        let name_str = name.to_string();

        // For flatten fields, use the nested sources
        if field.is_flatten() {
            let nested_sources_ident = format_ident!("__{}_nested_sources", name);
            return quote! {
                __sources.extend_nested(#name_str, #nested_sources_ident);
            };
        }

        // For regular fields
        if let Some(env_var) = field.env_var_name() {
            let source_ident = format_ident!("__{}_source", name);
            let has_default = field.default_value().is_some();

            quote! {
                let #source_ident = if std::env::var(#env_var).is_ok() {
                    if __dotenv_loaded && !__pre_dotenv_vars.contains(#env_var) {
                        ::procenv::ValueSource::new(#env_var, ::procenv::Source::DotenvFile(None))
                    } else {
                        ::procenv::ValueSource::new(#env_var, ::procenv::Source::Environment)
                    }
                } else if #has_default {
                    ::procenv::ValueSource::new(#env_var, ::procenv::Source::Default)
                } else {
                    ::procenv::ValueSource::new(#env_var, ::procenv::Source::NotSet)
                };
                __sources.add(#name_str, #source_ident);
            }
        } else {
            quote! {}
        }
    }
}
