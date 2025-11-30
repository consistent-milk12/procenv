//! Flatten field implementation.
//!
//! This module provides [`FlattenField`], the code generator for nested
//! configuration structs that should be embedded (flattened) into the parent.
//!
//! # Usage
//!
//! ```rust,ignore
//! #[derive(EnvConfig)]
//! struct DatabaseConfig {
//!     #[env(var = "DB_HOST")]
//!     host: String,
//!     #[env(var = "DB_PORT", default = "5432")]
//!     port: u16,
//! }
//!
//! #[derive(EnvConfig)]
//! struct AppConfig {
//!     #[env(flatten)]
//!     database: DatabaseConfig,
//!
//!     #[env(var = "APP_NAME")]
//!     name: String,
//! }
//! ```
//!
//! # Generated Code Pattern
//!
//! For a flatten field, generates:
//! ```rust,ignore
//! let (database, __database_nested_sources) = DatabaseConfig::from_env_with_sources()?;
//! ```
//!
//! Errors from the nested struct are merged into the parent's error list.
//!
//! # Prefix Support
//!
//! Flatten fields support an optional prefix:
//! ```rust,ignore
//! #[env(flatten, prefix = "DB_")]
//! database: DatabaseConfig,
//! ```
//!
//! This prepends `DB_` to all nested env var names. Prefixes can be combined
//! with the parent struct's prefix.

use proc_macro2::TokenStream as QuoteStream;
use quote::{format_ident, quote};
use syn::{Ident, Type};

use super::{EnvExampleEntry, FieldGenerator};

/// A flattened nested config field.
///
/// The nested type must also derive `EnvConfig`. Its `from_env()` is called
/// and any errors are merged into the parent's error list.
///
/// ## Behavior
/// - Calls `NestedType::from_env()` on the nested struct (or with external prefix)
/// - If successful -> `Some(value)`
/// - If errors occur -> merges them into parent's `__errors` and returns `None`
///
/// ## Prefix Support
/// When `prefix` is set (e.g., `#[env(flatten, prefix = "DB_")]`), the nested
/// type's env vars are prefixed with this value. The prefix is combined with
/// any parent struct's prefix.
pub struct FlattenField {
    /// The struct field name
    pub name: Ident,

    /// The field's type (the nested config struct)
    pub ty: Type,

    /// Optional prefix to prepend to nested env var names
    pub prefix: Option<String>,
}

impl FieldGenerator for FlattenField {
    fn generate_loader(&self) -> QuoteStream {
        let field_name = &self.name;
        let ty = &self.ty;

        let nested_sources_ident = format_ident!("__{}_nested_sources", field_name);

        // Determine how to call the nested type based on whether we have a prefix
        let load_call = if let Some(prefix) = &self.prefix {
            // When a prefix is explicitly specified, use __from_env_with_external_prefix
            // and combine with any external prefix from parent
            quote! {
                <#ty>::__from_env_with_external_prefix(
                    std::option::Option::Some(
                        &format!("{}{}", __external_prefix.unwrap_or(""), #prefix)
                    )
                )
            }
        } else {
            // No prefix specified - use regular from_env_with_sources for backwards compatibility
            // This maintains the original behavior where flatten fields don't inherit parent prefix
            quote! {
                <#ty>::from_env_with_sources()
            }
        };

        quote! {
            let (#field_name, #nested_sources_ident): (
                std::option::Option<#ty>,
                ::procenv::ConfigSources
            ) = match #load_call {
                std::result::Result::Ok((v, sources))=> {
                    (std::option::Option::Some(v), sources)
                }

                std::result::Result::Err(e) => {
                    match e {
                        ::procenv::Error::Multiple { errors } => {
                            __errors.extend(errors);
                        }

                        other => {
                            __errors.push(other);
                        }
                    }

                    (std::option::Option::None, ::procenv::ConfigSources::new())
                }
            };
        }
    }

    fn generate_assignment(&self) -> QuoteStream {
        let name = &self.name;

        quote! { #name: #name.unwrap() }
    }

    fn name(&self) -> &Ident {
        &self.name
    }

    fn type_name(&self) -> String {
        let ty = &self.ty;
        quote!(#ty).to_string().replace(' ', "")
    }

    fn is_secret(&self) -> bool {
        false // The nested struct has its own Debug impl
    }

    fn is_secrecy_type(&self) -> bool {
        false
    }

    fn example_entries(&self) -> Vec<EnvExampleEntry> {
        // Flattened fields don't have direct env vars - their entries
        // come from the nested type's env_example() method
        vec![]
    }

    fn generate_example_fragment(&self) -> QuoteStream {
        let ty = &self.ty;
        // Generate a call to the nested type's env_example_entries method (without header)
        quote! { <#ty>::env_example_entries() }
    }

    fn is_flatten(&self) -> bool {
        true
    }

    fn field_type(&self) -> Option<&Type> {
        Some(&self.ty)
    }

    fn flatten_prefix(&self) -> Option<&str> {
        self.prefix.as_deref()
    }

    fn generate_source_tracking(&self) -> QuoteStream {
        let field_name = &self.name;
        let field_name_str = field_name.to_string();
        let _ty = &self.ty;

        let nested_sources_ident = format_ident!("__{}_nested_sources", field_name);

        quote! {
            // Nested sources were captured during from_env_with_sources call
            __sources.extend_nested(#field_name_str, #nested_sources_ident);
        }
    }

    fn env_var_name(&self) -> Option<&str> {
        None // Flatten fields don't have their own env var
    }

    fn field_name(&self) -> Option<&Ident> {
        None // Flatten fields don't participate in direct key access
    }
}
