//! Type-erased configuration values for runtime access.
//!
//! The [`ConfigValue`] enum provides a way to work with configuration values
//! without knowing their types at compile time. This enables:
//!
//! - Dynamic key-based access to configuration
//! - Partial loading without instantiating full config structs
//! - Runtime introspection of configuration values
//!
//! # Example
//!
//! ```rust, ignore
//! use procenv::collections::HashMap;
//!
//! let value = ConfigValue::Integer(8080);
//!
//! // Type-safe extraction
//! let port: i64 = value.as_i64.unwrap();
//!
//! // Parse to specific type
//! let port: u16 = value.parse().unwrap();
//! ```

use std::collections::HashMap;
use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

/// A type-erased configuration value.
///
/// This enum represents configuration values that can be accessed dynamically
/// without compile-time type information. It supports common configuration
/// value types and provides conversion methods.
///
/// # Supported Types
///
/// | Variant | Rust Types |
/// |---------|------------|
/// | `String` | `String` , `&str`|
/// | `Integer` | `i8`, `i16`, `i32`, `i64`, `isize` |
/// | `UnsignedInteger` | `u8`, `u16`, `u32`, `u64`, `usize` |
/// | `Float` | `f32`, `f64` |
/// | `Boolean` | `bool` |
/// | `List` | `Vec<T>` |
/// | `Map` | `HashMap<String, T>` |
///
/// # Example
/// ```rust, ignore
/// use procenv::ConfigValue;
///
/// // From environment or provider
/// let value = ConfigValue::from_str_value("8080");
///
/// // Access Methods
/// assert_eq!(value.as_str(), Some("8080"));
/// assert_eq!(value.as_i64(), Some(8080));
/// assert_eq!(value.parse::<u16>().unwrap(), 8080);
/// ```
#[derive(Clone, Debug, PartialEq)]
pub enum ConfigValue {
    /// A string value.
    String(String),

    /// A signed integer value (stored as i64 for maximum range).
    Integer(i64),

    /// An unsigned integer value (stored as u64 for maximum range).
    UnsignedInteger(u64),

    /// A floating-point value (stored as f64 for maximum precision).
    Float(f64),

    /// A boolean value.
    Boolean(bool),

    /// A list of values (for array/vector configuration).
    List(Vec<ConfigValue>),

    /// A map of string keys to values (for nested configurations).
    Map(HashMap<String, ConfigValue>),

    /// No value (represents missing optional values).
    None,
}

impl ConfigValue {
    /// Creates a `ConfigValue` from a raw string, attempting to infer the type.
    ///
    /// Type inference order:
    /// 1. Boolean (`true`/`false`)
    /// 2. Unsigned integer (if positive and fits in u64)
    /// 3. Signed integer (if fits in i64)
    /// 4. Float (if contains `.` or `e`/`E`)
    /// 5. String (fallback)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use procenv::ConfigValue;
    ///
    /// assert!(matches!(ConfigValue::from_str_infer("true")), ConfigValue::Boolean(true));
    /// assert!(matches!(ConfigValue::from_str_infer("42")), ConfigValue::UnsignedInteger(42));
    /// assert!(matches!(ConfigValue::from_str_infer("-5")))
    /// ```
    pub fn from_str_infer(s: &str) -> Self {
        todo!()
    }
}
