//! Error message quality tests.
//!
//! Ensures error messages are actionable and helpful.

use procenv::{EnvConfig, Error};
use serial_test::serial;

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

fn cleanup_vars(vars: &[&str]) {
    unsafe {
        for k in vars {
            std::env::remove_var(*k);
        }
    }
}

// ============================================================================
// Missing Error Message Quality
// ============================================================================

#[derive(EnvConfig)]
struct MissingErrorConfig {
    #[env(var = "ERR_DATABASE_URL")]
    database_url: String,
}

#[test]
#[serial]
fn test_missing_error_contains_var_name() {
    cleanup_vars(&["ERR_DATABASE_URL"]);

    let result = MissingErrorConfig::from_env();
    let err = result.unwrap_err();
    let display = format!("{}", err);

    assert!(
        display.contains("ERR_DATABASE_URL"),
        "Missing error should contain var name: {display}"
    );
}

#[test]
#[serial]
fn test_missing_error_has_help_text() {
    cleanup_vars(&["ERR_DATABASE_URL"]);

    let result = MissingErrorConfig::from_env();
    let err = result.unwrap_err();
    let display = format!("{}", err);

    // Should have actionable help
    assert!(
        display.contains("set") || display.contains("environment") || display.contains("help"),
        "Missing error should have actionable help: {display}"
    );
}

// ============================================================================
// Parse Error Message Quality
// ============================================================================

#[derive(EnvConfig)]
struct ParseErrorConfig {
    #[env(var = "ERR_PORT")]
    port: u16,
}

#[test]
#[serial]
fn test_parse_error_contains_var_name() {
    with_env(&[("ERR_PORT", "not_a_number")], || {
        let result = ParseErrorConfig::from_env();
        let err = result.unwrap_err();
        let display = format!("{}", err);

        assert!(
            display.contains("ERR_PORT"),
            "Parse error should contain var name: {display}"
        );
    })
}

#[test]
#[serial]
fn test_parse_error_shows_actual_value() {
    with_env(&[("ERR_PORT", "not_a_number")], || {
        let result = ParseErrorConfig::from_env();
        let err = result.unwrap_err();
        let display = format!("{}", err);

        assert!(
            display.contains("not_a_number"),
            "Parse error should show actual value: {display}"
        );
    })
}

#[test]
#[serial]
fn test_parse_error_shows_expected_type() {
    with_env(&[("ERR_PORT", "not_a_number")], || {
        let result = ParseErrorConfig::from_env();
        let err = result.unwrap_err();
        let display = format!("{}", err);

        assert!(
            display.contains("u16") || display.contains("integer") || display.contains("number"),
            "Parse error should indicate expected type: {display}"
        );
    })
}

// ============================================================================
// Secret Redaction in Errors
// ============================================================================

#[derive(EnvConfig)]
#[allow(dead_code)]
struct SecretErrorConfig {
    #[env(var = "ERR_SECRET", secret)]
    api_key: String,
}

#[test]
#[serial]
fn test_debug_redacts_secret_value() {
    with_env(&[("ERR_SECRET", "my-super-secret-key")], || {
        let config = SecretErrorConfig::from_env().expect("string should parse");
        // Verify debug output redacts secret
        let debug = format!("{:?}", config);

        assert!(
            !debug.contains("my-super-secret-key"),
            "Debug should not contain secret value: {debug}"
        );
        assert!(
            debug.contains("***") || debug.contains("[redacted]") || debug.contains("REDACTED"),
            "Debug should show redaction marker: {debug}"
        );
    })
}

#[derive(EnvConfig)]
#[allow(dead_code)]
struct SecretParseErrorConfig {
    #[env(var = "ERR_SECRET_NUM", secret)]
    secret_num: u32,
}

#[test]
#[serial]
fn test_secret_parse_error_redacts_value() {
    with_env(&[("ERR_SECRET_NUM", "secret_value_123")], || {
        let result = SecretParseErrorConfig::from_env();
        let err = result.unwrap_err();
        let display = format!("{}", err);

        // Should NOT leak the actual secret value
        assert!(
            !display.contains("secret_value_123"),
            "Parse error should NOT contain secret value: {display}"
        );

        // Should show that it's redacted (procenv uses <redacted>)
        assert!(
            display.contains("<redacted>")
                || display.contains("***")
                || display.contains("[redacted]")
                || display.contains("REDACTED"),
            "Parse error should show redaction for secret: {display}"
        );
    })
}

