//! Property-based tests for procenv invariants.
//!
//! These tests verify that critical invariants hold for all possible inputs,
//! not just hand-picked test cases.

use proptest::prelude::*;
use std::collections::HashMap;

// ============================================================================
// ConfigValue Properties
// ============================================================================

mod config_value_properties {
    use super::*;
    use procenv::ConfigValue;

    proptest! {
        /// from_str_infer never panics on any input
        #[test]
        fn from_str_infer_never_panics(s in ".*") {
            let _ = ConfigValue::from_str_infer(&s);
        }

        /// Boolean strings always parse to Boolean variant
        #[test]
        fn bool_strings_parse_to_bool(b in prop::bool::ANY) {
            let s = if b { "true" } else { "false" };
            let value = ConfigValue::from_str_infer(s);
            prop_assert!(matches!(value, ConfigValue::Boolean(_)));
        }

        /// Unsigned integers roundtrip correctly
        #[test]
        fn unsigned_int_roundtrip(n in 0u64..=u64::MAX) {
            let s = n.to_string();
            let value = ConfigValue::from_str_infer(&s);

            match value {
                ConfigValue::UnsignedInteger(parsed) => prop_assert_eq!(parsed, n),
                _ => prop_assert!(false, "Expected UnsignedInteger, got {:?}", value),
            }
        }

        /// Negative integers parse to Integer variant
        #[test]
        fn negative_int_roundtrip(n in i64::MIN..0i64) {
            let s = n.to_string();
            let value = ConfigValue::from_str_infer(&s);

            match value {
                ConfigValue::Integer(parsed) => prop_assert_eq!(parsed, n),
                _ => prop_assert!(false, "Expected Integer, got {:?}", value),
            }
        }

        /// Float roundtrip (with tolerance for precision)
        #[test]
        fn float_roundtrip(f in prop::num::f64::NORMAL) {
            let s = format!("{}", f);
            let value = ConfigValue::from_str_infer(&s);

            // Should parse as float if it contains decimal
            if s.contains('.') || s.contains('e') || s.contains('E') {
                match value {
                    ConfigValue::Float(parsed) => {
                        // Allow for floating point precision issues
                        let diff = (parsed - f).abs();
                        let tolerance = f.abs() * 1e-10 + 1e-10;
                        prop_assert!(diff < tolerance,
                            "Float mismatch: {} vs {} (diff: {})", f, parsed, diff);
                    }
                    _ => {} // May parse as int if no decimal in formatted output
                }
            }
        }

        /// to_string produces valid Display output (never panics)
        #[test]
        fn display_never_panics(s in ".*") {
            let value = ConfigValue::from_str_infer(&s);
            let _ = format!("{}", value);
            let _ = format!("{:?}", value);
        }
    }
}

// ============================================================================
// MaybeRedacted Properties
// ============================================================================

mod maybe_redacted_properties {
    use super::*;
    use procenv::MaybeRedacted;

    proptest! {
        /// CRITICAL: Secret values are NEVER stored or exposed
        /// Uses secrets with minimum length of 8 chars to avoid false positives
        /// from short substrings matching common words like "<redacted>"
        #[test]
        fn secrets_never_exposed(value in "[a-zA-Z0-9]{8,32}") {
            let redacted = MaybeRedacted::new(&value, true);

            // Structural check: value not accessible
            prop_assert!(redacted.is_redacted());
            prop_assert!(redacted.as_str().is_none());

            // Display check: original value never in output
            let debug = format!("{:?}", redacted);
            let display = format!("{}", redacted);

            prop_assert!(!debug.contains(&value),
                "Secret '{}' leaked in Debug: {}", value, debug);
            prop_assert!(!display.contains(&value),
                "Secret '{}' leaked in Display: {}", value, display);
        }

        /// Non-secret values are preserved exactly
        #[test]
        fn plain_values_preserved(value in ".*") {
            let plain = MaybeRedacted::new(&value, false);

            prop_assert!(!plain.is_redacted());
            prop_assert_eq!(plain.as_str(), Some(value.as_str()));
        }

        /// Clone preserves redaction state
        #[test]
        fn clone_preserves_redaction(value in ".*", is_secret in prop::bool::ANY) {
            let original = MaybeRedacted::new(&value, is_secret);
            let cloned = original.clone();

            prop_assert_eq!(original.is_redacted(), cloned.is_redacted());
            prop_assert_eq!(original.as_str(), cloned.as_str());
        }
    }
}

