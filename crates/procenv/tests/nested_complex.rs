//! Complex nested configuration hierarchy tests.
//!
//! Tests for deeply nested structs and complex configuration hierarchies.

use procenv::EnvConfig;
use serial_test::serial;

fn cleanup_env(vars: &[&str]) {
    unsafe {
        for k in vars {
            std::env::remove_var(*k);
        }
    }
}

fn with_env<F, R>(vars: &[(&str, &str)], f: F) -> R
where
    F: FnOnce() -> R,
{
    unsafe {
        for (k, v) in vars {
            std::env::set_var(*k, *v);
        }
    }

    let result = f();

    unsafe {
        for (k, _) in vars {
            std::env::remove_var(*k);
        }
    }

    result
}

// ============================================================================
// Two-Level Nesting (Basic)
// ============================================================================

#[derive(EnvConfig)]
struct InnerConfig {
    #[env(var = "INNER_HOST", default = "localhost")]
    host: String,

    #[env(var = "INNER_PORT", default = "5432")]
    port: u16,
}

#[derive(EnvConfig)]
#[env_config(prefix = "APP_")]
struct TwoLevelConfig {
    #[env(var = "NAME", default = "app")]
    name: String,

    #[env(flatten)]
    inner: InnerConfig,
}

#[test]
#[serial]
fn test_two_level_nesting_defaults() {
    cleanup_env(&["APP_NAME", "INNER_HOST", "INNER_PORT"]);

    let config = TwoLevelConfig::from_env().expect("should load with defaults");

    assert_eq!(config.name, "app");
    assert_eq!(config.inner.host, "localhost");
    assert_eq!(config.inner.port, 5432);
}

#[test]
#[serial]
fn test_two_level_nesting_override() {
    cleanup_env(&["APP_NAME", "INNER_HOST", "INNER_PORT"]);

    with_env(
        &[
            ("APP_NAME", "custom_app"),
            ("INNER_HOST", "db.example.com"),
            ("INNER_PORT", "3306"),
        ],
        || {
            let config = TwoLevelConfig::from_env().expect("should load with overrides");

            assert_eq!(config.name, "custom_app");
            assert_eq!(config.inner.host, "db.example.com");
            assert_eq!(config.inner.port, 3306);
        },
    )
}

// ============================================================================
// Multiple Siblings at Same Level
// ============================================================================

#[derive(EnvConfig)]
struct DatabaseConfig {
    #[env(var = "DB_HOST", default = "localhost")]
    host: String,

    #[env(var = "DB_PORT", default = "5432")]
    port: u16,
}

#[derive(EnvConfig)]
struct CacheConfig {
    #[env(var = "CACHE_HOST", default = "localhost")]
    host: String,

    #[env(var = "CACHE_PORT", default = "6379")]
    port: u16,
}

#[derive(EnvConfig)]
#[env_config(prefix = "SVC_")]
struct MultiSiblingConfig {
    #[env(var = "NAME", default = "myservice")]
    name: String,

    #[env(flatten)]
    database: DatabaseConfig,

    #[env(flatten)]
    cache: CacheConfig,
}

#[test]
#[serial]
fn test_multiple_siblings_defaults() {
    cleanup_env(&["SVC_NAME", "DB_HOST", "DB_PORT", "CACHE_HOST", "CACHE_PORT"]);

    let config = MultiSiblingConfig::from_env().expect("should load with defaults");

    assert_eq!(config.name, "myservice");
    assert_eq!(config.database.host, "localhost");
    assert_eq!(config.database.port, 5432);
    assert_eq!(config.cache.host, "localhost");
    assert_eq!(config.cache.port, 6379);
}

