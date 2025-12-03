//! Secret field implementations (`SecretString`, `SecretBox`).
//!
//! This module provides code generators for fields using the `secrecy` crate's
//! types, which provide automatic secret protection:
//!
//! - [`SecretStringField`] - For `SecretString` (alias for `SecretBox<str>`)
//! - [`SecretBoxField`] - For `SecretBox<T>` with any inner type
//!
//! # Secrecy Integration
//!
//! The `secrecy` crate provides types that:
//! - Redact values in `Debug` and `Display` output
//! - Zeroize memory on drop (with `zeroize` feature)
//! - Require explicit `.expose_secret()` to access the value
//!
//! # Generated Code Pattern
//!
//! For a `SecretString` field:
//! ```rust,ignore
//! #[env(var = "API_KEY")]
//! api_key: SecretString,
//! ```
//!
//! Generates:
//! ```rust,ignore
//! let api_key: Option<SecretString> = match std::env::var("API_KEY") {
//!     Ok(val) => Some(SecretString::from(val)),
//!     Err(VarError::NotPresent) => { __errors.push(Error::missing(...)); None }
//!     // ...
//! };
//! ```
//!
//! # Automatic Secret Detection
//!
//! Fields with `SecretString` or `SecretBox<T>` types are automatically
//! treated as secrets - no need to add the `secret` attribute.

use proc_macro2::TokenStream as QuoteStream;
use quote::{format_ident, quote};
use syn::{Ident, Type};

use super::{EnvExampleEntry, FieldGenerator};

/// A field of type `SecretString` for secret string values.
///
/// `SecretString` is a type alias for `SecretBox<str>` from the secrecy crate.
/// It provides automatic Debug/Display redaction and zeroize on drop.
///
/// ## Behavior
/// - If env var exists -> `SecretString::from(value)`
/// - If env var is missing -> `None` + `Error::Missing`
/// - If env var contains invalid UTF-8 -> `None` + `Error::InvalidUtf8`
pub struct SecretStringField {
    /// The struct field name
    pub name: Ident,

    /// The environment variable name
    pub env_var: String,

    /// Doc comment from the field
    pub doc: Option<String>,
}