// ============================================================================
// Numeric Conversion Properties
// ============================================================================

mod numeric_properties {
    use super::*;
    use procenv::ConfigValue;

    proptest! {
        /// i32 values fit in i64 container
        #[test]
        fn i32_fits_in_i64(n in prop::num::i32::ANY) {
            let value = ConfigValue::Integer(n as i64);
            let result = value.to_i32();
            prop_assert_eq!(result, Some(n));
        }

        /// u32 values fit in u64 container
        #[test]
        fn u32_fits_in_u64(n in prop::num::u32::ANY) {
            let value = ConfigValue::UnsignedInteger(n as u64);
            let result = value.to_u32();
            prop_assert_eq!(result, Some(n));
        }

        /// Overflow detection: large u64 doesn't fit in i32
        #[test]
        fn overflow_detected(n in (i32::MAX as u64 + 1)..=u64::MAX) {
            let value = ConfigValue::UnsignedInteger(n);
            let result = value.to_i32();
            prop_assert!(result.is_none(),
                "{} should not fit in i32, got {:?}", n, result);
        }

        /// Boolean to numeric conversion
        #[test]
        fn bool_to_numeric(b in prop::bool::ANY) {
            let value = ConfigValue::Boolean(b);
            let expected = if b { 1 } else { 0 };

            prop_assert_eq!(value.to_i64(), Some(expected));
            prop_assert_eq!(value.to_u64(), Some(expected as u64));
        }
    }
}

// ============================================================================
// Error Type Properties
// ============================================================================

mod error_properties {
    use super::*;
    use procenv::Error;

    proptest! {
        /// Error::parse with secret=true never stores the value
        /// Uses secrets with minimum length of 8 chars to avoid false positives
        #[test]
        fn parse_error_redacts_secrets(
            var in "[A-Z][A-Z0-9_]{2,10}",
            value in "[a-zA-Z0-9!@#$%^&*]{8,32}",
            type_name in "[a-z]{4,10}",
        ) {
            let error = Error::parse(
                &var,
                &value,
                true, // secret
                &type_name,
                Box::new(std::io::Error::new(std::io::ErrorKind::Other, "test")),
            );

            // Check the error doesn't contain the secret value
            let error_str = format!("{}", error);
            let debug_str = format!("{:?}", error);

            prop_assert!(!error_str.contains(&value),
                "Secret leaked in Error Display");
            prop_assert!(!debug_str.contains(&value),
                "Secret leaked in Error Debug");
        }

        /// Error::missing generates valid help text
        #[test]
        fn missing_error_has_help(var in "[A-Z][A-Z0-9_]*") {
            let error = Error::missing(&var);
            let error_str = format!("{}", error);

            // Should mention the variable name
            prop_assert!(error_str.contains(&var) || error_str.to_lowercase().contains("missing"));
        }
    }
}

// ============================================================================
// ConfigValue Edge Cases
// ============================================================================

mod config_value_edge_cases {
    use super::*;
    use procenv::ConfigValue;

