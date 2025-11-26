//! Example: Generate .env.example file from config struct
//!
//! Run with:
//!   cargo run --example env_example_gen
//!
//! This will print the generated .env.example content to stdout.
//! To save to a file:
//!   cargo run --example env_example_gen > .env.example

use procenv::EnvConfig;

/// Database configuration
#[derive(EnvConfig)]
struct DatabaseConfig {
    /// Database connection URL
    #[env(var = "DATABASE_URL")]
    url: String,

    /// Database connection pool size
    #[env(var = "DATABASE_POOL_SIZE", default = "10")]
    pool_size: u32,

    /// Database connection timeout in seconds
    #[env(var = "DATABASE_TIMEOUT", default = "30")]
    timeout: u32,
}

/// Application configuration
#[derive(EnvConfig)]
struct AppConfig {
    /// Application name displayed in logs
    #[env(var = "APP_NAME", default = "my-app")]
    name: String,

    /// Server port to listen on
    #[env(var = "PORT", default = "8080")]
    port: u16,

    /// Enable debug mode
    #[env(var = "DEBUG", default = "false")]
    debug: bool,

    /// API key for external services
    #[env(var = "API_KEY", secret)]
    api_key: String,

    /// Optional webhook URL for notifications
    #[env(var = "WEBHOOK_URL", optional)]
    webhook_url: Option<String>,

    /// Database settings
    #[env(flatten)]
    database: DatabaseConfig,
}

fn main() {
    // Generate and print the .env.example content
    let example = AppConfig::env_example();
    println!("{}", example);

    // You could also write it to a file:
    // std::fs::write(".env.example", example).expect("Failed to write .env.example");
}