#[test]
#[serial]
fn test_multiple_siblings_selective_override() {
    cleanup_env(&["SVC_NAME", "DB_HOST", "DB_PORT", "CACHE_HOST", "CACHE_PORT"]);

    with_env(
        &[("DB_HOST", "db.prod.com"), ("CACHE_PORT", "6380")],
        || {
            let config = MultiSiblingConfig::from_env().expect("should load with overrides");

            // Overridden values
            assert_eq!(config.database.host, "db.prod.com");
            assert_eq!(config.cache.port, 6380);

            // Default values preserved
            assert_eq!(config.database.port, 5432);
            assert_eq!(config.cache.host, "localhost");
        },
    )
}

// ============================================================================
// Source Attribution for Nested Fields
// ============================================================================

#[test]
#[serial]
fn test_nested_source_attribution() {
    cleanup_env(&["SVC_NAME", "DB_HOST", "DB_PORT", "CACHE_HOST", "CACHE_PORT"]);

    with_env(&[("DB_HOST", "custom-db")], || {
        let (config, sources) =
            MultiSiblingConfig::from_env_with_sources().expect("should load with sources");

        assert_eq!(config.database.host, "custom-db");

        // database.host should be from Environment
        let db_host_src = sources
            .get("database.host")
            .expect("should have database.host");
        assert!(
            matches!(db_host_src.source, procenv::Source::Environment),
            "database.host should be Environment, got {:?}",
            db_host_src.source
        );

        // database.port should be from Default
        let db_port_src = sources
            .get("database.port")
            .expect("should have database.port");
        assert!(
            matches!(db_port_src.source, procenv::Source::Default),
            "database.port should be Default, got {:?}",
            db_port_src.source
        );
    })
}

// ============================================================================
// Nested with Optional Fields
// ============================================================================

#[derive(EnvConfig)]
struct OptionalChild {
    #[env(var = "OPT_CHILD_REQUIRED")]
    required: String,

    #[env(var = "OPT_CHILD_OPTIONAL", optional)]
    optional: Option<String>,

    #[env(var = "OPT_CHILD_DEFAULT", default = "default_val")]
    defaulted: String,
}

#[derive(EnvConfig)]
#[env_config(prefix = "OPT_")]
struct OptionalParent {
    #[env(var = "NAME")]
    name: String,

    #[env(flatten)]
    child: OptionalChild,
}

#[test]
#[serial]
fn test_nested_with_optional_fields() {
    cleanup_env(&[
        "OPT_NAME",
        "OPT_CHILD_REQUIRED",
        "OPT_CHILD_OPTIONAL",
        "OPT_CHILD_DEFAULT",
    ]);

    with_env(
        &[
            ("OPT_NAME", "parent"),
            ("OPT_CHILD_REQUIRED", "required_val"),
        ],
        || {
            let config = OptionalParent::from_env().expect("should load with optional");

            assert_eq!(config.name, "parent");
            assert_eq!(config.child.required, "required_val");
            assert!(config.child.optional.is_none());
            assert_eq!(config.child.defaulted, "default_val");
        },
    )
}

#[test]
#[serial]
fn test_nested_with_optional_set() {
    cleanup_env(&[
        "OPT_NAME",
        "OPT_CHILD_REQUIRED",
        "OPT_CHILD_OPTIONAL",
        "OPT_CHILD_DEFAULT",
    ]);

    with_env(
        &[
            ("OPT_NAME", "parent"),
            ("OPT_CHILD_REQUIRED", "required_val"),
            ("OPT_CHILD_OPTIONAL", "optional_val"),
        ],
        || {
            let config = OptionalParent::from_env().expect("should load with optional set");

            assert_eq!(config.child.optional, Some("optional_val".to_string()));
        },
    )
}

// ============================================================================
// Nested with Secrets
// ============================================================================

#[derive(EnvConfig)]
struct SecretChild {
    #[env(var = "SEC_DB_PASSWORD", secret)]
    password: String,

    #[env(var = "SEC_DB_USERNAME")]
    username: String,
}

