//! File parsing and value manipulation utilities.
//!
//! This module provides the [`FileUtils`] struct with static methods for:
//!
//! - **Parsing** - Reading and parsing configuration files (JSON, TOML, YAML)
//! - **Merging** - Deep merging JSON values with proper object recursion
//! - **Coercion** - Converting string values to appropriate JSON types
//! - **Error Location** - Finding field offsets for precise error reporting
//!
//! # Parsing Files
//!
//! ```rust,ignore
//! use procenv::file::{FileUtils, FileFormat};
//! use std::path::Path;
//!
//! // Parse with automatic format detection
//! let value = FileUtils::parse_file(Path::new("config.toml"), true)?;
//!
//! // Parse with explicit format
//! let json = FileUtils::parse_str(r#"{"port": 8080}"#, FileFormat::Json)?;
//! ```
//!
//! # Deep Merging
//!
//! Objects are merged recursively, with overlay values taking precedence:
//!
//! ```rust,ignore
//! use procenv::file::FileUtils;
//! use serde_json::json;
//!
//! let mut base = json!({"db": {"host": "localhost", "port": 5432}});
//! let overlay = json!({"db": {"port": 5433}});
//!
//! FileUtils::deep_merge(&mut base, overlay);
//! // Result: {"db": {"host": "localhost", "port": 5433}}
//! ```
//!
//! # Value Coercion
//!
//! Environment variables are coerced to appropriate JSON types:
//!
//! - `"true"`/`"false"` → `Bool`
//! - `"42"` → `Number` (integer)
//! - `"3.14"` → `Number` (float, only if contains `.`)
//! - Everything else → `String`

use std::path::Path;

use miette::{NamedSource, SourceSpan};
use serde_json as SJSON;

#[cfg(feature = "yaml")]
use serde_saphyr as YAML;

#[cfg(feature = "toml")]
use toml as TOML;

use super::error::FileError;
use super::format::FileFormat;
use super::origin::ValueOrigin;

/// Utilities for file parsing and value manipulation.
///
/// This struct provides static methods for:
/// - Parsing configuration files in various formats
/// - Converting between formats
/// - Deep merging JSON values
/// - Coercing string values to appropriate JSON types
///
/// Most users will interact with these utilities indirectly through
/// [`ConfigBuilder`](crate::file::ConfigBuilder) or the derive macro.
/// Direct use is available for advanced use cases.
pub struct FileUtils;

impl FileUtils {
    /// Converts a byte offset to a [`SourceSpan`] with a reasonable length.
    ///
    /// Used internally for error reporting to highlight the relevant
    /// portion of a configuration file.
    pub(crate) fn offset_to_span(offset: usize, content: &str) -> SourceSpan {
        let remaining = &content[offset.min(content.len())..];
        let len = remaining
            .find(|c: char| c.is_whitespace() || (c == ',') || (c == '}') || (c == ']'))
            .unwrap_or(remaining.len().min(20))
            .max(1);

        SourceSpan::new(offset.into(), len)
    }

    /// Convert line/column (1-indexed) to byte offset.
    pub(crate) fn line_col_to_offset(content: &str, line: usize, col: usize) -> usize {
        let mut offset = 0;

        for (i, l) in content.lines().enumerate() {
            if (i + 1) == line {
                return offset + col.saturating_sub(1);
            }

            offset += l.len() + 1;
        }

        offset
    }

    /// Search for a field using multiple patterns and return the value offset.
    ///
    /// This is a helper function that extracts the common pattern matching
    /// logic used across different file formats.
    ///
    /// # Arguments
    /// - `content` - The content to search in
    /// - `base_offset` - Offset adjustment for nested content
    /// - `patterns` - Patterns to search for (e.g., `["field:", "field :"]`)
    /// - `value_finder` - Predicate to find where the value starts after the delimiter
    fn find_value_with_patterns<F>(
        content: &str,
        base_offset: usize,
        patterns: &[String],
        value_finder: F,
    ) -> Option<usize>
    where
        F: Fn(char) -> bool,
    {
        for pattern in patterns {
            if let Some(pos) = content.find(pattern.as_str()) {
                let after_delimiter = pos + pattern.len();
                let remaining = &content[after_delimiter..];

                if let Some(value_start) = remaining.find(&value_finder) {
                    return Some(base_offset + after_delimiter + value_start);
                }
            }
        }
        None
    }

