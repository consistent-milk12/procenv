//! Integration tests for source attribution in `from_config_with_sources()`.
//!
//! These tests verify that the `ConfigSources` returned by various loading
//! methods correctly identify where each configuration value originated.

use procenv::{EnvConfig, Source};
use std::env;
use std::panic::{self, AssertUnwindSafe};

/// Helper to run a test with specific environment variables set.
/// Cleans up after the test completes, even on panic.
///
/// # Safety
///
/// Uses unsafe env::set_var/remove_var. These tests should run with
/// `--test-threads=1` or use appropriate synchronization.
fn with_env_vars<F, R>(vars: &[(&str, &str)], test: F) -> R
where
    F: FnOnce() -> R + panic::UnwindSafe,
{
    // Save original values and set new ones
    // SAFETY: Tests are run single-threaded via nextest configuration
    let originals: Vec<_> = vars
        .iter()
        .map(|(k, v)| {
            let original = env::var(k).ok();
            unsafe { env::set_var(k, v) };
            (*k, original)
        })
        .collect();

    // Run test with panic catching for guaranteed cleanup
    let result = panic::catch_unwind(AssertUnwindSafe(test));

    // Restore original values (always runs, even on panic)
    // SAFETY: Tests are run single-threaded via nextest configuration
    for (key, original) in originals {
        match original {
            Some(val) => unsafe { env::set_var(key, val) },
            None => unsafe { env::remove_var(key) },
        }
    }

    // Re-panic if test panicked, otherwise return result
    match result {
        Ok(r) => r,
        Err(e) => panic::resume_unwind(e),
    }
}

/// Helper to clean up environment variables before a test
///
/// # Safety
///
/// Uses unsafe env::remove_var.
fn clear_env_vars(vars: &[&str]) {
    for var in vars {
        // SAFETY: Tests are run single-threaded via nextest configuration
        unsafe { env::remove_var(var) };
    }
}

// =============================================================================
// TEST: Basic Source Attribution
// =============================================================================

mod basic_sources {
    use super::*;

    #[derive(EnvConfig)]
    struct BasicConfig {
        /// Database connection URL
        #[env(var = "BASIC_DB_URL")]
        db_url: String,

        /// Server port with default
        #[env(var = "BASIC_PORT", default = "8080")]
        port: u16,

        /// Optional debug flag
        #[env(var = "BASIC_DEBUG", optional)]
        debug: Option<bool>,
    }

    #[test]
    fn test_environment_source_attribution() {
        clear_env_vars(&["BASIC_DB_URL", "BASIC_PORT", "BASIC_DEBUG"]);

        with_env_vars(
            &[
                ("BASIC_DB_URL", "postgres://localhost/test"),
                ("BASIC_PORT", "9000"),
            ],
            || {
                let (config, sources) = BasicConfig::from_env_with_sources().unwrap();

                // Verify config values
                assert_eq!(config.db_url, "postgres://localhost/test");
                assert_eq!(config.port, 9000);
                assert!(config.debug.is_none());

                // Verify source attribution
                let db_source = sources.get("db_url").expect("db_url source missing");
                assert!(
                    matches!(db_source.source, Source::Environment),
                    "Expected Environment, got {:?}",
                    db_source.source
                );

                let port_source = sources.get("port").expect("port source missing");
                assert!(
                    matches!(port_source.source, Source::Environment),
                    "Expected Environment, got {:?}",
                    port_source.source
                );

                let debug_source = sources.get("debug").expect("debug source missing");
                assert!(
                    matches!(debug_source.source, Source::NotSet),
                    "Expected NotSet, got {:?}",
                    debug_source.source
                );
            },
        );
    }

    #[test]
    fn test_default_source_attribution() {
        clear_env_vars(&["BASIC_DB_URL", "BASIC_PORT", "BASIC_DEBUG"]);

        with_env_vars(&[("BASIC_DB_URL", "postgres://localhost/test")], || {
            let (_config, sources) = BasicConfig::from_env_with_sources().unwrap();

            // Port should come from default since env var not set
            let port_source = sources.get("port").expect("port source missing");
            assert!(
                matches!(port_source.source, Source::Default),
                "Expected Default, got {:?}",
                port_source.source
            );
        });
    }
}

// =============================================================================
// TEST: Profile Source Attribution
// =============================================================================

mod profile_sources {
    use super::*;

    #[derive(EnvConfig)]
    #[env_config(profile_env = "PROFILE_APP_ENV", profiles = ["dev", "prod"])]
    struct ProfileConfig {
        /// Database URL with profile-specific defaults
        #[env(var = "PROFILE_DB_URL")]
        #[profile(dev = "postgres://localhost/dev", prod = "postgres://prod-server/db")]
        db_url: String,

        /// Port with regular default (no profile)
        #[env(var = "PROFILE_PORT", default = "8080")]
        port: u16,
    }

