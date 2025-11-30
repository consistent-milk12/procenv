//! Tests for Phase D: Runtime Access.

use procenv::{ConfigLoader, ConfigValue, EnvConfig};
use serial_test::serial;
use std::env;

fn with_env<F, R>(vars: &[(&str, &str)], f: F) -> R
where
    F: FnOnce() -> R,
{
    for (k, v) in vars {
        // SAFETY: Tests run serially and don't have concurrent access to env vars
        unsafe { env::set_var(k, v) };
    }
    let result = f();
    for (k, _) in vars {
        // SAFETY: Tests run serially and don't have concurrent access to env vars
        unsafe { env::remove_var(k) };
    }
    result
}

// ============================================================================
// ConfigValue Tests
// ============================================================================

#[test]
fn test_config_value_numeric_cast() {
    let val = ConfigValue::Integer(8080);
    assert_eq!(val.cast::<u16>(), Some(8080u16));
    assert_eq!(val.cast::<i32>(), Some(8080i32));
    assert_eq!(val.to_u16(), Some(8080u16));

    let val = ConfigValue::UnsignedInteger(u64::MAX);
    assert_eq!(val.to_i64(), None); // Overflow
    assert_eq!(val.to_u64(), Some(u64::MAX));
}

#[test]
fn test_config_value_inference() {
    assert!(matches!(
        ConfigValue::from_str_infer("true"),
        ConfigValue::Boolean(true)
    ));
    assert!(matches!(
        ConfigValue::from_str_infer("42"),
        ConfigValue::UnsignedInteger(42)
    ));
    assert!(matches!(
        ConfigValue::from_str_infer("-1"),
        ConfigValue::Integer(-1)
    ));
    assert!(matches!(
        ConfigValue::from_str_infer("3.14"),
        ConfigValue::Float(_)
    ));
    assert!(matches!(
        ConfigValue::from_str_infer("hello"),
        ConfigValue::String(_)
    ));
}

#[test]
fn test_config_value_path_access() {
    use std::collections::HashMap;

    let mut db = HashMap::new();
    db.insert("host".to_string(), ConfigValue::from("localhost"));
    db.insert("port".to_string(), ConfigValue::from(5432u16));

    let mut root = HashMap::new();
    root.insert("database".to_string(), ConfigValue::Map(db));

    let config = ConfigValue::Map(root);

    assert_eq!(
        config.get_path("database.host").and_then(|v| v.as_str()),
        Some("localhost")
    );
    assert_eq!(
        config.get_path("database.port").and_then(|v| v.to_u16()),
        Some(5432)
    );
    assert!(config.get_path("nonexistent").is_none());
}

// ============================================================================
// Generated Method Tests
// ============================================================================

#[derive(EnvConfig)]
struct SimpleConfig {
    #[env(var = "RT_HOST")]
    host: String,

    #[env(var = "RT_PORT", default = "8080")]
    port: u16,

    #[env(var = "RT_DEBUG")]
    debug: bool,
}

#[test]
#[serial]
fn test_keys() {
    let keys = SimpleConfig::keys();
    assert!(keys.contains(&"host"));
    assert!(keys.contains(&"port"));
    assert!(keys.contains(&"debug"));
}

#[test]
#[serial]
fn test_get_str() {
    with_env(
        &[
            ("RT_HOST", "localhost"),
            ("RT_PORT", "3000"),
            ("RT_DEBUG", "true"),
        ],
        || {
            let config = SimpleConfig::from_env().unwrap();
            assert_eq!(config.get_str("host"), Some("localhost".to_string()));
            assert_eq!(config.get_str("port"), Some("3000".to_string()));
            assert_eq!(config.get_str("unknown"), None);
        },
    )
}

#[test]
#[serial]
fn test_has_key() {
    assert!(SimpleConfig::has_key("host"));
    assert!(!SimpleConfig::has_key("unknown"));
}

// ============================================================================
// Secret Field Tests
// ============================================================================

#[derive(EnvConfig)]
#[allow(dead_code)]
struct SecretConfig {
    #[env(var = "RT_SEC_USER")]
    username: String,

    #[env(var = "RT_SEC_PASS", secret)]
    password: String,
}

#[test]
#[serial]
fn test_secret_redacted() {
    with_env(
        &[("RT_SEC_USER", "admin"), ("RT_SEC_PASS", "secret123")],
        || {
            let config = SecretConfig::from_env().unwrap();
            assert_eq!(config.get_str("username"), Some("admin".to_string()));
            assert_eq!(config.get_str("password"), Some("<redacted>".to_string()));
        },
    )
}

// ============================================================================
// Nested Config Tests
// ============================================================================

#[derive(EnvConfig)]
struct DbConfig {
    #[env(var = "DB_HOST")]
    host: String,

    #[env(var = "DB_PORT", default = "5432")]
    port: u16,
}

#[derive(EnvConfig)]
struct AppConfig {
    #[env(var = "APP_NAME")]
    name: String,

    #[env(flatten)]
    database: DbConfig,
}

#[test]
#[serial]
fn test_nested_get_str() {
    with_env(
        &[
            ("APP_NAME", "myapp"),
            ("DB_HOST", "localhost"),
            ("DB_PORT", "3306"),
        ],
        || {
            let config = AppConfig::from_env().unwrap();
            assert_eq!(config.get_str("name"), Some("myapp".to_string()));
            assert_eq!(
                config.get_str("database.host"),
                Some("localhost".to_string())
            );
            assert_eq!(config.get_str("database.port"), Some("3306".to_string()));
        },
    )
}

// ============================================================================
// ConfigLoader Tests
// ============================================================================

#[test]
#[serial]
fn test_loader_get_str() {
    with_env(&[("LOADER_VAR", "hello")], || {
        let mut loader = ConfigLoader::new().with_env();
        assert_eq!(loader.get_str("LOADER_VAR"), Some("hello".to_string()));
    })
}

#[test]
#[serial]
fn test_loader_get_parsed() {
    with_env(&[("LOADER_PORT", "8080")], || {
        let mut loader = ConfigLoader::new().with_env();
        let port: Option<u16> = loader.get_parsed("LOADER_PORT").unwrap();
        assert_eq!(port, Some(8080));
    })
}

#[test]
#[serial]
fn test_loader_get_value_infer() {
    with_env(&[("LOADER_BOOL", "true"), ("LOADER_INT", "42")], || {
        let mut loader = ConfigLoader::new().with_env();

        let val = loader.get_value_infer("LOADER_BOOL").unwrap();
        assert_eq!(val.as_bool(), Some(true));

        let val = loader.get_value_infer("LOADER_INT").unwrap();
        assert_eq!(val.to_u32(), Some(42));
    })
}

#[test]
#[serial]
fn test_loader_get_with_source() {
    with_env(&[("LOADER_SRC", "value")], || {
        let mut loader = ConfigLoader::new().with_env();
        let (value, source) = loader.get_with_source("LOADER_SRC").unwrap();
        assert_eq!(value, "value");
        assert!(matches!(source, procenv::Source::Environment));
    })
}
