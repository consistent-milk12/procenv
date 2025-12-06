//! Complex nested configuration hierarchy tests.
//!
//! Tests for deeply nested structs and complex configuration hierarchies.

#![allow(clippy::pedantic)]
#![allow(clippy::manual_strip)]

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
    );
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
    );
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
    });
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
    );
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
    );
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

            let debug = format!("{config:?}");
            assert!(
                !debug.contains("super-secret-password"),
                "Debug should not contain secret"
            );
        },
    );
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
    });
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
    );
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
    );
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
    );
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
    );
}

// ============================================================================
// Complex 3-Level Deep Nesting with Prefix Propagation
// ============================================================================

/// Database connection pool settings (Level 3)
#[derive(EnvConfig)]
struct PoolSettings {
    #[env(var = "MIN_SIZE", default = "5")]
    min_size: u32,

    #[env(var = "MAX_SIZE", default = "20")]
    max_size: u32,

    #[env(var = "TIMEOUT", default = "30")]
    timeout_seconds: u32,
}

/// Log file settings (Level 3)
#[derive(EnvConfig)]
struct LogFileSettings {
    #[env(var = "ENABLED", default = "false")]
    enabled: bool,

    #[env(var = "PATH", default = "/var/log/app.log")]
    path: String,

    #[env(var = "MAX_SIZE_MB", default = "100")]
    max_size_mb: u32,
}

/// Server configuration (Level 2)
#[derive(EnvConfig)]
struct ServerSettings {
    #[env(var = "HOST", default = "127.0.0.1")]
    host: String,

    #[env(var = "PORT", default = "8080")]
    port: u16,
}

/// Database configuration with nested pool (Level 2)
#[derive(EnvConfig)]
struct DatabaseSettings {
    #[env(var = "HOST", default = "localhost")]
    host: String,

    #[env(var = "PORT", default = "5432")]
    port: u16,

    #[env(var = "NAME", default = "myapp")]
    name: String,

    #[env(var = "MAX_CONNECTIONS", default = "50")]
    max_connections: u32,

    #[env(flatten, prefix = "POOL_")]
    pool: PoolSettings,
}

/// Cache configuration (Level 2)
#[derive(EnvConfig)]
struct CacheSettings {
    #[env(var = "HOST", default = "localhost")]
    host: String,

    #[env(var = "PORT", default = "6379")]
    port: u16,

    #[env(var = "TTL", default = "3600")]
    ttl_seconds: u32,
}

/// Logging configuration with nested file settings (Level 2)
#[derive(EnvConfig)]
struct LoggingSettings {
    #[env(var = "LEVEL", default = "info")]
    level: String,

    #[env(var = "FORMAT", default = "text")]
    format: String,

    #[env(flatten, prefix = "FILE_")]
    file: LogFileSettings,
}

/// Complete application configuration (Level 1)
#[derive(EnvConfig)]
#[env_config(prefix = "COMPLEX_")]
struct ComplexAppConfig {
    #[env(flatten, prefix = "SERVER_")]
    server: ServerSettings,

    #[env(flatten, prefix = "DB_")]
    database: DatabaseSettings,

    #[env(flatten, prefix = "CACHE_")]
    cache: CacheSettings,

    #[env(flatten, prefix = "LOG_")]
    logging: LoggingSettings,
}

#[test]
#[serial]
fn test_complex_three_level_defaults() {
    cleanup_env(&[
        "COMPLEX_SERVER_HOST",
        "COMPLEX_SERVER_PORT",
        "COMPLEX_DB_HOST",
        "COMPLEX_DB_PORT",
        "COMPLEX_DB_NAME",
        "COMPLEX_DB_MAX_CONNECTIONS",
        "COMPLEX_DB_POOL_MIN_SIZE",
        "COMPLEX_DB_POOL_MAX_SIZE",
        "COMPLEX_DB_POOL_TIMEOUT",
        "COMPLEX_CACHE_HOST",
        "COMPLEX_CACHE_PORT",
        "COMPLEX_CACHE_TTL",
        "COMPLEX_LOG_LEVEL",
        "COMPLEX_LOG_FORMAT",
        "COMPLEX_LOG_FILE_ENABLED",
        "COMPLEX_LOG_FILE_PATH",
        "COMPLEX_LOG_FILE_MAX_SIZE_MB",
    ]);

    let config = ComplexAppConfig::from_env().expect("should load with defaults");

    // Server defaults
    assert_eq!(config.server.host, "127.0.0.1");
    assert_eq!(config.server.port, 8080);

    // Database defaults
    assert_eq!(config.database.host, "localhost");
    assert_eq!(config.database.port, 5432);
    assert_eq!(config.database.name, "myapp");
    assert_eq!(config.database.max_connections, 50);

    // Database pool defaults (3rd level)
    assert_eq!(config.database.pool.min_size, 5);
    assert_eq!(config.database.pool.max_size, 20);
    assert_eq!(config.database.pool.timeout_seconds, 30);

    // Cache defaults
    assert_eq!(config.cache.host, "localhost");
    assert_eq!(config.cache.port, 6379);
    assert_eq!(config.cache.ttl_seconds, 3600);

    // Logging defaults
    assert_eq!(config.logging.level, "info");
    assert_eq!(config.logging.format, "text");

    // Log file defaults (3rd level)
    assert!(!config.logging.file.enabled);
    assert_eq!(config.logging.file.path, "/var/log/app.log");
    assert_eq!(config.logging.file.max_size_mb, 100);
}

