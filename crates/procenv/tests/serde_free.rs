//! Tests that `from_config()` works without serde derive.
//!
//! These tests verify the self-contained deserialization feature where
//! the macro generates field-by-field extraction code, eliminating the
//! need for users to derive `Deserialize` on their config structs.
//!
//! This is the "clap pattern" - just `#[derive(EnvConfig)]` is enough.

#![allow(clippy::pedantic)]
#![cfg(feature = "file-all")]

use procenv::EnvConfig;
use std::fs;

const BASE_DIR: &str = "/tmp/procenv_serde_free_tests";

fn ensure_dir() {
    let _ = fs::create_dir_all(BASE_DIR);
}

fn write_file(name: &str, content: &str) -> String {
    ensure_dir();
    let path = format!("{BASE_DIR}/{name}");
    fs::write(&path, content).expect("Failed to write test file");
    path
}

fn cleanup_file(name: &str) {
    let path = format!("{BASE_DIR}/{name}");
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
// Basic Tests - No Serde Required!
// ============================================================================

/// Simple config with defaults - no file, no env, just defaults work.
#[test]
fn test_simple_config_from_defaults() {
    cleanup_env(&["SF_HOST", "SF_PORT"]);

    // NO Deserialize derive - just EnvConfig!
    // file_optional with non-existent file - tests defaults work
    #[derive(EnvConfig, PartialEq)]
    #[env_config(prefix = "SF_", file_optional = "/nonexistent/config.toml")]
    struct SimpleConfig {
        #[env(var = "HOST", default = "localhost")]
        host: String,

        #[env(var = "PORT", default = "8080")]
        port: u16,
    }

    let config = SimpleConfig::from_config().expect("should load from defaults");
    assert_eq!(config.host, "localhost");
    assert_eq!(config.port, 8080);
}

/// Config loaded from environment variables.
#[test]
fn test_config_from_env_vars() {
    cleanup_env(&["SFENV_HOST", "SFENV_PORT"]);

    #[derive(EnvConfig)]
    #[env_config(prefix = "SFENV_", file_optional = "/nonexistent/config.toml")]
    struct EnvVarConfig {
        #[env(var = "HOST", default = "localhost")]
        host: String,

        #[env(var = "PORT", default = "8080")]
        port: u16,
    }

    unsafe {
        std::env::set_var("SFENV_HOST", "example.com");
        std::env::set_var("SFENV_PORT", "3000");
    }

    let config = EnvVarConfig::from_config().expect("should load from env");
    assert_eq!(config.host, "example.com");
    assert_eq!(config.port, 3000);

    cleanup_env(&["SFENV_HOST", "SFENV_PORT"]);
}

// ============================================================================
// File Loading Tests - No Serde Required!
// ============================================================================

#[test]
fn test_config_from_toml_file() {
    cleanup_env(&["SFTOML_NAME", "SFTOML_PORT", "SFTOML_DEBUG"]);

    let content = r#"
name = "my-app"
port = 9000
debug = true
"#;
    let _path = write_file("serde_free_basic.toml", content);

    #[derive(EnvConfig)]
    #[env_config(prefix = "SFTOML_", file_optional = "/tmp/procenv_serde_free_tests/serde_free_basic.toml")]
    struct TomlConfig {
        #[env(var = "NAME", default = "default")]
        name: String,

        #[env(var = "PORT", default = "8080")]
        port: u16,

        #[env(var = "DEBUG", default = "false")]
        debug: bool,
    }

    let config = TomlConfig::from_config().expect("should load TOML without serde");

    assert_eq!(config.name, "my-app");
    assert_eq!(config.port, 9000);
    assert!(config.debug);

    cleanup_file("serde_free_basic.toml");
}

#[test]
fn test_config_from_json_file() {
    cleanup_env(&["SFJSON_HOST", "SFJSON_PORT"]);

    let content = r#"{"host": "json-host", "port": 4000}"#;
    write_file("serde_free_basic.json", content);

    #[derive(EnvConfig)]
    #[env_config(prefix = "SFJSON_", file_optional = "/tmp/procenv_serde_free_tests/serde_free_basic.json")]
    struct JsonConfig {
        #[env(var = "HOST", default = "localhost")]
        host: String,

        #[env(var = "PORT", default = "8080")]
        port: u16,
    }

    let config = JsonConfig::from_config().expect("should load JSON without serde");

    assert_eq!(config.host, "json-host");
    assert_eq!(config.port, 4000);

    cleanup_file("serde_free_basic.json");
}

#[test]
fn test_config_from_yaml_file() {
    cleanup_env(&["SFYAML_NAME", "SFYAML_PORT"]);

    let content = r"
name: yaml-app
port: 5000
";
    write_file("serde_free_basic.yaml", content);

    #[derive(EnvConfig)]
    #[env_config(prefix = "SFYAML_", file_optional = "/tmp/procenv_serde_free_tests/serde_free_basic.yaml")]
    struct YamlConfig {
        #[env(var = "NAME", default = "default")]
        name: String,

        #[env(var = "PORT", default = "8080")]
        port: u16,
    }

    let config = YamlConfig::from_config().expect("should load YAML without serde");

    assert_eq!(config.name, "yaml-app");
    assert_eq!(config.port, 5000);

    cleanup_file("serde_free_basic.yaml");
}

// ============================================================================
// Optional Fields
// ============================================================================

#[test]
fn test_optional_fields_missing() {
    cleanup_env(&["SFOPT_NAME", "SFOPT_PORT"]);

    #[derive(EnvConfig)]
    #[env_config(prefix = "SFOPT_", file_optional = "/nonexistent/config.toml")]
    struct OptionalConfig {
        #[env(var = "NAME", default = "app")]
        name: String,

        #[env(var = "PORT", optional)]
        port: Option<u16>,
    }

    let config = OptionalConfig::from_config().expect("should handle optional");

    assert_eq!(config.name, "app");
    assert_eq!(config.port, None);
}

#[test]
fn test_optional_fields_present() {
    cleanup_env(&["SFOPT2_NAME", "SFOPT2_PORT"]);

    let content = r#"{"name": "present-app", "port": 3000}"#;
    write_file("serde_free_optional.json", content);

    #[derive(EnvConfig)]
    #[env_config(prefix = "SFOPT2_", file_optional = "/tmp/procenv_serde_free_tests/serde_free_optional.json")]
    struct OptionalPresentConfig {
        #[env(var = "NAME")]
        name: String,

        #[env(var = "PORT", optional)]
        port: Option<u16>,
    }

    let config = OptionalPresentConfig::from_config().expect("should load optional present");

    assert_eq!(config.name, "present-app");
    assert_eq!(config.port, Some(3000));

    cleanup_file("serde_free_optional.json");
}

// ============================================================================
// Nested/Flatten Fields
// ============================================================================

#[test]
fn test_nested_config_with_flatten() {
    cleanup_env(&["SFNEST_APP_NAME", "SFNEST_DB_HOST", "SFNEST_DB_PORT"]);

    // Nested struct - also no Deserialize!
    #[derive(EnvConfig, PartialEq)]
    struct DatabaseConfig {
        #[env(var = "DB_HOST", default = "localhost")]
        host: String,

        #[env(var = "DB_PORT", default = "5432")]
        port: u16,
    }

    #[derive(EnvConfig)]
    #[env_config(prefix = "SFNEST_", file_optional = "/nonexistent/config.toml")]
    struct AppConfig {
        #[env(var = "APP_NAME", default = "myapp")]
        name: String,

        #[env(flatten)]
        database: DatabaseConfig,
    }

    let config = AppConfig::from_config().expect("should load nested without serde");

    assert_eq!(config.name, "myapp");
    assert_eq!(config.database.host, "localhost");
    assert_eq!(config.database.port, 5432);
}

#[test]
fn test_nested_config_from_file() {
    cleanup_env(&["SFNESTF_APP_NAME", "SFNESTF_DB_HOST", "SFNESTF_DB_PORT"]);

    let content = r#"
app_name = "file-app"

[database]
host = "db.example.com"
port = 5433
"#;
    write_file("serde_free_nested.toml", content);

    #[derive(EnvConfig)]
    struct DbConfig {
        #[env(var = "DB_HOST", default = "localhost")]
        host: String,

        #[env(var = "DB_PORT", default = "5432")]
        port: u16,
    }

    #[derive(EnvConfig)]
    #[env_config(prefix = "SFNESTF_", file_optional = "/tmp/procenv_serde_free_tests/serde_free_nested.toml")]
    struct NestedFileConfig {
        #[env(var = "APP_NAME", default = "default")]
        app_name: String,

        #[env(flatten)]
        database: DbConfig,
    }

    let config = NestedFileConfig::from_config().expect("should load nested from file");

    assert_eq!(config.app_name, "file-app");
    assert_eq!(config.database.host, "db.example.com");
    assert_eq!(config.database.port, 5433);

    cleanup_file("serde_free_nested.toml");
}

// ============================================================================
// Environment Override of File Values
// ============================================================================

#[test]
fn test_env_overrides_file_values() {
    cleanup_env(&["SFOVER_HOST", "SFOVER_PORT"]);

    let content = r#"{"host": "file-host", "port": 3000}"#;
    write_file("serde_free_override.json", content);

    #[derive(EnvConfig)]
    #[env_config(prefix = "SFOVER_", file_optional = "/tmp/procenv_serde_free_tests/serde_free_override.json")]
    struct OverrideConfig {
        #[env(var = "HOST")]
        host: String,

        #[env(var = "PORT", default = "8080")]
        port: u16,
    }

    // Set env var to override file value
    unsafe {
        std::env::set_var("SFOVER_HOST", "env-host");
    }

    let config = OverrideConfig::from_config().expect("should allow env override");

    assert_eq!(config.host, "env-host"); // from env
    assert_eq!(config.port, 3000); // from file

    cleanup_env(&["SFOVER_HOST"]);
    cleanup_file("serde_free_override.json");
}

// ============================================================================
// Source Attribution
// ============================================================================

#[test]
fn test_source_attribution_without_serde() {
    cleanup_env(&["SFSRC_NAME", "SFSRC_PORT"]);

    let content = r#"name = "from-file""#;
    write_file("serde_free_sources.toml", content);

    #[derive(EnvConfig)]
    #[env_config(prefix = "SFSRC_", file_optional = "/tmp/procenv_serde_free_tests/serde_free_sources.toml")]
    struct SourceConfig {
        #[env(var = "NAME")]
        name: String,

        #[env(var = "PORT", default = "8080")]
        port: u16,
    }

    let (config, sources) = SourceConfig::from_config_with_sources()
        .expect("should load with sources");

    assert_eq!(config.name, "from-file");
    assert_eq!(config.port, 8080);

    // Name should come from file
    let name_source = sources.get("name").expect("should have name source");
    assert!(matches!(name_source.source, procenv::Source::ConfigFile(_)));

    // Port should come from default
    let port_source = sources.get("port").expect("should have port source");
    assert!(matches!(port_source.source, procenv::Source::Default));

    cleanup_file("serde_free_sources.toml");
}

// ============================================================================
// Error Handling
// ============================================================================

#[test]
fn test_missing_required_field_error() {
    cleanup_env(&["SFERR_NAME", "SFERR_PORT"]);

    #[derive(EnvConfig)]
    #[env_config(prefix = "SFERR_", file_optional = "/nonexistent/config.toml")]
    struct RequiredConfig {
        #[env(var = "NAME")] // No default, required!
        name: String,

        #[env(var = "PORT", default = "8080")]
        port: u16,
    }

    let result = RequiredConfig::from_config();
    assert!(result.is_err(), "should fail when required field missing");

    let err = result.unwrap_err();
    let err_str = err.to_string();
    assert!(
        err_str.contains("name") || err_str.contains("NAME"),
        "error should mention the missing field"
    );
}

#[test]
fn test_parse_error_on_invalid_type() {
    cleanup_env(&["SFPARSE_PORT"]);

    let content = r#"{"port": "not-a-number"}"#;
    write_file("serde_free_parse_error.json", content);

    #[derive(EnvConfig)]
    #[env_config(prefix = "SFPARSE_", file_optional = "/tmp/procenv_serde_free_tests/serde_free_parse_error.json")]
    struct ParseConfig {
        #[env(var = "PORT")]
        port: u16,
    }

    let result = ParseConfig::from_config();
    assert!(result.is_err(), "should fail on type mismatch");

    cleanup_file("serde_free_parse_error.json");
}

// ============================================================================
// Various Types
// ============================================================================

#[test]
fn test_various_types() {
    cleanup_env(&["SFTYPE_STR", "SFTYPE_U8", "SFTYPE_I32", "SFTYPE_F64", "SFTYPE_BOOL"]);

    let content = r#"
str_val = "hello"
u8_val = 255
i32_val = -42
f64_val = 3.14159
bool_val = true
"#;
    write_file("serde_free_types.toml", content);

    #[derive(EnvConfig)]
    #[env_config(prefix = "SFTYPE_", file_optional = "/tmp/procenv_serde_free_tests/serde_free_types.toml")]
    struct TypesConfig {
        #[env(var = "STR", default = "")]
        str_val: String,

        #[env(var = "U8", default = "0")]
        u8_val: u8,

        #[env(var = "I32", default = "0")]
        i32_val: i32,

        #[env(var = "F64", default = "0.0")]
        f64_val: f64,

        #[env(var = "BOOL", default = "false")]
        bool_val: bool,
    }

    let config = TypesConfig::from_config().expect("should handle various types");

    assert_eq!(config.str_val, "hello");
    assert_eq!(config.u8_val, 255);
    assert_eq!(config.i32_val, -42);
    assert!((config.f64_val - 3.14159).abs() < 0.0001);
    assert!(config.bool_val);

    cleanup_file("serde_free_types.toml");
}
