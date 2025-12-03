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
//! ```rust,ignore
//! use procenv::ConfigValue;
//!
//! let value = ConfigValue::Integer(8080);
//! let port: i64 = value.to_i64().unwrap();
//! let port: u16 = value.cast().unwrap();
//! ```

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

use num_traits::{NumCast, ToPrimitive};

#[cfg(feature = "file")]
use serde_json as SJSON;

// ============================================================================
// Macros for reducing boilerplate
// ============================================================================

/// Generates `From<T>` implementations for `ConfigValue` (signed integers).
macro_rules! impl_from_signed {
    ($($t:ty),+ $(,)?) => {
        $(
            impl From<$t> for ConfigValue {
                fn from(n: $t) -> Self {
                    ConfigValue::Integer(<i64 as From<$t>>::from(n))
                }
            }
        )+
    };
}

/// Generates `From<T>` implementations for `ConfigValue` (unsigned integers).
macro_rules! impl_from_unsigned {
    ($($t:ty),+ $(,)?) => {
        $(
            impl From<$t> for ConfigValue {
                fn from(n: $t) -> Self {
                    ConfigValue::UnsignedInteger(<u64 as From<$t>>::from(n))
                }
            }
        )+
    };
}

/// Generates `to_*` methods that use `ToPrimitive`.
macro_rules! impl_to_primitive {
    ($($method:ident -> $t:ty),+ $(,)?) => {
        $(
            #[doc = concat!("Converts to `", stringify!($t), "` if possible.")]
            pub fn $method(&self) -> Option<$t> {
                match self {
                    ConfigValue::Integer(n) => n.$method(),
                    ConfigValue::UnsignedInteger(n) => n.$method(),
                    ConfigValue::Float(f) => f.$method(),
                    ConfigValue::String(s) => s.parse().ok(),
                    ConfigValue::Boolean(b) => <i32 as From<bool>>::from(*b).$method(),
                    _ => None,
                }
            }
        )+
    };
}

// ============================================================================
// ConfigValue Enum
// ============================================================================

/// A type-erased configuration value.
///
/// Supports common configuration value types with automatic conversions
/// via the `num-traits` crate.
///
/// # Supported Types
///
/// | Variant | Rust Types |
/// |---------|------------|
/// | `String` | `String`, `&str` |
/// | `Integer` | `i8` - `i64`, `isize` |
/// | `UnsignedInteger` | `u8` - `u64`, `usize` |
/// | `Float` | `f32`, `f64` |
/// | `Boolean` | `bool` |
/// | `List` | `Vec<ConfigValue>` |
/// | `Map` | `HashMap<String, ConfigValue>` |
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ConfigValue {
    /// A string value.
    String(String),
    /// A signed integer (stored as i64).
    Integer(i64),
    /// An unsigned integer (stored as u64).
    UnsignedInteger(u64),
    /// A floating-point value (stored as f64).
    Float(f64),
    /// A boolean value.
    Boolean(bool),
    /// A list of values.
    List(Vec<ConfigValue>),
    /// A map of string keys to values.
    Map(HashMap<String, ConfigValue>),
    /// No value (missing optional).
    None,
}

// ============================================================================
// Constructors
// ============================================================================

impl ConfigValue {
    /// Creates from a string with automatic type inference.
    ///
    /// Inference order: bool -> unsigned int -> signed int -> float -> string
    #[must_use]
    pub fn from_str_infer(s: &str) -> Self {
        // Boolean
        match s.to_ascii_lowercase().as_str() {
            "true" => return ConfigValue::Boolean(true),
            "false" => return ConfigValue::Boolean(false),
            _ => {}
        }

        // Unsigned integer (positive numbers)
        if let Ok(n) = s.parse::<u64>() {
            return ConfigValue::UnsignedInteger(n);
        }

        // Signed integer (negative numbers)
        if let Ok(n) = s.parse::<i64>() {
            return ConfigValue::Integer(n);
        }

        // Float (contains decimal or exponent)
        if (s.contains('.') || s.contains('e') || s.contains('E'))
            && let Ok(f) = s.parse::<f64>()
        {
            return ConfigValue::Float(f);
        }

        // Default: string
        ConfigValue::String(s.to_string())
    }

