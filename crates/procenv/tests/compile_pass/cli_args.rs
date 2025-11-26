//! Test that CLI argument attributes are parsed correctly.

use procenv::EnvConfig;

#[derive(EnvConfig)]
struct CliConfig {
    /// The database URL
    #[env(var = "DATABASE_URL", arg = "database-url")]
    database_url: String,

    /// The port to listen on
    #[env(var = "PORT", default = "8080", arg = "port", short = 'p')]
    port: u16,

    /// Optional debug mode
    #[env(var = "DEBUG", optional, arg = "debug", short = 'd')]
    debug: Option<bool>,

    /// Regular env-only field (no CLI arg)
    #[env(var = "HOST", default = "localhost")]
    host: String,
}

fn main() {
    // This test verifies that the macro compiles with CLI attributes.
    // The from_args() method is generated but we don't call it here
    // because it would actually parse CLI args.

    // Instead, we verify from_env() still works
    // SAFETY: This is a test environment with no concurrent access
    unsafe {
        std::env::set_var("DATABASE_URL", "postgres://localhost/test");
    }

    let config = CliConfig::from_env().unwrap();
    assert_eq!(config.database_url, "postgres://localhost/test");
    assert_eq!(config.port, 8080);
    assert!(config.debug.is_none());
    assert_eq!(config.host, "localhost");
}
