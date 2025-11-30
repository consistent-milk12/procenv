//! Environment variable provider.

use super::{priority, Provider, ProviderResult, ProviderSource, ProviderValue};
use crate::Source;

/// Provider that reads configuration from environment variables.
///
/// # Example
///
/// ```rust,ignore
/// use procenv::provider::EnvProvider;
///
/// // Without prefix
/// let provider = EnvProvider::new();
///
/// // With prefix (reads APP_DATABASE_URL for key "DATABASE_URL")
/// let provider = EnvProvider::with_prefix("APP_");
/// ```
pub struct EnvProvider {
    prefix: Option<String>,
}

impl EnvProvider {
    /// Create a new environment provider without a prefix.
    pub fn new() -> Self {
        Self { prefix: None }
    }

    /// Creates a new environment provider with a prefix.
    ///
    /// When getting a key, the prefix is prepended to the key name.
    /// For example, with prefix "APP_", getting "PORT" will look for "APP_PORT".
    pub fn with_prefix(prefix: impl Into<String>) -> Self {
        Self {
            prefix: Some(prefix.into()),
        }
    }

    /// Returns the full key name with prefix applied.
    fn full_key(&self, key: &str) -> String {
        match &self.prefix {
            Some(p) => format!("{p}{key}"),

            None => key.to_string(),
        }
    }
}

impl Default for EnvProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl Provider for EnvProvider {
    fn name(&self) -> &str {
        "environment"
    }

    fn get(&self, key: &str) -> ProviderResult<ProviderValue> {
        let full_key = self.full_key(key);

        match std::env::var(&full_key) {
            Ok(value) => Ok(Some(ProviderValue {
                value,
                source: ProviderSource::BuiltIn(Source::Environment),
                secret: false,
            })),

            Err(std::env::VarError::NotPresent) => Ok(None),

            Err(std::env::VarError::NotUnicode(_)) => Err(super::ProviderError::InvalidValue {
                key: full_key,
                provider: self.name().to_string(),
                message: "environment variable contains invalid UTF-8".to_string(),
            }),
        }
    }

    fn priority(&self) -> u32 {
        priority::ENVIRONMENT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_provider_no_prefix() {
        let provider = EnvProvider::new();
        assert_eq!(provider.full_key("PORT"), "PORT");
    }

    #[test]
    fn test_env_provider_with_prefix() {
        let provider = EnvProvider::with_prefix("APP_");
        assert_eq!(provider.full_key("PORT"), "APP_PORT");
    }

    #[test]
    fn test_env_provider_priority() {
        let provider = EnvProvider::new();
        assert_eq!(provider.priority(), priority::ENVIRONMENT);
    }
}
