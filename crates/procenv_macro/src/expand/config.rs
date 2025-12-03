//! File-based configuration code generation.
//!
//! This module generates the `from_config()` method for loading configuration
//! from files (TOML, JSON, YAML) with environment variable overlay.
//!
//! # Generated Methods
//!
//! - [`generate_from_config_impl`] - Main `from_config()` and `from_config_with_sources()`
//! - [`generate_config_defaults_impl`] - Internal `__config_defaults()` for nested structs
//!
//! # Layering Order
//!
//! Configuration is loaded in this priority order (lowest to highest):
//!
//! 1. **Macro defaults** - `#[env(default = "...")]` attributes
//! 2. **Config files** - In order specified (later files override earlier)
//! 3. **Environment variables** - Highest priority
//!
//! # Generated Code Pattern
//!
//! ```rust,ignore
//! pub fn from_config() -> Result<Self, Error> {
//!     // Load dotenv first
//!     let _ = dotenvy::dotenv();
//!
//!     let mut builder = ConfigBuilder::new();
//!
//!     // Apply defaults from macro attributes
//!     builder = builder.defaults_value(json!({ "port": 8080 }));
//!
//!     // Add config files
//!     builder = builder.file_optional("config.toml");
//!
//!     // Set env prefix
//!     builder = builder.env_prefix("APP_");
//!
//!     builder.build()
//! }
//! ```
//!
//! # Nested Struct Support
//!
//! The `__config_defaults()` method is generated for all structs to support
//! flatten fields. It returns a JSON object with default values that can
//! be merged into the parent's defaults.

use std::string::String;

use proc_macro2::TokenStream as QuoteStream;
use quote::quote;
use syn::{Generics, Ident};

use crate::field::FieldGenerator;
use crate::parse::EnvConfigAttr;

use super::env::generate_dotenv_load;

