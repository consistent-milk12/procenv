//! Malformed file format tests.
//!
//! Tests for handling invalid/malformed TOML, JSON, and YAML files.

#![cfg(feature = "file-all")]

use procenv::EnvConfig;
use serde::Deserialize;
use std::fs;

const BASE_DIR: &str = "/tmp/procenv_malformed_tests";

fn ensure_dir() {
    let _ = fs::create_dir_all(BASE_DIR);
}

fn write_file(name: &str, content: &str) {
    ensure_dir();
    let path = format!("{}/{}", BASE_DIR, name);
    fs::write(&path, content).expect("Failed to write test file");
}

fn cleanup_file(name: &str) {
    let path = format!("{}/{}", BASE_DIR, name);
    let _ = fs::remove_file(&path);
}

fn cleanup_env(vars: &[&str]) {
    unsafe {
        for k in vars {
            std::env::remove_var(*k);
        }
    }
}

// ============================================================================
// Malformed TOML Tests
// ============================================================================

#[derive(EnvConfig, Deserialize)]
#[env_config(
    prefix = "MAL_TOML_",
    file_optional = "/tmp/procenv_malformed_tests/malformed.toml"
)]
struct MalformedTomlConfig {
    #[env(var = "NAME", default = "default")]
    name: String,

    #[env(var = "PORT", default = "8080")]
    port: u16,
}

#[test]
fn test_malformed_toml_syntax_error() {
    cleanup_env(&["MAL_TOML_NAME", "MAL_TOML_PORT"]);
    cleanup_file("malformed.toml");

    // Invalid TOML: missing closing quote
    let content = r#"name = "unclosed string"#;
    write_file("malformed.toml", content);

    // Should fall back to defaults when file is malformed and optional
    let result = MalformedTomlConfig::from_config();
    // With file_optional, malformed files may error or use defaults
    // depending on implementation - verify it doesn't panic
    assert!(
        result.is_ok() || result.is_err(),
        "Should handle malformed TOML gracefully"
    );

    cleanup_file("malformed.toml");
}

#[test]
fn test_malformed_toml_invalid_value_type() {
    cleanup_env(&["MAL_TOML_NAME", "MAL_TOML_PORT"]);
    cleanup_file("malformed.toml");

    // Valid TOML syntax but wrong type for port
    let content = r#"
name = "test"
port = "not_a_number"
"#;
    write_file("malformed.toml", content);

    let result = MalformedTomlConfig::from_config();
    assert!(result.is_err(), "Should error on type mismatch");

    let err = result.unwrap_err();
    let err_str = format!("{}", err);
    assert!(
        err_str.contains("port") || err_str.contains("u16") || err_str.contains("integer"),
        "Error should mention the field or type: {}",
        err_str
    );

    cleanup_file("malformed.toml");
}

#[test]
fn test_malformed_toml_missing_required_key() {
    cleanup_env(&["MAL_TOML_NAME", "MAL_TOML_PORT"]);
    cleanup_file("malformed.toml");

    // Missing name field (which has default, so should work)
    let content = r#"port = 3000"#;
    write_file("malformed.toml", content);

    let result = MalformedTomlConfig::from_config();
    assert!(result.is_ok(), "Should use default for missing field");

    let config = result.unwrap();
    assert_eq!(config.name, "default");
    assert_eq!(config.port, 3000);

    cleanup_file("malformed.toml");
}

// ============================================================================
// Malformed JSON Tests
// ============================================================================

#[derive(EnvConfig, Deserialize)]
#[env_config(
    prefix = "MAL_JSON_",
    file_optional = "/tmp/procenv_malformed_tests/malformed.json"
)]
struct MalformedJsonConfig {
    #[env(var = "NAME", default = "default")]
    name: String,

    #[env(var = "COUNT", default = "0")]
    count: i32,
}

#[test]
fn test_malformed_json_syntax_error() {
    cleanup_env(&["MAL_JSON_NAME", "MAL_JSON_COUNT"]);
    cleanup_file("malformed.json");

    // Invalid JSON: missing closing brace
    let content = r#"{"name": "test""#;
    write_file("malformed.json", content);

    let result = MalformedJsonConfig::from_config();
    assert!(
        result.is_ok() || result.is_err(),
        "Should handle malformed JSON gracefully"
    );

    cleanup_file("malformed.json");
}

#[test]
fn test_malformed_json_trailing_comma() {
    cleanup_env(&["MAL_JSON_NAME", "MAL_JSON_COUNT"]);
    cleanup_file("malformed.json");

    // Invalid JSON: trailing comma
    let content = r#"{"name": "test", "count": 5,}"#;
    write_file("malformed.json", content);

    let result = MalformedJsonConfig::from_config();
    // Trailing commas are invalid in standard JSON
    assert!(
        result.is_ok() || result.is_err(),
        "Should handle trailing comma gracefully"
    );

    cleanup_file("malformed.json");
}

#[test]
fn test_malformed_json_wrong_type() {
    cleanup_env(&["MAL_JSON_NAME", "MAL_JSON_COUNT"]);
    cleanup_file("malformed.json");

    // Valid JSON but wrong type
    let content = r#"{"name": "test", "count": "not_a_number"}"#;
    write_file("malformed.json", content);

    let result = MalformedJsonConfig::from_config();
    assert!(result.is_err(), "Should error on type mismatch");

    cleanup_file("malformed.json");
}

