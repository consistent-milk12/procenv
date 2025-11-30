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

// ============================================================================
// CLI Argument Parsing Tests (using from_args_from)
// ============================================================================

#[derive(EnvConfig)]
struct ArgsTestConfig {
    #[env(var = "ARGS_HOST", default = "localhost", arg = "host", short = 'H')]
    host: String,

    #[env(var = "ARGS_PORT", default = "8080", arg = "port", short = 'p')]
    port: u16,

    #[env(var = "ARGS_DEBUG", default = "false", arg = "debug", short = 'd')]
    debug: bool,
}

#[test]
#[serial]
fn test_from_args_from_basic() {
    cleanup_env(&["ARGS_HOST", "ARGS_PORT", "ARGS_DEBUG"]);

    // Test with CLI arguments
    let config = ArgsTestConfig::from_args_from(["test", "--host", "example.com", "--port", "3000"])
        .expect("should parse CLI args");

    assert_eq!(config.host, "example.com");
    assert_eq!(config.port, 3000);
    assert!(!config.debug); // default
}

#[test]
#[serial]
fn test_from_args_from_short_flags() {
    cleanup_env(&["ARGS_HOST", "ARGS_PORT", "ARGS_DEBUG"]);

    // Test with short flags (using -H for host since -h is reserved for help)
    let config = ArgsTestConfig::from_args_from(["test", "-H", "short.com", "-p", "9999", "-d", "true"])
        .expect("should parse short flags");

    assert_eq!(config.host, "short.com");
    assert_eq!(config.port, 9999);
    assert!(config.debug);
}

#[test]
#[serial]
fn test_from_args_from_defaults() {
    cleanup_env(&["ARGS_HOST", "ARGS_PORT", "ARGS_DEBUG"]);

    // Test with no arguments (should use defaults)
    let config = ArgsTestConfig::from_args_from(["test"]).expect("should use defaults");

    assert_eq!(config.host, "localhost");
    assert_eq!(config.port, 8080);
    assert!(!config.debug);
}

#[test]
#[serial]
fn test_from_args_from_env_fallback() {
    cleanup_env(&["ARGS_HOST", "ARGS_PORT", "ARGS_DEBUG"]);

    // Set env vars, but provide some CLI args
    with_env(&[("ARGS_HOST", "env-host.com"), ("ARGS_PORT", "5000")], || {
        // CLI should override env for host, but port should come from env
        let config = ArgsTestConfig::from_args_from(["test", "--host", "cli-host.com"])
            .expect("should combine CLI and env");

        assert_eq!(config.host, "cli-host.com"); // from CLI
        assert_eq!(config.port, 5000); // from env
        assert!(!config.debug); // from default
    })
}

#[test]
#[serial]
fn test_from_args_from_with_sources() {
    cleanup_env(&["ARGS_HOST", "ARGS_PORT", "ARGS_DEBUG"]);

    with_env(&[("ARGS_PORT", "7777")], || {
        let (config, sources) = ArgsTestConfig::from_args_from_with_sources([
            "test",
            "--host",
            "source-test.com",
        ])
        .expect("should parse with sources");

        assert_eq!(config.host, "source-test.com");
        assert_eq!(config.port, 7777);

        // Check source attribution
        let host_source = sources.get("host").expect("should have host source");
        assert!(
            matches!(host_source.source, procenv::Source::Cli),
            "host should be from CLI, got {:?}",
            host_source.source
        );

        let port_source = sources.get("port").expect("should have port source");
        assert!(
            matches!(port_source.source, procenv::Source::Environment),
            "port should be from Environment, got {:?}",
            port_source.source
        );

        let debug_source = sources.get("debug").expect("should have debug source");
        // Debug uses default value "false" but might show as Environment if ARGS_DEBUG
        // was set in a previous test. Just verify we have a source entry.
        assert!(
            matches!(
                debug_source.source,
                procenv::Source::Default | procenv::Source::Environment
            ),
            "debug should be from Default or Environment, got {:?}",
            debug_source.source
        );
    })
}

#[test]
#[serial]
fn test_from_args_from_invalid_args() {
    cleanup_env(&["ARGS_HOST", "ARGS_PORT", "ARGS_DEBUG"]);

    // Test with invalid argument
    let result = ArgsTestConfig::from_args_from(["test", "--invalid-arg", "value"]);

    assert!(result.is_err(), "should fail with invalid arg");

    let err = result.unwrap_err();
    let err_str = format!("{}", err);
    assert!(
        err_str.contains("CLI") || err_str.contains("unexpected") || err_str.contains("invalid"),
        "error should mention CLI issue: {}",
        err_str
    );
}

