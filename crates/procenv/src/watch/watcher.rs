//! Internal file watcher implementation.
//!
//! This module contains the [`ConfigWatcher`] which manages file system
//! events using the `notify` crate and triggers configuration reloads.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender, bounded, select};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use super::WatchedConfig;
use super::types::{ChangeTrigger, ConfigChange, WatchError};
use crate::{ConfigSources, Error};

/// Commands sent to the watcher thread.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum WatchCommand {
    /// Request a manual reload.
    Reload,
    /// Stop the watcher.
    Stop,
}

/// Internal watcher state shared between threads.
pub(crate) struct WatcherState<T> {
    /// The watched configuration container.
    pub config: Arc<WatchedConfig<T>>,
    /// Files being watched.
    pub watched_paths: Vec<PathBuf>,
    /// Whether the watcher is running.
    pub running: AtomicBool,
}

impl<T> WatcherState<T> {
    pub fn new(config: Arc<WatchedConfig<T>>, watched_paths: Vec<PathBuf>) -> Self {
        Self {
            config,
            watched_paths,
            running: AtomicBool::new(true),
        }
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::Release);
    }
}

/// Result from a reload attempt.
pub(crate) enum ReloadResult<T> {
    /// Reload succeeded with new config.
    Success(ConfigChange<T>),
    /// Reload failed, old config retained.
    Failed(WatchError),
    /// No reload needed (file not relevant).
    Skipped,
}

/// Configuration for the internal watcher.
pub(crate) struct WatcherConfig {
    /// Debounce duration for file events.
    pub debounce: Duration,
    /// Paths to watch.
    pub paths: Vec<PathBuf>,
}

impl Default for WatcherConfig {
    fn default() -> Self {
        Self {
            debounce: Duration::from_millis(100),
            paths: Vec::new(),
        }
    }
}

/// Internal file watcher that manages notify events and reloads.
pub(crate) struct ConfigWatcher<T: Clone + Send + Sync + 'static> {
    /// Shared state.
    state: Arc<WatcherState<T>>,
    /// Command sender for controlling the watcher.
    command_tx: Sender<WatchCommand>,
    /// Receiver for change notifications.
    change_rx: Receiver<ConfigChange<T>>,
    /// Receiver for errors.
    error_rx: Receiver<WatchError>,
    /// Watcher thread handle.
    thread_handle: Option<JoinHandle<()>>,
}