#[test]
#[serial]
fn test_complex_three_level_full_override() {
    cleanup_env(&[
        "COMPLEX_SERVER_HOST",
        "COMPLEX_SERVER_PORT",
        "COMPLEX_DB_HOST",
        "COMPLEX_DB_PORT",
        "COMPLEX_DB_NAME",
        "COMPLEX_DB_MAX_CONNECTIONS",
        "COMPLEX_DB_POOL_MIN_SIZE",
        "COMPLEX_DB_POOL_MAX_SIZE",
        "COMPLEX_DB_POOL_TIMEOUT",
        "COMPLEX_CACHE_HOST",
        "COMPLEX_CACHE_PORT",
        "COMPLEX_CACHE_TTL",
        "COMPLEX_LOG_LEVEL",
        "COMPLEX_LOG_FORMAT",
        "COMPLEX_LOG_FILE_ENABLED",
        "COMPLEX_LOG_FILE_PATH",
        "COMPLEX_LOG_FILE_MAX_SIZE_MB",
    ]);

    with_env(
        &[
            ("COMPLEX_SERVER_HOST", "0.0.0.0"),
            ("COMPLEX_SERVER_PORT", "9000"),
            ("COMPLEX_DB_HOST", "prod-db.example.com"),
            ("COMPLEX_DB_PORT", "5433"),
            ("COMPLEX_DB_NAME", "production"),
            ("COMPLEX_DB_MAX_CONNECTIONS", "100"),
            ("COMPLEX_DB_POOL_MIN_SIZE", "10"),
            ("COMPLEX_DB_POOL_MAX_SIZE", "50"),
            ("COMPLEX_DB_POOL_TIMEOUT", "60"),
            ("COMPLEX_CACHE_HOST", "redis.example.com"),
            ("COMPLEX_CACHE_PORT", "6380"),
            ("COMPLEX_CACHE_TTL", "7200"),
            ("COMPLEX_LOG_LEVEL", "debug"),
            ("COMPLEX_LOG_FORMAT", "json"),
            ("COMPLEX_LOG_FILE_ENABLED", "true"),
            ("COMPLEX_LOG_FILE_PATH", "/var/log/production.log"),
            ("COMPLEX_LOG_FILE_MAX_SIZE_MB", "500"),
        ],
        || {
            let config = ComplexAppConfig::from_env().expect("should load with all overrides");

            // Server overrides
            assert_eq!(config.server.host, "0.0.0.0");
            assert_eq!(config.server.port, 9000);

            // Database overrides
            assert_eq!(config.database.host, "prod-db.example.com");
            assert_eq!(config.database.port, 5433);
            assert_eq!(config.database.name, "production");
            assert_eq!(config.database.max_connections, 100);

            // Pool overrides (3rd level)
            assert_eq!(config.database.pool.min_size, 10);
            assert_eq!(config.database.pool.max_size, 50);
            assert_eq!(config.database.pool.timeout_seconds, 60);

            // Cache overrides
            assert_eq!(config.cache.host, "redis.example.com");
            assert_eq!(config.cache.port, 6380);
            assert_eq!(config.cache.ttl_seconds, 7200);

            // Logging overrides
            assert_eq!(config.logging.level, "debug");
            assert_eq!(config.logging.format, "json");

            // Log file overrides (3rd level)
            assert!(config.logging.file.enabled);
            assert_eq!(config.logging.file.path, "/var/log/production.log");
            assert_eq!(config.logging.file.max_size_mb, 500);
        },
    );
}

