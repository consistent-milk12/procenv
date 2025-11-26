//! File-based configuration support.
//!
//! This module provides utilities for loading configuration from files
//! and merging multiple configuration sources with proper layering.
//!
//! # Supported Formats
//!
//! - **JSON** - Always available with the `file` feature
//! - **TOML** - Available with the `toml` feature
//! - **YAML** - Available with the `yaml` feature
//!
//! # Layering Priority
//!
//! Configuration sources are merged in this order (lowest to highest priority):
//! 1. Compiled defaults (from `#[env(default = "...")]`)
//! 2. Config files (in order specified)
//! 3. `.env` file (if `dotenv` feature enabled)
//! 4. Environment variables (highest priority)

use miette::{Diagnostic, NamedSource, SourceSpan};
use serde_json::Value;
use std::path::Path;

use crate::Error;

// ============================================================================
// File Format Detection and Parsing
// ============================================================================

/// Supported configuration file formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileFormat {
    /// JSON format (.json)
    Json,
    /// TOML format (.toml)
    #[cfg(feature = "toml")]
    Toml,
    /// YAML format (.yaml, .yml)
    #[cfg(feature = "yaml")]
    Yaml,
}

impl FileFormat {
    /// Detect file format from file extension.
    pub fn from_path(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?;
        match ext.to_lowercase().as_str() {
            "json" => Some(FileFormat::Json),
            #[cfg(feature = "toml")]
            "toml" => Some(FileFormat::Toml),
            #[cfg(feature = "yaml")]
            "yaml" | "yml" => Some(FileFormat::Yaml),
            _ => None,
        }
    }
}

/// Error type for file parsing operations with rich diagnostics.
///
/// Uses miette for beautiful terminal output with source code snippets
/// and line/column information when available.
#[derive(Debug, Diagnostic, thiserror::Error)]
pub enum FileError {
    /// Configuration file not found
    #[error("configuration file not found: {path}")]
    #[diagnostic(
        code(procenv::file::not_found),
        help("ensure the file exists at the specified path")
    )]
    NotFound {
        /// Path to the missing file
        path: String,
    },

    /// Failed to read file
    #[error("failed to read configuration file: {path}")]
    #[diagnostic(
        code(procenv::file::read_error),
        help("check file permissions and ensure it's readable")
    )]
    ReadError {
        /// Path to the file
        path: String,
        /// The underlying I/O error
        #[source]
        source: std::io::Error,
    },

    /// Unknown file format
    #[error("unknown configuration file format: .{extension}")]
    #[diagnostic(
        code(procenv::file::unknown_format),
        help("supported formats: .json, .toml, .yaml, .yml")
    )]
    UnknownFormat {
        /// The file extension that wasn't recognized
        extension: String,
    },

    /// Parse error with source location
    #[error("{format} parse error in {}", .path)]
    #[diagnostic(code(procenv::file::parse_error))]
    Parse {
        /// Format name (JSON, TOML, YAML)
        format: &'static str,
        /// Path to the file
        path: String,
        /// The source file content for display
        #[source_code]
        src: NamedSource<String>,
        /// The location of the error
        #[label("{message}")]
        span: SourceSpan,
        /// Description of what went wrong
        message: String,
        /// Suggestion for how to fix
        #[help]
        help: String,
    },

    /// Parse error without source location (fallback)
    #[error("{format} parse error: {message}")]
    #[diagnostic(code(procenv::file::parse_error))]
    ParseNoSpan {
        /// Format name
        format: &'static str,
        /// Error message
        message: String,
        /// Suggestion for how to fix
        #[help]
        help: String,
    },
}

// ============================================================================
// Error Construction Helpers
// ============================================================================

/// Convert a byte offset to a SourceSpan with a reasonable length.
fn offset_to_span(offset: usize, content: &str) -> SourceSpan {
    // Try to find the end of the current token/line for a reasonable span
    let remaining = &content[offset.min(content.len())..];
    let len = remaining
        .find(|c: char| c.is_whitespace() || c == ',' || c == '}' || c == ']')
        .unwrap_or(remaining.len().min(20))
        .max(1);
    SourceSpan::new(offset.into(), len)
}

