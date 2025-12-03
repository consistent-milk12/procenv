//! Example: File-based configuration with layered loading
//!
//! This demonstrates TWO approaches to file configuration:
//!
//! 1. **Simple (serde-free)**: Use `#[derive(EnvConfig)]` + `from_config()`
//!    - No serde dependency needed on your structs
//!    - Best for most use cases
//!
//! 2. **Advanced (ConfigBuilder)**: Use `ConfigBuilder` directly
//!    - Requires serde's `Deserialize` derive
//!    - More control: custom defaults, multiple files, programmatic config
//!
//! Run with:
//!   `cargo run --example file_config --features file-all`
//!
//! Or with env override:
//!   `APP_PORT=9000 cargo run --example file_config --features file-all`

#![allow(
    unused,
    dead_code,
    clippy::no_effect_underscore_binding,
    clippy::struct_field_names,
    clippy::manual_strip,
    clippy::result_large_err
)]

use procenv::serde::Deserialize;
use procenv::{ConfigBuilder, EnvConfig};

// ============================================================================
// APPROACH 1: Simple (Serde-Free) - Recommended for most cases
// ============================================================================

/// Database configuration - NO serde needed!
#[derive(EnvConfig)]
struct SimpleDatabase {
    #[env(var = "DB_HOST", default = "localhost")]
    host: String,
    #[env(var = "DB_PORT", default = "5432")]
    port: u16,
    #[env(var = "DB_NAME", default = "myapp")]
    name: String,
}

/// Simple config using EnvConfig - NO serde needed!
#[derive(EnvConfig)]
#[env_config(prefix = "SIMPLE_", file_optional = "crates/procenv/data/config.toml")]
struct SimpleConfig {
    #[env(var = "NAME", default = "myapp")]
    name: String,
    #[env(var = "PORT", default = "8080")]
    port: u16,
    #[env(var = "DEBUG", default = "false")]
    debug: bool,
    #[env(flatten)]
    database: SimpleDatabase,
}

// ============================================================================
// APPROACH 2: Advanced (ConfigBuilder) - For complex scenarios
// ============================================================================

/// Database configuration (nested) - requires Deserialize for ConfigBuilder
#[derive(Debug, Deserialize, Default)]
#[serde(crate = "procenv::serde")]
struct DatabaseConfig {
    #[serde(default = "default_host")]
    host: String,
    #[serde(default = "default_db_port")]
    port: u16,
    #[serde(default = "default_db_name")]
    name: String,
}

fn default_host() -> String {
    "localhost".to_string()
}
fn default_db_port() -> u16 {
    5432
}
fn default_db_name() -> String {
    "myapp".to_string()
}

/// Application configuration - requires Deserialize for ConfigBuilder
#[derive(Debug, Deserialize)]
#[serde(crate = "procenv::serde")]
struct AppConfig {
    /// Application name
    #[serde(default = "default_name")]
    name: String,

    /// Server port
    #[serde(default = "default_port")]
    port: u16,

    /// Debug mode
    #[serde(default)]
    debug: bool,

    /// Database settings
    #[serde(default)]
    database: DatabaseConfig,
}

fn default_name() -> String {
    "myapp".to_string()
}
fn default_port() -> u16 {
    8080
}

