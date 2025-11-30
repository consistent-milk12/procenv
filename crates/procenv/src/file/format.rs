//! File format detection and representation.
//!
//! This module provides the [`FileFormat`] enum for representing supported
//! configuration file formats and automatic format detection from file paths.
//!
//! # Supported Formats
//!
//! | Format | Feature | Extensions |
//! |--------|---------|------------|
//! | JSON | `file` (always) | `.json` |
//! | TOML | `toml` | `.toml` |
//! | YAML | `yaml` | `.yaml`, `.yml` |
//!
//! # Format Detection
//!
//! The format is automatically detected from the file extension when using
//! [`FileUtils::parse_file`](super::FileUtils::parse_file) or
//! [`ConfigBuilder::file`](super::ConfigBuilder::file).
//!
//! ```rust,ignore
//! use procenv::file::FileFormat;
//! use std::path::Path;
//!
//! // Automatic detection
//! assert_eq!(FileFormat::from_path(Path::new("config.json")), Some(FileFormat::Json));
//! assert_eq!(FileFormat::from_path(Path::new("config.toml")), Some(FileFormat::Toml));
//! assert_eq!(FileFormat::from_path(Path::new("config.yaml")), Some(FileFormat::Yaml));
//! assert_eq!(FileFormat::from_path(Path::new("config.txt")), None);
//! ```

use std::path::Path;

/// Supported configuration file formats.
///
/// The format is automatically detected from the file extension when using
/// [`FileUtils::parse_file`](crate::file::FileUtils::parse_file) or
/// [`ConfigBuilder::file`](crate::file::ConfigBuilder::file). You can also
/// explicitly specify a format with [`FileUtils::parse_str`](crate::file::FileUtils::parse_str).
///
/// # Feature Flags
///
/// - `file` - Enables JSON support (always included)
/// - `toml` - Enables TOML support
/// - `yaml` - Enables YAML support
///
/// # Example
///
/// ```rust,ignore
/// use procenv::file::FileFormat;
/// use std::path::Path;
///
/// // Automatic detection from path
/// assert_eq!(FileFormat::from_path(Path::new("config.json")), Some(FileFormat::Json));
/// assert_eq!(FileFormat::from_path(Path::new("config.toml")), Some(FileFormat::Toml));
/// assert_eq!(FileFormat::from_path(Path::new("config.yaml")), Some(FileFormat::Yaml));
/// ```
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileFormat {
    /// JSON format (`.json` extension).
    ///
    /// Always available with the `file` feature.
    Json,

    /// TOML format (`.toml` extension).
    ///
    /// Requires the `toml` feature flag.
    #[cfg(feature = "toml")]
    Toml,

    /// YAML format (`.yaml` or `.yml` extension).
    ///
    /// Requires the `yaml` feature flag.
    #[cfg(feature = "yaml")]
    Yaml,
}

impl FileFormat {
    /// Detects the file format from the file extension.
    ///
    /// Returns `None` if the extension is not recognized or if the
    /// required feature flag is not enabled.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use procenv::file::FileFormat;
    /// use std::path::Path;
    ///
    /// let format = FileFormat::from_path(Path::new("config.toml"));
    /// assert_eq!(format, Some(FileFormat::Toml));
    /// ```
    pub fn from_path(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?;

        match ext.to_lowercase().as_str() {
            "json" => Some(FileFormat::Json),

            #[cfg(feature = "toml")]
            "toml" => Some(FileFormat::Toml),

            #[cfg(feature = "yaml")]
            "yaml" | "yml" => Some(FileFormat::Yaml),

            _ => None,
        }
    }

    /// Get the format name for error messages.
    pub fn name(&self) -> &'static str {
        match self {
            FileFormat::Json => "JSON",

            #[cfg(feature = "toml")]
            FileFormat::Toml => "TOML",

            #[cfg(feature = "yaml")]
            FileFormat::Yaml => "YAML",
        }
    }
}
