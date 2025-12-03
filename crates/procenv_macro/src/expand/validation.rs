//! Validation code generation.
//!
//! This module generates validation methods that integrate with the `validator`
//! crate to validate configuration values after loading.
//!
//! # Generated Methods
//!
//! - [`generate_validated_impl`] - Generates:
//!   - `from_env_validated()` - Load and validate from environment
//!   - `from_env_validated_with_sources()` - With source attribution
//!
//! # Requirements
//!
//! For validation to be generated, the struct must:
//! 1. Have `#[env_config(validate)]` attribute
//! 2. Also derive `validator::Validate`
//!
//! ```rust,ignore
//! #[derive(EnvConfig, Validate)]
//! #[env_config(validate)]
//! struct Config {
//!     #[env(var = "PORT", default = "8080")]
//!     #[validate(range(min = 1, max = 65535))]
//!     port: u16,
//! }
//! ```
//!
//! # Validation Flow
//!
//! 1. Load configuration using `from_env()` (may fail with Missing/Parse errors)
//! 2. Run `validator::Validate::validate()` on the loaded struct
//! 3. Run any custom field validators (`#[env(validate = "fn_name")]`)
//! 4. Accumulate all validation errors
//! 5. Return `Error::Validation` if any validations failed
//!
//! # Custom Validators
//!
//! Fields can specify custom validation functions:
//!
//! ```rust,ignore
//! #[env(var = "NAME", validate = "validate_name")]
//! name: String,
//!
//! fn validate_name(name: &str) -> Result<(), validator::ValidationError> {
//!     if name.len() < 3 {
//!         return Err(validator::ValidationError::new("too_short"));
//!     }
//!     Ok(())
//! }
//! ```

use proc_macro2::TokenStream as QuoteStream;
use quote::{format_ident, quote};
use syn::{Generics, Ident};

use crate::field::FieldGenerator;
use crate::parse::EnvConfigAttr;

/// Generate the `from_env_validated` method for structs with validation.
pub fn generate_validated_impl(
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

    quote! {
        impl #impl_generics #struct_name #type_generics #where_clause
        where
            Self: ::procenv::Validate,
        {
            /// Load configuration from environment variables with validation.
            pub fn from_env_validated() -> std::result::Result<Self, ::procenv::Error> {
                let __config = Self::from_env()?;

                let mut __validation_errors: Vec<::procenv::ValidationFieldError> = Vec::new();

                if let Err(e) = ::procenv::Validate::validate(&__config) {
                    __validation_errors.extend(::procenv::validation_errors_to_procenv(&e));
                }

                #(#custom_validations)*

                if !__validation_errors.is_empty() {
                    return Err(::procenv::Error::Validation {
                        errors: __validation_errors,
                    });
                }

                Ok(__config)
            }

            pub fn from_env_validated_with_sources() -> std::result::Result<(Self, ::procenv::ConfigSources), ::procenv::Error> {
                let (__config, __sources) = Self::from_env_with_sources()?;

                let mut __validation_errors: Vec<::procenv::ValidationFieldError> = Vec::new();

                if let Err(e) = ::procenv::Validate::validate(&__config) {
                    __validation_errors.extend(::procenv::validation_errors_to_procenv(&e));
                }

                #(#custom_validations)*

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
#[allow(dead_code)]
pub fn generate_from_config_validated_impl(
    struct_name: &Ident,
    generics: &Generics,
    generators: &[Box<dyn FieldGenerator>],
) -> QuoteStream {
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();

    // Generate custom field validation calls
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
        impl #impl_generics #struct_name #type_generics #where_clause
        where
            Self: ::procenv::Validate + ::procenv::serde::de::DeserializeOwned,
        {
            /// Load configuration from files with validation.
            pub fn from_config_validated() -> std::result::Result<Self, ::procenv::Error> {
                let __config = Self::from_config()?;

                let mut __validation_errors: Vec<::procenv::ValidationFieldError> = Vec::new();

                if let Err(e) = ::procenv::Validate::validate(&__config) {
                    __validation_errors.extend(::procenv::validation_errors_to_procenv(&e));
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

                if let Err(e) = ::procenv::Validate::validate(&__config) {
                    __validation_errors.extend(::procenv::validation_errors_to_procenv(&e));
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
