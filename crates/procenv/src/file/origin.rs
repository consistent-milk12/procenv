//! Value origin tracking for error reporting.
//!
//! This module provides types for tracking where configuration values originated,
//! enabling precise error locations in diagnostic messages. When a type mismatch
//! or validation error occurs, the origin tracker can point to the exact file
//! and location where the problematic value was defined.
//!
//! # How It Works
//!
//! 1. As configuration files are loaded, [`OriginTracker`] records which file
//!    each configuration path (e.g., `"database.port"`) came from
//! 2. When deserialization fails, the tracker provides source location info
//! 3. This enables rich error messages with <file:line> highlighting via miette
//!
//! # Internal Use
//!
//! This module is primarily used internally by [`ConfigBuilder`](super::ConfigBuilder)
//! to build source-aware error messages. The public [`OriginTracker`] type is
//! returned from `build_with_origins()` for debugging purposes.
//!
//! # Example Error Output
//!
//! ```text
//! Error: type mismatch at `database.port` in config.toml
//!    ╭─[config.toml:5:8]
//!    │
//!  5 │ port = "not_a_number"
//!    │        ^^^^^^^^^^^^^^ expected u16
//!    ╰────
//! ```

use std::collections::HashMap;
use std::path::PathBuf;
use std::string::String;

use serde_json as SJSON;

use super::format::FileFormat;

/// Tracks the origin of a value for error reporting.
#[derive(Clone, Debug)]
pub(crate) struct ValueOrigin {
    /// Path to the source file
    pub file_path: String,

    /// Original file content
    pub content: String,

    /// File format
    pub format: FileFormat,
}

/// Tracks the origin of configuration values for precise error reporting.
///
/// This struct maintains a mapping of configuration paths (e.g., `"database.port"`)
/// to their source files. It's used internally by [`ConfigBuilder`](crate::file::ConfigBuilder)
/// to provide accurate source locations in error messages.
///
/// When a type mismatch or parse error occurs during deserialization,
/// `OriginTracker` enables the error message to point to the exact file
/// and location where the problematic value was defined.
///
/// # Example
///
/// ```rust,ignore
/// let (config, origins) = ConfigBuilder::new()
///     .file("config.toml")
///     .build_with_origins()?;
///
/// // Check where a specific field came from
/// if let Some(path) = origins.get_file_source("database.port") {
///     println!("database.port defined in: {}", path.display());
/// }
/// ```
#[derive(Clone, Debug, Default)]
pub struct OriginTracker {
    /// Maps JSON paths (e.g., `"database.port"`) to their source file.
    pub(crate) origins: HashMap<String, ValueOrigin>,

    /// List of all source files in priority order (last = highest priority).
    pub(crate) sources: Vec<ValueOrigin>,
}

impl OriginTracker {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Record a source file.
    pub(crate) fn add_source(&mut self, file_path: String, content: String, format: FileFormat) {
        self.sources.push(ValueOrigin {
            file_path,
            content,
            format,
        });
    }

    /// Record origins for all paths in a value, attributing to the most recent source.
    pub(crate) fn track_value(&mut self, value: &SJSON::Value, prefix: &str) {
        // Get the most recent source, or return early if none
        let Some(source) = self.sources.last().cloned() else {
            return;
        };

        self.track_value_recursive(value, prefix, &source);
    }

    fn track_value_recursive(&mut self, value: &SJSON::Value, prefix: &str, source: &ValueOrigin) {
        match value {
            SJSON::Value::Object(map) => {
                for (key, val) in map {
                    let path = if prefix.is_empty() {
                        key.clone()
                    } else {
                        format!("{prefix}.{key}")
                    };

                    self.origins.insert(path.clone(), source.clone());
                    self.track_value_recursive(val, &path, source);
                }
            }

            SJSON::Value::Array(arr) => {
                for (i, val) in arr.iter().enumerate() {
                    let path = format!("{prefix}[{i}]");

                    self.origins.insert(path.clone(), source.clone());
                    self.track_value_recursive(val, &path, source);
                }
            }

            _ => {
                // Leaf value - already tracked by parent
            }
        }
    }

    /// Find the origin of a value at the given path.
    pub(crate) fn find_origin(&self, path: &str) -> Option<&ValueOrigin> {
        // Try exact match first
        if let Some(origin) = self.origins.get(path) {
            return Some(origin);
        }

        // Try parent paths (for missing fields, the parent object might be tracked)
        let mut current = path.to_string();

        while let Some(dot_pos) = current.rfind('.') {
            current = current[..dot_pos].to_string();

            if let Some(origin) = self.origins.get(&current) {
                return Some(origin);
            }
        }

        // Fall back to most recent source
        self.sources.last()
    }

    /// Check if a field came from a configuration file.
    ///
    /// Returns the file path if the field was explicitly loaded from a file.
    /// Does not return a fallback - only returns Some if the field is actually tracked.
    #[must_use]
    pub fn get_file_source(&self, field_name: &str) -> Option<PathBuf> {
        // Only return if we have an exact match or parent match in origins
        // Do NOT use the fallback to most recent source
        if let Some(origin) = self.origins.get(field_name) {
            return Some(PathBuf::from(&origin.file_path));
        }

        // Try parent paths (for nested fields)
        let mut current = field_name.to_string();
        while let Some(dot_pos) = current.rfind('.') {
            current = current[..dot_pos].to_string();
            if let Some(origin) = self.origins.get(&current) {
                return Some(PathBuf::from(&origin.file_path));
            }
        }

        None
    }

    /// Check if any files were loaded.
    #[must_use]
    pub fn has_file_sources(&self) -> bool {
        !self.sources.is_empty()
    }

    /// Get all tracked field paths.
    pub fn tracked_fields(&self) -> impl Iterator<Item = &str> {
        self.origins.keys().map(String::as_str)
    }
}
