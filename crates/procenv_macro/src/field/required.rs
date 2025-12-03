//! Required field implementation.
//!
//! This module provides [`RequiredField`], the code generator for fields that
//! must have a value from the environment. If the environment variable is not
//! set, an error is recorded (but loading continues for error accumulation).
//!
//! # Generated Code Pattern
//!
//! For a required field like:
//! ```rust,ignore
//! #[env(var = "DATABASE_URL")]
//! db_url: String,
//! ```
//!
//! Generates:
//! ```rust,ignore
//! let db_url: Option<String> = match std::env::var("DATABASE_URL") {
//!     Ok(val) => match val.parse() {
//!         Ok(v) => Some(v),
//!         Err(e) => { __errors.push(Error::parse(...)); None }
//!     },
//!     Err(VarError::NotPresent) => { __errors.push(Error::missing(...)); None }
//!     Err(VarError::NotUnicode(_)) => { __errors.push(Error::InvalidUtf8 {...}); None }
//! };
//! ```
//!
//! # Error Behavior
//!
//! - **Missing** → `Error::Missing` pushed to accumulator
//! - **Invalid UTF-8** → `Error::InvalidUtf8` pushed
//! - **Parse failure** → `Error::Parse` pushed with type info

use proc_macro2::TokenStream as QuoteStream;
use quote::{format_ident, quote};
use syn::{Ident, Type};

use crate::parse::{CliAttr, ProfileAttr};

use super::{EnvExampleEntry, FieldGenerator};

/// A required field that errors if the environment variable is missing.
///
/// This is the default field type when no `default` or `optional` is specified.
///
/// ## Behavior
/// - If env var exists and parses successfully -> `Some(value)`
/// - If env var exists but fails to parse -> `None` + `Error::Parse`
/// - If env var is missing -> `None` + `Error::Missing`
/// - If env var contains invalid UTF-8 -> `None` + `Error::InvalidUtf8`
pub struct RequiredField {
    /// The struct field (e.g., `db_url`)
    pub name: Ident,

    /// The field's type (e.g., `String`, `u16`)
    pub ty: Type,

    /// The environment variable name (e.g., `"DATABASE_URL"`)
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
    /// When set, uses serde deserialization instead of `FromStr`
    pub format: Option<String>,

    /// Custom Validation function name
    pub validate: Option<String>,
}

impl FieldGenerator for RequiredField {
    fn generate_loader(&self) -> QuoteStream {
        let name = &self.name;
        let ty = &self.ty;
        let env_var = &self.env_var;
        let secret = self.secret;

        // Convert type to string for error messages (e.g., "u16")
        let type_name = quote!(#ty).to_string();

        // Generate the loader code
        //
        // WARN: We use qualified paths (std::...) to avoid conflicts
        // with user code that might have imported different items
        quote! {
            // Try to read the environment variable
            let #name: std::option::Option<#ty> = match std::env::var(#env_var) {
                // Env var exists try to parse it
                std::result::Result::Ok(val) => {
                    match val.parse::<#ty>() {
                        // Parse succeeded
                        std::result::Result::Ok(v) => std::option::Option::Some(v),

                        // Parse failed - record error and continue
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

                // Env var doesn't exist or is invalid
                std::result::Result::Err(e) => {
                    match e {
                        // Not set - required field, so this is an error
                        std::env::VarError::NotPresent => {
                            __errors.push(::procenv::Error::missing(#env_var));
                        }

                        // Contains invalid UTF-8 bytes
                        std::env::VarError::NotUnicode(_) => {
                            __errors.push(::procenv::Error::InvalidUtf8 { var: #env_var.to_string() });
                        }
                    }

                    std::option::Option::None
                }
            };
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "proc-macro code generation inherently requires verbose quote! blocks"
    )]
    fn generate_loader_with_external_prefix(&self) -> QuoteStream {
        let name = &self.name;
        let ty = &self.ty;
        let base_var = &self.env_var;
        let secret = self.secret;
        let type_name = quote!(#ty).to_string();
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

                // Get value to parse: env var > profile default > error
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
                                    // Required field with no env var and no profile match = error
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

                // Parse the value
                let #name: std::option::Option<#ty> = match __value_to_parse {
                    std::option::Option::Some(val) => {
                        match val.parse::<#ty>() {
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
                    std::option::Option::None => {
                        __errors.push(::procenv::Error::missing(&#effective_var_ident));
                        std::option::Option::None
                    }
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

                let #name: std::option::Option<#ty> = match std::env::var(&#effective_var_ident) {
                    std::result::Result::Ok(val) => {
                        match val.parse::<#ty>() {
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
                        match e {
                            std::env::VarError::NotPresent => {
                                __errors.push(::procenv::Error::missing(&#effective_var_ident));
                            }
                            std::env::VarError::NotUnicode(_) => {
                                __errors.push(::procenv::Error::InvalidUtf8 {
                                    var: #effective_var_ident.clone(),
                                });
                            }
                        }
                        std::option::Option::None
                    }
                };
            }
        }
    }

    fn generate_assignment(&self) -> QuoteStream {
        let name = &self.name;

        // Safe to unwrap because we check __errors.is_empty() before constructing
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
            required: true,
            default: None,
            secret: self.secret,
            type_hint: quote!(#ty).to_string().replace(' ', ""),
        }]
    }

    fn generate_source_tracking(&self) -> QuoteStream {
        let field_name = &self.name;
        let field_name_str = field_name.to_string();
        let env_var = &self.env_var;

        // Source tracking identifier
        let source_ident = format_ident!("__{}_source", field_name);

        // Check if this field has profile config - if so, check profile flag first
        if self.profile.is_some() {
            let profile_used_ident = format_ident!("__{}_from_profile", field_name);

            quote! {
                let #source_ident = if #profile_used_ident {
                    // Value came from profile - use the profile name
                    ::procenv::ValueSource::new(
                        #env_var,
                        ::procenv::Source::Profile(__profile.clone().unwrap_or_default())
                    )
                } else if #field_name.is_some() {
                    ::procenv::ValueSource::new(
                        #env_var,
                        if __dotenv_loaded {
                            // Check if var existed before dotenv
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
        } else {
            // No profile config - use original logic
            quote! {
                let #source_ident = if #field_name.is_some() {
                    ::procenv::ValueSource::new(
                        #env_var,
                        if __dotenv_loaded {
                            // Check if var existed before dotenv
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
                    ::procenv::ValueSource::new(
                        #env_var,
                        ::procenv::Source::NotSet
                    )
                };

                __sources.add(#field_name_str, #source_ident);
            }
        }
    }

    fn generate_format_loader(&self, format: &str) -> QuoteStream {
        let name = &self.name;
        let env_var = &self.env_var;
        let secret = self.secret;

        let deserialize_call = match format {
            "json" => quote! { ::serde_json::from_str(&val) },

            "toml" => quote! { ::toml::from_str(&val) },

            "yaml" => quote! { ::serde_saphyr::from_str(&val) },

            _ => unreachable!("Format validated at parse time"),
        };

        let format_name = format.to_uppercase();

        quote! {
            let #name = match std::env::var(#env_var) {
                std::result::Result::Ok(val) => {
                    match #deserialize_call {
                        std::result::Result::Ok(v) => std::option::Option::Some(v),

                        std::result::Result::Err(e) => {
                            __errors.push(::procenv::Error::parse(
                                #env_var,
                                val,
                                #secret,
                                concat!(#format_name, " data"),
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
                                var: #env_var.to_string(),
                            });
                        }
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

    fn field_type(&self) -> Option<&Type> {
        Some(&self.ty)
    }
}
