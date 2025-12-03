//! Runtime access code generation.
//!
//! This module generates methods for runtime key-based access to configuration:
//! - `keys()` - Returns all field names as static strings
//! - `get_str(&self, key)` - Gets field value as string by key
//! - `has_key(key)` - Checks if a key exists

use std::string::ToString;

use proc_macro2::TokenStream as QuoteStream;
use quote::quote;
use syn::{Generics, Ident};

use crate::field::FieldGenerator;

/// Generates runtime access methods: `keys()`, `get_str()`, `has_key()`.
pub fn generate_runtime_access_impl(
    struct_name: &Ident,
    generics: &Generics,
    generators: &[Box<dyn FieldGenerator>],
) -> QuoteStream {
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();

    // Collect non-flatten field names (including format fields for completeness)
    let key_names: Vec<String> = generators
        .iter()
        .filter(|g| !g.is_flatten())
        .filter_map(|g| g.field_name().map(ToString::to_string))
        .collect();

    let key_literals: Vec<_> = key_names.iter().map(|k| quote! { #k }).collect();
    let num_keys = key_names.len();

    // Match arms for get_str
    // Regular fields use Display, format fields use Debug (may be complex structures)
    let get_str_arms: Vec<_> = generators
        .iter()
        .filter(|g| !g.is_flatten())
        .filter_map(|g| {
            let name = g.field_name()?;
            let name_str = name.to_string();

            if g.is_secret() {
                Some(quote! { #name_str => std::option::Option::Some("<redacted>".to_string()), })
            } else if g.format_config().is_some() {
                // Format fields (json/toml/yaml) use Debug since they may not implement Display
                if g.is_optional() {
                    Some(quote! { #name_str => self.#name.as_ref().map(|v| format!("{:?}", v)), })
                } else {
                    Some(quote! { #name_str => std::option::Option::Some(format!("{:?}", self.#name)), })
                }
            } else if g.is_optional() {
                Some(quote! { #name_str => self.#name.as_ref().map(|v| v.to_string()), })
            } else {
                Some(quote! { #name_str => std::option::Option::Some(self.#name.to_string()), })
            }
        })
        .collect();

    // Flatten field delegation for get_str
    let flatten_get_str_arms: Vec<_> = generators
        .iter()
        .filter(|g| g.is_flatten())
        .map(|g| {
            let name = g.name();
            let name_str = name.to_string();
            let prefix = format!("{name_str}.");

            quote! {
                key if key.starts_with(#prefix) => {
                    self.#name.get_str(&key[#prefix.len()..])
                }
            }
        })
        .collect();

    // Flatten field delegation for has_key
    let flatten_has_key_arms: Vec<_> = generators
        .iter()
        .filter(|g| g.is_flatten())
        .filter_map(|g| {
            let ty = g.field_type()?;
            let name_str = g.name().to_string();
            let prefix = format!("{name_str}.");

            Some(quote! {
                if key.starts_with(#prefix) {
                    return <#ty>::has_key(&key[#prefix.len()..]);
                }
            })
        })
        .collect();

    quote! {
        impl #impl_generics #struct_name #type_generics #where_clause {
            /// Returns all configuration keys.
            pub fn keys() -> &'static [&'static str] {
                static KEYS: [&str; #num_keys] = [#(#key_literals),*];
                &KEYS
            }

            /// Gets field value as string by key.
            /// Secret fields return "<redacted>".
            pub fn get_str(&self, key: &str) -> Option<String> {
                match key {
                    #(#get_str_arms)*
                    #(#flatten_get_str_arms)*
                    _ => None,
                }
            }

            /// Checks if a key exists.
            pub fn has_key(key: &str) -> bool {
                // Check direct keys first
                if Self::keys().contains(&key) {
                    return true;
                }

                // Check nested keys via flatten fields
                #(#flatten_has_key_arms)*

                false
            }
        }
    }
}