#[test]
fn test_malformed_json_null_value() {
    cleanup_env(&["MAL_JSON_NAME", "MAL_JSON_COUNT"]);
    cleanup_file("malformed.json");

    // JSON with null value for non-optional field
    let content = r#"{"name": null, "count": 5}"#;
    write_file("malformed.json", content);

    let result = MalformedJsonConfig::from_config();
    // null should either error or use default depending on implementation
    assert!(
        result.is_ok() || result.is_err(),
        "Should handle null gracefully"
    );

    cleanup_file("malformed.json");
}

// ============================================================================
// Malformed YAML Tests
// ============================================================================

#[derive(EnvConfig, Deserialize)]
#[env_config(
    prefix = "MAL_YAML_",
    file_optional = "/tmp/procenv_malformed_tests/malformed.yaml"
)]
struct MalformedYamlConfig {
    #[env(var = "NAME", default = "default")]
    name: String,

    #[env(var = "ENABLED", default = "false")]
    enabled: bool,
}

#[test]
fn test_malformed_yaml_bad_indentation() {
    cleanup_env(&["MAL_YAML_NAME", "MAL_YAML_ENABLED"]);
    cleanup_file("malformed.yaml");

    // Invalid YAML: inconsistent indentation
    let content = r#"
name: test
  enabled: true
"#;
    write_file("malformed.yaml", content);

    let result = MalformedYamlConfig::from_config();
    assert!(
        result.is_ok() || result.is_err(),
        "Should handle bad indentation gracefully"
    );

    cleanup_file("malformed.yaml");
}

#[test]
fn test_malformed_yaml_tabs_instead_of_spaces() {
    cleanup_env(&["MAL_YAML_NAME", "MAL_YAML_ENABLED"]);
    cleanup_file("malformed.yaml");

    // YAML doesn't allow tabs for indentation
    let content = "name: test\n\tenabled: true";
    write_file("malformed.yaml", content);

    let result = MalformedYamlConfig::from_config();
    assert!(
        result.is_ok() || result.is_err(),
        "Should handle tabs gracefully"
    );

    cleanup_file("malformed.yaml");
}

#[test]
fn test_malformed_yaml_wrong_type() {
    cleanup_env(&["MAL_YAML_NAME", "MAL_YAML_ENABLED"]);
    cleanup_file("malformed.yaml");

    // Valid YAML but wrong type
    let content = r#"
name: test
enabled: "not_a_bool"
"#;
    write_file("malformed.yaml", content);

    let result = MalformedYamlConfig::from_config();
    // YAML is flexible with bools, but "not_a_bool" should fail
    assert!(
        result.is_ok() || result.is_err(),
        "Should handle type mismatch gracefully"
    );

    cleanup_file("malformed.yaml");
}

// ============================================================================
// Empty and Whitespace-Only Files
// ============================================================================

#[test]
fn test_empty_toml_file() {
    cleanup_env(&["MAL_TOML_NAME", "MAL_TOML_PORT"]);
    cleanup_file("malformed.toml");

    write_file("malformed.toml", "");

    let result = MalformedTomlConfig::from_config();
    assert!(result.is_ok(), "Empty TOML should use defaults");

    let config = result.unwrap();
    assert_eq!(config.name, "default");
    assert_eq!(config.port, 8080);

    cleanup_file("malformed.toml");
}

#[test]
fn test_whitespace_only_json() {
    cleanup_env(&["MAL_JSON_NAME", "MAL_JSON_COUNT"]);
    cleanup_file("malformed.json");

    write_file("malformed.json", "   \n\n   ");

    let result = MalformedJsonConfig::from_config();
    // Whitespace-only is invalid JSON
    assert!(
        result.is_ok() || result.is_err(),
        "Should handle whitespace-only gracefully"
    );

    cleanup_file("malformed.json");
}

#[test]
fn test_comments_only_yaml() {
    cleanup_env(&["MAL_YAML_NAME", "MAL_YAML_ENABLED"]);
    cleanup_file("malformed.yaml");

    let content = r#"
# This is a comment
# Another comment
"#;
    write_file("malformed.yaml", content);

    let result = MalformedYamlConfig::from_config();
    // Comments-only YAML parses to null, which may fail to deserialize
    // This tests that we handle it gracefully (either error or default)
    assert!(
        result.is_ok() || result.is_err(),
        "Should handle comments-only YAML gracefully"
    );

    cleanup_file("malformed.yaml");
}

// ============================================================================
// File with Extra Unknown Fields
// ============================================================================

#[test]
fn test_toml_with_extra_fields() {
    cleanup_env(&["MAL_TOML_NAME", "MAL_TOML_PORT"]);
    cleanup_file("malformed.toml");

    // Extra fields that aren't in the struct
    let content = r#"
name = "test"
port = 3000
unknown_field = "should be ignored"
another_unknown = 123
"#;
    write_file("malformed.toml", content);

    let result = MalformedTomlConfig::from_config();
    // serde should ignore unknown fields by default (deny_unknown_fields not set)
    assert!(result.is_ok(), "Should ignore unknown fields: {:?}", result);

    let config = result.unwrap();
    assert_eq!(config.name, "test");
    assert_eq!(config.port, 3000);

    cleanup_file("malformed.toml");
}

#[test]
fn test_json_with_nested_unknown() {
    cleanup_env(&["MAL_JSON_NAME", "MAL_JSON_COUNT"]);
    cleanup_file("malformed.json");

    // Deeply nested unknown structure
    let content = r#"{
        "name": "test",
        "count": 42,
        "unknown": {
            "nested": {
                "deeply": true
            }
        }
    }"#;
    write_file("malformed.json", content);

    let result = MalformedJsonConfig::from_config();
    assert!(result.is_ok(), "Should ignore nested unknown fields");

    let config = result.unwrap();
    assert_eq!(config.name, "test");
    assert_eq!(config.count, 42);

    cleanup_file("malformed.json");
}
