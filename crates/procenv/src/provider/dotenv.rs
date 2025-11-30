//! Dotenv file provider.

use super::{Provider, ProviderResult, ProviderSource, ProviderValue};
use crate::Source;
use std::collections::HashMap;
use std::path::PathBuf;

/// Provider that loads configuration from `.env` files.
///
/// This provider reads a `.env` file once at construction time and
/// caches the values. It does NOT modify the process environment.
///
/// # Example
///
/// ```rust,ignore
/// use procenv::provider::DotenvProvider;
///
/// // Load from default .env
/// let provider = DotenvProvider::new()?;
///
/// // Load from specific file
/// let provider = DotenvProvider::from_path(".env.local")?;
///
/// // Optional loading (returns empty provider if file missing)
/// let provider = DotenvProvider::from_path_optional(".env.local");
/// ```
#[derive(Debug, Clone)]
pub struct DotenvProvider {
    values: HashMap<String, String>,
    path: Option<PathBuf>,
    prefix: Option<String>,
}

impl DotenvProvider {
    /// Creates a new dotenv provider from the default `.env` file.
    ///
    /// Returns an error if the file exists but cannot be parsed.
    /// Returns an empty provider if the file doesn't exist.
    pub fn new() -> Result<Self, std::io::Error> {
        Self::from_path_optional(".env")
    }

    /// Creates a provider from a specific `.env` file path.
    ///
    /// Returns an error if the file doesn't exist or cannot be parsed.
    pub fn from_path(path: impl Into<PathBuf>) -> Result<Self, std::io::Error> {
        let path = path.into();
        let values = Self::parse_dotenv_file(&path)?;
        Ok(Self {
            values,
            path: Some(path),
            prefix: None,
        })
    }

    /// Creates a provider from a path, returning empty if file doesn't exist.
    pub fn from_path_optional(path: impl Into<PathBuf>) -> Result<Self, std::io::Error> {
        let path = path.into();
        if !path.exists() {
            return Ok(Self {
                values: HashMap::new(),
                path: None,
                prefix: None,
            });
        }
        Self::from_path(path)
    }

    /// Creates a provider with a key prefix filter.
    ///
    /// Only keys starting with the prefix will be returned, and the prefix
    /// is stripped from the key names.
    pub fn with_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prefix = Some(prefix.into());
        self
    }

    /// Parse a dotenv file into a HashMap.
    fn parse_dotenv_file(path: &PathBuf) -> Result<HashMap<String, String>, std::io::Error> {
        let content = std::fs::read_to_string(path)?;
        Ok(Self::parse_dotenv_content(&content))
    }

    /// Parse dotenv content (for testing).
    fn parse_dotenv_content(content: &str) -> HashMap<String, String> {
        let mut values = HashMap::new();

        for line in content.lines() {
            let line = line.trim();

            // Skip empty lines and comments
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Parse KEY=VALUE
            if let Some(eq_pos) = line.find('=') {
                let key = line[..eq_pos].trim();
                let mut value = line[eq_pos + 1..].trim();

                // Strip surrounding quotes
                if (value.starts_with('"') && value.ends_with('"'))
                    || (value.starts_with('\'') && value.ends_with('\''))
                {
                    value = &value[1..value.len() - 1];
                }

                values.insert(key.to_string(), value.to_string());
            }
        }

        values
    }

    /// Returns the full key with prefix handling.
    fn lookup_key(&self, key: &str) -> String {
        match &self.prefix {
            Some(p) => format!("{}{}", p, key),
            None => key.to_string(),
        }
    }
}

impl Default for DotenvProvider {
    fn default() -> Self {
        Self::new().unwrap_or_else(|_| Self {
            values: HashMap::new(),
            path: None,
            prefix: None,
        })
    }
}

impl Provider for DotenvProvider {
    fn name(&self) -> &str {
        "dotenv"
    }

    fn get(&self, key: &str) -> ProviderResult<ProviderValue> {
        let lookup = self.lookup_key(key);

        match self.values.get(&lookup) {
            Some(value) => Ok(Some(ProviderValue {
                value: value.clone(),
                source: ProviderSource::BuiltIn(Source::DotenvFile(self.path.clone())),
                secret: false,
            })),
            None => Ok(None),
        }
    }

    fn priority(&self) -> u32 {
        30 // After env (20), before config files (50)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_dotenv_content() {
        let content = r#"
# Comment
DATABASE_URL=postgres://localhost/db
PORT=8080
QUOTED="hello world"
SINGLE='single quoted'
"#;
        let values = DotenvProvider::parse_dotenv_content(content);

        assert_eq!(
            values.get("DATABASE_URL"),
            Some(&"postgres://localhost/db".to_string())
        );
        assert_eq!(values.get("PORT"), Some(&"8080".to_string()));
        assert_eq!(values.get("QUOTED"), Some(&"hello world".to_string()));
        assert_eq!(values.get("SINGLE"), Some(&"single quoted".to_string()));
    }

    #[test]
    fn test_dotenv_priority() {
        let provider = DotenvProvider::default();
        assert_eq!(provider.priority(), 30);
    }
}
