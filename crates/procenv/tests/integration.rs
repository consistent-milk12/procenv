//! Integration tests for EnvConfig derive macro
//!
//! Run with: cargo nextest run --package procenv

use std::process::Command;

/// Helper to run the example with specific environment variables
fn run_example(env_vars: &[(&str, &str)]) -> (bool, String, String) {
    let mut cmd = Command::new("cargo");
    cmd.args([
        "run",
        "--package",
        "procenv",
        "--example",
        "basic",
        "--quiet",
    ]);

    // Start with a clean environment, only keeping essentials for cargo to work
    cmd.env_clear();
    cmd.env("PATH", std::env::var("PATH").unwrap_or_default());
    cmd.env("HOME", std::env::var("HOME").unwrap_or_default());
    cmd.env(
        "CARGO_HOME",
        std::env::var("CARGO_HOME")
            .unwrap_or_else(|_| format!("{}/.cargo", std::env::var("HOME").unwrap_or_default())),
    );
    cmd.env(
        "RUSTUP_HOME",
        std::env::var("RUSTUP_HOME")
            .unwrap_or_else(|_| format!("{}/.rustup", std::env::var("HOME").unwrap_or_default())),
    );

    // Set the specified env vars for the test
    for (key, value) in env_vars {
        cmd.env(key, value);
    }

    let output = cmd.output().expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    (output.status.success(), stdout, stderr)
}

#[test]
fn test_missing_required_vars() {
    let (success, _stdout, stderr) = run_example(&[]);

    assert!(!success, "Should fail when required vars are missing");
    assert!(
        stderr.contains("configuration error"),
        "Should mention configuration error: {stderr}"
    );
}

#[test]
fn test_all_required_vars_set() {
    let (success, stdout, _stderr) = run_example(&[
        ("DATABASE_URL", "postgres://localhost"),
        ("SECRET", "mysecret"),
    ]);

    assert!(success, "Should succeed with required vars set");
    assert!(stdout.contains("Successfully loaded config!"));
    assert!(stdout.contains("DATABASE_URL    = postgres://localhost"));
    assert!(stdout.contains("PORT            = 8080")); // default
    assert!(stdout.contains("DEBUG           = false")); // default
}

#[test]
fn test_optional_fields_set() {
    let (success, stdout, _stderr) = run_example(&[
        ("DATABASE_URL", "postgres://localhost"),
        ("SECRET", "mysecret"),
        ("API_KEY", "my-api-key"),
        ("MAX_CONNECTIONS", "100"),
    ]);

    assert!(success, "Should succeed with optional vars set");
    assert!(stdout.contains("API_KEY         = Some(\"my-api-key\")"));
    assert!(stdout.contains("MAX_CONNECTIONS = Some(100)"));
}

#[test]
fn test_optional_fields_unset() {
    let (success, stdout, _stderr) = run_example(&[
        ("DATABASE_URL", "postgres://localhost"),
        ("SECRET", "mysecret"),
    ]);

    assert!(success);
    assert!(stdout.contains("API_KEY         = None"));
    assert!(stdout.contains("MAX_CONNECTIONS = None"));
}

#[test]
fn test_default_values_override() {
    let (success, stdout, _stderr) = run_example(&[
        ("DATABASE_URL", "postgres://localhost"),
        ("SECRET", "mysecret"),
        ("PORT", "3000"),
        ("DEBUG", "true"),
    ]);

    assert!(success, "Should succeed with overridden defaults");
    assert!(stdout.contains("PORT            = 3000"));
    assert!(stdout.contains("DEBUG           = true"));
}

#[test]
fn test_parse_error_port() {
    let (success, _stdout, stderr) = run_example(&[
        ("DATABASE_URL", "postgres://localhost"),
        ("SECRET", "mysecret"),
        ("PORT", "invalid"),
    ]);

    assert!(!success, "Should fail with invalid PORT");
    assert!(
        stderr.contains("failed to parse PORT"),
        "Should mention PORT parse error: {stderr}"
    );
    assert!(
        stderr.contains("invalid"),
        "Should show the invalid value: {stderr}"
    );
}

#[test]
fn test_parse_error_bool() {
    let (success, _stdout, stderr) = run_example(&[
        ("DATABASE_URL", "postgres://localhost"),
        ("SECRET", "mysecret"),
        ("DEBUG", "notabool"),
    ]);

    assert!(!success, "Should fail with invalid DEBUG");
    assert!(
        stderr.contains("failed to parse DEBUG"),
        "Should mention DEBUG parse error: {stderr}"
    );
}

