//! Default field implementation.
//!
//! This module provides [`DefaultField`], the code generator for fields that
//! have a fallback value when the environment variable is not set.
//!
//! # Generated Code Pattern
//!
//! For a field with default like:
//! ```rust,ignore
//! #[env(var = "PORT", default = "8080")]
//! port: u16,
//! ```
//!
//! Generates code that:
//! 1. Reads the environment variable
//! 2. Falls back to `"8080"` if not present
//! 3. Parses the value (env var or default) as `u16`
//!
//! # Important: Default Validation
//!
//! The default value is parsed at **runtime**, not compile time. If you specify
//! `default = "invalid"` for a `u16` field, you'll get a parse error when the
//! env var is missing and the default is used. This catches configuration
//! mistakes early.
//!
//! # Error Behavior
//!
//! - **Missing env var** → Uses default value (no error)
//! - **Invalid UTF-8** → `Error::InvalidUtf8` pushed
//! - **Parse failure** → `Error::Parse` pushed (whether from env or default)

use proc_macro2::TokenStream as QuoteStream;
use quote::{format_ident, quote};
use syn::{Ident, Type};

use crate::parse::{CliAttr, ProfileAttr};

use super::{EnvExampleEntry, FieldGenerator};

/// A field with a default value used when the environment variable is missing.
///
/// ## Behavior
/// - If env var exists and parses successfully -> `Some(value)`
/// - If env var exists but fails to parse -> `None` + `Error::Parse`
/// - If env var is missing -> parse and use default value
/// - If env var contains invalid UTF-8 -> `None` + `Error::InvalidUtf8`
///
/// NOTE: On Default parsing
/// The default value is also parsed at runtime. If the default itself
/// fails to parse (e.g., `default = "abc"` for a `u16` field), an error
/// is recorded. This catches configuration mistakes early.
pub struct DefaultField {
    /// The struct field name
    pub name: Ident,

    /// The field's type
    pub ty: Type,

    /// The environment variable name
    pub env_var: String,

    /// The default value as a string (will be parsed at runtime)
    pub default: String,

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

impl FieldGenerator for DefaultField {
    fn generate_loader(&self) -> QuoteStream {
        let field_name = &self.name;
        let ty = &self.ty;
        let env_var = &self.env_var;
        let default = &self.default;
        let secret = self.secret;

        let used_default_ident = format_ident!("__{}_used_default", field_name);

        quote! {
            let mut #used_default_ident = false;

            let #field_name: std::option::Option<#ty> = (|| {
                let val = match std::env::var(#env_var) {
                    std::result::Result::Ok(v) => v,

                    std::result::Result::Err(std::env::VarError::NotPresent) => {
                        #used_default_ident = true;
                        #default.to_string()
                    },

                    std::result::Result::Err(std::env::VarError::NotUnicode(_)) => {
                        __errors.push(::procenv::Error::InvalidUtf8 {
                            var: #env_var.to_string(),
                        });

                        return std::option::Option::None;
                    }
                };

                match val.parse::<#ty>() {
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
                            std::any::type_name::<#ty>(),
                            std::boxed::Box::new(e),
                        ));

