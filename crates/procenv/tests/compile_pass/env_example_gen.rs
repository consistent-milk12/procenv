//! Test: .env.example generation compiles correctly

use procenv::EnvConfig;

#[derive(EnvConfig)]
#[env_config(prefix = "APP_")]
struct Config {
    /// Database connection URL
    #[env(var = "DATABASE_URL")]
    db_url: String,

    /// Server port number
    #[env(var = "PORT", default = "8080")]
    port: u16,

    /// Optional API key
    #[env(var = "API_KEY", optional)]
    api_key: Option<String>,

    /// Secret token (will be marked as secret in output)
    #[env(var = "SECRET", secret)]
    secret: String,
}

fn main() {
    // Test that env_example() returns a String
    let example: String = Config::env_example();
    assert!(!example.is_empty());

    // Test that env_example_entries() also works
    let entries: String = Config::env_example_entries();
    assert!(!entries.is_empty());
}