    proptest! {
        /// ConfigValue::from correctly handles all numeric types
        #[test]
        fn from_i8_works(n in prop::num::i8::ANY) {
            let value: ConfigValue = n.into();
            prop_assert!(matches!(value, ConfigValue::Integer(_)));
        }

        #[test]
        fn from_u8_works(n in prop::num::u8::ANY) {
            let value: ConfigValue = n.into();
            prop_assert!(matches!(value, ConfigValue::UnsignedInteger(_)));
        }

        #[test]
        fn from_f32_works(f in prop::num::f32::NORMAL) {
            let value: ConfigValue = f.into();
            prop_assert!(matches!(value, ConfigValue::Float(_)));
        }

        /// Clone produces identical values
        #[test]
        fn clone_is_identical(s in ".*") {
            let value = ConfigValue::from_str_infer(&s);
            let cloned = value.clone();
            prop_assert_eq!(value, cloned);
        }

        /// into_string produces valid string representation
        #[test]
        fn into_string_works(s in ".*") {
            let value = ConfigValue::from_str_infer(&s);
            let string = value.clone().into_string();
            // Should be a valid string (not panic)
            let _ = string.len(); // Just verify it doesn't panic
        }

        /// type_name returns valid type names
        #[test]
        fn type_name_is_valid(s in ".*") {
            let value = ConfigValue::from_str_infer(&s);
            let type_name = value.type_name();
            let valid_types = ["string", "integer", "unsigned integer", "float", "boolean", "list", "map", "none"];
            prop_assert!(valid_types.contains(&type_name));
        }
    }

    /// Test boundary values for numeric conversions
    #[test]
    fn test_boundary_values() {
        // Test i64::MIN boundary
        let value = ConfigValue::Integer(i64::MIN);
        assert_eq!(value.to_i64(), Some(i64::MIN));
        assert_eq!(value.to_i32(), None); // Overflow

        // Test i64::MAX boundary
        let value = ConfigValue::Integer(i64::MAX);
        assert_eq!(value.to_i64(), Some(i64::MAX));
        assert_eq!(value.to_i32(), None); // Overflow

        // Test u64::MAX boundary
        let value = ConfigValue::UnsignedInteger(u64::MAX);
        assert_eq!(value.to_u64(), Some(u64::MAX));
        assert_eq!(value.to_i64(), None); // Overflow
    }
}

// ============================================================================
// ConfigValue Map/List Properties
// ============================================================================

mod config_value_collections {
    use super::*;
    use procenv::ConfigValue;

    proptest! {
        /// Vec<T> can be converted to ConfigValue::List
        #[test]
        fn vec_to_list(values in prop::collection::vec(prop::num::i32::ANY, 0..10)) {
            let config_value: ConfigValue = values.clone().into();
            prop_assert!(matches!(config_value, ConfigValue::List(_)));

            if let ConfigValue::List(list) = config_value {
                prop_assert_eq!(list.len(), values.len());
            }
        }

        /// Option<T> conversion works correctly
        #[test]
        fn option_conversion(value in prop::option::of(prop::num::i32::ANY)) {
            let config_value: ConfigValue = value.into();

            match value {
                Some(_) => prop_assert!(matches!(config_value, ConfigValue::Integer(_))),
                None => prop_assert!(matches!(config_value, ConfigValue::None)),
            }
        }

        /// None variant has correct properties
        #[test]
        fn none_properties(_x in 0..1i32) {
            let value = ConfigValue::None;
            prop_assert!(value.is_none());
            prop_assert!(!value.is_some());
            prop_assert_eq!(value.type_name(), "none");
        }
    }

    #[test]
    fn test_map_operations() {
        let mut map = HashMap::new();
        map.insert("key".to_string(), ConfigValue::from("value"));

        let value = ConfigValue::Map(map);
        assert!(value.as_map().is_some());
        assert_eq!(value.type_name(), "map");

        // get_path should work on maps
        let nested_map = {
            let mut inner = HashMap::new();
            inner.insert("inner_key".to_string(), ConfigValue::from("inner_value"));
            let mut outer = HashMap::new();
            outer.insert("outer".to_string(), ConfigValue::Map(inner));
            ConfigValue::Map(outer)
        };

        // Nested access
        assert!(nested_map.get_path("outer.inner_key").is_some());
        assert!(nested_map.get_path("nonexistent").is_none());
    }
}

