//! Example demonstrating dotenvy integration with procenv.
//!
//! This example shows three ways to use the `#[env_config(dotenv)]` attribute:
//! 1. Default: `#[env_config(dotenv)]` - loads .env from current directory
//! 2. Custom path: `#[env_config(dotenv = ".env.local")]`
//! 3. Multiple files: `#[env_config(dotenv = [".env", ".env.local"])]`
//!
//! Run with:
//! ```sh
//! # Create a .env file first
//! echo 'DATABASE_URL=postgres://localhost/mydb' > .env
//! echo 'APP_PORT=3000' >> .env
//!
//! cargo run --example dotenv_example
//! ```

use procenv::EnvConfig;

/// Config that loads from default .env file
#[derive(EnvConfig)]
#[env_config(dotenv)]
struct Config {
    #[env(var = "DATABASE_URL")]
    db_url: String,

    #[env(var = "APP_PORT", default = "8080")]
    port: u16,
}

fn main() -> Result<(), procenv::Error> {
    // The .env file is automatically loaded by from_env()
    // No need to call dotenvy::dotenv() manually!
    let config = Config::from_env()?;

    println!("=== Config loaded from .env ===\n");
    println!("Database URL: {}", config.db_url);
    println!("Port: {}", config.port);

    Ok(())
}
