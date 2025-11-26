//! Attribute parsing for `#[env(var = "VAR", default = "value", optional, secret)]`
//!
//! This module is responsible for extracting configuration from field attributes.
//! It uses syn's `ParseNestedMeta` for robust, extensible parsing with:
//! - Automatic comma and trailing comma handling
//! - Precise error spans pointing to the exact problematic token
//! - Duplicate option detection
//!
//! ## Supported Syntax
//!
//! ```ignore
//! #[env(var = "ENV_VAR_NAME")]                           // Required field
//! #[env(var = "ENV_VAR_NAME", default = "fallback")]     // With default
//! #[env(var = "ENV_VAR_NAME", optional)]                 // Option<T> field
//! #[env(var = "ENV_VAR_NAME", secret)]                   // Masked in output
//! #[env(var = "ENV_VAR_NAME", secret, default = "key")]  // Combinable
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
pub fn extract_doc_comment(field: &Field) -> Option<String> {
    let docs: Vec<String> = field
        .attrs
        .iter()
        .filter_map(|attr| {
            if !attr.path().is_ident("doc") {
                return None;
            }

            // Parse #[doc = "..."]
            if let Meta::NameValue(meta) = &attr.meta {
                if let Expr::Lit(ExprLit {
                    lit: Lit::Str(lit_str),
                    ..
                }) = &meta.value
                {
                    return Some(lit_str.value().trim().to_string());
                }
            }

            None
        })
        .collect();

    if docs.is_empty() {
        None
    } else {
        Some(docs.join(" "))
    }
}

/// Distinguishes between regular env fields and flattened nested configs.
pub enum FieldConfig {
    /// Regular field: `#[env(var = "NAME", ...)]`
    Env(EnvAttr),

    /// Flattened nested config: `#[env(flatten)]`
    Flatten,
}

/// CLI argument configuration for a field.
#[derive(Clone, Debug, Default)]
pub struct CliAttr {
    /// The long argument name (e.g., "database-url" for --database-url)
    pub long: Option<String>,

    /// Optional short flag (e.g., 'd' for -d)
    pub short: Option<char>,
}

/// Profile-specific default values for a field.
///
/// Parsed from `#[profile(dev = "value1", prod = "value2")]` attribute.
#[derive(Clone, Debug, Default)]
pub struct ProfileAttr {
    /// Map of profile name -> default value for that profile
    /// e.g., {"dev": "localhost", "prod": "prod-db.internal"}
    pub values: HashMap<String, String>,
}

/// The parsed result of an `#[env(...)]` attribute.
///
/// This struct holds all the configuration options extracted from a field's
/// attribute. It is created by `Parser::parse_field_config()`.
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
}

/// Builder pattern parser for `#[env(...)]` attributes.
///
/// This struct accumulates parsed options as we iterate through the attribute's
/// contents, then validates and builds the final `EnvAttr`.
#[derive(Default)]
pub struct Parser {
    /// Accumulated variable name (from `var = "..."`)
    var_name: Option<String>,

    /// Accumulated default value (from `default = "..."`)
    default: Option<String>,

    /// Whether `optional` flag was seen
    optional: bool,

    /// Whether `secret` flag was seen
    secret: bool,

    /// Deserialization format (from `format = "json"`)
    format: Option<String>,

    /// Track which options we've seen to detect duplicates
    /// Uses &'static str for zero-allocation comparison
    seen: HashSet<&'static str>,

    /// Skip the struct-level prefix for this field
    no_prefix: bool,

    /// Whether this is a flattened nested config
    flatten: bool,

    /// CLI long argument name (from `arg = "..."`)
    arg_long: Option<String>,

    /// CLI short flag (from `short = 'x'`)
    arg_short: Option<char>,
}