#[test]
#[serial]
fn test_complex_selective_override() {
    cleanup_env(&[
        "COMPLEX_SERVER_HOST",
        "COMPLEX_SERVER_PORT",
        "COMPLEX_DB_HOST",
        "COMPLEX_DB_PORT",
        "COMPLEX_DB_NAME",
        "COMPLEX_DB_MAX_CONNECTIONS",
        "COMPLEX_DB_POOL_MIN_SIZE",
        "COMPLEX_DB_POOL_MAX_SIZE",
        "COMPLEX_DB_POOL_TIMEOUT",
        "COMPLEX_CACHE_HOST",
        "COMPLEX_CACHE_PORT",
        "COMPLEX_CACHE_TTL",
        "COMPLEX_LOG_LEVEL",
        "COMPLEX_LOG_FORMAT",
        "COMPLEX_LOG_FILE_ENABLED",
        "COMPLEX_LOG_FILE_PATH",
        "COMPLEX_LOG_FILE_MAX_SIZE_MB",
    ]);

    // Only override some values at each level
    with_env(
        &[
            ("COMPLEX_SERVER_PORT", "9000"),      // Level 2
            ("COMPLEX_DB_HOST", "prod-db"),       // Level 2
            ("COMPLEX_DB_POOL_MAX_SIZE", "100"),  // Level 3
            ("COMPLEX_LOG_FILE_ENABLED", "true"), // Level 3
        ],
        || {
            let config =
                ComplexAppConfig::from_env().expect("should load with selective overrides");

            // Server: port overridden, host default
            assert_eq!(config.server.host, "127.0.0.1");
            assert_eq!(config.server.port, 9000);

            // Database: host overridden, others default
            assert_eq!(config.database.host, "prod-db");
            assert_eq!(config.database.port, 5432);
            assert_eq!(config.database.name, "myapp");

            // Pool: max_size overridden, others default
            assert_eq!(config.database.pool.min_size, 5);
            assert_eq!(config.database.pool.max_size, 100);
            assert_eq!(config.database.pool.timeout_seconds, 30);

            // Cache: all defaults
            assert_eq!(config.cache.host, "localhost");
            assert_eq!(config.cache.port, 6379);

            // Log file: enabled overridden, others default
            assert!(config.logging.file.enabled);
            assert_eq!(config.logging.file.path, "/var/log/app.log");
        },
    );
}

#[test]
#[serial]
fn test_complex_source_attribution_deep() {
    cleanup_env(&[
        "COMPLEX_SERVER_HOST",
        "COMPLEX_SERVER_PORT",
        "COMPLEX_DB_HOST",
        "COMPLEX_DB_PORT",
        "COMPLEX_DB_NAME",
        "COMPLEX_DB_MAX_CONNECTIONS",
        "COMPLEX_DB_POOL_MIN_SIZE",
        "COMPLEX_DB_POOL_MAX_SIZE",
        "COMPLEX_DB_POOL_TIMEOUT",
        "COMPLEX_CACHE_HOST",
        "COMPLEX_CACHE_PORT",
        "COMPLEX_CACHE_TTL",
        "COMPLEX_LOG_LEVEL",
        "COMPLEX_LOG_FORMAT",
        "COMPLEX_LOG_FILE_ENABLED",
        "COMPLEX_LOG_FILE_PATH",
        "COMPLEX_LOG_FILE_MAX_SIZE_MB",
    ]);

    with_env(
        &[
            ("COMPLEX_SERVER_PORT", "9000"),
            ("COMPLEX_DB_POOL_MAX_SIZE", "100"),
            ("COMPLEX_LOG_FILE_ENABLED", "true"),
        ],
        || {
            let (_config, sources) =
                ComplexAppConfig::from_env_with_sources().expect("should load with sources");

            // Check server.port is from Environment
            let server_port_src = sources.get("server.port").expect("should have server.port");
            assert!(
                matches!(server_port_src.source, procenv::Source::Environment),
                "server.port should be from Environment"
            );

            // Check server.host is from Default
            let server_host_src = sources.get("server.host").expect("should have server.host");
            assert!(
                matches!(server_host_src.source, procenv::Source::Default),
                "server.host should be from Default"
            );

            // Check 3rd level: database.pool.max_size from Environment
            let pool_max_src = sources
                .get("database.pool.max_size")
                .expect("should have database.pool.max_size");
            assert!(
                matches!(pool_max_src.source, procenv::Source::Environment),
                "database.pool.max_size should be from Environment"
            );

            // Check 3rd level: database.pool.min_size from Default
            let pool_min_src = sources
                .get("database.pool.min_size")
                .expect("should have database.pool.min_size");
            assert!(
                matches!(pool_min_src.source, procenv::Source::Default),
                "database.pool.min_size should be from Default"
            );

            // Check 3rd level: logging.file.enabled from Environment
            let log_enabled_src = sources
                .get("logging.file.enabled")
                .expect("should have logging.file.enabled");
            assert!(
                matches!(log_enabled_src.source, procenv::Source::Environment),
                "logging.file.enabled should be from Environment"
            );
        },
    );
}

