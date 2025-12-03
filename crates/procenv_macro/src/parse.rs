//! Attribute parsing for `#[env(var = "VAR", default = "value", optional, secret)]`
//!
//! This module is responsible for extracting configuration from field attributes.
//! It uses syn's `ParseNestedMeta` for robust, extensible parsing with:
//! - Automatic comma and trailing comma handling
//! - Precise error spans pointing to the exact problematic token
//! - Duplicate option detection
//!
//! # Architecture
//!
//! The parsing system uses a **builder pattern** with two-phase validation:
//!
//! 1. **Accumulation phase**: [`Parser::parse_meta()`] collects options one at a time
//! 2. **Validation phase**: [`Parser::build_config()`] validates combinations and builds output
//!
//! This separation allows us to:
//! - Report ALL errors at once (not just the first one)
//! - Handle option interdependencies cleanly
//! - Support both `flatten` and regular field modes with shared parsing
//!
//! # Design Decisions
//!
//! ## Why `HashSet<&'static str>` for duplicate detection?
//!
//! We use `&'static str` keys instead of `String` to avoid allocations during
//! the hot path of attribute parsing. The match statement in `parse_meta()` maps
//! dynamic strings to static references, enabling O(1) hash comparisons without
//! heap allocation.
//!
//! ## Why single-pass doc comment extraction?
//!
//! [`extract_doc_comment()`] builds the result string incrementally rather than
//! collecting into a `Vec<String>` and joining. This reduces allocations from
//! N+1 (N strings + 1 joined result) to just 1 (the final string).
//!
//! ## Why collect-then-report for flatten validation?
//!
//! Instead of returning on the first incompatible option, we collect ALL
//! incompatible options and report them together. This provides better UX:
//! users see everything they need to fix in one error message.
//!
//! # Supported Syntax
//!
//! ## Field-level attributes
//!
//! ```ignore
//! #[env(var = "ENV_VAR_NAME")]                           // Required field
//! #[env(var = "ENV_VAR_NAME", default = "fallback")]     // With default
//! #[env(var = "ENV_VAR_NAME", optional)]                 // Option<T> field
//! #[env(var = "ENV_VAR_NAME", secret)]                   // Masked in output
//! #[env(var = "ENV_VAR_NAME", secret, default = "key")]  // Combinable
//! #[env(flatten)]                                        // Nested config
//! #[env(flatten, prefix = "DB_")]                        // Nested with prefix
//! ```
//!
//! ## Struct-level attributes
//!
//! ```ignore
//! #[env_config(prefix = "APP_")]                         // Prefix for all vars
//! #[env_config(dotenv)]                                  // Load .env file
//! #[env_config(file = "config.toml")]                    // Load config file
//! #[env_config(profile_env = "APP_ENV", profiles = ["dev", "prod"])]
//! ```
//!
//! ## Profile Support (Phase 16)
//!
//! ```ignore
//! #[env_config(profile_env = "APP_ENV", profiles = ["dev", "staging", "prod"])]
//! struct Config {
//!     #[env(var = "DATABASE_URL")]
//!     #[profile(dev = "postgres://localhost/dev", prod = "postgres://prod/app")]
//!     database_url: String,
//! }
//! ```

use std::collections::{HashMap, HashSet};

use syn::meta::ParseNestedMeta;
use syn::punctuated::Punctuated;
use syn::token::Comma;
use syn::{
    Attribute, DeriveInput, Error as SynError, Expr, ExprLit, Field, Lit, LitStr, Meta,
    Result as SynResult, bracketed,
};

/// Extract doc comments from a field's attributes.
///
/// Doc comments like `/// This is a comment` become `#[doc = "This is a comment"]`
/// attributes. This function extracts and joins all doc comments into a single string.
///
/// # Algorithm
///
/// Uses single-pass incremental string building instead of collect-then-join:
///
/// ```text
/// Traditional approach (N+1 allocations):
///   attrs → filter_map → Vec<String> → join(" ") → String
///
/// Optimized approach (1 allocation):
///   attrs → for loop → push_str to single String
/// ```
///
/// For typical 1-3 doc comment lines, this eliminates 2-4 intermediate allocations.
/// The string grows via `push`/`push_str`, which amortizes reallocation cost.
pub fn extract_doc_comment(field: &Field) -> Option<String> {
    let mut result = String::new();

    for attr in &field.attrs {
        // Skip non-doc attributes early (most common case)
        if !attr.path().is_ident("doc") {
            continue;
        }

        // Parse #[doc = "..."] using let-chains for clean pattern matching
        // Doc comments are always NameValue meta with string literal values
        if let Meta::NameValue(meta) = &attr.meta
            && let Expr::Lit(ExprLit {
                lit: Lit::Str(lit_str),
                ..
            }) = &meta.value
        {
            // Add space separator between multiple doc lines
            if !result.is_empty() {
                result.push(' ');
            }
            result.push_str(lit_str.value().trim());
        }
    }

    // Use bool::then_some for idiomatic Option construction
    (!result.is_empty()).then_some(result)
}

