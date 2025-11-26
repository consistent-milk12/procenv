//! Compile check for format attribute with optional/default field semantics
//!
//! This verifies that format = "json" works correctly with:
//! - Required fields (must be present)
//! - Optional fields (Option<T>, None if missing)
//! - Default fields (uses default value if missing)

use procenv::EnvConfig;
use serde::Deserialize;

#[derive(Debug, Deserialize, PartialEq)]
struct Tags {
    values: Vec<String>,
}

#[derive(EnvConfig)]
struct Config {
    // Required format field - should error if missing
    #[env(var = "REQUIRED_JSON", format = "json")]
    required: Tags,

    // Optional format field - should be None if missing, no error
    #[env(var = "OPTIONAL_JSON", format = "json", optional)]
    optional: Option<Tags>,

    // Default format field - should use default if missing
    #[env(
        var = "DEFAULT_JSON",
        format = "json",
        default = r#"{"values":["default"]}"#
    )]
    with_default: Tags,
}

fn main() {}