    /// Find a field's value location in file content.
    ///
    /// Handles nested paths like "database.port" by finding the parent section first.
    pub(crate) fn find_field_offset(
        content: &str,
        field: &str,
        format: FileFormat,
    ) -> Option<usize> {
        // Handle nested paths like "database.port"
        let parts: Vec<&str> = field.split('.').collect();
        let leaf_field = parts.last()?;

        // Determine search region based on parent path
        let search_content = if parts.len() > 1 {
            let parent_parts = &parts[..parts.len() - 1];
            Self::find_section_content(content, parent_parts, format)?
        } else {
            content
        };

        // Calculate offset adjustment (search_content might be a slice of content)
        let base_offset = content.len() - search_content.len();

        match format {
            FileFormat::Json => {
                let patterns = vec![format!("\"{leaf_field}\":"), format!("\"{leaf_field}\" :")];
                Self::find_value_with_patterns(search_content, base_offset, &patterns, |c: char| {
                    !c.is_whitespace()
                })
            }

            #[cfg(feature = "toml")]
            FileFormat::Toml => {
                let patterns = vec![format!("{leaf_field} ="), format!("{leaf_field}=")];
                Self::find_value_with_patterns(search_content, base_offset, &patterns, |c: char| {
                    !c.is_whitespace()
                })
            }

            #[cfg(feature = "yaml")]
            FileFormat::Yaml => {
                let patterns = vec![format!("{leaf_field}:"), format!("{leaf_field} :")];
                Self::find_value_with_patterns(search_content, base_offset, &patterns, |c: char| {
                    !c.is_whitespace() && c != '\n'
                })
            }
        }
    }

