//! File error types with rich diagnostics.

use miette::{Diagnostic, NamedSource, SourceSpan};

/// Error type for file parsing operations with rich diagnostics.
///
/// This enum represents all file-related errors that can occur during
/// configuration loading. It integrates with [`miette`] to provide
/// beautiful terminal output with source code snippets and precise
/// error locations.
///
/// # Diagnostic Features
///
/// - **Source spans** - Points to the exact location of syntax errors
/// - **Help text** - Provides suggestions for fixing common issues
/// - **Error codes** - Unique codes for each error type
///
/// # Example Output
///
/// ```text
/// Error: TOML parse error in config.toml
///    ╭─[config.toml:5:12]
///    │
///  5 │ port = "8080"
///    │        ^^^^^^ expected integer
///    ╰────
///   help: check for missing quotes, invalid values, or syntax errors
/// ```
#[derive(Debug, Diagnostic, thiserror::Error)]
#[non_exhaustive]
pub enum FileError {
    /// Configuration file not found
    #[error("configuration file not found: {path}")]
    #[diagnostic(
        code(procenv::file::not_found),
        help("ensure the file exists at the specified path")
    )]
    NotFound {
        /// Path to the missing file
        path: String,
    },

    /// Failed to read file
    #[error("failed to read configuration file: {path}")]
    #[diagnostic(
        code(procenv::file::read_error),
        help("check file permissions and ensure it's readable")
    )]
    ReadError {
        /// Path to the file
        path: String,

        /// The underlying I/O error
        #[source]
        source: std::io::Error,
    },

    /// Unknown file format
    #[error("unknown configuration file format: .{extension}")]
    #[diagnostic(
        code(procenv::file::unknown_format),
        help("supported formats: .json, .toml, .yaml, .yml")
    )]
    UnknownFormat {
        /// The file extension that wasn't recognized
        extension: String,
    },

    /// Parse error with source location
    #[error("{format} parse error in {path}")]
    #[diagnostic(code(procenv::file::parse_error))]
    Parse {
        /// Format name (JSON, TOML, YAML/YML)
        format: &'static str,

        /// Path to the file
        path: String,

        /// The source file content for display
        #[source_code]
        src: NamedSource<String>,

        /// The location of the error
        #[label("{message}")]
        span: SourceSpan,

        /// Description of what went wrong
        message: String,

        /// Suggestion for how to fix
        #[help]
        help: String,
    },

    /// Parse error without source location (fallback)
    #[error("{format} parse error: {message}")]
    #[diagnostic(code(procenv::file::parse_error))]
    ParseNoSpan {
        /// Format name
        format: &'static str,

        /// Error message
        message: String,

        /// Suggestion for how to fix
        #[help]
        help: String,
    },

    /// Type mismatch error with source location
    #[error("type mismatch at `{path_str}` in {file_path}")]
    TypeMismatch {
        /// The JSON path where the error occurred (e.g., "database.port")
        path_str: String,

        /// The path to the file
        file_path: String,

        /// The source file content for display
        #[source_code]
        src: NamedSource<String>,

        /// The location of the error
        #[label("{message}")]
        span: SourceSpan,

        /// Description of what went wrong
        message: String,

        /// Suggestion for how to fix
        #[help]
        help: String,
    },
}
