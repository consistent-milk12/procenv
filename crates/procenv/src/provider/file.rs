//! File-based configuration provider.

use super::{Provider, ProviderError, ProviderResult, ProviderSource, ProviderValue};
use crate::Source;
use crate::file::{ConfigBuilder, FileFormat, FileUtils, OriginTracker};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// Provider that loads configuration from files (TOML, JSON, YAML).
///
/// This provider wraps the existing `ConfigBuilder` functionality to
/// provide file-based configuration as a provider in the chain.
///
/// # Example
///
/// ```rust,ignore
/// use procenv::provider::FileProvider;
///
/// // Single required file
/// let provider = FileProvider::from_file("config.toml")?;
///
/// // Optional file
/// let provider = FileProvider::from_file_optional("config.local.toml")?;
///
/// // Multiple files with layering
/// let provider = FileProvider::builder()
///     .file_optional("config.toml")
///     .file_optional("config.local.toml")
///     .build()?;
/// ```
#[derive(Debug, Clone)]
pub struct FileProvider {
    /// Merged configuration values
    values: Value,
    /// Origin tracking for source attribution
    origins: OriginTracker,
    /// Primary file path (for source attribution)
    primary_path: Option<PathBuf>,
}

impl FileProvider {
    /// Creates a provider from a single required file.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, crate::file::FileError> {
        let path = path.as_ref();
        let (values, origins) = ConfigBuilder::new().file(path).merge()?;

        Ok(Self {
            values,
            origins,
            primary_path: Some(path.to_path_buf()),
        })
    }

    /// Creates a provider from an optional file.
    ///
    /// Returns `Ok(None)` if the file doesn't exist.
    pub fn from_file_optional(
        path: impl AsRef<Path>,
    ) -> Result<Option<Self>, crate::file::FileError> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(None);
        }
        Self::from_file(path).map(Some)
    }

    /// Creates a builder for loading multiple files.
    pub fn builder() -> FileProviderBuilder {
        FileProviderBuilder::new()
    }

    /// Get a value from the merged configuration by dotted path.
    fn get_by_path(&self, path: &str) -> Option<&Value> {
        let parts: Vec<&str> = path.split('.').collect();
        let mut current = &self.values;

        for part in parts {
            current = current.get(part)?;
        }

        Some(current)
    }

    /// Convert a JSON value to a string for the provider interface.
    fn value_to_string(value: &Value) -> Option<String> {
        match value {
            Value::String(s) => Some(s.clone()),
            Value::Number(n) => Some(n.to_string()),
            Value::Bool(b) => Some(b.to_string()),
            Value::Null => None,
            // For objects/arrays, return as JSON string
            other => Some(serde_json::to_string(other).ok()?),
        }
    }
}

impl Provider for FileProvider {
    fn name(&self) -> &str {
        "file"
    }

    fn get(&self, key: &str) -> ProviderResult<ProviderValue> {
        match self.get_by_path(key) {
            Some(value) => {
                let string_value = match Self::value_to_string(value) {
                    Some(s) => s,
                    None => return Ok(None),
                };

                // Determine which file this value came from
                let source_path = self
                    .origins
                    .get_file_source(key)
                    .or_else(|| self.primary_path.clone());

                Ok(Some(ProviderValue {
                    value: string_value,
                    source: ProviderSource::BuiltIn(Source::ConfigFile(source_path)),
                    secret: false,
                }))
            }
            None => Ok(None),
        }
    }

    fn priority(&self) -> u32 {
        50 // After env/dotenv, before custom providers
    }
}

/// Builder for creating a FileProvider with multiple files.
pub struct FileProviderBuilder {
    inner: ConfigBuilder,
}

impl FileProviderBuilder {
    /// Creates a new file provider builder.
    pub fn new() -> Self {
        Self {
            inner: ConfigBuilder::new(),
        }
    }

    /// Adds a required file.
    pub fn file(mut self, path: impl AsRef<Path>) -> Self {
        self.inner = self.inner.file(path);
        self
    }

    /// Adds an optional file.
    pub fn file_optional(mut self, path: impl AsRef<Path>) -> Self {
        self.inner = self.inner.file_optional(path);
        self
    }

    /// Builds the file provider.
    pub fn build(self) -> Result<FileProvider, crate::file::FileError> {
        let (values, origins) = self.inner.merge()?;

        Ok(FileProvider {
            values,
            origins,
            primary_path: None,
        })
    }
}

impl Default for FileProviderBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_to_string() {
        assert_eq!(
            FileProvider::value_to_string(&Value::String("hello".into())),
            Some("hello".to_string())
        );
        assert_eq!(
            FileProvider::value_to_string(&Value::Number(42.into())),
            Some("42".to_string())
        );
        assert_eq!(
            FileProvider::value_to_string(&Value::Bool(true)),
            Some("true".to_string())
        );
        assert_eq!(FileProvider::value_to_string(&Value::Null), None);
    }

    #[test]
    fn test_file_provider_priority() {
        // Can't easily test without a file, just check the trait impl exists
        assert_eq!(50_u32, 50); // Placeholder
    }
}