#[test]
fn test_parse_error_optional() {
    let (success, _stdout, stderr) = run_example(&[
        ("DATABASE_URL", "postgres://localhost"),
        ("SECRET", "mysecret"),
        ("MAX_CONNECTIONS", "notanumber"),
    ]);

    assert!(!success, "Should fail with invalid optional field");
    assert!(
        stderr.contains("failed to parse MAX_CONNECTIONS"),
        "Should mention MAX_CONNECTIONS parse error: {stderr}"
    );
}

#[test]
fn test_multiple_errors() {
    let (success, _stdout, stderr) = run_example(&[("PORT", "invalid"), ("DEBUG", "bad")]);

    assert!(!success, "Should fail with multiple errors");
    assert!(
        stderr.contains("4 configuration error"),
        "Should report 4 errors (2 missing + 2 parse): {stderr}"
    );
}

#[test]
fn test_error_accumulation() {
    // Missing both DATABASE_URL and SECRET
    let (success, _stdout, stderr) = run_example(&[]);

    assert!(!success);
    assert!(
        stderr.contains("2 configuration error"),
        "Should accumulate errors: {stderr}"
    );
}

// ============================================================================
// Source Attribution Tests
// ============================================================================

/// Helper to run the source_attribution example
fn run_source_example(env_vars: &[(&str, &str)]) -> (bool, String, String) {
    let mut cmd = Command::new("cargo");
    cmd.args([
        "run",
        "--package",
        "procenv",
        "--example",
        "source_attribution",
        "--quiet",
    ]);

    cmd.env_clear();
    cmd.env("PATH", std::env::var("PATH").unwrap_or_default());
    cmd.env("HOME", std::env::var("HOME").unwrap_or_default());
    cmd.env(
        "CARGO_HOME",
        std::env::var("CARGO_HOME")
            .unwrap_or_else(|_| format!("{}/.cargo", std::env::var("HOME").unwrap_or_default())),
    );
    cmd.env(
        "RUSTUP_HOME",
        std::env::var("RUSTUP_HOME")
            .unwrap_or_else(|_| format!("{}/.rustup", std::env::var("HOME").unwrap_or_default())),
    );

    for (key, value) in env_vars {
        cmd.env(key, value);
    }

    let output = cmd.output().expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    (output.status.success(), stdout, stderr)
}

#[test]
fn test_source_attribution_runs() {
    let (success, stdout, stderr) = run_source_example(&[]);

    assert!(
        success,
        "Source attribution example should run successfully. stderr: {stderr}"
    );
    assert!(
        stdout.contains("Configuration Source"),
        "Should show source header: {stdout}"
    );
}

#[test]
fn test_source_shows_environment_variable() {
    let (success, stdout, _stderr) = run_source_example(&[]);

    assert!(success);
    // The example sets APP_DATABASE_URL and APP_DEBUG via set_var
    assert!(
        stdout.contains("Environment variable"),
        "Should show environment variable source: {stdout}"
    );
}

#[test]
fn test_source_shows_default_value() {
    let (success, stdout, _stderr) = run_source_example(&[]);

    assert!(success);
    // The example doesn't set APP_PORT, so it should use default
    assert!(
        stdout.contains("Default value"),
        "Should show default value source: {stdout}"
    );
    assert!(
        stdout.contains("port: 8080"),
        "Port should be default 8080: {stdout}"
    );
}

#[test]
fn test_source_shows_not_set() {
    let (success, stdout, _stderr) = run_source_example(&[]);

    assert!(success);
    // The example doesn't set APP_API_KEY (optional field)
    assert!(
        stdout.contains("Not set"),
        "Should show not set for optional field: {stdout}"
    );
    assert!(
        stdout.contains("api_key: None"),
        "api_key should be None: {stdout}"
    );
}

#[test]
fn test_source_attribution_display_format() {
    let (success, stdout, _stderr) = run_source_example(&[]);

    assert!(success);
    // Check that the display format shows field names and env var names
    assert!(
        stdout.contains("[APP_DATABASE_URL]"),
        "Should show env var name in brackets: {stdout}"
    );
    assert!(
        stdout.contains("[APP_PORT]"),
        "Should show env var name for port: {stdout}"
    );
    assert!(stdout.contains("<-"), "Should use arrow format: {stdout}");
}