fn main() {
    println!("=== File Configuration Example ===\n");

    // ========================================================================
    // APPROACH 1: Simple serde-free loading with EnvConfig
    // ========================================================================
    println!("=== APPROACH 1: Serde-Free (Recommended) ===\n");

    match SimpleConfig::from_config() {
        Ok(config) => {
            println!("Loaded with just #[derive(EnvConfig)] - no serde needed!");
            println!("   name:     {}", config.name);
            println!("   port:     {}", config.port);
            println!("   debug:    {}", config.debug);
            println!(
                "   database: {}:{}/{}",
                config.database.host, config.database.port, config.database.name
            );
        }
        Err(e) => eprintln!("   Error: {e}"),
    }

    println!();

    // ========================================================================
    // APPROACH 2: Advanced ConfigBuilder (requires serde)
    // ========================================================================
    println!("=== APPROACH 2: ConfigBuilder (Advanced) ===\n");

    // Method 1: Load from file using ConfigBuilder
    println!("1. Loading from TOML file:\n");

    let config: Result<AppConfig, _> = ConfigBuilder::new()
        .file_optional("crates/procenv/data/config.toml")
        .env_prefix("APP_")
        .build();

    match config {
        Ok(config) => {
            println!("   name:     {}", config.name);
            println!("   port:     {}", config.port);
            println!("   debug:    {}", config.debug);
            println!(
                "   database: {}:{}/{}",
                config.database.host, config.database.port, config.database.name
            );
        }
        Err(e) => {
            eprintln!("   Error loading config: {e}");
        }
    }

    println!();

    // Method 2: Environment variable override
    println!("2. With environment variable override:\n");

    unsafe {
        std::env::set_var("APP_PORT", "9000");
        std::env::set_var("APP_DEBUG", "true");
    }

    let config: Result<AppConfig, _> = ConfigBuilder::new()
        .file_optional("crates/procenv/data/config.toml")
        .env_prefix("APP_")
        .build();

    match config {
        Ok(config) => {
            println!("   name:     {} (from file)", config.name);
            println!("   port:     {} (from APP_PORT env)", config.port);
            println!("   debug:    {} (from APP_DEBUG env)", config.debug);
        }
        Err(e) => {
            eprintln!("   Error: {e}");
        }
    }

    unsafe {
        std::env::remove_var("APP_PORT");
        std::env::remove_var("APP_DEBUG");
    }

    println!();

    // Method 3: Defaults, then file override
    println!("3. Defaults + file override:\n");

    // Defaults are the base, file values override them
    let defaults = serde_json::json!({
        "name": "default-app",
        "port": 3000,
        "debug": true,
        "database": {
            "host": "default-host",
            "port": 1234,
            "name": "default-db"
        }
    });

    let config: Result<AppConfig, _> = ConfigBuilder::new()
        .defaults_value(defaults) // Start with defaults
        .file_optional("crates/procenv/data/config.toml") // File overrides defaults
        .env_prefix("APP_")
        .build();

    match config {
        Ok(config) => {
            println!(
                "   name:     {} (file overrides default 'default-app')",
                config.name
            );
            println!("   port:     {} (file overrides default 3000)", config.port);
            println!(
                "   debug:    {} (file overrides default true)",
                config.debug
            );
            println!(
                "   database: {}:{}/{} (file overrides defaults)",
                config.database.host, config.database.port, config.database.name
            );
        }
        Err(e) => {
            eprintln!("   Error: {e}");
        }
    }

    // Method 4: Test all file formats
    println!("\n4. Testing all file formats:\n");

    // JSON
    let config: Result<AppConfig, _> = ConfigBuilder::new()
        .file_optional("crates/procenv/data/config.json")
        .build();

    match config {
        Ok(config) => {
            println!("   JSON:  name={}, port={}, db_host={}", config.name, config.port, config.database.host);
        }
        Err(e) => eprintln!("   JSON Error: {e}"),
    }

    // YAML
    let config: Result<AppConfig, _> = ConfigBuilder::new()
        .file_optional("crates/procenv/data/config.yaml")
        .build();

    match config {
        Ok(config) => {
            println!("   YAML: name={}, port={}, db_host={}", config.name, config.port, config.database.host);
        }
        Err(e) => eprintln!("   YAML Error: {e}"),
    }

    // YML
    let config: Result<AppConfig, _> = ConfigBuilder::new()
        .file_optional("crates/procenv/data/config.yml")
        .build();

    match config {
        Ok(config) => {
            println!("   YML:  name={}, port={}, db_host={}", config.name, config.port, config.database.host);
        }
        Err(e) => eprintln!("   YML Error: {e}"),
    }

    println!("\n=== Priority Order ===");
    println!("1. Defaults (lowest)");
    println!("2. Config files (in order added)");
    println!("3. Environment variables (highest)");
    println!();
    println!("Supported formats: TOML, JSON, YAML (with respective features)");
}
