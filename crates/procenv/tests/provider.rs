//! Integration tests for provider extensibility (Phase C).

use procenv::provider::{Provider, ProviderResult, ProviderSource, ProviderValue};
use procenv::{ConfigLoader, Source};
use std::collections::HashMap;

// ============================================================================
// Test Providers
// ============================================================================

/// A simple in-memory provider for testing.
struct MemoryProvider {
    name: String,
    values: HashMap<String, String>,
    priority: u32,
}

impl MemoryProvider {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            values: HashMap::new(),
            priority: 100,
        }
    }

    fn with_value(mut self, key: &str, value: &str) -> Self {
        self.values.insert(key.to_string(), value.to_string());
        self
    }

    fn with_priority(mut self, priority: u32) -> Self {
        self.priority = priority;
        self
    }
}

impl Provider for MemoryProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn get(&self, key: &str) -> ProviderResult<ProviderValue> {
        match self.values.get(key) {
            Some(v) => Ok(Some(ProviderValue {
                value: v.clone(),
                source: ProviderSource::custom(&self.name, None),
                secret: false,
            })),
            None => Ok(None),
        }
    }

    fn priority(&self) -> u32 {
        self.priority
    }
}

// ============================================================================
// Provider Trait Tests
// ============================================================================

#[test]
fn test_memory_provider_get() {
    let provider = MemoryProvider::new("test")
        .with_value("key1", "value1")
        .with_value("key2", "value2");

    let result = provider.get("key1").unwrap();
    assert!(result.is_some());
    assert_eq!(result.unwrap().value, "value1");

    let result = provider.get("missing").unwrap();
    assert!(result.is_none());
}

#[test]
fn test_memory_provider_source() {
    let provider = MemoryProvider::new("my-provider").with_value("key", "value");

    let result = provider.get("key").unwrap().unwrap();

    match result.source {
        ProviderSource::Custom { provider, path } => {
            assert_eq!(provider, "my-provider");
            assert!(path.is_none());
        }
        _ => panic!("Expected Custom source"),
    }
}

#[test]
fn test_provider_priority() {
    let p1 = MemoryProvider::new("low").with_priority(100);
    let p2 = MemoryProvider::new("high").with_priority(10);

    assert_eq!(p1.priority(), 100);
    assert_eq!(p2.priority(), 10);
}

// ============================================================================
// ConfigLoader Tests
// ============================================================================

#[test]
fn test_loader_single_provider() {
    let provider = MemoryProvider::new("test").with_value("PORT", "8080");

    let mut loader = ConfigLoader::new().with_provider(Box::new(provider));

    let value = loader.get("PORT");
    assert!(value.is_some());
    assert_eq!(value.unwrap().value, "8080");
}

#[test]
fn test_loader_priority_order() {
    // High priority (10) should win over low priority (100)
    let high = MemoryProvider::new("high")
        .with_priority(10)
        .with_value("KEY", "high-value");

    let low = MemoryProvider::new("low")
        .with_priority(100)
        .with_value("KEY", "low-value");

    // Add in reverse order to verify sorting works
    let mut loader = ConfigLoader::new()
        .with_provider(Box::new(low))
        .with_provider(Box::new(high));

    let value = loader.get("KEY").unwrap();
    assert_eq!(value.value, "high-value");
}

#[test]
fn test_loader_fallthrough() {
    // First provider doesn't have the key, second does
    let first = MemoryProvider::new("first")
        .with_priority(10)
        .with_value("FIRST_ONLY", "from-first");

    let second = MemoryProvider::new("second")
        .with_priority(20)
        .with_value("SECOND_ONLY", "from-second");

    let mut loader = ConfigLoader::new()
        .with_provider(Box::new(first))
        .with_provider(Box::new(second));

    // First provider has it
    let v1 = loader.get("FIRST_ONLY").unwrap();
    assert_eq!(v1.value, "from-first");

    // Second provider has it (first doesn't)
    let v2 = loader.get("SECOND_ONLY").unwrap();
    assert_eq!(v2.value, "from-second");
}

#[test]
fn test_loader_caching() {
    let provider = MemoryProvider::new("test").with_value("KEY", "value");

    let mut loader = ConfigLoader::new().with_provider(Box::new(provider));

    // First call
    let v1 = loader.get("KEY").unwrap();
    assert_eq!(v1.value, "value");

    // Second call should use cache
    let v2 = loader.get("KEY").unwrap();
    assert_eq!(v2.value, "value");
}

