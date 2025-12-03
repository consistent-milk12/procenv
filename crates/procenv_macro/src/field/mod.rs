//! Field processing and code generation for `EnvConfig` derive macro.
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

// Field type implementations
mod default;
mod flatten;
mod optional;
mod required;
mod secret;

pub use default::DefaultField;
pub use flatten::FlattenField;
pub use optional::OptionalField;
pub use required::RequiredField;
pub use secret::{SecretBoxField, SecretKind, SecretStringField};

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
    /// Used for CLI parse errors to show "expected u16" instead of "expected `ParseIntError`".
    fn type_name(&self) -> String;

    /// Whether this field is marked as secret.
    ///
    /// Secret fields are masked in Debug output ("***") and error messages
    /// ("<redacted>") to prevent accidental exposure of sensitive data.
    fn is_secret(&self) -> bool;

    /// Whether this field uses a secrecy crate type (`SecretString`, `SecretBox`).
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

    /// Generate code fragment for `env_example()` method.
    ///
    /// For regular fields, returns a string literal with the formatted entry.
    /// For flattened fields, returns a call to the nested type's `env_example()`.
    fn generate_example_fragment(&self) -> QuoteStream {
        // Default implementation for regular fields
        let entries = self.example_entries();
        if entries.is_empty() {
            return quote! {};
        }

        let formatted: Vec<String> = entries.iter().map(EnvExampleEntry::format).collect();
        let combined = formatted.join("\n");
        quote! { #combined }
    }

    /// Generate format-aware loader using serde deserialization.
    ///
    /// This is like `generate_loader()` but sues serde (json/toml/yaml)
    /// instead of `FromStr`. Each field type implements this to maintain
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

    /// Generate code to load this field's value with an external prefix.
    ///
    /// This is like `generate_loader()` but prepends `__external_prefix` to the
    /// env var name at runtime. Used by `__from_env_with_external_prefix()`.
    ///
    /// The generated code should use:
    /// ```ignore
    /// let __effective_var = format!("{}{}", __external_prefix.unwrap_or(""), "BASE_VAR");
    /// match std::env::var(&__effective_var) { ... }
    /// ```
    fn generate_loader_with_external_prefix(&self) -> QuoteStream {
        // Default: same as regular loader (for fields that don't use env vars)
        self.generate_loader()
    }

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

    /// Returns the prefix for flatten fields.
    ///
    /// Used to prepend a prefix to nested env var names.
    #[allow(dead_code)]
    fn flatten_prefix(&self) -> Option<&str> {
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
            ::procenv::clap::Arg::new(#name_str)
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

    /// Returns the field name identifier (alias for `name()`).
    /// Used for runtime access generation.
    fn field_name(&self) -> Option<&Ident> {
        Some(self.name())
    }

    /// Whether this field is optional (type is `Option<T>`).
    fn is_optional(&self) -> bool {
        false
    }
}

// ============================================================================
// FieldFactory
// ============================================================================

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
        if let FieldConfig::Flatten {
            prefix: flatten_prefix,
        } = field_config
        {
            // Flatten fields only get a prefix if explicitly specified via `prefix = "..."`
            // By default, flatten fields DON'T inherit the parent struct's prefix
            // (this maintains backwards compatibility)
            //
            // When a prefix IS specified on the flatten field:
            // - If struct has prefix = "APP_" and flatten has prefix = "DB_",
            //   the effective prefix for nested fields is "APP_DB_"
            // - If no struct prefix, just the flatten prefix is used
            let effective_prefix = match flatten_prefix {
                Some(field_prefix) => {
                    // Combine struct prefix (if any) with the flatten prefix
                    match prefix {
                        Some(struct_prefix) => Some(format!("{struct_prefix}{field_prefix}")),
                        None => Some(field_prefix),
                    }
                }
                None => None, // No prefix propagation by default
            };

            return Ok(Box::new(FlattenField {
                name,
                ty,
                prefix: effective_prefix,
            }));
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