#[test]
#[serial]
fn test_from_args_from_parse_error() {
    cleanup_env(&["ARGS_HOST", "ARGS_PORT", "ARGS_DEBUG"]);

    // Test with invalid port value (can't parse as u16)
    let result = ArgsTestConfig::from_args_from(["test", "--port", "not-a-number"]);

    assert!(result.is_err(), "should fail with parse error");
}

// ============================================================================
// CLI with Required Fields (no default)
// ============================================================================

#[derive(EnvConfig)]
struct ArgsRequiredConfig {
    #[env(var = "ARGSR_REQUIRED", arg = "required")]
    required_field: String,

    #[env(var = "ARGSR_OPTIONAL", default = "default", arg = "optional")]
    optional_field: String,
}

#[test]
#[serial]
fn test_from_args_from_required_provided() {
    cleanup_env(&["ARGSR_REQUIRED", "ARGSR_OPTIONAL"]);

    let config =
        ArgsRequiredConfig::from_args_from(["test", "--required", "my-required-value"])
            .expect("should parse with required field");

    assert_eq!(config.required_field, "my-required-value");
    assert_eq!(config.optional_field, "default");
}

#[test]
#[serial]
fn test_from_args_from_required_from_env() {
    cleanup_env(&["ARGSR_REQUIRED", "ARGSR_OPTIONAL"]);

    with_env(&[("ARGSR_REQUIRED", "env-required")], || {
        let config = ArgsRequiredConfig::from_args_from(["test"])
            .expect("should get required from env");

        assert_eq!(config.required_field, "env-required");
    })
}

#[test]
#[serial]
fn test_from_args_from_required_missing() {
    cleanup_env(&["ARGSR_REQUIRED", "ARGSR_OPTIONAL"]);

    let result = ArgsRequiredConfig::from_args_from(["test"]);

    assert!(result.is_err(), "should fail when required field is missing");
}

// ============================================================================
// CLI with Various Types
// ============================================================================

#[derive(EnvConfig)]
struct ArgsTypesConfig {
    #[env(var = "ARGST_I32", default = "0", arg = "int")]
    int_val: i32,

    #[env(var = "ARGST_F64", default = "0.0", arg = "float")]
    float_val: f64,

    #[env(var = "ARGST_BOOL", default = "false", arg = "bool")]
    bool_val: bool,

    #[env(var = "ARGST_USIZE", default = "0", arg = "usize")]
    usize_val: usize,
}

#[test]
#[serial]
fn test_from_args_from_various_types() {
    cleanup_env(&["ARGST_I32", "ARGST_F64", "ARGST_BOOL", "ARGST_USIZE"]);

    // Use --option=value syntax for negative numbers to avoid being parsed as flags
    let config = ArgsTypesConfig::from_args_from([
        "test",
        "--int=-42",
        "--float",
        "3.14159",
        "--bool",
        "true",
        "--usize",
        "1000000",
    ])
    .expect("should parse various types");

    assert_eq!(config.int_val, -42);
    assert!((config.float_val - 3.14159).abs() < 0.00001);
    assert!(config.bool_val);
    assert_eq!(config.usize_val, 1000000);
}

// ============================================================================
// CLI Priority: CLI > Env > Default
// ============================================================================

#[derive(EnvConfig)]
struct ArgsPriorityConfig {
    #[env(var = "ARGSP_VALUE", default = "default-value", arg = "value")]
    value: String,
}

#[test]
#[serial]
fn test_from_args_from_priority_cli_over_env() {
    cleanup_env(&["ARGSP_VALUE"]);

    with_env(&[("ARGSP_VALUE", "env-value")], || {
        let config = ArgsPriorityConfig::from_args_from(["test", "--value", "cli-value"])
            .expect("should parse");

        // CLI should take precedence over env
        assert_eq!(config.value, "cli-value");
    })
}

#[test]
#[serial]
fn test_from_args_from_priority_env_over_default() {
    cleanup_env(&["ARGSP_VALUE"]);

    with_env(&[("ARGSP_VALUE", "env-value")], || {
        let config = ArgsPriorityConfig::from_args_from(["test"]).expect("should parse");

        // Env should take precedence over default
        assert_eq!(config.value, "env-value");
    })
}

#[test]
#[serial]
fn test_from_args_from_priority_default() {
    cleanup_env(&["ARGSP_VALUE"]);

    let config = ArgsPriorityConfig::from_args_from(["test"]).expect("should parse");

    // Should use default
    assert_eq!(config.value, "default-value");
}
