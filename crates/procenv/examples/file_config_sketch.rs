//! Sketch: How file config support could work
//!
//! This demonstrates the ARCHITECTURE - not a working implementation yet.
//! The idea: leverage serde + existing format crates, just add layering logic.

use serde::Deserialize;
use serde_json::Value;
use std::fs;

// ============================================================================
// THE VISION: What the API could look like
// ============================================================================
//
// ```rust
// #[derive(EnvConfig, Deserialize)]
// #[env_config(
//     file = "config.toml",           // Primary config file
//     file = "config.local.toml",     // Optional override file
//     dotenv,                          // Load .env
//     prefix = "APP_"                  // Env var prefix
// )]
// struct Config {
//     #[env(var = "DATABASE_URL")]
//     database_url: String,
//
//     #[env(var = "PORT", default = "8080")]
//     port: u16,
// }
// ```
//
// Priority (lowest to highest):
// 1. Defaults from #[env(default = "...")]
// 2. config.toml
// 3. config.local.toml (optional)
// 4. .env file
// 5. Environment variables
//
// This matches how most real apps work!

// ============================================================================
// THE TRICK: Use serde_json::Value as universal intermediate
// ============================================================================

/// Parse any supported format into a universal Value
fn parse_file(path: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let content = fs::read_to_string(path)?;

    // Detect format from extension
    if path.ends_with(".toml") {
        // toml -> Value
        let toml_value: toml::Value = toml::from_str(&content)?;
        Ok(toml_to_json(toml_value))
    } else if path.ends_with(".json") {
        // json -> Value (already native!)
        Ok(serde_json::from_str(&content)?)
    } else if path.ends_with(".yaml") || path.ends_with(".yml") {
        // yaml -> Value
        // Would need: serde_yaml = "0.9"
        // let yaml_value: serde_yaml::Value = serde_yaml::from_str(&content)?;
        // Ok(yaml_to_json(yaml_value))
        todo!("yaml support")
    } else {
        Err("Unknown file format".into())
    }
}

/// Convert TOML Value to JSON Value (they're almost identical)
fn toml_to_json(toml: toml::Value) -> Value {
    match toml {
        toml::Value::String(s) => Value::String(s),
        toml::Value::Integer(i) => Value::Number(i.into()),
        toml::Value::Float(f) => Value::Number(serde_json::Number::from_f64(f).unwrap_or(0.into())),
        toml::Value::Boolean(b) => Value::Bool(b),
        toml::Value::Datetime(dt) => Value::String(dt.to_string()),
        toml::Value::Array(arr) => Value::Array(arr.into_iter().map(toml_to_json).collect()),
        toml::Value::Table(table) => {
            let map: serde_json::Map<String, Value> = table
                .into_iter()
                .map(|(k, v)| (k, toml_to_json(v)))
                .collect();
            Value::Object(map)
        }
    }
}

// ============================================================================
// LAYERING: Deep merge of Value objects
// ============================================================================

/// Deep merge: overlay takes priority
fn deep_merge(base: &mut Value, overlay: Value) {
    match (base, overlay) {
        (Value::Object(base_map), Value::Object(overlay_map)) => {
            for (key, overlay_value) in overlay_map {
                if let Some(base_value) = base_map.get_mut(&key) {
                    deep_merge(base_value, overlay_value);
                } else {
                    base_map.insert(key, overlay_value);
                }
            }
        }
        (base, overlay) => {
            *base = overlay;
        }
    }
}

/// Coerce string values to appropriate JSON types
fn coerce_value(s: &str) -> Value {
    // Try bool
    if s.eq_ignore_ascii_case("true") {
        return Value::Bool(true);
    }
    if s.eq_ignore_ascii_case("false") {
        return Value::Bool(false);
    }

    // Try integer
    if let Ok(i) = s.parse::<i64>() {
        return Value::Number(i.into());
    }

    // Try float
    if let Ok(f) = s.parse::<f64>() {
        if let Some(n) = serde_json::Number::from_f64(f) {
            return Value::Number(n);
        }
    }

    // Keep as string
    Value::String(s.to_string())
}