// ============================================================================
// Prefix Support Tests
// ============================================================================

/// Helper to run the prefix_example
fn run_prefix_example(env_vars: &[(&str, &str)]) -> (bool, String, String) {
    let mut cmd = Command::new("cargo");
    cmd.args([
        "run",
        "--package",
        "procenv",
        "--example",
        "prefix_example",
        "--quiet",
    ]);

    cmd.env_clear();
    cmd.env("PATH", std::env::var("PATH").unwrap_or_default());
    cmd.env("HOME", std::env::var("HOME").unwrap_or_default());
    cmd.env(
        "CARGO_HOME",
        std::env::var("CARGO_HOME")
            .unwrap_or_else(|_| format!("{}/.cargo", std::env::var("HOME").unwrap_or_default())),
    );
    cmd.env(
        "RUSTUP_HOME",
        std::env::var("RUSTUP_HOME")
            .unwrap_or_else(|_| format!("{}/.rustup", std::env::var("HOME").unwrap_or_default())),
    );

    for (key, value) in env_vars {
        cmd.env(key, value);
    }

    let output = cmd.output().expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    (output.status.success(), stdout, stderr)
}

#[test]
fn test_prefix_applies_to_var_names() {
    // Should require APP_DATABASE_URL (with prefix), not DATABASE_URL
    let (success, stdout, _stderr) =
        run_prefix_example(&[("APP_DATABASE_URL", "postgres://localhost")]);

    assert!(success, "Should succeed with prefixed var");
    assert!(
        stdout.contains("db_url (APP_DATABASE_URL): postgres://localhost"),
        "Should read APP_DATABASE_URL: {stdout}"
    );
}

#[test]
fn test_prefix_default_values_work() {
    let (success, stdout, _stderr) =
        run_prefix_example(&[("APP_DATABASE_URL", "postgres://localhost")]);

    assert!(success);
    // APP_PORT has default "8080"
    assert!(
        stdout.contains("port (APP_PORT): 8080"),
        "Should use default for APP_PORT: {stdout}"
    );
}

#[test]
fn test_prefix_can_be_overridden() {
    let (success, stdout, _stderr) = run_prefix_example(&[
        ("APP_DATABASE_URL", "postgres://localhost"),
        ("APP_PORT", "3000"),
    ]);

    assert!(success);
    assert!(
        stdout.contains("port (APP_PORT): 3000"),
        "Should override default: {stdout}"
    );
}

#[test]
fn test_no_prefix_skips_prefix() {
    // GLOBAL_DEBUG should work without APP_ prefix
    let (success, stdout, _stderr) = run_prefix_example(&[
        ("APP_DATABASE_URL", "postgres://localhost"),
        ("GLOBAL_DEBUG", "true"),
    ]);

    assert!(success);
    assert!(
        stdout.contains("debug (GLOBAL_DEBUG): true"),
        "no_prefix field should read GLOBAL_DEBUG: {stdout}"
    );
}

#[test]
fn test_prefix_missing_required_shows_prefixed_name() {
    // Don't set APP_DATABASE_URL
    let (success, _stdout, stderr) = run_prefix_example(&[]);

    assert!(!success, "Should fail when required var is missing");
    assert!(
        stderr.contains("APP_DATABASE_URL"),
        "Error should mention prefixed var name: {stderr}"
    );
}

// ============================================================================
// .env.example Generation Tests
// ============================================================================

/// Helper to run the env_example_gen example
fn run_env_example_gen() -> (bool, String, String) {
    let mut cmd = Command::new("cargo");
    cmd.args([
        "run",
        "--package",
        "procenv",
        "--example",
        "env_example_gen",
        "--quiet",
    ]);

    let output = cmd.output().expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    (output.status.success(), stdout, stderr)
}

#[test]
fn test_env_example_generates_output() {
    let (success, stdout, stderr) = run_env_example_gen();

    assert!(
        success,
        "env_example_gen should run successfully. stderr: {stderr}"
    );
    assert!(!stdout.is_empty(), "Should generate non-empty output");
}

#[test]
fn test_env_example_has_header() {
    let (success, stdout, _stderr) = run_env_example_gen();

    assert!(success);
    assert!(
        stdout.contains("Auto-generated by procenv"),
        "Should have header comment: {stdout}"
    );
}

