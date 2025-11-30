//! Optional field implementation.
//!
//! This module provides [`OptionalField`], the code generator for fields that
//! may or may not have a value. The field type must be `Option<T>`.
//!
//! # Generated Code Pattern
//!
//! For an optional field like:
//! ```rust,ignore
//! #[env(var = "DEBUG_MODE", optional)]
//! debug: Option<bool>,
//! ```
//!
//! Generates code that:
//! 1. Reads the environment variable
//! 2. Returns `None` if not present (no error!)
//! 3. Parses as `bool` if present
//!
//! # Type Validation
//!
//! The macro validates at compile time that optional fields use `Option<T>`.
//! Using `optional` on a non-Option field produces a compile error.
//!
//! # Error Behavior
//!
//! - **Missing env var** → `None` (no error, this is expected)
//! - **Invalid UTF-8** → `Error::InvalidUtf8` pushed
//! - **Parse failure** → `Error::Parse` pushed

use proc_macro2::TokenStream as QuoteStream;
use quote::{format_ident, quote};
use syn::{Ident, Type};

use crate::parse::{CliAttr, ProfileAttr};

use super::{EnvExampleEntry, FieldGenerator};

/// An optional field that becomes `None` when the environment variable is missing.
///
/// The field type must be `Option<T>` where `T: FromStr`.
///
/// ## Behavior
///
/// - If env var exists and parses successfully → `Some(value)`
/// - If env var exists but fails to parse → `None` + `Error::Parse`
/// - If env var is missing → `None` (no error!)
/// - If env var contains invalid UTF-8 → `None` + `Error::InvalidUtf8`
pub struct OptionalField {
    /// The struct field name
    pub name: Ident,

    /// The inner type of Option<T> (i.e., `T`)
    /// We store this separately because we need to call `.parse::<T>()`
    pub inner_type: Type,

    /// The environment variable name
    pub env_var: String,

    /// Whether to mask the value in error output
    pub secret: bool,

    /// Doc comment from the field
    pub doc: Option<String>,

    /// CLI argument configuration (Phase 14)
    pub cli: Option<CliAttr>,

    /// Profile-specific default values (Phase 16)
    pub profile: Option<ProfileAttr>,

    /// Deserialization format for structured data (Phase 17)
    pub format: Option<String>,

    pub validate: Option<String>,
}