#[derive(EnvConfig)]
#[env_config(prefix = "SEC_")]
struct SecretParent {
    #[env(var = "APP")]
    app_name: String,

    #[env(flatten)]
    database: SecretChild,
}

#[test]
#[serial]
fn test_nested_secrets_redacted() {
    cleanup_env(&["SEC_APP", "SEC_DB_PASSWORD", "SEC_DB_USERNAME"]);

    with_env(
        &[
            ("SEC_APP", "myapp"),
            ("SEC_DB_PASSWORD", "super-secret-password"),
            ("SEC_DB_USERNAME", "admin"),
        ],
        || {
            let config = SecretParent::from_env().expect("should load with secrets");

            assert_eq!(config.database.password, "super-secret-password");

            let debug = format!("{:?}", config);
            assert!(
                !debug.contains("super-secret-password"),
                "Debug should not contain secret"
            );
        },
    )
}

// ============================================================================
// Deep Three-Level Nesting (via direct embedding)
// ============================================================================

#[derive(EnvConfig)]
struct Level3 {
    #[env(var = "L3_VALUE", default = "level3")]
    value: String,
}

#[derive(EnvConfig)]
struct Level2 {
    #[env(var = "L2_VALUE", default = "level2")]
    value: String,

    #[env(flatten)]
    level3: Level3,
}

#[derive(EnvConfig)]
#[env_config(prefix = "DEEP_")]
struct Level1 {
    #[env(var = "VALUE", default = "level1")]
    value: String,

    #[env(flatten)]
    level2: Level2,
}

#[test]
#[serial]
fn test_three_level_nesting() {
    cleanup_env(&["DEEP_VALUE", "L2_VALUE", "L3_VALUE"]);

    let config = Level1::from_env().expect("should load three levels");

    assert_eq!(config.value, "level1");
    assert_eq!(config.level2.value, "level2");
    assert_eq!(config.level2.level3.value, "level3");
}

#[test]
#[serial]
fn test_three_level_override_deepest() {
    cleanup_env(&["DEEP_VALUE", "L2_VALUE", "L3_VALUE"]);

    with_env(&[("L3_VALUE", "custom_deep")], || {
        let config = Level1::from_env().expect("should load with deep override");

        assert_eq!(config.value, "level1");
        assert_eq!(config.level2.value, "level2");
        assert_eq!(config.level2.level3.value, "custom_deep");
    })
}

// ============================================================================
// Same Type Nested Multiple Times
// ============================================================================

#[derive(EnvConfig)]
struct Endpoint {
    #[env(var = "URL", default = "http://localhost")]
    url: String,

    #[env(var = "TIMEOUT", default = "30")]
    timeout: u32,
}

// Test without prefix propagation - flatten uses the nested type's own env vars
#[derive(EnvConfig)]
#[env_config(prefix = "API_")]
struct ApiConfig {
    #[env(var = "NAME", default = "api")]
    name: String,

    #[env(flatten)]
    endpoint: Endpoint,
}

#[test]
#[serial]
fn test_same_type_nested() {
    cleanup_env(&["API_NAME", "URL", "TIMEOUT"]);

    with_env(
        &[
            ("API_NAME", "my_api"),
            ("URL", "https://api.example.com"),
            ("TIMEOUT", "60"),
        ],
        || {
            let config = ApiConfig::from_env().expect("should load");

            assert_eq!(config.name, "my_api");
            assert_eq!(config.endpoint.url, "https://api.example.com");
            assert_eq!(config.endpoint.timeout, 60);
        },
    )
}

// ============================================================================
// Flatten with Prefix Propagation (New Feature)
// ============================================================================
// With prefix support on flatten fields, we can now have multiple flattened
// fields of the same type with different prefixes!

#[derive(EnvConfig)]
#[env_config(prefix = "APP_")]
struct MultiEndpointConfig {
    #[env(var = "NAME", default = "service")]
    name: String,