impl FieldGenerator for SecretStringField {
    fn generate_loader(&self) -> QuoteStream {
        let name = &self.name;
        let env_var = &self.env_var;

        quote! {
            let #name: std::option::Option<::procenv::SecretString> = match std::env::var(#env_var) {
                std::result::Result::Ok(val) => {
                    std::option::Option::Some(::procenv::SecretString::from(val))
                }

                std::result::Result::Err(e) => {
                    match e {
                        std::env::VarError::NotPresent => {
                            __errors.push(::procenv::Error::missing(#env_var));
                        }

                        std::env::VarError::NotUnicode(_) => {
                            __errors.push(::procenv::Error::InvalidUtf8 {
                                var: #env_var.to_string()
                            });
                        }
                    }

                    std::option::Option::None
                }
            };
        }
    }

    fn generate_loader_with_external_prefix(&self) -> QuoteStream {
        let name = &self.name;
        let base_var = &self.env_var;
        let effective_var_ident = format_ident!("__{}_effective_var", name);

        quote! {
            // Build effective env var name with external prefix
            let #effective_var_ident: std::string::String = format!(
                "{}{}",
                __external_prefix.unwrap_or(""),
                #base_var
            );

            let #name: std::option::Option<::procenv::SecretString> = match std::env::var(&#effective_var_ident) {
                std::result::Result::Ok(val) => {
                    std::option::Option::Some(::procenv::SecretString::from(val))
                }

                std::result::Result::Err(e) => {
                    match e {
                        std::env::VarError::NotPresent => {
                            __errors.push(::procenv::Error::missing(&#effective_var_ident));
                        }

                        std::env::VarError::NotUnicode(_) => {
                            __errors.push(::procenv::Error::InvalidUtf8 {
                                var: #effective_var_ident.clone()
                            });
                        }
                    }

                    std::option::Option::None
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
        "SecretString".to_string()
    }

    fn is_secret(&self) -> bool {
        true
    }

    fn is_secrecy_type(&self) -> bool {
        true
    }

    fn example_entries(&self) -> Vec<EnvExampleEntry> {
        vec![EnvExampleEntry {
            var_name: self.env_var.clone(),
            doc: self.doc.clone(),
            required: true,
            default: None,
            secret: true,
            type_hint: "SecretString".to_string(),
        }]
    }

    fn generate_source_tracking(&self) -> QuoteStream {
        let field_name = &self.name;
        let field_name_str = field_name.to_string();
        let env_var = &self.env_var;

        let source_ident = format_ident!("__{}_source", field_name);

        quote! {
            let #source_ident = if #field_name.is_some() {
                ::procenv::ValueSource::new(
                    #env_var,
                    if __dotenv_loaded {
                        if __pre_dotenv_vars.contains(#env_var) {
                            ::procenv::Source::Environment
                        } else {
                            ::procenv::Source::DotenvFile(None)
                        }
                    } else {
                        ::procenv::Source::Environment
                    }
                )
            } else {
                ::procenv::ValueSource::new(#env_var, ::procenv::Source::NotSet)
            };

            __sources.add(#field_name_str, #source_ident);
        }
    }

    fn env_var_name(&self) -> Option<&str> {
        Some(&self.env_var)
    }

    fn field_type(&self) -> Option<&Type> {
        // SecretString doesn't have a stored Type since it's always SecretString
        // The extraction code handles this specially by checking is_secrecy_type()
        None
    }
}

/// A field of type `SecretBox<T>` for secret values of any parseable type.
///
/// ## Behavior
/// - If env var exists and parses successfully -> `SecretBox::init_with(|| value)`
/// - If env var exists but fails to parse -> `None` + `Error::Parse`
/// - If env var is missing -> `None` + `Error::Missing`
/// - If env var contains invalid UTF-8 -> `None` + `Error::InvalidUtf8`
pub struct SecretBoxField {
    /// The struct field name
    pub name: Ident,

    /// The inner type T of `SecretBox<T>`
    pub inner_type: Type,

    /// The environment variable name
    pub env_var: String,

    /// Doc comment from the field
    pub doc: Option<String>,
}

impl FieldGenerator for SecretBoxField {
    fn generate_loader(&self) -> QuoteStream {
        let name = &self.name;
        let inner = &self.inner_type;
        let env_var = &self.env_var;
        let type_name = quote!(#inner).to_string();

        quote! {
            let #name: std::option::Option<::procenv::SecretBox<#inner>> = match std::env::var(#env_var) {
                std::result::Result::Ok(val) => {
                    match val.parse::<#inner>() {
                        std::result::Result::Ok(v) => {
                            // Use init_with to minimize exposure and zeroize stack copy
                            std::option::Option::Some(::procenv::SecretBox::init_with(|| v))
                        }

                        std::result::Result::Err(e) => {
                            __errors.push(::procenv::Error::parse(
                                #env_var,
                                val,
                                true, // Always secret
                                #type_name,
                                std::boxed::Box::new(e),
                            ));

                            std::option::Option::None
                        }
                    }
                }

                std::result::Result::Err(e) => {
                    match e {
                        std::env::VarError::NotPresent => {
                            __errors.push(::procenv::Error::missing(#env_var));
                        }

                        std::env::VarError::NotUnicode(_) => {
                            __errors.push(::procenv::Error::InvalidUtf8 {
                                var: #env_var.to_string()
                            });
                        }
                    }

                    std::option::Option::None
                }
            };
        }
    }

    fn generate_loader_with_external_prefix(&self) -> QuoteStream {
        let name = &self.name;
        let inner = &self.inner_type;
        let base_var = &self.env_var;
        let type_name = quote!(#inner).to_string();
        let effective_var_ident = format_ident!("__{}_effective_var", name);

        quote! {
            // Build effective env var name with external prefix
            let #effective_var_ident: std::string::String = format!(
                "{}{}",
                __external_prefix.unwrap_or(""),
                #base_var
            );

            let #name: std::option::Option<::procenv::SecretBox<#inner>> = match std::env::var(&#effective_var_ident) {
                std::result::Result::Ok(val) => {
                    match val.parse::<#inner>() {
                        std::result::Result::Ok(v) => {
                            std::option::Option::Some(::procenv::SecretBox::init_with(|| v))
                        }

                        std::result::Result::Err(e) => {
                            __errors.push(::procenv::Error::parse(
                                &#effective_var_ident,
                                val,
                                true,
                                #type_name,
                                std::boxed::Box::new(e),
                            ));

                            std::option::Option::None
                        }
                    }
                }

                std::result::Result::Err(e) => {
                    match e {
                        std::env::VarError::NotPresent => {
                            __errors.push(::procenv::Error::missing(&#effective_var_ident));
                        }

                        std::env::VarError::NotUnicode(_) => {
                            __errors.push(::procenv::Error::InvalidUtf8 {
                                var: #effective_var_ident.clone()
                            });
                        }
                    }

                    std::option::Option::None
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
        let inner = &self.inner_type;
        quote!(#inner).to_string().replace(' ', "")
    }

    fn is_secret(&self) -> bool {
        true
    }

    fn is_secrecy_type(&self) -> bool {
        true
    }

    fn example_entries(&self) -> Vec<EnvExampleEntry> {
        let inner = &self.inner_type;
        vec![EnvExampleEntry {
            var_name: self.env_var.clone(),
            doc: self.doc.clone(),
            required: true,
            default: None,
            secret: true,
            type_hint: format!("SecretBox<{}>", quote!(#inner).to_string().replace(' ', "")),
        }]
    }

    fn generate_source_tracking(&self) -> QuoteStream {
        let field_name = &self.name;
        let field_name_str = field_name.to_string();
        let env_var = &self.env_var;

        let source_ident = format_ident!("__{}_source", field_name);

        quote! {
            let #source_ident = if #field_name.is_some() {
                ::procenv::ValueSource::new(
                    #env_var,
                    if __dotenv_loaded {
                        if __pre_dotenv_vars.contains(#env_var) {
                            ::procenv::Source::Environment
                        } else {
                            ::procenv::Source::DotenvFile(None)
                        }
                    } else {
                        ::procenv::Source::Environment
                    }
                )
            } else {
                ::procenv::ValueSource::new(#env_var, ::procenv::Source::NotSet)
            };

            __sources.add(#field_name_str, #source_ident);
        }
    }

    fn env_var_name(&self) -> Option<&str> {
        Some(&self.env_var)
    }

    fn field_type(&self) -> Option<&Type> {
        Some(&self.inner_type)
    }
}

/// The kind of secret field detected from the type.
#[derive(Clone, Debug)]
pub enum SecretKind {
    /// `SecretString` (alias for `SecretBox<str>`)
    String,

    /// `SecretBox<T>` with the inner type (boxed to reduce enum size)
    Box(Box<Type>),
}