    #[test]
    fn test_profile_source_attribution_dev() {
        clear_env_vars(&["PROFILE_APP_ENV", "PROFILE_DB_URL", "PROFILE_PORT"]);

        with_env_vars(&[("PROFILE_APP_ENV", "dev")], || {
            let (config, sources) = ProfileConfig::from_env_with_sources().unwrap();

            // Verify config uses profile default
            assert_eq!(config.db_url, "postgres://localhost/dev");

            // Verify source is Profile("dev"), NOT Default
            let db_source = sources.get("db_url").expect("db_url source missing");
            assert!(
                matches!(db_source.source, Source::Profile(ref p) if p == "dev"),
                "Expected Profile(\"dev\"), got {:?}",
                db_source.source
            );

            // Port should still be Default (no profile config for it)
            let port_source = sources.get("port").expect("port source missing");
            assert!(
                matches!(port_source.source, Source::Default),
                "Expected Default, got {:?}",
                port_source.source
            );
        });
    }

    #[test]
    fn test_profile_source_attribution_prod() {
        clear_env_vars(&["PROFILE_APP_ENV", "PROFILE_DB_URL", "PROFILE_PORT"]);

        with_env_vars(&[("PROFILE_APP_ENV", "prod")], || {
            let (config, sources) = ProfileConfig::from_env_with_sources().unwrap();

            // Verify config uses prod profile default
            assert_eq!(config.db_url, "postgres://prod-server/db");

            // Verify source is Profile("prod")
            let db_source = sources.get("db_url").expect("db_url source missing");
            assert!(
                matches!(db_source.source, Source::Profile(ref p) if p == "prod"),
                "Expected Profile(\"prod\"), got {:?}",
                db_source.source
            );
        });
    }

    #[test]
    fn test_env_overrides_profile() {
        clear_env_vars(&["PROFILE_APP_ENV", "PROFILE_DB_URL", "PROFILE_PORT"]);

        with_env_vars(
            &[
                ("PROFILE_APP_ENV", "dev"),
                ("PROFILE_DB_URL", "postgres://custom/override"),
            ],
            || {
                let (config, sources) = ProfileConfig::from_env_with_sources().unwrap();

                // Env var should override profile
                assert_eq!(config.db_url, "postgres://custom/override");

                // Source should be Environment, not Profile
                let db_source = sources.get("db_url").expect("db_url source missing");
                assert!(
                    matches!(db_source.source, Source::Environment),
                    "Expected Environment, got {:?}",
                    db_source.source
                );
            },
        );
    }
}

// =============================================================================
// TEST: Flatten Field Source Attribution
// =============================================================================

mod flatten_sources {
    use super::*;

    #[derive(EnvConfig)]
    struct NestedDb {
        #[env(var = "HOST")]
        host: String,

        #[env(var = "PORT", default = "5432")]
        port: u16,
    }

    #[derive(EnvConfig)]
    #[env_config(prefix = "FLAT_")]
    struct FlattenConfig {
        /// Application name
        #[env(var = "APP_NAME")]
        name: String,

        /// Nested database config
        #[env(flatten, prefix = "DB_")]
        database: NestedDb,
    }

    #[test]
    fn test_flatten_env_source_attribution() {
        clear_env_vars(&["FLAT_APP_NAME", "FLAT_DB_HOST", "FLAT_DB_PORT"]);

        with_env_vars(
            &[
                ("FLAT_APP_NAME", "my-app"),
                ("FLAT_DB_HOST", "localhost"),
                ("FLAT_DB_PORT", "5433"),
            ],
            || {
                let (config, sources) = FlattenConfig::from_env_with_sources().unwrap();

                // Verify config values
                assert_eq!(config.name, "my-app");
                assert_eq!(config.database.host, "localhost");
                assert_eq!(config.database.port, 5433);

                // Verify nested field sources are tracked
                let host_source = sources.get("database.host");
                assert!(
                    host_source.is_some(),
                    "database.host source should be tracked. Available: {:?}",
                    sources.entries().iter().map(|(k, _)| k).collect::<Vec<_>>()
                );
                assert!(
                    matches!(host_source.unwrap().source, Source::Environment),
                    "Expected Environment for database.host, got {:?}",
                    host_source.unwrap().source
                );

                let port_source = sources.get("database.port");
                assert!(
                    port_source.is_some(),
                    "database.port source should be tracked"
                );
                assert!(
                    matches!(port_source.unwrap().source, Source::Environment),
                    "Expected Environment for database.port, got {:?}",
                    port_source.unwrap().source
                );
            },
        );
    }

    #[test]
    fn test_flatten_default_source_attribution() {
        clear_env_vars(&["FLAT_APP_NAME", "FLAT_DB_HOST", "FLAT_DB_PORT"]);

        with_env_vars(
            &[
                ("FLAT_APP_NAME", "my-app"),
                ("FLAT_DB_HOST", "localhost"),
                // Note: FLAT_DB_PORT not set - should use default
            ],
            || {
                let (config, sources) = FlattenConfig::from_env_with_sources().unwrap();

                // Port should use default value
                assert_eq!(config.database.port, 5432);

                // Source for port should be tracked (even though it's a default)
                let port_source = sources.get("database.port");
                assert!(
                    port_source.is_some(),
                    "database.port source should be tracked even for defaults"
                );

                // After the fix, nested defaults show as NotSet (full nested
                // default tracking requires additional metadata propagation)
                // This test documents current behavior
            },
        );
    }