// ============================================================================
// Multiple Error Formatting
// ============================================================================

#[derive(EnvConfig)]
struct MultipleErrorConfig {
    #[env(var = "MULTI_A")]
    a: String,

    #[env(var = "MULTI_B")]
    b: String,

    #[env(var = "MULTI_C")]
    c: u32,
}

#[test]
#[serial]
fn test_multiple_errors_shows_count() {
    cleanup_vars(&["MULTI_A", "MULTI_B", "MULTI_C"]);

    let result = MultipleErrorConfig::from_env();
    let err = result.unwrap_err();
    let display = format!("{}", err);

    assert!(
        display.contains("3") || display.contains("multiple") || display.contains("errors"),
        "Multiple error should indicate count: {display}"
    );
}

#[test]
#[serial]
fn test_multiple_errors_is_multiple_variant() {
    cleanup_vars(&["MULTI_A", "MULTI_B", "MULTI_C"]);

    let result = MultipleErrorConfig::from_env();
    let err = result.unwrap_err();

    // Verify it's a Multiple error with 3 errors
    match err {
        Error::Multiple { errors } => {
            assert_eq!(errors.len(), 3, "Should have 3 errors");
        }
        other => panic!("Expected Multiple error, got: {:?}", other),
    }
}

// ============================================================================
// Error Type Variants
// ============================================================================

#[test]
#[serial]
fn test_error_is_missing_variant() {
    cleanup_vars(&["ERR_DATABASE_URL"]);

    let result = MissingErrorConfig::from_env();
    let err = result.unwrap_err();

    assert!(
        matches!(err, Error::Missing { .. }),
        "Should be Missing variant"
    );
}

#[test]
#[serial]
fn test_error_is_parse_variant() {
    with_env(&[("ERR_PORT", "invalid")], || {
        let result = ParseErrorConfig::from_env();
        let err = result.unwrap_err();

        assert!(
            matches!(err, Error::Parse { .. }),
            "Should be Parse variant"
        );
    })
}

#[test]
#[serial]
fn test_error_is_multiple_variant() {
    cleanup_vars(&["MULTI_A", "MULTI_B", "MULTI_C"]);

    let result = MultipleErrorConfig::from_env();
    let err = result.unwrap_err();

    assert!(
        matches!(err, Error::Multiple { .. }),
        "Should be Multiple variant"
    );
}

// ============================================================================
// miette Diagnostic Codes
// ============================================================================

use miette::Diagnostic;

#[test]
#[serial]
fn test_missing_error_has_diagnostic_code() {
    cleanup_vars(&["ERR_DATABASE_URL"]);

    let result = MissingErrorConfig::from_env();
    let err = result.unwrap_err();

    let code = err.code();
    assert!(code.is_some(), "Missing error should have diagnostic code");

    let code_str = format!("{}", code.unwrap());
    assert!(
        code_str.contains("procenv"),
        "Diagnostic code should contain 'procenv': {code_str}"
    );
}

#[test]
#[serial]
fn test_parse_error_has_diagnostic_code() {
    with_env(&[("ERR_PORT", "invalid")], || {
        let result = ParseErrorConfig::from_env();
        let err = result.unwrap_err();

        let code = err.code();
        assert!(code.is_some(), "Parse error should have diagnostic code");
    })
}

#[test]
#[serial]
fn test_missing_error_has_help() {
    cleanup_vars(&["ERR_DATABASE_URL"]);

    let result = MissingErrorConfig::from_env();
    let err = result.unwrap_err();

    let help = err.help();
    assert!(help.is_some(), "Missing error should have help text");

    let help_str = format!("{}", help.unwrap());
    assert!(
        !help_str.is_empty(),
        "Help text should not be empty: {help_str}"
    );
}

// ============================================================================
// Error Display Stability
// ============================================================================

#[test]
#[serial]
fn test_error_display_is_stable() {
    cleanup_vars(&["ERR_DATABASE_URL"]);

    let result1 = MissingErrorConfig::from_env();
    let err1 = result1.unwrap_err();
    let display1 = format!("{}", err1);

    let result2 = MissingErrorConfig::from_env();
    let err2 = result2.unwrap_err();
    let display2 = format!("{}", err2);

    assert_eq!(
        display1, display2,
        "Error display should be stable across invocations"
    );
}