/// Convert line/column (1-indexed) to byte offset.
fn line_col_to_offset(content: &str, line: usize, col: usize) -> usize {
    let mut offset = 0;
    for (i, l) in content.lines().enumerate() {
        if i + 1 == line {
            return offset + col.saturating_sub(1);
        }
        offset += l.len() + 1; // +1 for newline
    }
    offset
}

/// Create a JSON parse error with source location.
fn json_parse_error(e: serde_json::Error, content: &str, path: &Path) -> FileError {
    let line = e.line();
    let col = e.column();
    let offset = line_col_to_offset(content, line, col);

    FileError::Parse {
        format: "JSON",
        path: path.display().to_string(),
        src: NamedSource::new(path.display().to_string(), content.to_string()),
        span: offset_to_span(offset, content),
        message: e.to_string(),
        help: "check for missing commas, quotes, or brackets".to_string(),
    }
}

/// Create a TOML parse error with source location.
#[cfg(feature = "toml")]
fn toml_parse_error(e: toml::de::Error, content: &str, path: &Path) -> FileError {
    if let Some(span) = e.span() {
        FileError::Parse {
            format: "TOML",
            path: path.display().to_string(),
            src: NamedSource::new(path.display().to_string(), content.to_string()),
            span: SourceSpan::new(span.start.into(), span.end - span.start),
            message: e.message().to_string(),
            help: "check for missing quotes, invalid values, or syntax errors".to_string(),
        }
    } else {
        FileError::ParseNoSpan {
            format: "TOML",
            message: e.to_string(),
            help: "check for missing quotes, invalid values, or syntax errors".to_string(),
        }
    }
}

/// Create a YAML parse error with source location.
#[cfg(feature = "yaml")]
fn yaml_parse_error(e: serde_saphyr::Error, content: &str, path: &Path) -> FileError {
    // serde_saphyr provides location info via Display
    // We'll parse the error message or use fallback
    let msg = e.to_string();

    // Try to extract line info from error message (format: "... at line X column Y")
    if let Some(loc) = extract_yaml_location(&msg) {
        let offset = line_col_to_offset(content, loc.0, loc.1);
        FileError::Parse {
            format: "YAML",
            path: path.display().to_string(),
            src: NamedSource::new(path.display().to_string(), content.to_string()),
            span: offset_to_span(offset, content),
            message: msg.clone(),
            help: "check indentation and ensure proper YAML syntax".to_string(),
        }
    } else {
        FileError::ParseNoSpan {
            format: "YAML",
            message: msg,
            help: "check indentation and ensure proper YAML syntax".to_string(),
        }
    }
}

/// Try to extract line/column from YAML error message.
#[cfg(feature = "yaml")]
fn extract_yaml_location(msg: &str) -> Option<(usize, usize)> {
    // Look for patterns like "at line 5 column 10"
    let line_idx = msg.find("line ")?;
    let after_line = &msg[line_idx + 5..];
    let line_end = after_line.find(|c: char| !c.is_ascii_digit())?;
    let line: usize = after_line[..line_end].parse().ok()?;

    let col_idx = after_line.find("column ")?;
    let after_col = &after_line[col_idx + 7..];
    let col_end = after_col
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(after_col.len());
    let col: usize = after_col[..col_end].parse().ok()?;

    Some((line, col))
}

// ============================================================================
// File Parsing
// ============================================================================

