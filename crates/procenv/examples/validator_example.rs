//! Example: EnvConfig with validator integration
//!
//! This demonstrates the `#[env_config(validate)]` attribute which automatically
//! runs validator crate validation after loading environment variables.
//!
//! Run with valid config:
//!   DATABASE_URL=postgres://localhost/mydb \
//!   ADMIN_EMAIL=admin@example.com \
//!   PORT=8080 \
//!   MAX_WORKERS=4 \
//!   cargo run --package procenv --example validator_example --features validator
//!
//! Run with invalid email (validation error):
//!   DATABASE_URL=postgres://localhost/mydb \
//!   ADMIN_EMAIL=not-an-email \
//!   cargo run --package procenv --example validator_example --features validator
//!
//! Run with port out of range (validation error):
//!   DATABASE_URL=postgres://localhost/mydb \
//!   ADMIN_EMAIL=admin@example.com \
//!   PORT=99999 \
//!   cargo run --package procenv --example validator_example --features validator

use procenv::EnvConfig;
use validator::Validate;

/// Custom validator: ensure username is not "root"
fn validate_not_root(username: &str) -> Result<(), validator::ValidationError> {
    if username.eq_ignore_ascii_case("root") {
        let mut err = validator::ValidationError::new("forbidden_username");
        err.message = Some("username cannot be 'root'".into());
        return Err(err);
    }
    Ok(())
}

/// Server configuration with both env loading and validation
///
/// The `#[env_config(validate)]` attribute enables the `from_env_validated()`
/// method which combines loading and validation in a single step.
#[derive(EnvConfig, Validate)]
#[env_config(validate)]
struct ServerConfig {
    /// Database connection URL (must be valid URL format)
    #[env(var = "DATABASE_URL")]
    #[validate(url)]
    database_url: String,

    /// Server port (must be in valid port range)
    #[env(var = "PORT", default = "8080")]
    #[validate(range(min = 1, max = 65535))]
    port: u16,

    /// Admin email address (must be valid email format)
    #[env(var = "ADMIN_EMAIL")]
    #[validate(email)]
    admin_email: String,

    /// Number of worker threads (must be between 1 and 128)
    #[env(var = "MAX_WORKERS", default = "4")]
    #[validate(range(min = 1, max = 128))]
    max_workers: u32,

    /// Application username (custom validation: cannot be "root")
    #[env(var = "APP_USER", default = "app")]
    #[validate(length(min = 1), custom(function = "validate_not_root"))]
    app_user: String,

    /// Optional API key (no validation needed)
    #[env(var = "API_KEY", optional)]
    api_key: Option<String>,
}

fn main() {
    println!("Loading and validating configuration...\n");

    // Use from_env_validated() to load AND validate in one step
    // This handles:
    // - Missing required variables (procenv::Error::Missing)
    // - Type parsing errors (procenv::Error::Parse)
    // - Validation failures (procenv::Error::Validation)
    let config = match ServerConfig::from_env_validated() {
        Ok(cfg) => {
            println!("Configuration loaded and validated successfully!\n");
            cfg
        }
        Err(e) => {
            eprintln!("Configuration error:");
            eprintln!("{:?}", miette::Report::from(e));
            std::process::exit(1);
        }
    };

    // Use the validated configuration
    println!("Configuration summary:");
    println!("  DATABASE_URL = {}", config.database_url);
    println!("  PORT         = {}", config.port);
    println!("  ADMIN_EMAIL  = {}", config.admin_email);
    println!("  MAX_WORKERS  = {}", config.max_workers);
    println!("  APP_USER     = {}", config.app_user);
    println!("  API_KEY      = {:?}", config.api_key);
    println!("\nServer ready to start!");
}