#[test]
fn test_env_example_includes_required_vars() {
    let (success, stdout, _stderr) = run_env_example_gen();

    assert!(success);
    // Required fields should show with = (empty value)
    assert!(
        stdout.contains("API_KEY="),
        "Should include required API_KEY: {stdout}"
    );
    assert!(
        stdout.contains("DATABASE_URL="),
        "Should include required DATABASE_URL: {stdout}"
    );
}

#[test]
fn test_env_example_shows_defaults_commented() {
    let (success, stdout, _stderr) = run_env_example_gen();

    assert!(success);
    // Fields with defaults should be commented out with the default value
    assert!(
        stdout.contains("# PORT=8080") || stdout.contains("# APP_NAME=my-app"),
        "Should show defaults as comments: {stdout}"
    );
}

#[test]
fn test_env_example_includes_doc_comments() {
    let (success, stdout, _stderr) = run_env_example_gen();

    assert!(success);
    // Doc comments should appear as # comments
    assert!(
        stdout.contains("Database connection URL") || stdout.contains("Server port"),
        "Should include doc comments: {stdout}"
    );
}

#[test]
fn test_env_example_marks_secrets() {
    let (success, stdout, _stderr) = run_env_example_gen();

    assert!(success);
    assert!(
        stdout.contains("secret"),
        "Should mark secret fields: {stdout}"
    );
}

#[test]
fn test_env_example_includes_type_hints() {
    let (success, stdout, _stderr) = run_env_example_gen();

    assert!(success);
    // Type hints should be included
    assert!(
        stdout.contains("type:"),
        "Should include type hints: {stdout}"
    );
}

#[test]
fn test_env_example_includes_nested_config() {
    let (success, stdout, _stderr) = run_env_example_gen();

    assert!(success);
    // Nested DatabaseConfig fields should be included
    assert!(
        stdout.contains("DATABASE_URL"),
        "Should include nested DATABASE_URL: {stdout}"
    );
    assert!(
        stdout.contains("DATABASE_POOL_SIZE") || stdout.contains("POOL_SIZE"),
        "Should include nested pool_size: {stdout}"
    );
}

// ============================================================================
// File Configuration Tests (Phase 13)
// ============================================================================

/// Helper to run the file_config example
fn run_file_config_example(env_vars: &[(&str, &str)]) -> (bool, String, String) {
    let mut cmd = Command::new("cargo");
    cmd.args([
        "run",
        "--package",
        "procenv",
        "--example",
        "file_config",
        "--features",
        "file-all",
        "--quiet",
    ]);

    // Set current dir to workspace root (where Cargo.toml is)
    // This is needed because the example uses relative paths to config files
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    let workspace_root = std::path::Path::new(&manifest_dir)
        .parent()
        .and_then(|p| p.parent())
        .unwrap_or(std::path::Path::new("."));
    cmd.current_dir(workspace_root);

    cmd.env_clear();
    cmd.env("PATH", std::env::var("PATH").unwrap_or_default());
    cmd.env("HOME", std::env::var("HOME").unwrap_or_default());
    cmd.env(
        "CARGO_HOME",
        std::env::var("CARGO_HOME")
            .unwrap_or_else(|_| format!("{}/.cargo", std::env::var("HOME").unwrap_or_default())),
    );
    cmd.env(
        "RUSTUP_HOME",
        std::env::var("RUSTUP_HOME")
            .unwrap_or_else(|_| format!("{}/.rustup", std::env::var("HOME").unwrap_or_default())),
    );

    for (key, value) in env_vars {
        cmd.env(key, value);
    }

    let output = cmd.output().expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    (output.status.success(), stdout, stderr)
}

#[test]
fn test_file_config_loads_toml() {
    let (success, stdout, stderr) = run_file_config_example(&[]);

    assert!(
        success,
        "File config example should run successfully. stderr: {stderr}"
    );
    assert!(
        stdout.contains("app-from-toml"),
        "Should load name from TOML file: {stdout}"
    );
}

#[test]
fn test_file_config_loads_nested_struct() {
    let (success, stdout, _stderr) = run_file_config_example(&[]);

    assert!(success);
    assert!(
        stdout.contains("localhost:5432/myapp_db"),
        "Should load nested database config: {stdout}"
    );
}