/// Parse a configuration file into a JSON Value.
///
/// The format is auto-detected from the file extension.
/// Returns `Ok(None)` if the file doesn't exist and `required` is false.
pub fn parse_file(path: &Path, required: bool) -> Result<Option<Value>, FileError> {
    let path_str = path.display().to_string();

    // Check if file exists
    if !path.exists() {
        if required {
            return Err(FileError::NotFound { path: path_str });
        }
        return Ok(None);
    }

    // Read file content
    let content = std::fs::read_to_string(path).map_err(|e| FileError::ReadError {
        path: path_str.clone(),
        source: e,
    })?;

    // Detect format
    let format = FileFormat::from_path(path).ok_or_else(|| FileError::UnknownFormat {
        extension: path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("unknown")
            .to_string(),
    })?;

    // Parse based on format with rich error handling
    let value = match format {
        FileFormat::Json => {
            serde_json::from_str(&content).map_err(|e| json_parse_error(e, &content, path))?
        }

        #[cfg(feature = "toml")]
        FileFormat::Toml => {
            let toml_value: toml::Value =
                toml::from_str(&content).map_err(|e| toml_parse_error(e, &content, path))?;
            toml_to_json(toml_value)
        }

        #[cfg(feature = "yaml")]
        FileFormat::Yaml => {
            serde_saphyr::from_str(&content).map_err(|e| yaml_parse_error(e, &content, path))?
        }
    };

    Ok(Some(value))
}

/// Parse a configuration string with explicit format.
pub fn parse_str(content: &str, format: FileFormat) -> Result<Value, FileError> {
    let dummy_path = Path::new("<string>");
    match format {
        FileFormat::Json => {
            serde_json::from_str(content).map_err(|e| json_parse_error(e, content, dummy_path))
        }

        #[cfg(feature = "toml")]
        FileFormat::Toml => {
            let toml_value: toml::Value =
                toml::from_str(content).map_err(|e| toml_parse_error(e, content, dummy_path))?;
            Ok(toml_to_json(toml_value))
        }

        #[cfg(feature = "yaml")]
        FileFormat::Yaml => {
            serde_saphyr::from_str(content).map_err(|e| yaml_parse_error(e, content, dummy_path))
        }
    }
}

// ============================================================================
// Format Conversion
// ============================================================================

/// Convert a TOML Value to a JSON Value.
#[cfg(feature = "toml")]
fn toml_to_json(toml: toml::Value) -> Value {
    match toml {
        toml::Value::String(s) => Value::String(s),
        toml::Value::Integer(i) => Value::Number(i.into()),
        toml::Value::Float(f) => {
            Value::Number(serde_json::Number::from_f64(f).unwrap_or_else(|| 0.into()))
        }
        toml::Value::Boolean(b) => Value::Bool(b),
        toml::Value::Datetime(dt) => Value::String(dt.to_string()),
        toml::Value::Array(arr) => Value::Array(arr.into_iter().map(toml_to_json).collect()),
        toml::Value::Table(table) => {
            let map: serde_json::Map<String, Value> = table
                .into_iter()
                .map(|(k, v)| (k, toml_to_json(v)))
                .collect();
            Value::Object(map)
        }
    }
}

// ============================================================================
// Value Merging
// ============================================================================

/// Deep merge two JSON values.
///
/// The `overlay` value takes priority over `base`. For objects, keys are
/// merged recursively. For other types, `overlay` completely replaces `base`.
pub fn deep_merge(base: &mut Value, overlay: Value) {
    match (base, overlay) {
        (Value::Object(base_map), Value::Object(overlay_map)) => {
            for (key, overlay_value) in overlay_map {
                if let Some(base_value) = base_map.get_mut(&key) {
                    deep_merge(base_value, overlay_value);
                } else {
                    base_map.insert(key, overlay_value);
                }
            }
        }
        (base, overlay) => {
            *base = overlay;
        }
    }
}

// ============================================================================
// Environment Variable Conversion
// ============================================================================

/// Coerce a string value to an appropriate JSON type.
///
/// Attempts to parse as bool, integer, or float, falling back to string.
pub fn coerce_value(s: &str) -> Value {
    // Try bool
    if s.eq_ignore_ascii_case("true") {
        return Value::Bool(true);
    }
    if s.eq_ignore_ascii_case("false") {
        return Value::Bool(false);
    }

    // Try integer
    if let Ok(i) = s.parse::<i64>() {
        return Value::Number(i.into());
    }

    // Try float (only if it contains a decimal point to avoid int->float coercion)
    if s.contains('.') {
        if let Ok(f) = s.parse::<f64>() {
            if let Some(n) = serde_json::Number::from_f64(f) {
                return Value::Number(n);
            }
        }
    }

    // Keep as string
    Value::String(s.to_string())
}