impl Parser {
    /// Parse a single option from within the attribute.
    ///
    /// Called by `parse_nested_meta` for each item in the attribute.
    /// For example, `#[env(var = "X", secret)]` calls this twice:
    /// 1. Once with `var = "X"`
    /// 2. Once with `secret`
    fn parse_meta(&mut self, meta: ParseNestedMeta) -> SynResult<()> {
        // Get the option name (e.g., "var", "default", "optional", "secret")
        let ident = meta
            .path
            .get_ident()
            .ok_or_else(|| meta.error("Expecteed Identifier"))?;
        let name = ident.to_string();

        // Map to static str for duplicate detection (avoids allocation)
        // Also validates that the option name is known
        let key: &'static str = match name.as_str() {
            "var" => "var",
            "default" => "default",
            "optional" => "optional",
            "secret" => "secret",
            "no_prefix" => "no_prefix",
            "flatten" => "flatten",
            "arg" => "arg",
            "short" => "short",
            "format" => "format",
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
                            "Unknown format '{}'. Supported: json, toml, yaml",
                            format_val
                        )));
                    }
                }
                self.format = Some(format_val);
            }

            // We validated the key above
            _ => unreachable!(),
        }

        Ok(())
    }

    /// Validate the accumulated options and build the final `EnvAttr`.
    ///
    /// # Validation rules
    /// - `var` is required
    /// - `default` and `optional` cannot be used together (conflicting semantics)
    /// - `short` requires `arg` to be set
    fn build(self, attr: &Attribute) -> SynResult<EnvAttr> {
        // Ensure `var` was provided
        let var_name = self
            .var_name
            .ok_or_else(|| SynError::new_spanned(attr, "Missing required `var` option"))?;

        // Disallow combining default and optional
        // Raitionale: `default` means "use this value if missing"
        //             `optional` means "be None if missing"
        // These are mutually exclusive behaviors
        if self.default.is_some() && self.optional {
            return Err(SynError::new_spanned(
                attr,
                "Cannot use both `default` and `optional` on the same field",
            ));
        }

        // Validate CLI attributes
        if self.arg_short.is_some() && self.arg_long.is_none() {
            return Err(SynError::new_spanned(
                attr,
                "`short` requires `arg` to be set",
            ));
        }

        // Build CLI config if any CLI attributes were provided
        let cli = if self.arg_long.is_some() {
            Some(CliAttr {
                long: self.arg_long,
                short: self.arg_short,
            })
        } else {
            None
        };

        Ok(EnvAttr {
            var_name,
            default: self.default,
            optional: self.optional,
            secret: self.secret,
            no_prefix: self.no_prefix,
            cli,
            profile: None, // Parsed separately via #[profile(...)] attribute
            format: self.format,
        })
    }

    /// Parse the `#[profile(...)]` attribute from a field.
    ///
    /// Returns a ProfileAttr containing profile name -> value mappings.
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
            .map(|ident| ident.to_string())
            .unwrap_or_else(|| "unnamed".into());

        Err(SynError::new_spanned(
            field,
            format!("Field `{field_name}` is missing #[env(...)] attribute"),
        ))
    }

    /// Build either a Flatten or Env config based on parsed options.
    fn build_config(self, attr: &Attribute) -> SynResult<FieldConfig> {
        // If flatten is set, validate no other options are used
        if self.flatten {
            if self.var_name.is_some() {
                return Err(SynError::new_spanned(
                    attr,
                    "Cannot use `var` with `flatten`",
                ));
            }
            if self.default.is_some() {
                return Err(SynError::new_spanned(
                    attr,
                    "Cannot use `default` with `flatten`",
                ));
            }
            if self.optional {
                return Err(SynError::new_spanned(
                    attr,
                    "Cannot use `optional` with `flatten`",
                ));
            }
            if self.secret {
                return Err(SynError::new_spanned(
                    attr,
                    "Cannot use `secret` with `flatten`",
                ));
            }
            if self.no_prefix {
                return Err(SynError::new_spanned(
                    attr,
                    "Cannot use `no_prefix` with `flatten`",
                ));
            }
            if self.arg_long.is_some() || self.arg_short.is_some() {
                return Err(SynError::new_spanned(
                    attr,
                    "Cannot use `arg` or `short` with `flatten`",
                ));
            }

            return Ok(FieldConfig::Flatten);
        }

        // Otherwise, build a regular EnvAttr
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
/// such as dotenv file loading settings and config file paths.
#[derive(Clone, Debug, Default)]
pub struct EnvConfigAttr {
    /// Dotenv configuration: None (disabled), Some(DotenvConfig) (enabled)
    pub dotenv: Option<DotenvConfig>,

    /// Prefix to append to all environment variable names
    /// Generated from: `#[env_config(prefix = "APP_")]`
    pub prefix: Option<String>,

    /// Config files to load (in order, later files override earlier)
    /// Generated from: `#[env_config(file = "config.toml")]`
    /// or `#[env_config(file = ["config.toml", "config.local.toml"])]`
    pub files: Vec<FileConfig>,

    /// Environment variable that selects the active profile (Phase 16)
    /// Generated from: `#[env_config(profile_env = "APP_ENV")]`
    pub profile_env: Option<String>,

    /// Valid profile names (Phase 16)
    /// Generated from: `#[env_config(profiles = ["dev", "staging", "prod"])]`
    pub profiles: Option<Vec<String>>,
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

                            let paths: Vec<String> = paths
                                .iter()
                                .map(|lit: &LitStr| -> String { lit.value() })
                                .collect();

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

                    let profiles: Vec<String> = profiles
                        .iter()
                        .map(|lit: &LitStr| -> String { lit.value() })
                        .collect();

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

        // Validate profile configuration
        if result.profile_env.is_some() && result.profiles.is_none() {
            // profile_env without profiles is a warning but allowed
            // (any profile value will be accepted)
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
    fn parse_file_config(
        meta: &ParseNestedMeta,
        files: &mut Vec<FileConfig>,
        required: bool,
    ) -> SynResult<()> {
        let _eq: syn::Token![=] = meta.input.parse()?;

        // Could be string or array
        if meta.input.peek(syn::token::Bracket) {
            // Array: file = ["config.toml", "config.local.toml"]
            let content;
            bracketed!(content in meta.input);

            let paths: Punctuated<LitStr, Comma> = Punctuated::parse_terminated(&content)?;

            for lit in paths {
                if required {
                    files.push(FileConfig::required(lit.value()));
                } else {
                    files.push(FileConfig::optional(lit.value()));
                }
            }
        } else {
            // String: file = "config.toml"
            let lit_str: LitStr = meta.input.parse()?;
            if required {
                files.push(FileConfig::required(lit_str.value()));
            } else {
                files.push(FileConfig::optional(lit_str.value()));
            }
        }

        Ok(())
    }
}
