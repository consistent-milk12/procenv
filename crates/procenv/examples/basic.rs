//! Example: EnvConfig with error accumulation
//!
//! Run with missing vars to see all errors at once:
//!   cargo run --package procenv --example basic
//!
//! Run with required vars set:
//!   DATABASE_URL=postgres://localhost SECRET=xyz cargo run --package procenv  --example basic

use procenv::EnvConfig;

#[derive(EnvConfig)]
struct Config {
    #[env(var = "DATABASE_URL")]
    db_url: String,

    #[env(var = "PORT", default = "8080")]
    port: u16,

    #[env(var = "DEBUG", default = "false")]
    debug: bool,

    #[env(var = "API_KEY", optional)]
    api_key: Option<String>,

    #[env(var = "MAX_CONNECTIONS", optional)]
    max_connections: Option<u32>,

    #[env(var = "SECRET")]
    secret: String,
}

fn main() {
    match Config::from_env() {
        Ok(config) => {
            println!("Successfully loaded config!");
            println!("  DATABASE_URL    = {}", config.db_url);
            println!("  PORT            = {} (default: 8080)", config.port);
            println!("  DEBUG           = {} (default: false)", config.debug);
            println!("  API_KEY         = {:?} (optional)", config.api_key);
            println!(
                "  MAX_CONNECTIONS = {:?} (optional)",
                config.max_connections
            );
            println!("  SECRET          = {}", config.secret);
        }

        Err(e) => {
            // Use miette's Report for fancy error rendering
            eprintln!("{:?}", miette::Report::from(e));
            std::process::exit(1);
        }
    }
}