/// Distinguishes between regular environment variable fields and flattened nested configs.
///
/// This enum is the result of parsing a field's `#[env(...)]` attribute.
/// It determines how the field's value will be loaded.
///
/// # Variants
///
/// - `Env(EnvAttr)` - Regular field loaded from an environment variable
/// - `Flatten` - Nested config struct whose fields are loaded recursively
///
/// # Example
///
/// ```ignore
/// #[env(var = "DATABASE_URL")]  // → FieldConfig::Env(...)
/// database_url: String,
///
/// #[env(flatten)]               // → FieldConfig::Flatten { prefix: None }
/// database: DatabaseConfig,
///
/// #[env(flatten, prefix = "DB_")]  // → FieldConfig::Flatten { prefix: Some("DB_") }
/// database: DatabaseConfig,
/// ```
pub enum FieldConfig {
    /// Regular field loaded from an environment variable.
    ///
    /// Contains all the parsed options from `#[env(var = "...", ...)]`.
    Env(EnvAttr),

    /// Flattened nested configuration struct.
    ///
    /// The nested type must also derive `EnvConfig`. Its fields are
    /// loaded recursively and errors are merged into the parent.
    ///
    /// The optional `prefix` is prepended to all nested field env var names.
    Flatten {
        /// Optional prefix to prepend to nested field env var names.
        /// Combined with any parent prefix and the struct's own prefix.
        prefix: Option<String>,
    },
}

/// CLI argument configuration for a field.
///
/// Parsed from `arg` and `short` options in `#[env(...)]` attribute.
/// When present, enables the `from_args()` method for CLI parsing.
///
/// # Example
///
/// ```ignore
/// #[env(var = "PORT", arg = "port", short = 'p')]
/// port: u16,
/// // Enables: --port 8080 or -p 8080
/// ```
#[derive(Clone, Debug, Default)]
pub struct CliAttr {
    /// The long argument name (e.g., `"port"` for `--port`).
    pub long: Option<String>,

    /// Optional short flag (e.g., `'p'` for `-p`).
    pub short: Option<char>,
}

/// Profile-specific default values for a field.
///
/// Parsed from the `#[profile(...)]` attribute on a field.
/// Allows different default values based on the active profile
/// (e.g., dev, staging, prod).
///
/// # Example
///
/// ```ignore
/// #[env(var = "DATABASE_URL")]
/// #[profile(dev = "postgres://localhost/dev", prod = "postgres://prod/app")]
/// database_url: String,
/// ```
///
/// When `APP_ENV=dev`, the default becomes `postgres://localhost/dev`.
#[derive(Clone, Debug, Default)]
pub struct ProfileAttr {
    /// Map of profile name to default value for that profile.
    ///
    /// Example: `{"dev": "localhost", "prod": "prod-db.internal"}`
    pub values: HashMap<String, String>,
}

/// The parsed result of an `#[env(...)]` attribute.
///
/// This struct contains all configuration options extracted from a field's
/// attribute. It is created by [`Parser::parse_field_config()`].
///
/// # Field Options
///
/// | Option | Type | Description |
/// |--------|------|-------------|
/// | `var` | Required | Environment variable name |
/// | `default` | Optional | Default value if env var missing |
/// | `optional` | Flag | Field becomes `Option<T>` |
/// | `secret` | Flag | Mask value in output |
/// | `no_prefix` | Flag | Skip struct-level prefix |
/// | `arg` | Optional | CLI argument name |
/// | `short` | Optional | CLI short flag |
/// | `format` | Optional | Serde format (json/toml/yaml) |
pub struct EnvAttr {
    /// The name of the environment variable to read (required).
    /// Example: `var = "DATABASE_URL"` → `var_name = "DATABASE_URL"`
    pub var_name: String,

