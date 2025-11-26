//! Example: EnvConfig with validator integration
//!
//! This demonstrates the recommended dual-derive pattern for combining
//! procenv (environment variable loading) with validator (runtime validation).
//!
//! Run with valid config:
//!   DATABASE_URL=postgres://localhost/mydb \
//!   ADMIN_EMAIL=admin@example.com \
//!   PORT=8080 \
//!   MAX_WORKERS=4 \
//!   cargo run --package procenv --example validator_example
//!
//! Run with invalid email (validation error):
//!   DATABASE_URL=postgres://localhost/mydb \
//!   ADMIN_EMAIL=not-an-email \
//!   cargo run --package procenv --example validator_example
//!
//! Run with port out of range (validation error):
//!   DATABASE_URL=postgres://localhost/mydb \
//!   ADMIN_EMAIL=admin@example.com \
//!   PORT=99999 \
//!   cargo run --package procenv --example validator_example

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
/// Note: EnvConfig generates a custom Debug impl that masks secret fields
#[derive(EnvConfig, Validate)]
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
    // Step 1: Load configuration from environment variables
    // This handles: missing vars, type parsing errors
    println!("Step 1: Loading configuration from environment...");
    let config = match ServerConfig::from_env() {
        Ok(cfg) => {
            println!("  Configuration loaded successfully!\n");
            cfg
        }
        Err(e) => {
            eprintln!("Configuration loading failed:");
            eprintln!("{:?}", miette::Report::from(e));
            std::process::exit(1);
        }
    };

    // Step 2: Validate the loaded configuration
    // This handles: semantic validation (email format, URL format, ranges, custom rules)
    println!("Step 2: Validating configuration...");
    if let Err(validation_errors) = config.validate() {
        eprintln!("Configuration validation failed:\n");

        // Pretty-print validation errors
        for (field, errors) in validation_errors.field_errors() {
            eprintln!("  Field '{}' has {} error(s):", field, errors.len());
            for err in errors {
                let msg = err
                    .message
                    .as_ref()
                    .map(|m| m.to_string())
                    .unwrap_or_else(|| format!("validation '{}' failed", err.code));
                eprintln!("    - {}", msg);
            }
        }
        std::process::exit(1);
    }
    println!("  Validation passed!\n");

    // Step 3: Use the validated configuration
    println!("Configuration summary:");
    println!("  DATABASE_URL = {}", config.database_url);
    println!("  PORT         = {}", config.port);
    println!("  ADMIN_EMAIL  = {}", config.admin_email);
    println!("  MAX_WORKERS  = {}", config.max_workers);
    println!("  APP_USER     = {}", config.app_user);
    println!("  API_KEY      = {:?}", config.api_key);
    println!("\nServer ready to start!");
}
