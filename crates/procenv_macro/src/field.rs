//! Field processing and code generation for EnvConfig derive macro.
//!
//! This module is the heart of the code generation logic. It classifies
//! struct fields based on their attributes and generates appropriate
//! loading code for each field type.
//!
//! # Field Types
//!
//! Fields are classified into one of these types based on attributes:
//!
//! | Type | Attribute | Behavior |
//! |------|-----------|----------|
//! | [`RequiredField`] | `var = "..."` only | Errors if missing |
//! | [`DefaultField`] | `default = "..."` | Uses default if missing |
//! | [`OptionalField`] | `optional` | Returns `None` if missing |
//! | [`FlattenField`] | `flatten` | Loads nested `EnvConfig` struct |
//! | [`SecretStringField`] | `SecretString` type | Wraps in `SecretString` |
//! | [`SecretBoxField`] | `SecretBox<T>` type | Wraps in `SecretBox<T>` |
//!
//! # Architecture
//!
//! This module uses the Strategy pattern via trait objects. The [`FieldGenerator`]
//! trait defines the interface for code generation, and each field type provides
//! its own implementation.
//!
//! ```text
//! ┌─────────────────┐
//! │   syn::Field    │  Raw AST from derive input
//! └────────┬────────┘
//!          │
//!          ▼
//! ┌─────────────────┐
//! │  FieldFactory   │  Parses attributes and classifies field
//! │  ::parse_field  │
//! └────────┬────────┘
//!          │  Box<dyn FieldGenerator>
//!          ▼
//! ┌─────────────────┐
//! │ FieldGenerator  │  Trait object for code generation
//! │  implementations│
//! └────────┬────────┘
//!          │
//!          ├─────────────────────────────────────┐
//!          ▼                                     ▼
//! ┌─────────────────┐                   ┌─────────────────┐
//! │ generate_loader │  Env var reading  │ generate_assign │  Struct init
//! └─────────────────┘                   └─────────────────┘
//! ```
//!
//! # Error Accumulation
//!
//! All field types follow the error accumulation pattern:
//! - Errors are pushed to a `Vec<Error>` rather than returning early
//! - The local variable is `Option<T>` to allow continuing after errors
//! - At the end, if errors exist, they're returned; otherwise `.unwrap()` is safe

use proc_macro2::TokenStream as QuoteStream;
use quote::{format_ident, quote};
use syn::{
    Error as SynError, Field, GenericArgument, Ident, PathArguments, Result as SynResult, Type,
};

use crate::parse::{CliAttr, FieldConfig, Parser, ProfileAttr, extract_doc_comment};

// ============================================================================
// EnvExampleEntry - Info for .env.example generation
// ============================================================================

/// Information about an environment variable for `.env.example` generation.
///
/// This struct contains all the metadata needed to generate a single entry
/// in a `.env.example` template file. It is returned by
/// [`FieldGenerator::example_entries()`].
///
/// # Generated Format
///
/// The [`format()`](Self::format) method produces output like:
///
/// ```text
/// # Database connection URL (required, type: String)
/// DATABASE_URL=
///
/// # Server port (type: u16)
/// # PORT=8080
/// ```
///
/// - Required fields without defaults show `VAR=`
/// - Fields with defaults show `# VAR=default` (commented out)
/// - Doc comments, requirements, and type hints appear as comments above
#[derive(Clone, Debug)]
pub struct EnvExampleEntry {
    /// The environment variable name (with prefix applied if configured).
    pub var_name: String,

    /// Documentation comment extracted from the field's doc attributes.
    pub doc: Option<String>,

    /// Whether this field is required (no default, not optional).
    pub required: bool,

    /// Default value if the field has one.
    pub default: Option<String>,

    /// Whether this field is marked as secret.
    pub secret: bool,

    /// Type name for documentation hints (e.g., `"u16"`, `"String"`).
    pub type_hint: String,
}