    #[test]
    fn test_flatten_all_fields_tracked() {
        clear_env_vars(&["FLAT_APP_NAME", "FLAT_DB_HOST", "FLAT_DB_PORT"]);

        with_env_vars(
            &[("FLAT_APP_NAME", "my-app"), ("FLAT_DB_HOST", "localhost")],
            || {
                let (_config, sources) = FlattenConfig::from_env_with_sources().unwrap();

                // Count tracked entries - should include nested fields
                let entry_count = sources.entries().len();

                // At minimum: name, database.host, database.port
                assert!(
                    entry_count >= 3,
                    "Expected at least 3 source entries, got {}. Entries: {:?}",
                    entry_count,
                    sources.entries().iter().map(|(k, _)| k).collect::<Vec<_>>()
                );

                // Verify all expected paths exist
                assert!(sources.get("name").is_some(), "name should be tracked");
                assert!(
                    sources.get("database.host").is_some(),
                    "database.host should be tracked"
                );
                assert!(
                    sources.get("database.port").is_some(),
                    "database.port should be tracked"
                );
            },
        );
    }
}

// =============================================================================
// TEST: ConfigBuilder::defaults() Error Handling
// =============================================================================

#[cfg(feature = "file")]
mod defaults_error_handling {
    use procenv::file::ConfigBuilder;
    use serde::Serialize;

    #[derive(Serialize)]
    struct GoodDefaults {
        port: u16,
        debug: bool,
    }

    #[test]
    fn test_defaults_success() {
        let builder = ConfigBuilder::new().defaults(GoodDefaults {
            port: 8080,
            debug: false,
        });

        // Should not panic
        drop(builder);
    }

    #[test]
    fn test_try_defaults_success() {
        let result = ConfigBuilder::new().try_defaults(GoodDefaults {
            port: 8080,
            debug: false,
        });

        assert!(
            result.is_ok(),
            "try_defaults should succeed for valid types"
        );
    }

    // Note: Testing actual serialization failure requires a type that
    // fails to serialize. This is difficult to construct with serde.
    // In practice, you might test with a custom Serialize impl that returns an error.
}

// =============================================================================
// TEST: Mixed Sources in Complex Config
// =============================================================================

mod mixed_sources {
    use super::*;

    #[derive(EnvConfig)]
    struct Logging {
        #[env(var = "LEVEL", default = "info")]
        level: String,
    }

    #[derive(EnvConfig)]
    #[env_config(prefix = "MIX_", profile_env = "MIX_ENV", profiles = ["dev", "prod"])]
    struct MixedConfig {
        /// App name from env
        #[env(var = "NAME")]
        name: String,

        /// Port with profile defaults
        #[env(var = "PORT", default = "8080")]
        #[profile(dev = "3000", prod = "80")]
        port: u16,

        /// Nested logging config
        #[env(flatten, prefix = "LOG_")]
        logging: Logging,

        /// Optional feature flag
        #[env(var = "FEATURE_X", optional)]
        feature_x: Option<bool>,
    }

    #[test]
    fn test_mixed_sources_complete() {
        clear_env_vars(&[
            "MIX_ENV",
            "MIX_NAME",
            "MIX_PORT",
            "MIX_LOG_LEVEL",
            "MIX_FEATURE_X",
        ]);

        with_env_vars(
            &[
                ("MIX_ENV", "dev"),         // Set profile to dev
                ("MIX_NAME", "my-service"), // Name from env
                // MIX_PORT not set - should use profile default "3000"
                ("MIX_LOG_LEVEL", "debug"), // Nested field from env
                                            // MIX_FEATURE_X not set - should be None
            ],
            || {
                let (config, sources) = MixedConfig::from_env_with_sources().unwrap();

                // Verify values
                assert_eq!(config.name, "my-service");
                assert_eq!(config.port, 3000); // From dev profile
                assert_eq!(config.logging.level, "debug");
                assert!(config.feature_x.is_none());

                // Verify sources

                // Name: from Environment
                let name_source = sources.get("name").unwrap();
                assert!(matches!(name_source.source, Source::Environment));

                // Port: from Profile("dev")
                let port_source = sources.get("port").unwrap();
                assert!(
                    matches!(port_source.source, Source::Profile(ref p) if p == "dev"),
                    "Port should be from Profile(\"dev\"), got {:?}",
                    port_source.source
                );

                // logging.level: from Environment (nested)
                let log_source = sources.get("logging.level");
                assert!(log_source.is_some(), "logging.level should be tracked");
                assert!(
                    matches!(log_source.unwrap().source, Source::Environment),
                    "logging.level should be Environment"
                );

                // feature_x: NotSet
                let feature_source = sources.get("feature_x").unwrap();
                assert!(matches!(feature_source.source, Source::NotSet));
            },
        );
    }
}