/// Convert env vars to nested Value using separator
fn env_to_value(prefix: &str, separator: &str) -> Value {
    let mut root = serde_json::Map::new();

    for (key, value) in std::env::vars() {
        if let Some(stripped) = key.strip_prefix(prefix) {
            // APP_DATABASE_URL -> database.url (lowercase + split)
            let lowered = stripped.to_lowercase();
            let parts: Vec<&str> = lowered.split(separator).collect();

            // Type coercion: try to parse as bool/int/float, else keep as string
            let typed_value = coerce_value(&value);
            insert_nested(&mut root, &parts, typed_value);
        }
    }

    Value::Object(root)
}

fn insert_nested(map: &mut serde_json::Map<String, Value>, parts: &[&str], value: Value) {
    if parts.is_empty() {
        return;
    }

    if parts.len() == 1 {
        map.insert(parts[0].to_string(), value);
    } else {
        let entry = map
            .entry(parts[0].to_string())
            .or_insert_with(|| Value::Object(serde_json::Map::new()));

        if let Value::Object(nested) = entry {
            insert_nested(nested, &parts[1..], value);
        }
    }
}

// ============================================================================
// PUTTING IT TOGETHER
// ============================================================================

#[derive(Debug, Deserialize)]
struct DatabaseConfig {
    host: String,
    port: u16,
    name: String,
}

#[derive(Debug, Deserialize)]
struct Config {
    debug: bool,
    database: DatabaseConfig,
}

fn load_config() -> Result<Config, Box<dyn std::error::Error>> {
    // Start with defaults (could be from a Default impl or generated)
    let mut merged = serde_json::json!({
        "debug": false,
        "database": {
            "host": "localhost",
            "port": 5432,
            "name": "myapp"
        }
    });

    // Layer 1: Load config.toml if exists
    if let Ok(file_config) = parse_file("crates/procenv/examples/config.toml") {
        println!("Loaded config.toml");
        deep_merge(&mut merged, file_config);
    }

    // Layer 2: Load config.local.toml if exists (gitignored overrides)
    if let Ok(local_config) = parse_file("crates/procenv/examples/config.local.toml") {
        println!("Loaded config.local.toml");
        deep_merge(&mut merged, local_config);
    }

    // Layer 3: Environment variables override everything
    let env_config = env_to_value("APP_", "_");
    if let Value::Object(map) = &env_config {
        if !map.is_empty() {
            println!("Loaded {} env var overrides", map.len());
            deep_merge(&mut merged, env_config);
        }
    }

    // Final: Deserialize merged Value into Config struct
    let config: Config = serde_json::from_value(merged)?;
    Ok(config)
}

// ============================================================================
// DEMO
// ============================================================================

fn main() {
    println!("=== File Config Architecture Sketch ===\n");

    // Set some env vars to demonstrate override
    unsafe {
        std::env::set_var("APP_DEBUG", "true");
        std::env::set_var("APP_DATABASE_HOST", "prod-db.example.com");
    }

    // This would work if config.toml existed - showing the flow
    println!("Loading config with layering...\n");

    match load_config() {
        Ok(config) => {
            println!("\nFinal config: {:#?}", config);
        }
        Err(e) => {
            println!("Error: {}", e);
        }
    }

    // Cleanup
    unsafe {
        std::env::remove_var("APP_DEBUG");
        std::env::remove_var("APP_DATABASE_HOST");
    }

    println!("\n=== Key Insight ===");
    println!("All we need to add:");
    println!("  - toml = \"0.8\"        (already in workspace!)");
    println!("  - serde_yaml = \"0.9\"  (optional feature)");
    println!("  - ~100 lines of merge logic");
    println!();
    println!("The derive macro generates the layering code.");
    println!("serde does ALL the deserialization work.");
}
