//! Validation integration tests for Phase B.
//!
//! Tests for the `#[env_config(validate)]` attribute which enables
//! `from_env_validated()` method generation.

#![cfg(feature = "validator")]

use procenv::EnvConfig;
use validator::Validate;

/// Helper to set env vars for tests
fn with_env<F: FnOnce()>(vars: &[(&str, &str)], f: F) {
    unsafe {
        for (k, v) in vars {
            std::env::set_var(*k, *v);
        }

        f();

        for (k, _) in vars {
            std::env::remove_var(k);
        }
    }
}

// ============================================================================
// Basic Validation Tests
// ============================================================================

#[derive(EnvConfig, Validate)]
#[env_config(validate)]
struct BasicValidatedConfig {
    #[env(var = "VAL_PORT", default = "8080")]
    #[validate(range(min = 1, max = 65535))]
    port: u16,

    #[env(var = "VAL_EMAIL")]
    #[validate(email)]
    email: String,
}

#[test]
fn test_valid_config_passes_validation() {
    with_env(&[("VAL_EMAIL", "test@example.com")], || {
        let result = BasicValidatedConfig::from_env_validated();
        assert!(result.is_ok());
        let config = result.unwrap();
        assert_eq!(config.port, 8080);
        assert_eq!(config.email, "test@example.com");
    });
}

#[test]
fn test_invalid_port_fails_validation() {
    with_env(
        &[("VAL_PORT", "0"), ("VAL_EMAIL", "test@example.com")],
        || {
            let result = BasicValidatedConfig::from_env_validated();
            assert!(result.is_err());

            let err = result.unwrap_err();
            let display = format!("{}", err);
            assert!(display.contains("validation"));
        },
    );
}

#[test]
fn test_invalid_email_fails_validation() {
    with_env(&[("VAL_EMAIL", "not-an-email")], || {
        let result = BasicValidatedConfig::from_env_validated();
        assert!(result.is_err());
    });
}

#[test]
fn test_multiple_validation_errors() {
    with_env(&[("VAL_PORT", "0"), ("VAL_EMAIL", "invalid")], || {
        let result = BasicValidatedConfig::from_env_validated();
        assert!(result.is_err());

        if let procenv::Error::Validation { errors } = result.unwrap_err() {
            // Should have 2 validation errors: port range and email
            assert!(errors.len() >= 2);
        } else {
            panic!("Expected Validation error");
        }
    });
}

// ============================================================================
// Custom Validation Function Tests
// ============================================================================

fn validate_username(username: &str) -> Result<(), validator::ValidationError> {
    if username.len() < 3 {
        let mut err = validator::ValidationError::new("username_too_short");
        err.message = Some("Username must be at least 3 characters".into());
        return Err(err);
    }
    if username.chars().any(|c| !c.is_alphanumeric() && c != '_') {
        let mut err = validator::ValidationError::new("invalid_characters");
        err.message =
            Some("Username can only contain alphanumeric characters and underscores".into());
        return Err(err);
    }
    Ok(())
}

#[derive(EnvConfig, Validate)]
#[env_config(validate)]
struct CustomValidatedConfig {
    #[env(var = "CUSTOM_USERNAME", validate = "validate_username")]
    username: String,
}

#[test]
fn test_custom_validation_passes() {
    with_env(&[("CUSTOM_USERNAME", "valid_user")], || {
        let result = CustomValidatedConfig::from_env_validated();
        assert!(result.is_ok());
    });
}

#[test]
fn test_custom_validation_fails_too_short() {
    with_env(&[("CUSTOM_USERNAME", "ab")], || {
        let result = CustomValidatedConfig::from_env_validated();
        assert!(result.is_err());
    });
}

// ============================================================================
// URL Validation Tests
// ============================================================================

#[derive(EnvConfig, Validate)]
#[env_config(validate)]
struct UrlConfig {
    #[env(var = "VAL_WEBHOOK_URL")]
    #[validate(url)]
    webhook: String,
}

#[test]
fn test_url_validation_passes() {
    with_env(
        &[("VAL_WEBHOOK_URL", "https://example.com/webhook")],
        || {
            let result = UrlConfig::from_env_validated();
            assert!(result.is_ok());
        },
    );
}

#[test]
fn test_url_validation_fails() {
    with_env(&[("VAL_WEBHOOK_URL", "not-a-url")], || {
        let result = UrlConfig::from_env_validated();
        assert!(result.is_err());
    });
}

// ============================================================================
// Length Validation Tests
// ============================================================================

#[derive(EnvConfig, Validate)]
#[env_config(validate)]
struct LengthConfig {
    #[env(var = "VAL_API_KEY")]
    #[validate(length(min = 32, max = 64))]
    api_key: String,
}

#[test]
fn test_length_validation_passes() {
    let key = "a".repeat(32);
    with_env(&[("VAL_API_KEY", &key)], || {
        let result = LengthConfig::from_env_validated();
        assert!(result.is_ok());
    });
}

#[test]
fn test_length_validation_fails_too_short() {
    with_env(&[("VAL_API_KEY", "short")], || {
        let result = LengthConfig::from_env_validated();
        assert!(result.is_err());
    });
}

// ============================================================================
// Loading Error Before Validation Tests
// ============================================================================

#[derive(EnvConfig, Validate)]
#[env_config(validate)]
struct LoadThenValidateConfig {
    #[env(var = "LTV_REQUIRED")]
    #[validate(length(min = 1))]
    required: String,
}

#[test]
fn test_loading_error_before_validation() {
    // Don't set LTV_REQUIRED - should get Missing error, not validation error
    unsafe {
        std::env::remove_var("LTV_REQUIRED");
    }

    let result = LoadThenValidateConfig::from_env_validated();
    assert!(result.is_err());

    // Should be a Missing error, not Validation
    match result.unwrap_err() {
        procenv::Error::Missing { .. } => (), // Expected
        other => panic!("Expected Missing error, got: {:?}", other),
    }
}

// ============================================================================
// Validated With Sources Tests
// ============================================================================

#[test]
fn test_validated_with_sources() {
    with_env(&[("VAL_EMAIL", "test@example.com")], || {
        let result = BasicValidatedConfig::from_env_validated_with_sources();
        assert!(result.is_ok());

        let (config, sources) = result.unwrap();
        assert_eq!(config.email, "test@example.com");

        // Should have source attribution
        let email_source = sources.get("email");
        assert!(email_source.is_some());
    });
}

// ============================================================================
// Non-Validated Config (should NOT have from_env_validated)
// ============================================================================

// This config does NOT have #[env_config(validate)], so it should NOT
// have from_env_validated() method. This is tested at compile time by
// ensuring the code compiles without issues.
#[derive(EnvConfig)]
struct NonValidatedConfig {
    #[env(var = "NV_PORT", default = "8080")]
    port: u16,
}

#[test]
fn test_non_validated_config_works() {
    let result = NonValidatedConfig::from_env();
    assert!(result.is_ok());
    assert_eq!(result.unwrap().port, 8080);
}