    /// Creates a String variant without type inference.
    pub fn from_str_value(s: impl Into<String>) -> Self {
        ConfigValue::String(s.into())
    }
}

// ============================================================================
// Accessor Methods
// ============================================================================

impl ConfigValue {
    /// Returns as string reference if String variant.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            ConfigValue::String(s) => Some(s),
            _ => None,
        }
    }

    /// Returns as bool, parsing strings "true"/"false"/"1"/"0".
    #[must_use]
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            ConfigValue::Boolean(b) => Some(*b),
            ConfigValue::String(s) => match s.to_ascii_lowercase().as_str() {
                "true" | "1" | "yes" | "on" => Some(true),
                "false" | "0" | "no" | "off" => Some(false),
                _ => None,
            },
            ConfigValue::Integer(n) => Some(*n != 0),
            ConfigValue::UnsignedInteger(n) => Some(*n != 0),
            _ => None,
        }
    }

    /// Returns as list reference if List variant.
    #[must_use]
    pub fn as_list(&self) -> Option<&[ConfigValue]> {
        match self {
            ConfigValue::List(v) => Some(v),
            _ => None,
        }
    }

    /// Returns as map reference if Map variant.
    #[must_use]
    pub fn as_map(&self) -> Option<&HashMap<String, ConfigValue>> {
        match self {
            ConfigValue::Map(m) => Some(m),
            _ => None,
        }
    }

    /// Checks if None variant.
    #[must_use]
    pub fn is_none(&self) -> bool {
        matches!(self, ConfigValue::None)
    }

    /// Checks if not None.
    #[must_use]
    pub fn is_some(&self) -> bool {
        !self.is_none()
    }

    /// Returns the type name of this value.
    #[must_use]
    pub fn type_name(&self) -> &'static str {
        match self {
            ConfigValue::String(_) => "string",
            ConfigValue::Integer(_) => "integer",
            ConfigValue::UnsignedInteger(_) => "unsigned integer",
            ConfigValue::Float(_) => "float",
            ConfigValue::Boolean(_) => "boolean",
            ConfigValue::List(_) => "list",
            ConfigValue::Map(_) => "map",
            ConfigValue::None => "none",
        }
    }

    // Generate to_* methods using macro + ToPrimitive
    impl_to_primitive! {
        to_i8 -> i8,
        to_i16 -> i16,
        to_i32 -> i32,
        to_i64 -> i64,
        to_isize -> isize,
        to_u8 -> u8,
        to_u16 -> u16,
        to_u32 -> u32,
        to_u64 -> u64,
        to_usize -> usize,
        to_f32 -> f32,
        to_f64 -> f64,
    }
}

// ============================================================================
// Conversion Methods
// ============================================================================

impl ConfigValue {
    /// Casts to any numeric type using `NumCast`.
    ///
    /// Works for all primitive numeric types (i8-i64, u8-u64, f32, f64).
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let value = ConfigValue::Integer(8080);
    /// let port: u16 = value.cast().unwrap();
    /// ```
    pub fn cast<T: NumCast>(&self) -> Option<T> {
        match self {
            ConfigValue::Integer(n) => NumCast::from(*n),
            ConfigValue::UnsignedInteger(n) => NumCast::from(*n),
            ConfigValue::Float(f) => NumCast::from(*f),
            ConfigValue::String(s) => s.parse::<f64>().ok().and_then(NumCast::from),
            ConfigValue::Boolean(b) => NumCast::from(<i64 as From<bool>>::from(*b)),
            _ => None,
        }
    }

    /// Parses to any type implementing `FromStr`.
    ///
    /// First converts to string representation, then parses.
    ///
    /// # Errors
    ///
    /// Returns the parse error from `T::FromStr` if the string representation
    /// cannot be parsed into the target type.
    pub fn parse<T: FromStr>(&self) -> Result<T, T::Err> {
        self.to_string_repr().parse()
    }