impl EnvExampleEntry {
    /// Format this entry as a line (or lines) for .env.example
    pub fn format(&self) -> String {
        let mut lines = Vec::new();

        // Build the comment line
        let mut comment_parts = Vec::new();

        if let Some(doc) = &self.doc {
            comment_parts.push(doc.clone());
        }

        // Add metadata
        let mut meta = Vec::new();
        if self.required {
            meta.push("required".to_string());
        }
        if self.secret {
            meta.push("secret".to_string());
        }
        meta.push(format!("type: {}", self.type_hint));

        if !meta.is_empty() {
            comment_parts.push(format!("({})", meta.join(", ")));
        }

        if !comment_parts.is_empty() {
            lines.push(format!("# {}", comment_parts.join(" ")));
        }

        // Build the variable line
        if let Some(default) = &self.default {
            // Has default - show commented out with default value
            lines.push(format!("# {}={}", self.var_name, default));
        } else {
            // Required or optional without default - show empty
            lines.push(format!("{}=", self.var_name));
        }

        lines.join("\n")
    }
}

// ============================================================================
// FieldGenerator Trait
// ============================================================================

/// Trait for generating field-specific code in the derive macro.
///
/// This trait is the core abstraction that enables different field types
/// to be handled polymorphically. Each implementation generates appropriate
/// code for reading environment variables, handling errors, and constructing
/// the struct.
///
/// # Implementations
///
/// | Type | When Used |
/// |------|-----------|
/// | [`RequiredField`] | `#[env(var = "...")]` with no default |
/// | [`DefaultField`] | `#[env(var = "...", default = "...")]` |
/// | [`OptionalField`] | `#[env(var = "...", optional)]` |
/// | [`FlattenField`] | `#[env(flatten)]` |
/// | [`SecretStringField`] | Field type is `SecretString` |
/// | [`SecretBoxField`] | Field type is `SecretBox<T>` |
///
/// # Generated Code Pattern
///
/// Each implementation generates code following this pattern:
///
/// ```ignore
/// // Loader code (generate_loader)
/// let field_name: Option<T> = match std::env::var("VAR") {
///     Ok(val) => { /* parse and return Some(v) or push error */ }
///     Err(e) => { /* push error if required, return None */ }
/// };
///
/// // Assignment code (generate_assignment)
/// Self { field_name: field_name.unwrap() }  // for required/default
/// Self { field_name }                        // for optional
/// ```
pub trait FieldGenerator {
    /// Generate code to load this field's value from the environment.
    ///
    /// The generated code should:
    /// 1. Call `std::env::var()` to read the environment variable
    /// 2. Parse the value using `.parse::<T>()`
    /// 3. Push any errors to `__errors` vector
    /// 4. Store the result in a local variable as `Option<T>`
    ///
    /// The variable being `Option<T>` allows error accumulation - we can
    /// continue loading other fields even if this one fails.
    fn generate_loader(&self) -> QuoteStream;

    /// Generate the struct field assignment expression.
    ///
    /// For required/default fields: `field_name: field_name.unwrap()`
    /// For optional fields: `field_name` (already Option<T>)
    fn generate_assignment(&self) -> QuoteStream;

    /// Returns the field name identifier.
    ///
    /// Used by the expander for generating Debug impl and error messages.
    fn name(&self) -> &Ident;

    /// Returns the field's type as a string for error messages.
    ///
    /// Used for CLI parse errors to show "expected u16" instead of "expected ParseIntError".
    fn type_name(&self) -> String;

    /// Whether this field is marked as secret.
    ///
    /// Secret fields are masked in Debug output ("***") and error messages
    /// ("<redacted>") to prevent accidental exposure of sensitive data.
    fn is_secret(&self) -> bool;

    /// Whether this field uses a secrecy crate type (SecretString, SecretBox).
    ///
    /// When true, the expander skips generating custom Debug field entries
    /// because secrecy types handle their own Debug redaction.
    fn is_secrecy_type(&self) -> bool {
        false // Default: not a secrecy type
    }

    /// Returns entries for .env.example generation.
    ///
    /// For regular fields, returns a single entry.
    /// For flattened fields, returns an empty vec (handled specially).
    fn example_entries(&self) -> Vec<EnvExampleEntry>;