impl<T: Clone + Send + Sync + 'static> ConfigWatcher<T> {
    /// Start the file watcher.
    ///
    /// # Arguments
    ///
    /// * `initial_config` - Initial configuration
    /// * `initial_sources` - Source attribution for initial config
    /// * `config` - Watcher configuration
    /// * `reload_fn` - Function to reload the configuration
    pub fn start<F>(
        initial_config: T,
        initial_sources: ConfigSources,
        watcher_config: &WatcherConfig,
        reload_fn: F,
    ) -> Result<Self, WatchError>
    where
        F: Fn() -> Result<(T, ConfigSources), Error> + Send + Sync + 'static,
    {
        let config_container = Arc::new(WatchedConfig::new(initial_config, initial_sources));
        let state = Arc::new(WatcherState::new(
            config_container.clone(),
            watcher_config.paths.clone(),
        ));

        // Create channels
        let (command_tx, command_rx) = bounded::<WatchCommand>(16);
        let (change_tx, change_rx) = bounded::<ConfigChange<T>>(16);
        let (error_tx, error_rx) = bounded::<WatchError>(16);

        // Set up notify watcher
        let (notify_tx, notify_rx) = bounded::<notify::Result<Event>>(100);
        let mut watcher = create_notify_watcher(notify_tx)?;

        // Watch all configured paths
        for path in &watcher_config.paths {
            watch_path(&mut watcher, path)?;
        }

        // Spawn watcher thread
        let thread_state = state.clone();
        let debounce = watcher_config.debounce;

        // Store both original paths AND canonical paths (if file exists)
        // This allows matching newly created files that didn't exist at startup
        let watched_paths: HashSet<PathBuf> = watcher_config
            .paths
            .iter()
            .flat_map(|p| {
                let mut paths = vec![p.clone()];
                // Also store canonical path if file exists
                if let Ok(canonical) = p.canonicalize()
                    && canonical != *p
                {
                    paths.push(canonical);
                }
                paths
            })
            .collect();

        let thread_handle = thread::Builder::new()
            .name("procenv-watcher".to_string())
            .spawn(move || {
                watcher_loop(
                    thread_state,
                    command_rx,
                    notify_rx,
                    change_tx,
                    error_tx,
                    reload_fn,
                    debounce,
                    watched_paths,
                    watcher,
                );
            })
            .map_err(|e| {
                WatchError::init_failed(format!("failed to spawn watcher thread: {e}"), None)
            })?;

        Ok(Self {
            state,
            command_tx,
            change_rx,
            error_rx,
            thread_handle: Some(thread_handle),
        })
    }

    /// Get a reference to the watched configuration.
    pub fn config(&self) -> &Arc<WatchedConfig<T>> {
        &self.state.config
    }

    /// Request a manual reload.
    pub fn request_reload(&self) -> Result<(), WatchError> {
        if !self.state.is_running() {
            return Err(WatchError::Stopped);
        }
        self.command_tx
            .send(WatchCommand::Reload)
            .map_err(|_| WatchError::channel_error("failed to send reload command"))
    }

    /// Stop the watcher.
    pub fn stop(&self) {
        self.state.stop();
        let _ = self.command_tx.send(WatchCommand::Stop);
    }

    /// Get the change receiver.
    pub fn change_receiver(&self) -> &Receiver<ConfigChange<T>> {
        &self.change_rx
    }

    /// Get the error receiver.
    pub fn error_receiver(&self) -> &Receiver<WatchError> {
        &self.error_rx
    }

    /// Check if the watcher is still running.
    pub fn is_running(&self) -> bool {
        self.state.is_running()
    }

    /// Clone the command sender for external use.
    pub fn command_sender(&self) -> Sender<WatchCommand> {
        self.command_tx.clone()
    }
}

impl<T: Clone + Send + Sync + 'static> Drop for ConfigWatcher<T> {
    fn drop(&mut self) {
        self.stop();
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }
}

/// Create a notify watcher with the given event sender.
fn create_notify_watcher(
    tx: Sender<notify::Result<Event>>,
) -> Result<RecommendedWatcher, WatchError> {
    notify::recommended_watcher(move |res| {
        let _ = tx.send(res);
    })
    .map_err(|e| WatchError::init_failed(format!("failed to create file watcher: {e}"), Some(e)))
}

/// Watch a path with the notify watcher.
fn watch_path(watcher: &mut RecommendedWatcher, path: &Path) -> Result<(), WatchError> {
    // Watch the parent directory if the file doesn't exist yet
    let watch_target = if path.exists() {
        path.to_path_buf()
    } else if let Some(parent) = path.parent() {
        if parent.exists() {
            parent.to_path_buf()
        } else {
            return Err(WatchError::path_error(
                path,
                "neither file nor parent directory exists",
            ));
        }
    } else {
        return Err(WatchError::path_error(path, "invalid path"));
    };

    watcher
        .watch(&watch_target, RecursiveMode::NonRecursive)
        .map_err(|e| WatchError::path_error(path, format!("failed to watch: {e}")))
}