    /// Converts to owned String representation.
    #[must_use]
    pub fn into_string(self) -> String {
        match self {
            ConfigValue::String(s) => s,
            ConfigValue::Integer(n) => n.to_string(),
            ConfigValue::UnsignedInteger(n) => n.to_string(),
            ConfigValue::Float(f) => f.to_string(),
            ConfigValue::Boolean(b) => b.to_string(),
            ConfigValue::List(v) => {
                let items: Vec<_> = v.into_iter().map(ConfigValue::into_string).collect();
                format!("[{}]", items.join(", "))
            }
            ConfigValue::Map(m) => {
                let items: Vec<_> = m
                    .into_iter()
                    .map(|(k, v)| format!("{}: {}", k, v.into_string()))
                    .collect();
                format!("{{{}}}", items.join(", "))
            }
            ConfigValue::None => String::new(),
        }
    }

    /// Returns string representation (borrowed when possible).
    ///
    /// Uses `Display` formatting for non-String variants to avoid cloning.
    fn to_string_repr(&self) -> Cow<'_, str> {
        use std::borrow::Cow;

        match self {
            ConfigValue::String(s) => Cow::Borrowed(s),

            // Use Display trait instead of cloning + into_string()
            other => Cow::Owned(other.to_string()),
        }
    }
}

// ============================================================================
// Path-based Access
// ============================================================================

impl ConfigValue {
    /// Gets nested value by dotted path (e.g., `"database.host"`).
    #[must_use]
    pub fn get_path(&self, path: &str) -> Option<&ConfigValue> {
        let mut current = self;

        for key in path.split('.') {
            match current {
                ConfigValue::Map(m) => {
                    current = m.get(key)?;
                }

                _ => return None,
            }
        }

        Some(current)
    }

    /// Gets mutable nested value by dotted path.
    pub fn get_path_mut(&mut self, path: &str) -> Option<&mut ConfigValue> {
        let mut current = self;
        for key in path.split('.') {
            match current {
                ConfigValue::Map(m) => {
                    current = m.get_mut(key)?;
                }
                _ => return None,
            }
        }
        Some(current)
    }
}

// ============================================================================
// From Implementations (using macro)
// ============================================================================

impl From<String> for ConfigValue {
    fn from(s: String) -> Self {
        ConfigValue::String(s)
    }
}

impl From<&str> for ConfigValue {
    fn from(s: &str) -> Self {
        ConfigValue::String(s.to_string())
    }
}

impl From<bool> for ConfigValue {
    fn from(b: bool) -> Self {
        ConfigValue::Boolean(b)
    }
}

impl From<f32> for ConfigValue {
    fn from(f: f32) -> Self {
        ConfigValue::Float(<f64 as From<f32>>::from(f))
    }
}

impl From<f64> for ConfigValue {
    fn from(f: f64) -> Self {
        ConfigValue::Float(f)
    }
}

// Generate From impls for integer types (using From trait for lossless conversion)
impl_from_signed!(i8, i16, i32);
impl_from_unsigned!(u8, u16, u32);

// i64/u64/isize/usize need manual impls since From<isize/usize> -> i64/u64 doesn't exist
impl From<i64> for ConfigValue {
    fn from(n: i64) -> Self {
        ConfigValue::Integer(n)
    }
}

impl From<u64> for ConfigValue {
    fn from(n: u64) -> Self {
        ConfigValue::UnsignedInteger(n)
    }
}

#[allow(clippy::cast_possible_wrap)]
impl From<isize> for ConfigValue {
    fn from(n: isize) -> Self {
        ConfigValue::Integer(n as i64)
    }
}

#[allow(clippy::cast_possible_truncation)]
impl From<usize> for ConfigValue {
    fn from(n: usize) -> Self {
        ConfigValue::UnsignedInteger(n as u64)
    }
}

impl<T: Into<ConfigValue>> From<Vec<T>> for ConfigValue {
    fn from(v: Vec<T>) -> Self {
        ConfigValue::List(v.into_iter().map(Into::into).collect())
    }
}

impl<T: Into<ConfigValue>> From<Option<T>> for ConfigValue {
    fn from(opt: Option<T>) -> Self {
        match opt {
            Some(v) => v.into(),
            None => ConfigValue::None,
        }
    }
}