// ============================================================================
// File Utils Properties (requires 'file' feature)
// ============================================================================

#[cfg(feature = "file")]
mod file_utils_properties {
    use super::*;
    use procenv::file::{FileFormat, FileUtils};

    proptest! {
        /// coerce_value never panics
        #[test]
        fn coerce_value_never_panics(s in ".*") {
            let _ = FileUtils::coerce_value(&s);
        }

        /// coerce_value is deterministic
        #[test]
        fn coerce_value_is_deterministic(s in ".*") {
            let first = FileUtils::coerce_value(&s);
            let second = FileUtils::coerce_value(&s);
            prop_assert_eq!(first, second);
        }

        /// Boolean coercion is case-insensitive
        #[test]
        fn bool_coercion_case_insensitive(mixed_case in "[tT][rR][uU][eE]|[fF][aA][lL][sS][eE]") {
            let value = FileUtils::coerce_value(&mixed_case);
            prop_assert!(value.is_boolean());
        }

        /// Integer strings coerce to numbers
        #[test]
        fn integer_coercion(n in prop::num::i64::ANY) {
            let s = n.to_string();
            let value = FileUtils::coerce_value(&s);
            prop_assert!(value.is_number());
        }

        /// parse_str with JSON never panics (may return error)
        #[test]
        fn json_parse_never_panics(s in ".*") {
            let _ = FileUtils::parse_str(&s, FileFormat::Json);
        }
    }

    #[test]
    fn test_deep_merge() {
        use serde_json::json;

        let mut base = json!({"a": 1, "b": {"c": 2}});
        let overlay = json!({"b": {"d": 3}, "e": 4});

        FileUtils::deep_merge(&mut base, overlay);

        // Check merged structure
        assert_eq!(base["a"], 1);
        assert_eq!(base["b"]["c"], 2); // preserved from base
        assert_eq!(base["b"]["d"], 3); // added from overlay
        assert_eq!(base["e"], 4); // added from overlay
    }
}

// ============================================================================
// String Parsing Edge Cases
// ============================================================================

mod string_edge_cases {
    use super::*;
    use procenv::ConfigValue;

    #[test]
    fn test_special_strings() {
        // Empty string
        let value = ConfigValue::from_str_infer("");
        assert!(matches!(value, ConfigValue::String(_)));

        // Whitespace only
        let value = ConfigValue::from_str_infer("   ");
        assert!(matches!(value, ConfigValue::String(_)));

        // Unicode strings
        let value = ConfigValue::from_str_infer("你好世界");
        assert!(matches!(value, ConfigValue::String(_)));

        // Emoji
        let value = ConfigValue::from_str_infer("🦀🔥");
        assert!(matches!(value, ConfigValue::String(_)));

        // Special float strings that shouldn't parse as floats
        let value = ConfigValue::from_str_infer("NaN");
        assert!(matches!(value, ConfigValue::String(_)));

        let value = ConfigValue::from_str_infer("Infinity");
        assert!(matches!(value, ConfigValue::String(_)));

        // Scientific notation
        let value = ConfigValue::from_str_infer("1e10");
        assert!(matches!(value, ConfigValue::Float(_)));

        // Leading zeros
        let value = ConfigValue::from_str_infer("007");
        assert!(matches!(value, ConfigValue::UnsignedInteger(7)));
    }

    proptest! {
        /// Unicode strings are handled correctly
        #[test]
        fn unicode_handling(s in "\\PC*") {
            let value = ConfigValue::from_str_infer(&s);
            // Should not panic and should produce valid output
            let _ = format!("{}", value);
            let _ = format!("{:?}", value);
        }

        /// Strings with special characters work
        #[test]
        fn special_chars(s in "[\\x00-\\x7F]*") {
            let value = ConfigValue::from_str_infer(&s);
            let _ = value.type_name();
        }
    }
}