    /// Optional default value if the environment variable is not set.
    /// Example: `default = "8080"` → `default = Some("8080")`
    pub default: Option<String>,

    /// Whether this field is optional (field type must be `Option<T>`).
    /// If true, missing env var results in `None` instead of error.
    pub optional: bool,

    /// Whether this field contains sensitive data.
    /// If true, the value is masked as "***" in Debug output and
    /// "<redacted>" in error messages.
    pub secret: bool,

    /// Skip the struct-level prefix for this field
    pub no_prefix: bool,

    /// CLI argument configuration (Phase 14)
    /// Example: `#[env(var = "PORT", arg = "port", short = 'p')]`
    pub cli: Option<CliAttr>,

    /// Profile-specific default values (Phase 16)
    /// Parsed from separate `#[profile(...)]` attribute
    pub profile: Option<ProfileAttr>,

    /// Deserialization format for structured data (Phase 17)
    /// Example: `#[env(var = "HOSTS", format = "json")]`
    /// Supported: "json", "toml", "yaml"
    pub format: Option<String>,

    /// Custom validation function name.
    /// Example: `#[env(var = "...", validate = "my_validator")]`
    pub validate: Option<String>,
}

/// Builder pattern parser for `#[env(...)]` attributes.
///
/// This struct accumulates parsed options as we iterate through the attribute's
/// contents, then validates and builds the final [`EnvAttr`] or [`FieldConfig`].
///
/// # Architecture
///
/// The parser uses a two-phase approach:
///
/// ```text
/// Phase 1: Accumulation (parse_meta)
/// ┌─────────────────────────────────────────────────────────────┐
/// │  #[env(var = "X", default = "Y", secret)]                   │
/// │         │              │            │                       │
/// │         ▼              ▼            ▼                       │
/// │  parse_meta()   parse_meta()  parse_meta()                  │
/// │         │              │            │                       │
/// │         └──────────────┴────────────┘                       │
/// │                        ▼                                    │
/// │              Parser { var_name: Some("X"),                  │
/// │                       default: Some("Y"),                   │
/// │                       secret: true, ... }                   │
/// └─────────────────────────────────────────────────────────────┘
///
/// Phase 2: Validation & Build (build_config → build)
/// ┌──────────────────────────────────────────────────────────────┐
/// │  Parser state                                                │
/// │         │                                                    │
/// │         ▼                                                    │
/// │  build_config() ─── flatten? ─── Yes ──► validate flatten    │
/// │         │                                 constraints        │
/// │         No                                      │            │
/// │         ▼                                       ▼            │
/// │     build() ──────────────────────────► FieldConfig::Flatten │
/// │         │                                                    │
/// │         ▼                                                    │
/// │  FieldConfig::Env(EnvAttr)                                   │
/// └──────────────────────────────────────────────────────────────┘
/// ```
///
/// # Why This Design?
///
/// - **Separation of concerns**: Parsing logic is independent of validation
/// - **Better error messages**: We can collect all issues before reporting
/// - **Extensibility**: Adding new options only requires updating `parse_meta`
///
/// # Validation Rules
///
/// The parser enforces these semantic constraints:
/// - `var` is required for non-flatten fields
/// - `default` and `optional` are mutually exclusive (different "missing" semantics)
/// - `short` requires `arg` to be set (short flag needs a long name)
/// - `flatten` can only be combined with `prefix` (all other options are field-specific)
/// - `format` must be one of: `json`, `toml`, `yaml`
#[derive(Default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "attribute parser naturally has many boolean flags for each supported option"
)]
pub struct Parser {
    /// Accumulated variable name (from `var = "..."`).
    var_name: Option<String>,

    /// Accumulated default value (from `default = "..."`).
    default: Option<String>,

    /// Whether `optional` flag was seen.
    optional: bool,

    /// Whether `secret` flag was seen.
    secret: bool,

    /// Deserialization format (from `format = "json"`).
    format: Option<String>,

    /// Track which options we've seen to detect duplicates.
    ///
    /// Uses `&'static str` for zero-allocation comparison. The match in
    /// `parse_meta()` maps dynamic strings to static references, so hash
    /// lookups don't allocate. With ~11 possible options and small sets,
    /// `HashSet` provides O(1) lookup without measurable overhead.
    seen: HashSet<&'static str>,