// ============================================================================
// JSON Value Extraction (for macro-generated code)
// ============================================================================

impl ConfigValue {
    /// Creates a `ConfigValue` from a `serde_json::Value`.
    ///
    /// This is the bridge between file-loaded JSON and our type safe extraction.
    /// Used by macro generated `__from_json_value()` methods.
    #[cfg(feature = "file")]
    #[must_use]
    pub fn from_json(value: SJSON::Value) -> Self {
        match value {
            SJSON::Value::Null => ConfigValue::None,

            SJSON::Value::Bool(b) => ConfigValue::Boolean(b),

            SJSON::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    ConfigValue::Integer(i)
                } else if let Some(u) = n.as_u64() {
                    ConfigValue::UnsignedInteger(u)
                } else if let Some(f) = n.as_f64() {
                    ConfigValue::Float(f)
                } else {
                    // Fallback: Convert to String.
                    ConfigValue::String(n.to_string())
                }
            }

            SJSON::Value::String(s) => ConfigValue::String(s),

            SJSON::Value::Array(arr) => {
                ConfigValue::List(arr.into_iter().map(Self::from_json).collect())
            }

            SJSON::Value::Object(map) => ConfigValue::Map(
                map.into_iter()
                    .map(|(k, v)| (k, Self::from_json(v)))
                    .collect(),
            ),
        }
    }

    /// Extracts and parses a value to type `T` using `FromStr`.
    ///
    /// This is the primary extraction method used by macro-generated code.
    /// It handles type coercion and produces appropriate error messages.
    ///
    /// # Type Parameters
    ///
    /// * `T` - The target type (must implement `FromStr`)
    ///
    /// # Errors
    ///
    /// Returns a boxed error if parsing fails.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let value = ConfigValue::from_json(json!(8080));
    /// let port: u16 = value.extract("port")?;
    /// ```
    pub fn extract<T: FromStr>(
        &self,
        field_name: &str,
    ) -> Result<T, Box<dyn std::error::Error + Send + Sync>>
    where
        T::Err: std::error::Error + Send + Sync + 'static,
    {
        let string_repr: Cow<'_, str> = self.to_string_repr();

        string_repr.parse::<T>().map_err(|e| {
            let err: Box<dyn std::error::Error + Send + Sync> = Box::new(e);

            err
        })
    }

    /// Extracts a boolean value with flexible parsing.
    ///
    /// Accepts: `true`, `false`, `1`, `0`, `"true"`, `"false"`, `"yes"`, `"no"`, etc.
    #[must_use]
    pub fn extract_bool(&self) -> Option<bool> {
        self.as_bool()
    }

    /// Extracts a String value.
    ///
    /// For non-string types, converts to string representation.
    #[must_use]
    pub fn extract_string(&self) -> String {
        match self {
            ConfigValue::String(s) => s.clone(),
            other => other.to_string(),
        }
    }

    /// Checks if this value represents "not set" (null/none).
    #[must_use]
    pub fn is_null(&self) -> bool {
        matches!(self, ConfigValue::None)
    }
}

// ============================================================================
// Display
// ============================================================================

