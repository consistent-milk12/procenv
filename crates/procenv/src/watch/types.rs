//! Core types for hot reload functionality.
//!
//! This module contains the fundamental types used throughout the watch system:
//! - [`WatchError`] - Errors specific to file watching and reloading
//! - [`ConfigChange`] - Represents a configuration change event
//! - [`ChangeTrigger`] - What caused the configuration to reload

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use miette::Diagnostic;
use thiserror::Error;

use crate::{ConfigSources, Source};

/// Error type for watch and hot reload operations.
///
/// All watch-related errors are reported through this type, which integrates
/// with [`miette`] for rich terminal diagnostics.
#[derive(Debug, Error, Diagnostic)]
#[non_exhaustive]
pub enum WatchError {
    /// Failed to initialize the file watcher.
    #[error("failed to initialize file watcher: {message}")]
    #[diagnostic(
        code(procenv::watch::init_failed),
        help("Check that the file paths exist and are accessible")
    )]
    InitFailed {
        /// Human-readable error message.
        message: String,
        /// The underlying notify error, if available.
        #[source]
        source: Option<notify::Error>,
    },

    /// Failed to watch a specific path.
    #[error("failed to watch path '{path}': {message}")]
    #[diagnostic(
        code(procenv::watch::path_error),
        help("Ensure the path exists and you have read permissions")
    )]
    PathError {
        /// The path that could not be watched.
        path: PathBuf,
        /// Human-readable error message.
        message: String,
    },

    /// Configuration reload failed.
    ///
    /// When this error occurs, the previous valid configuration is retained.
    #[error("configuration reload failed: {message}")]
    #[diagnostic(
        code(procenv::watch::reload_failed),
        help(
            "Fix the configuration errors and save the file again. The previous valid configuration remains active."
        )
    )]
    ReloadFailed {
        /// Human-readable error message.
        message: String,
        /// The underlying configuration errors.
        #[related]
        errors: Vec<crate::Error>,
    },

    /// The watcher has been stopped.
    #[error("watcher has been stopped")]
    #[diagnostic(
        code(procenv::watch::stopped),
        help("Create a new watcher if you need to continue watching for changes")
    )]
    Stopped,

    /// A file was deleted while being watched.
    #[error("watched file was deleted: {path}")]
    #[diagnostic(
        code(procenv::watch::file_deleted),
        help("The configuration file was deleted. Recreate it to resume watching.")
    )]
    FileDeleted {
        /// The path of the deleted file.
        path: PathBuf,
    },

    /// Channel communication error.
    #[error("internal channel error: {message}")]
    #[diagnostic(code(procenv::watch::channel_error))]
    ChannelError {
        /// Human-readable error message.
        message: String,
    },
}

impl WatchError {
    /// Create a new `InitFailed` error.
    pub fn init_failed(message: impl Into<String>, source: Option<notify::Error>) -> Self {
        Self::InitFailed {
            message: message.into(),
            source,
        }
    }

    /// Create a new `PathError`.
    pub fn path_error(path: impl Into<PathBuf>, message: impl Into<String>) -> Self {
        Self::PathError {
            path: path.into(),
            message: message.into(),
        }
    }

    /// Create a new `ReloadFailed` error.
    pub fn reload_failed(message: impl Into<String>, errors: Vec<crate::Error>) -> Self {
        Self::ReloadFailed {
            message: message.into(),
            errors,
        }
    }

    /// Create a new `FileDeleted` error.
    pub fn file_deleted(path: impl Into<PathBuf>) -> Self {
        Self::FileDeleted { path: path.into() }
    }

    /// Create a new `ChannelError`.
    pub fn channel_error(message: impl Into<String>) -> Self {
        Self::ChannelError {
            message: message.into(),
        }
    }
}

/// Represents a configuration change event.
///
/// This struct is passed to change callbacks and contains information about
/// what changed, including the old and new configurations, which fields
/// changed, and what triggered the reload.
///
/// # Example
///
/// ```ignore
/// handle.on_change(|change: ConfigChange<MyConfig>| {
///     println!("Config reloaded!");
///     println!("Changed fields: {:?}", change.changed_fields);
///     if change.old.is_some() {
///         println!("This was a reload, not initial load");
///     }
/// });
/// ```
#[derive(Debug, Clone)]
pub struct ConfigChange<T> {
    /// Previous configuration, or `None` if this is the initial load.
    pub old: Option<Arc<T>>,

    /// New configuration after reload.
    pub new: Arc<T>,

    /// Names of fields that changed (empty for initial load).
    pub changed_fields: Vec<String>,

    /// What triggered this configuration change.
    pub trigger: ChangeTrigger,

    /// When the change was detected.
    pub timestamp: Instant,

    /// Source attribution for the new configuration.
    pub sources: ConfigSources,
}

