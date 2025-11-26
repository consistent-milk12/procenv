//! Example demonstrating source attribution with dotenv files.
//!
//! Run from the examples directory:
//! ```bash
//! cd crates/procenv/examples && cargo run --example source_attribution_dotenv
//! ```

use procenv::EnvConfig;

#[derive(EnvConfig)]
#[env_config(prefix = "APP_", dotenv = ".env.source_test")]
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
        // Clear all env vars first
        std::env::remove_var("APP_DATABASE_URL");
        std::env::remove_var("APP_PORT");
        std::env::remove_var("APP_DEBUG");
        std::env::remove_var("APP_API_KEY");

        // Set DEBUG via real environment (should show as Environment, not Dotenv)
        std::env::set_var("APP_DEBUG", "true");
    }

    // Change to examples directory where .env.source_test is located
    let examples_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    std::env::set_current_dir(&examples_dir).expect("Failed to change to examples dir");

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

            println!("Expected sources:");
            println!("  database_url: .env file (set in .env.source_test)");
            println!("  port:         Default value (not in env or .env)");
            println!("  debug:        Environment variable (set before dotenv load)");
            println!("  api_key:      .env file (set in .env.source_test)");
        }
        Err(e) => {
            eprintln!("Failed to load config: {}", e);
            std::process::exit(1);
        }
    }
}
