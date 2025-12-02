//! Hot reload example demonstrating file watching.
//!
//! This example shows how to use the hot reload feature to automatically
//! reload configuration when files change.
//!
//! # Running
//!
//! ```bash
//! # Create a config file
//! echo 'port = 8080' > /tmp/config.toml
//!
//! # Run the example
//! cargo run --example hot_reload --features watch
//!
//! # In another terminal, modify the config
//! echo 'port = 9090' > /tmp/config.toml
//! ```

use std::fs;
use std::io::Write;
use std::thread;
use std::time::Duration;

use procenv::{ConfigSources, WatchBuilder};

#[derive(Clone)]
struct Config {
    port: u16,
    host: String,
    debug: bool,
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("port", &self.port)
            .field("host", &self.host)
            .field("debug", &self.debug)
            .finish()
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a temporary config file
    let config_path = std::env::temp_dir().join("procenv_example.toml");

    // Write initial config
    {
        let mut file = fs::File::create(&config_path)?;
        writeln!(file, "port = 8080")?;
        writeln!(file, "host = \"localhost\"")?;
        writeln!(file, "debug = false")?;
    }

    println!("Config file: {}", config_path.display());
    println!("Modify this file to see hot reload in action!\n");

    // Build a file watcher
    let config_path_clone = config_path.clone();
    let handle = WatchBuilder::new()
        .watch_file(&config_path)
        .debounce(Duration::from_millis(100))
        .on_change(|change| {
            println!("\n[RELOAD] Configuration changed!");
            println!("  Trigger: {}", change.trigger);
            if !change.changed_fields.is_empty() {
                println!("  Changed fields: {:?}", change.changed_fields);
            }
            println!("  New config: {:?}", change.new);
        })
        .on_error(|err| {
            eprintln!("\n[ERROR] Config reload failed: {err}");
            eprintln!("  Previous config is still active");
        })
        .build_sync(move || {
            // Parse the config file
            let content =
                fs::read_to_string(&config_path_clone).map_err(|e| procenv::Error::Missing {
                    var: config_path_clone.display().to_string(),
                    help: format!("Failed to read config file: {e}"),
                })?;

            let value: toml::Value =
                toml::from_str(&content).map_err(|e: toml::de::Error| procenv::Error::Missing {
                    var: "config".to_string(),
                    help: format!("Invalid TOML: {e}"),
                })?;

            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let port = value
                .get("port")
                .and_then(toml::Value::as_integer)
                .unwrap_or(8080) as u16;
            let host = value
                .get("host")
                .and_then(|v| v.as_str())
                .unwrap_or("localhost")
                .to_string();
            let debug = value
                .get("debug")
                .and_then(toml::Value::as_bool)
                .unwrap_or(false);

            Ok((Config { port, host, debug }, ConfigSources::default()))
        })?;

    // Show initial config
    let config = handle.get();
    println!("Initial configuration:");
    println!("  {config:?}");
    println!("\nWatching for changes (press Ctrl+C to exit)...");
    println!("Try editing {} to see hot reload!\n", config_path.display());

    // Keep running and periodically show current config
    let mut last_epoch = handle.epoch();
    loop {
        thread::sleep(Duration::from_secs(2));

        if handle.has_changed_since(last_epoch) {
            last_epoch = handle.epoch();
            let current = handle.get();
            println!("[POLL] Current config: {current:?}");
        }

        // Check if still running
        if !handle.is_running() {
            println!("Watcher stopped");
            break;
        }
    }

    // Cleanup
    let _ = fs::remove_file(&config_path);

    Ok(())
}
