//! Test that profile attributes are parsed and work correctly.

use procenv::EnvConfig;

#[derive(EnvConfig)]
#[env_config(
    profile_env = "APP_ENV",
    profiles = ["dev", "staging", "prod"]
)]
struct ProfileConfig {
    /// The database URL
    #[env(var = "DATABASE_URL")]
    #[profile(
        dev = "postgres://localhost/myapp_dev",
        staging = "postgres://staging-db/myapp",
        prod = "postgres://prod-db/myapp"
    )]
    database_url: String,

    /// Log level
    #[env(var = "LOG_LEVEL", default = "info")]
    #[profile(dev = "debug", staging = "info")]
    log_level: String,

    /// Port (no profile - same across all environments)
    #[env(var = "PORT", default = "8080")]
    port: u16,
}

fn main() {
    // Test 1: Profile defaults work when no env vars set
    // SAFETY: This is a test environment with no concurrent access
    unsafe {
        std::env::set_var("APP_ENV", "dev");
    }

    let config = ProfileConfig::from_env().unwrap();
    assert_eq!(config.database_url, "postgres://localhost/myapp_dev");
    assert_eq!(config.log_level, "debug");
    assert_eq!(config.port, 8080);

    // Test 2: Env vars override profile defaults
    unsafe {
        std::env::set_var("APP_ENV", "dev");
        std::env::set_var("DATABASE_URL", "postgres://custom/db");
    }

    let config = ProfileConfig::from_env().unwrap();
    assert_eq!(config.database_url, "postgres://custom/db");

    // Clean up
    unsafe {
        std::env::remove_var("APP_ENV");
        std::env::remove_var("DATABASE_URL");
    }

    // Test 3: Production profile
    unsafe {
        std::env::set_var("APP_ENV", "prod");
    }

    let config = ProfileConfig::from_env().unwrap();
    assert_eq!(config.database_url, "postgres://prod-db/myapp");
    // log_level uses default "info" because prod has no profile override
    assert_eq!(config.log_level, "info");

    // Clean up
    unsafe {
        std::env::remove_var("APP_ENV");
    }
}