#[test]
fn test_file_config_env_override() {
    let (success, stdout, _stderr) = run_file_config_example(&[]);

    assert!(success);
    // The example sets APP_PORT=9000 and APP_DEBUG=true internally
    assert!(
        stdout.contains("port:     9000"),
        "Should show env override for port: {stdout}"
    );
    assert!(
        stdout.contains("debug:    true"),
        "Should show env override for debug: {stdout}"
    );
}

#[test]
fn test_file_config_defaults_overridden() {
    let (success, stdout, _stderr) = run_file_config_example(&[]);

    assert!(success);
    // Example 3 shows file overriding defaults
    assert!(
        stdout.contains("file overrides default"),
        "Should show file overriding defaults: {stdout}"
    );
}

#[test]
fn test_file_config_priority_order() {
    let (success, stdout, _stderr) = run_file_config_example(&[]);

    assert!(success);
    assert!(
        stdout.contains("Priority Order"),
        "Should explain priority order: {stdout}"
    );
    assert!(
        stdout.contains("Environment variables (highest)"),
        "Should show env vars have highest priority: {stdout}"
    );
}

// ============================================================================
// Serde Format Semantics Tests (Phase 17 - Format Loader Parity)
// ============================================================================

mod serde_format_tests {
    use procenv::EnvConfig;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, PartialEq)]
    struct Tags {
        values: Vec<String>,
    }

    #[derive(EnvConfig)]
    struct FormatConfig {
        #[env(var = "TEST_REQUIRED_JSON", format = "json")]
        required: Tags,

        #[env(var = "TEST_OPTIONAL_JSON", format = "json", optional)]
        optional: Option<Tags>,

        #[env(
            var = "TEST_DEFAULT_JSON",
            format = "json",
            default = r#"{"values":["default"]}"#
        )]
        with_default: Tags,
    }

    #[test]
    fn test_format_optional_missing_returns_none() {
        // SAFETY: Test is single-threaded, env manipulation is isolated
        unsafe {
            std::env::set_var("TEST_REQUIRED_JSON", r#"{"values":["req"]}"#);
            std::env::set_var("TEST_DEFAULT_JSON", r#"{"values":["custom"]}"#);
            std::env::remove_var("TEST_OPTIONAL_JSON");
        }

        let config = FormatConfig::from_env().expect("should load");
        assert!(config.optional.is_none(), "Optional field should be None when env var missing");

        unsafe {
            std::env::remove_var("TEST_REQUIRED_JSON");
            std::env::remove_var("TEST_DEFAULT_JSON");
        }
    }

    #[test]
    fn test_format_default_used_when_missing() {
        unsafe {
            std::env::set_var("TEST_REQUIRED_JSON", r#"{"values":["req"]}"#);
            std::env::remove_var("TEST_DEFAULT_JSON");
            std::env::remove_var("TEST_OPTIONAL_JSON");
        }

        let config = FormatConfig::from_env().expect("should load with default");
        assert_eq!(
            config.with_default.values,
            vec!["default".to_string()],
            "Should use default JSON value"
        );

        unsafe {
            std::env::remove_var("TEST_REQUIRED_JSON");
        }
    }

    #[test]
    fn test_format_optional_set_returns_some() {
        unsafe {
            std::env::set_var("TEST_REQUIRED_JSON", r#"{"values":["req"]}"#);
            std::env::set_var("TEST_OPTIONAL_JSON", r#"{"values":["opt"]}"#);
            std::env::remove_var("TEST_DEFAULT_JSON");
        }

        let config = FormatConfig::from_env().expect("should load");
        assert_eq!(
            config.optional.unwrap().values,
            vec!["opt".to_string()],
            "Optional field should have parsed value"
        );

        unsafe {
            std::env::remove_var("TEST_REQUIRED_JSON");
            std::env::remove_var("TEST_OPTIONAL_JSON");
        }
    }

    #[test]
    fn test_format_default_overridden_by_env() {
        unsafe {
            std::env::set_var("TEST_REQUIRED_JSON", r#"{"values":["req"]}"#);
            std::env::set_var("TEST_DEFAULT_JSON", r#"{"values":["overridden"]}"#);
            std::env::remove_var("TEST_OPTIONAL_JSON");
        }

        let config = FormatConfig::from_env().expect("should load");
        assert_eq!(
            config.with_default.values,
            vec!["overridden".to_string()],
            "Env var should override default"
        );

        unsafe {
            std::env::remove_var("TEST_REQUIRED_JSON");
            std::env::remove_var("TEST_DEFAULT_JSON");
        }
    }
}