/// Convert environment variables to a nested JSON Value.
///
/// Environment variables are converted to nested objects using the separator.
/// For example, with prefix "APP_" and separator "_":
/// - `APP_DATABASE_HOST=localhost` becomes `{"database": {"host": "localhost"}}`
pub fn env_to_value(prefix: &str, separator: &str) -> Value {
    let mut root = serde_json::Map::new();

    for (key, value) in std::env::vars() {
        if let Some(stripped) = key.strip_prefix(prefix) {
            let lowered = stripped.to_lowercase();
            let parts: Vec<&str> = lowered.split(separator).collect();
            let typed_value = coerce_value(&value);
            insert_nested(&mut root, &parts, typed_value);
        }
    }

    Value::Object(root)
}

/// Insert a value into a nested map structure.
fn insert_nested(map: &mut serde_json::Map<String, Value>, parts: &[&str], value: Value) {
    if parts.is_empty() {
        return;
    }

    if parts.len() == 1 {
        map.insert(parts[0].to_string(), value);
    } else {
        let entry = map
            .entry(parts[0].to_string())
            .or_insert_with(|| Value::Object(serde_json::Map::new()));

        if let Value::Object(nested) = entry {
            insert_nested(nested, &parts[1..], value);
        }
    }
}

// ============================================================================
// Configuration Builder
// ============================================================================

/// Builder for layered configuration loading.
///
/// Use this to load configuration from multiple sources with proper priority.
///
/// # Example
///
/// ```ignore
/// use procenv::file::ConfigBuilder;
///
/// let config: MyConfig = ConfigBuilder::new()
///     .defaults(MyConfig::default())
///     .file("config.toml")
///     .file_optional("config.local.toml")
///     .env_prefix("APP_")
///     .build()?;
/// ```
pub struct ConfigBuilder {
    base: Value,
    files: Vec<(std::path::PathBuf, bool)>, // (path, required)
    env_prefix: Option<String>,
    env_separator: String,
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigBuilder {
    /// Create a new configuration builder.
    pub fn new() -> Self {
        Self {
            base: Value::Object(serde_json::Map::new()),
            files: Vec::new(),
            env_prefix: None,
            env_separator: "_".to_string(),
        }
    }

    /// Set default values from a serializable struct.
    pub fn defaults<T: serde::Serialize>(mut self, defaults: T) -> Self {
        if let Ok(value) = serde_json::to_value(defaults) {
            self.base = value;
        }
        self
    }

    /// Set default values from a JSON Value.
    pub fn defaults_value(mut self, value: Value) -> Self {
        self.base = value;
        self
    }

    /// Add a required configuration file.
    ///
    /// Returns an error if the file doesn't exist.
    pub fn file<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.files.push((path.as_ref().to_path_buf(), true));
        self
    }

