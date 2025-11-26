//! Figment Comparison Example
//!
//! This demonstrates what makes figment "mature":
//! 1. Layered providers with clear priority
//! 2. Rich source tracking (where each value came from)
//! 3. Multiple file format support
//! 4. Profile-based configuration (dev/prod/test)
//! 5. Nested keys with dot notation
//! 6. Metadata and error context
//!
//! Run: cargo run --example comparison_figment

use figment::{
    Figment, Metadata, Profile, Provider,
    providers::{Env, Format, Json, Serialized, Toml},
    value::{Dict, Map},
};
use serde::{Deserialize, Serialize};

// ============================================================================
// 1. LAYERED CONFIGURATION WITH PRIORITY
// ============================================================================
//
// Figment lets you stack providers. Later providers override earlier ones.
// This is VERY powerful for real applications.

#[derive(Debug, Deserialize, Serialize)]
struct DatabaseConfig {
    host: String,
    port: u16,
    name: String,
    max_connections: u32,
}

#[derive(Debug, Deserialize, Serialize)]
struct ServerConfig {
    host: String,
    port: u16,
    workers: usize,
}

#[derive(Debug, Deserialize, Serialize)]
struct AppConfig {
    debug: bool,
    database: DatabaseConfig,
    server: ServerConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            debug: false,
            database: DatabaseConfig {
                host: "localhost".into(),
                port: 5432,
                name: "myapp".into(),
                max_connections: 10,
            },
            server: ServerConfig {
                host: "0.0.0.0".into(),
                port: 8080,
                workers: 4,
            },
        }
    }
}

fn demo_layered_config() {
    println!("=== 1. LAYERED CONFIGURATION ===\n");

    // Priority (lowest to highest):
    // 1. Defaults (compiled in)
    // 2. Config file (config.toml)
    // 3. Environment variables (APP_*)
    //
    // This is how real apps work - you have sensible defaults,
    // then config files for deployment, then env vars for secrets/overrides.

    let figment = Figment::new()
        // Start with compiled defaults
        .merge(Serialized::defaults(AppConfig::default()))
        // Then merge config file (if exists) - won't fail if missing
        .merge(Toml::file("config.toml").nested())
        // Finally, environment variables override everything
        // The underscore splits nested keys: APP_DATABASE_PORT -> database.port
        .merge(Env::prefixed("APP_").split("_"));

    // Even without files, this works with defaults + env
    match figment.extract::<AppConfig>() {
        Ok(config) => {
            println!("Loaded config: {:#?}", config);
        }
        Err(e) => {
            println!("Error (expected - no env vars set): {}", e);
        }
    }

    println!();
}

// ============================================================================
// 2. RICH SOURCE TRACKING (METADATA)
// ============================================================================
//
// Figment tracks WHERE each value came from. This is critical for debugging
// "why is my config wrong?" issues in production.

fn demo_source_tracking() {
    println!("=== 2. SOURCE TRACKING ===\n");

    // Set some env vars for demo
    // SAFETY: Single-threaded example
    unsafe {
        std::env::set_var("DEMO_DATABASE_HOST", "prod-db.example.com");
        std::env::set_var("DEMO_SERVER_PORT", "9000");
    }

    let figment = Figment::new()
        .merge(Serialized::defaults(AppConfig::default()))
        .merge(Env::prefixed("DEMO_").split("_"));

    // Extract with metadata - this is the killer feature
    let value = figment.find_value("database.host");
    match value {
        Ok(val) => {
            println!("database.host = {:?}", val);
            // The metadata tells you exactly where it came from
            let meta = figment.find_metadata("database.host");
            if let Some(m) = meta {
                println!("  Source: {:?}", m.source);
                println!("  Name: {}", m.name);
            }
        }
        Err(e) => println!("Error: {}", e),
    }

    let value = figment.find_value("server.port");
    match value {
        Ok(val) => {
            println!("\nserver.port = {:?}", val);
            let meta = figment.find_metadata("server.port");
            if let Some(m) = meta {
                println!("  Source: {:?}", m.source);
            }
        }
        Err(e) => println!("Error: {}", e),
    }

    // Value from defaults (not overridden)
    let value = figment.find_value("debug");
    match value {
        Ok(val) => {
            println!("\ndebug = {:?}", val);
            let meta = figment.find_metadata("debug");
            if let Some(m) = meta {
                println!("  Source: {:?}", m.source);
                println!("  (came from Serialized defaults)");
            }
        }
        Err(e) => println!("Error: {}", e),
    }

    // Cleanup
    unsafe {
        std::env::remove_var("DEMO_DATABASE_HOST");
        std::env::remove_var("DEMO_SERVER_PORT");
    }

    println!();
}

