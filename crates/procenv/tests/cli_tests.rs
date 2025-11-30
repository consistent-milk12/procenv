//! CLI integration tests.
//!
//! Tests for `from_args()` method and CLI argument handling.
//! Note: These tests verify CLI integration works correctly.

#![cfg(feature = "clap")]

use procenv::EnvConfig;
use serial_test::serial;

fn cleanup_env(vars: &[&str]) {
    unsafe {
        for k in vars {
            std::env::remove_var(*k);
        }
    }
}

fn with_env<F, R>(vars: &[(&str, &str)], f: F) -> R
where
    F: FnOnce() -> R,
{
    unsafe {
        for (k, v) in vars {
            std::env::set_var(*k, *v);
        }
    }

    let result = f();

    unsafe {
        for (k, _) in vars {
            std::env::remove_var(*k);
        }
    }

    result
}

// ============================================================================
// CLI Attribute Compilation Tests
// ============================================================================

#[derive(EnvConfig)]
struct CliBasicConfig {
    #[env(var = "CLI_HOST", default = "localhost", arg = "host", short = 'h')]
    host: String,

    #[env(var = "CLI_PORT", default = "8080", arg = "port", short = 'p')]
    port: u16,

    #[env(var = "CLI_DEBUG", optional, arg = "debug", short = 'd')]
    debug: Option<bool>,
}

#[test]
#[serial]
fn test_cli_config_compiles_and_from_env_works() {
    cleanup_env(&["CLI_HOST", "CLI_PORT", "CLI_DEBUG"]);

    // Verify from_env still works with CLI attributes
    let config = CliBasicConfig::from_env().expect("should load from env");
    assert_eq!(config.host, "localhost");
    assert_eq!(config.port, 8080);
    assert!(config.debug.is_none());
}

#[test]
#[serial]
fn test_cli_config_env_values_work() {
    cleanup_env(&["CLI_HOST", "CLI_PORT", "CLI_DEBUG"]);

    with_env(
        &[
            ("CLI_HOST", "example.com"),
            ("CLI_PORT", "3000"),
            ("CLI_DEBUG", "true"),
        ],
        || {
            let config = CliBasicConfig::from_env().expect("should load from env");
            assert_eq!(config.host, "example.com");
            assert_eq!(config.port, 3000);
            assert_eq!(config.debug, Some(true));
        },
    )
}

// ============================================================================
// CLI with Different Field Types
// ============================================================================

#[derive(EnvConfig)]
struct CliTypesConfig {
    #[env(var = "CLIT_STRING", default = "default", arg = "string")]
    string_val: String,

    #[env(var = "CLIT_INT", default = "42", arg = "int")]
    int_val: i32,

    #[env(var = "CLIT_FLOAT", default = "3.14", arg = "float")]
    float_val: f64,

    #[env(var = "CLIT_BOOL", default = "false", arg = "bool")]
    bool_val: bool,
}

#[test]
#[serial]
fn test_cli_various_types_from_env() {
    cleanup_env(&["CLIT_STRING", "CLIT_INT", "CLIT_FLOAT", "CLIT_BOOL"]);

    with_env(
        &[
            ("CLIT_STRING", "hello"),
            ("CLIT_INT", "-100"),
            ("CLIT_FLOAT", "2.718"),
            ("CLIT_BOOL", "true"),
        ],
        || {
            let config = CliTypesConfig::from_env().expect("should parse all types");
            assert_eq!(config.string_val, "hello");
            assert_eq!(config.int_val, -100);
            assert!((config.float_val - 2.718).abs() < 0.001);
            assert!(config.bool_val);
        },
    )
}

// ============================================================================
// CLI with Required Fields
// ============================================================================

#[derive(EnvConfig)]
struct CliRequiredConfig {
    #[env(var = "CLIR_REQUIRED", arg = "required")]
    required_field: String,

    #[env(var = "CLIR_OPTIONAL", default = "default", arg = "optional")]
    optional_field: String,
}

#[test]
#[serial]
fn test_cli_required_field_from_env() {
    cleanup_env(&["CLIR_REQUIRED", "CLIR_OPTIONAL"]);

    with_env(&[("CLIR_REQUIRED", "provided")], || {
        let config = CliRequiredConfig::from_env().expect("should load with required");
        assert_eq!(config.required_field, "provided");
        assert_eq!(config.optional_field, "default");
    })
}

#[test]
#[serial]
fn test_cli_missing_required_fails() {
    cleanup_env(&["CLIR_REQUIRED", "CLIR_OPTIONAL"]);

    let result = CliRequiredConfig::from_env();
    assert!(result.is_err(), "should fail when required field missing");
}

// ============================================================================
// CLI Mixed with Non-CLI Fields
// ============================================================================

#[derive(EnvConfig)]
struct CliMixedConfig {
    #[env(var = "CLIM_CLI_FIELD", default = "cli", arg = "cli-field")]
    cli_field: String,

    #[env(var = "CLIM_ENV_ONLY", default = "env")]
    env_only_field: String,
}

#[test]
#[serial]
fn test_cli_mixed_fields_from_env() {
    cleanup_env(&["CLIM_CLI_FIELD", "CLIM_ENV_ONLY"]);

    with_env(
        &[
            ("CLIM_CLI_FIELD", "from-env"),
            ("CLIM_ENV_ONLY", "also-env"),
        ],
        || {
            let config = CliMixedConfig::from_env().expect("should load mixed config");
            assert_eq!(config.cli_field, "from-env");
            assert_eq!(config.env_only_field, "also-env");
        },
    )
}

// ============================================================================
// CLI with Prefix
// ============================================================================

#[derive(EnvConfig)]
#[env_config(prefix = "CLIP_")]
struct CliPrefixConfig {
    #[env(var = "HOST", default = "localhost", arg = "host")]
    host: String,

    #[env(var = "PORT", default = "8080", arg = "port")]
    port: u16,
}

#[test]
#[serial]
fn test_cli_with_prefix_from_env() {
    cleanup_env(&["CLIP_HOST", "CLIP_PORT"]);

    with_env(
        &[("CLIP_HOST", "prefixed.com"), ("CLIP_PORT", "9000")],
        || {
            let config = CliPrefixConfig::from_env().expect("should load prefixed config");
            assert_eq!(config.host, "prefixed.com");
            assert_eq!(config.port, 9000);
        },
    )
}

// ============================================================================
// CLI Secret Fields (should still be marked as secret)
// ============================================================================

#[derive(EnvConfig)]
struct CliSecretConfig {
    #[env(var = "CLIS_TOKEN", arg = "token", secret)]
    api_token: String,

    #[env(var = "CLIS_PUBLIC", default = "public", arg = "public")]
    public_field: String,
}

#[test]
#[serial]
fn test_cli_secret_field_loads() {
    cleanup_env(&["CLIS_TOKEN", "CLIS_PUBLIC"]);

    with_env(&[("CLIS_TOKEN", "super-secret-value")], || {
        let config = CliSecretConfig::from_env().expect("should load secret");
        assert_eq!(config.api_token, "super-secret-value");

        // Debug should redact secret
        let debug_str = format!("{:?}", config);
        assert!(
            !debug_str.contains("super-secret-value"),
            "Debug should not contain secret"
        );
    })
}
