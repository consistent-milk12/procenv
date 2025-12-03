//! Example: File configuration without serde dependency
//!
//! This demonstrates the "clap pattern" where users only need to derive
//! `EnvConfig` - no `Deserialize` derive required!
//!
//! Run with:
//!   `cargo run --example serde_free --features file,toml`

use procenv::EnvConfig;

// NO serde import needed!
// NO #[derive(Deserialize)] needed!

/// Database configuration - nested struct
#[derive(EnvConfig)]
struct DatabaseConfig {
    #[env(var = "DB_HOST", default = "localhost")]
    host: String,

    #[env(var = "DB_PORT", default = "5432")]
    port: u16,

    #[env(var = "DB_NAME", default = "myapp")]
    name: String,
}

/// Main application configuration
#[derive(EnvConfig)]
#[env_config(
    prefix = "APP_",
    file_optional = "crates/procenv/data/config.toml",
    dotenv
)]
struct AppConfig {
    /// Application name
    #[env(var = "NAME", default = "my-application")]
    name: String,

    /// Server port
    #[env(var = "PORT", default = "8080")]
    port: u16,

    /// Debug mode
    #[env(var = "DEBUG", default = "false")]
    debug: bool,

    /// Optional feature flag
    #[env(var = "FEATURE_X", optional)]
    feature_x: Option<bool>,

    /// Nested database configuration
    #[env(flatten)]
    database: DatabaseConfig,
}

fn main() -> Result<(), procenv::Error> {
    println!("=== Serde-Free Configuration Example ===\n");

    // Just EnvConfig - no serde required!
    // Priority: defaults < config.toml < .env < environment

    let config = AppConfig::from_config()?;

    println!("Loaded configuration:");
    println!("  Name:     {}", config.name);
    println!("  Port:     {}", config.port);
    println!("  Debug:    {}", config.debug);
    println!("  Feature:  {:?}", config.feature_x);
    println!();
    println!("  Database:");
    println!("    Host: {}", config.database.host);
    println!("    Port: {}", config.database.port);
    println!("    Name: {}", config.database.name);

    println!();
    println!("=== Key Point ===");
    println!("This config was loaded using ONLY #[derive(EnvConfig)]");
    println!("No serde dependency or Deserialize derive needed!");

    Ok(())
}