impl<T> ConfigChange<T> {
    /// Create a new configuration change event.
    pub fn new(
        old: Option<Arc<T>>,
        new: Arc<T>,
        changed_fields: Vec<String>,
        trigger: ChangeTrigger,
        sources: ConfigSources,
    ) -> Self {
        Self {
            old,
            new,
            changed_fields,
            trigger,
            timestamp: Instant::now(),
            sources,
        }
    }

    /// Create a change event for initial configuration load.
    pub fn initial(config: Arc<T>, sources: ConfigSources) -> Self {
        Self::new(None, config, Vec::new(), ChangeTrigger::Initial, sources)
    }

    /// Returns `true` if this is the initial configuration load.
    #[must_use]
    pub fn is_initial(&self) -> bool {
        matches!(self.trigger, ChangeTrigger::Initial)
    }

    /// Returns `true` if any fields changed.
    #[must_use]
    pub fn has_changes(&self) -> bool {
        !self.changed_fields.is_empty()
    }

    /// Check if a specific field changed.
    #[must_use]
    pub fn field_changed(&self, field_name: &str) -> bool {
        self.changed_fields.iter().any(|f| f == field_name)
    }
}

/// What triggered a configuration reload.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ChangeTrigger {
    /// A watched file was modified.
    FileModified(PathBuf),

    /// A watched file was created (possibly recreated after deletion).
    FileCreated(PathBuf),

    /// A watched file was deleted.
    ///
    /// Note: The configuration is not reloaded on deletion; this trigger
    /// is informational only.
    FileDeleted(PathBuf),

    /// An environment variable changed (detected via polling).
    EnvVarChanged(String),

    /// Configuration was manually reloaded via `reload()`.
    ManualReload,

    /// Initial configuration load.
    Initial,
}

impl ChangeTrigger {
    /// Returns the file path if this trigger is file-related.
    #[must_use]
    pub fn file_path(&self) -> Option<&PathBuf> {
        match self {
            Self::FileModified(p) | Self::FileCreated(p) | Self::FileDeleted(p) => Some(p),
            _ => None,
        }
    }

    /// Returns the env var name if this trigger is env-related.
    #[must_use]
    pub fn env_var(&self) -> Option<&str> {
        match self {
            Self::EnvVarChanged(var) => Some(var),
            _ => None,
        }
    }

    /// Returns `true` if this is a file-related trigger.
    #[must_use]
    pub fn is_file_trigger(&self) -> bool {
        matches!(
            self,
            Self::FileModified(_) | Self::FileCreated(_) | Self::FileDeleted(_)
        )
    }
}

impl std::fmt::Display for ChangeTrigger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FileModified(p) => write!(f, "file modified: {}", p.display()),
            Self::FileCreated(p) => write!(f, "file created: {}", p.display()),
            Self::FileDeleted(p) => write!(f, "file deleted: {}", p.display()),
            Self::EnvVarChanged(var) => write!(f, "env var changed: {var}"),
            Self::ManualReload => write!(f, "manual reload"),
            Self::Initial => write!(f, "initial load"),
        }
    }
}

/// Information about a changed field.
///
/// Used for detailed change tracking when diffing configurations.
#[derive(Debug, Clone)]
pub struct ChangedField {
    /// Name of the field that changed.
    pub name: String,

    /// Previous value as string (respects secret masking).
    pub old_value: Option<String>,

    /// New value as string (respects secret masking).
    pub new_value: Option<String>,

    /// Source of the new value.
    pub source: Source,
}

impl ChangedField {
    /// Create a new changed field record.
    pub fn new(
        name: impl Into<String>,
        old_value: Option<String>,
        new_value: Option<String>,
        source: Source,
    ) -> Self {
        Self {
            name: name.into(),
            old_value,
            new_value,
            source,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_watch_error_display() {
        let err = WatchError::init_failed("test error", None);
        assert!(err.to_string().contains("test error"));

        let err = WatchError::path_error("/test/path", "permission denied");
        assert!(err.to_string().contains("/test/path"));

        let err = WatchError::reload_failed("config invalid", vec![]);
        assert!(err.to_string().contains("config invalid"));
    }

    #[test]
    fn test_change_trigger_display() {
        let trigger = ChangeTrigger::FileModified(PathBuf::from("config.toml"));
        assert!(trigger.to_string().contains("config.toml"));

        let trigger = ChangeTrigger::EnvVarChanged("PORT".to_string());
        assert!(trigger.to_string().contains("PORT"));

        let trigger = ChangeTrigger::ManualReload;
        assert!(trigger.to_string().contains("manual"));
    }

    #[test]
    fn test_change_trigger_helpers() {
        let trigger = ChangeTrigger::FileModified(PathBuf::from("test.toml"));
        assert!(trigger.is_file_trigger());
        assert!(trigger.file_path().is_some());
        assert!(trigger.env_var().is_none());

        let trigger = ChangeTrigger::EnvVarChanged("TEST".to_string());
        assert!(!trigger.is_file_trigger());
        assert!(trigger.file_path().is_none());
        assert_eq!(trigger.env_var(), Some("TEST"));
    }
}