    /// Skip the struct-level prefix for this field.
    no_prefix: bool,

    /// Whether this is a flattened nested config.
    flatten: bool,

    /// Prefix for flatten fields (from `prefix = "..."`).
    /// Only valid when `flatten` is true.
    flatten_prefix: Option<String>,

    /// CLI long argument name (from `arg = "..."`).
    arg_long: Option<String>,

    /// CLI short flag (from `short = 'x'`).
    arg_short: Option<char>,

    /// Custom validation function (from `validate = "..."`).
    validate: Option<String>,
}

impl Parser {
    /// Parse a single option from within the attribute.
    ///
    /// Called by `parse_nested_meta` for each item in the attribute.
    /// For example, `#[env(var = "X", secret)]` calls this twice:
    /// 1. Once with `var = "X"`
    /// 2. Once with `secret`
    ///
    /// # Algorithm
    ///
    /// 1. Extract identifier from the meta path
    /// 2. Map to `&'static str` for zero-allocation duplicate detection
    /// 3. Check for duplicates using `HashSet::insert`
    /// 4. Parse value (if any) based on option type
    ///
    /// The static string mapping serves dual purpose:
    /// - Validates option name is known (unknown → error)
    /// - Enables O(1) duplicate check without String allocation
    #[expect(
        clippy::needless_pass_by_value,
        reason = "ParseNestedMeta is passed by value per syn's parse_nested_meta callback signature"
    )]
    fn parse_meta(&mut self, meta: ParseNestedMeta) -> SynResult<()> {
        // Extract the option name (e.g., "var", "default", "optional", "secret")
        let ident = meta
            .path
            .get_ident()
            .ok_or_else(|| meta.error("Expected Identifier"))?;
        let name = ident.to_string();

        // Map dynamic string to static str for zero-allocation duplicate detection.
        // This match also validates that the option name is recognized.
        // Unknown options fail here with a helpful error message.
        let key: &'static str = match name.as_str() {
            "var" => "var",
            "default" => "default",
            "optional" => "optional",
            "secret" => "secret",
            "no_prefix" => "no_prefix",
            "flatten" => "flatten",
            "prefix" => "prefix",
            "arg" => "arg",
            "short" => "short",
            "format" => "format",
            "validate" => "validate",
            _ => return Err(meta.error(format!("Unknown option `{name}`"))),
        };

        // Check for duplicate options like #[env(var = "X", var = "Y")]
        if !self.seen.insert(key) {
            return Err(meta.error(format!("Duplicate option: `{key}`")));
        }

        // Parse the option based on its type
        match key {
            // var = "ENV_VAR_NAME" - requires a string value
            "var" => {
                // meta.value()? consumes the `=` token
                // .parse()? parses the string literal
                let lit_str: LitStr = meta.value()?.parse()?;
                self.var_name = Some(lit_str.value());
            }

            // default = "fallback_value" - requires a string value
            "default" => {
                let lit_str: LitStr = meta.value()?.parse()?;
                self.default = Some(lit_str.value());
            }

            // optional - just a flag, no value
            "optional" => {
                self.optional = true;
            }

            // secret - just a flag, no value
            "secret" => {
                self.secret = true;
            }

            "no_prefix" => {
                self.no_prefix = true;
            }

            "flatten" => {
                self.flatten = true;
            }

            // prefix = "DB_" - prefix for flatten fields
            "prefix" => {
                let lit_str: LitStr = meta.value()?.parse()?;
                self.flatten_prefix = Some(lit_str.value());
            }

            // arg = "port" - CLI long argument name
            "arg" => {
                let lit_str: LitStr = meta.value()?.parse()?;
                self.arg_long = Some(lit_str.value());
            }

            // short = 'p' - CLI short flag
            "short" => {
                let lit_char: syn::LitChar = meta.value()?.parse()?;
                self.arg_short = Some(lit_char.value());
            }

            // format = "json" - parse env value as structured data
            "format" => {
                let lit_str: LitStr = meta.value()?.parse()?;
                let format_val = lit_str.value();
                // Validate format is supported
                match format_val.as_str() {
                    "json" | "toml" | "yaml" => {}
                    _ => {
                        return Err(meta.error(format!(
                            "Unknown format '{format_val}'. Supported: json, toml, yaml"
                        )));
                    }
                }
                self.format = Some(format_val);
            }

            // validate = "function_name" - custom validation function
            "validate" => {
                let lit_str: LitStr = meta.value()?.parse()?;
                self.validate = Some(lit_str.value());
            }

            // We validated the key above
            _ => unreachable!(),
        }

        Ok(())
    }

    /// Validate the accumulated options and build the final `EnvAttr`.
    ///
    /// This is the second phase of parsing, called after all options have been
    /// accumulated by `parse_meta()`.
    ///
    /// # Validation Rules
    ///
    /// - `var` is required (environment variable name must be specified)
    /// - `default` and `optional` are mutually exclusive:
    ///   - `default`: "use this value if env var is missing"
    ///   - `optional`: "be `None` if env var is missing"
    /// - `short` requires `arg` to be set (can't have `-p` without `--port`)
    ///
    /// # CLI Construction Optimization
    ///
    /// Uses `Option::map` instead of if-else for idiomatic Option construction:
    ///
    /// ```text
    /// Before:
    ///   let cli = if self.arg_long.is_some() {
    ///       Some(CliAttr { long: self.arg_long, short: self.arg_short })
    ///   } else { None };
    ///
    /// After:
    ///   let cli = self.arg_long.map(|long| CliAttr { long: Some(long), ... });
    /// ```
    ///
    /// This is more concise and expresses intent clearly: "if there's a long arg,
    /// create a `CliAttr`; otherwise None".
    fn build(self, attr: &Attribute) -> SynResult<EnvAttr> {
        // Ensure `var` was provided - this is the only required option
        let var_name = self
            .var_name
            .ok_or_else(|| SynError::new_spanned(attr, "Missing required `var` option"))?;

        // Disallow combining default and optional - they have conflicting semantics.
        // Rationale: `default` means "use this value if missing"
        //            `optional` means "be None if missing"
        // Both handle "missing" case but in incompatible ways.
        if self.default.is_some() && self.optional {
            return Err(SynError::new_spanned(
                attr,
                "Cannot use both `default` and `optional` on the same field",
            ));
        }

        // Validate CLI attributes: short flag requires long name
        // (clap convention: can't have just `-p`, need `--port` too)
        if self.arg_short.is_some() && self.arg_long.is_none() {
            return Err(SynError::new_spanned(
                attr,
                "`short` requires `arg` to be set",
            ));
        }

        // Build CLI config using Option::map for idiomatic construction.
        // If arg_long is Some, we create CliAttr; otherwise cli is None.
        let cli = self.arg_long.map(|long| CliAttr {
            long: Some(long),
            short: self.arg_short,
        });

        Ok(EnvAttr {
            var_name,
            default: self.default,
            optional: self.optional,
            secret: self.secret,
            no_prefix: self.no_prefix,
            cli,
            profile: None, // Parsed separately via #[profile(...)] attribute
            format: self.format,
            validate: self.validate,
        })
    }

    /// Parse the `#[profile(...)]` attribute from a field.
    ///
    /// Returns a `ProfileAttr` containing profile name -> value mappings.
    pub fn parse_profile_attr(field: &Field) -> SynResult<Option<ProfileAttr>> {
        for attr in &field.attrs {
            if !attr.path().is_ident("profile") {
                continue;
            }

            let mut values = HashMap::new();

            attr.parse_nested_meta(|meta| {
                // Each entry is: profile_name = "value"
                let profile_name = meta
                    .path
                    .get_ident()
                    .ok_or_else(|| meta.error("Expected profile name identifier"))?
                    .to_string();

                let lit_str: LitStr = meta.value()?.parse()?;
                values.insert(profile_name, lit_str.value());

                Ok(())
            })?;

            if values.is_empty() {
                return Err(SynError::new_spanned(
                    attr,
                    "#[profile(...)] requires at least one profile value",
                ));
            }

            return Ok(Some(ProfileAttr { values }));
        }

        Ok(None)
    }

    /// Parse the `#[env(...)]` attribute and return the field configuration.
    ///
    /// This is the preferred entry point as it handles both regular env fields
    /// and flattened nested configs. Also parses `#[profile(...)]` attribute if present.
    pub fn parse_field_config(field: &Field) -> SynResult<FieldConfig> {
        for attr in &field.attrs {
            if !attr.path().is_ident("env") {
                continue;
            }

            let mut builder = Self::default();
            attr.parse_nested_meta(|meta: ParseNestedMeta<'_>| builder.parse_meta(meta))?;

            let mut config = builder.build_config(attr)?;

            // Also parse #[profile(...)] attribute if present (for non-flatten fields)
            if let FieldConfig::Env(ref mut env_attr) = config {
                env_attr.profile = Self::parse_profile_attr(field)?;
            }

            return Ok(config);
        }

        let field_name = field
            .ident
            .as_ref()
            .map_or_else(|| "unnamed".into(), std::string::ToString::to_string);

        Err(SynError::new_spanned(
            field,
            format!("Field `{field_name}` is missing #[env(...)] attribute"),
        ))
    }

    /// Build either a Flatten or Env config based on parsed options.
    ///
    /// # Flatten Validation Strategy
    ///
    /// When `flatten` is set, we use a **collect-then-report** pattern:
    ///
    /// ```text
    /// Traditional approach (fail-fast):
    ///   if var_name.is_some() { return Err("Cannot use var") }
    ///   if default.is_some() { return Err("Cannot use default") }
    ///   // User sees ONE error, fixes it, recompiles, sees NEXT error...
    ///
    /// Optimized approach (collect-all):
    ///   let incompatible = [var?, default?, optional?, ...].flatten().collect()
    ///   if !incompatible.is_empty() { return Err("Cannot use var, default") }
    ///   // User sees ALL errors at once, fixes everything in one pass
    /// ```
    ///
    /// This provides better developer experience: instead of fix-compile-fix cycles,
    /// users see all incompatible options in a single error message.
    ///
    /// # Implementation Notes
    ///
    /// The array-of-Options pattern with `into_iter().flatten()` is idiomatic Rust
    /// for conditionally including items. Each `bool::then_some()` returns `Some(&str)`
    /// if the condition is true, `None` otherwise. `flatten()` removes the `None`s.
    fn build_config(self, attr: &Attribute) -> SynResult<FieldConfig> {
        // If flatten is set, validate only `prefix` is allowed as additional option
        if self.flatten {
            // Collect ALL incompatible options to report them together.
            // This improves UX: users see everything to fix in one error message.
            //
            // Pattern: [Option<&str>; N] → Iterator<Item=Option<&str>> → Iterator<Item=&str> → Vec<&str>
            // The flatten() call removes None values, keeping only Some(name) items.
            let incompatible: Vec<&str> = [
                self.var_name.is_some().then_some("var"),
                self.default.is_some().then_some("default"),
                self.optional.then_some("optional"),
                self.secret.then_some("secret"),
                self.no_prefix.then_some("no_prefix"),
                (self.arg_long.is_some() || self.arg_short.is_some()).then_some("arg/short"),
                self.format.is_some().then_some("format"),
                self.validate.is_some().then_some("validate"),
            ]
            .into_iter()
            .flatten()
            .collect();

            if !incompatible.is_empty() {
                // Join with "`, `" to produce: "Cannot use `var`, `default` with `flatten`"
                return Err(SynError::new_spanned(
                    attr,
                    format!("Cannot use `{}` with `flatten`", incompatible.join("`, `")),
                ));
            }

            return Ok(FieldConfig::Flatten {
                prefix: self.flatten_prefix,
            });
        }

        // `prefix` requires `flatten` - catch misuse early with clear error
        if self.flatten_prefix.is_some() {
            return Err(SynError::new_spanned(
                attr,
                "`prefix` can only be used with `flatten`",
            ));
        }

        // Otherwise, build a regular EnvAttr via the build() method
        Ok(FieldConfig::Env(self.build(attr)?))
    }
}