// ============================================================================
// 3. PROFILES (dev/staging/prod)
// ============================================================================
//
// This is a HUGE feature. You can have different configs per environment,
// all in one file, and select at runtime.

fn demo_profiles() {
    println!("=== 3. PROFILES ===\n");

    // Imagine this is in a config.toml:
    // [default]
    // debug = false
    //
    // [debug.database]
    // host = "localhost"
    //
    // [release.database]
    // host = "prod-db.internal"

    // Figment can select which profile to use
    let dev_figment = Figment::new()
        .merge(Serialized::defaults(AppConfig::default()))
        .select(Profile::new("development"));

    let prod_figment = Figment::new()
        .merge(Serialized::defaults(AppConfig::default()))
        .select(Profile::new("production"));

    println!("Development profile selected: {:?}", dev_figment.profile());
    println!("Production profile selected: {:?}", prod_figment.profile());

    // In real apps, you'd select profile from APP_PROFILE env var:
    // .select(Profile::from_env_or("APP_PROFILE", "development"))

    println!();
}

// ============================================================================
// 4. CUSTOM PROVIDERS
// ============================================================================
//
// You can implement Provider trait for ANY source - Vault, Consul, AWS SSM, etc.

struct VaultProvider {
    // In reality, this would be a Vault client
    secrets: Dict,
}

impl VaultProvider {
    fn new() -> Self {
        let mut secrets = Dict::new();
        secrets.insert(
            "api_key".into(),
            figment::value::Value::from("vault-secret-key-12345"),
        );
        secrets.insert(
            "database_password".into(),
            figment::value::Value::from("super-secret-password"),
        );
        Self { secrets }
    }
}

impl Provider for VaultProvider {
    fn metadata(&self) -> Metadata {
        // This metadata appears in error messages and source tracking
        Metadata::named("Vault secrets at vault.example.com")
    }

    fn data(&self) -> Result<Map<Profile, Dict>, figment::Error> {
        let mut map = Map::new();
        map.insert(Profile::Default, self.secrets.clone());
        Ok(map)
    }
}

fn demo_custom_provider() {
    println!("=== 4. CUSTOM PROVIDERS ===\n");

    #[derive(Debug, Deserialize)]
    struct SecretsConfig {
        api_key: String,
        database_password: String,
    }

    let figment = Figment::new()
        // Your custom provider plugs right in!
        .merge(VaultProvider::new());

    match figment.extract::<SecretsConfig>() {
        Ok(config) => {
            println!("Secrets loaded from custom Vault provider:");
            println!("  api_key: {}...", &config.api_key[..20]);
            println!("  database_password: <redacted>");

            // And source tracking still works!
            if let Some(meta) = figment.find_metadata("api_key") {
                println!("\n  api_key source: {}", meta.name);
            }
        }
        Err(e) => println!("Error: {}", e),
    }

    println!();
}

// ============================================================================
// 5. ERROR MESSAGES
// ============================================================================
//
// Figment's errors tell you exactly what went wrong and where.

fn demo_error_messages() {
    println!("=== 5. ERROR MESSAGES ===\n");

    #[derive(Debug, Deserialize)]
    struct StrictConfig {
        required_field: String,
        port: u16,
    }

    // Set an invalid port
    unsafe {
        std::env::set_var("ERR_PORT", "not-a-number");
    }

    let figment = Figment::new().merge(Env::prefixed("ERR_"));

    match figment.extract::<StrictConfig>() {
        Ok(_) => println!("Unexpectedly succeeded"),
        Err(e) => {
            println!("Figment error output:\n");
            println!("{}", e);
            // The error includes:
            // - Which field failed
            // - What the value was
            // - Where it came from (env var name)
            // - What type was expected
        }
    }

    unsafe {
        std::env::remove_var("ERR_PORT");
    }

    println!();
}

// ============================================================================
// MAIN
// ============================================================================

fn main() {
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║          FIGMENT - What Makes It 'Mature'                    ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    demo_layered_config();
    demo_source_tracking();
    demo_profiles();
    demo_custom_provider();
    demo_error_messages();

    println!("═══════════════════════════════════════════════════════════════");
    println!("KEY TAKEAWAYS:");
    println!("═══════════════════════════════════════════════════════════════");
    println!("1. Layered providers with clear priority");
    println!("2. Rich metadata - know WHERE each value came from");
    println!("3. Profiles for dev/staging/prod");
    println!("4. Extensible - custom providers for Vault, Consul, etc.");
    println!("5. Clear, contextual error messages");
    println!();
    println!("procenv has: error accumulation, .env.example gen, miette diagnostics");
    println!("procenv lacks: layered providers, profiles, custom providers");
}
