//! Config-rs Comparison Example
//!
//! This demonstrates what makes config-rs "mature":
//! 1. Multiple file format support (TOML, JSON, YAML, INI, RON)
//! 2. Hierarchical configuration with overrides
//! 3. Environment variable integration
//! 4. Runtime value access without struct
//! 5. Configuration watching (hot reload)
//! 6. Type coercion and conversion
//!
//! Run: cargo run --example comparison_config_rs

use config::{Config, ConfigError, Environment, File, FileFormat, Map, Value};
use serde::Deserialize;
use std::collections::HashMap;

// ============================================================================
// 1. HIERARCHICAL CONFIGURATION
// ============================================================================
//
// Config-rs builds a tree of values from multiple sources.
// Each source can override specific keys.

#[derive(Debug, Deserialize)]
struct Database {
    host: String,
    port: u16,
    name: String,
}

#[derive(Debug, Deserialize)]
struct Server {
    host: String,
    port: u16,
}

#[derive(Debug, Deserialize)]
struct AppSettings {
    debug: bool,
    database: Database,
    server: Server,
}

fn demo_hierarchical() {
    println!("=== 1. HIERARCHICAL CONFIGURATION ===\n");

    // Build configuration from multiple sources
    // Priority: later sources override earlier ones

    let settings = Config::builder()
        // Start with default values
        .set_default("debug", false)
        .unwrap()
        .set_default("database.host", "localhost")
        .unwrap()
        .set_default("database.port", 5432)
        .unwrap()
        .set_default("database.name", "myapp")
        .unwrap()
        .set_default("server.host", "0.0.0.0")
        .unwrap()
        .set_default("server.port", 8080)
        .unwrap()
        // Add file (if exists) - note: won't fail if missing when using required(false)
        // .add_source(File::with_name("config").required(false))
        // Environment variables with prefix
        .add_source(Environment::with_prefix("APP").separator("_"))
        .build();

    match settings {
        Ok(config) => {
            println!("Raw configuration tree:");
            // Access individual values without deserializing to struct
            println!("  debug = {:?}", config.get::<bool>("debug"));
            println!(
                "  database.host = {:?}",
                config.get::<String>("database.host")
            );
            println!("  server.port = {:?}", config.get::<i64>("server.port"));
            println!();

            // Or deserialize to struct
            match config.try_deserialize::<AppSettings>() {
                Ok(app) => println!("Deserialized: {:#?}", app),
                Err(e) => println!("Deserialize error: {}", e),
            }
        }
        Err(e) => println!("Config error: {}", e),
    }

    println!();
}

// ============================================================================
// 2. MULTIPLE FILE FORMATS
// ============================================================================
//
// Config-rs supports TOML, JSON, YAML, INI, RON out of the box.
// You can even mix formats!

fn demo_multiple_formats() {
    println!("=== 2. MULTIPLE FILE FORMATS ===\n");

    // You can explicitly specify format or let it auto-detect from extension
    let toml_source = r#"
        [database]
        host = "db.example.com"
        port = 5432
    "#;

    let json_source = r#"
        {
            "server": {
                "host": "0.0.0.0",
                "port": 9000
            }
        }
    "#;

    let config = Config::builder()
        .set_default("debug", true)
        .unwrap()
        .set_default("database.name", "default_db")
        .unwrap()
        // Mix TOML and JSON sources!
        .add_source(File::from_str(toml_source, FileFormat::Toml))
        .add_source(File::from_str(json_source, FileFormat::Json))
        .build()
        .unwrap();

    println!("Mixed TOML + JSON configuration:");
    println!(
        "  database.host = {:?}",
        config.get::<String>("database.host")
    );
    println!("  database.port = {:?}", config.get::<i64>("database.port"));
    println!(
        "  database.name = {:?}",
        config.get::<String>("database.name")
    );
    println!("  server.host = {:?}", config.get::<String>("server.host"));
    println!("  server.port = {:?}", config.get::<i64>("server.port"));

    println!();
}

// ============================================================================
// 3. RUNTIME VALUE ACCESS
// ============================================================================
//
// You don't need a struct - access any value at runtime by path.
// This is great for dynamic configuration.

fn demo_runtime_access() {
    println!("=== 3. RUNTIME VALUE ACCESS ===\n");

    let config = Config::builder()
        .add_source(File::from_str(
            r#"
            app_name = "MyApp"
            features = ["auth", "logging", "metrics"]
            limits = { max_connections = 100, timeout_ms = 5000 }
            "#,
            FileFormat::Toml,
        ))
        .build()
        .unwrap();

    // Access scalar values
    println!("app_name: {}", config.get_string("app_name").unwrap());

    // Access arrays
    let features: Vec<String> = config
        .get_array("features")
        .unwrap()
        .into_iter()
        .map(|v| v.into_string().unwrap())
        .collect();
    println!("features: {:?}", features);

    // Access nested values with dot notation
    println!(
        "limits.max_connections: {}",
        config.get_int("limits.max_connections").unwrap()
    );

    // Access as table/map
    let limits: HashMap<String, Value> = config.get_table("limits").unwrap();
    println!("limits table: {:?}", limits);

    // Check if key exists
    println!(
        "has 'missing_key': {}",
        config.get::<String>("missing_key").is_ok()
    );

    println!();
}

// ============================================================================
// 4. ENVIRONMENT VARIABLE FEATURES
// ============================================================================
//
// Rich environment variable handling with prefixes, separators, and lists.