// ============================================================================
// Struct-level Attribute Parsing (#[env_config(...)])
// ============================================================================

/// Configuration for dotenvy integration.
///
/// Specifies how `.env` files should be loaded before reading environment variables.
#[derive(Clone, Debug)]
pub enum DotenvConfig {
    /// Use default .env searching from current dir upward.
    /// Generated from: `#[env_config(dotenv)]`
    Default,

    /// Load from a specific file path.
    /// Generated from: `#[env_config(dotenv = ".env.local")]`
    Custom(String),

    /// Load from multiple files in order (later files override earlier).
    /// Generated from: `#[env_config(dotenv = [".env", ".env.local"])]`
    Multiple(Vec<String>),
}

/// Configuration for a config file source.
#[derive(Clone, Debug)]
pub struct FileConfig {
    /// Path to the config file
    pub path: String,

    /// Whether this file is required (error if missing) or optional
    pub required: bool,
}

impl FileConfig {
    /// Create a required file config.
    pub fn required(path: String) -> Self {
        Self {
            path,
            required: true,
        }
    }

    /// Create an optional file config.
    pub fn optional(path: String) -> Self {
        Self {
            path,
            required: false,
        }
    }
}

/// The parsed result of an `#[env_config(...)]` struct-level attribute.
///
/// This struct holds configuration that applies to the entire struct,
/// such as environment variable prefix, dotenv file loading, config files,
/// and profile settings.
///
/// # Struct-Level Options
///
/// | Option | Description |
/// |--------|-------------|
/// | `prefix = "APP_"` | Prefix added to all env var names |
/// | `dotenv` | Load `.env` file from current directory |
/// | `dotenv = ".env.local"` | Load specific dotenv file |
/// | `file = "config.toml"` | Load required config file |
/// | `file_optional = "..."` | Load optional config file |
/// | `profile_env = "APP_ENV"` | Env var for profile selection |
/// | `profiles = ["dev", "prod"]` | Valid profile names |
///
/// # Example
///
/// ```ignore
/// #[derive(EnvConfig)]
/// #[env_config(
///     prefix = "APP_",
///     dotenv,
///     file_optional = "config.toml",
///     profile_env = "APP_ENV",
///     profiles = ["dev", "staging", "prod"]
/// )]
/// struct Config {
///     // ...
/// }
/// ```
#[derive(Clone, Debug, Default)]
pub struct EnvConfigAttr {
    /// Dotenv configuration: `None` (disabled) or `Some(DotenvConfig)` (enabled).
    pub dotenv: Option<DotenvConfig>,