impl FieldGenerator for OptionalField {
    fn generate_loader(&self) -> QuoteStream {
        let name = &self.name;
        let inner = &self.inner_type;
        let env_var = &self.env_var;
        let secret = self.secret;
        let type_name = quote!(#inner).to_string();

        quote! {
            // WARN: The local variable is Option<inner_type>, not Option<Option<inner_type>>
            // The assignment will use this directly since the field is already Option<T>
            let #name: std::option::Option<#inner> = match std::env::var(#env_var) {
                std::result::Result::Ok(val) => {
                    match val.parse::<#inner>() {
                        std::result::Result::Ok(v) => std::option::Option::Some(v),

                        std::result::Result::Err(e) => {
                            __errors.push(::procenv::Error::parse(
                                #env_var,
                                val,
                                #secret,
                                #type_name,
                                std::boxed::Box::new(e),
                            ));

                            std::option::Option::None
                        }
                    }
                }

                std::result::Result::Err(e) => {
                    // Only report error for invalid UTF-8
                    // Missing env var is expected for optional fields
                    if let std::env::VarError::NotUnicode(_) = e {
                        __errors.push(::procenv::Error::InvalidUtf8 { var: #env_var.to_string() });
                    }

                    // Return None for both NotPresent and NotUnicode
                    std::option::Option::None
                }
            };
        }
    }

    fn generate_loader_with_external_prefix(&self) -> QuoteStream {
        let name = &self.name;
        let inner = &self.inner_type;
        let base_var = &self.env_var;
        let secret = self.secret;
        let type_name = quote!(#inner).to_string();
        let effective_var_ident = format_ident!("__{}_effective_var", name);
        let profile_used_ident = format_ident!("__{}_from_profile", name);

        // Check if this field has profile configuration
        if let Some(profile_config) = &self.profile {
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

            quote! {
                // Build effective env var name with external prefix
                let #effective_var_ident: std::string::String = format!(
                    "{}{}",
                    __external_prefix.unwrap_or(""),
                    #base_var
                );

                // Determine profile default value (if profile matches)
                let __profile_default: std::option::Option<&str> = match __profile.as_deref() {
                    #(#match_arms)*
                    _ => std::option::Option::None,
                };

                // Get value to parse: env var > profile default > None
                let (__value_to_parse, #profile_used_ident): (std::option::Option<std::string::String>, bool) =
                    match std::env::var(&#effective_var_ident) {
                        std::result::Result::Ok(val) => {
                            (std::option::Option::Some(val), false)
                        }
                        std::result::Result::Err(std::env::VarError::NotPresent) => {
                            match __profile_default {
                                std::option::Option::Some(profile_val) => {
                                    (std::option::Option::Some(profile_val.to_string()), true)
                                }
                                std::option::Option::None => {
                                    // Optional field with no env var and no profile match = None
                                    (std::option::Option::None, false)
                                }
                            }
                        }
                        std::result::Result::Err(std::env::VarError::NotUnicode(_)) => {
                            __errors.push(::procenv::Error::InvalidUtf8 {
                                var: #effective_var_ident.clone(),
                            });
                            (std::option::Option::None, false)
                        }
                    };

                // Parse the value if present
                let #name: std::option::Option<#inner> = match __value_to_parse {
                    std::option::Option::Some(val) => {
                        match val.parse::<#inner>() {
                            std::result::Result::Ok(v) => std::option::Option::Some(v),
                            std::result::Result::Err(e) => {
                                __errors.push(::procenv::Error::parse(
                                    &#effective_var_ident,
                                    if #secret { "[REDACTED]".to_string() } else { val },
                                    #secret,
                                    #type_name,
                                    std::boxed::Box::new(e),
                                ));
                                std::option::Option::None
                            }
                        }
                    }
                    std::option::Option::None => std::option::Option::None,
                };
            }
        } else {
            // No profile config - use original simple logic
            quote! {
                // Build effective env var name with external prefix
                let #effective_var_ident: std::string::String = format!(
                    "{}{}",
                    __external_prefix.unwrap_or(""),
                    #base_var
                );

                // No profile for this field
                let #profile_used_ident: bool = false;

                let #name: std::option::Option<#inner> = match std::env::var(&#effective_var_ident) {
                    std::result::Result::Ok(val) => {
                        match val.parse::<#inner>() {
                            std::result::Result::Ok(v) => std::option::Option::Some(v),

                            std::result::Result::Err(e) => {
                                __errors.push(::procenv::Error::parse(
                                    &#effective_var_ident,
                                    val,
                                    #secret,
                                    #type_name,
                                    std::boxed::Box::new(e),
                                ));

                                std::option::Option::None
                            }
                        }
                    }

                    std::result::Result::Err(e) => {
                        // Only report error for invalid UTF-8
                        if let std::env::VarError::NotUnicode(_) = e {
                            __errors.push(::procenv::Error::InvalidUtf8 {
                                var: #effective_var_ident.clone(),
                            });
                        }

                        std::option::Option::None
                    }
                };
            }
        }
    }

    fn generate_assignment(&self) -> QuoteStream {
        let name = &self.name;
        // No unwrap needed - the field type is Option<T> and our local is Option<T>
        quote! { #name }
    }

    fn name(&self) -> &Ident {
        &self.name
    }

    fn type_name(&self) -> String {
        let inner = &self.inner_type;
        quote!(#inner).to_string().replace(' ', "")
    }

    fn is_secret(&self) -> bool {
        self.secret
    }

    fn example_entries(&self) -> Vec<EnvExampleEntry> {
        let inner = &self.inner_type;
        vec![EnvExampleEntry {
            var_name: self.env_var.clone(),
            doc: self.doc.clone(),
            required: false, // Optional fields are not required
            default: None,
            secret: self.secret,
            type_hint: format!("Option<{}>", quote!(#inner).to_string().replace(' ', "")),
        }]
    }

    fn generate_source_tracking(&self) -> QuoteStream {
        let field_name = &self.name;
        let field_name_str = field_name.to_string();
        let env_var = &self.env_var;

        let source_ident = format_ident!("__{}_source", field_name);

        // Check if this field has profile config
        if self.profile.is_some() {
            let profile_used_ident = format_ident!("__{}_from_profile", field_name);

            quote! {
                let #source_ident = if #profile_used_ident {
                    ::procenv::ValueSource::new(
                        #env_var,
                        ::procenv::Source::Profile(__profile.clone().unwrap_or_default())
                    )
                } else if #field_name.is_some() {
                    if __dotenv_loaded && !__pre_dotenv_vars.contains(#env_var) {
                        ::procenv::ValueSource::new(#env_var, ::procenv::Source::DotenvFile(None))
                    } else {
                        ::procenv::ValueSource::new(#env_var, ::procenv::Source::Environment)
                    }
                } else {
                    ::procenv::ValueSource::new(#env_var, ::procenv::Source::NotSet)
                };

                __sources.add(#field_name_str, #source_ident);
            }
        } else {
            quote! {
                let #source_ident = if #field_name.is_some() {
                    if __dotenv_loaded && !__pre_dotenv_vars.contains(#env_var) {
                        ::procenv::ValueSource::new(#env_var, ::procenv::Source::DotenvFile(None))
                    } else {
                        ::procenv::ValueSource::new(#env_var, ::procenv::Source::Environment)
                    }
                } else {
                    ::procenv::ValueSource::new(#env_var, ::procenv::Source::NotSet)
                };

                __sources.add(#field_name_str, #source_ident);
            }
        }
    }

    fn generate_format_loader(&self, format: &str) -> QuoteStream {
        let name = &self.name;
        let inner = &self.inner_type;
        let env_var = &self.env_var;
        let secret = self.secret;

        let deserialize_call = match format {
            "json" => quote! { ::serde_json::from_str::<#inner>(&val) },

            "toml" => quote! { ::toml::from_str::<#inner>(&val) },

            "yaml" => quote! { ::serde_saphyr::from_str::<#inner>(&val) },

            _ => unreachable!("Format validated at parse time"),
        };

        let format_name = format.to_uppercase();

        quote! {
            let #name: std::option::Option<#inner> = match std::env::var(#env_var) {
                std::result::Result::Ok(val) => {
                    match #deserialize_call {
                        std::result::Result::Ok(v) => std::option::Option::Some(v),

                        std::result::Result::Err(e) => {
                            __errors.push(::procenv::Error::parse(
                                #env_var,
                                if #secret {
                                    "[REDACTED]".to_string()
                                } else {
                                    val
                                },
                                #secret,
                                concat!(#format_name, " data"),
                                std::boxed::Box::new(e),
                            ));

                            std::option::Option::None
                        }
                    }
                }

                std::result::Result::Err(e) => {
                    // Only report error invalid UTF-8
                    // Missing env var is expected for optional fields
                    if let std::env::VarError::NotUnicode(_) = e {
                        __errors.push(::procenv::Error::InvalidUtf8 {
                            var: #env_var.to_string(),
                        });
                    }

                    std::option::Option::None
                }
            };
        }
    }

    fn env_var_name(&self) -> Option<&str> {
        Some(&self.env_var)
    }

    fn cli_config(&self) -> Option<&CliAttr> {
        self.cli.as_ref()
    }

    fn profile_config(&self) -> Option<&ProfileAttr> {
        self.profile.as_ref()
    }

    fn format_config(&self) -> Option<&str> {
        self.format.as_deref()
    }

    fn validate_fn(&self) -> Option<&str> {
        self.validate.as_deref()
    }
}
