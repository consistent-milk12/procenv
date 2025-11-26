//! Test that serde format attribute works for structured env data.

use procenv::EnvConfig;
use serde::Deserialize;

#[derive(Debug, Deserialize, PartialEq)]
struct DatabaseConfig {
    host: String,
    port: u16,
}

#[derive(EnvConfig)]
struct Config {
    /// Allowed hosts as JSON array
    #[env(var = "ALLOWED_HOSTS", format = "json")]
    allowed_hosts: Vec<String>,

    /// Database config as JSON object
    #[env(var = "DB_CONFIG", format = "json")]
    db_config: DatabaseConfig,
}

fn main() {
    // SAFETY: This is a test environment with no concurrent access
    unsafe {
        std::env::set_var("ALLOWED_HOSTS", r#"["localhost", "example.com"]"#);
        std::env::set_var("DB_CONFIG", r#"{"host": "localhost", "port": 5432}"#);
    }

    let config = Config::from_env().unwrap();

    assert_eq!(config.allowed_hosts, vec!["localhost", "example.com"]);
    assert_eq!(config.db_config.host, "localhost");
    assert_eq!(config.db_config.port, 5432);

    // Clean up
    unsafe {
        std::env::remove_var("ALLOWED_HOSTS");
        std::env::remove_var("DB_CONFIG");
    }
}