/// Main watcher loop running in a separate thread.
#[allow(clippy::too_many_arguments, clippy::needless_pass_by_value)]
fn watcher_loop<T, F>(
    state: Arc<WatcherState<T>>,
    command_rx: Receiver<WatchCommand>,
    notify_rx: Receiver<notify::Result<Event>>,
    change_tx: Sender<ConfigChange<T>>,
    error_tx: Sender<WatchError>,
    reload_fn: F,
    debounce: Duration,
    watched_paths: HashSet<PathBuf>,
    _watcher: RecommendedWatcher, // Keep watcher alive
) where
    T: Clone + Send + Sync + 'static,
    F: Fn() -> Result<(T, ConfigSources), Error> + Send + Sync + 'static,
{
    let mut pending_reload: Option<ChangeTrigger> = None;
    let mut last_event = std::time::Instant::now();

    while state.is_running() {
        select! {
            // Handle commands
            recv(command_rx) -> cmd => {
                match cmd {
                    Ok(WatchCommand::Reload) => {
                        do_reload(
                            &state,
                            &reload_fn,
                            ChangeTrigger::ManualReload,
                            &change_tx,
                            &error_tx,
                        );
                    }
                    Ok(WatchCommand::Stop) | Err(_) => {
                        state.stop();
                        break;
                    }
                }
            }

            // Handle file events
            recv(notify_rx) -> event_result => {
                if let Ok(Ok(event)) = event_result
                    && let Some(trigger) = process_notify_event(&event, &watched_paths)
                {
                    pending_reload = Some(trigger);
                    last_event = std::time::Instant::now();
                }
            }

            // Debounce timeout - process pending reload
            default(debounce) => {
                if let Some(trigger) = pending_reload.take() {
                    if last_event.elapsed() >= debounce {
                        do_reload(&state, &reload_fn, trigger, &change_tx, &error_tx);
                    } else {
                        // Not enough time passed, re-queue
                        pending_reload = Some(trigger);
                    }
                }
            }
        }
    }
}

/// Process a notify event and return a trigger if relevant.
fn process_notify_event(event: &Event, watched_paths: &HashSet<PathBuf>) -> Option<ChangeTrigger> {
    // Check if any of the event paths are in our watched set
    for path in &event.paths {
        // Check direct path match first (handles newly created files)
        let is_watched = watched_paths.contains(path)
            // Then try canonical match (handles existing files with symlinks/relative paths)
            || path.canonicalize().is_ok_and(|c| watched_paths.contains(&c));

        if is_watched {
            return match event.kind {
                EventKind::Create(_) => Some(ChangeTrigger::FileCreated(path.clone())),
                EventKind::Modify(_) => Some(ChangeTrigger::FileModified(path.clone())),
                EventKind::Remove(_) => Some(ChangeTrigger::FileDeleted(path.clone())),
                _ => None,
            };
        }
    }
    None
}

/// Perform a reload and send results to channels.
fn do_reload<T, F>(
    state: &Arc<WatcherState<T>>,
    reload_fn: &F,
    trigger: ChangeTrigger,
    change_tx: &Sender<ConfigChange<T>>,
    error_tx: &Sender<WatchError>,
) where
    T: Clone + Send + Sync + 'static,
    F: Fn() -> Result<(T, ConfigSources), Error>,
{
    match reload_fn() {
        Ok((new_config, new_sources)) => {
            let new_arc = Arc::new(new_config);
            let (old_config, _old_sources) =
                state.config.swap(new_arc.clone(), new_sources.clone());

            let change = ConfigChange::new(
                Some(old_config),
                new_arc,
                Vec::new(), // TODO: Implement field diffing
                trigger,
                new_sources,
            );

            let _ = change_tx.send(change);
        }
        Err(e) => {
            let watch_err = WatchError::reload_failed(e.to_string(), vec![e]);
            let _ = error_tx.send(watch_err);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[derive(Clone, Default, Debug)]
    struct TestConfig {
        value: String,
    }

    #[test]
    fn test_watcher_config_default() {
        let config = WatcherConfig::default();
        assert_eq!(config.debounce, Duration::from_millis(100));
        assert!(config.paths.is_empty());
    }

    #[test]
    fn test_watched_paths_set() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.toml");
        fs::write(&file_path, "value = \"test\"").unwrap();

        let paths: HashSet<PathBuf> = [file_path.clone()]
            .iter()
            .filter_map(|p| p.canonicalize().ok())
            .collect();

        assert!(!paths.is_empty());
    }
}
