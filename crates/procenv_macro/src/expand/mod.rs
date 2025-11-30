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
//! | `from_env()` | [`env::generate_from_env_impl`] |
//! | `from_env_with_sources()` | [`sources::generate_from_env_with_sources_impl`] |
//! | `from_config()` | [`config::generate_from_config_impl`] |
//! | `from_args()` | [`args::generate_from_args_impl`] |
//! | `env_example()` | [`example::generate_env_example_impl`] |
//! | `impl Debug` | [`debug::generate_debug_impl`] |
//!
//! # Error Accumulation Pattern
//!
//! The generated `from_env()` method uses error accumulation rather than
//! failing on the first error. This provides users a complete picture of
//! all configuration issues at once.

use proc_macro::TokenStream;
use quote::quote;
use syn::punctuated::Punctuated;
use syn::token::Comma;
use syn::{Data, DeriveInput, Error as SynError, Field, Fields, Result as SynResult};

use crate::field::FieldFactory;
use crate::parse::EnvConfigAttr;

// Submodules
pub mod args;
pub mod config;
pub mod debug;
pub mod env;
pub mod example;
pub mod runtime;
pub mod sources;
pub mod validation;

/// The main orchestrator for macro expansion.
pub struct Expander;

impl Expander {
    /// Main entry point for expanding the derive macro.
    pub fn expand(input: DeriveInput) -> SynResult<TokenStream> {
        let struct_name = &input.ident;
        let generics = &input.generics;

        // Parse struct-level #[env_config(...)] attribute
        let env_config_attr = EnvConfigAttr::parse_from_struct(&input)?;

        // Validate and extract the struct's named fields
        let fields = Self::extract_struct_fields(&input)?;

        // Parse each field into a FieldGenerator trait object
        let generators: Vec<Box<dyn crate::field::FieldGenerator>> = fields
            .iter()
            .map(|f| FieldFactory::parse_field(f, env_config_attr.prefix.as_deref()))
            .collect::<SynResult<Vec<_>>>()?;

        let from_env_impl =
            env::generate_from_env_impl(struct_name, generics, &generators, &env_config_attr);

        let debug_impl = debug::generate_debug_impl(struct_name, generics, &generators);

        let env_example_impl =
            example::generate_env_example_impl(struct_name, generics, &generators);

        let sources_impl = sources::generate_from_env_with_sources_impl(
            struct_name,
            &generators,
            &env_config_attr,
        );

        // Always generate __config_defaults for nested struct support
        let config_defaults_impl =
            config::generate_config_defaults_impl(struct_name, generics, &generators);

        // Generate file config method if files are configured
        let file_config_impl = if !env_config_attr.files.is_empty() {
            config::generate_from_config_impl(struct_name, generics, &generators, &env_config_attr)
        } else {
            quote! {}
        };

        // Generate validation methods if validate attribute is set
        let validated_impl = if env_config_attr.validate {
            validation::generate_validated_impl(
                struct_name,
                generics,
                &generators,
                &env_config_attr,
            )
        } else {
            quote! {}
        };

        // Generate external prefix method for flatten support
        let external_prefix_impl = env::generate_from_env_with_external_prefix_impl(
            struct_name,
            generics,
            &generators,
            &env_config_attr,
        );

        // Generate runtime access methods
        let runtime_access_impl =
            runtime::generate_runtime_access_impl(struct_name, generics, &generators);

        let combined = quote! {
            #from_env_impl
            #debug_impl
            #env_example_impl
            #sources_impl
            #config_defaults_impl
            #file_config_impl
            #validated_impl
            #external_prefix_impl
            #runtime_access_impl
        };

        Ok(combined.into())
    }

    /// Extract named fields from the struct, rejecting invalid types.
    fn extract_struct_fields(input: &DeriveInput) -> SynResult<&Punctuated<Field, Comma>> {
        match &input.data {
            Data::Struct(data_struct) => match &data_struct.fields {
                // Named fields: struct Foo { bar: i32 }
                Fields::Named(fields_named) => Ok(&fields_named.named),

                // Tuple struct: struct Foo(i32)
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
}
