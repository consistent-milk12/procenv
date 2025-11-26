//! Test: Source attribution with nested (flattened) configs compiles

use procenv::EnvConfig;

#[derive(EnvConfig)]
struct DatabaseConfig {
    #[env(var = "DB_HOST", default = "localhost")]
    host: String,

    #[env(var = "DB_PORT", default = "5432")]
    port: u16,
}

#[derive(EnvConfig)]
#[env_config(prefix = "APP_")]
struct AppConfig {
    #[env(var = "NAME")]
    name: String,

    #[env(flatten)]
    database: DatabaseConfig,
}

fn main() {
    // Test that nested configs work with source attribution
    let _result: Result<(AppConfig, procenv::ConfigSources), procenv::Error> =
        AppConfig::from_env_with_sources();
}
