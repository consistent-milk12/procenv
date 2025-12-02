//! Example: Complex nested configuration with flatten
//!
//! This demonstrates multi-level nesting with the flatten feature,
//! showing how environment variables map to deeply nested structures.
//!
//! Run with:
//!   `cargo run --example complex_flatten`
//!
//! Or with env overrides:
//!   `APP_SERVER_PORT=9000 APP_DB_HOST=localhost cargo run --example complex_flatten`

#![allow(
    unused,
    dead_code,
    clippy::no_effect_underscore_binding,
    clippy::struct_field_names,
    clippy::manual_strip,
    clippy::result_large_err
)]

use procenv::EnvConfig;

// ============================================================================
// Level 3: Deepest nested configs
// ============================================================================

/// Database connection pool settings
#[derive(EnvConfig)]
struct PoolConfig {
    #[env(var = "MIN_SIZE", default = "5")]
    min_size: u32,

    #[env(var = "MAX_SIZE", default = "20")]
    max_size: u32,

    #[env(var = "TIMEOUT", default = "30")]
    timeout_seconds: u32,
}

/// Log file settings
#[derive(EnvConfig)]
struct LogFileConfig {
    #[env(var = "ENABLED", default = "false")]
    enabled: bool,

    #[env(var = "PATH", default = "/var/log/app.log")]
    path: String,

    #[env(var = "MAX_SIZE_MB", default = "100")]
    max_size_mb: u32,
}

// ============================================================================
// Level 2: Mid-level configs with nested children
// ============================================================================

/// Server configuration
#[derive(EnvConfig)]
struct ServerConfig {
    #[env(var = "HOST", default = "127.0.0.1")]
    host: String,

    #[env(var = "PORT", default = "8080")]
    port: u16,
}

/// Database configuration with nested pool
#[derive(EnvConfig)]
struct DatabaseConfig {
    #[env(var = "HOST", default = "localhost")]
    host: String,

    #[env(var = "PORT", default = "5432")]
    port: u16,

    #[env(var = "NAME", default = "myapp")]
    name: String,

    #[env(var = "MAX_CONNECTIONS", default = "50")]
    max_connections: u32,

    /// Nested pool configuration
    #[env(flatten, prefix = "POOL_")]
    pool: PoolConfig,
}

/// Cache configuration
#[derive(EnvConfig)]
struct CacheConfig {
    #[env(var = "HOST", default = "localhost")]
    host: String,

    #[env(var = "PORT", default = "6379")]
    port: u16,

    #[env(var = "TTL", default = "3600")]
    ttl_seconds: u32,
}

/// Logging configuration with nested file settings
#[derive(EnvConfig)]
struct LoggingConfig {
    #[env(var = "LEVEL", default = "info")]
    level: String,

    #[env(var = "FORMAT", default = "text")]
    format: String,

    /// Nested file logging configuration
    #[env(flatten, prefix = "FILE_")]
    file: LogFileConfig,
}

// ============================================================================
// Level 1: Top-level config with all sections
// ============================================================================

/// Complete application configuration
#[derive(EnvConfig)]
#[env_config(prefix = "APP_")]
struct AppConfig {
    /// Server settings (APP_SERVER_*)
    #[env(flatten, prefix = "SERVER_")]
    server: ServerConfig,

    /// Database settings (APP_DB_*)
    #[env(flatten, prefix = "DB_")]
    database: DatabaseConfig,

    /// Cache settings (APP_CACHE_*)
    #[env(flatten, prefix = "CACHE_")]
    cache: CacheConfig,

    /// Logging settings (APP_LOG_*)
    #[env(flatten, prefix = "LOG_")]
    logging: LoggingConfig,
}

fn main() {
    println!("=== Complex Flatten Example ===\n");

    // Show the env var mapping
    println!("Environment variable mapping:");
    println!("  APP_SERVER_HOST        -> server.host");
    println!("  APP_SERVER_PORT        -> server.port");
    println!("  APP_DB_HOST            -> database.host");
    println!("  APP_DB_PORT            -> database.port");
    println!("  APP_DB_NAME            -> database.name");
    println!("  APP_DB_POOL_MIN_SIZE   -> database.pool.min_size");
    println!("  APP_DB_POOL_MAX_SIZE   -> database.pool.max_size");
    println!("  APP_DB_POOL_TIMEOUT    -> database.pool.timeout_seconds");
    println!("  APP_CACHE_HOST         -> cache.host");
    println!("  APP_CACHE_PORT         -> cache.port");
    println!("  APP_CACHE_TTL          -> cache.ttl_seconds");
    println!("  APP_LOG_LEVEL          -> logging.level");
    println!("  APP_LOG_FORMAT         -> logging.format");
    println!("  APP_LOG_FILE_ENABLED   -> logging.file.enabled");
    println!("  APP_LOG_FILE_PATH      -> logging.file.path");
    println!("  APP_LOG_FILE_MAX_SIZE_MB -> logging.file.max_size_mb");
    println!();

    // Test 1: Load with all defaults
    println!("1. Loading with defaults:\n");

    match AppConfig::from_env() {
        Ok(config) => {
            println!("{config:#?}");
        }
        Err(e) => {
            eprintln!("Error: {:?}", miette::Report::from(e));
        }
    }

    println!();

    // Test 2: Override some values via env
    println!("2. With environment overrides:\n");

    // SAFETY: Single-threaded example
    unsafe {
        std::env::set_var("APP_SERVER_PORT", "9000");
        std::env::set_var("APP_DB_HOST", "prod-db.example.com");
        std::env::set_var("APP_DB_POOL_MAX_SIZE", "100");
        std::env::set_var("APP_CACHE_TTL", "7200");
        std::env::set_var("APP_LOG_LEVEL", "debug");
        std::env::set_var("APP_LOG_FILE_ENABLED", "true");
    }

    match AppConfig::from_env_with_sources() {
        Ok((config, sources)) => {
            println!("Config:");
            println!("  server.port: {} (was 8080)", config.server.port);
            println!("  database.host: {} (was localhost)", config.database.host);
            println!(
                "  database.pool.max_size: {} (was 20)",
                config.database.pool.max_size
            );
            println!("  cache.ttl: {} (was 3600)", config.cache.ttl_seconds);
            println!("  logging.level: {} (was info)", config.logging.level);
            println!(
                "  logging.file.enabled: {} (was false)",
                config.logging.file.enabled
            );
            println!("\nSources:\n{sources}");
        }
        Err(e) => {
            eprintln!("Error: {:?}", miette::Report::from(e));
        }
    }

    // Cleanup
    unsafe {
        std::env::remove_var("APP_SERVER_PORT");
        std::env::remove_var("APP_DB_HOST");
        std::env::remove_var("APP_DB_POOL_MAX_SIZE");
        std::env::remove_var("APP_CACHE_TTL");
        std::env::remove_var("APP_LOG_LEVEL");
        std::env::remove_var("APP_LOG_FILE_ENABLED");
    }

    // Test 3: Show available keys
    println!("3. Available keys for runtime access:\n");
    for key in AppConfig::keys() {
        println!("   {key}");
    }

    println!("\n=== Done ===");
}