    // Primary endpoint uses PRIMARY_ prefix (combined with APP_ = APP_PRIMARY_)
    #[env(flatten, prefix = "PRIMARY_")]
    primary: Endpoint,

    // Backup endpoint uses BACKUP_ prefix (combined with APP_ = APP_BACKUP_)
    #[env(flatten, prefix = "BACKUP_")]
    backup: Endpoint,
}

#[test]
#[serial]
fn test_flatten_with_prefix_propagation() {
    cleanup_env(&[
        "APP_NAME",
        "APP_PRIMARY_URL",
        "APP_PRIMARY_TIMEOUT",
        "APP_BACKUP_URL",
        "APP_BACKUP_TIMEOUT",
    ]);

    with_env(
        &[
            ("APP_NAME", "my_service"),
            ("APP_PRIMARY_URL", "https://primary.example.com"),
            ("APP_PRIMARY_TIMEOUT", "30"),
            ("APP_BACKUP_URL", "https://backup.example.com"),
            ("APP_BACKUP_TIMEOUT", "60"),
        ],
        || {
            let config = MultiEndpointConfig::from_env().expect("should load");

            assert_eq!(config.name, "my_service");
            assert_eq!(config.primary.url, "https://primary.example.com");
            assert_eq!(config.primary.timeout, 30);
            assert_eq!(config.backup.url, "https://backup.example.com");
            assert_eq!(config.backup.timeout, 60);
        },
    )
}

#[test]
#[serial]
fn test_flatten_prefix_uses_defaults() {
    cleanup_env(&[
        "APP_NAME",
        "APP_PRIMARY_URL",
        "APP_PRIMARY_TIMEOUT",
        "APP_BACKUP_URL",
        "APP_BACKUP_TIMEOUT",
    ]);

    // Only set some values - others should use defaults
    with_env(
        &[
            ("APP_NAME", "my_service"),
            ("APP_PRIMARY_URL", "https://primary.example.com"),
            // APP_PRIMARY_TIMEOUT not set - should use default "30"
            // APP_BACKUP_URL not set - should use default "http://localhost"
            ("APP_BACKUP_TIMEOUT", "120"),
        ],
        || {
            let config = MultiEndpointConfig::from_env().expect("should load with defaults");

            assert_eq!(config.name, "my_service");
            assert_eq!(config.primary.url, "https://primary.example.com");
            assert_eq!(config.primary.timeout, 30); // default
            assert_eq!(config.backup.url, "http://localhost"); // default
            assert_eq!(config.backup.timeout, 120);
        },
    )
}

// Test flatten prefix without struct-level prefix
#[derive(EnvConfig)]
struct NoPrefixMultiEndpoint {
    #[env(var = "SVC_NAME", default = "service")]
    name: String,

    #[env(flatten, prefix = "MAIN_")]
    main: Endpoint,

    #[env(flatten, prefix = "FALLBACK_")]
    fallback: Endpoint,
}

#[test]
#[serial]
fn test_flatten_prefix_without_struct_prefix() {
    cleanup_env(&[
        "SVC_NAME",
        "MAIN_URL",
        "MAIN_TIMEOUT",
        "FALLBACK_URL",
        "FALLBACK_TIMEOUT",
    ]);

    with_env(
        &[
            ("SVC_NAME", "test_service"),
            ("MAIN_URL", "https://main.example.com"),
            ("MAIN_TIMEOUT", "15"),
            ("FALLBACK_URL", "https://fallback.example.com"),
            ("FALLBACK_TIMEOUT", "45"),
        ],
        || {
            let config = NoPrefixMultiEndpoint::from_env().expect("should load");

            assert_eq!(config.name, "test_service");
            assert_eq!(config.main.url, "https://main.example.com");
            assert_eq!(config.main.timeout, 15);
            assert_eq!(config.fallback.url, "https://fallback.example.com");
            assert_eq!(config.fallback.timeout, 45);
        },
    )
}
