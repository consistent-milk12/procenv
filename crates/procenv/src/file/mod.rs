//! File-based configuration support.
//!
//! This module provides utilities for loading configuration from files
//! and merging multiple configuration sources with proper layering.
//! It is enabled with the `file` feature flag.
//!
//! # Supported Formats
//!
//! | Format | Feature Flag | Extensions |
//! |--------|--------------|------------|
//! | JSON | `file` (always) | `.json` |
//! | TOML | `toml` | `.toml` |
//! | YAML | `yaml` | `.yaml`, `.yml` |
//!
//! # Layering Priority
//!
//! Configuration sources are merged in this order (lowest to highest priority):
//!
//! 1. **Compiled defaults** - `#[env(default = "...")]`
//! 2. **Config files** - In order specified (later files override earlier)
//! 3. **Dotenv files** - `.env` file values
//! 4. **Environment variables** - Highest priority, always wins
//!
//! # Example Usage
//!
//! ## With Derive Macro
//!
//! ```rust,ignore
//! use procenv::EnvConfig;
//! use serde::Deserialize;
//!
//! #[derive(EnvConfig, Deserialize)]
//! #[env_config(
//!     file_optional = "config.toml",
//!     file_optional = "config.local.toml",
//!     prefix = "APP_"
//! )]
//! struct Config {
//!     #[env(var = "DATABASE_URL")]
//!     database_url: String,
//!
//!     #[env(var = "PORT", default = "8080")]
//!     port: u16,
//! }
//!
//! // Loads: defaults -> config.toml -> config.local.toml -> env vars
//! let config = Config::from_config()?;
//! ```
//!
//! ## With `ConfigBuilder` (Manual)
//!
//! ```rust,ignore
//! use procenv::file::{ConfigBuilder, FileFormat};
//! use serde::Deserialize;
//!
//! #[derive(Deserialize)]
//! struct DatabaseConfig {
//!     host: String,
//!     port: u16,
//! }
//!
//! let config: DatabaseConfig = ConfigBuilder::new()
//!     .file_optional("config.toml")
//!     .file_optional("config.local.toml")
//!     .env_prefix("DB_")
//!     .build()?;
//! ```
//!
//! # Error Handling
//!
//! Parse errors include source location information when available,
//! enabling rich diagnostic output via [`miette`]:
//!
//! ```text
//! Error: TOML parse error in config.toml
//!    ╭─[config.toml:3:8]
//!    │
//!  3 │ port = "not_a_number"
//!    │        ^^^^^^^^^^^^^^ expected integer, found string
//!    ╰────
//!   help: check for missing quotes, invalid values, or syntax errors
//! ```

// FileError is intentionally large to provide rich miette diagnostics with source spans
#![allow(clippy::result_large_err)]

mod builder;
mod error;
mod format;
mod origin;
mod utils;

pub use builder::ConfigBuilder;
pub use builder::DefaultsSerializationError;
pub use error::FileError;
pub use format::FileFormat;
pub use origin::OriginTracker;
pub use utils::FileUtils;

// Re-export serde_json types for macro-generated code
// These allow the macro to reference types through ::procenv::file:: paths
// which provides clear compile errors when the file feature isn't enabled
pub use serde_json::Map as JsonMap;
pub use serde_json::Value as JsonValue;

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json as SJSON;
    use std::path::Path;

    #[test]
    fn test_coerce_value_bool() {
        assert_eq!(FileUtils::coerce_value("true"), SJSON::Value::Bool(true));
        assert_eq!(FileUtils::coerce_value("TRUE"), SJSON::Value::Bool(true));
        assert_eq!(FileUtils::coerce_value("false"), SJSON::Value::Bool(false));
        assert_eq!(FileUtils::coerce_value("FALSE"), SJSON::Value::Bool(false));
    }

    #[test]
    fn test_coerce_value_integer() {
        assert_eq!(
            FileUtils::coerce_value("42"),
            SJSON::Value::Number(42.into())
        );
        assert_eq!(
            FileUtils::coerce_value("-100"),
            SJSON::Value::Number((-100).into())
        );
        assert_eq!(FileUtils::coerce_value("0"), SJSON::Value::Number(0.into()));
    }

    #[test]
    #[allow(clippy::approx_constant)]
    fn test_coerce_value_float() {
        let val = FileUtils::coerce_value("3.14");
        if let SJSON::Value::Number(n) = val {
            assert!((n.as_f64().unwrap() - 3.14).abs() < 0.001);
        } else {
            panic!("Expected number");
        }
    }

    #[test]
    fn test_coerce_value_string() {
        assert_eq!(
            FileUtils::coerce_value("hello"),
            SJSON::Value::String("hello".to_string())
        );
        assert_eq!(
            FileUtils::coerce_value("hello world"),
            SJSON::Value::String("hello world".to_string())
        );
    }

    #[test]
    fn test_deep_merge_objects() {
        let mut base = SJSON::json!({
            "a": 1,
            "b": {"x": 10, "y": 20}
        });
        let overlay = SJSON::json!({
            "b": {"y": 200, "z": 30},
            "c": 3
        });

        FileUtils::deep_merge(&mut base, overlay);

        assert_eq!(base["a"], 1);
        assert_eq!(base["b"]["x"], 10);
        assert_eq!(base["b"]["y"], 200);
        assert_eq!(base["b"]["z"], 30);
        assert_eq!(base["c"], 3);
    }

    #[test]
    fn test_deep_merge_replace() {
        let mut base = SJSON::json!({"a": [1, 2, 3]});
        let overlay = SJSON::json!({"a": [4, 5]});

        FileUtils::deep_merge(&mut base, overlay);

        assert_eq!(base["a"], SJSON::json!([4, 5]));
    }

    #[test]
    fn test_insert_nested() {
        let mut map = SJSON::Map::new();
        FileUtils::insert_nested(
            &mut map,
            &["database", "host"],
            SJSON::Value::String("localhost".into()),
        );

        assert_eq!(
            map.get("database")
                .and_then(|v| v.get("host"))
                .and_then(|v| v.as_str()),
            Some("localhost")
        );
    }

    #[test]
    fn test_file_format_detection() {
        assert_eq!(
            FileFormat::from_path(Path::new("config.json")),
            Some(FileFormat::Json)
        );

        #[cfg(feature = "toml")]
        assert_eq!(
            FileFormat::from_path(Path::new("config.toml")),
            Some(FileFormat::Toml)
        );

        #[cfg(feature = "yaml")]
        {
            assert_eq!(
                FileFormat::from_path(Path::new("config.yaml")),
                Some(FileFormat::Yaml)
            );
            assert_eq!(
                FileFormat::from_path(Path::new("config.yml")),
                Some(FileFormat::Yaml)
            );
        }

        assert_eq!(FileFormat::from_path(Path::new("config.txt")), None);
    }

    #[test]
    fn test_find_field_offset_json() {
        let content = r#"{"name": "test", "port": "bad"}"#;
        let offset = FileUtils::find_field_offset(content, "port", FileFormat::Json);
        assert!(offset.is_some());
        let off = offset.unwrap();
        assert_eq!(&content[off..off + 5], "\"bad\"");
    }

    #[cfg(feature = "toml")]
    #[test]
    fn test_find_field_offset_toml() {
        let content = "name = \"test\"\nport = \"bad\"";
        let offset = FileUtils::find_field_offset(content, "port", FileFormat::Toml);
        assert!(offset.is_some());
        let off = offset.unwrap();
        assert_eq!(&content[off..off + 5], "\"bad\"");
    }
}
