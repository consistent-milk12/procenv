//! Example: Runtime Access (Phase D)
//!
//! This example demonstrates dynamic key-based access to configuration values.
//!
//! Run with:
//! ```bash
//! APP_HOST=localhost APP_PORT=3000 APP_DEBUG=true cargo run --example runtime_access
//! ```

use procenv::{ConfigLoader, ConfigValue, EnvConfig};

#[allow(dead_code)]
#[derive(EnvConfig)]
#[env_config(prefix = "APP_")]
struct AppConfig {
    #[env(var = "HOST", default = "127.0.0.1")]
    host: String,

    #[env(var = "PORT", default = "8080")]
    port: u16,

    #[env(var = "DEBUG", default = "false")]
    debug: bool,

    #[env(var = "API_KEY", secret, default = "dev-key-123")]
    api_key: String,

    #[env(var = "TIMEOUT", optional)]
    timeout: Option<u64>,
}

#[derive(EnvConfig)]
struct DatabaseConfig {
    #[env(var = "DB_HOST", default = "localhost")]
    host: String,

    #[env(var = "DB_PORT", default = "5432")]
    port: u16,
}

#[derive(EnvConfig)]
struct FullConfig {
    #[env(var = "APP_NAME", default = "myapp")]
    name: String,

    #[env(flatten)]
    database: DatabaseConfig,
}

fn main() -> Result<(), procenv::Error> {
    println!("=== Runtime Access Example ===\n");

    // -------------------------------------------------------------------------
    // 1. Static key information
    // -------------------------------------------------------------------------
    println!("1. Available keys for AppConfig:");
    for key in AppConfig::keys() {
        let exists = AppConfig::has_key(key);
        println!("   - {} (exists: {})", key, exists);
    }
    println!();

    // -------------------------------------------------------------------------
    // 2. Load config and access by key
    // -------------------------------------------------------------------------
    let config = AppConfig::from_env()?;

    println!("2. Accessing loaded config by key:");
    println!("   host     = {:?}", config.get_str("host"));
    println!("   port     = {:?}", config.get_str("port"));
    println!("   debug    = {:?}", config.get_str("debug"));
    println!("   api_key  = {:?}", config.get_str("api_key")); // Returns <redacted>
    println!("   timeout  = {:?}", config.get_str("timeout"));
    println!("   unknown  = {:?}", config.get_str("unknown"));
    println!();

    // -------------------------------------------------------------------------
    // 3. Nested config with dot notation
    // -------------------------------------------------------------------------
    let full_config = FullConfig::from_env()?;

    println!("3. Nested config access (dot notation):");
    println!("   name          = {:?}", full_config.get_str("name"));
    println!("   database.host = {:?}", full_config.get_str("database.host"));
    println!("   database.port = {:?}", full_config.get_str("database.port"));
    println!();

    // -------------------------------------------------------------------------
    // 4. ConfigValue type-erased values
    // -------------------------------------------------------------------------
    println!("4. ConfigValue examples:");

    let val = ConfigValue::from_str_infer("8080");
    println!("   Inferred '8080': {:?}", val);
    println!("   As u16: {:?}", val.to_u16());

    let val = ConfigValue::from_str_infer("true");
    println!("   Inferred 'true': {:?}", val);
    println!("   As bool: {:?}", val.as_bool());

    let val = ConfigValue::from_str_infer("3.14");
    println!("   Inferred '3.14': {:?}", val);
    println!("   As f64: {:?}", val.to_f64());

    let val = ConfigValue::from_str_infer("hello");
    println!("   Inferred 'hello': {:?}", val);
    println!("   As str: {:?}", val.as_str());
    println!();

    // -------------------------------------------------------------------------
    // 5. ConfigLoader for partial loading
    // -------------------------------------------------------------------------
    println!("5. ConfigLoader (partial loading without full struct):");

    let mut loader = ConfigLoader::new().with_env();

    // Get raw string
    if let Some(host) = loader.get_str("APP_HOST") {
        println!("   APP_HOST = {}", host);
    } else {
        println!("   APP_HOST not set");
    }

    // Get with type inference
    if let Some(value) = loader.get_value_infer("APP_PORT") {
        println!("   APP_PORT = {} (type: {})", value, value.type_name());
        if let Some(port) = value.to_u16() {
            println!("   APP_PORT as u16 = {}", port);
        }
    }

    // Get with source attribution
    if let Some((value, source)) = loader.get_with_source("APP_DEBUG") {
        println!("   APP_DEBUG = {} (from {:?})", value, source);
    }
    println!();

    // -------------------------------------------------------------------------
    // 6. Numeric conversions with ConfigValue
    // -------------------------------------------------------------------------
    println!("6. Numeric conversions:");

    let val = ConfigValue::Integer(8080);
    println!("   Integer(8080):");
    println!("     to_u16() = {:?}", val.to_u16());
    println!("     to_i32() = {:?}", val.to_i32());
    println!("     cast::<f64>() = {:?}", val.cast::<f64>());

    let val = ConfigValue::UnsignedInteger(u64::MAX);
    println!("   UnsignedInteger(MAX):");
    println!("     to_i64() = {:?} (overflow!)", val.to_i64());
    println!("     to_u64() = {:?}", val.to_u64());
    println!();

    println!("=== Done ===");
    Ok(())
}
