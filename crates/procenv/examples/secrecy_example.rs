//! Example demonstrating secrecy integration with procenv.
//!
//! Run with:
//! ```sh
//! DATABASE_URL="postgres://localhost/mydb" \
//! API_KEY="super-secret-key" \
//! SECRET_PORT="8443" \
//! cargo run --example secrecy_example
//! ```

use procenv::{EnvConfig, ExposeSecret, SecretBox, SecretString};

#[derive(EnvConfig)]
struct Config {
    /// Regular string field (not secret)
    #[env(var = "DATABASE_URL")]
    db_url: String,

    /// Secret string using SecretString (auto-detected)
    #[env(var = "API_KEY")]
    api_key: SecretString,

    /// Secret numeric value using SecretBox<T>
    #[env(var = "SECRET_PORT")]
    secret_port: SecretBox<u16>,
}

fn main() -> Result<(), procenv::Error> {
    let config = Config::from_env()?;

    println!("=== Config loaded successfully ===\n");

    // Debug output - secrets are automatically redacted
    println!("Debug output (secrets redacted):");
    println!("{:?}\n", config);

    // Accessing regular field - just use it directly
    println!("Database URL: {}", config.db_url);

    // Accessing secrets - must explicitly call expose_secret()
    println!("API Key (exposed): {}", config.api_key.expose_secret());
    println!(
        "Secret Port (exposed): {}",
        config.secret_port.expose_secret()
    );

    Ok(())
}