#[test]
#[serial]
fn test_complex_count_all_fields() {
    cleanup_env(&[
        "COMPLEX_SERVER_HOST",
        "COMPLEX_SERVER_PORT",
        "COMPLEX_DB_HOST",
        "COMPLEX_DB_PORT",
        "COMPLEX_DB_NAME",
        "COMPLEX_DB_MAX_CONNECTIONS",
        "COMPLEX_DB_POOL_MIN_SIZE",
        "COMPLEX_DB_POOL_MAX_SIZE",
        "COMPLEX_DB_POOL_TIMEOUT",
        "COMPLEX_CACHE_HOST",
        "COMPLEX_CACHE_PORT",
        "COMPLEX_CACHE_TTL",
        "COMPLEX_LOG_LEVEL",
        "COMPLEX_LOG_FORMAT",
        "COMPLEX_LOG_FILE_ENABLED",
        "COMPLEX_LOG_FILE_PATH",
        "COMPLEX_LOG_FILE_MAX_SIZE_MB",
    ]);

    let (_config, sources) =
        ComplexAppConfig::from_env_with_sources().expect("should load with sources");

    // Count total fields: should be 17
    // Server: 2 (host, port)
    // Database: 4 (host, port, name, max_connections)
    // Database.Pool: 3 (min_size, max_size, timeout_seconds)
    // Cache: 3 (host, port, ttl_seconds)
    // Logging: 2 (level, format)
    // Logging.File: 3 (enabled, path, max_size_mb)
    // Total: 2 + 4 + 3 + 3 + 2 + 3 = 17

    let field_count = sources.iter().count();
    assert_eq!(field_count, 17, "Should have 17 total fields");
}

// ============================================================================
// Error Accumulation Across Nested Levels
// ============================================================================

#[derive(EnvConfig)]
struct RequiredPool {
    #[env(var = "MIN")]
    min: u32,

    #[env(var = "MAX")]
    max: u32,
}

#[derive(EnvConfig)]
struct RequiredDb {
    #[env(var = "HOST")]
    host: String,

    #[env(flatten, prefix = "POOL_")]
    pool: RequiredPool,
}

#[derive(EnvConfig)]
#[env_config(prefix = "REQ_")]
struct RequiredConfig {
    #[env(var = "NAME")]
    name: String,

    #[env(flatten, prefix = "DB_")]
    database: RequiredDb,
}

#[test]
#[serial]
fn test_complex_error_accumulation() {
    cleanup_env(&[
        "REQ_NAME",
        "REQ_DB_HOST",
        "REQ_DB_POOL_MIN",
        "REQ_DB_POOL_MAX",
    ]);

    // Don't set any values - should get multiple errors
    let result = RequiredConfig::from_env();

    assert!(result.is_err(), "Should fail with missing required fields");

    let err = result.unwrap_err();
    let err_str = format!("{err:?}");

    // Should mention all missing fields
    assert!(err_str.contains("REQ_NAME"), "Should mention REQ_NAME");
    assert!(
        err_str.contains("REQ_DB_HOST"),
        "Should mention REQ_DB_HOST"
    );
    assert!(
        err_str.contains("REQ_DB_POOL_MIN"),
        "Should mention REQ_DB_POOL_MIN"
    );
    assert!(
        err_str.contains("REQ_DB_POOL_MAX"),
        "Should mention REQ_DB_POOL_MAX"
    );
}

#[test]
#[serial]
fn test_complex_partial_error() {
    cleanup_env(&[
        "REQ_NAME",
        "REQ_DB_HOST",
        "REQ_DB_POOL_MIN",
        "REQ_DB_POOL_MAX",
    ]);

    // Set some but not all values
    with_env(
        &[
            ("REQ_NAME", "myapp"),
            ("REQ_DB_HOST", "localhost"),
            // Missing: REQ_DB_POOL_MIN and REQ_DB_POOL_MAX
        ],
        || {
            let result = RequiredConfig::from_env();

            assert!(result.is_err(), "Should fail with missing pool fields");

            let err = result.unwrap_err();
            let err_str = format!("{err:?}");

            // Should NOT mention the fields we set
            assert!(
                !err_str.contains("REQ_NAME="),
                "Should not complain about REQ_NAME"
            );
            assert!(
                !err_str.contains("REQ_DB_HOST="),
                "Should not complain about REQ_DB_HOST"
            );

            // Should mention the missing pool fields
            assert!(
                err_str.contains("REQ_DB_POOL_MIN"),
                "Should mention REQ_DB_POOL_MIN"
            );
            assert!(
                err_str.contains("REQ_DB_POOL_MAX"),
                "Should mention REQ_DB_POOL_MAX"
            );
        },
    );
}