    /// Generate code fragment for env_example() method.
    ///
    /// For regular fields, returns a string literal with the formatted entry.
    /// For flattened fields, returns a call to the nested type's env_example().
    fn generate_example_fragment(&self) -> QuoteStream {
        // Default implementation for regular fields
        let entries = self.example_entries();
        if entries.is_empty() {
            return quote! {};
        }

        let formatted: Vec<String> = entries.iter().map(|e| e.format()).collect();
        let combined = formatted.join("\n");
        quote! { #combined }
    }

    /// Generate format-aware loader using serde deserialization.
    ///
    /// This is like `generate_loader()` but sues serde (json/toml/yaml)
    /// instead of FromStr. Each field type implements this to maintain
    /// proper optional/default/required semantics.
    fn generate_format_loader(&self, format: &str) -> QuoteStream {
        // Default implementation for fields that don't override.
        //  WARN: This should never be called if format_config() return None.
        let _ = format;
        self.generate_loader()
    }

    /// Whether this is a flattened field that delegates to a nested type.
    fn is_flatten(&self) -> bool {
        false
    }

    fn generate_source_tracking(&self) -> QuoteStream;

    fn env_var_name(&self) -> Option<&str>;

    /// Returns CLI configuration if this field has CLI argument support.
    fn cli_config(&self) -> Option<&CliAttr> {
        None
    }

    /// Returns profile configuration if this field has profile-specific defaults.
    fn profile_config(&self) -> Option<&ProfileAttr> {
        None
    }

    /// Returns the default value if this field has one.
    ///
    /// Used by profile loader to fall back to default when profile doesn't match.
    fn default_value(&self) -> Option<&str> {
        None
    }

    /// Returns format configuration if this field uses serde deserialization.
    fn format_config(&self) -> Option<&str> {
        None
    }

    /// Returns the field's type for flatten fields.
    ///
    /// Used to generate calls to nested types' methods (e.g., `__config_defaults()`).
    fn field_type(&self) -> Option<&Type> {
        None
    }

    /// Returns custom validation function name if specified.
    fn validate_fn(&self) -> Option<&str> {
        None
    }

    /// Generate clap Arg definition for this field (if CLI-enabled).
    fn generate_clap_arg(&self) -> Option<QuoteStream> {
        let cli = self.cli_config()?;
        let long = cli.long.as_ref()?;
        let name = self.name();
        let name_str = name.to_string();

        let short_attr = if let Some(short) = cli.short {
            quote! { .short(#short) }
        } else {
            quote! {}
        };

        Some(quote! {
            ::clap::Arg::new(#name_str)
                .long(#long)
                #short_attr
                .value_name(#name_str)
        })
    }

    /// Generate code to extract CLI value for this field.
    /// Returns code that sets a local variable `__{name}_cli: Option<String>`.
    fn generate_cli_extraction(&self) -> Option<QuoteStream> {
        let cli = self.cli_config()?;
        let _long = cli.long.as_ref()?;
        let name = self.name();
        let cli_var = format_ident!("__{}_cli", name);
        let name_str = name.to_string();

        Some(quote! {
            let #cli_var: std::option::Option<std::string::String> = __matches
                .get_one::<std::string::String>(#name_str)
                .cloned();
        })
    }
}

// ============================================================================
// RequiredField Implementation
// ============================================================================

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
    /// When set, uses serde deserialization instead of FromStr
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

                        // Prase failed - record error and continue
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
                            __errors.push(::procenv::Error::InvalidUtf8 { var: #env_var });
                        }
                    }

                    std::option::Option::None
                }
            };
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
                    match e {
                        std::env::VarError::NotPresent => {
                            __errors.push(::procenv::Error::missing(#env_var));
                        }

                        std::env::VarError::NotUnicode(_) => {
                            __errors.push(::procenv::Error::InvalidUtf8 {
                                var: #env_var,
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
}

// ============================================================================
// DefaultField Implementation
// ============================================================================

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
                            var: #env_var,
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
                            var: #env_var,
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

// ============================================================================
// OptionalField Implementation
// ============================================================================

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
                        __errors.push(::procenv::Error::InvalidUtf8 { var: #env_var });
                    }

                    // Return None for both NotPresent and NotUnicode
                    std::option::Option::None
                }
            };
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
                            var: #env_var,
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

// ============================================================================
// SecretString Field Implementation
// ============================================================================

/// A field of type `SecretString` for secret string values.
///
/// SecretString is a type alias for `SecretBox<str>` from the secrecy crate.
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
                                var: #env_var
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
}

// ============================================================================
// SecretBox<T> Field Implementation
// ============================================================================

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

    /// The inner type T of SecretBox<T>
    pub inner_type: Type,

    /// The environment variable name
    pub env_var: String,

    /// Doc comment from the field
    pub doc: Option<String>,
}

// ============================================================================
// FlattenField Implementation
// ============================================================================

/// A flattened nested config field.
///
/// The nested type must also derive `EnvConfig`. Its `from_env()` is called
/// and any errors are merged into the parent's error list.
///
/// ## Behavior
/// - Calls `NestedType::from_env()` on the nested struct
/// - If successful -> `Some(value)`
/// - If errors occur -> merges them into parent's `__errors` and returns `None`
pub struct FlattenField {
    /// The struct field name
    pub name: Ident,

    /// The field's type (the nested config struct)
    pub ty: Type,
}

impl FieldGenerator for FlattenField {
    fn generate_loader(&self) -> QuoteStream {
        let field_name = &self.name;
        let ty = &self.ty;

        let nested_sources_ident = format_ident!("__{}_nested_sources", field_name);

        quote! {
            let (#field_name, #nested_sources_ident): (
                std::option::Option<#ty>,
                ::procenv::ConfigSources
            ) = match <#ty>::from_env_with_sources() {
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
}

/// Factory for creating field generators from parsed field definitions.
///
/// This struct provides the [`parse_field`](Self::parse_field) method that
/// examines a field's type and attributes to create the appropriate
/// [`FieldGenerator`] implementation.
///
/// # Classification Logic
///
/// ```text
/// Field
///   │
///   ├─► Has `flatten` attr? ──► FlattenField
///   │
///   ├─► Type is SecretString? ──► SecretStringField
///   │
///   ├─► Type is SecretBox<T>? ──► SecretBoxField
///   │
///   ├─► Has `optional` attr? ──► OptionalField
///   │
///   ├─► Has `default` attr? ──► DefaultField
///   │
///   └─► Otherwise ──► RequiredField
/// ```
pub struct FieldFactory;

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
                                var: #env_var
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
}

/// The kind of secret field detected from the type.
#[derive(Clone, Debug)]
pub enum SecretKind {
    /// `SecretString` (alias for `SecretBox<str>`)
    String,

    /// `SecretBox<T>` with the inner type (boxed to reduce enum size)
    Box(Box<Type>),
}

impl FieldFactory {
    /// Factory function to parse a `syn::Field` and return the appropriate `FieldGenerator`.
    ///
    /// This function:
    /// 1. Extracts the field name and type from the syn AST
    /// 2. Parses the `#[env(...)]` attribute
    /// 3. Creates the appropriate field generator based on the attributes
    ///
    /// ## Field Type Selection
    ///
    /// - `flatten` attribute → `FlattenField` (nested config)
    /// - `optional` attribute → `OptionalField` (validates that type is `Option<T>`)
    /// - `default` attribute → `DefaultField`
    /// - Neither → `RequiredField`
    pub fn parse_field(field: &Field, prefix: Option<&str>) -> SynResult<Box<dyn FieldGenerator>> {
        // Extract field name (unwrap is safe for named struct fields)
        let name = field.ident.clone().unwrap();
        let ty = field.ty.clone();

        // Extract doc comment for .env.example generation
        let doc = extract_doc_comment(field);

        // Parse the #[env(...)] attribute
        let field_config = Parser::parse_field_config(field)?;

        // Handle flatten fields separately - they don't use env vars directly
        if let FieldConfig::Flatten = field_config {
            return Ok(Box::new(FlattenField { name, ty }));
        }

        // Extract EnvAttr for regular fields
        let FieldConfig::Env(env_attr) = field_config else {
            unreachable!()
        };

        // Apply prefix to var name (unless no_prefix is set)
        let env_var = if !env_attr.no_prefix
            && let Some(prefix_val) = prefix
        {
            format!("{}{}", prefix_val, env_attr.var_name)
        } else {
            env_attr.var_name
        };

        if let Some(secret_kind) = Self::extract_secret_kind(&ty) {
            return match secret_kind {
                SecretKind::String => Ok(Box::new(SecretStringField { name, env_var, doc })),

                SecretKind::Box(inner_type) => Ok(Box::new(SecretBoxField {
                    name,
                    inner_type: *inner_type,
                    env_var,
                    doc,
                })),
            };
        }

        let secret = env_attr.secret;
        let cli = env_attr.cli;
        let profile = env_attr.profile;
        let format = env_attr.format;
        let validate = env_attr.validate;

        // Choose the appropriate field generator based on attributes
        if env_attr.optional {
            // Optional field - must be Option<T>
            let inner_type = Self::extract_option_inner(&ty)
                .ok_or_else(|| {
                    SynError::new_spanned(&ty, "Field marked `optional` must have type `Option<T>`")
                })?
                .clone();

            Ok(Box::new(OptionalField {
                name,
                inner_type,
                env_var,
                secret,
                doc,
                cli,
                profile,
                format,
                validate,
            }))
        } else if let Some(default) = env_attr.default {
            // Default field
            Ok(Box::new(DefaultField {
                name,
                ty,
                env_var,
                default,
                secret,
                doc,
                cli,
                profile,
                format,
                validate,
            }))
        } else {
            // Required field (the default)
            Ok(Box::new(RequiredField {
                name,
                ty,
                env_var,
                secret,
                doc,
                cli,
                profile,
                format,
                validate,
            }))
        }
    }

    pub fn extract_secret_kind(ty: &Type) -> Option<SecretKind> {
        let Type::Path(type_path) = ty else {
            return None;
        };

        let segment = type_path.path.segments.last()?;

        // Check for SecretString (no generic args)
        if segment.ident == "SecretString" {
            return Some(SecretKind::String);
        }

        // Check for SecretBox<T>
        if segment.ident == "SecretBox" {
            let PathArguments::AngleBracketed(args) = &segment.arguments else {
                return None;
            };

            let GenericArgument::Type(inner) = args.args.first()? else {
                return None;
            };

            return Some(SecretKind::Box(Box::new(inner.clone())));
        }

        None
    }

    /// Check if a type is `Option<T>` and extract the inner type `T`.
    ///
    /// This is used to validate optional fields and to generate correct
    /// parsing code (we need to parse `T`, not `Option<T>`).
    ///
    /// ## Examples
    ///
    /// - `Option<String>` → `Some(&Type::Path("String"))`
    /// - `Option<u16>` → `Some(&Type::Path("u16"))`
    /// - `String` → `None`
    /// - `Vec<String>` → `None`
    pub fn extract_option_inner(ty: &Type) -> Option<&Type> {
        // Must be a Type::Path (e.g., `Option<T>`, not `&str` or `[u8]`)
        let Type::Path(type_path) = ty else {
            return None;
        };

        // Get the last segment (handles `std::option::Option<T>` as well as `Option<T>`)
        let segment = type_path.path.segments.last()?;

        // Check it's named "Option"
        if segment.ident != "Option" {
            return None;
        }

        // Must have angle bracket arguments (the `<T>` part)
        let PathArguments::AngleBracketed(args) = &segment.arguments else {
            return None;
        };

        // Get the first (and should be only) type argument
        let GenericArgument::Type(inner) = args.args.first()? else {
            return None;
        };

        Some(inner)
    }
}
