//! Example demonstrating source attribution feature.
//!
//! Run with:
//! ```bash
//! # Set some env vars directly
//! APP_DATABASE_URL="postgres://localhost/mydb" cargo run --example source_attribution
//! ```

use procenv::EnvConfig;

#[derive(EnvConfig)]
#[env_config(prefix = "APP_", dotenv)]
struct Config {
    /// Database connection URL
    #[env(var = "DATABASE_URL")]
    database_url: String,

    /// Server port
    #[env(var = "PORT", default = "8080")]
    port: u16,

    /// Debug mode
    #[env(var = "DEBUG", default = "false")]
    debug: bool,

    /// Optional API key
    #[env(var = "API_KEY", optional)]
    api_key: Option<String>,
}

fn main() {
    // SAFETY: Single-threaded example, no concurrent env access
    unsafe {
        // Clear any existing env vars to start fresh
        std::env::remove_var("APP_DATABASE_URL");
        std::env::remove_var("APP_PORT");
        std::env::remove_var("APP_DEBUG");
        std::env::remove_var("APP_API_KEY");

        // Set only DATABASE_URL via environment (simulating shell export)
        std::env::set_var("APP_DATABASE_URL", "postgres://localhost/prod_db");

        // Set DEBUG to override the default
        std::env::set_var("APP_DEBUG", "true");
    }

    // Load config with source tracking
    match Config::from_env_with_sources() {
        Ok((config, sources)) => {
            println!("Configuration loaded successfully!\n");

            println!("Values:");
            println!("  database_url: {}", config.database_url);
            println!("  port: {}", config.port);
            println!("  debug: {}", config.debug);
            println!("  api_key: {:?}", config.api_key);

            println!("\n{}", sources);
        }
        Err(e) => {
            eprintln!("Failed to load config: {}", e);
            std::process::exit(1);
        }
    }

    // Also test the sources() convenience method
    println!("\n--- Using sources() method ---");
    match Config::sources() {
        Ok(sources) => {
            println!("{}", sources);
        }
        Err(e) => {
            eprintln!("Failed: {}", e);
        }
    }
}