    /// Find the content region for a nested section.
    ///
    /// For TOML, finds `[section]` or `[parent.section]` and returns content until next section.
    /// For JSON/YAML, finds the parent object and returns its content.
    fn find_section_content<'a>(
        content: &'a str,
        parent_parts: &[&str],
        format: FileFormat,
    ) -> Option<&'a str> {
        match format {
            #[cfg(feature = "toml")]
            FileFormat::Toml => {
                // Look for [section] or [parent.child] header
                let section_name = parent_parts.join(".");
                let header = format!("[{}]", section_name);

                if let Some(section_start) = content.find(&header) {
                    let after_header = section_start + header.len();
                    // Find the end of this section (next [...] or end of file)
                    let section_content = &content[after_header..];
                    let section_end = section_content
                        .find("\n[")
                        .map(|pos| after_header + pos)
                        .unwrap_or(content.len());
                    return Some(&content[after_header..section_end]);
                }
                None
            }

            FileFormat::Json => {
                // For JSON, find the parent object by searching for "parent": {
                let parent_key = parent_parts.last()?;
                let pattern = format!("\"{}\"", parent_key);

                if let Some(key_pos) = content.find(&pattern) {
                    // Find the opening brace after the key
                    let after_key = &content[key_pos + pattern.len()..];
                    if let Some(brace_pos) = after_key.find('{') {
                        let obj_start = key_pos + pattern.len() + brace_pos;
                        return Some(&content[obj_start..]);
                    }
                }
                None
            }

            #[cfg(feature = "yaml")]
            FileFormat::Yaml => {
                // For YAML, find parent key and return indented content after it
                let parent_key = parent_parts.last()?;
                let pattern = format!("{}:", parent_key);

                if let Some(key_pos) = content.find(&pattern) {
                    return Some(&content[key_pos..]);
                }
                None
            }
        }
    }

    pub(crate) fn json_parse_error(e: SJSON::Error, content: &str, path: &Path) -> FileError {
        let line = e.line();
        let col = e.column();
        let offset = Self::line_col_to_offset(content, line, col);

        FileError::Parse {
            format: "JSON",
            path: path.display().to_string(),
            src: NamedSource::new(path.display().to_string(), content.to_string()),
            span: Self::offset_to_span(offset, content),
            message: e.to_string(),
            help: "check for missing commands, quotes, or brackets".to_string(),
        }
    }

    #[cfg(feature = "toml")]
    pub(crate) fn toml_parse_error(e: TOML::de::Error, content: &str, path: &Path) -> FileError {
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

    #[cfg(feature = "yaml")]
    pub(crate) fn yaml_parse_error(e: YAML::Error, content: &str, path: &Path) -> FileError {
        let msg = e.to_string();

        if let Some(loc) = Self::extract_yaml_location(&msg) {
            let offset = Self::line_col_to_offset(content, loc.0, loc.1);

            FileError::Parse {
                format: "YAML",
                path: path.display().to_string(),
                src: NamedSource::new(path.display().to_string(), content.to_string()),
                span: Self::offset_to_span(offset, content),
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
    pub(crate) fn extract_yaml_location(msg: &str) -> Option<(usize, usize)> {
        let line_idx = msg.find("line ")?;
        let after_line = &msg[(line_idx + 5)..];
        let line_end = after_line.find(|c: char| !c.is_ascii_digit())?;
        let line = after_line[..line_end].parse::<usize>().ok()?;

        let col_idx = after_line.find("column ")?;
        let after_col = &after_line[(col_idx) + 7..];
        let col_end = after_col
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(after_col.len());
        let col = after_col[..col_end].parse::<usize>().ok()?;

        Some((line, col))
    }

    /// Create a type mismatch error with source location.
    pub(crate) fn type_mismatch_error(
        path: &str,
        message: &str,
        origin: &ValueOrigin,
    ) -> Option<FileError> {
        let offset = Self::find_field_offset(&origin.content, path, origin.format)?;

        Some(FileError::TypeMismatch {
            path_str: path.into(),
            file_path: origin.file_path.clone(),
            src: NamedSource::new(origin.file_path.clone(), origin.content.clone()),
            span: Self::offset_to_span(offset, &origin.content),
            message: message.to_string(),
            help: "check that the value matches the expected type".to_string(),
        })
    }

    // ============================================================================
    // File Parsing
    // ============================================================================

    /// Parses a configuration file into a JSON value.
    ///
    /// The file format is automatically detected from the extension.
    /// Supports `.json`, `.toml` (with feature), and `.yaml`/`.yml` (with feature).
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the configuration file
    /// * `required` - If `true`, returns an error when the file doesn't exist
    ///
    /// # Returns
    ///
    /// - `Ok(Some(value))` - File was parsed successfully
    /// - `Ok(None)` - File doesn't exist and `required` is `false`
    /// - `Err(...)` - File doesn't exist (when required) or parse error
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use procenv::file::FileUtils;
    /// use std::path::Path;
    ///
    /// // Required file (errors if missing)
    /// let value = FileUtils::parse_file(Path::new("config.toml"), true)?;
    ///
    /// // Optional file (returns None if missing)
    /// let value = FileUtils::parse_file(Path::new("config.local.toml"), false)?;
    /// ```
    pub fn parse_file(path: &Path, required: bool) -> Result<Option<SJSON::Value>, FileError> {
        Self::parse_file_with_content(path, required).map(|opt| opt.map(|(v, _, _)| v))
    }

    /// Parse a configuration file and return content for error reporting.
    pub(crate) fn parse_file_with_content(
        path: &Path,
        required: bool,
    ) -> Result<Option<(SJSON::Value, String, FileFormat)>, FileError> {
        let path_str = path.display().to_string();

        if !path.exists() {
            if required {
                return Err(FileError::NotFound { path: path_str });
            }
            return Ok(None);
        }

        let content = std::fs::read_to_string(path).map_err(|e| FileError::ReadError {
            path: path_str.clone(),
            source: e,
        })?;

        let format = FileFormat::from_path(path).ok_or_else(|| FileError::UnknownFormat {
            extension: path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("unknown")
                .to_string(),
        })?;

        let value = match format {
            FileFormat::Json => serde_json::from_str(&content)
                .map_err(|e| Self::json_parse_error(e, &content, path))?,

            #[cfg(feature = "toml")]
            FileFormat::Toml => {
                let toml_value: toml::Value = toml::from_str(&content)
                    .map_err(|e| Self::toml_parse_error(e, &content, path))?;
                Self::toml_to_json(toml_value)
            }

            #[cfg(feature = "yaml")]
            FileFormat::Yaml => serde_saphyr::from_str(&content)
                .map_err(|e| Self::yaml_parse_error(e, &content, path))?,
        };

        Ok(Some((value, content, format)))
    }

    /// Parses a configuration string with an explicit format.
    ///
    /// Unlike [`parse_file`](Self::parse_file), this method requires you to
    /// specify the format explicitly since there's no filename to detect from.
    ///
    /// # Arguments
    ///
    /// * `content` - The configuration content as a string
    /// * `format` - The format to parse as
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use procenv::file::{FileUtils, FileFormat};
    ///
    /// let toml_content = r#"
    /// [database]
    /// host = "localhost"
    /// port = 5432
    /// "#;
    ///
    /// let value = FileUtils::parse_str(toml_content, FileFormat::Toml)?;
    /// ```
    pub fn parse_str(content: &str, format: FileFormat) -> Result<SJSON::Value, FileError> {
        let dummy_path = Path::new("<string>");
        match format {
            FileFormat::Json => serde_json::from_str(content)
                .map_err(|e| Self::json_parse_error(e, content, dummy_path)),

            #[cfg(feature = "toml")]
            FileFormat::Toml => {
                let toml_value: toml::Value = toml::from_str(content)
                    .map_err(|e| Self::toml_parse_error(e, content, dummy_path))?;
                Ok(Self::toml_to_json(toml_value))
            }

            #[cfg(feature = "yaml")]
            FileFormat::Yaml => serde_saphyr::from_str(content)
                .map_err(|e| Self::yaml_parse_error(e, content, dummy_path)),
        }
    }

    // ============================================================================
    // Format Conversion
    // ============================================================================

    /// Convert a TOML Value to a JSON Value.
    #[cfg(feature = "toml")]
    fn toml_to_json(toml: TOML::Value) -> SJSON::Value {
        match toml {
            TOML::Value::String(s) => SJSON::Value::String(s),

            TOML::Value::Integer(i) => SJSON::Value::Number(i.into()),

            TOML::Value::Float(f) => {
                SJSON::Value::Number(SJSON::Number::from_f64(f).unwrap_or_else(|| 0.into()))
            }

            TOML::Value::Boolean(b) => SJSON::Value::Bool(b),

            TOML::Value::Datetime(dt) => SJSON::Value::String(dt.to_string()),

            TOML::Value::Array(arr) => {
                SJSON::Value::Array(arr.into_iter().map(Self::toml_to_json).collect())
            }

            TOML::Value::Table(table) => {
                let map: SJSON::Map<String, SJSON::Value> = table
                    .into_iter()
                    .map(|(k, v)| (k, Self::toml_to_json(v)))
                    .collect();
                SJSON::Value::Object(map)
            }
        }
    }

    // ============================================================================
    // Value Merging
    // ============================================================================

    /// Deep merges two JSON values, with overlay taking precedence.
    ///
    /// For objects, keys from `overlay` are recursively merged into `base`.
    /// For all other types (arrays, primitives), `overlay` replaces `base`.
    ///
    /// # Arguments
    ///
    /// * `base` - The base value (modified in place)
    /// * `overlay` - The overlay value (takes precedence)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use procenv::file::FileUtils;
    /// use serde_json::json;
    ///
    /// let mut base = json!({
    ///     "database": { "host": "localhost", "port": 5432 }
    /// });
    /// let overlay = json!({
    ///     "database": { "port": 5433 }
    /// });
    ///
    /// FileUtils::deep_merge(&mut base, overlay);
    ///
    /// // Result: { "database": { "host": "localhost", "port": 5433 } }
    /// ```
    pub fn deep_merge(base: &mut SJSON::Value, overlay: SJSON::Value) {
        match (base, overlay) {
            (SJSON::Value::Object(base_map), SJSON::Value::Object(overlay_map)) => {
                for (key, overlay_value) in overlay_map {
                    if let Some(base_value) = base_map.get_mut(&key) {
                        Self::deep_merge(base_value, overlay_value);
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

    /// Coerces a string value to an appropriate JSON type.
    ///
    /// Attempts to parse the string as (in order):
    /// 1. Boolean (`true`/`false`, case-insensitive)
    /// 2. Integer (`i64`)
    /// 3. Float (`f64`, only if contains `.`)
    /// 4. String (fallback)
    ///
    /// This is used when converting environment variables to JSON for
    /// merging with config file values.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use procenv::file::FileUtils;
    /// use serde_json::Value;
    ///
    /// assert_eq!(FileUtils::coerce_value("true"), Value::Bool(true));
    /// assert_eq!(FileUtils::coerce_value("42"), Value::Number(42.into()));
    /// assert_eq!(FileUtils::coerce_value("3.14"), Value::Number(/* 3.14 */));
    /// assert_eq!(FileUtils::coerce_value("hello"), Value::String("hello".into()));
    /// ```
    pub fn coerce_value(s: &str) -> SJSON::Value {
        if s.eq_ignore_ascii_case("true") {
            return SJSON::Value::Bool(true);
        }
        if s.eq_ignore_ascii_case("false") {
            return SJSON::Value::Bool(false);
        }

        if let Ok(i) = s.parse::<i64>() {
            return SJSON::Value::Number(i.into());
        }

        if s.contains('.')
            && let Ok(f) = s.parse::<f64>()
            && let Some(n) = SJSON::Number::from_f64(f)
        {
            return SJSON::Value::Number(n);
        }

        SJSON::Value::String(s.to_string())
    }

    /// Convert environment variables to a nested JSON Value.
    pub fn env_to_value(prefix: &str, separator: &str) -> SJSON::Value {
        let mut root = serde_json::Map::new();

        for (key, value) in std::env::vars() {
            if let Some(stripped) = key.strip_prefix(prefix) {
                let lowered = stripped.to_lowercase();
                let parts: Vec<&str> = lowered.split(separator).collect();
                let typed_value = Self::coerce_value(&value);
                Self::insert_nested(&mut root, &parts, typed_value);
            }
        }

        SJSON::Value::Object(root)
    }

    /// Insert a value into a nested map structure.
    ///
    /// Used for building nested JSON objects from flat key paths like "database.host".
    pub fn insert_nested(
        map: &mut SJSON::Map<String, SJSON::Value>,
        parts: &[&str],
        value: SJSON::Value,
    ) {
        if parts.is_empty() {
            return;
        }

        if parts.len() == 1 {
            map.insert(parts[0].to_string(), value);
        } else {
            let entry = map
                .entry(parts[0].to_string())
                .or_insert_with(|| SJSON::Value::Object(SJSON::Map::new()));

            if let SJSON::Value::Object(nested) = entry {
                Self::insert_nested(nested, &parts[1..], value);
            }
        }
    }
}