fn demo_env_features() {
    println!("=== 4. ENVIRONMENT VARIABLE FEATURES ===\n");

    // Set some env vars for demo
    // Note: config-rs lowercases all keys, and uses prefix_separator (default "_")
    // MYAPP_DATABASE_HOST -> database_host (with separator "_" -> database.host)
    unsafe {
        std::env::set_var("MYAPP_DATABASE_HOST", "env-db-host");
        std::env::set_var("MYAPP_DATABASE_PORT", "6543");
        std::env::set_var("MYAPP_FEATURES", "auth,metrics,tracing");
        std::env::set_var("MYAPP_DEBUG", "true");
    }

    let config = Config::builder()
        .set_default("database.host", "default-host")
        .unwrap()
        .set_default("database.port", 5432)
        .unwrap()
        .set_default("debug", false)
        .unwrap()
        // Environment variables with prefix "MYAPP_"
        // The separator splits nested keys: MYAPP_DATABASE_HOST -> database.host
        .add_source(
            Environment::with_prefix("MYAPP")
                .separator("_")
                .try_parsing(true)
                .list_separator(",")
                .with_list_parse_key("features"),
        )
        .build()
        .unwrap();

    println!("Environment override examples:");
    println!(
        "  database.host = {} (from MYAPP_DATABASE_HOST)",
        config.get_string("database.host").unwrap()
    );
    println!(
        "  database.port = {} (from MYAPP_DATABASE_PORT)",
        config.get_int("database.port").unwrap()
    );
    println!(
        "  debug = {} (from MYAPP_DEBUG)",
        config.get_bool("debug").unwrap()
    );

    // List parsing from comma-separated env var
    let features: Vec<String> = config
        .get_array("features")
        .unwrap_or_default()
        .into_iter()
        .filter_map(|v| v.into_string().ok())
        .collect();
    println!("  features = {:?} (from MYAPP_FEATURES)", features);

    // Cleanup
    unsafe {
        std::env::remove_var("MYAPP_DATABASE_HOST");
        std::env::remove_var("MYAPP_DATABASE_PORT");
        std::env::remove_var("MYAPP_FEATURES");
        std::env::remove_var("MYAPP_DEBUG");
    }

    println!();
}

// ============================================================================
// 5. TYPE COERCION
// ============================================================================
//
// Config-rs automatically coerces types where sensible.

fn demo_type_coercion() {
    println!("=== 5. TYPE COERCION ===\n");

    let config = Config::builder()
        .add_source(File::from_str(
            r#"
            string_number = "42"
            actual_number = 42
            string_bool = "true"
            numeric_bool = 1
            "#,
            FileFormat::Toml,
        ))
        .build()
        .unwrap();

    // Strings can be parsed to numbers
    println!(
        "string_number as i64: {}",
        config.get_int("string_number").unwrap()
    );

    // Numbers work as numbers
    println!(
        "actual_number as i64: {}",
        config.get_int("actual_number").unwrap()
    );

    // String "true"/"false" -> bool
    println!(
        "string_bool as bool: {}",
        config.get_bool("string_bool").unwrap()
    );

    // Numeric 0/1 -> bool
    println!(
        "numeric_bool as bool: {}",
        config.get_bool("numeric_bool").unwrap()
    );

    println!();
}

// ============================================================================
// 6. SOURCE TRACKING (Origin)
// ============================================================================
//
// Config-rs tracks where values came from.

fn demo_source_tracking() {
    println!("=== 6. SOURCE TRACKING ===\n");

    unsafe {
        std::env::set_var("TRACK_PORT", "9999");
    }

    let config = Config::builder()
        .set_default("host", "localhost")
        .unwrap()
        .set_default("port", 8080)
        .unwrap()
        .add_source(Environment::with_prefix("TRACK"))
        .build()
        .unwrap();

    // Get value with origin information
    if let Ok(value) = config.get::<Value>("host") {
        println!("host = {:?}", value);
        println!("  origin: {:?}", value.origin());
    }

    if let Ok(value) = config.get::<Value>("port") {
        println!("port = {:?}", value);
        println!("  origin: {:?}", value.origin());
    }

    unsafe {
        std::env::remove_var("TRACK_PORT");
    }

    println!();
}

// ============================================================================
// MAIN
// ============================================================================

fn main() {
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║          CONFIG-RS - What Makes It 'Mature'                  ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    demo_hierarchical();
    demo_multiple_formats();
    demo_runtime_access();
    demo_env_features();
    demo_type_coercion();
    demo_source_tracking();

    println!("═══════════════════════════════════════════════════════════════");
    println!("KEY TAKEAWAYS:");
    println!("═══════════════════════════════════════════════════════════════");
    println!("1. Multiple file formats (TOML, JSON, YAML, INI, RON)");
    println!("2. Hierarchical config with layered overrides");
    println!("3. Runtime value access without struct definition");
    println!("4. Rich env var handling (prefixes, separators, lists)");
    println!("5. Automatic type coercion");
    println!("6. Source/origin tracking for values");
    println!();
    println!("WHAT CONFIG-RS LACKS (that procenv has):");
    println!("- Error accumulation (fails on first error)");
    println!("- .env.example generation");
    println!("- miette-style rich diagnostics");
    println!("- Compile-time derive macro (config-rs is runtime-only)");
}