    /// Prefix prepended to all environment variable names.
    ///
    /// For example, with `prefix = "APP_"`, a field with `var = "PORT"`
    /// will read from `APP_PORT`.
    pub prefix: Option<String>,

    /// Config files to load (in order, later files override earlier).
    ///
    /// Supports both required and optional files.
    pub files: Vec<FileConfig>,

    /// Environment variable that selects the active profile.
    ///
    /// For example, `profile_env = "APP_ENV"` means the value of
    /// `APP_ENV` determines which profile defaults to use.
    pub profile_env: Option<String>,

    /// Valid profile names (validates the profile env var value).
    ///
    /// If set, loading fails when the profile env var contains
    /// a value not in this list.
    pub profiles: Option<Vec<String>>,

    /// Enable automatic validation after loading.
    /// Generated from: `#[env_config(validate)]`
    pub validate: bool,
}

impl EnvConfigAttr {
    /// Parse `#[env_config(...)]` attribute from a struct.
    ///
    /// # Supported Syntax
    ///
    /// ```ignore
    /// #[env_config(dotenv)]                              // Default .env loading
    /// #[env_config(dotenv = ".env.local")]               // Custom file path
    /// #[env_config(dotenv = [".env", ".env.local"])]     // Multiple files
    /// #[env_config(file = "config.toml")]                // Single config file
    /// #[env_config(file = ["config.toml", "config.local.toml"])]  // Multiple files
    /// #[env_config(file_optional = "config.local.toml")] // Optional config file
    /// ```
    pub fn parse_from_struct(input: &DeriveInput) -> SynResult<Self> {
        let mut result = EnvConfigAttr::default();

        for attr in &input.attrs {
            if !attr.path().is_ident("env_config") {
                continue;
            }

            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("dotenv") {
                    // Check if it has a value: #[env_config(dotenv = "...")]
                    if meta.input.peek(syn::Token![=]) {
                        let _eq: syn::Token![=] = meta.input.parse()?;

                        // Could be string or array
                        if meta.input.peek(syn::token::Bracket) {
                            // Array: dotenv = [".env", ".env.local"]
                            let content;
                            bracketed!(content in meta.input);

                            let paths: Punctuated<LitStr, Comma> =
                                Punctuated::parse_terminated(&content)?;

                            let paths: Vec<_> = paths.iter().map(LitStr::value).collect();

                            result.dotenv = Some(DotenvConfig::Multiple(paths));
                        } else {
                            // String: dotenv = ".env.local"
                            let lit_str: LitStr = meta.input.parse()?;
                            result.dotenv = Some(DotenvConfig::Custom(lit_str.value()));
                        }
                    } else {
                        // Just the flag: #[env_config(dotenv)]
                        result.dotenv = Some(DotenvConfig::Default);
                    }

                    Ok(())
                } else if meta.path.is_ident("validate") {
                    result.validate = true;

                    Ok(())
                } else if meta.path.is_ident("prefix") {
                    let lit_str: LitStr = meta.value()?.parse()?;
                    result.prefix = Some(lit_str.value());

                    Ok(())
                } else if meta.path.is_ident("file") {
                    // Required config file(s)
                    Self::parse_file_config(&meta, &mut result.files, true)?;
                    Ok(())
                } else if meta.path.is_ident("file_optional") {
                    // Optional config file(s)
                    Self::parse_file_config(&meta, &mut result.files, false)?;
                    Ok(())
                } else if meta.path.is_ident("profile_env") {
                    // Profile selection env var: profile_env = "APP_ENV"
                    let lit_str: LitStr = meta.value()?.parse()?;
                    result.profile_env = Some(lit_str.value());
                    Ok(())
                } else if meta.path.is_ident("profiles") {
                    // Valid profile names: profiles = ["dev", "staging", "prod"]
                    let _eq: syn::Token![=] = meta.input.parse()?;

                    if !meta.input.peek(syn::token::Bracket) {
                        return Err(meta.error(
                            "profiles must be an array, e.g., profiles = [\"dev\", \"prod\"]",
                        ));
                    }

                    let content;
                    bracketed!(content in meta.input);

                    let profiles: Punctuated<LitStr, Comma> =
                        Punctuated::parse_terminated(&content)?;

                    let profiles: Vec<_> = profiles.iter().map(LitStr::value).collect();

                    if profiles.is_empty() {
                        return Err(meta.error("profiles array cannot be empty"));
                    }

                    result.profiles = Some(profiles);
                    Ok(())
                } else {
                    Err(meta.error("unknown env_config option"))
                }
            })?;
        }

        if result.profiles.is_some() && result.profile_env.is_none() {
            // profiles without profile_env doesn't make sense
            return Err(SynError::new_spanned(
                &input.ident,
                "profiles requires profile_env to be set",
            ));
        }

        Ok(result)
    }

    /// Parse file config from attribute (helper method).
    ///
    /// Handles both single file and array syntax:
    /// - `file = "config.toml"` → single required file
    /// - `file = ["a.toml", "b.toml"]` → multiple required files
    /// - `file_optional = "..."` → single optional file
    ///
    /// # DRY Optimization
    ///
    /// Uses function pointer (`fn(String) -> FileConfig`) to avoid code duplication:
    ///
    /// ```text
    /// Before (duplicated logic):
    ///   if required { files.push(FileConfig::required(...)) }
    ///   else { files.push(FileConfig::optional(...)) }
    ///   // Repeated for both single and array cases = 4 branches
    ///
    /// After (single dispatch point):
    ///   let make_config = if required { FileConfig::required } else { FileConfig::optional };
    ///   files.push(make_config(...))  // Same call for both cases
    /// ```
    ///
    /// This reduces 4 conditional branches to 1, improving maintainability.
    fn parse_file_config(
        meta: &ParseNestedMeta,
        files: &mut Vec<FileConfig>,
        required: bool,
    ) -> SynResult<()> {
        let _eq: syn::Token![=] = meta.input.parse()?;

        // Select constructor once, use for all files.
        // Function pointer avoids closure overhead and is more explicit.
        let make_config: fn(String) -> FileConfig = if required {
            FileConfig::required
        } else {
            FileConfig::optional
        };

        // Handle both single string and array syntax
        if meta.input.peek(syn::token::Bracket) {
            // Array: file = ["config.toml", "config.local.toml"]
            let content;
            bracketed!(content in meta.input);

            let paths: Punctuated<LitStr, Comma> = Punctuated::parse_terminated(&content)?;
            // Use extend + map for efficient batch insertion
            files.extend(paths.iter().map(|lit| make_config(lit.value())));
        } else {
            // String: file = "config.toml"
            let lit_str: LitStr = meta.input.parse()?;
            files.push(make_config(lit_str.value()));
        }

        Ok(())
    }
}