                        std::option::Option::None
                    }
                }
            })();
        }
    }

    fn generate_loader_with_external_prefix(&self) -> QuoteStream {
        let field_name = &self.name;
        let ty = &self.ty;
        let base_var = &self.env_var;
        let default = &self.default;
        let secret = self.secret;

        let used_default_ident = format_ident!("__{}_used_default", field_name);
        let effective_var_ident = format_ident!("__{}_effective_var", field_name);
        let profile_used_ident = format_ident!("__{}_from_profile", field_name);

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
                let mut #used_default_ident = false;

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

                // Get value to parse: env var > profile default > compile-time default
                let (val, #profile_used_ident): (std::string::String, bool) =
                    match std::env::var(&#effective_var_ident) {
                        std::result::Result::Ok(v) => (v, false),
                        std::result::Result::Err(std::env::VarError::NotPresent) => {
                            match __profile_default {
                                std::option::Option::Some(profile_val) => {
                                    (profile_val.to_string(), true)
                                }
                                std::option::Option::None => {
                                    #used_default_ident = true;
                                    (#default.to_string(), false)
                                }
                            }
                        }
                        std::result::Result::Err(std::env::VarError::NotUnicode(_)) => {
                            __errors.push(::procenv::Error::InvalidUtf8 {
                                var: #effective_var_ident.clone(),
                            });
                            // Use default on UTF-8 error
                            #used_default_ident = true;
                            (#default.to_string(), false)
                        }
                    };

                // Parse the value
                let #field_name: std::option::Option<#ty> = match val.parse::<#ty>() {
                    std::result::Result::Ok(v) => std::option::Option::Some(v),
                    std::result::Result::Err(e) => {
                        __errors.push(::procenv::Error::parse(
                            &#effective_var_ident,
                            if #secret { "[REDACTED]".to_string() } else { val },
                            #secret,
                            std::any::type_name::<#ty>(),
                            std::boxed::Box::new(e),
                        ));
                        std::option::Option::None
                    }
                };
            }
        } else {
            // No profile config - use original simple logic
            quote! {
                let mut #used_default_ident = false;

                // Build effective env var name with external prefix
                let #effective_var_ident: std::string::String = format!(
                    "{}{}",
                    __external_prefix.unwrap_or(""),
                    #base_var
                );

                // No profile for this field
                let #profile_used_ident: bool = false;

                let #field_name: std::option::Option<#ty> = (|| {
                    let val = match std::env::var(&#effective_var_ident) {
                        std::result::Result::Ok(v) => v,

                        std::result::Result::Err(std::env::VarError::NotPresent) => {
                            #used_default_ident = true;
                            #default.to_string()
                        },

                        std::result::Result::Err(std::env::VarError::NotUnicode(_)) => {
                            __errors.push(::procenv::Error::InvalidUtf8 {
                                var: #effective_var_ident.clone(),
                            });

                            return std::option::Option::None;
                        }
                    };

                    match val.parse::<#ty>() {
                        std::result::Result::Ok(v) => std::option::Option::Some(v),

                        std::result::Result::Err(e) => {
                            __errors.push(::procenv::Error::parse(
                                &#effective_var_ident,
                                if #secret {
                                    "[REDACTED]".to_string()
                                } else {
                                    val
                                },
                                #secret,
                                std::any::type_name::<#ty>(),
                                std::boxed::Box::new(e),
                            ));

                            std::option::Option::None
                        }
                    }
                })();
            }
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
        self.secret
    }

    fn example_entries(&self) -> Vec<EnvExampleEntry> {
        let ty = &self.ty;
        vec![EnvExampleEntry {
            var_name: self.env_var.clone(),
            doc: self.doc.clone(),
            required: false,
            default: Some(self.default.clone()),
            secret: self.secret,
            type_hint: quote!(#ty).to_string().replace(' ', ""),
        }]
    }

    fn generate_source_tracking(&self) -> QuoteStream {
        let field_name = &self.name;
        let field_name_str = field_name.to_string();
        let env_var = &self.env_var;

        let source_ident = format_ident!("__{}_source", field_name);
        let used_default_ident = format_ident!("__{}_used_default", field_name);

        // Check if this field has profile config
        if self.profile.is_some() {
            let profile_used_ident = format_ident!("__{}_from_profile", field_name);

            quote! {
                let #source_ident = if #profile_used_ident {
                    // Value came from profile
                    ::procenv::ValueSource::new(
                        #env_var,
                        ::procenv::Source::Profile(__profile.clone().unwrap_or_default())
                    )
                } else if #used_default_ident {
                    ::procenv::ValueSource::new(#env_var, ::procenv::Source::Default)
                } else if __dotenv_loaded && !__pre_dotenv_vars.contains(#env_var) {
                    ::procenv::ValueSource::new(#env_var, ::procenv::Source::DotenvFile(None))
                } else {
                    ::procenv::ValueSource::new(#env_var, ::procenv::Source::Environment)
                };

                __sources.add(#field_name_str, #source_ident);
            }
        } else {
            // No profile config use original logic
            quote! {
                let #source_ident = if #used_default_ident {
                    ::procenv::ValueSource::new(#env_var, ::procenv::Source::Default)
                } else if __dotenv_loaded && !__pre_dotenv_vars.contains(#env_var) {
                    ::procenv::ValueSource::new(#env_var, ::procenv::Source::DotenvFile(None))
                } else {
                    ::procenv::ValueSource::new(#env_var, ::procenv::Source::Environment)
                };

                __sources.add(#field_name_str, #source_ident);
            }
        }
    }

    fn generate_format_loader(&self, format: &str) -> QuoteStream {
        let field_name = &self.name;
        let env_var = &self.env_var;
        let default = &self.default;
        let secret = self.secret;

        let used_default_ident = format_ident!("__{}_used_default", field_name);

        let deserialize_call: QuoteStream = match format {
            "json" => quote! { ::serde_json::from_str(&val) },

            "toml" => quote! { ::toml::from_str(&val) },

            "yaml" => quote! { ::serde_saphyr::from_str(&val) },

            _ => unreachable!("Format validated at parse time"),
        };

        let format_name: String = format.to_uppercase();

        quote! {
            let mut #used_default_ident = false;

            let #field_name = (|| {
                let val = match std::env::var(#env_var) {
                    std::result::Result::Ok(v) => v,

                    std::result::Result::Err(std::env::VarError::NotPresent) => {
                        #used_default_ident = true;
                        #default.to_string()
                    }

                    std::result::Result::Err(std::env::VarError::NotUnicode(_)) => {
                        __errors.push(::procenv::Error::InvalidUtf8 {
                            var: #env_var.to_string(),
                        });

                        return std::option::Option::None;
                    }
                };

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
            })();
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

    fn default_value(&self) -> Option<&str> {
        Some(&self.default)
    }

    fn format_config(&self) -> Option<&str> {
        self.format.as_deref()
    }

    fn validate_fn(&self) -> Option<&str> {
        self.validate.as_deref()
    }
}