    /// Add an optional configuration file.
    ///
    /// Silently skipped if the file doesn't exist.
    pub fn file_optional<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.files.push((path.as_ref().to_path_buf(), false));
        self
    }

    /// Set the environment variable prefix.
    ///
    /// Variables matching `{prefix}*` will be included.
    pub fn env_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.env_prefix = Some(prefix.into());
        self
    }

    /// Set the environment variable separator for nested keys.
    ///
    /// Default is "_". For example, `APP_DATABASE_HOST` becomes `database.host`.
    pub fn env_separator(mut self, separator: impl Into<String>) -> Self {
        self.env_separator = separator.into();
        self
    }

    /// Build the configuration by merging all sources.
    pub fn merge(mut self) -> Result<Value, FileError> {
        // Layer files
        for (path, required) in &self.files {
            if let Some(file_value) = parse_file(path, *required)? {
                deep_merge(&mut self.base, file_value);
            }
        }

        // Layer environment variables
        if let Some(prefix) = &self.env_prefix {
            let env_value = env_to_value(prefix, &self.env_separator);
            if let Value::Object(map) = &env_value {
                if !map.is_empty() {
                    deep_merge(&mut self.base, env_value);
                }
            }
        }

        Ok(self.base)
    }

    /// Build and deserialize the configuration.
    pub fn build<T: serde::de::DeserializeOwned>(self) -> Result<T, Error> {
        let merged = self.merge()?;

        serde_json::from_value(merged).map_err(|e| {
            FileError::ParseNoSpan {
                format: "JSON",
                message: e.to_string(),
                help: "check that the config file values match the expected struct types"
                    .to_string(),
            }
            .into()
        })
    }
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coerce_value_bool() {
        assert_eq!(coerce_value("true"), Value::Bool(true));
        assert_eq!(coerce_value("TRUE"), Value::Bool(true));
        assert_eq!(coerce_value("false"), Value::Bool(false));
        assert_eq!(coerce_value("FALSE"), Value::Bool(false));
    }

    #[test]
    fn test_coerce_value_integer() {
        assert_eq!(coerce_value("42"), Value::Number(42.into()));
        assert_eq!(coerce_value("-100"), Value::Number((-100).into()));
        assert_eq!(coerce_value("0"), Value::Number(0.into()));
    }

    #[test]
    fn test_coerce_value_float() {
        let val = coerce_value("3.14");
        if let Value::Number(n) = val {
            assert!((n.as_f64().unwrap() - 3.14).abs() < 0.001);
        } else {
            panic!("Expected number");
        }
    }

    #[test]
    fn test_coerce_value_string() {
        assert_eq!(coerce_value("hello"), Value::String("hello".to_string()));
        assert_eq!(
            coerce_value("hello world"),
            Value::String("hello world".to_string())
        );
    }

    #[test]
    fn test_deep_merge_objects() {
        let mut base = serde_json::json!({
            "a": 1,
            "b": {"x": 10, "y": 20}
        });
        let overlay = serde_json::json!({
            "b": {"y": 200, "z": 30},
            "c": 3
        });

        deep_merge(&mut base, overlay);

        assert_eq!(base["a"], 1);
        assert_eq!(base["b"]["x"], 10);
        assert_eq!(base["b"]["y"], 200);
        assert_eq!(base["b"]["z"], 30);
        assert_eq!(base["c"], 3);
    }

    #[test]
    fn test_deep_merge_replace() {
        let mut base = serde_json::json!({"a": [1, 2, 3]});
        let overlay = serde_json::json!({"a": [4, 5]});

        deep_merge(&mut base, overlay);

        assert_eq!(base["a"], serde_json::json!([4, 5]));
    }

    #[test]
    fn test_insert_nested() {
        let mut map = serde_json::Map::new();
        insert_nested(
            &mut map,
            &["database", "host"],
            Value::String("localhost".into()),
        );

        assert_eq!(
            map.get("database")
                .and_then(|v| v.get("host"))
                .and_then(|v| v.as_str()),
            Some("localhost")
        );
    }

    #[test]
    fn test_env_to_value() {
        // Set test env vars
        unsafe {
            std::env::set_var("TEST_FILE_DATABASE_HOST", "testhost");
            std::env::set_var("TEST_FILE_DATABASE_PORT", "5432");
            std::env::set_var("TEST_FILE_DEBUG", "true");
        }

        let value = env_to_value("TEST_FILE_", "_");

        assert_eq!(
            value
                .get("database")
                .and_then(|v| v.get("host"))
                .and_then(|v| v.as_str()),
            Some("testhost")
        );
        assert_eq!(
            value
                .get("database")
                .and_then(|v| v.get("port"))
                .and_then(|v| v.as_i64()),
            Some(5432)
        );
        assert_eq!(value.get("debug").and_then(|v| v.as_bool()), Some(true));

        // Cleanup
        unsafe {
            std::env::remove_var("TEST_FILE_DATABASE_HOST");
            std::env::remove_var("TEST_FILE_DATABASE_PORT");
            std::env::remove_var("TEST_FILE_DEBUG");
        }
    }

    #[test]
    fn test_parse_json_string() {
        let content = r#"{"name": "test", "port": 8080}"#;
        let value = parse_str(content, FileFormat::Json).unwrap();

        assert_eq!(value.get("name").and_then(|v| v.as_str()), Some("test"));
        assert_eq!(value.get("port").and_then(|v| v.as_i64()), Some(8080));
    }

    #[cfg(feature = "toml")]
    #[test]
    fn test_parse_toml_string() {
        let content = r#"
            name = "test"
            port = 8080

            [database]
            host = "localhost"
        "#;
        let value = parse_str(content, FileFormat::Toml).unwrap();

        assert_eq!(value.get("name").and_then(|v| v.as_str()), Some("test"));
        assert_eq!(value.get("port").and_then(|v| v.as_i64()), Some(8080));
        assert_eq!(
            value
                .get("database")
                .and_then(|v| v.get("host"))
                .and_then(|v| v.as_str()),
            Some("localhost")
        );
    }

    #[cfg(feature = "yaml")]
    #[test]
    fn test_parse_yaml_string() {
        let content = r#"
name: test
port: 8080
database:
  host: localhost
"#;
        let value = parse_str(content, FileFormat::Yaml).unwrap();

        assert_eq!(value.get("name").and_then(|v| v.as_str()), Some("test"));
        assert_eq!(value.get("port").and_then(|v| v.as_i64()), Some(8080));
        assert_eq!(
            value
                .get("database")
                .and_then(|v| v.get("host"))
                .and_then(|v| v.as_str()),
            Some("localhost")
        );
    }

    #[test]
    fn test_config_builder_defaults() {
        use serde::{Deserialize, Serialize};

        #[derive(Serialize, Deserialize, Debug, PartialEq)]
        struct TestConfig {
            name: String,
            port: u16,
        }

        let defaults = TestConfig {
            name: "default".to_string(),
            port: 8080,
        };

        let config: TestConfig = ConfigBuilder::new().defaults(defaults).build().unwrap();

        assert_eq!(config.name, "default");
        assert_eq!(config.port, 8080);
    }

    #[test]
    fn test_config_builder_env_override() {
        use serde::{Deserialize, Serialize};

        #[derive(Serialize, Deserialize, Debug)]
        struct TestConfig {
            name: String,
            port: u16,
        }

        // Set test env var
        unsafe {
            std::env::set_var("CFGTEST_PORT", "9000");
        }

        let defaults = TestConfig {
            name: "default".to_string(),
            port: 8080,
        };

        let config: TestConfig = ConfigBuilder::new()
            .defaults(defaults)
            .env_prefix("CFGTEST_")
            .build()
            .unwrap();

        assert_eq!(config.name, "default");
        assert_eq!(config.port, 9000); // Overridden by env

        // Cleanup
        unsafe {
            std::env::remove_var("CFGTEST_PORT");
        }
    }

    #[test]
    fn test_file_format_detection() {
        assert_eq!(
            FileFormat::from_path(Path::new("config.json")),
            Some(FileFormat::Json)
        );

        #[cfg(feature = "toml")]
        assert_eq!(
            FileFormat::from_path(Path::new("config.toml")),
            Some(FileFormat::Toml)
        );

        #[cfg(feature = "yaml")]
        {
            assert_eq!(
                FileFormat::from_path(Path::new("config.yaml")),
                Some(FileFormat::Yaml)
            );
            assert_eq!(
                FileFormat::from_path(Path::new("config.yml")),
                Some(FileFormat::Yaml)
            );
        }

        assert_eq!(FileFormat::from_path(Path::new("config.txt")), None);
    }
}