/// Generate the `from_config()` method for file-based configuration loading.
///
/// This generates self-contained deserialization - no serde derive required.
#[expect(
    clippy::too_many_lines,
    reason = "proc-macro code generation inherently requires verbose quote! blocks"
)]
pub fn generate_from_config_impl(
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
    let env_mapping_calls: Vec<QuoteStream> = generators
        .iter()
        .filter_map(|g| {
            if g.is_flatten() {
                return None;
            }

            let env_var = g.env_var_name()?;
            let field_name = g.name().to_string();

            Some(quote! {
                builder = builder.env_mapping(#field_name, #env_var);
            })
        })
        .collect();

    let env_mappings = quote! {
        #(#env_mapping_calls)*
    };

    // Generate dotenv loading
    let dotenv_load = generate_dotenv_load(env_config_attr.dotenv.as_ref());

    // Generate profile setup for from_config
    let (profile_setup, profile_defaults) =
        generate_profile_defaults_for_config(env_config_attr, generators);

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

    // Track if dotenv was loaded
    let dotenv_loaded_flag = if env_config_attr.dotenv.is_some() {
        quote! { let __dotenv_loaded = true; }
    } else {
        quote! { let __dotenv_loaded = false; }
    };

    // Generate default values for fields that have them
    let default_entries: Vec<QuoteStream> = generators
        .iter()
        .filter_map(|g| {
            if g.is_flatten() {
                return None;
            }

            let field_name = g.name().to_string();
            let json_key = field_name.clone();

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
    let has_profile = env_config_attr.profile_env.is_some();
    let defaults_setup = if default_entries.is_empty() && !has_flatten && !has_profile {
        quote! {}
    } else {
        quote! {
            let mut __defaults = ::serde_json::Map::new();
            // Apply macro defaults first (lowest priority)
            #(#default_entries)*
            #(#flatten_default_entries)*
            // Apply profile defaults (override macro defaults)
            #profile_defaults
            builder = builder.defaults_value(::serde_json::Value::Object(__defaults));
        }
    };

    // Generate source tracking entries
    let source_entries: Vec<QuoteStream> = generators
        .iter()
        .map(|g| {
            let field_name = g.name().to_string();
            let has_default = g.default_value().is_some();

            if g.is_flatten() {
                quote! {
                    {
                        let prefix = concat!(#field_name, ".");
                        for tracked_path in __origins.tracked_fields() {
                            if tracked_path.starts_with(prefix) || tracked_path == #field_name {
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

                quote! {
                    {
                        let source = if std::env::var(#env_var).is_ok() {
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
        impl #impl_generics #struct_name #type_generics #where_clause {
            /// Load configuration from files and environment variables.
            pub fn from_config() -> std::result::Result<Self, ::procenv::Error> {
                #dotenv_load

                #profile_setup

                let mut builder = ::procenv::ConfigBuilder::new();

                #defaults_setup

                #(#file_loads)*

                #env_prefix

                #env_mappings

                let (__value, __origins) = builder.into_value()?;
                Self::__from_json_value(__value)
            }

            /// Load configuration from files and environment variables with source attribution.
            pub fn from_config_with_sources() -> std::result::Result<(Self, ::procenv::ConfigSources), ::procenv::Error> {
                #pre_dotenv_collection

                #dotenv_load

                #dotenv_loaded_flag

                #profile_setup

                let mut builder = ::procenv::ConfigBuilder::new();

                #defaults_setup

                #(#file_loads)*

                #env_prefix

                #env_mappings

                let (__value, __origins) = builder.into_value()?;
                let __config = Self::__from_json_value(__value)?;

                let mut __sources = ::procenv::ConfigSources::new();
                #(#source_entries)*

                std::result::Result::Ok((__config, __sources))
            }
        }
    }
}

/// Generate field extraction code for `__from_json_value`.
///
/// IMPORTANT: This function requires access to the field's actual type for proper
/// code generation. The `FieldGenerator` trait needs a `field_type()` method that
/// returns the type for ALL field kinds (not just flatten).
#[expect(clippy::too_many_lines, reason = "Complex proc-macro logic")]
fn generate_field_extractions(generators: &[Box<dyn FieldGenerator>]) -> QuoteStream {
    let extractions: Vec<QuoteStream> = generators
        .iter()
        .map(|g| {
            let name = g.name();
            let field_name_str = name.to_string();
            let local_var = quote::format_ident!("__{}", name);

            if g.is_flatten() {
                // Flatten field: extract nested object and call nested type's __from_json_value
                let ty = g.field_type().expect("flatten field must have type");
                quote! {
                    let #local_var: std::option::Option<#ty> = {
                        let nested_value = __obj.get(#field_name_str)
                            .cloned()
                            .unwrap_or(::serde_json::Value::Object(::serde_json::Map::new()));
                        match <#ty>::__from_json_value(nested_value) {
                            std::result::Result::Ok(v) => std::option::Option::Some(v),
                            std::result::Result::Err(e) => {
                                __errors.push(e);
                                std::option::Option::None
                            }
                        }
                    };
                }
            } else if g.is_optional() {
                // Optional field: None if missing
                // Note: For optional fields, field_type() returns the INNER type (T from Option<T>)
                let inner_ty = g.field_type().expect("optional field must have inner type");
                let type_name = g.type_name();

                if g.format_config().is_some() {
                    // Optional with serde format
                    quote! {
                        let #local_var: std::option::Option<std::option::Option<#inner_ty>> = match __obj.get(#field_name_str) {
                            std::option::Option::Some(v) if !v.is_null() => {
                                match ::serde_json::from_value::<#inner_ty>(v.clone()) {
                                    std::result::Result::Ok(parsed) => std::option::Option::Some(std::option::Option::Some(parsed)),
                                    std::result::Result::Err(e) => {
                                        __errors.push(::procenv::Error::extraction(
                                            #field_name_str,
                                            #type_name,
                                            e.to_string()
                                        ));
                                        std::option::Option::None
                                    }
                                }
                            }
                            _ => std::option::Option::Some(std::option::Option::None),
                        };
                    }
                } else {
                    // Optional with FromStr
                    quote! {
                        let #local_var: std::option::Option<std::option::Option<#inner_ty>> = match __obj.get(#field_name_str) {
                            std::option::Option::Some(v) if !v.is_null() => {
                                let cv = ::procenv::ConfigValue::from_json(v.clone());
                                match cv.extract::<#inner_ty>(#field_name_str) {
                                    std::result::Result::Ok(parsed) => std::option::Option::Some(std::option::Option::Some(parsed)),
                                    std::result::Result::Err(e) => {
                                        __errors.push(::procenv::Error::extraction(
                                            #field_name_str,
                                            #type_name,
                                            e.to_string()
                                        ));
                                        std::option::Option::None
                                    }
                                }
                            }
                            _ => std::option::Option::Some(std::option::Option::None),
                        };
                    }
                }
            } else if g.is_secrecy_type() && g.field_type().is_none() {
                // SecretString field - special handling since it doesn't store a Type
                quote! {
                    let #local_var: std::option::Option<::procenv::SecretString> = match __obj.get(#field_name_str) {
                        std::option::Option::Some(v) if !v.is_null() => {
                            match v.as_str() {
                                std::option::Option::Some(s) => {
                                    std::option::Option::Some(::procenv::SecretString::from(s.to_string()))
                                }
                                std::option::Option::None => {
                                    __errors.push(::procenv::Error::extraction(
                                        #field_name_str,
                                        "SecretString",
                                        "expected string value"
                                    ));
                                    std::option::Option::None
                                }
                            }
                        }
                        _ => {
                            __errors.push(::procenv::Error::missing(#field_name_str));
                            std::option::Option::None
                        }
                    };
                }
            } else if g.is_secrecy_type() && g.field_type().is_some() {
                // SecretBox<T> field - parse inner type and wrap in SecretBox
                let inner_ty = g.field_type().expect("SecretBox field must have inner type");
                let type_name = g.type_name();

                quote! {
                    let #local_var: std::option::Option<::procenv::SecretBox<#inner_ty>> = match __obj.get(#field_name_str) {
                        std::option::Option::Some(v) if !v.is_null() => {
                            let cv = ::procenv::ConfigValue::from_json(v.clone());
                            match cv.extract::<#inner_ty>(#field_name_str) {
                                std::result::Result::Ok(parsed) => {
                                    std::option::Option::Some(::procenv::SecretBox::init_with(|| parsed))
                                }
                                std::result::Result::Err(e) => {
                                    __errors.push(::procenv::Error::extraction(
                                        #field_name_str,
                                        #type_name,
                                        e.to_string()
                                    ));
                                    std::option::Option::None
                                }
                            }
                        }
                        _ => {
                            __errors.push(::procenv::Error::missing(#field_name_str));
                            std::option::Option::None
                        }
                    };
                }
            } else if g.format_config().is_some() {
                // Field with format = "json/yaml/toml" - use serde deserialization
                let ty = g.field_type().expect("format field must have type");
                let type_name = g.type_name();

                if let Some(default) = g.default_value() {
                    // Default field with serde format
                    quote! {
                        let #local_var: std::option::Option<#ty> = match __obj.get(#field_name_str) {
                            std::option::Option::Some(v) if !v.is_null() => {
                                match ::serde_json::from_value::<#ty>(v.clone()) {
                                    std::result::Result::Ok(parsed) => std::option::Option::Some(parsed),
                                    std::result::Result::Err(e) => {
                                        __errors.push(::procenv::Error::extraction(
                                            #field_name_str,
                                            #type_name,
                                            e.to_string()
                                        ));
                                        std::option::Option::None
                                    }
                                }
                            }
                            _ => {
                                // Use default value - parse JSON string
                                match ::serde_json::from_str::<#ty>(#default) {
                                    std::result::Result::Ok(v) => std::option::Option::Some(v),
                                    std::result::Result::Err(e) => {
                                        __errors.push(::procenv::Error::extraction(
                                            #field_name_str,
                                            #type_name,
                                            format!("failed to parse default: {}", e)
                                        ));
                                        std::option::Option::None
                                    }
                                }
                            }
                        };
                    }
                } else {
                    // Required field with serde format
                    quote! {
                        let #local_var: std::option::Option<#ty> = match __obj.get(#field_name_str) {
                            std::option::Option::Some(v) if !v.is_null() => {
                                match ::serde_json::from_value::<#ty>(v.clone()) {
                                    std::result::Result::Ok(parsed) => std::option::Option::Some(parsed),
                                    std::result::Result::Err(e) => {
                                        __errors.push(::procenv::Error::extraction(
                                            #field_name_str,
                                            #type_name,
                                            e.to_string()
                                        ));
                                        std::option::Option::None
                                    }
                                }
                            }
                            _ => {
                                __errors.push(::procenv::Error::missing(#field_name_str));
                                std::option::Option::None
                            }
                        };
                    }
                }
            } else {
                // Required or Default field (using FromStr)
                let ty = g.field_type().expect("field must have type");
                let type_name = g.type_name();

                if let Some(default) = g.default_value() {
                    // Default field: use default if missing
                    quote! {
                        let #local_var: std::option::Option<#ty> = match __obj.get(#field_name_str) {
                            std::option::Option::Some(v) if !v.is_null() => {
                                let cv = ::procenv::ConfigValue::from_json(v.clone());
                                match cv.extract::<#ty>(#field_name_str) {
                                    std::result::Result::Ok(parsed) => std::option::Option::Some(parsed),
                                    std::result::Result::Err(e) => {
                                        __errors.push(::procenv::Error::extraction(
                                            #field_name_str,
                                            #type_name,
                                            e.to_string()
                                        ));
                                        std::option::Option::None
                                    }
                                }
                            }
                            _ => {
                                // Use default value
                                match #default.parse::<#ty>() {
                                    std::result::Result::Ok(v) => std::option::Option::Some(v),
                                    std::result::Result::Err(e) => {
                                        __errors.push(::procenv::Error::extraction(
                                            #field_name_str,
                                            #type_name,
                                            format!("failed to parse default: {}", e)
                                        ));
                                        std::option::Option::None
                                    }
                                }
                            }
                        };
                    }
                } else {
                    // Required field: error if missing
                    quote! {
                        let #local_var: std::option::Option<#ty> = match __obj.get(#field_name_str) {
                            std::option::Option::Some(v) if !v.is_null() => {
                                let cv = ::procenv::ConfigValue::from_json(v.clone());
                                match cv.extract::<#ty>(#field_name_str) {
                                    std::result::Result::Ok(parsed) => std::option::Option::Some(parsed),
                                    std::result::Result::Err(e) => {
                                        __errors.push(::procenv::Error::extraction(
                                            #field_name_str,
                                            #type_name,
                                            e.to_string()
                                        ));
                                        std::option::Option::None
                                    }
                                }
                            }
                            _ => {
                                __errors.push(::procenv::Error::missing(#field_name_str));
                                std::option::Option::None
                            }
                        };
                    }
                }
            }
        })
        .collect();

    quote! { #(#extractions)* }
}

/// Generate field assignment expressions for struct construction.
fn generate_field_assignments_from_json(generators: &[Box<dyn FieldGenerator>]) -> QuoteStream {
    let assignments: Vec<QuoteStream> = generators
        .iter()
        .map(|g| {
            let name = g.name();
            let local_var = quote::format_ident!("__{}", name);

            if g.is_optional() {
                // Optional fields are Option<Option<T>> during extraction
                // Flatten to Option<T>
                quote! { #name: #local_var.flatten(), }
            } else {
                // Required/default/flatten fields use .unwrap()
                // Safe because we checked for errors above
                quote! { #name: #local_var.unwrap(), }
            }
        })
        .collect();

    quote! { #(#assignments)* }
}

/// Generate profile setup code and profile defaults for `from_config()`.
fn generate_profile_defaults_for_config(
    env_config_attr: &EnvConfigAttr,
    generators: &[Box<dyn FieldGenerator>],
) -> (QuoteStream, QuoteStream) {
    let Some(profile_env) = &env_config_attr.profile_env else {
        // No profile configured - return empty setup and defaults
        return (
            quote! {
                let __profile: std::option::Option<std::string::String> = std::option::Option::None;
            },
            quote! {},
        );
    };

    // Generate profile validation if profiles list is provided
    let validation = if let Some(profiles) = &env_config_attr.profiles {
        let profile_strs: Vec<&str> = profiles.iter().map(String::as_str).collect();
        quote! {
            // Validate profile against allowed list
            if let std::option::Option::Some(ref p) = __profile {
                let valid_profiles: &[&str] = &[#(#profile_strs),*];
                if !valid_profiles.contains(&p.as_str()) {
                    return std::result::Result::Err(::procenv::Error::invalid_profile(
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

    let profile_setup = quote! {
        // Read profile from environment variable
        let __profile: std::option::Option<std::string::String> = std::env::var(#profile_env).ok();
        #validation
    };

    // Generate profile default entries for fields that have profile config
    let profile_default_entries: Vec<QuoteStream> = generators
        .iter()
        .filter_map(|g| {
            if g.is_flatten() {
                return None;
            }

            let profile_config = g.profile_config()?;
            let field_name = g.name().to_string();

            // Generate match arms for each profile value
            let match_arms: Vec<QuoteStream> = profile_config
                .values
                .iter()
                .map(|(profile_name, value)| {
                    quote! {
                        std::option::Option::Some(#profile_name) => {
                            __defaults.insert(
                                #field_name.to_string(),
                                ::procenv::FileUtils::coerce_value(#value)
                            );
                        }
                    }
                })
                .collect();

            Some(quote! {
                match __profile.as_deref() {
                    #(#match_arms)*
                    _ => {}
                }
            })
        })
        .collect();

    let profile_defaults = quote! {
        #(#profile_default_entries)*
    };

    (profile_setup, profile_defaults)
}

/// Generate the `__config_defaults()` method for nested struct defaults.
pub fn generate_config_defaults_impl(
    struct_name: &Ident,
    generics: &Generics,
    generators: &[Box<dyn FieldGenerator>],
) -> QuoteStream {
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();

    // Generate default entries for regular fields
    let default_entries: Vec<QuoteStream> = generators
        .iter()
        .filter_map(|g| {
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
                if let ::procenv::file::JsonValue::Object(nested) = <#ty>::__config_defaults() {
                    __map.insert(
                        #field_name.to_string(),
                        ::procenv::file::JsonValue::Object(nested)
                    );
                }
            })
        })
        .collect();

    quote! {
        // Only generate __config_defaults when file feature is enabled
        #[cfg(feature = "file")]
        impl #impl_generics #struct_name #type_generics #where_clause {
            /// Returns default values for this config as a JSON object.
            #[doc(hidden)]
            pub fn __config_defaults() -> ::procenv::file::JsonValue {
                let mut __map = ::procenv::file::JsonMap::new();
                #(#default_entries)*
                #(#flatten_entries)*
                ::procenv::file::JsonValue::Object(__map)
            }
        }
    }
}

/// Generate the `__from_json_value()` method for serde-free deserialization.
///
/// This method is generated for ALL EnvConfig structs so they can be used
/// as nested types in `from_config()`. It extracts fields from a JSON value
/// without requiring the struct to derive `Deserialize`.
pub fn generate_from_json_value_impl(
    struct_name: &Ident,
    generics: &Generics,
    generators: &[Box<dyn FieldGenerator>],
) -> QuoteStream {
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();

    let field_extractions = generate_field_extractions(generators);
    let field_assignments = generate_field_assignments_from_json(generators);

    quote! {
        // Only generate __from_json_value when file feature is enabled
        // (needed for from_config() and nested struct support)
        #[cfg(feature = "file")]
        impl #impl_generics #struct_name #type_generics #where_clause {
            /// Extract config from a JSON value (internal, generated by macro).
            #[doc(hidden)]
            pub fn __from_json_value(
                __value: ::serde_json::Value
            ) -> std::result::Result<Self, ::procenv::Error> {
                let __obj = __value.as_object().ok_or_else(|| {
                    ::procenv::Error::extraction(
                        "<root>",
                        "object",
                        "expected JSON object at root"
                    )
                })?;

                let mut __errors: std::vec::Vec<::procenv::Error> = std::vec::Vec::new();

                #field_extractions

                if let std::option::Option::Some(err) = ::procenv::Error::multiple(__errors) {
                    return std::result::Result::Err(err);
                }

                std::result::Result::Ok(Self {
                    #field_assignments
                })
            }
        }
    }
}