// ============================================================================
// Mixed Types in Nested Config
// ============================================================================

#[derive(EnvConfig)]
struct MixedTypesChild {
    #[env(var = "STRING_VAL", default = "default")]
    string_val: String,

    #[env(var = "INT_VAL", default = "42")]
    int_val: i32,

    #[env(var = "FLOAT_VAL", default = "3.14")]
    float_val: f64,

    #[env(var = "BOOL_VAL", default = "true")]
    bool_val: bool,

    #[env(var = "OPTIONAL_VAL", optional)]
    optional_val: Option<String>,
}

#[derive(EnvConfig)]
#[env_config(prefix = "MIX_")]
struct MixedTypesParent {
    #[env(var = "NAME", default = "mixed")]
    name: String,

    #[env(flatten, prefix = "CHILD_")]
    child: MixedTypesChild,
}

#[test]
#[serial]
fn test_mixed_types_defaults() {
    cleanup_env(&[
        "MIX_NAME",
        "MIX_CHILD_STRING_VAL",
        "MIX_CHILD_INT_VAL",
        "MIX_CHILD_FLOAT_VAL",
        "MIX_CHILD_BOOL_VAL",
        "MIX_CHILD_OPTIONAL_VAL",
    ]);

    let config = MixedTypesParent::from_env().expect("should load with defaults");

    assert_eq!(config.name, "mixed");
    assert_eq!(config.child.string_val, "default");
    assert_eq!(config.child.int_val, 42);
    assert!((config.child.float_val - 3.14).abs() < 0.001);
    assert!(config.child.bool_val);
    assert!(config.child.optional_val.is_none());
}

#[test]
#[serial]
fn test_mixed_types_override() {
    cleanup_env(&[
        "MIX_NAME",
        "MIX_CHILD_STRING_VAL",
        "MIX_CHILD_INT_VAL",
        "MIX_CHILD_FLOAT_VAL",
        "MIX_CHILD_BOOL_VAL",
        "MIX_CHILD_OPTIONAL_VAL",
    ]);

    with_env(
        &[
            ("MIX_NAME", "custom"),
            ("MIX_CHILD_STRING_VAL", "overridden"),
            ("MIX_CHILD_INT_VAL", "-100"),
            ("MIX_CHILD_FLOAT_VAL", "2.718"),
            ("MIX_CHILD_BOOL_VAL", "false"),
            ("MIX_CHILD_OPTIONAL_VAL", "present"),
        ],
        || {
            let config = MixedTypesParent::from_env().expect("should load with overrides");

            assert_eq!(config.name, "custom");
            assert_eq!(config.child.string_val, "overridden");
            assert_eq!(config.child.int_val, -100);
            assert!((config.child.float_val - 2.718).abs() < 0.001);
            assert!(!config.child.bool_val);
            assert_eq!(config.child.optional_val, Some("present".to_string()));
        },
    );
}

#[test]
#[serial]
fn test_mixed_types_parse_error() {
    cleanup_env(&[
        "MIX_NAME",
        "MIX_CHILD_STRING_VAL",
        "MIX_CHILD_INT_VAL",
        "MIX_CHILD_FLOAT_VAL",
        "MIX_CHILD_BOOL_VAL",
        "MIX_CHILD_OPTIONAL_VAL",
    ]);

    with_env(
        &[
            ("MIX_CHILD_INT_VAL", "not_a_number"),
            ("MIX_CHILD_FLOAT_VAL", "also_not_a_number"),
        ],
        || {
            let result = MixedTypesParent::from_env();

            assert!(result.is_err(), "Should fail with parse errors");

            let err = result.unwrap_err();
            let err_str = format!("{err:?}");

            // Should accumulate both parse errors
            assert!(
                err_str.contains("MIX_CHILD_INT_VAL"),
                "Should mention INT_VAL"
            );
            assert!(
                err_str.contains("MIX_CHILD_FLOAT_VAL"),
                "Should mention FLOAT_VAL"
            );
        },
    );
}
