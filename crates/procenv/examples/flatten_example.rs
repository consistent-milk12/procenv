//! Example: Nested configs with flatten
//!
//! Run with:
//!   DATABASE_URL="postgres://localhost/db" DATABASE_PORT="5432" \
//!   REDIS_URL="redis://localhost" \
//!   cargo run --example flatten_example

use procenv::EnvConfig;

/// Database configuration - a standalone config struct
#[derive(EnvConfig)]
struct DatabaseConfig {
    #[env(var = "DATABASE_URL")]
    url: String,

    #[env(var = "DATABASE_PORT", default = "5432")]
    port: u16,
}

/// Redis configuration - another standalone config struct
#[derive(EnvConfig)]
struct RedisConfig {
    #[env(var = "REDIS_URL")]
    url: String,

    #[env(var = "REDIS_POOL_SIZE", default = "10")]
    pool_size: u32,
}

/// Main application config that composes other configs
#[derive(EnvConfig)]
struct AppConfig {
    /// Flattens DatabaseConfig fields into this struct
    #[env(flatten)]
    database: DatabaseConfig,

    /// Flattens RedisConfig fields into this struct
    #[env(flatten)]
    redis: RedisConfig,

    /// A regular field alongside flattened configs
    #[env(var = "APP_NAME", default = "my-app")]
    name: String,
}

fn main() {
    match AppConfig::from_env() {
        Ok(config) => {
            println!("Config loaded!");
            println!("  name: {}", config.name);
            println!("  database.url: {}", config.database.url);
            println!("  database.port: {}", config.database.port);
            println!("  redis.url: {}", config.redis.url);
            println!("  redis.pool_size: {}", config.redis.pool_size);
        }
        Err(e) => {
            eprintln!("{:?}", miette::Report::from(e));
            std::process::exit(1);
        }
    }
}