#[test]
fn test_loader_get_with_default() {
    let mut loader = ConfigLoader::new();

    let value = loader.get_with_default("MISSING", "MISSING", "default");
    assert_eq!(value.value, "default");

    match value.source {
        ProviderSource::BuiltIn(Source::Default) => {}
        _ => panic!("Expected Default source"),
    }
}

#[test]
fn test_loader_get_required_missing() {
    let mut loader = ConfigLoader::new();

    let value = loader.get_required("MISSING", "MISSING_VAR");
    assert!(value.is_none());
    assert!(loader.has_errors());

    let errors = loader.take_errors();
    assert_eq!(errors.len(), 1);
}

#[test]
fn test_loader_source_attribution() {
    let provider = MemoryProvider::new("my-source").with_value("KEY", "value");

    let mut loader = ConfigLoader::new().with_provider(Box::new(provider));

    loader.get("KEY");

    let sources = loader.sources();
    let source = sources.get("KEY").unwrap();

    assert_eq!(
        source.source,
        Source::CustomProvider("my-source".to_string())
    );
}

#[test]
fn test_loader_finish_success() {
    let provider = MemoryProvider::new("test").with_value("KEY", "value");

    let mut loader = ConfigLoader::new().with_provider(Box::new(provider));

    loader.get("KEY");

    let result = loader.finish();
    assert!(result.is_ok());
}

#[test]
fn test_loader_finish_with_errors() {
    let mut loader = ConfigLoader::new();

    // Request a required key that doesn't exist
    loader.get_required("MISSING", "MISSING_VAR");

    let result = loader.finish();
    assert!(result.is_err());
}

// ============================================================================
// ProviderSource Tests
// ============================================================================

#[test]
fn test_provider_source_to_source() {
    let builtin = ProviderSource::environment();
    assert_eq!(builtin.to_source(), Source::Environment);

    let custom = ProviderSource::custom("vault", Some("secret/app".into()));
    assert_eq!(
        custom.to_source(),
        Source::CustomProvider("vault".to_string())
    );
}

#[test]
fn test_provider_source_display() {
    let env = ProviderSource::environment();
    assert_eq!(env.to_string(), "Environment variable");

    let custom_with_path = ProviderSource::custom("vault", Some("secret/app/key".into()));
    assert_eq!(custom_with_path.to_string(), "vault (secret/app/key)");

    let custom_no_path = ProviderSource::custom("consul", None);
    assert_eq!(custom_no_path.to_string(), "consul");
}

// ============================================================================
// EnvProvider Tests
// ============================================================================

#[test]
fn test_env_provider_reads_env() {
    use procenv::provider::EnvProvider;

    // Set a test env var
    unsafe {
        std::env::set_var("PROCENV_TEST_VAR", "test_value");
    }

    let provider = EnvProvider::new();
    let result = provider.get("PROCENV_TEST_VAR").unwrap();

    assert!(result.is_some());
    assert_eq!(result.unwrap().value, "test_value");

    // Cleanup
    unsafe {
        std::env::remove_var("PROCENV_TEST_VAR");
    }
}

#[test]
fn test_env_provider_with_prefix() {
    use procenv::provider::EnvProvider;

    unsafe {
        std::env::set_var("APP_DATABASE_URL", "postgres://localhost");
    }

    let provider = EnvProvider::with_prefix("APP_");
    let result = provider.get("DATABASE_URL").unwrap();

    assert!(result.is_some());
    assert_eq!(result.unwrap().value, "postgres://localhost");

    unsafe {
        std::env::remove_var("APP_DATABASE_URL");
    }
}

#[test]
fn test_env_provider_missing_key() {
    use procenv::provider::EnvProvider;

    let provider = EnvProvider::new();
    let result = provider.get("DEFINITELY_NOT_SET_12345").unwrap();

    assert!(result.is_none());
}

// ============================================================================
// Integration with Existing API
// ============================================================================

#[test]
fn test_source_custom_provider_variant() {
    let source = Source::CustomProvider("vault".to_string());
    assert_eq!(source.to_string(), "Custom provider (vault)");

    let source2 = Source::CustomProvider("vault".to_string());
    assert_eq!(source, source2);
}