impl Display for ConfigValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            ConfigValue::String(s) => write!(f, "{s}"),
            ConfigValue::Integer(n) => write!(f, "{n}"),
            ConfigValue::UnsignedInteger(n) => write!(f, "{n}"),
            ConfigValue::Float(n) => write!(f, "{n}"),
            ConfigValue::Boolean(b) => write!(f, "{b}"),
            ConfigValue::List(v) => {
                write!(f, "[")?;
                for (i, item) in v.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{item}")?;
                }
                write!(f, "]")
            }
            ConfigValue::Map(m) => {
                write!(f, "{{")?;
                // Sort keys for deterministic output
                let mut keys: Vec<_> = m.keys().collect();
                keys.sort();
                for (i, k) in keys.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    // SAFETY: We're iterating over keys from this map, so get() is guaranteed Some
                    if let Some(v) = m.get(*k) {
                        write!(f, "{k}: {v}")?;
                    }
                }
                write!(f, "}}")
            }
            ConfigValue::None => write!(f, "<none>"),
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_str_infer() {
        assert_eq!(
            ConfigValue::from_str_infer("true"),
            ConfigValue::Boolean(true)
        );
        assert_eq!(
            ConfigValue::from_str_infer("FALSE"),
            ConfigValue::Boolean(false)
        );
        assert_eq!(
            ConfigValue::from_str_infer("42"),
            ConfigValue::UnsignedInteger(42)
        );
        assert_eq!(ConfigValue::from_str_infer("-5"), ConfigValue::Integer(-5));
        assert!(matches!(
            ConfigValue::from_str_infer("3.14"),
            ConfigValue::Float(_)
        ));
        assert!(matches!(
            ConfigValue::from_str_infer("hello"),
            ConfigValue::String(_)
        ));
    }

    #[test]
    fn test_numeric_conversions() {
        let val = ConfigValue::Integer(8080);
        assert_eq!(val.to_u16(), Some(8080u16));
        assert_eq!(val.to_i32(), Some(8080i32));
        assert_eq!(val.cast::<u16>(), Some(8080u16));

        let val = ConfigValue::UnsignedInteger(255);
        assert_eq!(val.to_u8(), Some(255u8));
        assert_eq!(val.to_i16(), Some(255i16));

        let val = ConfigValue::Float(3.7);
        assert_eq!(val.to_i32(), Some(3i32));
        assert_eq!(val.to_f32(), Some(3.7f32));
    }

    #[test]
    fn test_overflow_returns_none() {
        let val = ConfigValue::Integer(i64::MAX);
        assert_eq!(val.to_i8(), None); // Overflow
        assert_eq!(val.to_i64(), Some(i64::MAX)); // Fits

        let val = ConfigValue::Integer(-1);
        assert_eq!(val.to_u64(), None); // Negative to unsigned
    }

    #[test]
    fn test_cast_generic() {
        let val = ConfigValue::String("42".to_string());
        let n: Option<u16> = val.cast();
        assert_eq!(n, Some(42u16));

        let val = ConfigValue::Boolean(true);
        let n: Option<i32> = val.cast();
        assert_eq!(n, Some(1i32));
    }

    #[test]
    fn test_as_bool_extended() {
        assert_eq!(ConfigValue::String("yes".to_string()).as_bool(), Some(true));
        assert_eq!(ConfigValue::String("no".to_string()).as_bool(), Some(false));
        assert_eq!(ConfigValue::String("on".to_string()).as_bool(), Some(true));
        assert_eq!(
            ConfigValue::String("off".to_string()).as_bool(),
            Some(false)
        );
        assert_eq!(ConfigValue::String("invalid".to_string()).as_bool(), None);
    }

    #[test]
    fn test_path_access() {
        let mut db = HashMap::new();
        db.insert("host".to_string(), ConfigValue::from("localhost"));
        db.insert("port".to_string(), ConfigValue::from(5432u16));

        let mut root = HashMap::new();
        root.insert("database".to_string(), ConfigValue::Map(db));

        let config = ConfigValue::Map(root);

        assert_eq!(
            config.get_path("database.host").and_then(|v| v.as_str()),
            Some("localhost")
        );
        assert_eq!(
            config
                .get_path("database.port")
                .and_then(super::ConfigValue::to_u16),
            Some(5432)
        );
        assert!(config.get_path("nonexistent").is_none());
    }

    #[test]
    fn test_from_impls() {
        let _: ConfigValue = 42i8.into();
        let _: ConfigValue = 42u32.into();
        let _: ConfigValue = 2.5f32.into();
        let _: ConfigValue = "hello".into();
        let _: ConfigValue = true.into();
        let _: ConfigValue = vec![1i32, 2, 3].into();
        let _: ConfigValue = None::<i32>.into();
    }

    #[test]
    fn test_parse() {
        let val = ConfigValue::String("8080".to_string());
        assert_eq!(val.parse::<u16>().unwrap(), 8080);

        let val = ConfigValue::UnsignedInteger(443);
        assert_eq!(val.parse::<u16>().unwrap(), 443);
    }
}
